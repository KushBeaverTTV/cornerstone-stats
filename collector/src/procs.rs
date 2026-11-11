//! Processes: CPU, memory and disk I/O per program, plus what each one has
//! open (files per volume, graphics clients, TCP sockets) for the pages that
//! ask for it.

use crate::gpu::GpuProc;
use crate::json::{bounded, Json};
use crate::sockets;
use crate::sys;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::time::Instant;

const PF_KTHREAD: u64 = 0x0020_0000;
const DRM_MAJOR: u32 = 226;

/// The fields of /proc/<pid>/stat we use.
#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub comm: String,
    pub ppid: u32,
    pub flags: u64,
    pub ticks: u64,
    pub start: u64,
    pub rss_pages: u64,
}

pub fn parse_stat(text: &str) -> Option<Stat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_string();
    let f: Vec<&str> = text.get(close + 2..)?.split_ascii_whitespace().collect();
    if f.len() < 22 {
        return None;
    }
    let n = |i: usize| f[i].parse::<u64>().unwrap_or(0);
    Some(Stat { comm, ppid: n(1) as u32, flags: n(6), ticks: n(11) + n(12), start: n(19), rss_pages: n(21) })
}

/// A readable name: the kernel's comm, which it cuts at 15 bytes. When it
/// looks cut, the first word of argv[0]'s file name, if that starts with
/// it ("chrome-headless" -> "chrome-headless-shell"). Never the arguments.
pub fn display_name(comm: &str, argv0: Option<&str>) -> String {
    if comm.len() >= 15 {
        if let Some(arg) = argv0 {
            let word = arg.split_ascii_whitespace().next().unwrap_or("");
            let base = word.rsplit('/').next().unwrap_or(word);
            if base.len() > comm.len() && base.as_bytes().starts_with(comm.as_bytes()) {
                return bounded(base, 64).to_string();
            }
        }
    }
    comm.to_string()
}

/// io read_bytes and write_bytes (storage, not cache hits).
pub fn parse_io(text: &str) -> Option<(u64, u64)> {
    let r = sys::field(text, "read_bytes")?.parse().ok()?;
    let w = sys::field(text, "write_bytes")?.parse().ok()?;
    Some((r, w))
}

// ------------------------------------------------------------ DRM fdinfo

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DrmClient {
    pub pdev: String,
    pub client: u64,
    /// engine -> busy nanoseconds, or for xe engine -> (cycles, total cycles)
    pub engines: Vec<(String, u64, Option<u64>)>,
    pub vram: u64,
    pub system: u64,
}

fn fdinfo_bytes(v: &str) -> Option<u64> {
    let mut it = v.split_ascii_whitespace();
    let n: u64 = it.next()?.parse().ok()?;
    Some(match it.next() {
        Some("KiB") => n * 1024,
        Some("MiB") => n * 1024 * 1024,
        Some("GiB") => n * 1024 * 1024 * 1024,
        _ => n,
    })
}

pub fn parse_fdinfo(text: &str) -> Option<DrmClient> {
    let mut c = DrmClient::default();
    let mut have_client = false;
    let mut total_cycles: HashMap<String, u64> = HashMap::new();
    let mut cycles: Vec<(String, u64)> = Vec::new();
    let (mut vram_mem, mut vram_res, mut vram_tot) = (None, None, None);
    let mut sys_mem = 0u64;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        if k == "drm-pdev" {
            c.pdev = v.to_string();
        } else if k == "drm-client-id" {
            c.client = v.parse().unwrap_or(0);
            have_client = true;
        } else if let Some(e) = k.strip_prefix("drm-engine-") {
            if e != "capacity" {
                if let Some(ns) = v.split_ascii_whitespace().next().and_then(|x| x.parse().ok()) {
                    c.engines.push((e.to_string(), ns, None));
                }
            }
        } else if let Some(e) = k.strip_prefix("drm-total-cycles-") {
            total_cycles.insert(e.to_string(), v.parse().unwrap_or(0));
        } else if let Some(e) = k.strip_prefix("drm-cycles-") {
            cycles.push((e.to_string(), v.parse().unwrap_or(0)));
        } else if let Some(region) = k.strip_prefix("drm-memory-").or_else(|| k.strip_prefix("drm-resident-")).or_else(|| k.strip_prefix("drm-total-")) {
            let bytes = fdinfo_bytes(v).unwrap_or(0);
            let is_vram = region == "vram" || region.starts_with("local") || region.starts_with("vram");
            if is_vram {
                if k.starts_with("drm-memory-") {
                    vram_mem = Some(bytes);
                } else if k.starts_with("drm-resident-") {
                    vram_res = Some(bytes);
                } else {
                    vram_tot = Some(bytes);
                }
            } else if (k.starts_with("drm-memory-") || k.starts_with("drm-resident-")) && region != "cpu" {
                sys_mem += bytes;
            }
        }
    }
    for (e, cyc) in cycles {
        c.engines.push((e.clone(), cyc, total_cycles.get(&e).copied()));
    }
    c.vram = vram_res.or(vram_mem).or(vram_tot).unwrap_or(0);
    c.system = sys_mem;
    (have_client && !c.pdev.is_empty()).then_some(c)
}

// ------------------------------------------------------------- statx on fds

/// statx of an open file through /proc/<pid>/fd/<n>: (mode, mount id, inode, rdev major).
fn statx_fd(path_nul: &[u8]) -> Option<(u32, u64, u64, u32)> {
    const STATX_TYPE: u32 = 0x1;
    const STATX_INO: u32 = 0x100;
    const STATX_MNT_ID: u32 = 0x1000;
    let mut buf = [0u8; 256];
    let r = unsafe {
        libc::syscall(
            libc::SYS_statx,
            libc::AT_FDCWD,
            path_nul.as_ptr() as *const libc::c_char,
            // Cached attributes: never ask a network or FUSE server, which
            // could hang the whole frame on a dead share.
            0x4000, // AT_STATX_DONT_SYNC
            STATX_TYPE | STATX_INO | STATX_MNT_ID,
            buf.as_mut_ptr(),
        )
    };
    if r != 0 {
        return None;
    }
    let mask = u32::from_ne_bytes(buf[0..4].try_into().ok()?);
    let mode = u16::from_ne_bytes(buf[28..30].try_into().ok()?) as u32;
    let ino = u64::from_ne_bytes(buf[32..40].try_into().ok()?);
    let rdev_major = u32::from_ne_bytes(buf[128..132].try_into().ok()?);
    let mnt = if mask & STATX_MNT_ID != 0 { u64::from_ne_bytes(buf[144..152].try_into().ok()?) } else { 0 };
    Some((mode, mnt, ino, rdev_major))
}

// ------------------------------------------------------------------ scanner

#[derive(Clone)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub start: u64,
    pub name: String,
    pub ticks: u64,
    pub rss: u64,
    pub io: Option<(u64, u64)>,
    pub cpu: f64,
    pub read: f64,
    pub write: f64,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub pid: u32,
    pub name: String,
    pub count: u32,
    pub cpu: f64,
    pub mem: u64,
    pub read: f64,
    pub write: f64,
    start: u64,
}

pub fn write_row(j: &mut Json, r: &Row) {
    j.begin_obj();
    j.uint("pid", r.pid as u64);
    j.str("name", &r.name);
    j.uint("count", r.count as u64);
    j.num("cpu", r.cpu, 1);
    j.uint("mem", r.mem);
    j.num("read", r.read, 0);
    j.num("write", r.write, 0);
    j.end_obj();
}

pub struct Procs {
    pub list: Vec<Proc>,
    prev: HashMap<u32, Proc>,
    prev_at: Option<Instant>,
    pub scanned_at: Option<Instant>,
    clk: f64,
    page: u64,
    threads: f64,
    me: u32,
    buf: String,
    // Extras for the panel's pages.
    drm_prev: HashMap<(String, u64), (Vec<(String, u64, Option<u64>)>, Instant)>,
    sock_prev: Option<HashMap<u64, (u64, u64)>>,
    sock_at: Option<Instant>,
}

pub struct NetRow {
    pub pid: u32,
    pub name: String,
    pub count: u32,
    pub rx: f64,
    pub tx: f64,
    pub connections: u32,
}

impl Procs {
    pub fn new(threads: usize) -> Self {
        Procs {
            list: Vec::new(),
            prev: HashMap::new(),
            prev_at: None,
            scanned_at: None,
            clk: sys::clock_ticks(),
            page: sys::page_size(),
            threads: threads.max(1) as f64,
            me: sys::my_uid(),
            buf: String::with_capacity(1024),
            drm_prev: HashMap::new(),
            sock_prev: None,
            sock_at: None,
        }
    }

    fn read_into(&mut self, path: &str) -> bool {
        self.buf.clear();
        match std::fs::File::open(path) {
            Ok(mut f) => f.read_to_string(&mut self.buf).is_ok(),
            Err(_) => false,
        }
    }

    pub fn scan(&mut self) {
        let now = Instant::now();
        let seconds = self.prev_at.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
        let mut list = Vec::with_capacity(self.prev.len().max(256));
        let Ok(dir) = std::fs::read_dir("/proc") else { return };
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            if !self.read_into(&format!("/proc/{pid}/stat")) {
                continue;
            }
            let Some(st) = parse_stat(&self.buf) else { continue };
            if st.flags & PF_KTHREAD != 0 {
                continue;
            }
            let known = self.prev.get(&pid).filter(|p| p.start == st.start);
            let (uid, pname) = match known {
                Some(p) => (p.uid, p.name.clone()),
                None => {
                    let uid = std::fs::metadata(format!("/proc/{pid}")).map(|m| m.uid()).unwrap_or(0);
                    let argv0 = if st.comm.len() >= 15 && self.read_into(&format!("/proc/{pid}/cmdline")) {
                        Some(self.buf.split('\0').next().unwrap_or("").to_string())
                    } else {
                        None
                    };
                    (uid, display_name(&st.comm, argv0.as_deref()))
                }
            };
            let io = if uid == self.me && self.read_into(&format!("/proc/{pid}/io")) { parse_io(&self.buf) } else { None };
            let mut p = Proc {
                pid,
                ppid: st.ppid,
                uid,
                start: st.start,
                name: pname,
                ticks: st.ticks,
                rss: st.rss_pages * self.page,
                io,
                cpu: 0.0,
                read: 0.0,
                write: 0.0,
            };
            if let (Some(old), true) = (self.prev.get(&pid).filter(|o| o.start == st.start), seconds > 0.0) {
                p.cpu = (st.ticks.saturating_sub(old.ticks) as f64 / self.clk / seconds / self.threads * 100.0).clamp(0.0, 100.0);
                if let (Some((r, w)), Some((or, ow))) = (p.io, old.io) {
                    p.read = r.saturating_sub(or) as f64 / seconds;
                    p.write = w.saturating_sub(ow) as f64 / seconds;
                }
            }
            list.push(p);
        }
        self.prev = list.iter().map(|p| (p.pid, p.clone())).collect();
        self.prev_at = Some(now);
        self.scanned_at = Some(now);
        self.list = list;
    }

    /// Processes grouped per user and name, represented by the oldest member.
    pub fn groups<'a>(&self, members: impl Iterator<Item = &'a Proc>) -> Vec<Row> {
        let mut map: HashMap<(u32, &str), Row> = HashMap::new();
        for p in members {
            let row = map.entry((p.uid, p.name.as_str())).or_insert_with(|| Row {
                pid: p.pid,
                name: p.name.clone(),
                count: 0,
                cpu: 0.0,
                mem: 0,
                read: 0.0,
                write: 0.0,
                start: p.start,
            });
            row.count += 1;
            row.cpu += p.cpu;
            row.mem += p.rss;
            row.read += p.read;
            row.write += p.write;
            if p.start < row.start {
                row.start = p.start;
                row.pid = p.pid;
            }
        }
        map.into_values().collect()
    }

    pub fn write(&self, j: &mut Json, all: bool, by_drive: Option<&HashMap<String, Vec<Row>>>) {
        let mut rows = self.groups(self.list.iter());
        j.obj("procs");
        j.uint("count", self.list.len() as u64);
        j.obj("top");
        let by = |rows: &mut Vec<Row>, key: fn(&Row) -> f64| {
            rows.sort_by(|a, b| key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.name.cmp(&b.name)));
        };
        by(&mut rows, |r| r.cpu);
        j.arr("cpu");
        for r in rows.iter().take(6) {
            write_row(j, r);
        }
        j.end_arr();
        let cpu_sorted = if all { Some(rows.clone()) } else { None };
        by(&mut rows, |r| r.mem as f64);
        j.arr("mem");
        for r in rows.iter().take(6) {
            write_row(j, r);
        }
        j.end_arr();
        by(&mut rows, |r| r.read + r.write);
        j.arr("io");
        for r in rows.iter().take(6).filter(|r| r.read + r.write > 0.0) {
            write_row(j, r);
        }
        j.end_arr();
        j.end_obj();
        match cpu_sorted {
            Some(list) => {
                j.arr("all");
                for r in list.iter().take(1000) {
                    write_row(j, r);
                }
                j.end_arr();
            }
            None => j.null("all"),
        }
        if let Some(map) = by_drive {
            j.obj("byDrive");
            for (key, rows) in map {
                j.arr(key);
                for r in rows {
                    write_row(j, r);
                }
                j.end_arr();
            }
            j.end_obj();
        }
        j.end_obj();
    }

    // ------------------------------------------------------------- fd walk

    /// Walk the open files of every readable process once and collect what
    /// the asked-for pages need.
    fn walk_fds(&self, mounts: bool, drm: bool, sockets: bool) -> FdFacts {
        let mut facts = FdFacts::default();
        let mut path: Vec<u8> = Vec::with_capacity(64);
        for p in &self.list {
            let dir = format!("/proc/{}/fd", p.pid);
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut mnts = HashSet::new();
            for e in rd.flatten() {
                let fd = e.file_name();
                let Some(fd) = fd.to_str() else { continue };
                path.clear();
                path.extend_from_slice(dir.as_bytes());
                path.push(b'/');
                path.extend_from_slice(fd.as_bytes());
                path.push(0);
                let Some((mode, mnt, ino, rdev_major)) = statx_fd(&path) else { continue };
                let kind = mode & libc::S_IFMT as u32;
                if kind == libc::S_IFSOCK as u32 {
                    if sockets {
                        facts.sockets.insert(ino, p.pid);
                    }
                } else if kind == libc::S_IFCHR as u32 {
                    if drm && rdev_major == DRM_MAJOR {
                        if let Some(text) = sys::read_text(format!("/proc/{}/fdinfo/{fd}", p.pid)) {
                            if let Some(c) = parse_fdinfo(&text) {
                                facts.drm.push((p.pid, c));
                            }
                        }
                    }
                } else if mounts && mnt != 0 {
                    mnts.insert(mnt);
                }
            }
            if mounts && !mnts.is_empty() {
                facts.mounts.insert(p.pid, mnts);
            }
        }
        facts
    }

    /// Processes with files open on each volume, busiest first.
    pub fn by_drive(&self, mount_keys: &HashMap<u64, String>) -> HashMap<String, Vec<Row>> {
        let facts = self.walk_fds(true, false, false);
        let mut per_key: HashMap<String, Vec<&Proc>> = HashMap::new();
        for p in &self.list {
            let Some(mnts) = facts.mounts.get(&p.pid) else { continue };
            let keys: HashSet<&String> = mnts.iter().filter_map(|m| mount_keys.get(m)).collect();
            for k in keys {
                per_key.entry(k.clone()).or_default().push(p);
            }
        }
        per_key
            .into_iter()
            .map(|(k, procs)| {
                let mut rows = self.groups(procs.into_iter());
                rows.sort_by(|a, b| (b.read + b.write).partial_cmp(&(a.read + a.write)).unwrap_or(std::cmp::Ordering::Equal).then(b.mem.cmp(&a.mem)));
                rows.truncate(12);
                (k, rows)
            })
            .collect()
    }

    /// What each process uses of every graphics card, keyed by PCI address.
    pub fn gpu_usage(&mut self, integrated: &HashMap<String, bool>) -> HashMap<String, Vec<GpuProc>> {
        let facts = self.walk_fds(false, true, false);
        let now = Instant::now();
        let mut seen: HashSet<(String, u64)> = HashSet::new();
        // card -> name -> (pid of first, count of pids, load, mem)
        let mut cards: HashMap<String, HashMap<String, (u32, HashSet<u32>, f64, u64)>> = HashMap::new();
        let mut next_prev = HashMap::new();
        for (pid, c) in facts.drm {
            let key = (c.pdev.clone(), c.client);
            if !seen.insert(key.clone()) {
                continue;
            }
            let mut load: f64 = 0.0;
            if let Some((old, at)) = self.drm_prev.get(&key) {
                let ns = now.duration_since(*at).as_nanos() as f64;
                for (e, v, total) in &c.engines {
                    if let Some((_, ov, ototal)) = old.iter().find(|(oe, _, _)| oe == e) {
                        let busy = match (total, ototal) {
                            (Some(t), Some(ot)) if t > ot => v.saturating_sub(*ov) as f64 / (t - ot) as f64 * 100.0,
                            (None, None) if ns > 0.0 => v.saturating_sub(*ov) as f64 / ns * 100.0,
                            _ => 0.0,
                        };
                        load = load.max(busy);
                    }
                }
            }
            next_prev.insert(key, (c.engines.clone(), now));
            let integrated = integrated.get(&c.pdev).copied().unwrap_or(false);
            let mem = if integrated { c.system + c.vram } else { c.vram };
            let name = self.list.iter().find(|p| p.pid == pid).map(|p| p.name.clone()).unwrap_or_else(|| pid.to_string());
            let entry = cards.entry(c.pdev.clone()).or_default().entry(name).or_insert((pid, HashSet::new(), 0.0, 0));
            entry.1.insert(pid);
            entry.2 += load.min(100.0);
            entry.3 += mem;
        }
        self.drm_prev = next_prev;
        cards
            .into_iter()
            .map(|(card, names)| {
                let mut rows: Vec<GpuProc> = names
                    .into_iter()
                    .map(|(name, (pid, pids, load, vram))| GpuProc { pid, name, count: pids.len() as u32, load: load.min(100.0), vram })
                    .collect();
                rows.sort_by(|a, b| b.load.partial_cmp(&a.load).unwrap_or(std::cmp::Ordering::Equal).then(b.vram.cmp(&a.vram)));
                rows.truncate(12);
                (card, rows)
            })
            .collect()
    }

    /// TCP traffic per program since the last call; None on the first.
    pub fn net_usage(&mut self) -> Option<Vec<NetRow>> {
        let now = Instant::now();
        let socks = sockets::tcp_sockets()?;
        let owners = self.walk_fds(false, false, true).sockets;
        let seconds = self.sock_at.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
        let prev = self.sock_prev.take();
        self.sock_prev = Some(socks.iter().map(|s| (s.inode, (s.sent, s.received))).collect());
        self.sock_at = Some(now);
        let prev = prev?;
        if seconds <= 0.0 {
            return None;
        }
        let mut per_pid: HashMap<u32, (f64, f64, u32)> = HashMap::new();
        for s in &socks {
            let Some(pid) = owners.get(&s.inode) else { continue };
            let (os, or) = prev.get(&s.inode).copied().unwrap_or((0, 0));
            let e = per_pid.entry(*pid).or_default();
            e.0 += s.received.saturating_sub(or) as f64 / seconds;
            e.1 += s.sent.saturating_sub(os) as f64 / seconds;
            if s.established {
                e.2 += 1;
            }
        }
        let mut groups: HashMap<(u32, String), NetRow> = HashMap::new();
        for (pid, (rx, tx, conns)) in per_pid {
            let Some(p) = self.list.iter().find(|p| p.pid == pid) else { continue };
            let row = groups.entry((p.uid, p.name.clone())).or_insert(NetRow { pid, name: p.name.clone(), count: 0, rx: 0.0, tx: 0.0, connections: 0 });
            row.count += 1;
            row.rx += rx;
            row.tx += tx;
            row.connections += conns;
            if pid < row.pid {
                row.pid = pid;
            }
        }
        let mut rows: Vec<NetRow> = groups.into_values().filter(|r| r.connections > 0 || r.rx + r.tx > 0.0).collect();
        rows.sort_by(|a, b| (b.rx + b.tx).partial_cmp(&(a.rx + a.tx)).unwrap_or(std::cmp::Ordering::Equal).then(b.connections.cmp(&a.connections)));
        rows.truncate(40);
        Some(rows)
    }

    /// Children of each pid, for process trees.
    pub fn children(&self) -> HashMap<u32, Vec<u32>> {
        let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in &self.list {
            out.entry(p.ppid).or_default().push(p.pid);
        }
        out
    }
}

#[derive(Default)]
struct FdFacts {
    mounts: HashMap<u32, HashSet<u64>>,
    drm: Vec<(u32, DrmClient)>,
    sockets: HashMap<u64, u32>,
}

pub fn write_net_rows(j: &mut Json, rows: &Option<Vec<NetRow>>) {
    match rows {
        None => j.null("procs"),
        Some(rows) => {
            j.arr("procs");
            for r in rows {
                j.begin_obj();
                j.uint("pid", r.pid as u64);
                j.str("name", &r.name);
                j.uint("count", r.count as u64);
                j.num("rx", r.rx, 0);
                j.num("tx", r.tx, 0);
                j.uint("connections", r.connections as u64);
                j.end_obj();
            }
            j.end_arr();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_with_spaces_and_parens_in_comm() {
        let s = "1234 (Web (Content) x) S 1 1234 1234 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0 98765 1000000 2048 18446744073709551615";
        let st = parse_stat(s).unwrap();
        assert_eq!(st.comm, "Web (Content) x");
        assert_eq!(st.ppid, 1);
        assert_eq!(st.ticks, 300);
        assert_eq!(st.start, 98765);
        assert_eq!(st.rss_pages, 2048);
        assert_eq!(st.flags & PF_KTHREAD, 0);
    }

    #[test]
    fn names_never_carry_arguments() {
        assert_eq!(display_name("chrome-headless", Some("chrome-headless-shell --type=renderer --headless=old")), "chrome-headless-shell");
        assert_eq!(display_name("chrome-headless", Some("/opt/x/chrome-headless-shell")), "chrome-headless-shell");
        assert_eq!(display_name("Isolated Web Co", Some("/usr/lib/firefox/firefox -contentproc")), "Isolated Web Co");
        assert_eq!(display_name("bash", Some("/usr/bin/bash --login")), "bash");
        assert_eq!(display_name("python3", None), "python3");
    }

    #[test]
    fn io_fields() {
        let t = "rchar: 10\nwchar: 20\nsyscr: 1\nsyscw: 1\nread_bytes: 4096\nwrite_bytes: 8192\ncancelled_write_bytes: 0\n";
        assert_eq!(parse_io(t), Some((4096, 8192)));
    }

    #[test]
    fn amdgpu_and_xe_fdinfo() {
        let amd = "pos:\t0\nflags:\t02100002\ndrm-driver:\tamdgpu\ndrm-client-id:\t42\ndrm-pdev:\t0000:0c:00.0\ndrm-memory-vram:\t102400 KiB\ndrm-memory-gtt:\t2048 KiB\ndrm-memory-cpu:\t0 KiB\ndrm-engine-gfx:\t5000000 ns\ndrm-engine-compute:\t0 ns\n";
        let c = parse_fdinfo(amd).unwrap();
        assert_eq!(c.client, 42);
        assert_eq!(c.pdev, "0000:0c:00.0");
        assert_eq!(c.vram, 102400 * 1024);
        assert_eq!(c.system, 2048 * 1024);
        assert_eq!(c.engines[0], ("gfx".to_string(), 5000000, None));
        let xe = "drm-driver:\txe\ndrm-client-id:\t7\ndrm-pdev:\t0000:00:02.0\ndrm-cycles-rcs:\t500\ndrm-total-cycles-rcs:\t1000\ndrm-resident-system:\t4 MiB\n";
        let c = parse_fdinfo(xe).unwrap();
        assert_eq!(c.engines, vec![("rcs".to_string(), 500, Some(1000))]);
        assert_eq!(c.system, 4 * 1024 * 1024);
        assert!(parse_fdinfo("pos: 0\nflags: 1\n").is_none());
    }

    #[test]
    fn groups_take_the_oldest_pid() {
        let mut p = Procs::new(4);
        let mk = |pid, start, cpu| Proc { pid, ppid: 1, uid: 1000, start, name: "chrome".into(), ticks: 0, rss: 100, io: None, cpu, read: 0.0, write: 0.0 };
        p.list = vec![mk(30, 300, 1.0), mk(10, 100, 2.0), mk(20, 200, 3.0)];
        let g = p.groups(p.list.iter());
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].pid, g[0].count, g[0].cpu, g[0].mem), (10, 3, 6.0, 300));
    }

    #[test]
    fn scans_this_machine() {
        let mut p = Procs::new(1);
        p.scan();
        assert!(p.list.iter().any(|x| x.pid == std::process::id()));
    }
}

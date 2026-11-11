//! Drives (transfer rates, load, latency) and mounted volumes (space,
//! kind, and setup problems worth fixing).

use crate::json::{bounded, Json};
use crate::memory::parse_pressure;
use crate::sys::{self, Probe};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- diskstats

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Counters {
    pub reads: u64,
    pub read_sectors: u64,
    pub read_ms: u64,
    pub writes: u64,
    pub write_sectors: u64,
    pub write_ms: u64,
    pub io_ms: u64,
    pub weighted_ms: u64,
}

/// One /proc/diskstats line: (name, counters).
pub fn parse_diskstats_line(line: &str) -> Option<(&str, Counters)> {
    let f: Vec<&str> = line.split_ascii_whitespace().collect();
    if f.len() < 14 {
        return None;
    }
    let n = |i: usize| f[i].parse::<u64>().unwrap_or(0);
    Some((
        f[2],
        Counters {
            reads: n(3),
            read_sectors: n(5),
            read_ms: n(6),
            writes: n(7),
            write_sectors: n(9),
            write_ms: n(10),
            io_ms: n(12),
            weighted_ms: n(13),
        },
    ))
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Load {
    pub read: f64,
    pub write: f64,
    pub busy: f64,
    pub latency: Option<f64>,
    pub queue: f64,
}

/// Rates and load between two readings `seconds` apart. Sectors are always
/// 512 bytes in diskstats.
pub fn load_between(a: &Counters, b: &Counters, seconds: f64) -> Load {
    if seconds <= 0.0 {
        return Load::default();
    }
    let d = |x: u64, y: u64| y.saturating_sub(x) as f64;
    let ms = seconds * 1000.0;
    let requests = d(a.reads, b.reads) + d(a.writes, b.writes);
    let waited = d(a.read_ms, b.read_ms) + d(a.write_ms, b.write_ms);
    Load {
        read: d(a.read_sectors, b.read_sectors) * 512.0 / seconds,
        write: d(a.write_sectors, b.write_sectors) * 512.0 / seconds,
        busy: (d(a.io_ms, b.io_ms) / ms * 100.0).clamp(0.0, 100.0),
        latency: (requests > 0.0).then(|| waited / requests),
        queue: d(a.weighted_ms, b.weighted_ms) / ms,
    }
}

// ------------------------------------------------------------------ by-id

/// How good a /dev/disk/by-id name is as a drive's lasting identity: model
/// and serial names first, then generated ones. Lower is better.
pub fn by_id_rank(link: &str) -> Option<usize> {
    if link.contains("-part") {
        return None;
    }
    let generated = ["nvme-eui.", "nvme-nvme.", "wwn-", "scsi-0", "scsi-1", "scsi-3", "dm-", "lvm-", "md-"];
    if link.starts_with("dm-") || link.starts_with("lvm-") || link.starts_with("md-") {
        return None;
    }
    if generated.iter().any(|g| link.starts_with(g)) {
        return Some(2);
    }
    let named = ["nvme-", "ata-", "usb-", "mmc-", "scsi-", "ieee1394-", "memstick-"];
    Some(if named.iter().any(|p| link.starts_with(p)) { 0 } else { 1 })
}

/// Best id per kernel name from (link name, target kernel name) pairs.
pub fn pick_ids(links: &[(String, String)]) -> HashMap<String, String> {
    let mut best: HashMap<String, (usize, usize, String)> = HashMap::new();
    for (link, target) in links {
        let Some(rank) = by_id_rank(link) else { continue };
        let candidate = (rank, link.len(), link.clone());
        match best.get(target) {
            Some(current) if *current <= candidate => {}
            _ => {
                best.insert(target.clone(), candidate);
            }
        }
    }
    best.into_iter().map(|(k, v)| (k, v.2)).collect()
}

fn scan_ids() -> HashMap<String, String> {
    let links: Vec<(String, String)> = sys::list_dir("/dev/disk/by-id")
        .into_iter()
        .filter_map(|l| {
            let target = sys::link_name(format!("/dev/disk/by-id/{l}"))?;
            Some((l, target))
        })
        .collect();
    pick_ids(&links)
}

// -------------------------------------------------------------- mountinfo

#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    pub id: u64,
    pub mount: String,
    pub options: Vec<String>,
    pub fstype: String,
    pub source: String,
}

/// mountinfo escapes space, tab, newline and backslash as octal.
pub fn unescape(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            let v = (b[i + 1] - b'0') as u32 * 64 + (b[i + 2] - b'0') as u32 * 8 + (b[i + 3] - b'0') as u32;
            out.push(v as u8);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((left, right)) = line.split_once(" - ") else { continue };
        let l: Vec<&str> = left.split(' ').collect();
        let r: Vec<&str> = right.split(' ').collect();
        if l.len() < 6 || r.len() < 2 {
            continue;
        }
        let mut options: Vec<String> = l[5].split(',').map(String::from).collect();
        if let Some(sup) = r.get(2) {
            options.extend(sup.split(',').map(String::from));
        }
        out.push(Mount {
            id: l[0].parse().unwrap_or(0),
            mount: unescape(l[4]),
            options,
            fstype: r[0].to_string(),
            source: unescape(r[1]),
        });
    }
    out
}

pub fn is_network_fs(fstype: &str) -> bool {
    matches!(
        fstype,
        "nfs" | "nfs4" | "cifs" | "smb3" | "smbfs" | "sshfs" | "fuse.sshfs" | "fuse.rclone" | "davfs" | "fuse.davfs2"
            | "9p" | "afs" | "ceph" | "glusterfs" | "fuse.glusterfs" | "fuse.s3fs" | "fuse.gcsfuse"
    )
}

const SKIP_PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/snap/", "/var/lib/docker", "/var/lib/containers", "/var/snap"];

// ----------------------------------------------------------------- drives

struct Drive {
    name: String,
    id: String,
    model: String,
    rotational: bool,
    removable: bool,
    usb: bool,
    discard: bool,
    temp: Option<Probe>,
    temp_c: Option<f64>,
    prev: Option<Counters>,
    load: Load,
}

fn drive_temp_probe(name: &str) -> Option<Probe> {
    let dev = format!("/sys/block/{name}/device");
    for dir in [format!("{dev}/hwmon"), dev.clone()] {
        for hw in sys::list_dir(&dir) {
            if hw.starts_with("hwmon") {
                let p = format!("{dir}/{hw}/temp1_input");
                if sys::exists(&p) {
                    return Some(Probe::new(p));
                }
            }
        }
    }
    None
}

fn drive_model(name: &str) -> String {
    let dev = format!("/sys/block/{name}/device");
    let model = sys::read_text(format!("{dev}/model")).filter(|m| !m.is_empty());
    let vendor = sys::read_text(format!("{dev}/vendor")).filter(|v| !v.is_empty() && v != "ATA");
    let text = match (vendor, model) {
        (Some(v), Some(m)) if !m.starts_with(&v) => format!("{v} {m}"),
        (_, Some(m)) => m,
        (Some(v), None) => v,
        _ => String::new(),
    };
    // SATA models are space padded, sometimes in the middle ("WDC  WDS100T2B0A").
    text.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

/// The physical drive under a block device, through partitions and
/// device-mapper or md layers, with the dm layers met on the way.
fn physical_drive(block: &str, dm: &mut Vec<String>, depth: u8) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let path = sys::canonical(format!("/sys/class/block/{block}"))?;
    if path.join("partition").exists() {
        let parent = path.parent()?.file_name()?.to_string_lossy().into_owned();
        return physical_drive(&parent, dm, depth + 1);
    }
    let slaves = sys::list_dir(path.join("slaves"));
    if !slaves.is_empty() {
        if block.starts_with("dm-") {
            dm.push(block.to_string());
        }
        return physical_drive(&slaves[0], dm, depth + 1);
    }
    Some(block.to_string())
}

/// Kernel name of the block device a mount source names.
fn block_of(source: &str) -> Option<String> {
    let meta = std::fs::metadata(source).ok()?;
    let rdev = meta.rdev();
    let (major, minor) = (libc::major(rdev), libc::minor(rdev));
    let path = sys::canonical(format!("/sys/dev/block/{major}:{minor}"))?;
    Some(path.file_name()?.to_string_lossy().into_owned())
}

fn plain_arg(v: &str) -> bool {
    !v.is_empty() && v.len() <= 128 && !v.contains("..") && v.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
}

// ---------------------------------------------------------------- volumes

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub id: &'static str,
    pub fix: Option<&'static str>,
    pub args: Vec<String>,
    pub layer: Option<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Volume {
    pub key: String,
    pub mount: String,
    pub device: String,
    pub drive: String,
    pub fstype: String,
    pub kind: &'static str,
    pub options: Vec<String>,
    pub dm: Vec<String>,
    pub size: Option<u64>,
    pub used: Option<u64>,
    pub avail: Option<u64>,
    pub checks: Vec<Check>,
}

pub struct DriveFacts {
    pub ssd: bool,
    pub discard: bool,
}

/// Problems with how a volume is set up. `crypt_blocks` is the dm-crypt
/// layer that drops discards, if any; `timer` whether fstrim.timer is on.
pub fn volume_checks(v: &Volume, drive: Option<&DriveFacts>, crypt_blocks: Option<&str>, timer: bool, uuid: Option<&str>) -> Vec<Check> {
    let mut out = Vec::new();
    let has = |o: &str| v.options.iter().any(|x| x == o || x.starts_with(&format!("{o}=")));
    if let Some(d) = drive {
        if d.ssd && d.discard {
            if let Some(layer) = crypt_blocks {
                out.push(Check {
                    id: "trim-blocked",
                    fix: plain_arg(layer).then_some("allow-discards"),
                    args: if plain_arg(layer) { vec![layer.to_string()] } else { vec![] },
                    layer: Some(layer.to_string()),
                    flags: vec![],
                });
            } else if matches!(v.fstype.as_str(), "ext4" | "btrfs" | "xfs" | "f2fs" | "vfat" | "exfat" | "jfs")
                && !has("discard")
                && !timer
            {
                out.push(Check { id: "trim-off", fix: Some("enable-fstrim"), args: vec![], layer: None, flags: vec![] });
            }
        }
    }
    if matches!(v.fstype.as_str(), "vfat" | "exfat") {
        let flags: Vec<String> = v.options.iter().filter(|o| *o == "flush" || *o == "sync").cloned().collect::<HashSet<_>>().into_iter().collect();
        if !flags.is_empty() {
            let fixable = v.mount.starts_with("/run/media/") && uuid.is_some_and(plain_arg) && plain_arg(&v.device);
            let mut flags = flags;
            flags.sort();
            out.push(Check {
                id: "fat-flush",
                fix: fixable.then_some("udisks-no-flush"),
                args: if fixable { vec![uuid.unwrap_or("").to_string(), v.device.clone()] } else { vec![] },
                layer: None,
                flags,
            });
        }
    }
    out
}

fn statvfs(path: &str) -> Option<(u64, u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let f = s.f_frsize as u64;
    let size = s.f_blocks as u64 * f;
    let avail = s.f_bavail as u64 * f;
    let used = (s.f_blocks as u64).saturating_sub(s.f_bfree as u64) * f;
    Some((size, used, avail))
}

/// statvfs in a forked child, for network mounts. A dead share leaves the
/// caller blocked in the kernel (FUSE waits in request_wait_answer, which
/// even exit cannot interrupt), so a thread would pin this process alive;
/// a child can be abandoned. None when it did not answer in time.
fn statvfs_isolated(path: &str, timeout: Duration, abandoned: &mut Vec<(i32, String)>) -> Option<Option<(u64, u64, u64)>> {
    // Reap children abandoned earlier whose mount has answered since, and
    // never stack a second one on a mount that still hangs.
    abandoned.retain(|(pid, _)| unsafe { libc::waitpid(*pid, std::ptr::null_mut(), libc::WNOHANG) } == 0);
    if abandoned.iter().any(|(_, m)| m == path) {
        return None;
    }
    let c = std::ffi::CString::new(path).ok()?;
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return None;
    }
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        return None;
    }
    if pid == 0 {
        // Child: only async-signal-safe calls from here on. It lets go of
        // stdin/stdout/stderr first, so a child stuck on a dead share never
        // holds the shell's pipe open after we exit.
        unsafe {
            libc::close(0);
            libc::close(1);
            libc::close(2);
            let mut s: libc::statvfs = std::mem::zeroed();
            let mut out = [0u64; 4];
            if libc::statvfs(c.as_ptr(), &mut s) == 0 {
                let f = s.f_frsize as u64;
                out = [1, s.f_blocks as u64 * f, (s.f_blocks as u64).saturating_sub(s.f_bfree as u64) * f, s.f_bavail as u64 * f];
            }
            libc::write(fds[1], out.as_ptr() as *const libc::c_void, 32);
            libc::_exit(0);
        }
    }
    unsafe { libc::close(fds[1]) };
    let mut pfd = libc::pollfd { fd: fds[0], events: libc::POLLIN, revents: 0 };
    let ready = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as i32) } > 0;
    let mut out = [0u64; 4];
    let got = ready && unsafe { libc::read(fds[0], out.as_mut_ptr() as *mut libc::c_void, 32) } == 32;
    unsafe { libc::close(fds[0]) };
    if got {
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
        return Some((out[0] == 1).then_some((out[1], out[2], out[3])));
    }
    unsafe { libc::kill(pid, libc::SIGKILL) };
    abandoned.push((pid, path.to_string()));
    None
}

// ------------------------------------------------------------------ disks

pub struct Disks {
    stats: Probe,
    pressure: Probe,
    mountinfo: Option<std::fs::File>,
    drives: Vec<Drive>,
    pub volumes: Vec<Volume>,
    /// Every mount id whose files belong to a volume, to that volume's key.
    pub mount_keys: HashMap<u64, String>,
    prev_at: Option<Instant>,
    refreshed: Option<Instant>,
    sized: Option<Instant>,
    stuck: HashMap<String, Instant>,
    abandoned: Vec<(i32, String)>,
    stall: (f64, f64),
    read: f64,
    write: f64,
}

impl Disks {
    pub fn new() -> Self {
        let mut d = Disks {
            stats: Probe::new("/proc/diskstats"),
            pressure: Probe::new("/proc/pressure/io"),
            mountinfo: std::fs::File::open("/proc/self/mountinfo").ok(),
            drives: Vec::new(),
            volumes: Vec::new(),
            mount_keys: HashMap::new(),
            prev_at: None,
            refreshed: None,
            sized: None,
            stuck: HashMap::new(),
            abandoned: Vec::new(),
            stall: (0.0, 0.0),
            read: 0.0,
            write: 0.0,
        };
        d.refresh();
        d
    }

    /// Whether the mount table changed since the last poll (the kernel
    /// reports a change to the first poll after it, once).
    fn mounts_changed(&self) -> bool {
        let Some(f) = &self.mountinfo else { return false };
        let mut pfd = libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLPRI, revents: 0 };
        let n = unsafe { libc::poll(&mut pfd, 1, 0) };
        n > 0 && pfd.revents & (libc::POLLPRI | libc::POLLERR) != 0
    }

    fn refresh_drives(&mut self) {
        let ids = scan_ids();
        let mut drives = Vec::new();
        for name in sys::list_dir("/sys/block") {
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") || name.starts_with("dm-") || name.starts_with("md") {
                continue;
            }
            if !sys::exists(format!("/sys/block/{name}/device")) {
                continue;
            }
            if sys::read_u64(format!("/sys/block/{name}/size")) == Some(0) {
                continue; // empty card reader slot
            }
            let previous = self.drives.iter().position(|d| d.name == name).map(|i| self.drives.swap_remove(i));
            let usb = sys::canonical(format!("/sys/block/{name}")).is_some_and(|p| p.to_string_lossy().contains("/usb"));
            let (prev, load, temp, temp_c) = match previous {
                Some(p) => (p.prev, p.load, p.temp, p.temp_c),
                None => (None, Load::default(), drive_temp_probe(&name), None),
            };
            drives.push(Drive {
                id: ids.get(&name).cloned().unwrap_or_else(|| name.clone()),
                model: drive_model(&name),
                rotational: sys::read_text(format!("/sys/block/{name}/queue/rotational")).as_deref() == Some("1"),
                removable: sys::read_text(format!("/sys/block/{name}/removable")).as_deref() == Some("1"),
                discard: sys::read_u64(format!("/sys/block/{name}/queue/discard_max_bytes")).unwrap_or(0) > 0,
                usb,
                temp,
                temp_c,
                prev,
                load,
                name,
            });
        }
        // Drive temperatures: some ask the drive itself, so every refresh only.
        for d in &mut drives {
            d.temp_c = d.temp.as_mut().and_then(|p| p.f64()).map(|m| m / 1000.0).filter(|c| (-20.0..150.0).contains(c));
        }
        self.drives = drives;
    }

    fn refresh(&mut self) {
        self.refreshed = Some(Instant::now());
        self.refresh_drives();
        let text = sys::read_text("/proc/self/mountinfo").unwrap_or_default();
        let mounts = parse_mountinfo(&text);
        let timer = sys::exists("/etc/systemd/system/timers.target.wants/fstrim.timer");
        let uuids: HashMap<String, String> = sys::list_dir("/dev/disk/by-uuid")
            .into_iter()
            .filter_map(|u| Some((sys::link_name(format!("/dev/disk/by-uuid/{u}"))?, u)))
            .collect();

        let mut volumes: Vec<Volume> = Vec::new();
        let mut source_keys: HashMap<String, String> = HashMap::new();
        for m in &mounts {
            if SKIP_PREFIXES.iter().any(|p| m.mount.starts_with(p)) || m.mount.starts_with("/run/user/") && !is_network_fs(&m.fstype) {
                continue;
            }
            let network = is_network_fs(&m.fstype);
            let zfs = m.fstype == "zfs";
            let block = !network && !zfs && m.source.starts_with("/dev/") && !m.source.starts_with("/dev/loop");
            if !(network || zfs || block) {
                continue;
            }
            let (drive, dm, key, kind) = if block {
                let Some(dev) = block_of(&m.source) else { continue };
                if dev.starts_with("loop") {
                    continue;
                }
                let mut dm = Vec::new();
                let drive = physical_drive(&dev, &mut dm, 0).unwrap_or(dev.clone());
                let facts = self.drives.iter().find(|d| d.name == drive);
                let external = facts.is_some_and(|d| d.removable || d.usb);
                (drive.clone(), dm, drive, if external { "external" } else { "internal" })
            } else if zfs {
                let pool = m.source.split('/').next().unwrap_or(&m.source).to_string();
                (String::new(), vec![], pool, "internal")
            } else {
                (String::new(), vec![], m.source.clone(), "network")
            };
            let dedupe = if zfs { key.clone() } else { m.source.clone() };
            source_keys.insert(m.source.clone(), key.clone());
            if let Some(existing) = volumes.iter_mut().find(|v| (if v.fstype == "zfs" { &v.key } else { &v.device }) == &dedupe) {
                if m.mount.len() < existing.mount.len() {
                    existing.mount = m.mount.clone();
                    existing.options = m.options.clone();
                }
                continue;
            }
            volumes.push(Volume {
                key,
                mount: m.mount.clone(),
                device: m.source.clone(),
                drive,
                fstype: m.fstype.clone(),
                kind,
                options: m.options.clone(),
                dm,
                size: None,
                used: None,
                avail: None,
                checks: Vec::new(),
            });
        }
        for v in &mut volumes {
            let facts = self.drives.iter().find(|d| d.name == v.drive).map(|d| DriveFacts { ssd: !d.rotational, discard: d.discard });
            // The dm-crypt layer that drops discards the drive would take.
            let blocker = v.dm.iter().find_map(|dm| {
                let crypt = sys::read_text(format!("/sys/block/{dm}/dm/uuid")).is_some_and(|u| u.starts_with("CRYPT-"));
                let drops = sys::read_u64(format!("/sys/block/{dm}/queue/discard_max_bytes")).unwrap_or(0) == 0;
                if crypt && drops { sys::read_text(format!("/sys/block/{dm}/dm/name")) } else { None }
            });
            let block = block_of(&v.device).unwrap_or_default();
            let uuid = uuids.get(&block).map(|s| s.as_str());
            v.checks = volume_checks(v, facts.as_ref(), blocker.as_deref(), timer, uuid);
        }
        volumes.sort_by(|a, b| {
            let rank = |k: &str| match k { "internal" => 0, "external" => 1, _ => 2 };
            rank(a.kind).cmp(&rank(b.kind)).then(a.mount.len().cmp(&b.mount.len())).then(a.mount.cmp(&b.mount))
        });
        self.mount_keys = mounts
            .iter()
            .filter_map(|m| Some((m.id, source_keys.get(&m.source)?.clone())))
            .collect();
        // Sizes carried over until measured again.
        for v in &mut volumes {
            if let Some(old) = self.volumes.iter().find(|o| o.mount == v.mount) {
                v.size = old.size;
                v.used = old.used;
                v.avail = old.avail;
            }
        }
        self.volumes = volumes;
        self.measure(true);
    }

    /// statvfs every volume. A network share can hang it for minutes, so
    /// those are asked on a thread with a deadline, and one that hung is
    /// left alone for 30 s.
    fn measure(&mut self, include_network: bool) {
        self.sized = Some(Instant::now());
        for v in &mut self.volumes {
            if v.kind == "network" {
                if !include_network {
                    continue;
                }
                if self.stuck.get(&v.mount).is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
                    v.size = None;
                    v.used = None;
                    v.avail = None;
                    continue;
                }
                match statvfs_isolated(&v.mount, Duration::from_millis(1500), &mut self.abandoned) {
                    Some(r) => {
                        self.stuck.remove(&v.mount);
                        (v.size, v.used, v.avail) = match r {
                            Some((s, u, a)) => (Some(s), Some(u), Some(a)),
                            None => (None, None, None),
                        };
                    }
                    None => {
                        self.stuck.insert(v.mount.clone(), Instant::now());
                        (v.size, v.used, v.avail) = (None, None, None);
                    }
                }
            } else if let Some((s, u, a)) = statvfs(&v.mount) {
                (v.size, v.used, v.avail) = (Some(s), Some(u), Some(a));
            }
        }
    }

    pub fn sample(&mut self) {
        let now = Instant::now();
        if self.mounts_changed() || self.refreshed.map_or(true, |t| t.elapsed() >= Duration::from_secs(10)) {
            self.refresh();
        } else if self.sized.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
            self.measure(false);
        }
        let seconds = self.prev_at.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
        self.prev_at = Some(now);
        if let Some(text) = self.stats.read() {
            let mut read = 0.0;
            let mut write = 0.0;
            for line in text.lines() {
                let Some((name, c)) = parse_diskstats_line(line) else { continue };
                let Some(d) = self.drives.iter_mut().find(|d| d.name == name) else { continue };
                if let Some(prev) = d.prev {
                    d.load = load_between(&prev, &c, seconds);
                }
                d.prev = Some(c);
                read += d.load.read;
                write += d.load.write;
            }
            self.read = read;
            self.write = write;
        }
        if let Some(text) = self.pressure.read() {
            self.stall = parse_pressure(text);
        }
    }

    pub fn write(&self, j: &mut Json) {
        j.obj("disks");
        j.num("read", self.read, 0);
        j.num("write", self.write, 0);
        j.obj("stall");
        j.num("some", self.stall.0, 2);
        j.num("full", self.stall.1, 2);
        j.end_obj();
        j.obj("drives");
        for d in &self.drives {
            j.obj(&d.name);
            j.str("id", bounded(&d.id, 120));
            j.str("model", bounded(&d.model, 60));
            j.num("read", d.load.read, 0);
            j.num("write", d.load.write, 0);
            j.num("busy", d.load.busy, 1);
            j.opt("latency", d.load.latency, 2);
            j.num("queue", d.load.queue, 2);
            j.opt("temp", d.temp_c, 1);
            j.bool("rotational", d.rotational);
            j.bool("removable", d.removable || d.usb);
            j.end_obj();
        }
        j.end_obj();
        j.arr("volumes");
        for v in &self.volumes {
            let drive = self.drives.iter().find(|d| d.name == v.drive);
            j.begin_obj();
            j.str("key", bounded(&v.key, 200));
            j.str("mount", bounded(&v.mount, 300));
            j.str("device", bounded(&v.device, 200));
            j.str("drive", &v.drive);
            j.str("fstype", bounded(&v.fstype, 30));
            j.str("kind", v.kind);
            j.opt_uint("size", v.size);
            j.opt_uint("used", v.used);
            j.opt_uint("avail", v.avail);
            j.str("model", drive.map(|d| bounded(&d.model, 60)).unwrap_or(""));
            j.opt("temp", drive.and_then(|d| d.temp_c), 1);
            j.bool("rotational", drive.is_some_and(|d| d.rotational));
            j.bool("removable", drive.is_some_and(|d| d.removable || d.usb));
            j.arr("checks");
            for c in &v.checks {
                j.begin_obj();
                j.str("id", c.id);
                j.opt_str("fix", c.fix);
                j.arr("args");
                for a in &c.args {
                    j.item_str(a);
                }
                j.end_arr();
                if let Some(l) = &c.layer {
                    j.str("layer", l);
                }
                if !c.flags.is_empty() {
                    j.arr("flags");
                    for f in &c.flags {
                        j.item_str(f);
                    }
                    j.end_arr();
                }
                j.end_obj();
            }
            j.end_arr();
            j.end_obj();
        }
        j.end_arr();
        j.end_obj();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskstats_and_load() {
        let a = parse_diskstats_line(" 259       0 nvme0n1 100 0 2000 50 200 0 4000 150 0 300 400 0 0 0 0").unwrap();
        assert_eq!(a.0, "nvme0n1");
        let b = parse_diskstats_line(" 259       0 nvme0n1 110 0 2200 60 230 0 4400 190 2 800 1400 0 0 0 0").unwrap();
        let l = load_between(&a.1, &b.1, 1.0);
        assert_eq!(l.read, 200.0 * 512.0);
        assert_eq!(l.write, 400.0 * 512.0);
        assert_eq!(l.busy, 50.0);
        assert_eq!(l.latency, Some(50.0 / 40.0));
        assert_eq!(l.queue, 1.0);
        assert_eq!(load_between(&a.1, &a.1, 1.0).latency, None);
        assert!(parse_diskstats_line("1 2 sda").is_none());
    }

    #[test]
    fn by_id_prefers_model_and_serial_without_suffix() {
        let links: Vec<(String, String)> = [
            ("nvme-eui.e8238fa6bf530001001b448b49039de3", "nvme0n1"),
            ("nvme-WDC_WDS100T2B0C-00PXH0_2043F7805825_1", "nvme0n1"),
            ("nvme-WDC_WDS100T2B0C-00PXH0_2043F7805825", "nvme0n1"),
            ("nvme-WDC_WDS100T2B0C-00PXH0_2043F7805825-part1", "nvme0n1p1"),
            ("wwn-0x5001b448bba69d85", "sda"),
            ("ata-WDC_WDS100T2B0A-00SM50_2038A3800317", "sda"),
            ("dm-name-root", "dm-0"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let ids = pick_ids(&links);
        assert_eq!(ids["nvme0n1"], "nvme-WDC_WDS100T2B0C-00PXH0_2043F7805825");
        assert_eq!(ids["sda"], "ata-WDC_WDS100T2B0A-00SM50_2038A3800317");
        assert!(!ids.contains_key("nvme0n1p1"));
        assert!(!ids.contains_key("dm-0"));
    }

    #[test]
    fn mountinfo_unescapes_and_merges_option_sets() {
        let text = "33 2 0:30 /@ / rw,relatime shared:1 - btrfs /dev/mapper/root rw,ssd,discard=async\n\
                    90 33 8:17 / /run/media/k/My\\040Stick rw,nosuid - vfat /dev/sdc1 rw,flush,utf8\n";
        let m = parse_mountinfo(text);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].fstype, "btrfs");
        assert!(m[0].options.contains(&"discard=async".to_string()));
        assert_eq!(m[1].mount, "/run/media/k/My Stick");
        assert_eq!(m[1].id, 90);
        assert_eq!(unescape("a\\134b\\011c"), "a\\b\tc");
        assert_eq!(unescape("tail\\04"), "tail\\04");
    }

    fn volume(fstype: &str, mount: &str, options: &[&str]) -> Volume {
        Volume {
            key: "sdc".into(),
            mount: mount.into(),
            device: "/dev/sdc1".into(),
            drive: "sdc".into(),
            fstype: fstype.into(),
            kind: "internal",
            options: options.iter().map(|s| s.to_string()).collect(),
            dm: vec![],
            size: None,
            used: None,
            avail: None,
            checks: vec![],
        }
    }

    #[test]
    fn trim_checks() {
        let ssd = DriveFacts { ssd: true, discard: true };
        let ids = |c: Vec<Check>| c.iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids(volume_checks(&volume("ext4", "/mnt/games", &["rw"]), Some(&ssd), None, false, None)), ["trim-off"]);
        assert!(volume_checks(&volume("ext4", "/mnt/games", &["rw"]), Some(&ssd), None, true, None).is_empty());
        assert!(volume_checks(&volume("btrfs", "/", &["rw", "discard=async"]), Some(&ssd), None, false, None).is_empty());
        let hdd = DriveFacts { ssd: false, discard: false };
        assert!(volume_checks(&volume("ext4", "/data", &["rw"]), Some(&hdd), None, false, None).is_empty());
        let blocked = volume_checks(&volume("btrfs", "/", &["rw"]), Some(&ssd), Some("root"), false, None);
        assert_eq!(blocked[0].id, "trim-blocked");
        assert_eq!(blocked[0].fix, Some("allow-discards"));
        assert_eq!(blocked[0].args, ["root"]);
    }

    #[test]
    fn fat_flush_check_is_fixable_only_for_plain_udisks_mounts() {
        let v = volume("vfat", "/run/media/k/STICK", &["rw", "flush", "utf8"]);
        let c = volume_checks(&v, None, None, true, Some("1234-ABCD"));
        assert_eq!(c[0].id, "fat-flush");
        assert_eq!(c[0].fix, Some("udisks-no-flush"));
        assert_eq!(c[0].args, ["1234-ABCD", "/dev/sdc1"]);
        assert_eq!(c[0].flags, ["flush"]);
        let c = volume_checks(&v, None, None, true, Some("bad;id"));
        assert_eq!(c[0].fix, None);
        let fixed = volume_checks(&volume("vfat", "/boot", &["rw"]), None, None, true, None);
        assert!(fixed.is_empty());
    }
}

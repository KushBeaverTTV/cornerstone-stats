//! Reading the kernel's files. Files read every frame are kept open as a
//! `Probe` and reread from offset 0, which procfs and sysfs both answer with
//! fresh contents: one syscall a read instead of three.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Probe {
    path: PathBuf,
    file: Option<File>,
    buf: Vec<u8>,
    /// When opening last failed, so a missing file is not retried every frame.
    failed_at: Option<Instant>,
}

impl Probe {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Probe { path: path.into(), file: None, buf: Vec::new(), failed_at: None }
    }

    /// The whole file as text, or None when it cannot be read right now.
    pub fn read(&mut self) -> Option<&str> {
        if self.file.is_none() {
            if let Some(at) = self.failed_at {
                if at.elapsed() < Duration::from_secs(30) {
                    return None;
                }
            }
            match File::open(&self.path) {
                Ok(f) => {
                    self.file = Some(f);
                    self.failed_at = None;
                }
                Err(_) => {
                    self.failed_at = Some(Instant::now());
                    return None;
                }
            }
        }
        let file = self.file.as_ref()?;
        if self.buf.is_empty() {
            self.buf.resize(4096, 0);
        }
        let mut len = 0;
        loop {
            match file.read_at(&mut self.buf[len..], len as u64) {
                Ok(0) => break,
                Ok(n) => {
                    len += n;
                    if len == self.buf.len() {
                        let grown = self.buf.len() * 2;
                        self.buf.resize(grown, 0);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    // A device that went away: reopen next time.
                    if e.raw_os_error() == Some(libc::ENODEV) || e.raw_os_error() == Some(libc::ESTALE) {
                        self.file = None;
                    }
                    return None;
                }
            }
        }
        std::str::from_utf8(&self.buf[..len]).ok()
    }

    pub fn f64(&mut self) -> Option<f64> {
        self.read().and_then(|t| t.trim().parse::<f64>().ok())
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.read().and_then(|t| t.trim().parse::<u64>().ok())
    }
}

pub fn read_text(path: impl AsRef<Path>) -> Option<String> {
    let text = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&text).trim().to_string())
}

pub fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_text(path)?.parse().ok()
}

pub fn read_f64(path: impl AsRef<Path>) -> Option<f64> {
    read_text(path)?.parse().ok()
}

pub fn exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

/// Names of the entries of a directory, sorted.
pub fn list_dir(path: impl AsRef<Path>) -> Vec<String> {
    let mut out: Vec<String> = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    out.sort_by(|a, b| natural_cmp(a, b));
    out
}

/// "hwmon10" after "hwmon9", "cpu10" after "cpu2".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| {
        let at = s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        (s[..at].to_string(), s[at..].parse::<u64>().unwrap_or(0), s.len())
    };
    split(a).cmp(&split(b))
}

pub fn link_name(path: impl AsRef<Path>) -> Option<String> {
    let target = std::fs::read_link(path).ok()?;
    Some(target.file_name()?.to_string_lossy().into_owned())
}

pub fn canonical(path: impl AsRef<Path>) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// A program from the usual system folders, never from PATH.
pub fn system_program(name: &str) -> Option<PathBuf> {
    for dir in ["/usr/bin", "/bin", "/usr/sbin", "/usr/local/bin"] {
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Run a program with a deadline and return its stdout (and whether it
/// succeeded, with its last stderr line). No shell is involved.
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(program: &Path, args: &[&str], timeout: Duration) -> Output {
    let failed = |msg: &str| Output { ok: false, stdout: String::new(), stderr: msg.to_string() };
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C.UTF-8")
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return failed(&e.to_string()),
    };
    // Readers on threads so a chatty child never blocks on a full pipe.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(p, &mut s);
        }
        s
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(p, &mut s);
        }
        s
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    Output { ok: status.map(|s| s.success()).unwrap_or(false), stdout, stderr }
}

/// The first value after `key` in a "key value" or "key: value" text.
pub fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start_matches(':').trim();
            if !rest.is_empty() || line.len() == key.len() {
                return Some(rest);
            }
        }
    }
    None
}

pub fn clock_ticks() -> f64 {
    let t = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if t > 0 { t as f64 } else { 100.0 }
}

pub fn page_size() -> u64 {
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if p > 0 { p as u64 } else { 4096 }
}

pub fn my_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["hwmon10", "hwmon2", "hwmon1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["hwmon1", "hwmon2", "hwmon10"]);
    }

    #[test]
    fn probe_rereads_fresh_contents() {
        let dir = std::env::temp_dir().join(format!("csc-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v");
        std::fs::write(&path, "41\n").unwrap();
        let mut p = Probe::new(&path);
        assert_eq!(p.u64(), Some(41));
        std::fs::write(&path, "5\n").unwrap();
        assert_eq!(p.u64(), Some(5));
        let _ = std::fs::remove_dir_all(dir);
    }
}

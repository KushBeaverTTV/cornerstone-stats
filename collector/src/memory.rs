//! Memory, swap, compressed memory and memory pressure.

use crate::json::Json;
use crate::sys::{self, Probe};
use std::time::Instant;

/// The meminfo fields we use, in bytes.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct MemInfo {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub shmem: u64,
    pub sreclaimable: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub zswap: u64,
    pub zswapped: u64,
}

pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut m = MemInfo::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else { continue };
        let kb: u64 = rest.split_ascii_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let bytes = kb * 1024;
        match key {
            "MemTotal" => m.total = bytes,
            "MemFree" => m.free = bytes,
            "MemAvailable" => m.available = bytes,
            "Buffers" => m.buffers = bytes,
            "Cached" => m.cached = bytes,
            "Shmem" => m.shmem = bytes,
            "SReclaimable" => m.sreclaimable = bytes,
            "SwapTotal" => m.swap_total = bytes,
            "SwapFree" => m.swap_free = bytes,
            "Zswap" => m.zswap = bytes,
            "Zswapped" => m.zswapped = bytes,
            _ => {}
        }
    }
    m
}

/// Breakdown shown in the panel. apps + shared + cached + free = total.
#[derive(Debug, PartialEq)]
pub struct Breakdown {
    pub used: u64,
    pub apps: u64,
    pub cached: u64,
    pub shared: u64,
}

pub fn breakdown(m: &MemInfo) -> Breakdown {
    let used = m.total.saturating_sub(m.available);
    let cached = (m.cached + m.buffers + m.sreclaimable).saturating_sub(m.shmem);
    let shared = m.shmem;
    let apps = m.total.saturating_sub(m.free).saturating_sub(cached).saturating_sub(shared);
    Breakdown { used, apps, cached, shared }
}

/// The avg10 figures of a /proc/pressure file: (some, full).
pub fn parse_pressure(text: &str) -> (f64, f64) {
    let mut some = 0.0;
    let mut full = 0.0;
    for line in text.lines() {
        let value = line
            .split_ascii_whitespace()
            .find_map(|p| p.strip_prefix("avg10="))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        if line.starts_with("some") {
            some = value;
        } else if line.starts_with("full") {
            full = value;
        }
    }
    (some, full)
}

pub struct Memory {
    meminfo: Probe,
    pressure: Probe,
    zram: Vec<Probe>,
    zram_scan_at: Option<Instant>,
    pub info: MemInfo,
    zram_stored: u64,
    zram_original: u64,
    stall: (f64, f64),
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            meminfo: Probe::new("/proc/meminfo"),
            pressure: Probe::new("/proc/pressure/memory"),
            zram: Vec::new(),
            zram_scan_at: None,
            info: MemInfo::default(),
            zram_stored: 0,
            zram_original: 0,
            stall: (0.0, 0.0),
        }
    }

    pub fn sample(&mut self) {
        if let Some(text) = self.meminfo.read() {
            self.info = parse_meminfo(text);
        }
        if let Some(text) = self.pressure.read() {
            self.stall = parse_pressure(text);
        }
        if self.zram_scan_at.map_or(true, |t| t.elapsed().as_secs() >= 30) {
            self.zram_scan_at = Some(Instant::now());
            self.zram = sys::list_dir("/sys/block")
                .into_iter()
                .filter(|n| n.starts_with("zram"))
                .map(|n| Probe::new(format!("/sys/block/{n}/mm_stat")))
                .collect();
        }
        // mm_stat: orig_data_size compr_data_size mem_used_total ...
        let (mut stored, mut original) = (0u64, 0u64);
        for p in &mut self.zram {
            if let Some(text) = p.read() {
                let v: Vec<u64> = text.split_ascii_whitespace().take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    original += v[0];
                    stored += v[2];
                }
            }
        }
        self.zram_stored = stored + self.info.zswap;
        self.zram_original = original + self.info.zswapped;
    }

    pub fn write(&self, j: &mut Json) {
        let m = &self.info;
        let b = breakdown(m);
        j.obj("memory");
        j.uint("total", m.total);
        j.uint("used", b.used);
        j.uint("available", m.available);
        j.uint("free", m.free);
        j.uint("apps", b.apps);
        j.uint("cached", b.cached);
        j.uint("shared", b.shared);
        j.uint("swapTotal", m.swap_total);
        j.uint("swapUsed", m.swap_total.saturating_sub(m.swap_free));
        if self.zram_original > 0 {
            j.obj("zram");
            j.uint("stored", self.zram_stored);
            j.uint("original", self.zram_original);
            j.end_obj();
        } else {
            j.null("zram");
        }
        j.obj("stall");
        j.num("some", self.stall.0, 2);
        j.num("full", self.stall.1, 2);
        j.end_obj();
        j.end_obj();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "MemTotal:       32740000 kB\nMemFree:         1700000 kB\nMemAvailable:   14650000 kB\nBuffers:           10000 kB\nCached:         14900000 kB\nSwapCached:        1000 kB\nShmem:            550000 kB\nSReclaimable:     350000 kB\nSwapTotal:      65479276 kB\nSwapFree:       63868000 kB\nZswap:            449000 kB\nZswapped:        1433000 kB\n";

    #[test]
    fn meminfo_parts_add_up_to_the_total() {
        let m = parse_meminfo(SAMPLE);
        assert_eq!(m.total, 32740000 * 1024);
        assert_eq!(m.zswapped, 1433000 * 1024);
        let b = breakdown(&m);
        assert_eq!(b.apps + b.shared + b.cached + m.free, m.total);
        assert_eq!(b.used, (32740000 - 14650000) * 1024);
    }

    #[test]
    fn pressure_avg10() {
        let t = "some avg10=1.50 avg60=0.10 avg300=0.00 total=1\nfull avg10=0.25 avg60=0 avg300=0 total=1\n";
        assert_eq!(parse_pressure(t), (1.5, 0.25));
    }
}

//! Graphics cards: AMD and Intel from sysfs, NVIDIA through nvidia-smi.

use crate::json::{bounded, Json};
use crate::sys::{self, Probe};
use crate::wake::{self, Gate, Power};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Vendor {
    Amd,
    Nvidia,
    Intel,
    Other,
}

impl Vendor {
    fn from_pci(id: &str) -> Vendor {
        match id.trim().trim_start_matches("0x").to_ascii_lowercase().as_str() {
            "1002" => Vendor::Amd,
            "10de" => Vendor::Nvidia,
            "8086" => Vendor::Intel,
            _ => Vendor::Other,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Vendor::Amd => "amd",
            Vendor::Nvidia => "nvidia",
            Vendor::Intel => "intel",
            Vendor::Other => "other",
        }
    }
    fn fallback(self) -> &'static str {
        match self {
            Vendor::Amd => "AMD GPU",
            Vendor::Nvidia => "NVIDIA GPU",
            Vendor::Intel => "Intel GPU",
            Vendor::Other => "GPU",
        }
    }
}

/// The marketing name in pci.ids: the bracketed part of the device line
/// ("Navi 31 [Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M]"), else the line.
pub fn pci_ids_name(ids: &str, vendor: &str, device: &str) -> Option<String> {
    let vendor = vendor.trim_start_matches("0x").to_ascii_lowercase();
    let device = device.trim_start_matches("0x").to_ascii_lowercase();
    let mut in_vendor = false;
    for line in ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if in_vendor {
                return None;
            }
            in_vendor = line.to_ascii_lowercase().starts_with(&format!("{vendor} "));
            continue;
        }
        if in_vendor && !line.starts_with("\t\t") {
            let rest = &line[1..];
            if rest.to_ascii_lowercase().starts_with(&format!("{device} ")) {
                let name = rest[device.len()..].trim();
                if let (Some(a), Some(b)) = (name.rfind('['), name.rfind(']')) {
                    if b > a + 1 {
                        return Some(name[a + 1..b].to_string());
                    }
                }
                return Some(name.to_string());
            }
        }
    }
    None
}

/// "00000000:01:00.0" (nvidia-smi) as sysfs spells it: "0000:01:00.0".
pub fn normalize_bus_id(raw: &str) -> String {
    let raw = raw.trim();
    match raw.split_once(':') {
        Some((domain, rest)) => match u32::from_str_radix(domain, 16) {
            Ok(d) => format!("{d:04x}:{}", rest.to_ascii_lowercase()),
            Err(_) => raw.to_ascii_lowercase(),
        },
        None => raw.to_ascii_lowercase(),
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Reading {
    pub load: Option<f64>,
    pub mhz: Option<f64>,
    pub mem_mhz: Option<f64>,
    pub temp: Option<f64>,
    pub hotspot: Option<f64>,
    pub watts: Option<f64>,
    pub watts_cap: Option<f64>,
    pub fan_percent: Option<f64>,
    pub fan_rpm: Option<f64>,
    pub vram_used: Option<f64>,
    pub vram_total: Option<f64>,
}

/// One nvidia-smi CSV row: bus id, name, then the readings.
pub fn parse_nvidia_row(line: &str) -> Option<(String, String, Reading)> {
    let f: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if f.len() < 11 {
        return None;
    }
    let n = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
    let mib = |s: &str| n(s).map(|v| v * 1048576.0);
    Some((
        normalize_bus_id(f[0]),
        f[1].to_string(),
        Reading {
            load: n(f[2]),
            vram_used: mib(f[3]),
            vram_total: mib(f[4]),
            temp: n(f[5]),
            watts: n(f[6]),
            watts_cap: n(f[7]),
            mhz: n(f[8]),
            mem_mhz: n(f[9]),
            fan_percent: n(f[10]),
            ..Reading::default()
        },
    ))
}

const NVIDIA_QUERY: &str = "pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed";

struct Hwmon {
    temp: Option<Probe>,
    hotspot: Option<Probe>,
    mhz: Option<Probe>,
    mem_mhz: Option<Probe>,
    watts: Option<Probe>,
    watts_cap: Option<f64>,
    fan: Option<Probe>,
    pwm: Option<Probe>,
    pwm_max: f64,
}

fn labelled(dir: &str, prefix: &str, label: &str) -> Option<String> {
    sys::list_dir(dir).into_iter().find(|f| {
        f.starts_with(prefix)
            && f.ends_with("_label")
            && sys::read_text(format!("{dir}/{f}")).is_some_and(|l| l.eq_ignore_ascii_case(label))
    }).map(|f| format!("{dir}/{}", f.replace("_label", "_input")))
}

impl Hwmon {
    fn find(device: &str) -> Option<Hwmon> {
        let hw = sys::list_dir(format!("{device}/hwmon")).into_iter().next()?;
        let dir = format!("{device}/hwmon/{hw}");
        let probe = |p: Option<String>| p.filter(|p| sys::exists(p)).map(Probe::new);
        let first_temp = Some(format!("{dir}/temp1_input"));
        Some(Hwmon {
            temp: probe(labelled(&dir, "temp", "edge").or(first_temp)),
            hotspot: probe(labelled(&dir, "temp", "junction")),
            mhz: probe(labelled(&dir, "freq", "sclk").or_else(|| Some(format!("{dir}/freq1_input")))),
            mem_mhz: probe(labelled(&dir, "freq", "mclk")),
            watts: probe(Some(format!("{dir}/power1_average"))).or_else(|| probe(Some(format!("{dir}/power1_input")))),
            watts_cap: sys::read_f64(format!("{dir}/power1_cap")).map(|v| v / 1e6).filter(|v| *v > 0.0),
            fan: probe(Some(format!("{dir}/fan1_input"))),
            pwm: probe(Some(format!("{dir}/pwm1"))),
            pwm_max: sys::read_f64(format!("{dir}/pwm1_max")).unwrap_or(255.0),
        })
    }
}

struct Card {
    id: String,
    device: String,
    vendor: Vendor,
    driver: String,
    name: String,
    integrated: bool,
    boot_vga: bool,
    hwmon: Option<Hwmon>,
    busy: Option<Probe>,
    vram_used: Option<Probe>,
    vram_total: Option<f64>,
    intel_freq: Option<Probe>,
    reading: Reading,
    asleep: bool,
}

#[derive(Clone, Debug)]
pub struct GpuProc {
    pub pid: u32,
    pub name: String,
    pub count: u32,
    pub load: f64,
    pub vram: u64,
}

pub struct Gpus {
    cards: Vec<Card>,
    nvidia: Option<Arc<Mutex<HashMap<String, (String, Reading)>>>>,
    scanned: Instant,
}

impl Gpus {
    pub fn new() -> Self {
        let mut g = Gpus { cards: Vec::new(), nvidia: None, scanned: Instant::now() };
        g.scan();
        g
    }

    fn scan(&mut self) {
        self.scanned = Instant::now();
        let ids = sys::read_text("/usr/share/hwdata/pci.ids").or_else(|| sys::read_text("/usr/share/misc/pci.ids")).unwrap_or_default();
        let mut cards: Vec<Card> = Vec::new();
        for name in sys::list_dir("/sys/class/drm") {
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let Some(device) = sys::canonical(format!("/sys/class/drm/{name}/device")) else { continue };
            let device = device.to_string_lossy().into_owned();
            if cards.iter().any(|c| c.device == device) {
                continue;
            }
            let Some(vendor_id) = sys::read_text(format!("{device}/vendor")) else { continue };
            let vendor = Vendor::from_pci(&vendor_id);
            let device_id = sys::read_text(format!("{device}/device")).unwrap_or_default();
            let driver = sys::link_name(format!("{device}/driver")).unwrap_or_default();
            let slot = device.rsplit('/').next().unwrap_or("").to_string();
            let product = sys::read_text(format!("{device}/product_name")).filter(|s| !s.is_empty());
            let name = product
                .or_else(|| pci_ids_name(&ids, &vendor_id, &device_id))
                .unwrap_or_else(|| vendor.fallback().to_string());
            let hwmon = Hwmon::find(&device);
            let hw_dir = sys::list_dir(format!("{device}/hwmon")).into_iter().next().map(|h| format!("{device}/hwmon/{h}"));
            let integrated = match vendor {
                // An APU's hwmon reports its northbridge rail.
                Vendor::Amd => hw_dir.as_deref().is_some_and(|d| labelled(d, "in", "vddnb").is_some()),
                Vendor::Intel => slot.starts_with("0000:00:"),
                _ => false,
            };
            let mem = |f: &str| Some(format!("{device}/{f}")).filter(|p| sys::exists(p));
            let (used, total) = if integrated {
                (mem("mem_info_gtt_used"), mem("mem_info_gtt_total"))
            } else {
                (mem("mem_info_vram_used"), mem("mem_info_vram_total"))
            };
            let intel_freq = if vendor == Vendor::Intel {
                [format!("/sys/class/drm/{name}/gt_cur_freq_mhz"), format!("{device}/tile0/gt0/freq0/cur_freq")]
                    .into_iter()
                    .find(|p| sys::exists(p))
                    .map(Probe::new)
            } else {
                None
            };
            cards.push(Card {
                id: if sensors_is_pci(&slot) { slot.clone() } else { name.clone() },
                boot_vga: sys::read_text(format!("{device}/boot_vga")).as_deref() == Some("1"),
                busy: mem("gpu_busy_percent").map(Probe::new),
                vram_used: used.map(Probe::new),
                vram_total: total.and_then(sys::read_f64),
                device,
                vendor,
                driver,
                name,
                integrated,
                hwmon,
                intel_freq,
                reading: Reading::default(),
                asleep: false,
            });
        }
        // The boot display first, then PCI order.
        cards.sort_by(|a, b| b.boot_vga.cmp(&a.boot_vga).then(a.id.cmp(&b.id)));
        // Carry readings over a rescan.
        for c in &mut cards {
            if let Some(old) = self.cards.iter().find(|o| o.id == c.id) {
                c.reading = old.reading.clone();
            }
        }
        self.cards = cards;
        let wants_nvidia = self.cards.iter().any(|c| c.vendor == Vendor::Nvidia && c.driver == "nvidia");
        if wants_nvidia && self.nvidia.is_none() {
            if let Some(smi) = sys::system_program("nvidia-smi") {
                let shared = Arc::new(Mutex::new(HashMap::new()));
                self.nvidia = Some(shared.clone());
                let devices: Vec<String> = self
                    .cards
                    .iter()
                    .filter(|c| c.vendor == Vendor::Nvidia)
                    .map(|c| c.device.clone())
                    .collect();
                std::thread::spawn(move || nvidia_loop(smi, devices, shared));
            }
        }
    }

    pub fn sample(&mut self, gate: &mut Gate) {
        if self.scanned.elapsed() >= Duration::from_secs(60) {
            self.scan();
        }
        let nvidia = self.nvidia.as_ref().and_then(|m| m.lock().ok().map(|m| m.clone()));
        for c in &mut self.cards {
            c.asleep = gate.asleep(&c.device);
            if c.asleep {
                c.reading = Reading::default();
                continue;
            }
            if c.vendor == Vendor::Nvidia && c.driver == "nvidia" {
                if let Some((name, r)) = nvidia.as_ref().and_then(|m| m.get(&c.id)) {
                    c.reading = r.clone();
                    if c.name == Vendor::Nvidia.fallback() || c.name.is_empty() {
                        c.name = name.clone();
                    }
                }
                continue;
            }
            if !gate.may_read(&c.device) {
                continue;
            }
            let mut r = Reading::default();
            r.load = c.busy.as_mut().and_then(|p| p.f64()).map(|v| v.clamp(0.0, 100.0));
            if let Some(h) = c.hwmon.as_mut() {
                let get = |p: &mut Option<Probe>| p.as_mut().and_then(|p| p.f64());
                r.temp = get(&mut h.temp).map(|v| v / 1000.0);
                r.hotspot = get(&mut h.hotspot).map(|v| v / 1000.0);
                r.mhz = get(&mut h.mhz).map(|v| v / 1e6);
                r.mem_mhz = get(&mut h.mem_mhz).map(|v| v / 1e6);
                r.watts = get(&mut h.watts).map(|v| v / 1e6);
                r.fan_rpm = get(&mut h.fan);
                let pwm_max = h.pwm_max;
                r.fan_percent = get(&mut h.pwm).map(|v| (v / pwm_max * 100.0).clamp(0.0, 100.0));
                r.watts_cap = h.watts_cap;
            }
            if let Some(p) = c.intel_freq.as_mut() {
                r.mhz = p.f64();
            }
            r.vram_used = c.vram_used.as_mut().and_then(|p| p.f64());
            r.vram_total = c.vram_total;
            c.reading = r;
        }
    }

    /// The watts of an integrated AMD card: on an APU that is the package.
    pub fn apu_watts(&self) -> Option<f64> {
        self.cards.iter().find(|c| c.integrated && c.vendor == Vendor::Amd).and_then(|c| c.reading.watts)
    }

    /// Readings for the sensors list of cards that have no hwmon (NVIDIA).
    pub fn nvidia_sensors(&self) -> (Vec<(String, String, String, f64)>, Vec<(String, String, String, f64)>) {
        let mut temps = Vec::new();
        let mut fans = Vec::new();
        for c in self.cards.iter().filter(|c| c.vendor == Vendor::Nvidia && c.hwmon.is_none()) {
            if let Some(t) = c.reading.temp {
                temps.push((format!("nvidia@{}/temp", c.id), "GPU".into(), c.name.clone(), t));
            }
            if let Some(f) = c.reading.fan_percent {
                fans.push((format!("nvidia@{}/fan", c.id), "GPU".into(), format!("{} fan", c.name), f));
            }
        }
        (temps, fans)
    }

    pub fn ids(&self) -> Vec<(String, bool)> {
        self.cards.iter().map(|c| (c.id.clone(), c.integrated)).collect()
    }

    pub fn write(&self, j: &mut Json, procs: Option<&HashMap<String, Vec<GpuProc>>>) {
        j.arr("gpus");
        for c in &self.cards {
            let r = &c.reading;
            j.begin_obj();
            j.str("id", &c.id);
            j.str("vendor", c.vendor.as_str());
            j.str("name", bounded(&c.name, 80));
            j.bool("integrated", c.integrated);
            j.bool("asleep", c.asleep);
            j.opt("load", r.load, 1);
            j.opt("mhz", r.mhz, 0);
            j.opt("memMhz", r.mem_mhz, 0);
            j.opt("temp", r.temp, 1);
            j.opt("hotspot", r.hotspot, 1);
            j.opt("watts", r.watts, 1);
            j.opt("wattsCap", r.watts_cap, 0);
            j.opt("fanPercent", r.fan_percent, 0);
            j.opt("fanRpm", r.fan_rpm, 0);
            j.opt("vramUsed", r.vram_used, 0);
            j.opt("vramTotal", r.vram_total, 0);
            if let Some(map) = procs {
                j.arr("procs");
                for p in map.get(&c.id).map(|v| v.as_slice()).unwrap_or(&[]) {
                    j.begin_obj();
                    j.uint("pid", p.pid as u64);
                    j.str("name", &p.name);
                    j.uint("count", p.count as u64);
                    j.num("load", p.load, 1);
                    j.uint("vram", p.vram);
                    j.end_obj();
                }
                j.end_arr();
            }
            j.end_obj();
        }
        j.end_arr();
    }
}

fn sensors_is_pci(s: &str) -> bool {
    crate::sensors::is_pci_address(s)
}

/// Every nvidia-smi call wakes every NVIDIA card, so ask only while none is
/// suspended, every 2 s for cards that never sleep and every 7 s otherwise,
/// which leaves an idle card room to suspend between questions.
fn nvidia_loop(smi: std::path::PathBuf, devices: Vec<String>, shared: Arc<Mutex<HashMap<String, (String, Reading)>>>) {
    loop {
        let powers: Vec<Power> = devices.iter().map(|d| wake::power_of(d)).collect();
        let asleep = powers.iter().any(|p| *p == Power::Asleep);
        let pause = if powers.iter().any(|p| matches!(p, Power::Idleable(_))) { 7 } else { 2 };
        if !asleep {
            let out = sys::run(
                &smi,
                &[&format!("--query-gpu={NVIDIA_QUERY}"), "--format=csv,noheader,nounits"],
                Duration::from_secs(5),
            );
            let mut map = HashMap::new();
            for line in out.stdout.lines() {
                if let Some((id, name, r)) = parse_nvidia_row(line) {
                    map.insert(id, (name, r));
                }
            }
            if let Ok(mut m) = shared.lock() {
                *m = map;
            }
        }
        std::thread::sleep(Duration::from_secs(pause));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pci_ids_bracketed_names() {
        let ids = "# c\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t744c  Navi 31 [Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M]\n\t\t1002 0e3b  sub\n\t7480  Navi 33\n10de  NVIDIA Corporation\n";
        assert_eq!(pci_ids_name(ids, "0x1002", "0x744c").as_deref(), Some("Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M"));
        assert_eq!(pci_ids_name(ids, "0x1002", "0x7480").as_deref(), Some("Navi 33"));
        assert_eq!(pci_ids_name(ids, "0x10de", "0x2684"), None);
    }

    #[test]
    fn nvidia_rows() {
        let (id, name, r) = parse_nvidia_row("00000000:01:00.0, NVIDIA GeForce RTX 4090, 12, 1024, 24564, 45, 30.5, 450.00, 210, 405, [N/A]").unwrap();
        assert_eq!(id, "0000:01:00.0");
        assert_eq!(name, "NVIDIA GeForce RTX 4090");
        assert_eq!(r.load, Some(12.0));
        assert_eq!(r.vram_used, Some(1024.0 * 1048576.0));
        assert_eq!(r.fan_percent, None);
        assert!(parse_nvidia_row("short, row").is_none());
    }
}

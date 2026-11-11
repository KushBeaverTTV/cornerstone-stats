//! Every hwmon temperature and fan.

use crate::json::Json;
use crate::sys::{self, Probe};
use crate::wake::{self, Gate};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// Drivers that can appear several times on one machine, so their id always
/// names the device.
const MULTI: &[&str] = &["nvme", "drivetemp", "spd5118", "jc42", "ee1004", "amdgpu", "nouveau", "i915", "xe"];

pub fn group_of(driver: &str) -> String {
    let base = driver.split('_').next().unwrap_or(driver);
    let group = match base {
        "k10temp" | "zenpower" | "coretemp" | "k8temp" | "cpu" | "via" => "CPU",
        "amdgpu" | "radeon" | "nouveau" | "i915" | "xe" => "GPU",
        "nvme" => "NVMe",
        "drivetemp" => "Drive",
        "spd5118" | "jc42" | "ee1004" => "Memory",
        "acpitz" => "ACPI",
        "iwlwifi" | "mt7921" | "mt7925" | "ath10k" | "ath11k" | "ath12k" | "rtw89" | "rtw88" | "brcmfmac" => "Wi-Fi",
        "r8169" | "igc" | "e1000e" | "atlantic" | "ixgbe" | "tg3" | "bnxt" => "Ethernet",
        "cpu_thermal" | "soc_thermal" => "CPU",
        b if b.starts_with("nct") || b.starts_with("it8") || b.starts_with("w83") || b.starts_with("f718")
            || b.starts_with("asus") || b.starts_with("gigabyte") || b.starts_with("dell") || b.starts_with("thinkpad")
            || b.starts_with("hp") || b == "corsaircpro" || b == "nzxt" => "Board",
        _ => return driver.to_string(),
    };
    group.to_string()
}

/// A tag for the device under an hwmon chip that survives reboots: the PCI
/// address for an NVMe controller (nvme0 can swap), else the device's name.
pub fn device_tag(canonical_device: &Path) -> String {
    let base = canonical_device.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if base.starts_with("nvme") {
        for part in canonical_device.ancestors() {
            let name = part.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if is_pci_address(&name) {
                return name;
            }
        }
    }
    base
}

pub fn is_pci_address(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 12 && b[4] == b':' && b[7] == b':' && b[10] == b'.'
        && s.chars().enumerate().all(|(i, c)| matches!(i, 4 | 7 | 10) || c.is_ascii_hexdigit())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Temp,
    Fan,
}

struct Sensor {
    id: String,
    group: String,
    label: String,
    kind: Kind,
    probe: Probe,
    /// Celsius limit for temperatures.
    critical: Option<f64>,
    /// Graphics card behind it, whose reads are rationed (see wake.rs).
    gpu: Option<String>,
    value: Option<f64>,
}

pub struct Sensors {
    list: Vec<Sensor>,
    scanned: Option<Instant>,
    hwmon_names: Vec<String>,
}

fn readable_label(raw: &str) -> String {
    let t = raw.trim();
    // "fan1" style labels from drivers read better as "Fan 1".
    let lower = t.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix("fan") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return format!("Fan {n}");
        }
    }
    t.to_string()
}

impl Sensors {
    pub fn new() -> Self {
        let mut s = Sensors { list: Vec::new(), scanned: None, hwmon_names: Vec::new() };
        s.scan();
        s
    }

    fn scan(&mut self) {
        self.scanned = Some(Instant::now());
        self.hwmon_names = sys::list_dir("/sys/class/hwmon");
        let mut chips = Vec::new();
        let mut count: HashMap<String, usize> = HashMap::new();
        for hw in &self.hwmon_names {
            let dir = format!("/sys/class/hwmon/{hw}");
            let Some(name) = sys::read_text(format!("{dir}/name")) else { continue };
            *count.entry(name.clone()).or_default() += 1;
            chips.push((dir, name));
        }
        let mut list = Vec::new();
        for (dir, name) in chips {
            let device = sys::canonical(format!("{dir}/device"));
            let tag = device.as_deref().map(device_tag).unwrap_or_default();
            let prefix = if (MULTI.contains(&name.as_str()) || count[&name] > 1) && !tag.is_empty() {
                format!("{name}@{tag}")
            } else {
                name.clone()
            };
            let gpu = device.as_deref().and_then(wake::gpu_device_of);
            let group = group_of(&name);
            let files = sys::list_dir(&dir);
            let temps: Vec<&String> = files.iter().filter(|f| f.starts_with("temp") && f.ends_with("_input")).collect();
            let fans: Vec<&String> = files.iter().filter(|f| f.starts_with("fan") && f.ends_with("_input")).collect();
            for (kind, inputs) in [(Kind::Temp, &temps), (Kind::Fan, &fans)] {
                for input in inputs.iter() {
                    let stem = input.trim_end_matches("_input");
                    let number = stem.trim_start_matches(|c: char| c.is_ascii_alphabetic());
                    let own = sys::read_text(format!("{dir}/{stem}_label")).filter(|l| !l.is_empty());
                    let label = match (own, kind) {
                        (Some(l), _) => readable_label(&l),
                        (None, Kind::Fan) => format!("Fan {number}"),
                        (None, Kind::Temp) if inputs.len() == 1 => String::new(),
                        (None, Kind::Temp) => number.to_string(),
                    };
                    let critical = if kind == Kind::Temp {
                        sys::read_f64(format!("{dir}/{stem}_crit"))
                            .or_else(|| sys::read_f64(format!("{dir}/{stem}_max")))
                            .map(|m| m / 1000.0)
                            .filter(|c| *c > 20.0 && *c < 200.0)
                    } else {
                        None
                    };
                    list.push(Sensor {
                        id: format!("{prefix}/{stem}"),
                        group: group.clone(),
                        label,
                        kind,
                        probe: Probe::new(format!("{dir}/{input}")),
                        critical,
                        gpu: gpu.clone(),
                        value: None,
                    });
                }
            }
        }
        self.list = list;
    }

    pub fn sample(&mut self, gate: &mut Gate) {
        if self.scanned.map_or(true, |t| t.elapsed().as_secs() >= 30) {
            let names = sys::list_dir("/sys/class/hwmon");
            if names != self.hwmon_names {
                self.scan();
            } else {
                self.scanned = Some(Instant::now());
            }
        }
        for s in &mut self.list {
            if let Some(dev) = &s.gpu {
                if gate.asleep(dev) {
                    s.value = None;
                    continue;
                }
                if !gate.may_read(dev) {
                    continue; // keep the last value
                }
            }
            s.value = s.probe.f64().map(|v| if s.kind == Kind::Temp { v / 1000.0 } else { v });
        }
    }

    /// Extra entries (NVIDIA cards read through nvidia-smi):
    /// (id, group, label, celsius) temps and (id, group, label, percent) fans.
    pub fn write(&self, j: &mut Json, extra_temps: &[(String, String, String, f64)], extra_fans: &[(String, String, String, f64)]) {
        j.obj("sensors");
        j.arr("temps");
        for s in self.list.iter().filter(|s| s.kind == Kind::Temp) {
            let Some(v) = s.value else { continue };
            // Unplugged or unsupported inputs read as absurd values.
            if !(-40.0..=200.0).contains(&v) {
                continue;
            }
            j.begin_obj();
            j.str("id", &s.id);
            j.str("group", &s.group);
            j.str("label", &s.label);
            j.num("celsius", v, 1);
            j.opt("critical", s.critical, 0);
            j.end_obj();
        }
        for (id, group, label, c) in extra_temps {
            j.begin_obj();
            j.str("id", id);
            j.str("group", group);
            j.str("label", label);
            j.num("celsius", *c, 1);
            j.null("critical");
            j.end_obj();
        }
        j.end_arr();
        j.arr("fans");
        for s in self.list.iter().filter(|s| s.kind == Kind::Fan) {
            let Some(v) = s.value else { continue };
            j.begin_obj();
            j.str("id", &s.id);
            j.str("group", &s.group);
            j.str("label", &s.label);
            j.num("rpm", v, 0);
            j.null("percent");
            j.end_obj();
        }
        for (id, group, label, pct) in extra_fans {
            j.begin_obj();
            j.str("id", id);
            j.str("group", group);
            j.str("label", label);
            j.null("rpm");
            j.num("percent", *pct, 0);
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
    fn groups_and_labels() {
        assert_eq!(group_of("k10temp"), "CPU");
        assert_eq!(group_of("iwlwifi_1"), "Wi-Fi");
        assert_eq!(group_of("nct6687"), "Board");
        assert_eq!(group_of("mystery"), "mystery");
        assert_eq!(readable_label("fan2"), "Fan 2");
        assert_eq!(readable_label("Composite"), "Composite");
    }

    #[test]
    fn nvme_ids_use_the_pci_address() {
        let p = Path::new("/sys/devices/pci0000:00/0000:00:01.2/0000:01:00.0/0000:02:02.0/0000:03:00.0/nvme/nvme0");
        assert_eq!(device_tag(p), "0000:03:00.0");
        assert_eq!(device_tag(Path::new("/sys/devices/x/0:0:0:0")), "0:0:0:0");
        assert!(is_pci_address("0000:0c:00.0"));
        assert!(!is_pci_address("nvme0"));
    }
}

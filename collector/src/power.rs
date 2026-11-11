//! Power readings: RAPL energy counters, a power supply that reports its
//! output, and zenpower's core rails.

use crate::json::Json;
use crate::sys::{self, Probe};
use std::time::Instant;

struct Zone {
    kind: String,
    energy: Probe,
    range: u64,
    last: Option<(u64, Instant)>,
    watts: Option<f64>,
}

impl Zone {
    fn sample(&mut self) {
        let now = Instant::now();
        let Some(uj) = self.energy.u64() else {
            self.watts = None;
            return;
        };
        if let Some((prev, at)) = self.last {
            let dt = now.duration_since(at).as_secs_f64();
            if dt > 0.05 {
                let delta = if uj >= prev { uj - prev } else { uj + self.range.saturating_sub(prev) };
                self.watts = Some(delta as f64 / 1e6 / dt);
            }
        }
        self.last = Some((uj, now));
    }
}

pub struct Power {
    zones: Vec<Zone>,
    pub rapl_hint: bool,
    psu: Option<Probe>,
    zen: Vec<Probe>,
}

impl Power {
    pub fn new() -> Self {
        let mut zones = Vec::new();
        let mut hint = false;
        for name in sys::list_dir("/sys/class/powercap") {
            if !name.starts_with("intel-rapl:") {
                continue;
            }
            let dir = format!("/sys/class/powercap/{name}");
            let kind = sys::read_text(format!("{dir}/name")).unwrap_or_default();
            let kind = if kind.starts_with("package") { "package".to_string() } else { kind };
            let path = format!("{dir}/energy_uj");
            if std::fs::File::open(&path).is_err() {
                hint = true;
                continue;
            }
            zones.push(Zone {
                kind,
                energy: Probe::new(path),
                range: sys::read_u64(format!("{dir}/max_energy_range_uj")).unwrap_or(u64::MAX / 2),
                last: None,
                watts: None,
            });
        }
        let mut psu = None;
        let mut zen = Vec::new();
        for hw in sys::list_dir("/sys/class/hwmon") {
            let dir = format!("/sys/class/hwmon/{hw}");
            let name = sys::read_text(format!("{dir}/name")).unwrap_or_default();
            if name.contains("psu") && psu.is_none() && sys::exists(format!("{dir}/power1_input")) {
                psu = Some(Probe::new(format!("{dir}/power1_input")));
            }
            if name == "zenpower" {
                for f in ["power1_input", "power2_input"] {
                    if sys::exists(format!("{dir}/{f}")) {
                        zen.push(Probe::new(format!("{dir}/{f}")));
                    }
                }
            }
        }
        let rapl_hint = hint && zones.is_empty();
        Power { zones, rapl_hint, psu, zen }
    }

    pub fn sample(&mut self) {
        for z in &mut self.zones {
            z.sample();
        }
    }

    fn zone(&self, kind: &str) -> Option<f64> {
        let mut sum = None;
        for z in self.zones.iter().filter(|z| z.kind == kind) {
            if let Some(w) = z.watts {
                *sum.get_or_insert(0.0) += w;
            }
        }
        sum
    }

    /// The processor package: RAPL, else zenpower's core + SoC rails.
    pub fn package(&mut self) -> Option<f64> {
        if let Some(w) = self.zone("package") {
            return Some(w);
        }
        if self.zen.is_empty() {
            return None;
        }
        let mut sum = 0.0;
        for p in &mut self.zen {
            sum += p.f64()? / 1e6;
        }
        Some(sum)
    }

    pub fn write(&mut self, j: &mut Json) {
        let psu = self.psu.as_mut().and_then(|p| p.f64()).map(|v| v / 1e6);
        j.obj("power");
        j.obj("rapl");
        for kind in ["package", "core", "uncore", "dram", "psys"] {
            j.opt(kind, self.zone(kind), 1);
        }
        j.end_obj();
        j.opt("psu", psu, 1);
        j.bool("raplHint", self.rapl_hint);
        j.end_obj();
    }
}

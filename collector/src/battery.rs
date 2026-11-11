//! The laptop battery, the charger, and wireless devices' batteries
//! (power_supply entries whose scope is Device).

use crate::json::{bounded, Json};
use crate::sys;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Peripheral {
    pub name: String,
    pub percent: Option<f64>,
    pub status: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Battery {
    pub present: bool,
    pub percent: Option<f64>,
    pub status: String,
    pub ac_online: Option<bool>,
    pub watts: Option<f64>,
    pub time_to_empty: Option<f64>,
    pub time_to_full: Option<f64>,
    pub energy_now: Option<f64>,
    pub energy_full: Option<f64>,
    pub energy_design: Option<f64>,
    pub health: Option<f64>,
    pub cycles: Option<u64>,
    pub voltage: Option<f64>,
    pub model: String,
    pub technology: String,
    pub peripherals: Vec<Peripheral>,
}

fn level_percent(level: &str) -> Option<f64> {
    match level {
        "Full" => Some(100.0),
        "High" => Some(80.0),
        "Normal" => Some(50.0),
        "Low" => Some(20.0),
        "Critical" => Some(5.0),
        _ => None,
    }
}

/// Read every power supply under `base` (normally /sys/class/power_supply).
/// None when there is neither a battery nor a device battery.
pub fn read(base: &Path) -> Option<Battery> {
    let mut b = Battery::default();
    let mut main: Option<PathBuf> = None;
    for name in sys::list_dir(base) {
        let dir = base.join(&name);
        let get = |f: &str| sys::read_text(dir.join(f)).filter(|s| !s.is_empty());
        let kind = get("type").unwrap_or_default();
        match kind.as_str() {
            "Mains" | "USB" => {
                if let Some(online) = get("online") {
                    let on = online == "1";
                    b.ac_online = Some(b.ac_online.unwrap_or(false) || on);
                }
            }
            "Battery" => {
                if get("scope").as_deref() == Some("Device") {
                    b.peripherals.push(Peripheral {
                        name: get("model_name").unwrap_or_else(|| name.clone()),
                        percent: get("capacity").and_then(|v| v.parse().ok()).or_else(|| get("capacity_level").as_deref().and_then(level_percent)),
                        status: get("status").unwrap_or_default(),
                    });
                } else if main.is_none() && get("present").as_deref() != Some("0") {
                    main = Some(dir.clone());
                }
            }
            _ => {}
        }
    }
    if let Some(dir) = main {
        let num = |f: &str| sys::read_f64(dir.join(f));
        let text = |f: &str| sys::read_text(dir.join(f)).unwrap_or_default();
        let volts = num("voltage_now").map(|v| v / 1e6);
        let design_volts = num("voltage_min_design").map(|v| v / 1e6).or(volts);
        // Energy in Wh, from energy_* (µWh) or charge_* (µAh) times voltage.
        let wh = |e: &str, c: &str, v: Option<f64>| num(e).map(|x| x / 1e6).or_else(|| Some(num(c)? / 1e6 * v?));
        b.present = true;
        b.energy_now = wh("energy_now", "charge_now", volts);
        b.energy_full = wh("energy_full", "charge_full", design_volts);
        b.energy_design = wh("energy_full_design", "charge_full_design", design_volts);
        b.percent = num("capacity").or_else(|| Some(b.energy_now? / b.energy_full? * 100.0)).map(|p| p.clamp(0.0, 100.0));
        b.status = text("status");
        b.watts = num("power_now")
            .map(|w| w / 1e6)
            .or_else(|| Some(num("current_now")? / 1e6 * volts?))
            .map(f64::abs)
            .filter(|w| *w > 0.0);
        b.voltage = volts;
        b.cycles = sys::read_u64(dir.join("cycle_count")).filter(|c| *c > 0);
        b.model = text("model_name");
        b.technology = text("technology");
        b.health = match (b.energy_full, b.energy_design) {
            (Some(f), Some(d)) if d > 0.0 => Some((f / d * 100.0).min(150.0)),
            _ => None,
        };
        if let Some(w) = b.watts {
            if b.status == "Discharging" {
                b.time_to_empty = b.energy_now.map(|e| e / w * 60.0);
            } else if b.status == "Charging" {
                b.time_to_full = match (b.energy_full, b.energy_now) {
                    (Some(f), Some(n)) if f > n => Some((f - n) / w * 60.0),
                    _ => None,
                };
            }
        }
    }
    if !b.present && b.peripherals.is_empty() {
        return None;
    }
    Some(b)
}

pub fn write(j: &mut Json, battery: &Option<Battery>) {
    let Some(b) = battery else {
        j.null("battery");
        return;
    };
    j.obj("battery");
    j.bool("present", b.present);
    j.opt("percent", b.percent, 0);
    if b.present {
        j.str("status", bounded(&b.status, 20));
    } else {
        j.null("status");
    }
    match b.ac_online {
        Some(v) => j.bool("acOnline", v),
        None => j.null("acOnline"),
    }
    j.opt("watts", b.watts, 1);
    j.opt("timeToEmpty", b.time_to_empty, 0);
    j.opt("timeToFull", b.time_to_full, 0);
    j.opt("energyNow", b.energy_now, 1);
    j.opt("energyFull", b.energy_full, 1);
    j.opt("energyDesign", b.energy_design, 1);
    j.opt("health", b.health, 0);
    j.opt_uint("cycles", b.cycles);
    j.opt("voltage", b.voltage, 2);
    j.opt_str("model", (!b.model.is_empty()).then(|| bounded(&b.model, 60)));
    j.opt_str("technology", (!b.technology.is_empty()).then(|| bounded(&b.technology, 20)));
    j.arr("peripherals");
    for p in &b.peripherals {
        j.begin_obj();
        j.str("name", bounded(&p.name, 60));
        j.opt("percent", p.percent, 0);
        j.str("status", bounded(&p.status, 20));
        j.end_obj();
    }
    j.end_arr();
    j.end_obj();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn supply(base: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = base.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (f, v) in files {
            fs::write(dir.join(f), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn laptop_battery_from_a_fake_sysfs() {
        let base = std::env::temp_dir().join(format!("csc-bat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        supply(&base, "AC", &[("type", "Mains"), ("online", "0")]);
        supply(&base, "BAT0", &[
            ("type", "Battery"), ("present", "1"), ("status", "Discharging"), ("capacity", "50"),
            ("energy_now", "30000000"), ("energy_full", "60000000"), ("energy_full_design", "80000000"),
            ("power_now", "15000000"), ("voltage_now", "12000000"), ("cycle_count", "120"),
            ("model_name", "5B10W51867"), ("technology", "Li-poly"),
        ]);
        supply(&base, "hidpp_battery_0", &[("type", "Battery"), ("scope", "Device"), ("model_name", "MX Master 3"), ("capacity_level", "High"), ("status", "Discharging")]);
        let b = read(&base).unwrap();
        assert!(b.present);
        assert_eq!(b.percent, Some(50.0));
        assert_eq!(b.ac_online, Some(false));
        assert_eq!(b.watts, Some(15.0));
        assert_eq!(b.time_to_empty, Some(120.0));
        assert_eq!(b.health, Some(75.0));
        assert_eq!(b.cycles, Some(120));
        assert_eq!(b.peripherals, vec![Peripheral { name: "MX Master 3".into(), percent: Some(80.0), status: "Discharging".into() }]);
        let mut j = Json::new();
        j.begin_obj();
        write(&mut j, &Some(b));
        j.end_obj();
        let v: serde_json::Value = serde_json::from_str(&j.out).unwrap();
        assert_eq!(v["battery"]["energyFull"], 60);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn charge_counters_and_desktops() {
        let base = std::env::temp_dir().join(format!("csc-bat2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        supply(&base, "BAT1", &[
            ("type", "Battery"), ("status", "Charging"), ("charge_now", "2000000"), ("charge_full", "4000000"),
            ("charge_full_design", "4000000"), ("current_now", "1000000"), ("voltage_now", "10000000"), ("voltage_min_design", "10000000"),
        ]);
        let b = read(&base).unwrap();
        assert_eq!(b.energy_now, Some(20.0));
        assert_eq!(b.percent, Some(50.0));
        assert_eq!(b.time_to_full, Some(120.0));
        let empty = base.join("none");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(read(&empty), None);
        let _ = fs::remove_dir_all(&base);
    }
}

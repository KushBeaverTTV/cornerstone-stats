//! Processor load, clocks, temperature, load average and uptime.

use crate::json::Json;
use crate::sys::{self, Probe};
use std::collections::HashSet;
use std::time::Instant;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Times {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub total: u64,
}

/// One "cpu..." line of /proc/stat: user nice system idle iowait irq softirq steal.
/// guest and guest_nice are already inside user and nice.
pub fn parse_times(line: &str) -> Option<(Option<usize>, Times)> {
    let mut parts = line.split_ascii_whitespace();
    let label = parts.next()?;
    let index = label.strip_prefix("cpu")?;
    let index = if index.is_empty() { None } else { Some(index.parse().ok()?) };
    let mut v = [0u64; 8];
    for slot in v.iter_mut() {
        *slot = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    }
    let [user, nice, system, idle, iowait, irq, softirq, steal] = v;
    Some((
        index,
        Times {
            user: user + nice,
            system: system + irq + softirq + steal,
            idle,
            iowait,
            total: user + nice + system + idle + iowait + irq + softirq + steal,
        },
    ))
}

#[derive(Default, Clone, Copy)]
pub struct Usage {
    pub total: f64,
    pub user: f64,
    pub system: f64,
    pub iowait: f64,
}

pub fn usage_between(a: &Times, b: &Times) -> Usage {
    let span = b.total.saturating_sub(a.total) as f64;
    if span <= 0.0 {
        return Usage::default();
    }
    let pct = |x: u64, y: u64| (y.saturating_sub(x) as f64 / span * 100.0).clamp(0.0, 100.0);
    let idle = pct(a.idle, b.idle);
    let iowait = pct(a.iowait, b.iowait);
    Usage {
        total: (100.0 - idle - iowait).clamp(0.0, 100.0),
        user: pct(a.user, b.user),
        system: pct(a.system, b.system),
        iowait,
    }
}

/// "0-3,8,10-11" as indices.
pub fn parse_cpu_list(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in text.trim().split(',') {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                out.extend(a..=b.min(a + 4096));
            }
        } else if let Ok(n) = part.trim().parse() {
            out.push(n);
        }
    }
    out
}

pub struct Cpu {
    stat: Probe,
    loadavg: Probe,
    uptime: Probe,
    freqs: Vec<Probe>,
    cpuinfo_mhz: Option<f64>,
    cpuinfo_at: Option<Instant>,
    temp: Option<Probe>,
    temp_search_at: Option<Instant>,
    prev_total: Option<Times>,
    prev_threads: Vec<Times>,
    pub usage: Usage,
    pub per_thread: Vec<f64>,
    pub model: String,
    pub cores: usize,
    pub threads: usize,
    pub efficiency: Vec<usize>,
    pub mhz: Option<f64>,
    pub temp_c: Option<f64>,
    pub load: [f64; 3],
    pub uptime_s: f64,
}

/// hwmon drivers that report the processor package, best first, with the
/// labels to prefer inside each.
const CPU_SENSORS: &[(&str, &[&str])] = &[
    ("k10temp", &["Tctl", "Tdie"]),
    ("zenpower", &["Tdie", "Tctl"]),
    ("coretemp", &["Package id 0"]),
    ("k8temp", &[]),
    ("cpu_thermal", &[]),
    ("soc_thermal", &[]),
];

impl Cpu {
    pub fn new() -> Self {
        let cpuinfo = sys::read_text("/proc/cpuinfo").unwrap_or_default();
        let model = ["model name", "Model", "Hardware", "cpu model"]
            .iter()
            .find_map(|k| {
                cpuinfo.lines().find_map(|l| {
                    let (key, value) = l.split_once(':')?;
                    (key.trim() == *k).then(|| value.trim().to_string())
                })
            })
            .unwrap_or_default();
        let online = sys::read_text("/sys/devices/system/cpu/online").map(|t| parse_cpu_list(&t)).unwrap_or_default();
        let mut cores = HashSet::new();
        let mut freqs = Vec::new();
        for &n in &online {
            let base = format!("/sys/devices/system/cpu/cpu{n}");
            let pkg = sys::read_text(format!("{base}/topology/physical_package_id")).unwrap_or_default();
            let core = sys::read_text(format!("{base}/topology/core_id")).unwrap_or_else(|| n.to_string());
            cores.insert((pkg, core));
            let f = format!("{base}/cpufreq/scaling_cur_freq");
            if sys::exists(&f) {
                freqs.push(Probe::new(f));
            }
        }
        let threads = online.len().max(1);
        let efficiency = sys::read_text("/sys/devices/cpu_atom/cpus").map(|t| parse_cpu_list(&t)).unwrap_or_default();
        let mut cpu = Cpu {
            stat: Probe::new("/proc/stat"),
            loadavg: Probe::new("/proc/loadavg"),
            uptime: Probe::new("/proc/uptime"),
            freqs,
            cpuinfo_mhz: None,
            cpuinfo_at: None,
            temp: None,
            temp_search_at: None,
            prev_total: None,
            prev_threads: Vec::new(),
            usage: Usage::default(),
            per_thread: Vec::new(),
            model,
            cores: cores.len().max(1),
            threads,
            efficiency,
            mhz: None,
            temp_c: None,
            load: [0.0; 3],
            uptime_s: 0.0,
        };
        cpu.find_temp();
        cpu
    }

    fn find_temp(&mut self) {
        self.temp_search_at = Some(Instant::now());
        let hwmons = sys::list_dir("/sys/class/hwmon");
        for (driver, labels) in CPU_SENSORS {
            for hw in &hwmons {
                let dir = format!("/sys/class/hwmon/{hw}");
                if sys::read_text(format!("{dir}/name")).as_deref() != Some(driver) {
                    continue;
                }
                let inputs: Vec<String> = sys::list_dir(&dir)
                    .into_iter()
                    .filter(|f| f.starts_with("temp") && f.ends_with("_input"))
                    .collect();
                let label_of = |input: &str| sys::read_text(format!("{dir}/{}", input.replace("_input", "_label")));
                let pick = labels
                    .iter()
                    .find_map(|want| inputs.iter().find(|i| label_of(i).as_deref() == Some(*want)))
                    .or_else(|| inputs.first());
                if let Some(input) = pick {
                    self.temp = Some(Probe::new(format!("{dir}/{input}")));
                    return;
                }
            }
        }
    }

    pub fn sample(&mut self) {
        if let Some(text) = self.stat.read() {
            let mut threads = Vec::with_capacity(self.threads);
            let mut total = None;
            for line in text.lines() {
                if !line.starts_with("cpu") {
                    break;
                }
                match parse_times(line) {
                    Some((None, t)) => total = Some(t),
                    Some((Some(_), t)) => threads.push(t),
                    None => {}
                }
            }
            if let (Some(now), Some(prev)) = (total, self.prev_total) {
                self.usage = usage_between(&prev, &now);
            }
            if threads.len() == self.prev_threads.len() {
                self.per_thread.clear();
                for (a, b) in self.prev_threads.iter().zip(&threads) {
                    self.per_thread.push(usage_between(a, b).total);
                }
            }
            self.prev_total = total;
            self.prev_threads = threads;
        }

        if !self.freqs.is_empty() {
            let mut sum = 0.0;
            let mut n = 0;
            for p in &mut self.freqs {
                if let Some(khz) = p.f64() {
                    sum += khz;
                    n += 1;
                }
            }
            self.mhz = (n > 0).then(|| sum / n as f64 / 1000.0);
        } else {
            // No cpufreq (some VMs): /proc/cpuinfo is large, so every 5 s.
            if self.cpuinfo_at.map_or(true, |t| t.elapsed().as_secs() >= 5) {
                self.cpuinfo_at = Some(Instant::now());
                let text = sys::read_text("/proc/cpuinfo").unwrap_or_default();
                let mhz: Vec<f64> = text
                    .lines()
                    .filter(|l| l.starts_with("cpu MHz"))
                    .filter_map(|l| l.split(':').nth(1)?.trim().parse().ok())
                    .collect();
                self.cpuinfo_mhz = (!mhz.is_empty()).then(|| mhz.iter().sum::<f64>() / mhz.len() as f64);
            }
            self.mhz = self.cpuinfo_mhz;
        }

        if self.temp.is_none() && self.temp_search_at.map_or(true, |t| t.elapsed().as_secs() >= 60) {
            self.find_temp();
        }
        self.temp_c = self.temp.as_mut().and_then(|p| p.f64()).map(|m| m / 1000.0);

        if let Some(text) = self.loadavg.read() {
            let mut it = text.split_ascii_whitespace().filter_map(|v| v.parse::<f64>().ok());
            self.load = [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)];
        }
        if let Some(up) = self.uptime.read().and_then(|t| t.split_ascii_whitespace().next()?.parse().ok()) {
            self.uptime_s = up;
        }
    }

    pub fn write(&self, j: &mut Json, watts: Option<f64>) {
        j.obj("cpu");
        j.str("model", &self.model);
        j.uint("cores", self.cores as u64);
        j.uint("threads", self.threads as u64);
        j.obj("usage");
        j.num("total", self.usage.total, 1);
        j.num("user", self.usage.user, 1);
        j.num("system", self.usage.system, 1);
        j.num("iowait", self.usage.iowait, 1);
        j.end_obj();
        j.arr("perThread");
        for v in &self.per_thread {
            j.item_num(*v, 1);
        }
        j.end_arr();
        j.arr("efficiency");
        for i in &self.efficiency {
            j.item_uint(*i as u64);
        }
        j.end_arr();
        j.opt("mhz", self.mhz, 0);
        j.opt("temp", self.temp_c, 1);
        j.opt("watts", watts, 1);
        j.arr("load");
        for v in self.load {
            j.item_num(v, 2);
        }
        j.end_arr();
        j.num("uptime", self.uptime_s, 0);
        j.end_obj();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_lines_fold_nice_irq_and_steal() {
        let (i, t) = parse_times("cpu3 10 5 20 100 4 1 2 3 0 0").unwrap();
        assert_eq!(i, Some(3));
        assert_eq!(t, Times { user: 15, system: 26, idle: 100, iowait: 4, total: 145 });
        assert_eq!(parse_times("cpu  1 2 3 4 5 6 7 8").unwrap().0, None);
        assert!(parse_times("intr 1 2").is_none());
    }

    #[test]
    fn usage_excludes_idle_and_iowait() {
        let a = Times { user: 0, system: 0, idle: 0, iowait: 0, total: 0 };
        let b = Times { user: 30, system: 10, idle: 50, iowait: 10, total: 100 };
        let u = usage_between(&a, &b);
        assert_eq!((u.total, u.user, u.system, u.iowait), (40.0, 30.0, 10.0, 10.0));
    }

    #[test]
    fn cpu_lists() {
        assert_eq!(parse_cpu_list("0-2,5,7-8\n"), vec![0, 1, 2, 5, 7, 8]);
        assert!(parse_cpu_list("").is_empty());
    }
}

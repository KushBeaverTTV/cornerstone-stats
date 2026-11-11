//! cornerstone-stats-collector: the data source of the Cornerstone Stats
//! bar plugin. See PROTOCOL.md for the frames it writes and the commands it
//! reads; this file owns the loop and decides what each frame collects.

mod battery;
mod cpu;
mod disks;
mod gpu;
mod hypr;
mod json;
mod memory;
mod net;
mod power;
mod procs;
mod sensors;
mod services;
mod sockets;
mod sys;
mod wake;

use json::Json;
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Control {
    interval: f64,
    detail: u8,
    focus: String,
    public_ip: bool,
    refresh: bool,
    quit: bool,
    /// Something changed that the loop should act on before its next tick.
    wake: bool,
}

type Shared = Arc<(Mutex<Control>, Condvar)>;

fn apply(line: &str, c: &mut Control) {
    let mut parts = line.split_ascii_whitespace();
    let (Some(cmd), arg) = (parts.next(), parts.next()) else { return };
    match (cmd, arg) {
        ("detail", Some(v)) => {
            if let Ok(d) = v.parse::<u8>() {
                c.detail = d.min(2);
            }
        }
        ("focus", Some(v)) => {
            let known = ["apps", "cpu", "gpu", "memory", "disks", "network", "sensors", "power", "services", "settings", "none"];
            c.focus = if known.contains(&v) && v != "none" { v.to_string() } else { String::new() };
        }
        ("interval", Some(v)) => {
            if let Ok(s) = v.parse::<f64>() {
                if s.is_finite() {
                    c.interval = s.clamp(0.1, 30.0);
                }
            }
        }
        ("publicip", None) => c.public_ip = true,
        ("refresh", None) => c.refresh = true,
        ("quit", None) => c.quit = true,
        _ => return,
    }
    c.wake = true;
}

struct Collector {
    seq: u64,
    gate: wake::Gate,
    cpu: cpu::Cpu,
    memory: memory::Memory,
    gpus: gpu::Gpus,
    disks: disks::Disks,
    net: net::Net,
    sensors: sensors::Sensors,
    sensors_at: Option<Instant>,
    power: power::Power,
    battery: Option<battery::Battery>,
    battery_at: Option<Instant>,
    procs: procs::Procs,
    /// The first process reading after a quiet spell; per-process figures
    /// need a second one, taken a quarter second later.
    procs_warming: bool,
    apps: Option<String>,
    apps_at: Option<Instant>,
    apps_problem: bool,
    by_drive: Option<HashMap<String, Vec<procs::Row>>>,
    by_drive_at: Option<Instant>,
    gpu_procs: Option<HashMap<String, Vec<gpu::GpuProc>>>,
    gpu_procs_at: Option<Instant>,
    net_procs: Option<Vec<procs::NetRow>>,
    net_procs_at: Option<Instant>,
    services: services::Services,
    out: Json,
}

fn older(at: Option<Instant>, secs: f64) -> bool {
    at.map_or(true, |t| t.elapsed().as_secs_f64() >= secs)
}

impl Collector {
    fn new() -> Self {
        let cpu = cpu::Cpu::new();
        let threads = cpu.threads;
        Collector {
            seq: 0,
            gate: wake::Gate::default(),
            cpu,
            memory: memory::Memory::new(),
            gpus: gpu::Gpus::new(),
            disks: disks::Disks::new(),
            net: net::Net::new(),
            sensors: sensors::Sensors::new(),
            sensors_at: None,
            power: power::Power::new(),
            battery: None,
            battery_at: None,
            procs: procs::Procs::new(threads),
            procs_warming: false,
            apps: None,
            apps_at: None,
            apps_problem: false,
            by_drive: None,
            by_drive_at: None,
            gpu_procs: None,
            gpu_procs_at: None,
            net_procs: None,
            net_procs_at: None,
            services: services::Services::new(),
            out: Json::new(),
        }
    }

    /// Collect and render one frame. Returns how soon the next frame should
    /// come when sooner than the interval (process warm-up).
    fn frame(&mut self, c: &Control) -> Option<Duration> {
        let mut sooner = None;
        let focus = c.focus.as_str();
        self.cpu.sample();
        self.memory.sample();
        self.disks.sample();
        self.net.sample(c.detail > 0);
        self.gpus.sample(&mut self.gate);
        self.power.sample();
        if older(self.sensors_at, if c.detail > 0 { 0.9 } else { 1.9 }) {
            self.sensors.sample(&mut self.gate);
            self.sensors_at = Some(Instant::now());
        }
        if older(self.battery_at, 1.9) {
            self.battery = battery::read(Path::new("/sys/class/power_supply"));
            self.battery_at = Some(Instant::now());
        }

        // Processes, only while a panel is open.
        let want_procs = c.detail >= 1 || focus == "apps";
        let mut procs_ready = false;
        if want_procs {
            if older(self.procs.scanned_at, 3.0) {
                self.procs.scan();
                self.procs_warming = true;
                sooner = Some(Duration::from_millis(250));
            } else if self.procs_warming || older(self.procs.scanned_at, 0.95) {
                self.procs.scan();
                self.procs_warming = false;
            }
            procs_ready = !self.procs_warming;
        }

        // Page extras.
        if focus == "apps" && procs_ready {
            if c.refresh || older(self.apps_at, 1.9) {
                self.apps = hypr::apps(&self.procs);
                self.apps_problem = self.apps.is_none();
                self.apps_at = Some(Instant::now());
            }
        } else if focus != "apps" {
            self.apps = None;
            self.apps_at = None;
            self.apps_problem = false;
        }
        if focus == "disks" && procs_ready {
            if older(self.by_drive_at, 1.9) {
                self.by_drive = Some(self.procs.by_drive(&self.disks.mount_keys));
                self.by_drive_at = Some(Instant::now());
            }
        } else if focus != "disks" {
            self.by_drive = None;
            self.by_drive_at = None;
        }
        if focus == "gpu" {
            if older(self.gpu_procs_at, 0.95) {
                let integrated: HashMap<String, bool> = self.gpus.ids().into_iter().collect();
                if self.procs.scanned_at.is_none() {
                    self.procs.scan();
                }
                let usage = self.procs.gpu_usage(&integrated);
                // The first reading has no busy time to compare against.
                self.gpu_procs = if self.gpu_procs_at.is_some() { Some(usage) } else { None };
                if self.gpu_procs_at.is_none() {
                    sooner = Some(Duration::from_millis(250));
                }
                self.gpu_procs_at = Some(Instant::now());
            }
        } else {
            self.gpu_procs = None;
            self.gpu_procs_at = None;
        }
        if focus == "network" && procs_ready {
            if older(self.net_procs_at, 0.95) {
                self.net_procs = self.procs.net_usage();
                self.net_procs_at = Some(Instant::now());
            }
        } else if focus != "network" {
            self.net_procs = None;
            self.net_procs_at = None;
        }
        self.services.set_wanted(focus == "services");
        if c.refresh && focus == "services" {
            self.services.refresh();
        }

        self.seq += 1;
        let watts = self.power.package().or_else(|| self.gpus.apu_watts());
        let (nv_temps, nv_fans) = self.gpus.nvidia_sensors();
        let j = &mut self.out;
        j.clear();
        j.begin_obj();
        j.uint("v", 1);
        j.uint("seq", self.seq);
        j.num("interval", c.interval, 2);
        j.arr("problems");
        if self.apps_problem && focus == "apps" {
            j.item_str("Hyprland did not answer, so open apps cannot be listed.");
        }
        j.end_arr();
        self.cpu.write(j, watts);
        self.memory.write(j);
        self.gpus.write(j, self.gpu_procs.as_ref());
        self.disks.write(j);
        self.net.write_open(j);
        if focus == "network" {
            procs::write_net_rows(j, &self.net_procs);
        }
        j.end_obj();
        self.sensors.write(j, &nv_temps, &nv_fans);
        self.power.write(j);
        battery::write(j, &self.battery);
        if want_procs && procs_ready && c.detail >= 1 {
            self.procs.write(j, c.detail >= 2, self.by_drive.as_ref());
        } else {
            j.null("procs");
        }
        match &self.apps {
            Some(a) if focus == "apps" => j.raw("apps", a),
            _ => j.null("apps"),
        }
        match self.services.latest() {
            Some(s) if focus == "services" => j.raw("services", &s),
            _ => j.null("services"),
        }
        j.end_obj();
        sooner
    }
}

fn stream(interval: f64) -> i32 {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 5);
    }
    let shared: Shared = Arc::new((
        Mutex::new(Control {
            interval,
            detail: 0,
            focus: String::new(),
            public_ip: false,
            refresh: false,
            quit: false,
            wake: false,
        }),
        Condvar::new(),
    ));
    let reader = shared.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        let mut lock = stdin.lock();
        loop {
            line.clear();
            // Commands are short; a runaway line is cut off.
            let n = match (&mut lock).take(512).read_line(&mut line) {
                Ok(n) => n,
                Err(_) => 0,
            };
            let (m, cv) = &*reader;
            let mut c = m.lock().unwrap_or_else(|e| e.into_inner());
            if n == 0 {
                // The shell went away: stop with it.
                c.quit = true;
                c.wake = true;
                cv.notify_all();
                return;
            }
            apply(line.trim(), &mut c);
            cv.notify_all();
        }
    });

    let mut collector = Collector::new();
    let stdout = std::io::stdout();
    loop {
        let started = Instant::now();
        let control = {
            let (m, _) = &*shared;
            let mut c = m.lock().unwrap_or_else(|e| e.into_inner());
            let snapshot = c.clone();
            c.public_ip = false;
            c.refresh = false;
            c.wake = false;
            snapshot
        };
        if control.quit {
            return 0;
        }
        if control.public_ip {
            collector.net.lookup_public_ip();
        }
        let sooner = collector.frame(&control);
        {
            let mut out = stdout.lock();
            if out.write_all(collector.out.out.as_bytes()).and_then(|_| out.write_all(b"\n")).and_then(|_| out.flush()).is_err() {
                return 0;
            }
        }
        let mut wait = Duration::from_secs_f64(control.interval);
        if let Some(s) = sooner {
            wait = wait.min(s);
        }
        let deadline = started + wait;
        let (m, cv) = &*shared;
        let mut c = m.lock().unwrap_or_else(|e| e.into_inner());
        while !c.wake {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            c = cv.wait_timeout(c, deadline - now).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

fn once() -> i32 {
    let mut collector = Collector::new();
    let control = Control {
        interval: 1.0,
        detail: 2,
        focus: String::new(),
        public_ip: false,
        refresh: false,
        quit: false,
        wake: false,
    };
    collector.frame(&control);
    std::thread::sleep(Duration::from_millis(300));
    collector.frame(&control);
    std::thread::sleep(Duration::from_secs(1));
    collector.frame(&control);
    println!("{}", collector.out.out);
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let code = match a.as_slice() {
        ["once"] => once(),
        ["summon", addr] => hypr::summon(addr),
        ["close", pid] => hypr::close(pid),
        ["unit", scope, action, unit] => services::act(scope, action, unit),
        [] => stream(1.0),
        ["--interval", s] => match s.parse::<f64>() {
            Ok(v) if v.is_finite() => stream(v.clamp(0.1, 30.0)),
            _ => usage(),
        },
        _ => usage(),
    };
    std::process::exit(code);
}

fn usage() -> i32 {
    eprintln!(
        "usage: cornerstone-stats-collector [--interval SECONDS] | once | summon ADDRESS | close PID | unit user|system start|stop|restart NAME.service"
    );
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        let mut c = Control { interval: 1.0, detail: 0, focus: String::new(), public_ip: false, refresh: false, quit: false, wake: false };
        apply("detail 2", &mut c);
        apply("focus gpu", &mut c);
        apply("interval 0.01", &mut c);
        assert_eq!((c.detail, c.focus.as_str(), c.interval), (2, "gpu", 0.1));
        apply("focus nonsense", &mut c);
        assert_eq!(c.focus, "");
        apply("interval NaN", &mut c);
        assert_eq!(c.interval, 0.1);
        apply("publicip", &mut c);
        apply("bogus 1", &mut c);
        assert!(c.public_ip && !c.quit);
    }
}

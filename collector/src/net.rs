//! Network interfaces, their traffic and addresses, and the public address.

use crate::json::{bounded, Json};
use crate::sys::{self, Probe};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// /proc/net/dev: name -> (rx bytes, tx bytes).
pub fn parse_net_dev(text: &str) -> Vec<(&str, u64, u64)> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        let f: Vec<u64> = rest.split_ascii_whitespace().map(|v| v.parse().unwrap_or(0)).collect();
        if f.len() >= 9 {
            out.push((name.trim(), f[0], f[8]));
        }
    }
    out
}

/// The interface of the default route with the lowest metric (IPv4 table).
pub fn parse_default_route(text: &str) -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        if f.len() < 8 || f[1] != "00000000" || f[7] != "00000000" {
            continue;
        }
        let metric = f[6].parse().unwrap_or(u64::MAX);
        if best.as_ref().map_or(true, |(m, _)| metric < *m) {
            best = Some((metric, f[0].to_string()));
        }
    }
    best.map(|b| b.1)
}

/// IPv6 default route: destination ::/0, interface in the last column.
pub fn parse_default_route6(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        (f.len() >= 10 && f[0].chars().all(|c| c == '0') && f[1] == "00" && f[9] != "lo").then(|| f[9].to_string())
    })
}

/// /proc/net/wireless: name -> signal level in dBm.
pub fn parse_wireless(text: &str) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for line in text.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        let f: Vec<&str> = rest.split_ascii_whitespace().collect();
        if let Some(level) = f.get(2).and_then(|v| v.trim_end_matches('.').parse::<f64>().ok()) {
            if level < 0.0 {
                out.insert(name.trim().to_string(), level);
            }
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Ethernet,
    Wifi,
    Vpn,
    Other,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Ethernet => "ethernet",
            Kind::Wifi => "wifi",
            Kind::Vpn => "vpn",
            Kind::Other => "other",
        }
    }
}

fn kind_of(name: &str) -> Option<Kind> {
    let base = format!("/sys/class/net/{name}");
    if name == "lo" {
        return None;
    }
    if sys::exists(format!("{base}/wireless")) || sys::exists(format!("{base}/phy80211")) {
        return Some(Kind::Wifi);
    }
    let uevent = sys::read_text(format!("{base}/uevent")).unwrap_or_default();
    let vpn_name = ["tun", "tap", "wg", "tailscale", "proton", "nord", "mullvad", "vpn", "zt"].iter().any(|p| name.starts_with(p));
    if sys::exists(format!("{base}/tun_flags")) || uevent.contains("DEVTYPE=wireguard") || vpn_name {
        return Some(Kind::Vpn);
    }
    if sys::exists(format!("{base}/device")) {
        return Some(if sys::read_text(format!("{base}/type")).as_deref() == Some("1") { Kind::Ethernet } else { Kind::Other });
    }
    // Bridges, veth pairs, docker and libvirt networks: not interesting.
    None
}

struct Iface {
    name: String,
    kind: Kind,
    up: bool,
    speed: Option<f64>,
    prev: Option<(u64, u64)>,
    rx: f64,
    tx: f64,
    rx_total: u64,
    tx_total: u64,
    ipv4: Vec<String>,
    ipv6: Vec<String>,
    ssid: Option<String>,
    bitrate: Option<f64>,
    signal: Option<f64>,
}

#[derive(Default)]
struct Public {
    address: String,
    busy: bool,
}

pub struct Net {
    dev: Probe,
    wireless: Probe,
    ifaces: Vec<Iface>,
    prev_at: Option<Instant>,
    slow_at: Option<Instant>,
    wifi_at: Option<Instant>,
    default_iface: Option<String>,
    rx: f64,
    tx: f64,
    public: Arc<Mutex<Public>>,
}

fn addresses() -> HashMap<String, (Vec<String>, Vec<String>)> {
    let mut out: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return out;
        }
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
            match (*ifa.ifa_addr).sa_family as i32 {
                libc::AF_INET => {
                    let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                    out.entry(name).or_default().0.push(ip.to_string());
                }
                libc::AF_INET6 => {
                    let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                    if (ip.segments()[0] & 0xffc0) != 0xfe80 {
                        out.entry(name).or_default().1.push(ip.to_string());
                    }
                }
                _ => {}
            }
        }
        libc::freeifaddrs(head);
    }
    out
}

impl Net {
    pub fn new() -> Self {
        let mut n = Net {
            dev: Probe::new("/proc/net/dev"),
            wireless: Probe::new("/proc/net/wireless"),
            ifaces: Vec::new(),
            prev_at: None,
            slow_at: None,
            wifi_at: None,
            default_iface: None,
            rx: 0.0,
            tx: 0.0,
            public: Arc::new(Mutex::new(Public::default())),
        };
        n.slow();
        n
    }

    /// Interface list, states, routes and addresses: every 5 s.
    fn slow(&mut self) {
        self.slow_at = Some(Instant::now());
        let names = sys::list_dir("/sys/class/net");
        let mut ifaces = Vec::new();
        for name in names {
            let Some(kind) = kind_of(&name) else { continue };
            let base = format!("/sys/class/net/{name}");
            let oper = sys::read_text(format!("{base}/operstate")).unwrap_or_default();
            let flags = sys::read_text(format!("{base}/flags"))
                .and_then(|f| u64::from_str_radix(f.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0);
            let carrier = sys::read_text(format!("{base}/carrier")).as_deref() == Some("1");
            let up = oper == "up" || ((oper == "unknown" || oper == "dormant") && flags & 1 == 1 && carrier);
            let speed = if up && kind == Kind::Ethernet {
                sys::read_f64(format!("{base}/speed")).filter(|s| *s > 0.0 && *s < 1e6)
            } else {
                None
            };
            let old = self.ifaces.iter().position(|i| i.name == name).map(|i| self.ifaces.swap_remove(i));
            let (prev, rx, tx, ssid, bitrate, signal) = match old {
                Some(o) => (o.prev, o.rx, o.tx, o.ssid, o.bitrate, o.signal),
                None => (None, 0.0, 0.0, None, None, None),
            };
            ifaces.push(Iface {
                name,
                kind,
                up,
                speed,
                prev,
                rx,
                tx,
                rx_total: 0,
                tx_total: 0,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                ssid: if up { ssid } else { None },
                bitrate: if up { bitrate } else { None },
                signal: if up { signal } else { None },
            });
        }
        let mut addrs = addresses();
        for i in &mut ifaces {
            if let Some((v4, v6)) = addrs.remove(&i.name) {
                i.ipv4 = v4;
                i.ipv6 = v6;
            }
        }
        self.ifaces = ifaces;
        self.default_iface = sys::read_text("/proc/net/route")
            .and_then(|t| parse_default_route(&t))
            .or_else(|| sys::read_text("/proc/net/ipv6_route").and_then(|t| parse_default_route6(&t)));
    }

    fn wifi_details(&mut self) {
        self.wifi_at = Some(Instant::now());
        let Some(iw) = sys::system_program("iw") else { return };
        for i in self.ifaces.iter_mut().filter(|i| i.kind == Kind::Wifi && i.up) {
            let out = sys::run(&iw, &["dev", &i.name, "link"], Duration::from_secs(2));
            for line in out.stdout.lines().map(str::trim) {
                if let Some(s) = line.strip_prefix("SSID:") {
                    i.ssid = Some(bounded(s.trim(), 64).to_string());
                } else if let Some(r) = line.strip_prefix("tx bitrate:") {
                    i.bitrate = r.split_ascii_whitespace().next().and_then(|v| v.parse().ok());
                }
            }
        }
    }

    pub fn sample(&mut self, detail: bool) {
        let now = Instant::now();
        if self.slow_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(5)) {
            self.slow();
        }
        let wifi_up = self.ifaces.iter().any(|i| i.kind == Kind::Wifi && i.up);
        if wifi_up && self.wifi_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(if detail { 10 } else { 30 })) {
            self.wifi_details();
        }
        if wifi_up {
            if let Some(text) = self.wireless.read() {
                let levels = parse_wireless(text);
                for i in self.ifaces.iter_mut().filter(|i| i.kind == Kind::Wifi) {
                    i.signal = levels.get(&i.name).copied();
                }
            }
        }
        let seconds = self.prev_at.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
        self.prev_at = Some(now);
        let Some(text) = self.dev.read() else { return };
        let (mut rx, mut tx) = (0.0, 0.0);
        for (name, r, t) in parse_net_dev(text) {
            let Some(i) = self.ifaces.iter_mut().find(|i| i.name == name) else { continue };
            if let Some((pr, pt)) = i.prev {
                if seconds > 0.0 {
                    i.rx = r.saturating_sub(pr) as f64 / seconds;
                    i.tx = t.saturating_sub(pt) as f64 / seconds;
                }
            }
            i.prev = Some((r, t));
            i.rx_total = r;
            i.tx_total = t;
            // A VPN's traffic also crosses the real link: count it once.
            if i.kind != Kind::Vpn {
                rx += i.rx;
                tx += i.tx;
            }
        }
        self.rx = rx;
        self.tx = tx;
    }

    pub fn lookup_public_ip(&self) {
        {
            let Ok(mut p) = self.public.lock() else { return };
            if p.busy {
                return;
            }
            p.busy = true;
        }
        let shared = self.public.clone();
        std::thread::spawn(move || {
            let mut found = String::new();
            if let Some(curl) = sys::system_program("curl") {
                for url in ["https://api.ipify.org", "https://icanhazip.com", "https://ifconfig.me/ip"] {
                    let out = sys::run(&curl, &["-s", "--max-time", "5", url], Duration::from_secs(7));
                    let candidate = out.stdout.trim();
                    if !candidate.is_empty() && candidate.len() <= 45 && candidate.parse::<std::net::IpAddr>().is_ok() {
                        found = candidate.to_string();
                        break;
                    }
                }
            }
            if let Ok(mut p) = shared.lock() {
                p.busy = false;
                if !found.is_empty() {
                    p.address = found;
                }
            }
        });
    }

    /// Writes "network" without closing it, so per-process traffic can follow.
    pub fn write_open(&self, j: &mut Json) {
        j.obj("network");
        j.num("rx", self.rx, 0);
        j.num("tx", self.tx, 0);
        j.bool("online", self.default_iface.is_some());
        j.str("defaultIface", self.default_iface.as_deref().unwrap_or(""));
        let public = self.public.lock().map(|p| p.address.clone()).unwrap_or_default();
        j.str("publicIp", &public);
        j.arr("ifaces");
        for i in &self.ifaces {
            j.begin_obj();
            j.str("name", &i.name);
            j.bool("up", i.up);
            j.bool("isDefault", self.default_iface.as_deref() == Some(i.name.as_str()));
            j.str("kind", i.kind.as_str());
            j.opt_str("ssid", i.ssid.as_deref());
            j.opt("signal", i.signal, 0);
            j.opt("bitrate", i.bitrate, 0);
            j.opt("speed", i.speed, 0);
            j.arr("ipv4");
            for a in &i.ipv4 {
                j.item_str(a);
            }
            j.end_arr();
            j.arr("ipv6");
            for a in i.ipv6.iter().take(4) {
                j.item_str(a);
            }
            j.end_arr();
            j.num("rx", i.rx, 0);
            j.num("tx", i.tx, 0);
            j.uint("rxTotal", i.rx_total);
            j.uint("txTotal", i.tx_total);
            j.end_obj();
        }
        j.end_arr();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_dev_columns() {
        let t = "Inter-|   Receive\n face |bytes\n    lo: 10 1 0 0 0 0 0 0 10 1 0 0 0 0 0 0\nenp6s0: 1000 5 0 0 0 0 0 0 2000 7 0 0 0 0 0 0\n";
        assert_eq!(parse_net_dev(t), vec![("lo", 10, 10), ("enp6s0", 1000, 2000)]);
    }

    #[test]
    fn default_routes() {
        let t = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\nwlp5s0\t00000000\t0100A8C0\t0003\t0\t0\t600\t00000000\nenp6s0\t00000000\t0100A8C0\t0003\t0\t0\t100\t00000000\nenp6s0\t0000A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\n";
        assert_eq!(parse_default_route(t).as_deref(), Some("enp6s0"));
        let v6 = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 enp6s0\n";
        assert_eq!(parse_default_route6(v6).as_deref(), Some("enp6s0"));
    }

    #[test]
    fn wireless_levels() {
        let t = "Inter-| sta-|   Quality\n face | tus | link level noise\nwlp5s0: 0000   58.  -52.  -256        0      0      0      0      0        0\n";
        assert_eq!(parse_wireless(t).get("wlp5s0"), Some(&-52.0));
    }
}

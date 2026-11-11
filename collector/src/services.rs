//! systemd services for the Services tab: listed on a background thread
//! while that tab is open, and started, stopped or restarted on request.

use crate::json::{bounded, Json};
use crate::sys;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub fn valid_unit(unit: &str) -> bool {
    let Some(stem) = unit.strip_suffix(".service") else { return false };
    !stem.is_empty() && unit.len() <= 208 && stem.chars().all(|c| c.is_ascii_alphanumeric() || "@._:\\-".contains(c))
}

/// `systemctl show` output: blank-line separated blocks of key=value.
pub fn parse_show(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut cur: HashMap<String, String> = HashMap::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let Some(id) = cur.get("Id").cloned() {
                out.insert(id, std::mem::take(&mut cur));
            }
            cur.clear();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            cur.insert(k.to_string(), v.to_string());
        }
    }
    out
}

fn list(scope: &str, home: &str, j: &mut Json) {
    let Some(systemctl) = sys::system_program("systemctl") else { return };
    let user = scope == "user";
    let mut args: Vec<&str> = Vec::new();
    if user {
        args.push("--user");
    }
    let mut list_args = args.clone();
    list_args.extend(["list-units", "--type=service", "--all", "--no-pager", "-o", "json"]);
    let out = sys::run(&systemctl, &list_args, Duration::from_secs(6));
    let units: Vec<Value> = serde_json::from_str(&out.stdout).unwrap_or_default();
    let units: Vec<&Value> = units
        .iter()
        .filter(|u| u["unit"].as_str().is_some_and(valid_unit) && u["load"].as_str() != Some("not-found"))
        .take(800)
        .collect();
    let names: Vec<&str> = units.iter().filter_map(|u| u["unit"].as_str()).collect();
    let mut info = HashMap::new();
    if !names.is_empty() {
        let mut show_args = args.clone();
        show_args.extend(["show", "-p", "Id,MemoryCurrent,FragmentPath,UnitFileState,NRestarts", "--"]);
        show_args.extend(names.iter().copied());
        info = parse_show(&sys::run(&systemctl, &show_args, Duration::from_secs(8)).stdout);
    }
    let mine_dir = format!("{home}/.config/systemd/user/");
    for u in units {
        let unit = u["unit"].as_str().unwrap_or("");
        let i = info.get(unit);
        let get = |k: &str| i.and_then(|m| m.get(k)).map(|s| s.as_str()).unwrap_or("");
        j.begin_obj();
        j.str("unit", unit);
        j.str("name", unit.trim_end_matches(".service"));
        j.str("description", bounded(u["description"].as_str().unwrap_or(""), 120));
        j.str("active", bounded(u["active"].as_str().unwrap_or(""), 20));
        j.str("sub", bounded(u["sub"].as_str().unwrap_or(""), 20));
        j.str("enabled", bounded(get("UnitFileState"), 20));
        j.uint("mem", get("MemoryCurrent").parse::<u64>().ok().filter(|m| *m < u64::MAX / 2).unwrap_or(0));
        j.uint("restarts", get("NRestarts").parse().unwrap_or(0));
        j.bool("mine", user && get("FragmentPath").starts_with(&mine_dir));
        j.str("scope", scope);
        j.end_obj();
    }
}

pub fn snapshot(home: &str) -> String {
    let mut j = Json::new();
    j.begin_obj();
    j.arr("user");
    list("user", home, &mut j);
    j.end_arr();
    j.arr("system");
    list("system", home, &mut j);
    j.end_arr();
    j.end_obj();
    j.out
}

#[derive(Default)]
struct State {
    wanted: bool,
    poke: bool,
    latest: Option<String>,
}

/// The listing thread. It runs while `set_wanted(true)`, every 3 s or at once
/// after `refresh()`, and never blocks the frames.
pub struct Services {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl Services {
    pub fn new() -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        let home = std::env::var("HOME").unwrap_or_default();
        std::thread::spawn(move || {
            let (lock, cv) = &*worker;
            loop {
                {
                    let mut st = lock.lock().unwrap_or_else(|e| e.into_inner());
                    while !st.wanted {
                        st = cv.wait(st).unwrap_or_else(|e| e.into_inner());
                    }
                    st.poke = false;
                }
                let text = snapshot(&home);
                let mut st = lock.lock().unwrap_or_else(|e| e.into_inner());
                st.latest = Some(text);
                if !st.poke {
                    let (s, _) = cv.wait_timeout(st, Duration::from_secs(3)).unwrap_or_else(|e| e.into_inner());
                    drop(s);
                }
            }
        });
        Services { shared }
    }

    pub fn set_wanted(&self, wanted: bool) {
        let (lock, cv) = &*self.shared;
        let mut st = lock.lock().unwrap_or_else(|e| e.into_inner());
        if st.wanted != wanted {
            st.wanted = wanted;
            if !wanted {
                st.latest = None;
            }
            cv.notify_all();
        }
    }

    pub fn refresh(&self) {
        let (lock, cv) = &*self.shared;
        let mut st = lock.lock().unwrap_or_else(|e| e.into_inner());
        st.poke = true;
        cv.notify_all();
    }

    pub fn latest(&self) -> Option<String> {
        self.shared.0.lock().ok()?.latest.clone()
    }
}

/// `unit SCOPE ACTION UNIT`: run it and tell the user when it fails.
pub fn act(scope: &str, action: &str, unit: &str) -> i32 {
    if !matches!(scope, "user" | "system") || !matches!(action, "start" | "stop" | "restart") || !valid_unit(unit) {
        eprintln!("unit: expected user|system start|stop|restart NAME.service");
        return 2;
    }
    let out = if scope == "user" {
        match sys::system_program("systemctl") {
            Some(p) => sys::run(&p, &["--user", action, "--", unit], Duration::from_secs(90)),
            None => return 1,
        }
    } else {
        match sys::system_program("pkexec") {
            Some(p) => sys::run(&p, &["systemctl", action, "--", unit], Duration::from_secs(120)),
            None => return 1,
        }
    };
    if out.ok {
        return 0;
    }
    let reason = out.stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("It failed.").trim().to_string();
    if let Some(notify) = sys::system_program("notify-send") {
        let title = format!("Couldn't {action} {}", unit.trim_end_matches(".service"));
        let _ = sys::run(Path::new(&notify), &["-a", "Cornerstone Stats", &title, bounded(&reason, 200)], Duration::from_secs(5));
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_names() {
        assert!(valid_unit("cornerstone-stt.service"));
        assert!(valid_unit("omarchy-time-machine@local-home.service"));
        assert!(valid_unit("systemd-fsck@dev-disk-by\\x2duuid-1234.service"));
        assert!(!valid_unit("foo.timer"));
        assert!(!valid_unit(".service"));
        assert!(!valid_unit("a b.service"));
        assert!(!valid_unit("x;rm.service"));
    }

    #[test]
    fn show_blocks() {
        let t = "Id=a.service\nMemoryCurrent=1024\nNRestarts=2\n\nId=b.service\nMemoryCurrent=[not set]\n";
        let m = parse_show(t);
        assert_eq!(m["a.service"]["MemoryCurrent"], "1024");
        assert_eq!(m["b.service"]["MemoryCurrent"], "[not set]");
    }

    #[test]
    fn bad_arguments_are_refused() {
        assert_eq!(act("user", "enable", "x.service"), 2);
        assert_eq!(act("root", "start", "x.service"), 2);
        assert_eq!(act("user", "start", "x;y.service"), 2);
    }
}

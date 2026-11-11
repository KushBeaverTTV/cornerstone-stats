//! Hyprland: open windows for the Apps tab, and the window actions behind
//! its buttons. Talks to Hyprland's request socket directly, the way hyprctl
//! does, without starting a process.

use crate::json::{bounded, Json};
use crate::procs::Procs;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// The request socket: from the environment, or when the shell started us
/// with a cleared environment, the one live Hyprland instance of this user.
fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", crate::sys::my_uid())));
    if let Some(sig) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") {
        let p = runtime.join("hypr").join(sig).join(".socket.sock");
        if p.exists() {
            return Some(p);
        }
    }
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = crate::sys::list_dir(runtime.join("hypr"))
        .into_iter()
        .map(|d| runtime.join("hypr").join(d).join(".socket.sock"))
        .filter(|p| UnixStream::connect(p).is_ok())
        .filter_map(|p| Some((std::fs::metadata(p.parent()?).ok()?.modified().ok()?, p)))
        .collect();
    candidates.sort();
    candidates.pop().map(|c| c.1)
}

pub fn request(command: &str) -> Option<String> {
    let mut s = UnixStream::connect(socket_path()?).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    s.write_all(command.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn json(command: &str) -> Option<Value> {
    serde_json::from_str(&request(command)?).ok()
}

fn dispatch(expr: &str) -> bool {
    request(&format!("dispatch {expr}")).is_some_and(|r| r.trim() == "ok")
}

pub fn valid_address(a: &str) -> bool {
    a.len() > 2 && a.len() <= 18 && a.starts_with("0x") && a[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// A window class as an anchored regex that matches only itself.
pub fn class_regex(class: &str) -> String {
    let mut out = String::from("^");
    for c in class.chars() {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('$');
    out
}

/// A Lua string literal for the dispatcher expression.
fn lua_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn active_address() -> String {
    json("j/activewindow").and_then(|v| v["address"].as_str().map(String::from)).unwrap_or_default()
}

/// Bring a window to the current workspace and focus it. Omarchy's floating
/// workspaces ignore focus by address, so when focus did not move, focus by
/// the window's exact class instead.
pub fn summon(address: &str) -> i32 {
    if !valid_address(address) {
        eprintln!("summon: not a window address");
        return 2;
    }
    let target = lua_string(&format!("address:{address}"));
    if let Some(ws) = json("j/activeworkspace").and_then(|v| v["id"].as_i64()) {
        dispatch(&format!("hl.dsp.window.move({{ workspace = \"{ws}\", follow = false, window = {target} }})"));
    }
    dispatch(&format!("hl.dsp.focus({{ window = {target} }})"));
    std::thread::sleep(Duration::from_millis(60));
    if active_address() != address {
        let class = json("j/clients")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .find(|c| c["address"].as_str() == Some(address))
            .and_then(|c| c["class"].as_str().map(String::from))
            .unwrap_or_default();
        if !class.is_empty() {
            dispatch(&format!("hl.dsp.focus({{ window = {} }})", lua_string(&format!("class:{}", class_regex(&class)))));
        }
    }
    0
}

/// Ask every window of a process to close, as its close button would.
pub fn close(pid: &str) -> i32 {
    let Ok(pid) = pid.parse::<i64>() else {
        eprintln!("close: not a process id");
        return 2;
    };
    for c in json("j/clients").and_then(|v| v.as_array().cloned()).unwrap_or_default() {
        if c["pid"].as_i64() != Some(pid) {
            continue;
        }
        if let Some(addr) = c["address"].as_str().filter(|a| valid_address(a)) {
            dispatch(&format!("hl.dsp.window.close({{ window = {} }})", lua_string(&format!("address:{addr}"))));
        }
    }
    0
}

struct Window {
    address: String,
    class: String,
    title: String,
    workspace: String,
    hidden: bool,
    focused: bool,
}

/// Open windows grouped by owning process, as a pre-rendered JSON array.
pub fn apps(procs: &Procs) -> Option<String> {
    let clients = json("j/clients")?;
    let active = active_address();
    let mut by_pid: HashMap<u32, Vec<Window>> = HashMap::new();
    for c in clients.as_array()? {
        let Some(pid) = c["pid"].as_i64().filter(|p| *p > 1) else { continue };
        let workspace = c["workspace"]["name"].as_str().unwrap_or("").to_string();
        let address = c["address"].as_str().unwrap_or("").to_string();
        by_pid.entry(pid as u32).or_default().push(Window {
            focused: !address.is_empty() && address == active,
            hidden: workspace.starts_with("special") || c["hidden"].as_bool().unwrap_or(false),
            address,
            class: c["class"].as_str().unwrap_or("").to_string(),
            title: c["title"].as_str().unwrap_or("").to_string(),
            workspace,
        });
    }
    let children = procs.children();
    let index: HashMap<u32, usize> = procs.list.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    struct App {
        pid: u32,
        cpu: f64,
        mem: u64,
        count: usize,
        windows: Vec<Window>,
    }
    let mut apps: Vec<App> = Vec::new();
    for (pid, windows) in by_pid {
        if !index.contains_key(&pid) {
            continue;
        }
        let (mut cpu, mut mem, mut count) = (0.0, 0u64, 0usize);
        let mut stack = vec![pid];
        let mut guard = 0;
        while let Some(p) = stack.pop() {
            guard += 1;
            if guard > 20000 {
                break;
            }
            if let Some(&i) = index.get(&p) {
                cpu += procs.list[i].cpu;
                mem += procs.list[i].rss;
                count += 1;
            }
            if let Some(kids) = children.get(&p) {
                stack.extend(kids.iter().filter(|k| **k != p));
            }
        }
        apps.push(App { pid, cpu, mem, count, windows });
    }
    apps.sort_by(|a, b| b.mem.cmp(&a.mem));
    apps.truncate(60);

    let mut j = Json::new();
    j.begin_arr();
    for a in &apps {
        let p = &procs.list[index[&a.pid]];
        let exe = std::fs::read_link(format!("/proc/{}/exe", a.pid)).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
        let deleted = exe.ends_with(" (deleted)");
        let exe_name = exe.trim_end_matches(" (deleted)").rsplit('/').next().unwrap_or("").to_string();
        j.begin_obj();
        j.uint("pid", a.pid as u64);
        j.str("comm", bounded(&p.name, 64));
        j.str("exe", bounded(&exe_name, 64));
        j.bool("deleted", deleted);
        j.uint("procs", a.count as u64);
        j.num("cpu", a.cpu.min(100.0), 1);
        j.uint("mem", a.mem);
        j.bool("focused", a.windows.iter().any(|w| w.focused));
        j.arr("windows");
        for w in &a.windows {
            j.begin_obj();
            j.str("address", bounded(&w.address, 20));
            j.str("class", bounded(&w.class, 120));
            j.str("title", bounded(&w.title, 160));
            j.str("workspace", bounded(&w.workspace, 40));
            j.bool("hidden", w.hidden);
            j.bool("focused", w.focused);
            j.end_obj();
        }
        j.end_arr();
        j.end_obj();
    }
    j.end_arr();
    Some(j.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_and_classes() {
        assert!(valid_address("0x55d1c0a0b2c0"));
        assert!(!valid_address("0x"));
        assert!(!valid_address("55d1"));
        assert!(!valid_address("0x12; rm"));
        assert_eq!(class_regex("org.gnome.Nautilus"), "^org\\.gnome\\.Nautilus$");
        assert_eq!(lua_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}

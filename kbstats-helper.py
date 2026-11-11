#!/usr/bin/python3
"""Helper for the Stats plugin's Apps and Services tabs (kushbeaverttv fork).

  kbstats-helper.py apps                 open windows grouped by owning process,
                                         with CPU % and memory of each process tree
  kbstats-helper.py summon ADDRESS       bring that window to the current workspace
  kbstats-helper.py close PID            politely close every window of that process
  kbstats-helper.py services             user + system services as JSON
  kbstats-helper.py svc SCOPE ACTION UNIT   start|stop|restart (system goes through pkexec)
  kbstats-helper.py logs SCOPE UNIT      open the unit's log in a terminal

Reads /proc and calls hyprctl / systemctl only; prints one JSON line.
"""
import json
import os
import re
import subprocess
import sys
import time

CLK = os.sysconf("SC_CLK_TCK")
PAGE = os.sysconf("SC_PAGE_SIZE")
CACHE = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"), "kbstats", "apps-cpu.json")
UNIT_RE = re.compile(r"^[A-Za-z0-9@._:\\-]{1,200}\.service$")
ADDR_RE = re.compile(r"^0x[0-9a-fA-F]{1,16}$")


def run(cmd, timeout=4):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout
    except Exception:
        return ""


def hypr(cmd):
    try:
        return json.loads(run(["hyprctl", "-j"] + cmd) or "null")
    except Exception:
        return None


def proc_table():
    """pid -> (ppid, ticks, rss bytes, comm)"""
    table = {}
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            with open(f"/proc/{name}/stat") as f:
                stat = f.read()
            close = stat.rindex(")")
            comm = stat[stat.index("(") + 1:close]
            fields = stat[close + 2:].split()
            ppid = int(fields[1])
            ticks = int(fields[11]) + int(fields[12])
            rss = int(fields[21]) * PAGE
            table[int(name)] = (ppid, ticks, rss, comm)
        except Exception:
            continue
    return table


def descendants(root, children):
    out, stack = [], [root]
    while stack:
        p = stack.pop()
        out.append(p)
        stack.extend(children.get(p, ()))
    return out


def apps():
    clients = hypr(["clients"]) or []
    active = hypr(["activewindow"]) or {}
    table = proc_table()
    children = {}
    for pid, (ppid, *_rest) in table.items():
        children.setdefault(ppid, []).append(pid)
    now = time.time()
    try:
        with open(CACHE) as f:
            prev = json.load(f)
    except Exception:
        prev = {}
    groups = {}
    for c in clients:
        pid = c.get("pid")
        if not isinstance(pid, int) or pid <= 1 or pid not in table:
            continue
        g = groups.setdefault(pid, {"pid": pid, "windows": []})
        g["windows"].append({
            "address": str(c.get("address", ""))[:20],
            "class": str(c.get("class", ""))[:120],
            "title": str(c.get("title", ""))[:160],
            "workspace": str((c.get("workspace") or {}).get("name", ""))[:40],
            "hidden": str((c.get("workspace") or {}).get("name", "")).startswith("special"),
            "focused": c.get("address") == active.get("address"),
        })
    cache = {}
    out = []
    for pid, g in groups.items():
        tree = descendants(pid, children)
        ticks = sum(table[p][1] for p in tree if p in table)
        rss = sum(table[p][2] for p in tree if p in table)
        key = f"{pid}:{table[pid][3]}"
        cpu = 0.0
        if key in prev:
            dt = now - prev[key][1]
            if dt > 0.2:
                cpu = max(0.0, (ticks - prev[key][0]) / CLK / dt * 100 / (os.cpu_count() or 1))
        cache[key] = [ticks, now]
        exe = ""
        try:
            exe = os.path.basename(os.readlink(f"/proc/{pid}/exe"))
        except Exception:
            pass
        g.update({"comm": table[pid][3][:40], "exe": exe[:60], "procs": len(tree),
                  "cpu": round(cpu, 1), "mem": rss,
                  "focused": any(w["focused"] for w in g["windows"])})
        out.append(g)
    os.makedirs(os.path.dirname(CACHE), exist_ok=True)
    with open(CACHE + ".tmp", "w") as f:
        json.dump(cache, f)
    os.replace(CACHE + ".tmp", CACHE)
    out.sort(key=lambda g: -g["mem"])
    print(json.dumps({"apps": out[:60], "t": int(now)}))


def summon(addr):
    if not ADDR_RE.match(addr):
        return
    ws = (hypr(["activeworkspace"]) or {}).get("id")
    if ws is not None:
        run(["hyprctl", "dispatch", f'hl.dsp.window.move({{ workspace = "{ws}", follow = false, window = "address:{addr}" }})'])
    run(["hyprctl", "dispatch", f'hl.dsp.focus({{ window = "address:{addr}" }})'])
    # Omarchy's floating workspaces ignore focus by address; fall back to the
    # window's exact class when focus didn't move.
    if (hypr(["activewindow"]) or {}).get("address") != addr:
        cls = next((c.get("class", "") for c in hypr(["clients"]) or [] if c.get("address") == addr), "")
        if cls:
            run(["hyprctl", "dispatch", f'hl.dsp.focus({{ window = "class:^{re.escape(cls)}$" }})'])


def close(pid):
    if not pid.isdigit():
        return
    for c in hypr(["clients"]) or []:
        if str(c.get("pid")) == pid and ADDR_RE.match(str(c.get("address", ""))):
            run(["hyprctl", "dispatch", f'hl.dsp.window.close({{ window = "address:{c["address"]}" }})'])


def unit_list(scope):
    base = ["systemctl"] + (["--user"] if scope == "user" else [])
    try:
        units = json.loads(run(base + ["list-units", "--type=service", "--all", "-o", "json"]) or "[]")
    except Exception:
        units = []
    names = [u.get("unit", "") for u in units if UNIT_RE.match(u.get("unit", ""))]
    info = {}
    if names:
        text = run(base + ["show", "-p", "Id,MemoryCurrent,FragmentPath,UnitFileState,ActiveEnterTimestampMonotonic,NRestarts"] + names, timeout=8)
        cur = {}
        for line in text.splitlines() + [""]:
            if not line.strip():
                if "Id" in cur:
                    info[cur["Id"]] = cur
                cur = {}
                continue
            k, _, v = line.partition("=")
            cur[k] = v
    home = os.path.expanduser("~")
    out = []
    for u in units:
        name = u.get("unit", "")
        if not UNIT_RE.match(name):
            continue
        i = info.get(name, {})
        mem = i.get("MemoryCurrent", "")
        path = i.get("FragmentPath", "")
        if u.get("load") == "not-found":
            continue
        out.append({
            "unit": name,
            "name": name[:-8],
            "description": str(u.get("description", ""))[:120],
            "active": str(u.get("active", ""))[:20],
            "sub": str(u.get("sub", ""))[:20],
            "enabled": i.get("UnitFileState", "")[:20],
            "mem": int(mem) if mem.isdigit() else 0,
            "restarts": int(i.get("NRestarts", "0")) if i.get("NRestarts", "0").isdigit() else 0,
            "mine": scope == "user" and path.startswith(home + "/.config/systemd/user/"),
            "scope": scope,
        })
    return out


def services():
    print(json.dumps({"user": unit_list("user"), "system": unit_list("system"), "t": int(time.time())}))


def svc(scope, action, unit):
    if scope not in ("user", "system") or action not in ("start", "stop", "restart") or not UNIT_RE.match(unit):
        sys.exit(2)
    if scope == "user":
        cmd = ["systemctl", "--user", action, unit]
    else:
        cmd = ["pkexec", "systemctl", action, unit]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        msg = (r.stderr or "failed").strip().splitlines()[-1][:200] if (r.stderr or "").strip() else "failed"
        subprocess.run(["notify-send", "-a", "Stats", f"Couldn't {action} {unit[:-8]}", msg])


def logs(scope, unit):
    if scope not in ("user", "system") or not UNIT_RE.match(unit):
        sys.exit(2)
    j = "journalctl " + ("--user " if scope == "user" else "") + "-u " + unit + " -n 300 -f"
    subprocess.Popen(["omarchy-launch-tui", "sh", "-c", j], start_new_session=True,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[:1] == ["apps"]:
        apps()
    elif a[:1] == ["summon"] and len(a) == 2:
        summon(a[1])
    elif a[:1] == ["close"] and len(a) == 2:
        close(a[1])
    elif a[:1] == ["services"]:
        services()
    elif a[:1] == ["svc"] and len(a) == 4:
        svc(a[1], a[2], a[3])
    elif a[:1] == ["logs"] and len(a) == 3:
        logs(a[1], a[2])
    else:
        print(__doc__, file=sys.stderr)
        sys.exit(2)

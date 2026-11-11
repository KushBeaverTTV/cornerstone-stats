.pragma library
.import "Format.js" as Format

// Reading a collector frame: which GPU, disk or sensor a setting means, what
// a reading should be called, and what the numbers add up to. Field names are
// the ones in collector/PROTOCOL.md. Nothing here keeps state.

function list(value) { return Array.isArray(value) ? value : [] }

// ------------------------------------------------------------------ GPUs

function gpus(frame) { return list(frame && frame.gpus) }

// The cards that matter by default: the discrete ones, or the integrated one
// on a machine that has nothing else.
function mainGpus(frame) {
  var all = gpus(frame)
  var out = all.filter(function(g) { return !g.integrated })
  return out.length > 0 ? out : all
}

function gpuById(frame, id) {
  var all = gpus(frame)
  for (var i = 0; i < all.length; i++) if (String(all[i].id) === String(id)) return all[i]
  return null
}

// barGpus: "discrete" (or empty), "all", "none", or PCI addresses. Addresses
// this machine lacks (a shell.json copied from another computer) fall back
// to the main cards.
function barGpus(frame, raw) {
  var text = String(raw === undefined || raw === null ? "" : raw).trim().toLowerCase()
  if (!text || text === "discrete") return mainGpus(frame)
  if (text === "all") return gpus(frame)
  if (text === "none") return []
  var out = []
  String(raw).split(/[\s,;]+/).forEach(function(id) {
    var g = gpuById(frame, id)
    if (g && out.indexOf(g) === -1) out.push(g)
  })
  return out.length > 0 ? out : mainGpus(frame)
}

// Three letters for the bar when several cards share it.
function gpuTag(gpu) {
  var v = gpu ? String(gpu.vendor || "") : ""
  return v === "nvidia" ? "NVD" : v === "amd" ? "AMD" : v === "intel" ? "INT" : "GPU"
}

// The card's name without the corporate padding: "AMD RX 7900 XT/...".
function gpuName(gpu) {
  var raw = String(gpu && gpu.name ? gpu.name : "")
  if (!raw) return gpuTag(gpu) === "GPU" ? "GPU" : gpuTag(gpu) + " GPU"
  if (/^(AMD\s+)?Radeon\s+Graphics$/i.test(raw)) return "AMD Radeon Graphics"
  return raw.replace(/^NVIDIA\s+GeForce\s+/i, "NVIDIA ")
    .replace(/^GeForce\s+/i, "NVIDIA ")
    .replace(/^(AMD\s+)?Radeon\s+/i, "AMD ")
    .replace(/^Intel\s+Corporation\s+/i, "Intel ")
}

function gpuHasLoad(gpu) { return !!gpu && Format.known(gpu.load) }

function vramPercent(gpu) {
  if (!gpu || !Format.known(gpu.vramUsed) || !(Number(gpu.vramTotal) > 0)) return null
  return Math.min(100, Number(gpu.vramUsed) / Number(gpu.vramTotal) * 100)
}

// ----------------------------------------------------------------- disks

function drives(frame) { return (frame && frame.disks && frame.disks.drives) || {} }
function volumes(frame) { return list(frame && frame.disks && frame.disks.volumes) }

// A saved disk names a drive by its by-id name (stable) or, from an older
// entry, its kernel name (nvme0n1 can change at boot). Returns the kernel
// name, "all", or "" when the drive is not attached.
function resolveDrive(frame, saved) {
  var s = String(saved || "")
  if (!s || s === "all") return "all"
  var d = drives(frame)
  for (var name in d) if (d[name] && String(d[name].id) === s) return name
  return d[s] ? s : ""
}

// The value to save for a drive: its by-id name when it has one.
function driveKey(frame, name) {
  var d = drives(frame)[name]
  return d && d.id ? String(d.id) : String(name || "")
}

function driveChoices(frame) {
  var out = [{ value: "all", label: "All disks" }]
  var d = drives(frame)
  Object.keys(d).sort().forEach(function(name) {
    var model = d[name] && d[name].model ? "  " + String(d[name].model).slice(0, 24) : ""
    out.push({ value: driveKey(frame, name), label: name + model })
  })
  return out
}

// The bar's disk readouts on this machine. When none of the saved drives is
// attached (a copied shell.json), one readout covers every disk.
function barDisksHere(frame, wanted) {
  var known = Object.keys(drives(frame)).length > 0
  var out = []
  for (var i = 0; i < wanted.length; i++) {
    var name = resolveDrive(frame, wanted[i].disk)
    if (name === "" && known) continue
    out.push({ disk: name || wanted[i].disk, show: wanted[i].show })
  }
  if (out.length === 0 && wanted.length > 0 && known) out.push({ disk: "all", show: wanted[0].show })
  return out
}

// Bar tag for one of several disk readouts: nvme1n1 -> NV1, sda -> SDA.
function driveTag(name) {
  var s = String(name || "")
  if (!s || s === "all") return "DSK"
  var m = s.match(/^nvme(\d+)n\d+$/)
  if (m) return "NV" + m[1]
  m = s.match(/^mmcblk(\d+)$/)
  if (m) return "MC" + m[1]
  return s.replace(/[^A-Za-z0-9]/g, "").slice(0, 3).toUpperCase() || "DSK"
}

// Space used on one drive, or on every local drive for "all": {used, size, fraction}.
function spaceUsed(frame, drive) {
  var used = 0, size = 0
  volumes(frame).forEach(function(v) {
    if (drive === "all" ? v.kind === "network" : v.drive !== drive) return
    used += Format.number(v.used)
    size += Format.number(v.size)
  })
  return { used: used, size: size, fraction: size > 0 ? Math.max(0, Math.min(1, used / size)) : 0 }
}

// Read and write rates for one drive or all of them.
function driveRates(frame, drive) {
  var d = drives(frame)[drive]
  var all = (frame && frame.disks) || {}
  return d ? { read: Format.number(d.read), write: Format.number(d.write) }
           : { read: Format.number(all.read), write: Format.number(all.write) }
}

function volumeName(mount) {
  var m = String(mount || "")
  if (m === "/") return "Root"
  if (m === "/home") return "Home"
  if (m === "/boot" || m === "/boot/efi" || m === "/efi") return "Boot"
  var parts = m.split("/")
  return parts[parts.length - 1] || m
}

var VOLUME_GROUPS = [
  { kind: "internal", title: "Internal" },
  { kind: "external", title: "External" },
  { kind: "network", title: "Network" }
]

function volumeGroups(all) {
  var out = []
  VOLUME_GROUPS.forEach(function(g) {
    var items = list(all).filter(function(v) { return (v.kind || "internal") === g.kind })
    if (items.length) out.push({ kind: g.kind, title: g.title, items: items })
  })
  return out
}

// How a drive copes right now: {level: "" | "warn" | "danger", text}. A drive
// busy nearly all the time while every task waits on I/O is what freezes a
// desktop; slow answers under load come first.
function driveStrain(drive, stallFull, rotational) {
  if (!drive) return { level: "", text: "" }
  var busy = Format.number(drive.busy)
  var full = Format.number(stallFull)
  if (busy >= 90 && full >= 20)
    return { level: "danger", text: "Holding up the system: busy " + Math.round(busy) + "% of the time while every task waits on it" }
  if (Format.known(drive.latency) && busy >= 50 && Number(drive.latency) >= (rotational ? 100 : 20))
    return { level: "warn", text: "Slow to answer: " + Format.latency(drive.latency) + " per request" }
  return { level: "", text: "" }
}

// What a volume check means, in words. The collector reports facts; the
// explanation and the fix live here.
function problem(check) {
  var c = check || {}
  if (c.id === "trim-blocked") return {
    title: "TRIM never reaches this drive",
    detail: "The encrypted volume " + (c.layer || "") + " was unlocked without allowing discards, so trimming reports success "
      + "but the SSD is never told which blocks are free. It slows down as it fills and wears faster."
      + (c.fix ? "" : " Unlock it with discards allowed (allow-discards in crypttab, or rd.luks.options=discard)."),
    action: "Allow TRIM"
  }
  if (c.id === "trim-off") return {
    title: "Free space is never trimmed",
    detail: "This SSD is mounted without discard and the weekly fstrim timer is off, so the drive is never told which blocks are free.",
    action: "Turn on weekly TRIM"
  }
  if (c.id === "fat-flush") {
    var flags = list(c.flags).join(", ") || "flush"
    return {
      title: "Every file written waits for the drive",
      detail: "Mounted with " + flags + ": closing each written file waits for the drive, about a tenth of a second on a USB stick, "
        + "so copying many small files crawls." + (c.fix ? " The fix mounts it without that option from now on; eject it before unplugging."
                                                         : " Mount it without " + flags + " to fix it."),
      action: "Mount without flush"
    }
  }
  return { title: String(c.id || "Check"), detail: "", action: "Fix" }
}

// Each fix runs in a terminal as a short sh script that says what it will do
// and asks first. Names arrive as positional arguments, never inside the script.
var FIXES = {
  "allow-discards": {
    args: 1,
    script: 'echo "Let TRIM pass through the encrypted volume $1 to the SSD."\n'
      + 'echo "Runs: sudo cryptsetup refresh --allow-discards --persistent $1"\n'
      + 'echo "The choice is stored in the LUKS header, so it lasts across reboots."\n'
      + 'ask || exit 1\n'
      + 'sudo cryptsetup refresh --allow-discards --persistent "$1" || exit 1\n'
      + 'echo "Trimming free space now:"\n'
      + 'sudo fstrim -av\n'
  },
  "enable-fstrim": {
    args: 0,
    script: 'echo "Trim every mounted SSD once a week."\n'
      + 'echo "Runs: sudo systemctl enable --now fstrim.timer"\n'
      + 'ask || exit 1\n'
      + 'sudo systemctl enable --now fstrim.timer\n'
  },
  "udisks-no-flush": {
    args: 2,
    script: 'conf=/etc/udisks2/mount_options.conf\n'
      + 'echo "Mount the FAT drive $2 without flush from now on, so small writes are fast."\n'
      + 'echo "Adds a section for it to $conf, then unmounts and mounts it again."\n'
      + 'echo "Let any copy to it finish first. Afterwards, always eject it before unplugging."\n'
      + 'ask || exit 1\n'
      + 'if ! grep -qsF "[/dev/disk/by-uuid/$1]" "$conf"; then\n'
      + '  printf \'\\n# Added by Cornerstone Stats: mount this FAT drive without flush.\\n[/dev/disk/by-uuid/%s]\\nvfat_defaults=uid=$UID,gid=$GID,shortname=mixed,utf8=1,showexec\\n\' "$1" | sudo tee -a "$conf" >/dev/null || exit 1\n'
      + 'fi\n'
      + 'udisksctl unmount -b "$2" && udisksctl mount -b "$2"\n'
  }
}

function plainName(v) {
  var s = String(v === undefined || v === null ? "" : v)
  return s.length > 0 && s.length <= 128 && /^[A-Za-z0-9._:\/-]+$/.test(s) && s.indexOf("..") === -1
}

// The argv a Fix button runs, or null when there is no safe fix.
function fixCommand(check) {
  var c = check || {}
  var fix = FIXES[String(c.fix || "")]
  var args = list(c.args)
  if (!fix || args.length !== fix.args || !args.every(plainName)) return null
  var script = 'ask() { printf "Go ahead? [y/N] "; read -r reply; [ "$reply" = y ] || [ "$reply" = Y ]; }\n'
    + "(\n" + fix.script + ")\n"
    + 'result=$?\necho\n[ "$result" -eq 0 ] && echo "Done." || echo "Nothing changed, or it failed (status $result)."\n'
    + 'printf "Press Enter to close. "; read -r _\n'
  return ["xdg-terminal-exec", "sh", "-c", script, "cornerstone-stats-fix"].concat(args.map(String))
}

// Checks on the volumes of one drive (by volume key), or on all of them.
function checksFor(all, key) {
  var out = []
  list(all).forEach(function(v) {
    if (key && key !== "all" && v.key !== key) return
    list(v.checks).forEach(function(c) { out.push({ check: c, volume: v }) })
  })
  return out
}

// --------------------------------------------------------------- sensors

function temps(frame) { return list(frame && frame.sensors && frame.sensors.temps) }
function fans(frame) { return list(frame && frame.sensors && frame.sensors.fans) }

var GROUP_ORDER = ["CPU", "GPU", "NVMe", "Drive", "Memory", "Board", "Ethernet", "Wi-Fi", "ACPI"]

function groupRank(group) {
  var i = GROUP_ORDER.indexOf(String(group || "").replace(/\s\d+$/, ""))
  return i < 0 ? GROUP_ORDER.length : i
}

// "CPU Tctl", "GPU edge", "NVMe Composite"; a label that already names its
// chip, or a board sensor, stands alone.
function sensorName(s) {
  var group = String(s.group || ""), label = String(s.label || "")
  if (!label || label === group) return group || "Sensor"
  if (!group || group === "Board" || label.indexOf(group) === 0) return label
  return group + " " + label
}

function sortedTemps(frame) {
  return temps(frame).slice().sort(function(a, b) {
    var r = groupRank(a.group) - groupRank(b.group)
    return r !== 0 ? r : sensorName(a).localeCompare(sensorName(b))
  })
}

function fanOn(fan) { return !!fan && (Format.number(fan.rpm) > 0 || Format.number(fan.percent) > 0) }

// {value, unit}: "1200" rpm, "30" %, or "Off".
function fanReading(fan) {
  if (fan && Format.known(fan.percent) && !Format.known(fan.rpm))
    return Number(fan.percent) > 0 ? { value: String(Math.round(fan.percent)), unit: "%" } : { value: "Off", unit: "" }
  var rpm = Format.number(fan ? fan.rpm : 0)
  return rpm > 0 ? { value: String(Math.round(rpm)), unit: "rpm" } : { value: "Off", unit: "" }
}

// How hard a fan spins, 0 to 1 (rpm against 2400 when no percent is known).
function fanLevel(fan) {
  if (fan && Format.known(fan.percent)) return Math.max(0, Math.min(1, Number(fan.percent) / 100))
  return Math.min(1, Format.number(fan ? fan.rpm : 0) / 2400)
}

// The average of the fans that spin, for the Fans dial.
function fanSummary(frame) {
  var all = fans(frame), on = 0, rpmCount = 0, sum = 0
  all.forEach(function(f) {
    if (!fanOn(f)) return
    on++
    if (Format.number(f.rpm) > 0) { rpmCount++; sum += Number(f.rpm) }
  })
  return { count: all.length, on: on, average: rpmCount > 0 ? sum / rpmCount : 0 }
}

// The card behind the plain "gpu" sensor: the first main card with a temperature.
function sensorGpu(frame) {
  var main = mainGpus(frame)
  for (var i = 0; i < main.length; i++) if (Format.known(main[i].temp)) return main[i]
  return null
}

// Everything a bar sensor readout can follow, for the Settings tab.
function sensorChoices(frame) {
  var out = []
  var cpu = frame && frame.cpu
  if (cpu && Format.known(cpu.temp)) out.push({ value: "cpu", label: "CPU temperature" })
  var all = gpus(frame)
  var primary = sensorGpu(frame)
  if (primary) out.push({ value: "gpu", label: all.length > 1 ? "GPU temperature (" + gpuName(primary) + ")" : "GPU temperature" })
  if (all.length > 1) all.forEach(function(g) {
    if (Format.known(g.temp)) out.push({ value: "gpu:" + g.id, label: gpuName(g) + " temperature" })
  })
  temps(frame).forEach(function(t) { out.push({ value: String(t.id), label: sensorName(t) }) })
  fans(frame).forEach(function(f) { out.push({ value: String(f.id), label: sensorName(f) }) })
  return out
}

// One bar reading: {short, glyph, label, text, kind, celsius} or null.
function sensorReading(frame, id, unit) {
  var key = String(id || "")
  var t = null
  if (key === "cpu") {
    var cpu = frame && frame.cpu
    if (cpu && Format.known(cpu.temp)) t = { short: "CPU", glyph: "󰻠", label: "CPU", celsius: cpu.temp }
  } else if (key === "gpu" || key.indexOf("gpu:") === 0) {
    var g = key === "gpu" ? sensorGpu(frame) : gpuById(frame, key.slice(4))
    if (g && Format.known(g.temp)) t = { short: key === "gpu" ? "GPU" : gpuTag(g), glyph: "󰢮", label: gpuName(g), celsius: g.temp }
  } else {
    temps(frame).forEach(function(s) {
      if (!t && String(s.id) === key) t = { short: "TMP", glyph: "󰔏", label: sensorName(s), celsius: s.celsius }
    })
    if (!t) {
      var f = null
      fans(frame).forEach(function(x) { if (!f && String(x.id) === key) f = x })
      if (f) {
        var r = fanReading(f)
        return { short: "FAN", glyph: "󰈐", label: sensorName(f), text: r.value, unit: r.unit, kind: "fan" }
      }
    }
  }
  if (!t) return null
  t.kind = "temp"
  t.text = Format.degrees(t.celsius, unit)
  t.unit = ""
  return t
}

// ------------------------------------------------------------------ power

var RAPL_ZONES = [
  { key: "core", label: "CPU cores" },
  { key: "uncore", label: "CPU graphics" },
  { key: "dram", label: "Memory" },
  { key: "psys", label: "Platform" }
]

function measured(v) { return Format.known(v) && Number(v) >= 0 }

// Every power reading the machine offers and the best total among them:
// {rows: [{id, label, watts, note}], cpu, gpu, total, source, note, onAc}.
// The total is measured when something sees the whole machine (a power supply
// with a sensor, the battery it runs on, RAPL's platform zone); otherwise it
// adds the CPU package and the discrete cards and says so.
function powerSummary(frame) {
  var f = frame || {}
  var power = f.power || {}
  var rapl = power.rapl || {}
  var battery = f.battery || null
  var rows = []
  var cpu = measured(f.cpu && f.cpu.watts) ? Number(f.cpu.watts) : (measured(rapl.package) ? Number(rapl.package) : null)
  if (cpu !== null) rows.push({ id: "cpu", label: "CPU package", watts: cpu, note: "" })
  RAPL_ZONES.forEach(function(z) { if (measured(rapl[z.key])) rows.push({ id: z.key, label: z.label, watts: Number(rapl[z.key]), note: "" }) })
  var gpu = null
  gpus(f).forEach(function(g) {
    if (g.integrated) return
    if (g.asleep) { rows.push({ id: "gpu:" + g.id, label: gpuName(g), watts: null, note: "asleep" }); return }
    if (!measured(g.watts)) return
    rows.push({ id: "gpu:" + g.id, label: gpuName(g), watts: Number(g.watts), note: measured(g.wattsCap) ? "limit " + Format.wattsText(g.wattsCap) : "" })
    gpu = (gpu || 0) + Number(g.watts)
  })
  var status = battery ? String(battery.status || "") : ""
  var batteryWatts = battery && battery.present && measured(battery.watts) && Number(battery.watts) > 0 ? Number(battery.watts) : null
  if (batteryWatts !== null && status === "Charging") rows.push({ id: "battery", label: "Battery", watts: batteryWatts, note: "charging" })
  var psu = measured(power.psu) ? Number(power.psu) : null
  if (psu !== null) rows.push({ id: "psu", label: "Power supply", watts: psu, note: "" })

  var total = null, source = "", note = ""
  if (psu !== null) { total = psu; source = "psu"; note = "Measured at the power supply" }
  else if (batteryWatts !== null && status === "Discharging") { total = batteryWatts; source = "battery"; note = "Drawn from the battery" }
  else if (measured(rapl.psys)) { total = Number(rapl.psys); source = "psys"; note = "Whole platform, measured by RAPL" }
  else if (cpu !== null || gpu !== null) {
    total = (cpu || 0) + (gpu || 0)
    source = "sum"
    note = cpu !== null && gpu !== null ? "CPU and graphics cards" : (cpu !== null ? "CPU package only" : "Graphics cards only")
  }
  return { rows: rows, cpu: cpu, gpu: gpu, total: total, source: source, note: note,
           onAc: battery && battery.acOnline !== undefined && battery.acOnline !== null ? !!battery.acOnline : null,
           raplHint: power.raplHint === true && cpu === null }
}

// ---------------------------------------------------------------- network

function wifiGlyph(dbm) {
  if (!Format.known(dbm)) return "󰤨"
  var d = Number(dbm)
  return d >= -55 ? "󰤨" : d >= -65 ? "󰤥" : d >= -75 ? "󰤢" : d >= -85 ? "󰤟" : "󰤯"
}

function ifaceGlyph(iface) {
  if (!iface) return "󰈀"
  if (iface.kind === "wifi") return wifiGlyph(iface.signal)
  if (iface.kind === "vpn") return "󰖂"
  return "󰈀"
}

function linkSpeed(iface) {
  if (!iface) return ""
  if (iface.kind === "wifi" && Format.known(iface.bitrate)) return Math.round(iface.bitrate) + " Mb/s"
  var mbps = Format.number(iface.speed)
  if (mbps <= 0) return ""
  return mbps >= 1000 ? (mbps / 1000) + " Gb/s" : mbps + " Mb/s"
}

// The interface the page describes: the default route's, else the first up.
function mainIface(frame) {
  var all = list(frame && frame.network && frame.network.ifaces)
  for (var i = 0; i < all.length; i++) if (all[i].isDefault) return all[i]
  for (var j = 0; j < all.length; j++) if (all[j].up) return all[j]
  return all.length > 0 ? all[0] : null
}

// ---------------------------------------------------------------- battery

function batteryGlyph(percent, charging) {
  if (charging) return "󰂄"
  var steps = ["󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"]
  return steps[Math.round(Format.clamp(percent, 0, 100) / 10)]
}

// -------------------------------------------------------------- processes

function processValue(row, key) {
  if (!row) return 0
  if (key === "io") return Format.number(row.read) + Format.number(row.write)
  if (key === "net") return Format.number(row.rx) + Format.number(row.tx)
  return Format.number(row[key])
}

// Rows matching a search, biggest first by key (or by name for "name").
function processRows(rows, query, key) {
  var q = String(query || "").trim().toLowerCase()
  var out = list(rows).filter(function(r) { return !q || String(r.name || "").toLowerCase().indexOf(q) !== -1 })
  out.sort(function(a, b) {
    if (key === "name") return String(a.name || "").localeCompare(String(b.name || ""))
    var d = processValue(b, key) - processValue(a, key)
    return d !== 0 ? d : String(a.name || "").localeCompare(String(b.name || ""))
  })
  return out
}

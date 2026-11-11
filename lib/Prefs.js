.pragma library

// What the user can set, how it is stored in shell.json, and how a stored
// entry reads back. Keys and values match the manifest schema, so an entry
// written by hand, by Setup, or by the panel's Settings tab all mean the same.

// ------------------------------------------------------------ catalogue

// Every tab and bar module. "bar" says the module can sit in the bar; the
// picture kinds say which looks make sense for it.
var CATALOGUE = [
  { id: "apps",     label: "Apps",     short: "APP", glyph: "󰕰", page: "AppsPage.qml",     bar: false },
  { id: "cpu",      label: "CPU",      short: "CPU", glyph: "󰻠", page: "CpuPage.qml",      bar: true, graph: true, ring: true },
  { id: "gpu",      label: "GPU",      short: "GPU", glyph: "󰢮", page: "GpuPage.qml",      bar: true, graph: true, ring: true },
  { id: "memory",   label: "Memory",   short: "MEM", glyph: "󰍛", page: "MemoryPage.qml",   bar: true, graph: true, ring: true },
  { id: "disks",    label: "Disks",    short: "DSK", glyph: "󰋊", page: "DisksPage.qml",    bar: true, graph: true, ring: true },
  { id: "network",  label: "Network",  short: "NET", glyph: "󰛳", page: "NetworkPage.qml",  bar: true, graph: true },
  { id: "sensors",  label: "Sensors",  short: "SEN", glyph: "󰔏", page: "SensorsPage.qml",  bar: true },
  { id: "battery",  label: "Battery",  short: "BAT", glyph: "󰁹", page: "PowerPage.qml",    bar: true, ring: true, tab: "power" },
  { id: "power",    label: "Power",    short: "PWR", glyph: "󱐋", page: "PowerPage.qml",    bar: false },
  { id: "services", label: "Services", short: "SVC", glyph: "󰒋", page: "ServicesPage.qml", bar: false },
  { id: "settings", label: "Settings", short: "SET", glyph: "󰒓", page: "SettingsPage.qml", bar: false }
]

// Tabs in the order the panel shows them. Settings is always last and
// always there, so it is not part of the user's list.
var TAB_ORDER = ["apps", "cpu", "gpu", "memory", "disks", "network", "sensors", "power", "services"]

// The VRAM readout borrows the GPU's look and opens the GPU tab.
var VRAM = { id: "vram", label: "VRAM", short: "VRM", glyph: "󰍛", page: "GpuPage.qml", bar: true, graph: true, ring: true, tab: "gpu" }

function entry(id) {
  if (id === "vram") return VRAM
  for (var i = 0; i < CATALOGUE.length; i++) if (CATALOGUE[i].id === id) return CATALOGUE[i]
  return null
}

// The tab a bar module opens.
function tabOf(id) {
  var e = entry(id)
  return e && e.tab ? e.tab : (e ? e.id : "cpu")
}

function pageOf(tab) {
  var e = entry(tab)
  return e ? e.page : "CpuPage.qml"
}

// --------------------------------------------------------------- defaults

var DEFAULTS = {
  modules: "cpu,memory,network,gpu,disks",
  tabs: "apps,cpu,gpu,memory,disks,network,sensors,power,services",
  style: "both",
  cpuStyle: "graph", gpuStyle: "graph", memoryStyle: "ring", disksStyle: "graph",
  networkStyle: "", sensorsStyle: "", batteryStyle: "",
  barLabels: "text",
  graphWidth: 36,
  barDisks: "",
  disksSource: "all",
  barSensors: "cpu",
  barGpus: "discrete",
  showGpuMemory: false,
  temperatureUnit: "Celsius",
  refreshSeconds: 1,
  historySeconds: 240,
  publicIp: true,
  utilizationColors: false,
  utilizationLowColor: "", utilizationNormalColor: "", utilizationWarningColor: "", utilizationCriticalColor: "",
  showProcesses: true,
  showCores: true, showLoad: true,
  showBreakdown: true,
  showVolumes: true, showActivity: true,
  showInterfaces: true, showTotals: true, showAddresses: true,
  showTemperatures: true, showFans: true,
  showHistory: true, showDetails: true, showDevices: true
}

// Sections a page can hide, as the Settings tab lists them.
var SECTIONS = {
  cpu: [{ key: "showCores", label: "Per-core rings" }, { key: "showLoad", label: "Load average and uptime" }],
  memory: [{ key: "showBreakdown", label: "Breakdown" }],
  disks: [{ key: "showVolumes", label: "Volumes" }, { key: "showActivity", label: "Read and write activity" }],
  network: [
    { key: "showInterfaces", label: "Interfaces" },
    { key: "showTotals", label: "Totals since boot" },
    { key: "publicIp", label: "Public IP address" },
    { key: "showAddresses", label: "IP addresses" }
  ],
  sensors: [{ key: "showTemperatures", label: "Temperatures" }, { key: "showFans", label: "Fans" }],
  power: [
    { key: "showHistory", label: "Battery charge history", battery: true },
    { key: "showDetails", label: "Battery details", battery: true },
    { key: "showDevices", label: "Devices", battery: true }
  ]
}

var REFRESH_STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10]
var HISTORY_STEPS = [30, 60, 120, 240, 300, 600, 900, 1800, 3600]

// --------------------------------------------------------------- reading

function isSet(settings, key) {
  return !!settings && settings[key] !== undefined && settings[key] !== null && settings[key] !== ""
}

function value(settings, key) {
  return isSet(settings, key) ? settings[key] : DEFAULTS[key]
}

// shell.json holds booleans as true/false, but entries edited elsewhere carry
// "true"/"false" strings (this machine's does), so read both.
function flag(settings, key) {
  var v = value(settings, key)
  if (typeof v === "boolean") return v
  if (typeof v === "number") return v !== 0
  var s = String(v).trim().toLowerCase()
  if (s === "true" || s === "yes" || s === "on" || s === "1") return true
  if (s === "false" || s === "no" || s === "off" || s === "0") return false
  return DEFAULTS[key] === true
}

function numberIn(settings, key, low, high) {
  var n = Number(value(settings, key))
  if (!isFinite(n)) n = Number(DEFAULTS[key])
  return Math.max(low, Math.min(high, n))
}

// Comma, space or semicolon separated words, each once, in order.
function words(raw) {
  var text = Array.isArray(raw) ? raw.join(",") : String(raw === undefined || raw === null ? "" : raw)
  var out = []
  var parts = text.split(/[\s,;]+/)
  for (var i = 0; i < parts.length; i++) if (parts[i] && out.indexOf(parts[i]) === -1) out.push(parts[i])
  return out
}

var NICKNAMES = { mem: "memory", ram: "memory", disk: "disks", storage: "disks", net: "network", wifi: "network",
  temp: "sensors", temps: "sensors", sensor: "sensors", bat: "battery" }

function canonical(id) {
  var s = String(id || "").toLowerCase()
  return NICKNAMES[s] || s
}

// Bar modules in the user's order, unknown names dropped.
function barModules(settings) {
  var out = []
  var list = words(value(settings, "modules"))
  for (var i = 0; i < list.length; i++) {
    var e = entry(canonical(list[i]))
    if (e && e.bar && out.indexOf(e.id) === -1) out.push(e.id)
  }
  return out
}

// Panel tabs, in the fixed order, filtered by the user's list and by what
// the machine has. A list naming battery means power, which carries it.
function tabs(settings, hasGpu) {
  var wanted = words(value(settings, "tabs")).map(function(id) { id = canonical(id); return id === "battery" ? "power" : id })
  var out = []
  for (var i = 0; i < TAB_ORDER.length; i++) {
    var id = TAB_ORDER[i]
    if (id === "gpu" && !hasGpu) continue
    if (wanted.indexOf(id) !== -1) out.push(id)
  }
  return out.length > 0 ? out : ["cpu"]
}

function labelMode(settings) {
  var mode = String(value(settings, "barLabels")).toLowerCase()
  return mode === "inline" || mode === "icon" ? mode : "text"
}

function temperatureUnit(settings) {
  return String(value(settings, "temperatureUnit")).toLowerCase() === "fahrenheit" ? "Fahrenheit" : "Celsius"
}

// ------------------------------------------------------------- bar looks

// graph, ring, text (figure only), both (graph and figure), ring-text.
var LOOKS = ["graph", "ring", "text", "both", "ring-text"]

function look(raw) {
  var s = String(raw || "").toLowerCase().replace("+", "-").replace("_", "-")
  if (s === "figure") s = "text"
  if (s === "graph-text") s = "both"
  if (s === "ringtext" || s === "ring-figure") s = "ring-text"
  return LOOKS.indexOf(s) !== -1 ? s : ""
}

// A module's look: its own setting, else the global one when the user set
// it, else the module's default, else the global default.
function lookOf(settings, module) {
  var id = module === "vram" ? "gpu" : module
  var own = settings ? look(settings[id + "Style"]) : ""
  if (own) return own
  var global = look(value(settings, "style")) || "both"
  if (isSet(settings, "style")) return global
  return look(DEFAULTS[id + "Style"]) || global
}

// The looks the Settings tab offers for a module.
function looksFor(module) {
  if (module === "disks") {
    return [{ value: "graph", label: "Graph or ring" }, { value: "text", label: "Figure" }, { value: "both", label: "Graph or ring, and figure" }]
  }
  var e = entry(module) || {}
  var out = []
  if (e.graph) out.push({ value: "graph", label: "Graph" })
  if (e.ring) out.push({ value: "ring", label: "Ring" })
  out.push({ value: "text", label: "Figure" })
  if (e.graph) out.push({ value: "both", label: "Graph and figure" })
  if (e.ring) out.push({ value: "ring-text", label: "Ring and figure" })
  return out
}

// What a look draws for a module that offers graph and/or ring:
// {graph, ring, figure}. A module without pictures always shows its figure.
function parts(module, chosen) {
  var e = entry(module) || {}
  var l = look(chosen) || "both"
  var graph = !!e.graph && (l === "graph" || l === "both")
  var ring = !!e.ring && (l === "ring" || l === "ring-text")
  var figure = l === "text" || l === "both" || l === "ring-text" || (!graph && !ring)
  return { graph: graph, ring: ring, figure: figure }
}

// --------------------------------------------------------------- disks

var DISK_SHOWS = [{ value: "speed", label: "Speed" }, { value: "used", label: "Used" }, { value: "both", label: "Both" }]

function diskShow(raw) {
  var s = String(raw || "").toLowerCase()
  if (s === "activity" || s === "io" || s === "rate" || s === "rates") return "speed"
  if (s === "space" || s === "usage" || s === "capacity") return "used"
  return s === "speed" || s === "used" || s === "both" ? s : ""
}

// The bar's disk readouts as [{disk, show}]. barDisks reads like
// "all:speed,nvme-WDC_X_123:both"; ids may hold colons, so the show is
// whatever follows the last colon when it names one. "none" means no
// readout; empty means one readout for disksSource, as a ring of space used
// when the disk look is a ring and speed otherwise.
function barDisks(settings) {
  var raw = String(value(settings, "barDisks") || "").trim()
  if (raw.toLowerCase() === "none") return []
  if (!raw) {
    var l = lookOf(settings, "disks")
    var styled = isSet(settings, "disksStyle") || isSet(settings, "style")
    return [{ disk: String(value(settings, "disksSource") || "all").trim() || "all",
              show: !styled || l === "ring" || l === "ring-text" ? "used" : "speed" }]
  }
  var out = []
  var seen = {}
  var items = words(raw)
  for (var i = 0; i < items.length; i++) {
    var at = items[i].lastIndexOf(":")
    var show = at === -1 ? "" : diskShow(items[i].slice(at + 1))
    var disk = show ? items[i].slice(0, at) : items[i]
    if (disk.toLowerCase() === "all") disk = "all"
    if (!disk || seen[disk]) continue
    seen[disk] = true
    out.push({ disk: disk, show: show || "speed" })
  }
  return out
}

function barDisksText(list) {
  if (!list || list.length === 0) return "none"
  return list.map(function(d) { return d.disk + ":" + d.show }).join(",")
}

// ---------------------------------------------------- utilization colours

var GRADE_KEYS = ["utilizationLowColor", "utilizationNormalColor", "utilizationWarningColor", "utilizationCriticalColor"]
var GRADE_LABELS = ["Low (under 25%)", "Normal (25 to 59%)", "Warning (60 to 84%)", "Critical (85% and up)"]
var GRADE_DARK = ["#72ca9b", "#759cd1", "#da9c6c", "#d67471"]
var GRADE_LIGHT = ["#277944", "#3569aa", "#9c6019", "#af4444"]

function hexColor(text) {
  return typeof text === "string" && /^#[0-9A-Fa-f]{6}$/.test(text)
}

function grade(percent) {
  if (percent === null || percent === undefined || percent === "" || !isFinite(Number(percent))) return -1
  var p = Math.round(Math.max(0, Math.min(100, Number(percent))))
  return p < 25 ? 0 : (p < 60 ? 1 : (p < 85 ? 2 : 3))
}

function gradeColor(settings, index, lightTheme) {
  var chosen = settings ? settings[GRADE_KEYS[index]] : ""
  return hexColor(chosen) ? chosen : (lightTheme ? GRADE_LIGHT : GRADE_DARK)[index]
}

// A function from a percentage to its grade colour, or null when the user
// has utilization colours off. Graphs call it per bar while painting.
function grader(settings, lightTheme) {
  if (!flag(settings, "utilizationColors")) return null
  var colors = [0, 1, 2, 3].map(function(i) { return gradeColor(settings, i, lightTheme) })
  return function(percent) {
    var g = grade(percent)
    return g < 0 ? null : colors[g]
  }
}

// ---------------------------------------------------------------- writing

// The nearest step to a stored value on a log scale, so values saved between
// steps (or by hand) move from where they are.
function nearestStep(stored, steps) {
  var v = Number(stored)
  if (!(v > 0)) v = 1
  var best = 0, distance = Infinity
  for (var i = 0; i < steps.length; i++) {
    var d = Math.abs(Math.log(steps[i]) - Math.log(v))
    if (d < distance) { distance = d; best = i }
  }
  return best
}

// A copy of settings with one key changed.
function withValue(settings, key, next) {
  var out = {}
  for (var k in settings) if (k !== "id") out[k] = settings[k]
  out[key] = next
  return out
}

// Move one id up or down in a list.
function moved(list, id, delta) {
  var out = list.slice()
  var from = out.indexOf(id)
  var to = Math.max(0, Math.min(out.length - 1, from + delta))
  if (from < 0 || to === from) return out
  out.splice(from, 1)
  out.splice(to, 0, id)
  return out
}

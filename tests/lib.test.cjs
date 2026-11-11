// Tests for lib/*.js, run with: node tests/lib.test.cjs
// The libraries are QML JavaScript (".pragma library", ".import"), so a tiny
// loader evaluates each one in its own context the way QML would.
"use strict"
const fs = require("fs")
const path = require("path")
const vm = require("vm")
const assert = require("assert")

const LIB = path.join(__dirname, "..", "lib")
const loaded = {}

function load(name) {
  if (loaded[name]) return loaded[name]
  const context = vm.createContext({ console })
  let source = fs.readFileSync(path.join(LIB, name), "utf8")
  source = source.replace(/^\.pragma library\s*$/m, "")
  source = source.replace(/^\.import "([^"]+)" as (\w+)\s*$/gm, (_, file, alias) => {
    context[alias] = load(file)
    return ""
  })
  vm.runInContext(source, context, { filename: name })
  loaded[name] = context
  return context
}

const Format = load("Format.js")
const Prefs = load("Prefs.js")
const Hardware = load("Hardware.js")
const Palette = load("Palette.js")

let passed = 0
function test(name, body) {
  try { body(); passed++ } catch (error) { console.error("FAIL " + name + "\n  " + error.message); process.exitCode = 1 }
}
const same = (a, b) => assert.deepStrictEqual(JSON.parse(JSON.stringify(a)), b)

// ------------------------------------------------------------- Format

test("sizes keep four characters", () => {
  same(Format.size(0), { value: "0", unit: "B" })
  same(Format.size(24 * 1024), { value: "24", unit: "KB" })
  same(Format.size(48.3 * 1048576), { value: "48.3", unit: "MB" })
  same(Format.size(929 * 1073741824), { value: "929", unit: "GB" })
  same(Format.size(1023.7 * 1048576), { value: "1.0", unit: "GB" })
  same(Format.size(null), { value: "0", unit: "B" })
  assert.strictEqual(Format.sizePair(6.4 * 1073741824, 20 * 1073741824), "6.4 / 20.0 GB")
  assert.strictEqual(Format.sizePair(512 * 1048576, 2 * 1073741824), "512 MB / 2.0 GB")
})

test("rates and tiny rates", () => {
  assert.strictEqual(Format.rateText(1536), "2 KB/s")
  assert.strictEqual(Format.rateTiny(100), "0")
  assert.strictEqual(Format.rateTiny(48 * 1024), "48K")
  assert.strictEqual(Format.rateTiny(1.2 * 1048576), "1.2M")
  assert.strictEqual(Format.rateOrDash(0), "–")
})

test("percent, share, degrees, clocks, watts, time", () => {
  assert.strictEqual(Format.percent(6.4), "6%")
  assert.strictEqual(Format.percent(140), "100%")
  assert.strictEqual(Format.share(0.34), "0.3%")
  assert.strictEqual(Format.share(12.6), "13%")
  assert.strictEqual(Format.degrees(63.4, "Celsius"), "63°")
  assert.strictEqual(Format.degrees(100, "Fahrenheit"), "212°")
  assert.strictEqual(Format.degrees(null, "Celsius"), "–")
  assert.strictEqual(Format.clockText(4160), "4.16 GHz")
  assert.strictEqual(Format.clockText(227), "227 MHz")
  assert.strictEqual(Format.clockTiny(2500), "2.5G")
  assert.strictEqual(Format.wattsText(57), "57 W")
  assert.strictEqual(Format.wattsText(4.25), "4.3 W")
  assert.strictEqual(Format.wattsText(null), "")
  assert.strictEqual(Format.duration(3 * 3600 + 31 * 60), "3h 31m")
  assert.strictEqual(Format.duration(2 * 86400 + 5 * 3600), "2d 5h")
  assert.strictEqual(Format.hoursMinutes(95), "1:35")
  assert.strictEqual(Format.span(240), "4 min")
  assert.strictEqual(Format.span(3600), "1 h")
  assert.strictEqual(Format.loadText === undefined, true)
  assert.strictEqual(Format.loadAverage([6.164, 5.15, 4.5]), "6.16  5.15  4.50")
})

// -------------------------------------------------------------- Prefs

// This machine's entry in shell.json, as written there.
const KUSH = { barGpus: "none", barLabels: "inline", cpuStyle: "text", memoryStyle: "text",
  modules: "cpu,memory", showGpuMemory: "false", style: "text", utilizationColors: "false" }

test("this machine's bar entry reads as CPU and memory figures, inline", () => {
  same(Prefs.barModules(KUSH), ["cpu", "memory"])
  assert.strictEqual(Prefs.labelMode(KUSH), "inline")
  assert.strictEqual(Prefs.lookOf(KUSH, "cpu"), "text")
  assert.strictEqual(Prefs.lookOf(KUSH, "memory"), "text")
  assert.strictEqual(Prefs.lookOf(KUSH, "gpu"), "text")
  assert.strictEqual(Prefs.flag(KUSH, "showGpuMemory"), false)
  assert.strictEqual(Prefs.flag(KUSH, "utilizationColors"), false)
  assert.strictEqual(Prefs.flag(KUSH, "publicIp"), true)
  same(Prefs.parts("cpu", Prefs.lookOf(KUSH, "cpu")), { graph: false, ring: false, figure: true })
})

test("defaults give each module its own look", () => {
  assert.strictEqual(Prefs.lookOf({}, "cpu"), "graph")
  assert.strictEqual(Prefs.lookOf({}, "memory"), "ring")
  assert.strictEqual(Prefs.lookOf({}, "network"), "both")
  assert.strictEqual(Prefs.lookOf({ style: "ring-text" }, "network"), "ring-text")
  same(Prefs.parts("network", "ring-text"), { graph: false, ring: false, figure: true })
  same(Prefs.parts("sensors", "graph"), { graph: false, ring: false, figure: true })
  assert.strictEqual(Prefs.lookOf({ cpuStyle: "Graph+Text" }, "cpu"), "both")
  assert.strictEqual(Prefs.lookOf({ gpuStyle: "ring" }, "vram"), "ring")
})

test("modules and tabs accept nicknames and keep order", () => {
  same(Prefs.barModules({ modules: "mem, net;cpu bogus cpu" }), ["memory", "network", "cpu"])
  same(Prefs.tabs({}, true), ["apps", "cpu", "gpu", "memory", "disks", "network", "sensors", "power", "services"])
  same(Prefs.tabs({}, false).indexOf("gpu"), -1)
  same(Prefs.tabs({ tabs: "battery,cpu" }, true), ["cpu", "power"])
  same(Prefs.tabs({ tabs: "nonsense" }, true), ["cpu"])
  assert.strictEqual(Prefs.tabOf("battery"), "power")
  assert.strictEqual(Prefs.tabOf("vram"), "gpu")
})

test("bar disks parse ids with colons and fall back to disksSource", () => {
  same(Prefs.barDisks({ barDisks: "all:speed,usb-Stick_0:0:both,nvme0n1" }),
    [{ disk: "all", show: "speed" }, { disk: "usb-Stick_0:0", show: "both" }, { disk: "nvme0n1", show: "speed" }])
  same(Prefs.barDisks({ barDisks: "none" }), [])
  same(Prefs.barDisks({}), [{ disk: "all", show: "used" }])
  same(Prefs.barDisks({ style: "graph" }), [{ disk: "all", show: "speed" }])
  assert.strictEqual(Prefs.barDisksText([{ disk: "a", show: "used" }]), "a:used")
})

test("utilization grades and colours", () => {
  assert.strictEqual(Prefs.grade(24.4), 0)
  assert.strictEqual(Prefs.grade(59), 1)
  assert.strictEqual(Prefs.grade(84.6), 3)
  assert.strictEqual(Prefs.grade(null), -1)
  assert.strictEqual(Prefs.grader({}, false), null)
  const g = Prefs.grader({ utilizationColors: "true", utilizationCriticalColor: "#123456" }, false)
  assert.strictEqual(g(10), "#72ca9b")
  assert.strictEqual(g(99), "#123456")
  assert.strictEqual(g(null), null)
})

test("nearest step and list moves", () => {
  assert.strictEqual(Prefs.nearestStep(1, Prefs.REFRESH_STEPS), 3)
  assert.strictEqual(Prefs.nearestStep(0.4, Prefs.REFRESH_STEPS), 2)
  assert.strictEqual(Prefs.nearestStep(250, Prefs.HISTORY_STEPS), 3)
  same(Prefs.moved(["a", "b", "c"], "c", -1), ["a", "c", "b"])
  same(Prefs.moved(["a", "b"], "a", -1), ["a", "b"])
  same(Prefs.withValue({ id: "x", a: 1 }, "b", 2), { a: 1, b: 2 })
})

// ----------------------------------------------------------- Hardware

const FRAME = {
  cpu: { temp: 56.3, watts: null, mhz: 3700 },
  gpus: [
    { id: "0000:0c:00.0", vendor: "amd", name: "Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M", integrated: false, load: 14, temp: 40,
      watts: 57, wattsCap: 265, vramUsed: 6.4e9, vramTotal: 20e9 },
    { id: "0000:10:00.0", vendor: "amd", name: "AMD Radeon Graphics", integrated: true, load: null, temp: 45 }
  ],
  disks: {
    read: 10, write: 20, stall: { some: 0, full: 0 },
    drives: { nvme0n1: { id: "nvme-WDC_X_1", model: "WDC X", read: 1, write: 2, busy: 95, latency: 30 }, sda: { id: "ata-MK_2", read: 0, write: 0 } },
    volumes: [
      { key: "nvme0n1", mount: "/", drive: "nvme0n1", kind: "internal", size: 1000, used: 240, checks: [] },
      { key: "nvme0n1", mount: "/boot", drive: "nvme0n1", kind: "internal", size: 2, used: 0.2, checks: [{ id: "trim-off", fix: "enable-fstrim", args: [] }] },
      { key: "sda", mount: "/mnt/games", drive: "sda", kind: "internal", size: 900, used: 450, checks: [] },
      { key: "nas:/share", mount: "/mnt/nas", drive: "", kind: "network", size: 5000, used: 4000, checks: [] }
    ]
  },
  sensors: {
    temps: [{ id: "acpitz/temp1", group: "ACPI", label: "ACPI 1", celsius: 17 }, { id: "k10temp/temp1", group: "CPU", label: "Tctl", celsius: 63 }],
    fans: [{ id: "amdgpu@0000:0c:00.0/fan1", group: "GPU", label: "Fan 1", rpm: 923, percent: null }, { id: "nct/fan2", group: "Board", label: "Fan 2", rpm: 0 }]
  },
  power: { rapl: { package: null }, psu: null, raplHint: true },
  battery: null,
  network: { ifaces: [{ name: "wlp5s0", up: false, kind: "wifi" }, { name: "enp6s0", up: true, isDefault: true, kind: "ethernet", speed: 1000 }] }
}

test("GPU choice for the bar", () => {
  same(Hardware.barGpus(FRAME, "discrete").map(g => g.id), ["0000:0c:00.0"])
  same(Hardware.barGpus(FRAME, "all").length, 2)
  same(Hardware.barGpus(FRAME, "none"), [])
  same(Hardware.barGpus(FRAME, "0000:10:00.0").map(g => g.id), ["0000:10:00.0"])
  same(Hardware.barGpus(FRAME, "0000:99:00.0").map(g => g.id), ["0000:0c:00.0"])
  assert.strictEqual(Hardware.gpuName(FRAME.gpus[0]), "AMD RX 7900 XT/7900 XTX/7900 GRE/7900M")
  assert.strictEqual(Hardware.gpuName(FRAME.gpus[1]), "AMD Radeon Graphics")
  assert.strictEqual(Hardware.gpuName({ name: "NVIDIA GeForce RTX 4090", vendor: "nvidia" }), "NVIDIA RTX 4090")
  assert.strictEqual(Math.round(Hardware.vramPercent(FRAME.gpus[0])), 32)
  assert.strictEqual(Hardware.vramPercent(FRAME.gpus[1]), null)
  assert.strictEqual(Hardware.gpuHasLoad(FRAME.gpus[1]), false)
})

test("drives resolve by id or kernel name", () => {
  assert.strictEqual(Hardware.resolveDrive(FRAME, "nvme-WDC_X_1"), "nvme0n1")
  assert.strictEqual(Hardware.resolveDrive(FRAME, "sda"), "sda")
  assert.strictEqual(Hardware.resolveDrive(FRAME, "gone"), "")
  assert.strictEqual(Hardware.resolveDrive(FRAME, ""), "all")
  assert.strictEqual(Hardware.driveKey(FRAME, "sda"), "ata-MK_2")
  same(Hardware.barDisksHere(FRAME, [{ disk: "gone", show: "used" }]), [{ disk: "all", show: "used" }])
  same(Hardware.barDisksHere(FRAME, [{ disk: "nvme-WDC_X_1", show: "both" }]), [{ disk: "nvme0n1", show: "both" }])
  assert.strictEqual(Hardware.driveTag("nvme1n1"), "NV1")
  assert.strictEqual(Hardware.driveTag("sdb"), "SDB")
})

test("space used counts local volumes only", () => {
  same(Hardware.spaceUsed(FRAME, "all"), { used: 690.2, size: 1902, fraction: 690.2 / 1902 })
  same(Hardware.spaceUsed(FRAME, "sda").fraction, 0.5)
  same(Hardware.volumeGroups(FRAME.disks.volumes).map(g => g.title), ["Internal", "Network"])
  assert.strictEqual(Hardware.volumeName("/boot/efi"), "Boot")
  assert.strictEqual(Hardware.volumeName("/mnt/games"), "games")
})

test("drive strain and checks", () => {
  assert.strictEqual(Hardware.driveStrain(FRAME.disks.drives.nvme0n1, 25, false).level, "danger")
  assert.strictEqual(Hardware.driveStrain(FRAME.disks.drives.nvme0n1, 0, false).level, "warn")
  assert.strictEqual(Hardware.driveStrain({ busy: 10 }, 0, false).level, "")
  assert.strictEqual(Hardware.checksFor(FRAME.disks.volumes, "all").length, 1)
  assert.strictEqual(Hardware.checksFor(FRAME.disks.volumes, "sda").length, 0)
  const argv = Hardware.fixCommand({ id: "trim-off", fix: "enable-fstrim", args: [] })
  assert.strictEqual(argv[0], "xdg-terminal-exec")
  assert.strictEqual(argv[4], "cornerstone-stats-fix")
  assert.strictEqual(Hardware.fixCommand({ fix: "allow-discards", args: ["/dev/mapper/x; rm -rf ~"] }), null)
  assert.strictEqual(Hardware.fixCommand({ fix: "allow-discards", args: ["../x"] }), null)
  assert.ok(Hardware.fixCommand({ fix: "allow-discards", args: ["/dev/mapper/root"] }))
  assert.ok(!/–/.test(Hardware.problem({ id: "fat-flush", fix: "udisks-no-flush", flags: ["flush"] }).detail))
})

test("sensors: names, order, readings", () => {
  same(Hardware.sortedTemps(FRAME).map(Hardware.sensorName), ["CPU Tctl", "ACPI 1"])
  assert.strictEqual(Hardware.sensorReading(FRAME, "cpu", "Celsius").text, "56°")
  assert.strictEqual(Hardware.sensorReading(FRAME, "gpu", "Celsius").label, "AMD RX 7900 XT/7900 XTX/7900 GRE/7900M")
  assert.strictEqual(Hardware.sensorReading(FRAME, "k10temp/temp1", "Celsius").text, "63°")
  same(Hardware.sensorReading(FRAME, "amdgpu@0000:0c:00.0/fan1", "Celsius").text, "923")
  assert.strictEqual(Hardware.sensorReading(FRAME, "nothing", "Celsius"), null)
  same(Hardware.fanSummary(FRAME), { count: 2, on: 1, average: 923 })
  same(Hardware.fanReading({ rpm: 0 }), { value: "Off", unit: "" })
  same(Hardware.fanReading({ rpm: null, percent: 30 }), { value: "30", unit: "%" })
  assert.ok(Hardware.sensorChoices(FRAME).some(c => c.value === "gpu:0000:10:00.0"))
})

test("power adds what it can and says so", () => {
  const p = Hardware.powerSummary(FRAME)
  assert.strictEqual(p.total, 57)
  assert.strictEqual(p.source, "sum")
  assert.strictEqual(p.note, "Graphics cards only")
  assert.strictEqual(p.raplHint, true)
  same(p.rows.map(r => r.id), ["gpu:0000:0c:00.0"])
  const laptop = { cpu: { watts: 5 }, gpus: [], power: { rapl: { psys: 11 } }, battery: { present: true, status: "Discharging", watts: 12, acOnline: false } }
  const q = Hardware.powerSummary(laptop)
  assert.strictEqual(q.total, 12)
  assert.strictEqual(q.source, "battery")
  assert.strictEqual(q.onAc, false)
})

test("network and processes", () => {
  assert.strictEqual(Hardware.mainIface(FRAME).name, "enp6s0")
  assert.strictEqual(Hardware.linkSpeed(FRAME.network.ifaces[1]), "1 Gb/s")
  const rows = [{ name: "b", cpu: 1, read: 5, write: 0 }, { name: "a", cpu: 1, read: 0, write: 9 }, { name: "chrome", cpu: 3 }]
  same(Hardware.processRows(rows, "", "cpu").map(r => r.name), ["chrome", "a", "b"])
  same(Hardware.processRows(rows, "", "io").map(r => r.name), ["a", "b", "chrome"])
  same(Hardware.processRows(rows, "CHR", "cpu").map(r => r.name), ["chrome"])
  same(Hardware.processRows(rows, "", "name").map(r => r.name), ["a", "b", "chrome"])
})

// ------------------------------------------------------------ Palette

test("palette keeps two series apart", () => {
  const theme = Palette.parseToml('accent = "#f3e600"\nblue = "#00b7ff"\nmagenta = \'#ff2a6d\'\nred = "#ff003c"\ngreen = "#00ff9f"\nyellow="#f3e600"\n')
  assert.strictEqual(theme.blue, "#00b7ff")
  const p = Palette.pick(theme, "#f3e600", "#0b0b0b", "#ff0000")
  assert.strictEqual(p.series1, "#f3e600")
  assert.strictEqual(p.series2, "#00b7ff")
  assert.notStrictEqual(p.series3, p.series2)
  assert.strictEqual(p.danger, "#ff003c")
  const bare = Palette.pick({}, "#f3e600", "#000000", "#ff0000")
  assert.ok(Palette.hueGap(Palette.hsl(bare.series2).h, Palette.hsl("#f3e600").h) > 150)
  assert.strictEqual(Palette.hsl("#ff0000ff".replace("#ff", "#")).h, Palette.hsl("#0000ff").h)
})

console.log(passed + " passed" + (process.exitCode ? ", some FAILED" : ""))

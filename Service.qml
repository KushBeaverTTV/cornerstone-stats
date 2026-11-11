import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import "lib/Format.js" as Format
import "lib/Hardware.js" as Hardware
import "lib/Palette.js" as Palette

// One collector per shell, shared by every copy of the bar widget on every
// screen. It keeps the latest frame, the histories the graphs draw, the
// graph colours, and the IPC entry point.
//
// Histories are arrays the service grows in place; `revision` goes up once
// per frame so graphs know to repaint. Nothing copies them per frame.
Item {
  id: service

  // ------------------------------------------------------------- state

  property var frame: ({})
  property bool ready: false
  property int revision: 0
  property var history: ({})
  property int capacity: 240
  property real interval: 1
  property real historySeconds: 240
  property string problem: ""

  readonly property var gpus: Hardware.gpus(frame)
  readonly property bool hasGpu: gpus.length > 0
  readonly property bool hasBattery: !!(frame.battery && frame.battery.present)

  // Graph colours from the theme.
  property var themeColors: ({})
  readonly property var palette: Palette.pick(themeColors, Commons.Color.accent, Commons.Color.background, Commons.Color.urgent)
  readonly property color series1: palette.series1
  readonly property color series2: palette.series2
  readonly property color series3: palette.series3
  readonly property color warn: palette.warn
  readonly property color danger: palette.danger
  readonly property color good: palette.good

  readonly property string collectorPath: Qt.resolvedUrl("bin/collector").toString().replace(/^file:\/\//, "")

  // ----------------------------------------------------------- history

  function series(key) {
    return history[key] || []
  }

  function record(key, value) {
    var list = history[key]
    if (!list) { list = []; history[key] = list }
    list.push(Format.known(value) ? Number(value) : null)
    if (list.length > capacity) list.splice(0, list.length - capacity)
  }

  function resize(next) {
    capacity = next
    for (var key in history) {
      var list = history[key]
      if (list.length > capacity) list.splice(0, list.length - capacity)
    }
  }

  function remember(f) {
    var cpu = f.cpu || {}, usage = cpu.usage || {}
    record("cpu.user", usage.user)
    record("cpu.system", usage.system)
    record("cpu.total", usage.total)
    var mem = f.memory || {}
    record("mem.used", mem.total > 0 ? Format.number(mem.used) / mem.total * 100 : null)
    var net = f.network || {}
    record("net.rx", net.rx)
    record("net.tx", net.tx)
    var disks = f.disks || {}
    record("disk.read", disks.read)
    record("disk.write", disks.write)
    var drives = disks.drives || {}
    for (var name in drives) {
      record("disk.read:" + name, drives[name].read)
      record("disk.write:" + name, drives[name].write)
    }
    var cards = Hardware.gpus(f)
    for (var i = 0; i < cards.length; i++) {
      record("gpu.load:" + cards[i].id, cards[i].load)
      record("gpu.vram:" + cards[i].id, Hardware.vramPercent(cards[i]))
    }
    var power = Hardware.powerSummary(f)
    record("power.cpu", power.cpu)
    record("power.gpu", power.gpu)
    record("power.total", power.total)
    var battery = f.battery
    record("battery", battery && battery.present ? battery.percent : null)
    record("battery.charging", battery && battery.present && battery.status === "Charging" ? 1 : 0)
  }

  function take(line) {
    var text = String(line || "")
    if (text.length < 2 || text.length > 1048576) return
    var f
    try { f = JSON.parse(text) } catch (e) { return }
    if (!f || typeof f !== "object" || f.v !== 1) return
    var problems = Array.isArray(f.problems) ? f.problems : []
    problem = problems.join(" ")
    if (!f.cpu) { ready = false; return }   // the launcher's "cannot run" frame
    remember(f)
    frame = f
    ready = true
    revision++
  }

  // ---------------------------------------------------------- commands

  function send(command) {
    if (collector.running) collector.write(command + "\n")
  }

  // Open panels say which page they show and whether they list every
  // process; the collector does page extras only while someone looks.
  property var viewers: ({})
  property string focusSent: "none"
  property int detailSent: 0

  function watch(key, page, everything) {
    var next = {}
    for (var k in viewers) next[k] = viewers[k]
    if (page) next[key] = { page: String(page), all: !!everything, at: Date.now() }
    else delete next[key]
    viewers = next
    sync()
  }

  function sync() {
    var detail = 0, latest = null
    for (var k in viewers) {
      var v = viewers[k]
      detail = Math.max(detail, v.all ? 2 : 1)
      if (!latest || v.at > latest.at) latest = v
    }
    var focus = latest ? latest.page : "none"
    if (detail !== detailSent) { detailSent = detail; send("detail " + detail) }
    if (focus !== focusSent) { focusSent = focus; send("focus " + focus) }
  }

  function refresh() { send("refresh") }

  property real publicIpAsked: 0
  property bool publicIpOwed: false

  // At most once every ten minutes unless forced (r in the panel, IPC).
  function lookUpPublicIp(force) {
    var now = Date.now()
    if (!force && now - publicIpAsked < 600000) return
    publicIpAsked = now
    if (collector.running) send("publicip")
    else publicIpOwed = true
  }

  // Every widget copy reports its refresh and history settings; the
  // collector runs at the fastest and the graphs keep the longest.
  property var wishes: ({})

  function wish(key, refreshSeconds, spanSeconds) {
    var next = {}
    for (var k in wishes) next[k] = wishes[k]
    if (refreshSeconds === null) delete next[key]
    else next[key] = { refresh: refreshSeconds, span: spanSeconds }
    wishes = next
    var fastest = 0, longest = 0
    for (var w in wishes) {
      var r = Format.clamp(wishes[w].refresh, 0.1, 30), s = Format.clamp(wishes[w].span, 30, 3600)
      fastest = fastest > 0 ? Math.min(fastest, r) : r
      longest = Math.max(longest, s)
    }
    if (!(fastest > 0)) { fastest = 1; longest = 240 }
    historySeconds = longest
    var samples = Math.round(Format.clamp(longest / fastest, 60, 3600))
    if (samples !== capacity) resize(samples)
    if (Math.abs(fastest - interval) > 0.001) {
      interval = fastest
      send("interval " + fastest)
    }
  }

  // ---------------------------------------------------------- widgets

  property var widgets: []

  function adopt(widget) {
    if (widget && widgets.indexOf(widget) === -1) widgets = widgets.concat([widget])
  }

  function release(widget) {
    widgets = widgets.filter(function(w) { return w !== widget })
    watch(widget ? widget.key : "", "")
    wish(widget ? widget.key : "", null, null)
  }

  // Point every copy at the tab, then let the shell open the one on the
  // focused screen.
  function present(tab, how) {
    var live = widgets.filter(function(w) { return !!w })
    if (live.length === 0) return "no widget in the bar"
    if (tab) live.forEach(function(w) { w.showTab(String(tab)) })
    var shell = live[0].bar ? live[0].bar.shell : null
    if (!shell) return "no shell"
    if (how === "hide") shell.hide("cornerstone.stats")
    else if (how === "toggle") shell.toggle("cornerstone.stats", "{}")
    else shell.summon("cornerstone.stats", "{}")
    return "ok"
  }

  // --------------------------------------------------------- collector

  property int restarts: 0
  property bool stopping: false

  Process {
    id: collector
    command: [service.collectorPath, "--interval", String(service.interval)]
    running: true
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { service.take(line) } }
    // Problems travel in frames; stderr is left unbound so nothing piles up.
    onStarted: {
      service.detailSent = -1
      service.focusSent = ""
      service.sync()
      if (service.publicIpOwed) { service.publicIpOwed = false; service.send("publicip") }
    }
    onExited: function(code, status) {
      service.ready = false
      if (service.stopping) return
      service.restarts += 1
      if (!service.problem) service.problem = "The collector stopped (exit " + code + "); starting it again."
      retry.interval = Math.min(30000, 1000 * Math.pow(2, Math.min(5, service.restarts)))
      retry.restart()
    }
  }

  Timer {
    id: retry
    onTriggered: if (!service.stopping && !collector.running) collector.running = true
  }

  // A collector that has run a minute has earned a fresh backoff.
  Timer {
    interval: 60000
    repeat: true
    running: service.ready
    onTriggered: service.restarts = 0
  }

  // The theme's named colours are not in the shell's Color singleton.
  FileView {
    id: themeFile
    path: Commons.Color.currentThemePath + "/colors.toml"
    watchChanges: true
    printErrors: false
    onLoaded: service.themeColors = Palette.parseToml(text())
    onFileChanged: reload()
    onLoadFailed: service.themeColors = ({})
  }

  Connections {
    target: Commons.Color
    function onAccentChanged() { themeFile.reload() }
  }

  Component.onDestruction: {
    stopping = true
    retry.stop()
    if (collector.running) {
      collector.write("quit\n")
      collector.running = false
    }
  }

  IpcHandler {
    target: "cornerstone.stats"

    function status(): string {
      var f = service.frame || {}
      var cpu = f.cpu || {}, mem = f.memory || {}, net = f.network || {}
      var gpu = Hardware.mainGpus(f)[0]
      return JSON.stringify({
        ready: service.ready,
        seq: f.seq || 0,
        cpu: cpu.usage ? cpu.usage.total : null,
        cpuTemp: cpu.temp === undefined ? null : cpu.temp,
        memoryPercent: mem.total > 0 ? Math.round(mem.used / mem.total * 1000) / 10 : null,
        download: net.rx === undefined ? null : net.rx,
        upload: net.tx === undefined ? null : net.tx,
        gpu: gpu ? gpu.load : null,
        battery: f.battery && f.battery.present ? f.battery.percent : null,
        problem: service.problem,
        error: service.problem   // the name 2.x used
      })
    }
    function refresh(): string { service.lookUpPublicIp(true); service.refresh(); return "ok" }
    function show(tab: string): string { return service.present(tab, "show") }
    function toggle(tab: string): string { return service.present(tab, "toggle") }
    function hide(): string { return service.present("", "hide") }
  }
}

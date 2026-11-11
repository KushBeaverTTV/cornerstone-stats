import QtQuick
import QtQuick.Controls
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "lib/Format.js" as Format
import "lib/Hardware.js" as Hardware
import "lib/Prefs.js" as Prefs
import "ui/bar" as BarUi
import "ui/kit" as Kit

// The bar widget: a strip of readouts (one per module, several for GPUs and
// disks when asked) and the panel they open. Each copy on each screen has
// its own panel; all of them share the one Service for data.
Panel {
  id: widget

  moduleName: "cornerstone.stats"
  ipcTarget: ""

  readonly property var service: bar && bar.shell ? bar.shell.serviceFor("cornerstone.stats") : null
  readonly property string key: "widget-" + Math.random().toString(36).slice(2, 9)
  readonly property var frame: service ? service.frame : ({})
  readonly property bool vertical: bar ? bar.vertical : false
  readonly property int barSize: bar ? bar.barSize : Style.bar.sizeHorizontal

  // ------------------------------------------------------------ settings

  readonly property string unit: Prefs.temperatureUnit(settings)
  readonly property string labels: Prefs.labelMode(settings)
  readonly property int graphWidth: Math.round(Prefs.numberIn(settings, "graphWidth", 16, 120))
  readonly property bool hasGpu: !!(service && service.hasGpu)
  readonly property bool hasBattery: !!(service && service.hasBattery)
  readonly property var tabs: Prefs.tabs(settings, hasGpu)

  function flag(name) { return Prefs.flag(settings, name) }

  // Write one setting to this widget's entry in shell.json.
  function save(name, value) {
    var next = Prefs.withValue(settings, name, value)
    settings = next
    if (bar && bar.shell && typeof bar.shell.updateEntryInline === "function")
      bar.shell.updateEntryInline(moduleName, next)
  }

  function resetSettings() {
    var next = {}
    for (var k in Prefs.DEFAULTS) next[k] = Prefs.DEFAULTS[k]
    settings = next
    if (bar && bar.shell) bar.shell.updateEntryInline(moduleName, next)
  }

  // ------------------------------------------------------------ readouts

  // What the strip shows, in order: [{module, gpu, disk, show, tag}].
  readonly property var readouts: {
    var out = []
    var modules = Prefs.barModules(settings)
    for (var i = 0; i < modules.length; i++) {
      var id = modules[i]
      if (id === "battery" && !hasBattery) continue
      if (id === "gpu") {
        var cards = Hardware.barGpus(frame, settings.barGpus)
        cards.forEach(function(card) {
          out.push({ module: "gpu", gpu: String(card.id), disk: "", show: "", tag: cards.length > 1 ? Hardware.gpuTag(card) : "" })
          if (Prefs.flag(settings, "showGpuMemory") && Hardware.vramPercent(card) !== null)
            out.push({ module: "vram", gpu: String(card.id), disk: "", show: "", tag: "" })
        })
        continue
      }
      if (id === "disks") {
        var disks = Hardware.barDisksHere(frame, Prefs.barDisks(settings))
        disks.forEach(function(d) {
          out.push({ module: "disks", gpu: "", disk: d.disk, show: d.show, tag: disks.length > 1 ? Hardware.driveTag(d.disk) : "" })
        })
        continue
      }
      out.push({ module: id, gpu: "", disk: "", show: "", tag: "" })
    }
    return out.length > 0 ? out : [{ module: "cpu", gpu: "", disk: "", show: "", tag: "" }]
  }

  // ---------------------------------------------------------------- panel

  property string tab: "cpu"
  property int pointedTab: -1

  // The PROCESSES lists share one unfolded state and one search.
  property bool processesOpen: false
  property string processQuery: ""
  property bool typing: false
  property var searchBox: null

  // The Apps tab lists every process, so it wants the full list too.
  readonly property bool wantsEverything: tab === "apps" || processesOpen

  function showAllProcesses(on) {
    processesOpen = on === true
    if (!processesOpen) { processQuery = ""; typing = false }
    if (processesOpen) Qt.callLater(function() { if (widget.searchBox) widget.searchBox.forceActiveFocus() })
  }

  function showTab(id) {
    var next = Prefs.tabOf(String(id || ""))
    var all = tabs.concat(["settings"])
    if (all.indexOf(next) === -1) next = all[0]
    if (next === tab) return
    tab = next
    scrollTo(0)
  }

  // A click on a readout opens its tab; a second click on it closes.
  function clicked(module) {
    var target = Prefs.tabOf(module)
    if (opened && tab === target) { close(); return }
    showTab(target)
    if (!opened) open()
  }

  function step(delta) {
    var all = tabs.concat(["settings"])
    var at = Math.max(0, all.indexOf(tab))
    showTab(all[(at + delta + all.length) % all.length])
  }

  function scrollTo(y) {
    var flick = scroller.contentItem
    if (flick && flick.contentY !== undefined) flick.contentY = y
  }

  function scroll(rows) {
    var flick = scroller.contentItem
    if (!flick || flick.contentY === undefined) return
    var bottom = Math.max(0, flick.contentHeight - flick.height)
    flick.contentY = Math.max(0, Math.min(bottom, flick.contentY + rows * Style.space(48)))
  }

  function refresh() {
    if (!service) return
    if (flag("publicIp")) service.lookUpPublicIp(true)
    service.refresh()
  }

  function report() {
    if (!service) return
    service.wish(key, Prefs.numberIn(settings, "refreshSeconds", 0.1, 30), Prefs.numberIn(settings, "historySeconds", 30, 3600))
    service.watch(key, opened ? tab : "", wantsEverything)
  }

  onOpenedChanged: {
    if (!opened) { typing = false; pointedTab = -1 }
    report()
  }
  onTabChanged: report()
  onWantsEverythingChanged: report()
  onSettingsChanged: report()
  onTabsChanged: if (tabs.concat(["settings"]).indexOf(tab) === -1) tab = tabs[0]
  onServiceChanged: if (service) { service.adopt(widget); report() }
  Component.onCompleted: if (service) { service.adopt(widget); report() }
  Component.onDestruction: if (service) service.release(widget)

  implicitWidth: strip.implicitWidth + Style.space(2)
  implicitHeight: vertical ? strip.implicitHeight : barSize

  Grid {
    id: strip
    anchors.centerIn: parent
    columns: widget.vertical ? 1 : widget.readouts.length
    rows: widget.vertical ? widget.readouts.length : 1

    Repeater {
      model: (widget.readouts).length

      BarUi.Readout {
        required property int index
        readonly property var modelData: (widget.readouts)[index] || ({})
        host: widget
        bar: widget.bar
        module: modelData.module
        gpuId: modelData.gpu
        disk: modelData.disk
        diskShow: modelData.show
        tag: modelData.tag
        onPressed: function(button) {
          if (button === Qt.LeftButton) widget.clicked(module)
          else if (button === Qt.RightButton && widget.bar) widget.bar.run("omarchy-launch-or-focus-tui btop")
          else if (button === Qt.MiddleButton) widget.refresh()
        }
      }
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: widget
    owner: widget
    bar: widget.bar
    open: widget.opened
    focusTarget: keys
    contentWidth: panel.fittedContentWidth(Math.max(Style.space(372), Math.ceil(tabStrip.neededWidth) + Style.spacing.popupPadding * 2))
    contentHeight: panel.fittedContentHeight(tabStrip.height + Style.space(10) + status.height + page.implicitHeight)

    PanelKeyCatcher {
      id: keys
      anchors.fill: parent
      blocked: widget.typing

      onMoveRequested: function(dx, dy) {
        if (dx !== 0) widget.step(dx)
        else widget.scroll(dy)
      }
      onCloseRequested: widget.close()
      onTabRequested: function(direction) { widget.switchPanel(direction) }
      onTextKey: function(text) {
        var n = parseInt(text, 10)
        if (n >= 1 && n <= widget.tabs.length) widget.showTab(widget.tabs[n - 1])
        else if (text === "r") widget.refresh()
        else if (text === "s" || text === ",") widget.showTab("settings")
        else if (text === "/") {
          widget.showAllProcesses(true)
          Qt.callLater(function() { if (widget.searchBox) widget.searchBox.forceActiveFocus() })
        }
      }

      Kit.Tabs {
        id: tabStrip
        anchors.top: parent.top
        tabs: widget.tabs
        current: widget.tab
        pointed: widget.pointedTab
        onPicked: function(id) { widget.showTab(id) }
        onPointedAt: function(index, inside) {
          if (inside) widget.pointedTab = index
          else if (widget.pointedTab === index) widget.pointedTab = -1
        }
      }

      Kit.Label {
        id: status
        anchors.top: tabStrip.bottom
        width: parent.width
        readonly property string message: !widget.service ? "The Stats service is not loaded. Turn the plugin off and on again."
          : (widget.service.problem !== "" ? widget.service.problem
          : (!widget.service.ready ? "Starting the collector…" : ""))
        visible: message !== ""
        height: visible ? implicitHeight + Style.space(10) : 0
        verticalAlignment: Text.AlignBottom
        text: message
        fade: 0.6
        wrapMode: Text.WordWrap
        font.pixelSize: Style.font.bodySmall
      }

      ScrollView {
        id: scroller
        anchors.top: status.bottom
        anchors.topMargin: Style.space(10)
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        clip: true
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ScrollBar.vertical.policy: page.implicitHeight > height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff

        Binding {
          target: scroller.contentItem
          property: "interactive"
          value: page.implicitHeight > scroller.height
        }

        Loader {
          id: page
          width: scroller.availableWidth
          active: widget.opened || panel.visible
          source: Qt.resolvedUrl("ui/pages/" + Prefs.pageOf(widget.tab))
          onLoaded: item.host = widget
        }
      }
    }
  }
}

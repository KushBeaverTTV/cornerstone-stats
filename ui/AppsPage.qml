import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import "../Model.js" as Model

// Apps tab (kushbeaverttv fork): a task manager in the Mission Center mould.
// Top: open apps (windows grouped by the program behind them) with CPU and
// memory of the whole program. Below: every process, sortable and searchable.
// Every row can be ended; apps can also be brought here or closed politely.
Column {
  id: root

  property var service: null
  property var host: null
  property var settings: ({})
  property string temperatureUnit: "Celsius"
  property bool publicIpEnabled: true
  property color foreground: Commons.Color.popups.text
  property string fontFamily: Style.font.family

  readonly property string pluginDir: Qt.resolvedUrl("..").toString().replace(/^file:\/\//, "").replace(/\/$/, "")
  readonly property string helper: pluginDir + "/kbstats-helper.py"
  readonly property string endBin: Quickshell.env("HOME") + "/.local/bin/taskman-end"
  readonly property var snap: service ? service.snapshot : ({})
  readonly property var procs: snap.procs || null
  readonly property color accent: service ? service.series1 : Commons.Color.accent
  readonly property color warn: service ? service.warn : Commons.Color.urgent
  readonly property color danger: service ? service.danger : Commons.Color.urgent
  readonly property real memTotal: Math.max(1, Model.num((snap.mem || {}).total, 1))

  property var apps: []
  property string sortKey: "mem"
  property string query: ""
  property string confirmKey: ""   // "app:<pid>" or "proc:<pid>" awaiting End task / Force kill
  property int maxRows: 150

  spacing: Style.space(10)
  width: parent ? parent.width : implicitWidth

  // ------------------------------------------------------------ data

  // The page's service arrives through a Binding after it loads, so the full
  // process list is requested when it shows up, and given back on unload.
  property var heldService: null
  function holdFull() {
    if (heldService === service) return
    if (heldService) heldService.releaseFull()
    heldService = service
    if (service) service.acquireFull()
  }
  onServiceChanged: holdFull()
  Component.onCompleted: { holdFull(); appsProc.running = true }
  Component.onDestruction: if (heldService) heldService.releaseFull()

  Process {
    id: appsProc
    command: ["/usr/bin/python3", "-I", root.helper, "apps"]
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var d = JSON.parse(String(this.text))
          root.apps = Array.isArray(d.apps) ? d.apps.slice(0, 60) : []
        } catch (e) {}
      }
    }
  }
  Timer {
    interval: 2000
    running: root.visible
    repeat: true
    onTriggered: if (!appsProc.running) appsProc.running = true
  }
  Timer { id: soon; interval: 600; onTriggered: if (!appsProc.running) appsProc.running = true }
  Timer { id: confirmReset; interval: 4000; onTriggered: root.confirmKey = "" }

  function run(args) { Quickshell.execDetached(args); root.confirmKey = ""; soon.restart() }
  function endPid(pid, force) {
    var p = parseInt(pid)
    if (p > 1) run(force ? [endBin, String(p), "--force"] : [endBin, String(p)])
  }
  function ask(key) { root.confirmKey = key; confirmReset.restart() }

  // Program names that differ from their desktop entry's.
  readonly property var aliases: ({ "chrome": "google-chrome", "brave": "brave-browser", "code": "code", "electron": "" })
  function classCount(app) {
    var classes = {}
    for (var i = 0; i < (app.windows || []).length; i++) classes[String(app.windows[i].class)] = true
    return Object.keys(classes).length
  }
  function programEntry(app) {
    var c = String(app.comm || "")
    var e = DesktopEntries.heuristicLookup(aliases[c] !== undefined && aliases[c] !== "" ? aliases[c] : c)
    return e || null
  }
  function entryFor(app) {
    // Several web apps can share one browser process: then it is the browser.
    if (classCount(app) > 1) return programEntry(app)
    var w = app && app.windows && app.windows.length > 0 ? app.windows[0] : null
    var e = w ? DesktopEntries.heuristicLookup(String(w.class)) : null
    return e || programEntry(app)
  }
  function appName(app) {
    var e = entryFor(app)
    if (e && e.name) return e.name
    var c = String(app.comm || "App")
    return c.charAt(0).toUpperCase() + c.slice(1)
  }
  function iconFor(app) {
    var e = entryFor(app)
    var icon = e && e.icon ? String(e.icon) : String(app.comm || "")
    if (icon.indexOf("/") === 0) return "file://" + icon
    return Quickshell.iconPath(icon, true)
  }
  function appDetail(app) {
    var w = app.windows || []
    var parts = []
    if (classCount(app) > 1) {
      var names = []
      for (var j = 0; j < w.length; j++) {
        var we = DesktopEntries.heuristicLookup(String(w[j].class))
        var nm = we && we.name ? we.name : String(w[j].title || w[j].class)
        if (names.indexOf(nm) < 0) names.push(nm)
      }
      parts.push(names.join(", "))
    } else if (w.length > 1) parts.push(w.length + " windows")
    else if (w.length === 1 && w[0].title && String(w[0].title) !== appName(app)) parts.push(String(w[0].title))
    var hidden = 0
    for (var i = 0; i < w.length; i++) if (w[i].hidden) hidden++
    if (hidden > 0) parts.push(hidden === w.length ? "hidden" : hidden + " hidden")
    if (String(app.exe).indexOf("(deleted)") >= 0) parts.push("uninstalled, still running")
    return parts.join(" · ")
  }

  readonly property var rows: {
    var list = procs && Array.isArray(procs.all) ? procs.all.slice() : []
    var q = query.trim().toLowerCase()
    if (q.length > 0) list = list.filter(function(p) { return String(p.name || "").toLowerCase().indexOf(q) >= 0 })
    var k = sortKey
    list.sort(function(a, b) {
      if (k === "name") return String(a.name || "").localeCompare(String(b.name || ""))
      if (k === "io") return (Model.num(b.read) + Model.num(b.write)) - (Model.num(a.read) + Model.num(a.write))
      return Model.num(b[k]) - Model.num(a[k])
    })
    return list
  }

  // ------------------------------------------------------------ bits

  component Pill: Rectangle {
    id: pill
    property string label: ""
    property color tint: root.foreground
    property bool strong: false
    signal clicked()
    width: pillText.implicitWidth + Style.space(12)
    height: Style.space(18)
    radius: height / 2
    color: strong ? Qt.rgba(tint.r, tint.g, tint.b, pillMouse.containsMouse ? 1 : 0.82)
                  : Util.alpha(root.foreground, pillMouse.containsMouse ? 0.3 : 0.16)
    Text {
      id: pillText
      anchors.centerIn: parent
      textFormat: Text.PlainText
      text: pill.label
      color: pill.strong ? "#FFFFFF" : root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      font.bold: true
    }
    MouseArea {
      id: pillMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: pill.clicked()
    }
  }

  component Figure: Text {
    property real w: Style.space(62)
    width: w
    horizontalAlignment: Text.AlignRight
    textFormat: Text.PlainText
    color: root.foreground
    opacity: 0.9
    font.family: root.fontFamily
    font.pixelSize: Style.font.bodySmall
  }

  function pct(v) { var n = Model.num(v); return n >= 10 ? Math.round(n) + "%" : n.toFixed(1) + "%" }
  function bytes(v) { return Model.bytesText(v) }
  function rate(v) { return Model.num(v) > 0 ? Model.rateParts(v).value + " " + Model.rateParts(v).unit : "–" }

  // ------------------------------------------------------------ apps

  Card {
    foreground: root.foreground

    Item {
      width: parent.width
      height: Style.space(18)
      SectionTitle {
        text: "Open apps" + (root.apps.length > 0 ? " · " + root.apps.length : "")
        fontFamily: root.fontFamily
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Figure { text: "CPU"; opacity: 0.5; font.bold: true; font.pixelSize: Style.font.caption }
        Figure { text: "Memory"; opacity: 0.5; font.bold: true; font.pixelSize: Style.font.caption }
      }
    }

    Text {
      visible: root.apps.length === 0
      textFormat: Text.PlainText
      text: "Looking…"
      color: root.foreground
      opacity: 0.45
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: root.apps
      delegate: Item {
        id: appRow
        required property var modelData
        readonly property string key: "app:" + modelData.pid
        readonly property bool confirming: root.confirmKey === key
        readonly property bool hot: appHover.containsMouse || confirming
        width: parent.width
        height: Style.space(38)

        MouseArea { id: appHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
        Rectangle {
          anchors.fill: parent
          anchors.margins: -Style.space(3)
          radius: Style.space(6)
          visible: appRow.hot || appRow.modelData.focused
          color: Util.alpha(root.foreground, appRow.hot ? 0.08 : 0.04)
        }

        Image {
          id: appIcon
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(24); height: width
          sourceSize.width: 48; sourceSize.height: 48
          source: root.iconFor(appRow.modelData)
          fillMode: Image.PreserveAspectFit
          smooth: true
          asynchronous: true
          // No icon found: a letter tile instead of a gap.
          Rectangle {
            anchors.fill: parent
            visible: appIcon.status !== Image.Ready
            radius: Style.space(6)
            color: Util.alpha(root.foreground, 0.14)
            Text {
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: root.appName(appRow.modelData).charAt(0).toUpperCase()
              color: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              font.bold: true
            }
          }
        }

        Column {
          anchors.left: appIcon.right
          anchors.leftMargin: Style.space(8)
          anchors.right: appRow.hot ? appPills.left : appFigs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(1)
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.appName(appRow.modelData)
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            font.bold: appRow.modelData.focused === true
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.appDetail(appRow.modelData)
            visible: text.length > 0
            color: String(appRow.modelData.exe).indexOf("(deleted)") >= 0 ? root.warn : root.foreground
            opacity: 0.5
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
          }
        }

        Row {
          id: appPills
          visible: appRow.hot
          anchors.right: appFigs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)
          Pill {
            visible: !appRow.confirming
            label: "Bring here"
            onClicked: {
              var w = appRow.modelData.windows || []
              if (w.length > 0) root.run(["/usr/bin/python3", "-I", root.helper, "summon", String(w[0].address)])
            }
          }
          Pill {
            visible: !appRow.confirming
            label: "Close"
            onClicked: root.run(["/usr/bin/python3", "-I", root.helper, "close", String(appRow.modelData.pid)])
          }
          Pill { visible: !appRow.confirming; label: "End"; onClicked: root.ask(appRow.key) }
          Pill { visible: appRow.confirming; label: "End task"; strong: true; tint: "#E8740F"; onClicked: root.endPid(appRow.modelData.pid, false) }
          Pill { visible: appRow.confirming; label: "Force kill"; strong: true; tint: "#E01414"; onClicked: root.endPid(appRow.modelData.pid, true) }
        }

        Row {
          id: appFigs
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(8)
          Figure {
            text: root.pct(appRow.modelData.cpu)
            color: Model.num(appRow.modelData.cpu) >= 80 ? root.danger : root.foreground
          }
          Figure {
            text: root.bytes(appRow.modelData.mem)
            color: Model.num(appRow.modelData.mem) / root.memTotal >= 0.25 ? root.warn : root.foreground
          }
        }
      }
    }
  }

  // ------------------------------------------------------------ processes

  Card {
    foreground: root.foreground

    Item {
      width: parent.width
      height: Style.space(18)
      SectionTitle {
        text: "All processes" + (root.procs ? " · " + Model.num(root.procs.total) : "")
        fontFamily: root.fontFamily
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Repeater {
          model: [{ k: "cpu", t: "CPU" }, { k: "mem", t: "Memory" }, { k: "io", t: "Disk" }]
          delegate: Figure {
            required property var modelData
            text: modelData.t + (root.sortKey === modelData.k ? " ▾" : "")
            opacity: root.sortKey === modelData.k ? 0.95 : 0.5
            font.bold: true
            font.pixelSize: Style.font.caption
            MouseArea {
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: root.sortKey = modelData.k
            }
          }
        }
      }
    }

    KeyField {
      width: parent.width
      placeholderText: "Search processes"
      text: root.query
      foreground: root.foreground
      verticalPadding: Style.space(4)
      font.pixelSize: Style.font.bodySmall
      onTextChanged: if (root.query !== text) root.query = text
      onActiveFocusChanged: if (root.host) root.host.searchActive = activeFocus
      Keys.onEscapePressed: function(event) {
        if (text !== "") { text = ""; event.accepted = true; return }
        focus = false
        event.accepted = true
      }
    }

    Text {
      visible: root.rows.length === 0
      textFormat: Text.PlainText
      text: root.procs ? (root.query ? "No match" : "Collecting…") : "Measuring…"
      color: root.foreground
      opacity: 0.45
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: Math.min(root.rows.length, root.maxRows)
      delegate: Item {
        id: procRow
        required property int index
        readonly property var p: root.rows[index] || ({})
        readonly property string key: "proc:" + p.pid
        readonly property bool confirming: root.confirmKey === key
        readonly property bool hot: rowHover.containsMouse || confirming
        width: parent.width
        height: Style.space(20)

        MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
        Rectangle {
          anchors.fill: parent
          anchors.leftMargin: -Style.space(4)
          anchors.rightMargin: -Style.space(4)
          radius: Style.space(4)
          visible: procRow.hot
          color: Util.alpha(root.foreground, 0.07)
        }

        Row {
          anchors.left: parent.left
          anchors.right: procRow.hot ? procPills.left : procFigs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(6)
          Text {
            textFormat: Text.PlainText
            text: String(procRow.p.name || "")
            color: root.foreground
            opacity: 0.9
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            elide: Text.ElideRight
            width: Math.min(implicitWidth, parent.width - Style.space(30))
          }
          Text {
            visible: Model.num(procRow.p.count, 1) > 1
            textFormat: Text.PlainText
            text: "×" + Model.num(procRow.p.count, 1)
            color: root.foreground
            opacity: 0.4
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            anchors.verticalCenter: parent.verticalCenter
          }
        }

        Row {
          id: procPills
          visible: procRow.hot
          anchors.right: procFigs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)
          Pill { visible: !procRow.confirming; label: "End"; onClicked: root.ask(procRow.key) }
          Pill { visible: procRow.confirming; label: "End task"; strong: true; tint: "#E8740F"; onClicked: root.endPid(procRow.p.pid, false) }
          Pill { visible: procRow.confirming; label: "Force kill"; strong: true; tint: "#E01414"; onClicked: root.endPid(procRow.p.pid, true) }
        }

        Row {
          id: procFigs
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(8)
          Figure { text: root.pct(procRow.p.cpu) }
          Figure { text: root.bytes(procRow.p.mem) }
          Figure { text: root.rate(Model.num(procRow.p.read) + Model.num(procRow.p.write)) }
        }
      }
    }

    Text {
      visible: root.rows.length > root.maxRows
      textFormat: Text.PlainText
      text: "and " + (root.rows.length - root.maxRows) + " more. Search to narrow it down."
      color: root.foreground
      opacity: 0.45
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }
  }
}

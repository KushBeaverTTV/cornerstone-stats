import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../kit"

// A task manager. On top, the open apps: windows grouped by the program
// that owns them, with that program's CPU and memory (children included),
// and Bring here, Close and End. Below, every process, sortable by CPU,
// memory or disk and searchable, each one endable.
Page {
  id: page

  readonly property var apps: Array.isArray(frame.apps) ? frame.apps : []
  readonly property bool appsKnown: Array.isArray(frame.apps)
  readonly property real memoryTotal: Math.max(1, Format.number(frame.memory ? frame.memory.total : 1, 1))
  readonly property string tool: service ? service.collectorPath : ""

  property string sortKey: "mem"
  property string query: ""
  property int limit: 150

  readonly property var everything: procs && Array.isArray(procs.all) ? procs.all : []
  readonly property var rows: Hardware.processRows(everything, query, sortKey)

  // Ask for fresh figures shortly after an action, rather than waiting.
  function act(argv) {
    Quickshell.execDetached(argv)
    soon.restart()
  }
  Timer { id: soon; interval: 700; onTriggered: if (page.service) page.service.refresh() }

  // ------------------------------------------------------- names, icons

  // Programs whose name differs from their desktop entry's.
  readonly property var renamed: ({ "chrome": "google-chrome", "brave": "brave-browser" })

  function classes(app) {
    var seen = []
    ;(app.windows || []).forEach(function(w) { if (seen.indexOf(String(w.class)) === -1) seen.push(String(w.class)) })
    return seen
  }
  function programEntry(app) {
    var comm = String(app.comm || "")
    return DesktopEntries.heuristicLookup(renamed[comm] || comm) || null
  }
  // One window class: that app. Several (web apps sharing a browser): the program.
  function desktopEntry(app) {
    if (classes(app).length > 1) return programEntry(app)
    var first = app.windows && app.windows.length > 0 ? app.windows[0] : null
    return (first ? DesktopEntries.heuristicLookup(String(first.class)) : null) || programEntry(app)
  }
  function appName(app) {
    var e = desktopEntry(app)
    return e && e.name ? String(e.name) : Format.capitalize(app.comm || "App")
  }
  function appIcon(app) {
    var e = desktopEntry(app)
    var icon = e && e.icon ? String(e.icon) : String(app.comm || "")
    return icon.charAt(0) === "/" ? "file://" + icon : Quickshell.iconPath(icon, true)
  }
  function appNote(app) {
    var windows = app.windows || []
    var bits = []
    if (classes(app).length > 1) {
      var names = []
      windows.forEach(function(w) {
        var e = DesktopEntries.heuristicLookup(String(w.class))
        var n = e && e.name ? String(e.name) : String(w.title || w.class)
        if (names.indexOf(n) === -1) names.push(n)
      })
      bits.push(names.join(", "))
    } else if (windows.length > 1) {
      bits.push(windows.length + " windows")
    } else if (windows.length === 1 && windows[0].title && String(windows[0].title) !== appName(app)) {
      bits.push(String(windows[0].title))
    }
    var hidden = windows.filter(function(w) { return w.hidden }).length
    if (hidden > 0) bits.push(hidden === windows.length ? "hidden" : hidden + " hidden")
    if (app.deleted) bits.push("old version still running")
    return bits.join(" · ")
  }

  // ------------------------------------------------------------ open apps

  Block {
    Item {
      width: parent.width
      height: Style.space(18)
      Heading {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: "Open apps" + (page.apps.length > 0 ? " · " + page.apps.length : "")
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Repeater {
          model: ["CPU", "Memory"]
          Label {
            required property string modelData
            width: Style.space(62)
            horizontalAlignment: Text.AlignRight
            text: modelData
            fade: 0.5
            font.pixelSize: Style.font.caption
            font.bold: true
          }
        }
      }
    }

    Label {
      visible: page.apps.length === 0
      text: page.appsKnown ? "No open windows" : "Looking…"
      fade: 0.45
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: (page.apps).length

      Item {
        id: app
        required property int index
        readonly property var modelData: (page.apps)[index] || ({})
        readonly property var a: modelData
        readonly property bool hot: hover.containsMouse || ender.asking

        width: parent.width
        height: Style.space(38)

        MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }

        Rectangle {
          anchors.fill: parent
          anchors.margins: -Style.space(3)
          radius: Style.space(6)
          visible: app.hot || app.a.focused === true
          color: Util.alpha(page.ink, app.hot ? 0.08 : 0.04)
        }

        Image {
          id: icon
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(24)
          height: width
          sourceSize.width: 48
          sourceSize.height: 48
          source: page.appIcon(app.a)
          fillMode: Image.PreserveAspectFit
          asynchronous: true

          // No icon: the first letter on a tile, not a gap.
          Rectangle {
            anchors.fill: parent
            visible: icon.status !== Image.Ready
            radius: Style.space(6)
            color: Util.alpha(page.ink, 0.14)
            Label {
              anchors.centerIn: parent
              text: page.appName(app.a).charAt(0).toUpperCase()
              font.bold: true
            }
          }
        }

        Column {
          anchors.left: icon.right
          anchors.leftMargin: Style.space(8)
          anchors.right: app.hot ? actions.left : figures.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(1)

          Label {
            width: parent.width
            text: page.appName(app.a)
            font.bold: app.a.focused === true
          }
          Label {
            width: parent.width
            visible: text !== ""
            text: page.appNote(app.a)
            color: app.a.deleted ? page.warn : page.ink
            fade: 0.5
            font.pixelSize: Style.font.caption
          }
        }

        Row {
          id: actions
          visible: app.hot
          anchors.right: figures.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)

          Chip {
            visible: !ender.asking && (app.a.windows || []).length > 0
            text: "Bring here"
            onClicked: page.act([page.tool, "summon", String(app.a.windows[0].address)])
          }
          Chip {
            visible: !ender.asking
            text: "Close"
            onClicked: page.act([page.tool, "close", String(app.a.pid)])
          }
          Ender {
            id: ender
            pid: Format.number(app.a.pid)
            shown: app.hot
            onDone: soon.restart()
          }
        }

        Row {
          id: figures
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(8)
          Label {
            width: Style.space(62)
            horizontalAlignment: Text.AlignRight
            text: Format.share(app.a.cpu)
            color: Format.number(app.a.cpu) >= 80 ? page.danger : page.ink
            fade: 0.9
            font.pixelSize: Style.font.bodySmall
          }
          Label {
            width: Style.space(62)
            horizontalAlignment: Text.AlignRight
            text: Format.sizeText(app.a.mem)
            color: Format.number(app.a.mem) / page.memoryTotal >= 0.25 ? page.warn : page.ink
            fade: 0.9
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }
  }

  // --------------------------------------------------------- every process

  Block {
    Item {
      width: parent.width
      height: Style.space(18)
      Heading {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: "All processes" + (page.procs ? " · " + Format.number(page.procs.count) : "")
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Repeater {
          model: [{ key: "cpu", title: "CPU" }, { key: "mem", title: "Memory" }, { key: "io", title: "Disk" }]
          Label {
            required property var modelData
            width: Style.space(62)
            horizontalAlignment: Text.AlignRight
            text: modelData.title + (page.sortKey === modelData.key ? " ▾" : "")
            fade: page.sortKey === modelData.key ? 0.95 : (sortMouse.containsMouse ? 0.8 : 0.5)
            font.pixelSize: Style.font.caption
            font.bold: true
            MouseArea {
              id: sortMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: page.sortKey = modelData.key
            }
          }
        }
      }
    }

    Entry {
      id: appsSearch
      placeholder: "Search processes"
      // "/" in the panel lands here on this tab.
      Connections { target: page; function onHostChanged() { if (page.host) page.host.searchBox = appsSearch } }
      Component.onDestruction: if (page.host && page.host.searchBox === appsSearch) page.host.searchBox = null
      text: page.query
      onTextChanged: if (page.query !== text) page.query = text
      onActiveFocusChanged: if (page.host) page.host.typing = activeFocus
    }

    Label {
      visible: page.rows.length === 0
      text: page.procs ? (page.query ? "No match" : "Collecting…") : "Measuring…"
      fade: 0.45
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: Math.min(page.rows.length, page.limit)

      Item {
        id: proc
        required property int index
        readonly property var p: page.rows[index] || ({})
        readonly property bool hot: rowHover.containsMouse || endRow.asking

        width: parent.width
        height: Style.space(20)

        MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
        Rectangle {
          visible: proc.hot
          anchors.fill: parent
          anchors.leftMargin: -Style.space(4)
          anchors.rightMargin: -Style.space(4)
          radius: Style.space(4)
          color: Util.alpha(page.ink, 0.07)
        }

        Row {
          anchors.left: parent.left
          anchors.right: proc.hot ? endRow.left : numbers.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(6)
          Label {
            width: Math.min(implicitWidth, parent.width - Style.space(30))
            text: String(proc.p.name || "")
            fade: 0.9
          }
          Label {
            visible: Format.number(proc.p.count, 1) > 1
            anchors.verticalCenter: parent.verticalCenter
            text: "×" + Format.number(proc.p.count, 1)
            fade: 0.4
            font.pixelSize: Style.font.caption
          }
        }

        Ender {
          id: endRow
          anchors.right: numbers.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          pid: Format.number(proc.p.pid)
          shown: rowHover.containsMouse
          onDone: soon.restart()
        }

        Row {
          id: numbers
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(8)
          Repeater {
            model: [Format.share(proc.p.cpu), Format.sizeText(proc.p.mem), Format.rateOrDash(Format.number(proc.p.read) + Format.number(proc.p.write))]
            Label {
              required property string modelData
              width: Style.space(62)
              horizontalAlignment: Text.AlignRight
              text: modelData
              fade: 0.9
              font.pixelSize: Style.font.bodySmall
            }
          }
        }
      }
    }

    Label {
      visible: page.rows.length > page.limit
      text: "and " + (page.rows.length - page.limit) + " more. Search to narrow it down."
      fade: 0.45
      font.pixelSize: Style.font.caption
    }
  }
}

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import "../Model.js" as Model

// Services tab (kushbeaverttv fork): background services, Mission Center
// style. "Yours" are the units in ~/.config/systemd/user (Cornerstone, HQ,
// bots); "Desktop" the rest of your session's; "System" the machine's, whose
// start/stop/restart asks for your password through the usual prompt.
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
  readonly property color okColor: "#3FB950"
  readonly property color warnColor: service ? service.warn : Commons.Color.urgent
  readonly property color badColor: service ? service.danger : Commons.Color.urgent

  property var userUnits: []
  property var systemUnits: []
  property string scope: "yours"     // yours | desktop | system
  property string query: ""
  property bool showStopped: false
  property string busyUnit: ""
  property bool loaded: false

  spacing: Style.space(10)
  width: parent ? parent.width : implicitWidth

  Component.onCompleted: listProc.running = true

  Process {
    id: listProc
    command: ["/usr/bin/python3", "-I", root.helper, "services"]
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var d = JSON.parse(String(this.text))
          root.userUnits = Array.isArray(d.user) ? d.user.slice(0, 400) : []
          root.systemUnits = Array.isArray(d.system) ? d.system.slice(0, 600) : []
          root.loaded = true
        } catch (e) {}
      }
    }
  }
  Timer {
    interval: 3000
    running: root.visible
    repeat: true
    onTriggered: if (!listProc.running) listProc.running = true
  }
  Timer { id: soon; interval: 900; onTriggered: { root.busyUnit = ""; if (!listProc.running) listProc.running = true } }

  function act(u, action) {
    root.busyUnit = u.unit
    Quickshell.execDetached(["/usr/bin/python3", "-I", root.helper, "svc", u.scope, action, u.unit])
    soon.restart()
  }
  function logs(u) { Quickshell.execDetached(["/usr/bin/python3", "-I", root.helper, "logs", u.scope, u.unit]) }

  function rank(u) {
    if (u.active === "failed") return 0
    if (u.active === "active" || u.active === "activating" || u.active === "reloading") return 1
    return 2
  }
  function pool() {
    if (scope === "system") return systemUnits
    return userUnits.filter(function(u) { return scope === "yours" ? u.mine === true : u.mine !== true })
  }
  readonly property var shown: {
    var list = pool().slice()
    var q = query.trim().toLowerCase()
    if (q.length > 0)
      list = list.filter(function(u) {
        return String(u.name).toLowerCase().indexOf(q) >= 0 || String(u.description).toLowerCase().indexOf(q) >= 0
      })
    else if (!showStopped && scope !== "yours")
      list = list.filter(function(u) { return root.rank(u) < 2 })
    list.sort(function(a, b) {
      var r = root.rank(a) - root.rank(b)
      if (r !== 0) return r
      return String(a.name).localeCompare(String(b.name))
    })
    return list
  }
  readonly property int failedCount: {
    var n = 0
    var all = userUnits.concat(systemUnits)
    for (var i = 0; i < all.length; i++) if (all[i].active === "failed") n++
    return n
  }

  component Pill: Rectangle {
    id: pill
    property string label: ""
    property bool selected: false
    property color tint: root.foreground
    property bool strong: false
    signal clicked()
    width: pillText.implicitWidth + Style.space(14)
    height: Style.space(20)
    radius: height / 2
    color: strong ? Qt.rgba(tint.r, tint.g, tint.b, pillMouse.containsMouse ? 1 : 0.82)
         : selected ? Util.alpha(root.foreground, 0.24)
         : Util.alpha(root.foreground, pillMouse.containsMouse ? 0.2 : 0.09)
    Text {
      id: pillText
      anchors.centerIn: parent
      textFormat: Text.PlainText
      text: pill.label
      color: pill.strong ? "#FFFFFF" : root.foreground
      opacity: pill.selected || pill.strong || pillMouse.containsMouse ? 1 : 0.75
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

  // ------------------------------------------------------------ filters

  Row {
    spacing: Style.space(6)
    Pill { label: "Yours"; selected: root.scope === "yours"; onClicked: root.scope = "yours" }
    Pill { label: "Desktop"; selected: root.scope === "desktop"; onClicked: root.scope = "desktop" }
    Pill { label: "System"; selected: root.scope === "system"; onClicked: root.scope = "system" }
    Pill {
      visible: root.scope !== "yours"
      label: root.showStopped ? "Hide stopped" : "Show stopped"
      onClicked: root.showStopped = !root.showStopped
    }
  }

  Text {
    visible: root.failedCount > 0
    width: parent.width
    textFormat: Text.PlainText
    text: root.failedCount + (root.failedCount === 1 ? " service has failed" : " services have failed") + " (red, listed first)"
    color: root.badColor
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  KeyField {
    width: parent.width
    placeholderText: "Search services"
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

  // ------------------------------------------------------------ list

  Card {
    foreground: root.foreground

    Text {
      visible: root.shown.length === 0
      textFormat: Text.PlainText
      text: !root.loaded ? "Loading…" : (root.query ? "No match" : "Nothing running here")
      color: root.foreground
      opacity: 0.45
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: Math.min(root.shown.length, 200)
      delegate: Item {
        id: row
        required property int index
        readonly property var u: root.shown[index] || ({})
        readonly property bool running: u.active === "active" || u.active === "activating" || u.active === "reloading"
        readonly property bool failed: u.active === "failed"
        readonly property bool busy: root.busyUnit === u.unit
        readonly property bool hot: rowHover.containsMouse
        width: parent.width
        height: Style.space(36)

        MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
        Rectangle {
          anchors.fill: parent
          anchors.margins: -Style.space(3)
          radius: Style.space(6)
          visible: row.hot || row.failed
          color: row.failed ? Qt.rgba(root.badColor.r, root.badColor.g, root.badColor.b, 0.12) : Util.alpha(root.foreground, 0.07)
        }

        Rectangle {
          id: dot
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(8); height: width; radius: width / 2
          color: row.busy ? root.warnColor : row.failed ? root.badColor : row.running ? root.okColor : Util.alpha(root.foreground, 0.3)
        }

        Column {
          anchors.left: dot.right
          anchors.leftMargin: Style.space(10)
          anchors.right: row.hot ? actions.left : figs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(1)
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: String(row.u.name || "")
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: String(row.u.description || "")
            color: root.foreground
            opacity: 0.5
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
          }
        }

        Row {
          id: actions
          visible: row.hot
          anchors.right: figs.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)
          Pill { visible: row.running; label: "Restart"; onClicked: root.act(row.u, "restart") }
          Pill { visible: row.running; label: "Stop"; onClicked: root.act(row.u, "stop") }
          Pill { visible: !row.running; label: "Start"; strong: true; tint: root.okColor; onClicked: root.act(row.u, "start") }
          Pill { label: "Logs"; onClicked: root.logs(row.u) }
        }

        Column {
          id: figs
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(84)
          Text {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            textFormat: Text.PlainText
            text: row.busy ? "working…" : row.failed ? "failed" : row.running ? (Model.num(row.u.mem) > 0 ? Model.bytesText(row.u.mem) : "running") : "stopped"
            color: row.failed ? root.badColor : root.foreground
            opacity: row.running || row.failed ? 0.9 : 0.45
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }
          Text {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            textFormat: Text.PlainText
            visible: text.length > 0
            text: Model.num(row.u.restarts) > 0 ? Model.num(row.u.restarts) + " restarts"
                : (row.u.enabled === "enabled" ? "starts at login" : "")
            color: Model.num(row.u.restarts) > 20 ? root.warnColor : root.foreground
            opacity: 0.45
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }
        }
      }
    }
  }

  Text {
    visible: root.scope === "system"
    width: parent.width
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    text: "System services ask for your password before they start, stop or restart."
    color: root.foreground
    opacity: 0.45
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
  }
}

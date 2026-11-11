import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import "../../lib/Format.js" as Format
import "../kit"

// Background services. "Yours" are the units you wrote yourself, in
// ~/.config/systemd/user (Cornerstone, HQ, the bots); "Desktop" is the rest
// of your session's; "System" is the machine's, where start, stop and
// restart ask for your password through the usual prompt.
Page {
  id: page

  readonly property var lists: frame.services || null
  readonly property var user: lists && Array.isArray(lists.user) ? lists.user : []
  readonly property var system: lists && Array.isArray(lists.system) ? lists.system : []
  readonly property string tool: service ? service.collectorPath : ""
  readonly property color ok: "#3fb950"

  property string scope: "yours"
  property string query: ""
  property bool showStopped: false
  // Unit being started, stopped or restarted, until the list shows the result.
  property string working: ""

  function running(u) { return u.active === "active" || u.active === "activating" || u.active === "reloading" }
  function rank(u) { return u.active === "failed" ? 0 : (running(u) ? 1 : 2) }

  readonly property var pool: scope === "system" ? system
    : user.filter(function(u) { return (u.mine === true) === (scope === "yours") })

  readonly property var shown: {
    var q = query.trim().toLowerCase()
    var out = pool.filter(function(u) {
      if (q) return String(u.name).toLowerCase().indexOf(q) !== -1 || String(u.description).toLowerCase().indexOf(q) !== -1
      return showStopped || scope === "yours" || rank(u) < 2
    })
    out.sort(function(a, b) { return rank(a) - rank(b) || String(a.name).localeCompare(String(b.name)) })
    return out
  }

  readonly property int failed: user.concat(system).filter(function(u) { return u.active === "failed" }).length

  function act(u, action) {
    working = u.unit
    Quickshell.execDetached([tool, "unit", u.scope, action, u.unit])
    settle.restart()
  }
  function logs(u) {
    var argv = ["omarchy-launch-tui", "journalctl"]
    if (u.scope === "user") argv.push("--user")
    Quickshell.execDetached(argv.concat(["-u", u.unit, "-n", "300", "-f"]))
  }

  // A user unit answers within a second; a system one waits for the
  // password, so the marker clears when the list next changes.
  Timer { id: settle; interval: 900; onTriggered: if (page.service) page.service.refresh() }
  onListsChanged: if (working !== "" && !settle.running) working = ""

  Row {
    spacing: Style.space(6)
    Chip { big: true; text: "Yours"; selected: page.scope === "yours"; onClicked: page.scope = "yours" }
    Chip { big: true; text: "Desktop"; selected: page.scope === "desktop"; onClicked: page.scope = "desktop" }
    Chip { big: true; text: "System"; selected: page.scope === "system"; onClicked: page.scope = "system" }
    Chip {
      big: true
      visible: page.scope !== "yours"
      text: page.showStopped ? "Hide stopped" : "Show stopped"
      onClicked: page.showStopped = !page.showStopped
    }
  }

  Label {
    visible: page.failed > 0
    width: parent.width
    text: Format.plural(page.failed, "service has", "services have") + " failed (red, listed first)"
    color: page.danger
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  Entry {
    placeholder: "Search services"
    text: page.query
    onTextChanged: if (page.query !== text) page.query = text
    onActiveFocusChanged: if (page.host) page.host.typing = activeFocus
  }

  Block {
    Label {
      visible: page.shown.length === 0
      text: !page.lists ? "Loading…" : (page.query ? "No match" : "Nothing running here")
      fade: 0.45
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: (page.shown.slice(0, 200)).length

      Item {
        id: unit
        required property int index
        readonly property var modelData: (page.shown.slice(0, 200))[index] || ({})
        readonly property var u: modelData
        readonly property bool up: page.running(u)
        readonly property bool broken: u.active === "failed"
        readonly property bool busy: page.working === u.unit
        readonly property bool hot: hover.containsMouse

        width: parent.width
        height: Style.space(36)

        MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
        Rectangle {
          anchors.fill: parent
          anchors.margins: -Style.space(3)
          radius: Style.space(6)
          visible: unit.hot || unit.broken
          color: unit.broken ? Qt.rgba(page.danger.r, page.danger.g, page.danger.b, 0.12) : Util.alpha(page.ink, 0.07)
        }

        Rectangle {
          id: light
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(8)
          height: width
          radius: width / 2
          color: unit.busy ? page.warn : unit.broken ? page.danger : unit.up ? page.ok : Util.alpha(page.ink, 0.3)
        }

        Column {
          anchors.left: light.right
          anchors.leftMargin: Style.space(10)
          anchors.right: unit.hot ? buttons.left : state.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(1)
          Label { width: parent.width; text: String(unit.u.name || "") }
          Label { width: parent.width; text: String(unit.u.description || ""); fade: 0.5; font.pixelSize: Style.font.caption }
        }

        Row {
          id: buttons
          visible: unit.hot
          anchors.right: state.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(4)
          Chip { big: true; visible: unit.up; text: "Restart"; onClicked: page.act(unit.u, "restart") }
          Chip { big: true; visible: unit.up; text: "Stop"; onClicked: page.act(unit.u, "stop") }
          Chip { big: true; visible: !unit.up; text: "Start"; strong: true; tint: page.ok; onClicked: page.act(unit.u, "start") }
          Chip { big: true; text: "Logs"; onClicked: page.logs(unit.u) }
        }

        Column {
          id: state
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(100)
          Label {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            text: unit.busy ? "working…" : unit.broken ? "failed"
              : unit.up ? (Format.number(unit.u.mem) > 0 ? Format.sizeText(unit.u.mem) : "running") : "stopped"
            color: unit.broken ? page.danger : page.ink
            fade: unit.up || unit.broken ? 0.9 : 0.45
            font.pixelSize: Style.font.bodySmall
          }
          Label {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            visible: text !== ""
            text: Format.number(unit.u.restarts) > 0 ? Format.plural(Format.number(unit.u.restarts), "restart")
              : (unit.u.enabled === "enabled" ? (unit.u.scope === "system" ? "starts at boot" : "starts at login") : "")
            color: Format.number(unit.u.restarts) > 20 ? page.warn : page.ink
            fade: 0.45
            font.pixelSize: Style.font.caption
          }
        }
      }
    }
  }

  Label {
    visible: page.scope === "system"
    width: parent.width
    wrapMode: Text.WordWrap
    text: "System services ask for your password before they start, stop or restart."
    fade: 0.45
    font.pixelSize: Style.font.caption
  }
}

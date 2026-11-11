import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../kit"

// Upload and download over time, the interfaces, totals since boot, the
// public and local addresses (click to copy), and which programs talk.
Page {
  id: page

  readonly property var net: frame.network || ({})
  readonly property var ifaces: Array.isArray(net.ifaces) ? net.ifaces : []
  readonly property var main: Hardware.mainIface(frame)
  readonly property var listed: ifaces.filter(function(i) { return i.up || i.isDefault || i.kind === "wifi" })
  readonly property var talkers: Array.isArray(net.procs) ? net.procs : []
  readonly property var addresses: {
    if (!main) return []
    var v4 = Array.isArray(main.ipv4) ? main.ipv4 : []
    var v6 = Array.isArray(main.ipv6) ? main.ipv6.slice(0, 2) : []
    return v4.concat(v6)
  }

  // ---------------------------------------------------------- copying

  property string copied: ""

  // The address reaches wl-copy on standard input, never as an argument: it
  // keeps running to serve the clipboard, and arguments are readable by any
  // local user in /proc/<pid>/cmdline.
  function copy(text) {
    if (!text || clipboard.running) return
    clipboard.pending = String(text)
    clipboard.running = true
    copied = text
    copiedFor.restart()
  }

  Process {
    id: clipboard
    property string pending: ""
    command: ["wl-copy"]
    stdinEnabled: true
    onStarted: {
      write(pending)
      pending = ""
      stdinEnabled = false
    }
    onExited: stdinEnabled = true
  }

  Timer { id: copiedFor; interval: 1400; onTriggered: page.copied = "" }

  function lookUp() { if (service && flag("publicIp")) service.lookUpPublicIp(false) }
  onServiceChanged: lookUp()
  Component.onCompleted: lookUp()

  // --------------------------------------------------------------- cards

  Block {
    Row {
      width: parent.width
      Hero { width: parent.width / 2; value: Format.rate(page.net.tx).value; unit: Format.rate(page.net.tx).unit; label: "Upload" }
      Hero { width: parent.width / 2; value: Format.rate(page.net.rx).value; unit: Format.rate(page.net.rx).unit; label: "Download" }
    }

    Mirror {
      id: graph
      width: parent.width
      height: Style.space(72)
      up: page.series("net.tx")
      down: page.series("net.rx")
      revision: page.revision
      upColor: page.s2
      downColor: page.s1
      floor: 10240
    }

    Key {
      entries: [
        { color: page.s2, label: "Peak ↑", value: Format.rate(graph.peakUp).value, unit: Format.rate(graph.peakUp).unit },
        { color: page.s1, label: "Peak ↓", value: Format.rate(graph.peakDown).value, unit: Format.rate(graph.peakDown).unit }
      ]
    }
  }

  Block {
    visible: page.flag("showInterfaces") || page.flag("showTotals")
    spacing: Style.space(4)

    Label {
      visible: page.flag("showInterfaces") && page.listed.length === 0
      text: page.ready ? "No network interfaces" : "Measuring…"
      fade: 0.5
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: (page.flag("showInterfaces") ? page.listed : []).length

      Item {
        id: iface
        required property int index
        readonly property var modelData: (page.flag("showInterfaces") ? page.listed : [])[index] || ({})
        readonly property var i: modelData
        width: parent.width
        height: Style.space(26)
        opacity: i.up ? 1 : 0.5

        Label {
          id: glyph
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          text: Hardware.ifaceGlyph(iface.i)
          font.pixelSize: Style.font.icon
        }
        Label {
          anchors.left: glyph.right
          anchors.leftMargin: Style.space(10)
          anchors.right: state.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          text: iface.i.kind === "wifi" && iface.i.ssid ? iface.i.ssid + "  " + iface.i.name : iface.i.name
          font.bold: iface.i.isDefault === true
        }
        Label {
          id: state
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          text: (iface.i.up ? (Hardware.linkSpeed(iface.i) || "Connected") : "Disconnected")
            + (iface.i.kind === "wifi" && Format.known(iface.i.signal) ? " · " + iface.i.signal + " dBm" : "")
          fade: 0.6
          font.pixelSize: Style.font.bodySmall
        }
      }
    }

    PanelSeparator { visible: !!page.main && page.flag("showInterfaces") && page.flag("showTotals") }

    Line {
      visible: !!page.main && page.flag("showTotals")
      label: "Received"
      note: "since boot"
      bold: false
      value: page.main ? Format.size(page.main.rxTotal).value : ""
      unit: page.main ? Format.size(page.main.rxTotal).unit : ""
    }
    Line {
      visible: !!page.main && page.flag("showTotals")
      label: "Sent"
      note: "since boot"
      bold: false
      value: page.main ? Format.size(page.main.txTotal).value : ""
      unit: page.main ? Format.size(page.main.txTotal).unit : ""
    }
  }

  Block {
    visible: page.flag("publicIp")
    spacing: Style.space(4)
    Heading { text: "Public IP address" }
    Address { value: page.net.publicIp || ""; waiting: page.net.online === false ? "Offline" : "Looking up…" }
  }

  Block {
    visible: page.flag("showAddresses")
    spacing: Style.space(4)
    Heading { text: "IP addresses" + (page.main ? " · " + page.main.name : "") }
    Label {
      visible: page.addresses.length === 0
      text: "No address assigned"
      fade: 0.5
      font.pixelSize: Style.font.bodySmall
    }
    Repeater {
      model: page.addresses.length
      Address { required property int index; value: String(page.addresses[index] || "") }
    }
  }

  Block {
    visible: page.flag("showProcesses")
    Processes {
      host: page.host
      title: "Processes"
      caption: "Counts TCP only, so some browser and game traffic is missing."
      rows: page.talkers.slice(0, 6)
      allRows: page.talkers
      total: page.talkers.length
      sortKey: "net"
      columns: [{ key: "rx", kind: "rate", title: "Down" }, { key: "tx", kind: "rate", title: "Up" }]
      figureWidth: Style.space(70)
      empty: page.net.procs === null || page.net.procs === undefined ? "Measuring…" : "No traffic"
    }
  }

  // An address line: click to copy.
  component Address: Item {
    id: address
    property string value: ""
    property string waiting: Format.DASH
    readonly property bool justCopied: page.copied !== "" && page.copied === value

    width: parent ? parent.width : implicitWidth
    height: Style.space(22)

    Rectangle {
      anchors.fill: parent
      anchors.leftMargin: -Style.space(6)
      anchors.rightMargin: -Style.space(6)
      radius: Style.cornerRadius
      color: mouse.containsMouse && address.value !== "" ? Style.hoverFillFor(page.ink, Commons.Color.accent) : "transparent"
    }
    Label {
      anchors.left: parent.left
      anchors.right: hint.left
      anchors.rightMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      text: address.value !== "" ? address.value : address.waiting
      fade: address.value !== "" ? 0.92 : 0.45
      elide: Text.ElideMiddle
    }
    Label {
      id: hint
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      visible: address.value !== "" && (mouse.containsMouse || address.justCopied)
      text: address.justCopied ? "Copied" : "󰆏"
      fade: address.justCopied ? 0.9 : 0.5
      font.pixelSize: address.justCopied ? Style.font.caption : Style.font.icon
    }
    MouseArea {
      id: mouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: address.value !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
      onClicked: page.copy(address.value)
    }
  }
}

import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../kit"

// Where the machine's power goes: the best total it can measure, every
// reading behind it, then the battery and wireless devices when there are any.
Page {
  id: page

  readonly property var summary: Hardware.powerSummary(frame)
  readonly property bool wholeMachine: summary.source !== "" && summary.source !== "sum"
  readonly property bool hasBattery: !!(frame.battery && frame.battery.present)
  readonly property var devices: frame.battery && Array.isArray(frame.battery.peripherals) ? frame.battery.peripherals : []

  Block {
    BlockTitle {
      text: "Power"
      detail: page.summary.onAc === null ? "" : (page.summary.onAc ? "On AC power" : "On battery")
    }

    Hero {
      visible: page.summary.total !== null
      width: parent.width
      value: page.summary.total !== null ? Format.watts(page.summary.total).value : ""
      unit: "W"
      label: page.summary.note
    }

    // A measured total is one series; a sum shows its two parts.
    Bars {
      visible: page.summary.total !== null
      width: parent.width
      height: visible ? Style.space(64) : 0
      series: page.wholeMachine ? [page.series("power.total")] : [page.series("power.cpu"), page.series("power.gpu")]
      revision: page.revision
      colors: page.wholeMachine ? [page.s1] : [page.s1, page.s2]
      floor: 10
    }

    Key {
      visible: !page.wholeMachine && page.summary.cpu !== null && page.summary.gpu !== null
      entries: [
        { color: page.s1, label: "CPU", value: page.summary.cpu !== null ? Format.watts(page.summary.cpu).value : Format.DASH, unit: page.summary.cpu !== null ? "W" : "" },
        { color: page.s2, label: "Graphics", value: page.summary.gpu !== null ? Format.watts(page.summary.gpu).value : Format.DASH, unit: page.summary.gpu !== null ? "W" : "" }
      ]
    }

    Label {
      visible: page.summary.total === null || page.summary.raplHint
      width: parent.width
      wrapMode: Text.WordWrap
      text: !page.ready ? "Starting the collector…"
        : (page.summary.total === null ? "This machine reports no power readings." : "")
          + (page.summary.raplHint ? (page.summary.total === null ? " " : "") + "CPU power is hidden: the kernel only lets root read the RAPL energy counters." : "")
      fade: 0.6
      font.pixelSize: Style.font.bodySmall
    }
  }

  Block {
    visible: page.summary.rows.length > 0
    spacing: Style.space(2)
    Heading { text: "Readings" }
    Repeater {
      model: (page.summary.rows).length
      Line {
        required property int index
        readonly property var modelData: (page.summary.rows)[index] || ({})
        label: modelData.label
        note: modelData.note
        value: modelData.watts === null ? "Off" : Format.watts(modelData.watts).value
        unit: modelData.watts === null ? "" : "W"
        bold: false
        labelFade: modelData.watts === null ? 0.5 : 0.85
      }
    }
  }

  BatteryCards {
    width: parent.width
    visible: page.hasBattery || page.devices.length > 0
    host: page.host
  }
}

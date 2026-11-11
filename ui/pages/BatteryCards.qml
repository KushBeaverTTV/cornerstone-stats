import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../kit"

// The battery part of the Power tab: charge and health dials, the charge
// over time coloured by whether it was charging, the details, and the
// wireless devices that report a charge (mice, headsets).
Page {
  id: cards

  readonly property var battery: frame.battery || ({})
  readonly property bool present: battery.present === true
  readonly property real percent: Format.number(battery.percent)
  readonly property string status: String(battery.status || "Unknown")
  readonly property bool charging: status === "Charging"
  readonly property bool full: status === "Full" || (percent >= 99 && battery.acOnline === true)
  readonly property var devices: Array.isArray(battery.peripherals) ? battery.peripherals : []

  readonly property string summary: {
    if (charging && Format.number(battery.timeToFull) > 0) return Format.hoursMinutes(battery.timeToFull) + " to full"
    if (status === "Discharging" && Format.number(battery.timeToEmpty) > 0) return Format.hoursMinutes(battery.timeToEmpty) + " left"
    if (full) return "Fully charged"
    if (status === "Not charging") return "Charge held"
    return charging ? "Charging" : status
  }

  // The charge history split in two, so charging and draining read apart.
  readonly property var split: {
    var level = series("battery"), flags = series("battery.charging")
    var up = [], down = []
    var tick = revision
    for (var i = 0; i < level.length; i++) {
      var on = flags[i] === 1
      up.push(on ? level[i] : 0)
      down.push(on ? 0 : level[i])
    }
    return { up: up, down: down }
  }

  readonly property color chargeColor: charging || full ? good : (percent <= 10 ? danger : (percent <= 20 ? warn : s1))

  function detail(label, value, unit, shown) { return { label: label, value: value, unit: unit || "", shown: shown } }
  readonly property var details: [
    detail("Status", summary, "", true),
    detail("Drawing", Format.number(battery.watts).toFixed(1), "W", Format.number(battery.watts) > 0),
    detail("Capacity", Format.number(battery.energyFull).toFixed(1), "Wh", Format.number(battery.energyFull) > 0),
    detail("Designed for", Format.number(battery.energyDesign).toFixed(1), "Wh", Format.number(battery.energyDesign) > 0),
    detail("Charge cycles", String(Math.round(Format.number(battery.cycles))), "", Format.number(battery.cycles) > 0),
    detail("Voltage", Format.number(battery.voltage).toFixed(2), "V", Format.number(battery.voltage) > 0),
    detail("Power adapter", battery.acOnline ? "Connected" : "Unplugged", "", battery.acOnline !== null && battery.acOnline !== undefined),
    detail("Model", String(battery.model || ""), "", !!battery.model),
    detail("Chemistry", String(battery.technology || ""), "", !!battery.technology)
  ].filter(function(d) { return d.shown })

  Block {
    visible: cards.present
    Row {
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: Style.space(30)
      Dial {
        size: Style.space(88)
        value: cards.percent / 100
        color: cards.chargeColor
        figure: String(Math.round(cards.percent))
        unit: "%"
        sub: cards.summary
      }
      Dial {
        visible: Format.known(cards.battery.health)
        size: Style.space(88)
        value: Format.number(cards.battery.health) / 100
        color: cards.s2
        figure: String(Math.round(Format.number(cards.battery.health)))
        unit: "%"
        name: "Health"
      }
    }
  }

  Block {
    visible: cards.present && cards.flag("showHistory")
    BlockTitle { text: "Charge"; detail: cards.charging ? "Charging" : (cards.full ? "Full" : "On battery") }
    Bars {
      width: parent.width
      height: Style.space(56)
      series: [cards.split.up, cards.split.down]
      revision: cards.revision
      colors: [cards.good, cards.s1]
      ceiling: 100
    }
    Key {
      entries: [{ color: cards.good, label: "Charging", value: "", unit: "" }, { color: cards.s1, label: "On battery", value: "", unit: "" }]
    }
  }

  Block {
    visible: cards.present && cards.flag("showDetails")
    spacing: Style.space(2)
    Repeater {
      model: cards.details.length
      Line {
        required property int index
        readonly property var modelData: cards.details[index] || ({})
        label: modelData.label
        value: modelData.value
        unit: modelData.unit
        bold: false
      }
    }
  }

  Block {
    visible: cards.devices.length > 0 && cards.flag("showDevices")
    spacing: Style.space(2)
    Heading { text: "Devices" }
    Repeater {
      model: (cards.devices).length
      Line {
        required property int index
        readonly property var modelData: (cards.devices)[index] || ({})
        label: String(modelData.name || "Device")
        note: modelData.status === "Charging" ? "charging" : ""
        value: String(Math.round(Format.number(modelData.percent)))
        unit: "%"
        bold: false
        Arc {
          size: Style.space(14)
          thickness: Style.spaceReal(2.2)
          value: Format.number(modelData.percent) / 100
          color: Format.number(modelData.percent) <= 20 ? cards.warn : cards.s1
        }
      }
    }
  }
}

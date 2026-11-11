import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../kit"

// Dials for the CPU, each main graphics card and the fans, then every
// temperature and every fan the machine reports.
Page {
  id: page

  readonly property var cpu: frame.cpu || ({})
  readonly property var temps: Hardware.sortedTemps(frame)
  readonly property var fans: Hardware.fans(frame)
  readonly property var fanSummary: Hardware.fanSummary(frame)
  readonly property var cardDials: Hardware.mainGpus(frame).filter(function(g) { return Format.known(g.temp) })

  // Warm is relative: a sensor's own critical point when it has one.
  function heat(celsius, critical, fallback) {
    var limit = Format.number(critical) > 0 ? Number(critical) : fallback
    var share = Format.number(celsius) / limit
    return share >= 0.92 ? danger : (share >= 0.78 ? warn : s1)
  }
  function level(celsius, critical) {
    var limit = Format.number(critical) > 0 ? Number(critical) : 100
    return Math.max(0, Math.min(1, Format.number(celsius) / limit))
  }

  Block {
    Row {
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: Style.space(18)

      Dial {
        visible: Format.known(page.cpu.temp)
        size: Style.space(84)
        value: page.level(page.cpu.temp, 0)
        color: page.heat(page.cpu.temp, 0, 95)
        caption: "CPU"
        figure: Format.degrees(page.cpu.temp, page.unit).replace("°", "")
        unit: "°"
        figureSize: Style.font.heading
        sub: Format.clockText(page.cpu.mhz)
      }

      Repeater {
        model: (page.cardDials).length
        Dial {
          required property int index
          readonly property var modelData: (page.cardDials)[index] || ({})
          size: Style.space(84)
          value: page.level(modelData.temp, 0)
          color: page.heat(modelData.temp, 0, 90)
          caption: page.cardDials.length > 1 ? Hardware.gpuTag(modelData) : "GPU"
          figure: Format.degrees(modelData.temp, page.unit).replace("°", "")
          unit: "°"
          figureSize: Style.font.heading
          sub: Format.clockText(modelData.mhz)
        }
      }

      Dial {
        visible: page.fans.length > 0
        size: Style.space(84)
        value: page.fanSummary.on > 0 ? Math.min(1, page.fanSummary.average / 2400) : 0
        color: page.s2
        caption: "Fans"
        figure: page.fanSummary.average > 0 ? String(Math.round(page.fanSummary.average)) : (page.fanSummary.on > 0 ? "On" : "Off")
        unit: page.fanSummary.average > 0 ? "rpm" : ""
        figureSize: page.fanSummary.average > 0 ? Style.font.heading : Style.font.title
        sub: page.fanSummary.on > 0 ? page.fanSummary.on + " of " + page.fanSummary.count + " on" : ""
      }
    }

    Label {
      visible: !Format.known(page.cpu.temp) && page.cardDials.length === 0 && page.fans.length === 0
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: page.ready ? "No hardware sensors found" : "Measuring…"
      fade: 0.5
      font.pixelSize: Style.font.bodySmall
    }
  }

  Block {
    visible: page.temps.length > 0 && page.flag("showTemperatures")
    spacing: Style.space(2)
    Heading { text: "Temperature" }
    Repeater {
      model: (page.temps).length
      Line {
        required property int index
        readonly property var modelData: (page.temps)[index] || ({})
        label: Hardware.sensorName(modelData)
        value: Format.degrees(modelData.celsius, page.unit)
        bold: false
        Arc {
          size: Style.space(14)
          thickness: Style.spaceReal(2.2)
          value: page.level(modelData.celsius, modelData.critical)
          color: page.heat(modelData.celsius, modelData.critical, 95)
        }
      }
    }
  }

  Block {
    visible: page.fans.length > 0 && page.flag("showFans")
    spacing: Style.space(2)
    Heading { text: "Fans" }
    Repeater {
      model: (page.fans).length
      Line {
        required property int index
        readonly property var modelData: (page.fans)[index] || ({})
        label: Hardware.sensorName(modelData)
        value: Hardware.fanReading(modelData).value
        unit: Hardware.fanReading(modelData).unit
        bold: false
        labelFade: Hardware.fanOn(modelData) ? 0.85 : 0.5
        Arc {
          size: Style.space(14)
          thickness: Style.spaceReal(2.2)
          value: Hardware.fanLevel(modelData)
          color: page.s2
        }
      }
    }
  }
}

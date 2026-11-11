import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../../lib/Prefs.js" as Prefs
import "../kit"

// One card per graphics card: load and VRAM over time, clocks, board power,
// and what each program uses of it. Temperatures and fans live on Sensors.
Page {
  id: page

  readonly property var cards: Hardware.gpus(frame)
  readonly property var grader: Prefs.grader(settings, lightTheme)

  Repeater {
    model: (page.cards).length

    Block {
      id: card
      required property int index
      readonly property var modelData: (page.cards)[index] || ({})
      readonly property var gpu: modelData
      readonly property string cardKey: String(gpu.id)
      readonly property bool hasLoad: Hardware.gpuHasLoad(gpu)
      readonly property var vram: Hardware.vramPercent(gpu)

      BlockTitle {
        text: Hardware.gpuName(card.gpu)
        detail: card.gpu.asleep ? "Asleep"
          : [Format.clockText(card.gpu.mhz), Format.known(card.gpu.temp) ? Format.degrees(card.gpu.temp, page.unit) : ""]
              .filter(function(s) { return s !== "" }).join(", ")
      }

      Bars {
        visible: card.hasLoad
        width: parent.width
        height: visible ? Style.space(64) : 0
        series: [page.series("gpu.load:" + card.cardKey)]
        revision: page.revision
        colors: [page.s1]
        grader: page.grader
        ceiling: 100
      }

      Line {
        label: "Load"
        note: card.hasLoad ? "" : (card.gpu.asleep ? "asleep" : "not reported by this driver")
        dot: page.grader ? "transparent" : page.s1
        value: card.hasLoad ? String(Math.round(card.gpu.load)) : Format.DASH
        unit: card.hasLoad ? "%" : ""
      }

      Bars {
        visible: card.vram !== null
        width: parent.width
        height: visible ? Style.space(64) : 0
        series: [page.series("gpu.vram:" + card.cardKey)]
        revision: page.revision
        colors: [page.s2]
        grader: page.grader
        ceiling: 100
      }

      Line {
        visible: card.vram !== null
        label: card.gpu.integrated ? "Graphics memory" : "VRAM"
        note: card.vram !== null ? Format.percent(card.vram) : ""
        dot: page.grader ? "transparent" : page.s2
        value: card.vram !== null ? Format.size(card.gpu.vramUsed).value + " / " + Format.size(card.gpu.vramTotal).value : ""
        unit: card.vram !== null ? Format.size(card.gpu.vramTotal).unit : ""
      }

      Line {
        visible: Format.known(card.gpu.memMhz)
        label: "Memory clock"
        value: visible ? Format.clock(card.gpu.memMhz).value : ""
        unit: visible ? Format.clock(card.gpu.memMhz).unit : ""
        keepDotSpace: false
      }

      Line {
        visible: Format.known(card.gpu.watts)
        label: "Board power"
        note: Format.known(card.gpu.wattsCap) ? "limit " + Format.wattsText(card.gpu.wattsCap) : ""
        value: visible ? Format.watts(card.gpu.watts).value : ""
        unit: "W"
      }

      Line {
        visible: Format.known(card.gpu.hotspot)
        label: "Hotspot"
        value: visible ? Format.degrees(card.gpu.hotspot, page.unit) : ""
      }

      // Per-program use arrives only while this tab is open.
      Processes {
        visible: page.flag("showProcesses")
        expandable: false
        endable: true
        host: page.host
        rows: Array.isArray(card.gpu.procs) ? card.gpu.procs : []
        columns: [{ key: "load", kind: "percent", title: "Load" }, { key: "vram", kind: "size", title: card.gpu.integrated ? "Memory" : "VRAM" }]
        empty: Array.isArray(card.gpu.procs) ? "Nothing is using this card" : "Measuring…"
      }
    }
  }

  Block {
    visible: page.cards.length === 0
    Label {
      width: parent.width
      text: page.ready ? "No graphics card found." : "Starting the collector…"
      fade: 0.6
      wrapMode: Text.WordWrap
    }
  }
}

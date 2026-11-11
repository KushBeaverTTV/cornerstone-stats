import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../kit"

// Load over time split into user and system, one ring per logical CPU,
// load average and uptime, and the busiest programs.
Page {
  id: page

  readonly property var cpu: frame.cpu || ({})
  readonly property var usage: cpu.usage || ({})
  readonly property var threads: Array.isArray(cpu.perThread) ? cpu.perThread : []
  readonly property var efficient: Array.isArray(cpu.efficiency) ? cpu.efficiency : []
  readonly property bool hybrid: efficient.length > 0 && efficient.length < threads.length

  Block {
    BlockTitle {
      text: "CPU"
      detail: [Format.clockText(page.cpu.mhz), Format.known(page.cpu.temp) ? Format.degrees(page.cpu.temp, page.unit) : "",
               Format.wattsText(page.cpu.watts)].filter(function(s) { return s !== "" }).join(", ")
    }

    Bars {
      width: parent.width
      height: Style.space(64)
      series: [page.series("cpu.user"), page.series("cpu.system")]
      revision: page.revision
      colors: [page.s1, page.s2]
      ceiling: 100
    }

    Key {
      entries: [
        { color: page.s1, label: "User", value: Math.round(Format.number(page.usage.user)), unit: "%" },
        { color: page.s2, label: "System", value: Math.round(Format.number(page.usage.system)), unit: "%" }
      ]
    }
  }

  Block {
    visible: page.flag("showCores")
    spacing: Style.space(10)

    Flow {
      width: parent.width
      spacing: Style.space(6)
      Repeater {
        model: page.threads.length
        Arc {
          required property int index
          value: Format.number(page.threads[index]) / 100
          color: page.efficient.indexOf(index) !== -1 ? page.s2 : page.s1
        }
      }
    }

    Key {
      entries: page.hybrid
        ? [{ color: page.s1, label: "Performance", value: page.threads.length - page.efficient.length, unit: "" },
           { color: page.s2, label: "Efficiency", value: page.efficient.length, unit: "" }]
        : [{ color: page.s1, label: "Cores", value: Format.number(page.cpu.cores), unit: "" },
           { color: Util.alpha(page.ink, 0.25), label: "Threads", value: Format.number(page.cpu.threads), unit: "" }]
    }

    Label {
      visible: text !== ""
      width: parent.width
      text: String(page.cpu.model || "")
      fade: 0.45
      font.pixelSize: Style.font.caption
    }
  }

  Block {
    visible: page.flag("showLoad")

    Item {
      width: parent.width
      height: loadColumn.height

      Column {
        id: loadColumn
        spacing: Style.space(3)
        Heading { text: "Load average" }
        Label { text: Format.loadAverage(page.cpu.load); font.bold: true }
      }

      Column {
        anchors.right: parent.right
        spacing: Style.space(3)
        Heading { anchors.right: parent.right; text: "Uptime" }
        Label { anchors.right: parent.right; text: Format.duration(page.cpu.uptime); font.bold: true }
      }
    }
  }

  Block {
    visible: page.flag("showProcesses")
    Processes {
      host: page.host
      rows: page.procs && page.procs.top ? page.procs.top.cpu : []
      allRows: page.procs ? page.procs.all : []
      total: page.procs ? Format.number(page.procs.count) : 0
      columns: [{ key: "cpu", kind: "share", title: "" }]
      empty: page.procs ? "Nothing busy" : "Measuring…"
    }
  }
}

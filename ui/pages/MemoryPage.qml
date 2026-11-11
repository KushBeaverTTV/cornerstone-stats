import QtQuick
import qs.Commons
import "../../lib/Format.js" as Format
import "../kit"

// Three dials (swap, memory by kind, pressure), the breakdown in figures,
// and the programs holding the most.
Page {
  id: page

  readonly property var mem: frame.memory || ({})
  readonly property real total: Math.max(1, Format.number(mem.total, 1))
  readonly property real swapTotal: Format.number(mem.swapTotal)
  readonly property real swapUsed: Format.number(mem.swapUsed)
  readonly property real swapShare: swapTotal > 0 ? swapUsed / swapTotal : 0
  readonly property color swapInk: swapShare >= 0.85 ? danger : (swapShare >= 0.5 ? warn : quiet)
  // How much of the last ten seconds some task, and every task, waited for
  // memory. Used memory alone cannot say whether the machine is short of it.
  readonly property real stallSome: Format.number(mem.stall ? mem.stall.some : 0)
  readonly property real stallFull: Format.number(mem.stall ? mem.stall.full : 0)
  readonly property var zram: mem.zram || null

  function stallInk(p) { return p >= 40 ? danger : (p >= 10 ? warn : quiet) }
  function figure(bytes) { return Format.size(bytes) }

  Block {
    Row {
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: Style.space(30)

      Dial {
        value: page.swapShare
        color: page.swapInk
        figure: page.swapTotal > 0 ? String(Math.round(page.swapShare * 100)) : "Off"
        unit: page.swapTotal > 0 ? "%" : ""
        figureSize: page.swapTotal > 0 ? Style.font.display : Style.font.title
        name: "Swap"
        sub: page.swapTotal > 0 ? Format.sizeText(page.swapUsed > 0 ? page.swapUsed : page.swapTotal) : ""
      }

      Dial {
        parts: [
          { value: Format.number(page.mem.apps) / page.total, color: page.s1 },
          { value: Format.number(page.mem.shared) / page.total, color: page.s3 },
          { value: Format.number(page.mem.cached) / page.total, color: page.s2 }
        ]
        figure: String(Math.round(Format.number(page.mem.used) / page.total * 100))
        unit: "%"
        name: "Memory"
      }

      Dial {
        value: page.stallSome / 100
        color: page.stallInk(page.stallSome)
        figure: String(Math.round(page.stallSome))
        unit: "%"
        name: "Pressure"
        sub: page.stallFull >= 1 ? "all " + Math.round(page.stallFull) + "%" : "waiting"
      }
    }
  }

  Block {
    visible: page.flag("showBreakdown")
    spacing: Style.space(2)

    Line { label: "Apps"; dot: page.s1; value: page.figure(page.mem.apps).value; unit: page.figure(page.mem.apps).unit }
    Line { label: "Shared"; dot: page.s3; value: page.figure(page.mem.shared).value; unit: page.figure(page.mem.shared).unit }
    Line { label: "Cached"; dot: page.s2; value: page.figure(page.mem.cached).value; unit: page.figure(page.mem.cached).unit }
    Line { label: "Free"; dot: Util.alpha(page.ink, 0.16); value: page.figure(page.mem.free).value; unit: page.figure(page.mem.free).unit }
    Line {
      label: "Total"
      keepDotSpace: true
      labelFade: 0.55
      bold: false
      value: page.figure(page.mem.total).value
      unit: page.figure(page.mem.total).unit
    }

    // Not part of the total above: compressed memory is counted inside it,
    // and swap lives on disk.
    Item { visible: page.zram !== null || page.swapTotal > 0; width: 1; height: Style.space(6) }

    Line {
      visible: page.zram !== null
      label: "Compressed"
      note: page.zram && Format.number(page.zram.stored) > 0
        ? "held in " + Format.sizeText(page.zram.stored) : ""
      keepDotSpace: true
      value: page.zram ? page.figure(page.zram.original).value : ""
      unit: page.zram ? page.figure(page.zram.original).unit : ""
    }
    Line {
      visible: page.swapTotal > 0
      label: "Swap"
      note: "of " + Format.sizeText(page.swapTotal)
      dot: page.swapUsed > 0 ? page.swapInk : "transparent"
      keepDotSpace: true
      value: page.figure(page.swapUsed).value
      unit: page.figure(page.swapUsed).unit
    }
  }

  Block {
    visible: page.flag("showProcesses")
    Processes {
      host: page.host
      rows: page.procs && page.procs.top ? page.procs.top.mem : []
      allRows: page.procs ? page.procs.all : []
      total: page.procs ? Format.number(page.procs.count) : 0
      columns: [{ key: "mem", kind: "size", title: "" }]
      empty: page.procs ? "Nothing resident" : "Measuring…"
    }
  }
}

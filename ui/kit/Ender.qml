import QtQuick
import Quickshell
import qs.Commons

// The End controls for a process row. "End" first asks: it turns into
// "End task" (polite) and "Force kill" for four seconds. Both go through
// ~/.local/bin/taskman-end, which ends every copy of that program you own
// and refuses the desktop's own core processes.
//
// Rows re-sort every second, so the question remembers the process it was
// asked about: if a different one slides into this row, the question goes
// away instead of applying to it.
Row {
  id: ender

  property int pid: 0
  property bool shown: true
  property int armedFor: 0
  readonly property bool asking: armedFor > 1 && armedFor === pid
  signal done()

  readonly property string tool: Quickshell.env("HOME") + "/.local/bin/taskman-end"

  function end(force) {
    var target = armedFor
    armedFor = 0
    if (target > 1) Quickshell.execDetached(force ? [tool, String(target), "--force"] : [tool, String(target)])
    done()
  }

  visible: shown || asking
  spacing: Style.space(4)

  Chip { visible: !ender.asking; text: "End"; onClicked: { ender.armedFor = ender.pid; giveUp.restart() } }
  Chip { visible: ender.asking; text: "End task"; strong: true; tint: "#e8740f"; onClicked: ender.end(false) }
  Chip { visible: ender.asking; text: "Force kill"; strong: true; tint: "#e01414"; onClicked: ender.end(true) }

  Timer { id: giveUp; interval: 4000; onTriggered: ender.armedFor = 0 }
}

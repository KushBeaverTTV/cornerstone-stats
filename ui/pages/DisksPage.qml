import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../../lib/Prefs.js" as Prefs
import "../kit"

// Volumes and how full they are, read and write activity for all drives or
// the one clicked, problems worth fixing (TRIM, slow FAT mounts), and which
// programs read and write.
Page {
  id: page

  readonly property var disks: frame.disks || ({})
  readonly property var drives: disks.drives || ({})
  readonly property var volumes: Hardware.volumes(frame)
  readonly property var groups: Hardware.volumeGroups(volumes)
  readonly property real stallSome: Format.number(disks.stall ? disks.stall.some : 0)
  readonly property real stallFull: Format.number(disks.stall ? disks.stall.full : 0)

  // The drive the page follows: one clicked here (for as long as the panel
  // stays open), else the Settings choice.
  property string picked: ""
  readonly property string chosen: Hardware.resolveDrive(frame, Prefs.value(settings, "disksSource"))
  readonly property string selected: picked !== "" ? picked : (chosen || "all")
  readonly property var selectedVolume: {
    if (selected === "all") return null
    for (var i = 0; i < volumes.length; i++) if (volumes[i].key === selected) return volumes[i]
    return null
  }
  readonly property bool shareSelected: !!selectedVolume && selectedVolume.kind === "network"
  readonly property bool driveSelected: selected !== "all" && !!drives[selected]
  readonly property var load: driveSelected ? drives[selected] : null
  readonly property var strain: Hardware.driveStrain(load, stallFull, !!(selectedVolume && selectedVolume.rotational))
  readonly property var rates: Hardware.driveRates(frame, driveSelected ? selected : "all")
  readonly property var problems: Hardware.checksFor(volumes, selected)
  readonly property var busiest: {
    if (!procs) return []
    if (selected === "all") return procs.top ? procs.top.io : []
    return procs.byDrive ? (procs.byDrive[selected] || []) : (procs.top ? procs.top.io : [])
  }

  function pick(key) { picked = selected === key ? "all" : key }
  function tone(level) { return level === "danger" ? danger : (level === "warn" ? warn : quiet) }
  function launch(argv) {
    if (!argv) return
    Quickshell.execDetached(argv)
    if (host) host.close()
  }

  Block {
    visible: page.flag("showVolumes")
    spacing: Style.space(4)

    Label {
      visible: page.volumes.length === 0
      text: page.ready ? "No mounted volumes" : "Measuring…"
      fade: 0.5
      font.pixelSize: Style.font.bodySmall
    }

    Repeater {
      model: (page.groups).length

      Column {
        id: group
        readonly property var modelData: (page.groups)[index] || ({})
        required property int index
        width: parent.width
        spacing: Style.space(4)

        Heading {
          visible: page.groups.length > 1
          topPadding: group.index > 0 ? Style.space(8) : 0
          text: group.modelData.title
        }

        Repeater {
          model: (group.modelData.items).length

          Item {
            id: volume
            required property int index
            readonly property var modelData: (group.modelData.items)[index] || ({})
            readonly property var v: modelData
            readonly property real used: Format.number(v.size) > 0 ? Format.number(v.used) / Number(v.size) : 0
            readonly property bool isSelected: page.selected === v.key
            readonly property var checks: Array.isArray(v.checks) ? v.checks : []
            readonly property var rowStrain: Hardware.driveStrain(page.drives[v.drive], page.stallFull, !!v.rotational)
            readonly property string alert: rowStrain.level === "danger" ? "danger" : ((rowStrain.level || checks.length) ? "warn" : "")

            width: parent.width
            height: Style.space(34)

            Rectangle {
              anchors.fill: parent
              anchors.leftMargin: -Style.space(6)
              anchors.rightMargin: -Style.space(6)
              radius: Style.cornerRadius
              color: volume.isSelected ? Util.alpha(Commons.Color.accent, 0.16)
                   : (pointer.containsMouse ? Style.hoverFillFor(page.ink, Commons.Color.accent) : "transparent")
            }

            Arc {
              id: fullness
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              size: Style.space(24)
              thickness: Style.spaceReal(3)
              value: volume.used
              color: volume.used >= 0.92 ? page.danger : (volume.used >= 0.8 ? page.warn : page.s1)
            }
            Label {
              anchors.centerIn: fullness
              text: Math.round(volume.used * 100)
              fade: 0.85
              font.pixelSize: Math.max(7, Style.font.caption - 2)
              font.bold: true
            }

            Column {
              anchors.left: fullness.right
              anchors.leftMargin: Style.space(10)
              anchors.right: figures.left
              anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter

              Row {
                width: parent.width
                spacing: Style.space(6)
                Label {
                  id: volumeName
                  width: Math.min(implicitWidth, parent.width - (mark.visible ? mark.width + parent.spacing : 0))
                  text: Hardware.volumeName(volume.v.mount)
                  font.bold: true
                }
                Label {
                  id: mark
                  visible: volume.alert !== ""
                  anchors.verticalCenter: volumeName.verticalCenter
                  text: "󰀦"
                  color: page.tone(volume.alert)
                }
              }
              Label {
                width: parent.width
                text: Format.known(volume.v.avail) ? Format.sizeText(volume.v.avail) + " available" : "not answering"
                fade: 0.55
                font.pixelSize: Style.font.caption
              }
            }

            Column {
              id: figures
              anchors.right: openButton.left
              anchors.rightMargin: Style.space(4)
              anchors.verticalCenter: parent.verticalCenter
              Amount {
                anchors.right: parent.right
                value: Format.size(volume.v.size).value
                unit: Format.size(volume.v.size).unit
                bold: false
                fade: 0.85
              }
              Label {
                anchors.right: parent.right
                text: [volume.v.fstype, Format.known(volume.v.temp) ? Format.degrees(volume.v.temp, page.unit) : ""]
                  .filter(function(s) { return !!s }).join(" · ")
                fade: 0.45
                font.pixelSize: Style.font.caption
              }
            }

            MouseArea {
              id: pointer
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: page.pick(volume.v.key)
            }

            PanelActionButton {
              id: openButton
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              size: Style.space(22)
              iconText: "󰉋"
              tooltipText: "Open in Files"
              property bool pointed: false
              opacity: pointer.containsMouse || pointed ? 1 : 0.35
              onHovered: function(inside) { pointed = inside }
              onClicked: page.launch(["xdg-open", String(volume.v.mount)])
            }

            PanelToolTip {
              visible: pointer.containsMouse
              text: [volume.v.model, volume.v.device, volume.v.mount, volume.rowStrain.text]
                .concat(volume.checks.map(function(c) { return Hardware.problem(c).title }))
                .filter(function(s) { return !!s }).join("\n")
                + (volume.isSelected ? "\nClick to show all disks" : "\nClick to follow this drive")
            }
          }
        }
      }
    }
  }

  Block {
    visible: page.flag("showActivity")

    BlockTitle {
      text: page.selected === "all" ? "All disks" : Hardware.volumeName(page.selectedVolume ? page.selectedVolume.mount : page.selected)
      detail: page.selected === "all" ? ""
        : (page.shareSelected ? String(page.selectedVolume.device || "")
          : page.selected + (page.load && page.load.model ? " · " + page.load.model : ""))
    }

    Label {
      visible: page.shareSelected
      width: parent.width
      wrapMode: Text.Wrap
      text: "A network share has no transfer counters on this side. The programs below have files open on it."
      fade: 0.55
      font.pixelSize: Style.font.bodySmall
    }

    Row {
      visible: !page.shareSelected
      width: parent.width
      Hero { width: parent.width / 2; value: Format.rate(page.rates.read).value; unit: Format.rate(page.rates.read).unit; label: "Read" }
      Hero { width: parent.width / 2; value: Format.rate(page.rates.write).value; unit: Format.rate(page.rates.write).unit; label: "Write" }
    }

    Mirror {
      id: graph
      visible: !page.shareSelected
      width: parent.width
      height: Style.space(72)
      up: page.series(page.driveSelected ? "disk.read:" + page.selected : "disk.read")
      down: page.series(page.driveSelected ? "disk.write:" + page.selected : "disk.write")
      revision: page.revision
      upColor: page.s2
      downColor: page.s1
      floor: 262144
    }

    Key {
      visible: !page.shareSelected
      entries: [
        { color: page.s2, label: "Read peak", value: Format.rate(graph.peakUp).value, unit: Format.rate(graph.peakUp).unit },
        { color: page.s1, label: "Write peak", value: Format.rate(graph.peakDown).value, unit: Format.rate(graph.peakDown).unit }
      ]
    }

    // How long each request takes, how many wait, how much of the time the
    // drive works. Rising latency with a deep queue is a drive falling behind.
    Key {
      visible: page.load !== null
      entries: page.load ? [
        { color: page.tone(page.strain.level), label: "Latency", value: Format.known(page.load.latency) ? Format.latency(page.load.latency).replace(/ ms$/, "") : Format.DASH, unit: Format.known(page.load.latency) ? "ms" : "" },
        { color: page.tone(page.strain.level), label: "Queue", value: Format.number(page.load.queue).toFixed(1), unit: "" },
        { color: page.tone(page.strain.level), label: "Busy", value: Math.round(Format.number(page.load.busy)), unit: "%" }
      ] : []
    }

    // The kernel's count of time lost waiting for storage, machine wide:
    // "some" when any task waits, "all" when every one does (a frozen desktop).
    Key {
      visible: !!page.disks.stall
      entries: [
        { color: Format.number(page.stallSome) >= 40 ? page.danger : (page.stallSome >= 10 ? page.warn : page.quiet), label: "I/O waits", value: Math.round(page.stallSome), unit: "%" },
        { color: Format.number(page.stallFull) >= 40 ? page.danger : (page.stallFull >= 10 ? page.warn : page.quiet), label: "All stalled", value: Math.round(page.stallFull), unit: "%" }
      ]
    }

    Label {
      visible: page.strain.text !== ""
      width: parent.width
      wrapMode: Text.Wrap
      text: page.strain.text
      color: page.tone(page.strain.level)
      font.pixelSize: Style.font.bodySmall
    }
  }

  Block {
    visible: page.problems.length > 0
    spacing: Style.space(10)

    Heading { text: Format.plural(page.problems.length, "problem") }

    Repeater {
      model: (page.problems).length

      Item {
        id: problem
        required property int index
        readonly property var modelData: (page.problems)[index] || ({})
        readonly property var words: Hardware.problem(modelData.check)
        readonly property var command: Hardware.fixCommand(modelData.check)
        width: parent.width
        height: Math.max(explain.implicitHeight, fix.visible ? fix.implicitHeight : 0)

        Column {
          id: explain
          anchors.left: parent.left
          anchors.right: fix.visible ? fix.left : parent.right
          anchors.rightMargin: fix.visible ? Style.space(10) : 0
          spacing: Style.space(2)
          Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: problem.words.title + "  ·  " + Hardware.volumeName(problem.modelData.volume ? problem.modelData.volume.mount : "")
            font.bold: true
          }
          Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: problem.words.detail
            fade: 0.65
            font.pixelSize: Style.font.bodySmall
          }
        }

        Button {
          id: fix
          visible: !!problem.command
          anchors.right: parent.right
          anchors.top: parent.top
          text: problem.words.action
          tooltipText: "Opens a terminal that shows each command and asks before running it"
          bordered: true
          fontSize: Style.font.bodySmall
          onClicked: page.launch(problem.command)
        }
      }
    }
  }

  Block {
    visible: page.flag("showProcesses")
    Processes {
      host: page.host
      rows: page.busiest
      allRows: page.procs ? page.procs.all : []
      total: page.procs ? Format.number(page.procs.count) : 0
      sortKey: "io"
      caption: page.selected === "all" ? "" : "Programs with files open here"
      columns: [{ key: "read", kind: "rate", title: "Read" }, { key: "write", kind: "rate", title: "Write" }]
      figureWidth: Style.space(70)
      empty: !page.procs ? "Measuring…" : (page.selected === "all" ? "No disk activity" : "Nothing here is reading or writing")
    }
  }
}

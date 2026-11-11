import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../../lib/Prefs.js" as Prefs
import "../kit"

// Everything the user can change, saved straight into this widget's entry
// in shell.json: which readouts the bar shows and how, which tabs and
// sections the panel shows, and the general choices.
Page {
  id: page

  readonly property var barOn: Prefs.barModules(settings)
  // Modules that can sit in the bar on this machine: the chosen ones in
  // their order, then the rest.
  readonly property var barAll: {
    var all = ["cpu", "gpu", "memory", "disks", "network", "sensors"]
    if (frame.battery && frame.battery.present) all.push("battery")
    return barOn.filter(function(id) { return all.indexOf(id) !== -1 })
      .concat(all.filter(function(id) { return barOn.indexOf(id) === -1 }))
  }
  readonly property var cards: Hardware.gpus(frame)
  readonly property var cardsShown: Hardware.barGpus(frame, settings.barGpus).map(function(g) { return String(g.id) })
  readonly property var driveChoices: Hardware.driveChoices(frame)
  readonly property var disksShown: Prefs.barDisks(settings)
  readonly property var sensorChoices: Hardware.sensorChoices(frame)
  readonly property var sensorsShown: Prefs.words(Prefs.value(settings, "barSensors"))
  readonly property var tabsOn: Prefs.words(Prefs.value(settings, "tabs")).map(Prefs.canonical)
  readonly property real indent: Style.space(14)

  function save(name, value) { if (host) host.save(name, value) }

  function setModule(id, on) {
    var next = barOn.filter(function(m) { return m !== id })
    if (on) next.push(id)
    save("modules", next.join(","))
  }

  function setTab(id, on) {
    var next = Prefs.TAB_ORDER.filter(function(t) {
      return t === id ? on : tabsOn.indexOf(t) !== -1 || (t === "power" && tabsOn.indexOf("battery") !== -1)
    })
    save("tabs", next.join(","))
  }

  function setCard(id, on) {
    var next = cardsShown.filter(function(c) { return c !== id })
    if (on) next.push(id)
    save("barGpus", next.length > 0 ? next.join(",") : "none")
  }

  function diskAt(disk) {
    for (var i = 0; i < disksShown.length; i++) {
      if (disksShown[i].disk === disk || Hardware.resolveDrive(frame, disksShown[i].disk) === Hardware.resolveDrive(frame, disk)) return i
    }
    return -1
  }

  function setDisk(disk, on, show) {
    var next = disksShown.slice()
    var at = diskAt(disk)
    if (!on) { if (at !== -1) next.splice(at, 1) }
    else if (at === -1) next.push({ disk: disk, show: show || "speed" })
    else next[at] = { disk: disk, show: show || next[at].show }
    save("barDisks", Prefs.barDisksText(next))
  }

  function setSensor(id, on) {
    var next = sensorsShown.filter(function(s) { return s !== id })
    if (on) next.push(id)
    save("barSensors", next.join(","))
  }

  // ------------------------------------------------------------ pieces

  // A name with a switch at the right.
  component Switch: Item {
    id: row
    property string label: ""
    property string note: ""
    property bool on: false
    property bool strong: false
    property real inset: 0
    signal flipped(bool next)

    width: parent ? parent.width : implicitWidth
    height: Math.max(Style.space(26), toggle.implicitHeight * 0.8)

    Label {
      x: row.inset
      anchors.verticalCenter: parent.verticalCenter
      width: toggle.x - x - Style.space(8)
      text: row.label + (row.note ? "  " : "")
      fade: row.strong ? 1 : 0.85
      font.bold: row.strong
      font.pixelSize: row.strong ? Style.font.body : Style.font.bodySmall
    }
    ToggleSwitch {
      id: toggle
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      checked: row.on
      foreground: Commons.Color.popups.text
      onToggled: row.flipped(!row.on)
    }
  }

  // A name with a set of choices at the right.
  component Choice: Item {
    id: choice
    property string label: ""
    property var options: []
    property string value: ""
    property real inset: 0
    signal picked(string value)

    width: parent ? parent.width : implicitWidth
    height: Math.max(Style.space(28), group.implicitHeight)

    Label {
      x: choice.inset
      anchors.verticalCenter: parent.verticalCenter
      text: choice.label
      fade: 0.85
      font.pixelSize: Style.font.bodySmall
    }
    ButtonGroup {
      id: group
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      options: choice.options
      value: choice.value
      foreground: Commons.Color.popups.text
      fontSize: Style.font.caption
      focusable: false
      onChanged: function(v) { choice.picked(v) }
    }
  }

  // A name with minus and plus buttons around a figure.
  component Stepper: Item {
    id: stepper
    property string label: ""
    property string shown: ""
    signal nudged(int direction)

    width: parent ? parent.width : implicitWidth
    height: Style.space(28)

    Label {
      anchors.verticalCenter: parent.verticalCenter
      text: stepper.label
      fade: 0.85
      font.pixelSize: Style.font.bodySmall
    }
    Row {
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(6)
      PanelActionButton { iconText: "󰍴"; size: Style.space(22); foreground: Commons.Color.popups.text; tooltipText: "Less"; onClicked: stepper.nudged(-1) }
      Label {
        width: Style.space(56)
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignHCenter
        text: stepper.shown
        font.bold: true
        font.pixelSize: Style.font.bodySmall
      }
      PanelActionButton { iconText: "󰐕"; size: Style.space(22); foreground: Commons.Color.popups.text; tooltipText: "More"; onClicked: stepper.nudged(1) }
    }
  }

  function stepIn(steps, key, direction) {
    var at = Prefs.nearestStep(Prefs.value(settings, key), steps)
    save(key, steps[Math.max(0, Math.min(steps.length - 1, at + direction))])
  }

  // ---------------------------------------------------------------- bar

  Block {
    Heading { text: "Bar" }
    Label {
      width: parent.width
      wrapMode: Text.WordWrap
      text: "Readouts shown in the bar, in this order. Each can show a mini graph, a figure, or both."
      fade: 0.55
      font.pixelSize: Style.font.caption
    }

    Repeater {
      model: (page.barAll).length

      Column {
        id: module
        required property int index
        readonly property string modelData: (page.barAll)[index] || ""
        readonly property string mid: modelData
        readonly property bool on: page.barOn.indexOf(mid) !== -1
        readonly property var info: Prefs.entry(mid) || {}
        width: parent.width
        spacing: Style.space(2)

        Item {
          width: parent.width
          height: Style.space(28)

          ToggleSwitch {
            id: moduleSwitch
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            checked: module.on
            foreground: Commons.Color.popups.text
            onToggled: page.setModule(module.mid, !module.on)
          }
          Label {
            anchors.left: moduleSwitch.right
            anchors.leftMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            text: module.info.label || module.mid
            fade: module.on ? 1 : 0.55
            font.bold: module.on
          }
          Row {
            visible: module.on
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            PanelActionButton {
              iconText: "󰁝"
              size: Style.space(22)
              foreground: Commons.Color.popups.text
              tooltipText: "Move earlier in the bar"
              opacity: page.barOn.indexOf(module.mid) > 0 ? 1 : 0.3
              onClicked: page.save("modules", Prefs.moved(page.barOn, module.mid, -1).join(","))
            }
            PanelActionButton {
              iconText: "󰁅"
              size: Style.space(22)
              foreground: Commons.Color.popups.text
              tooltipText: "Move later in the bar"
              opacity: page.barOn.indexOf(module.mid) < page.barOn.length - 1 ? 1 : 0.3
              onClicked: page.save("modules", Prefs.moved(page.barOn, module.mid, 1).join(","))
            }
          }
        }

        Dropdown {
          visible: module.on && (module.info.graph === true || module.info.ring === true)
          x: page.indent
          width: parent.width - x
          label: "Look"
          options: Prefs.looksFor(module.mid)
          value: {
            var l = Prefs.lookOf(page.settings, module.mid)
            if (module.mid !== "disks") return l
            return l === "ring" ? "graph" : (l === "ring-text" ? "both" : l)
          }
          onChanged: function(v) { page.save(module.mid + "Style", v) }
        }

        // GPU: a VRAM readout beside each card, and which cards get one.
        Switch {
          visible: module.on && module.mid === "gpu" && page.cards.some(function(g) { return Hardware.vramPercent(g) !== null })
          inset: page.indent
          label: "Show VRAM in bar"
          on: Prefs.flag(page.settings, "showGpuMemory")
          onFlipped: function(next) { page.save("showGpuMemory", next) }
        }
        Repeater {
          model: (module.on && module.mid === "gpu" && page.cards.length > 1 ? page.cards : []).length
          Switch {
            required property int index
            readonly property var modelData: (module.on && module.mid === "gpu" && page.cards.length > 1 ? page.cards : [])[index] || ({})
            inset: page.indent
            label: Hardware.gpuName(modelData) + (Hardware.gpuHasLoad(modelData) ? "" : " (no load figure)")
            on: page.cardsShown.indexOf(String(modelData.id)) !== -1
            onFlipped: function(next) { page.setCard(String(modelData.id), next) }
          }
        }

        // Disks: a readout per drive picked here, showing speed, space, or both.
        Repeater {
          model: (module.on && module.mid === "disks" ? page.driveChoices : []).length
          Column {
            id: drive
            required property int index
            readonly property var modelData: (module.on && module.mid === "disks" ? page.driveChoices : [])[index] || ({})
            readonly property int at: page.diskAt(modelData.value)
            width: parent.width
            Switch {
              inset: page.indent
              label: drive.modelData.label
              on: drive.at !== -1
              onFlipped: function(next) { page.setDisk(drive.modelData.value, next, "") }
            }
            Choice {
              visible: drive.at !== -1
              inset: page.indent * 2
              label: "Show"
              options: Prefs.DISK_SHOWS
              value: drive.at !== -1 ? page.disksShown[drive.at].show : "speed"
              onPicked: function(v) { page.setDisk(drive.modelData.value, true, v) }
            }
          }
        }

        // Sensors: every reading the readout carries.
        Repeater {
          model: (module.on && module.mid === "sensors" ? page.sensorChoices : []).length
          Switch {
            required property int index
            readonly property var modelData: (module.on && module.mid === "sensors" ? page.sensorChoices : [])[index] || ({})
            inset: page.indent
            label: modelData.label
            on: page.sensorsShown.indexOf(String(modelData.value)) !== -1
            onFlipped: function(next) { page.setSensor(String(modelData.value), next) }
          }
        }
      }
    }
  }

  // -------------------------------------------------------------- panel

  Block {
    Heading { text: "Panel" }
    Label {
      width: parent.width
      wrapMode: Text.WordWrap
      text: "Tabs shown in the panel and the sections on each page."
      fade: 0.55
      font.pixelSize: Style.font.caption
    }

    Repeater {
      model: (Prefs.TAB_ORDER.filter(function(t) { return t !== "gpu" || page.cards.length > 0 })).length

      Column {
        id: tab
        required property int index
        readonly property string modelData: (Prefs.TAB_ORDER.filter(function(t) { return t !== "gpu" || page.cards.length > 0 }))[index] || ""
        readonly property bool on: page.tabsOn.indexOf(modelData) !== -1 || (modelData === "power" && page.tabsOn.indexOf("battery") !== -1)
        readonly property var sections: (Prefs.SECTIONS[modelData] || []).filter(function(s) { return !s.battery || !!(page.frame.battery && page.frame.battery.present) })
        width: parent.width

        Switch {
          strong: true
          label: (Prefs.entry(tab.modelData) || {}).label || tab.modelData
          on: tab.on
          onFlipped: function(next) { page.setTab(tab.modelData, next) }
        }
        Repeater {
          model: (tab.on ? tab.sections : []).length
          Switch {
            required property int index
            readonly property var modelData: (tab.on ? tab.sections : [])[index] || ({})
            inset: page.indent
            label: modelData.label
            on: Prefs.flag(page.settings, modelData.key)
            onFlipped: function(next) { page.save(modelData.key, next) }
          }
        }
        Dropdown {
          visible: tab.on && tab.modelData === "disks"
          x: page.indent
          width: parent.width - x
          label: "Activity follows"
          options: page.driveChoices
          value: {
            var name = Hardware.resolveDrive(page.frame, Prefs.value(page.settings, "disksSource"))
            return name === "all" || name === "" ? String(Prefs.value(page.settings, "disksSource") || "all") : Hardware.driveKey(page.frame, name)
          }
          onChanged: function(v) { page.save("disksSource", v) }
        }
      }
    }

    Switch {
      strong: true
      label: "Top processes on every page"
      on: page.flag("showProcesses")
      onFlipped: function(next) { page.save("showProcesses", next) }
    }
  }

  // ------------------------------------------------------------ general

  Block {
    Heading { text: "General" }

    Choice {
      label: "Bar labels"
      options: [{ value: "text", label: "Letters" }, { value: "inline", label: "Inline" }, { value: "icon", label: "Icons" }]
      value: Prefs.labelMode(page.settings)
      onPicked: function(v) { page.save("barLabels", v) }
    }
    Choice {
      label: "Temperature"
      options: [{ value: "Celsius", label: "°C" }, { value: "Fahrenheit", label: "°F" }]
      value: page.unit
      onPicked: function(v) { page.save("temperatureUnit", v) }
    }
    Switch {
      label: "Utilization colors"
      on: page.flag("utilizationColors")
      onFlipped: function(next) { page.save("utilizationColors", next) }
    }

    Repeater {
      model: page.flag("utilizationColors") ? [0, 1, 2, 3] : []

      Item {
        id: grade
        required property int modelData
        readonly property string key: Prefs.GRADE_KEYS[modelData]
        readonly property string stored: String(page.settings[key] || "")
        width: parent.width
        height: Style.space(28)

        function commit() {
          var t = field.text.trim()
          if (t !== "" && t.charAt(0) !== "#") t = "#" + t
          if (t === "" || Prefs.hexColor(t)) { if (t !== grade.stored) page.save(grade.key, t) }
          else field.text = grade.stored
        }

        Rectangle {
          id: swatch
          x: page.indent
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(12)
          height: width
          radius: width / 2
          color: Prefs.gradeColor(page.settings, grade.modelData, page.lightTheme)
        }
        Label {
          anchors.left: swatch.right
          anchors.leftMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          text: Prefs.GRADE_LABELS[grade.modelData]
          fade: 0.85
          font.pixelSize: Style.font.bodySmall
        }
        Entry {
          id: field
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(96)
          limit: 7
          placeholder: "theme"
          text: grade.stored
          onAccepted: grade.commit()
          onActiveFocusChanged: {
            if (page.host) page.host.typing = activeFocus
            if (!activeFocus) grade.commit()
          }
        }
      }
    }

    Label {
      visible: page.flag("utilizationColors")
      x: page.indent
      text: "Six-digit hex, like #3fb950. Leave empty for the theme's color."
      fade: 0.45
      font.pixelSize: Style.font.caption
    }

    Stepper {
      label: "Refresh every"
      shown: Format.interval(Prefs.value(page.settings, "refreshSeconds"))
      onNudged: function(d) { page.stepIn(Prefs.REFRESH_STEPS, "refreshSeconds", d) }
    }
    Stepper {
      label: "History"
      shown: Format.span(Prefs.value(page.settings, "historySeconds"))
      onNudged: function(d) { page.stepIn(Prefs.HISTORY_STEPS, "historySeconds", d) }
    }
    Stepper {
      label: "Bar graph width"
      shown: Math.round(Prefs.numberIn(page.settings, "graphWidth", 16, 120)) + " px"
      onNudged: function(d) { page.save("graphWidth", Math.max(16, Math.min(120, Math.round(Prefs.numberIn(page.settings, "graphWidth", 16, 120)) + d * 4))) }
    }

    Item {
      id: resetRow
      width: parent.width
      height: reset.implicitHeight + Style.space(4)
      property bool armed: false
      Button {
        id: reset
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        text: resetRow.armed ? "Click again to reset everything" : "Reset to defaults"
        bordered: true
        fontSize: Style.font.bodySmall
        foreground: Commons.Color.popups.text
        onClicked: {
          if (resetRow.armed) { resetRow.armed = false; if (page.host) page.host.resetSettings() }
          else { resetRow.armed = true; disarm.restart() }
        }
      }
      Timer { id: disarm; interval: 4000; onTriggered: resetRow.armed = false }
    }
  }
}

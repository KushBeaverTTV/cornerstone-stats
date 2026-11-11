import QtQuick
import qs.Commons
import qs.Ui
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware
import "../../lib/Prefs.js" as Prefs
import "../kit" as Kit

// One readout in the bar: a label (stacked letters, a short name, or a
// glyph), then a mini graph or ring, then the figure. Built on the shell's
// WidgetButton so clicks, hover and the tooltip behave like every other
// bar item.
WidgetButton {
  id: readout

  property var host: null
  property string module: "cpu"
  property string gpuId: ""
  property string disk: "all"
  property string diskShow: "speed"
  property string tag: ""

  readonly property var service: host ? host.service : null
  readonly property var frame: service ? service.frame : ({})
  readonly property var settings: host ? host.settings : ({})
  readonly property bool ready: !!(service && service.ready)
  readonly property int revision: service ? service.revision : 0
  readonly property var entry: Prefs.entry(module) || Prefs.entry("cpu")
  readonly property string labels: host ? host.labels : "text"
  readonly property string unit: host ? host.unit : "Celsius"
  readonly property color s1: service ? service.series1 : foreground
  readonly property color s2: service ? service.series2 : foreground
  readonly property bool lightBar: bar && !bar.transparent && bar.background.a >= 0.95
    ? bar.background.hslLightness > 0.5 : foreground.hslLightness < 0.5
  readonly property var grader: Prefs.grader(settings, lightBar)

  // ---------------------------------------------------------- what to draw

  readonly property string look: Prefs.lookOf(settings, module)
  readonly property bool isDisk: module === "disks"
  readonly property bool isSensor: module === "sensors"
  // A disk readout's look picks picture, figure or both; what it shows
  // (speed or space) picks which picture and which figure.
  readonly property bool diskPicture: look !== "text"
  readonly property bool diskFigure: look !== "graph" && look !== "ring"
  readonly property bool diskSpeed: diskShow !== "used"
  readonly property bool diskSpace: diskShow === "used" || diskShow === "both"
  readonly property var parts: Prefs.parts(module, look)
  readonly property bool drawGraph: isDisk ? diskPicture && diskSpeed : parts.graph && !isSensor
  readonly property bool drawRing: isDisk ? diskPicture && diskSpace : parts.ring
  readonly property bool drawFigure: isDisk ? diskFigure && diskSpace : parts.figure && !isSensor
  readonly property bool drawRates: isDisk && diskFigure && diskSpeed
  readonly property bool twoSided: module === "network" || isDisk

  // ------------------------------------------------------------- readings

  readonly property var cpu: frame.cpu || ({})
  readonly property var memory: frame.memory || ({})
  readonly property var network: frame.network || ({})
  readonly property var battery: frame.battery || null
  readonly property var gpu: gpuId ? Hardware.gpuById(frame, gpuId) : Hardware.mainGpus(frame)[0] || null
  readonly property string gpuKey: gpu ? String(gpu.id) : ""
  readonly property var vram: Hardware.vramPercent(gpu)
  readonly property bool singleDisk: disk !== "all" && !!Hardware.drives(frame)[disk]
  readonly property var rates: Hardware.driveRates(frame, singleDisk ? disk : "all")
  readonly property var space: Hardware.spaceUsed(frame, singleDisk ? disk : "all")
  readonly property real memoryPercent: memory.total > 0 ? Format.number(memory.used) / memory.total * 100 : NaN
  readonly property bool charging: !!battery && (battery.status === "Charging" || battery.status === "Full")

  // The percentage behind the figure, for utilization colours.
  readonly property var percent: {
    switch (module) {
      case "cpu": return cpu.usage ? cpu.usage.total : null
      case "gpu": return Hardware.gpuHasLoad(gpu) ? gpu.load : null
      case "vram": return vram
      case "memory": return isFinite(memoryPercent) ? memoryPercent : null
      case "disks": return diskSpace && space.size > 0 ? space.fraction * 100 : null
    }
    return null
  }

  readonly property real fill: {
    switch (module) {
      case "battery": return battery ? Format.number(battery.percent) / 100 : 0
      case "disks": return space.fraction
    }
    return Format.number(percent) / 100
  }

  readonly property color fillColor: {
    if (module === "battery") return charging ? service.good : (fill <= 0.15 ? service.danger : s1)
    var graded = grader ? grader(percent) : null
    if (graded) return graded
    if (module === "disks") return fill >= 0.92 ? service.danger : (fill >= 0.8 ? service.warn : s1)
    return s1
  }
  readonly property color figureColor: (grader ? grader(percent) : null) || foreground

  readonly property string figure: {
    if (!ready) return "…"
    switch (module) {
      case "cpu": return Format.percent(cpu.usage ? cpu.usage.total : 0)
      case "memory": return Format.percent(memoryPercent)
      case "vram": return vram === null ? Format.DASH : Format.percent(vram)
      case "battery": return battery ? Format.percent(battery.percent) : Format.DASH
      case "disks": return Format.percent(space.fraction * 100)
      case "gpu":
        if (Hardware.gpuHasLoad(gpu)) return Format.percent(gpu.load)
        if (gpu && gpu.asleep) return "Off"
        return gpu ? Format.clockTiny(gpu.mhz) : Format.DASH
    }
    return ""
  }

  // Two small lines for network and disk speed.
  readonly property string upper: !ready ? "…" : module === "network" ? "↑ " + Format.rateTiny(network.tx) : "R " + Format.rateTiny(rates.read)
  readonly property string lower: !ready ? "…" : module === "network" ? "↓ " + Format.rateTiny(network.rx) : "W " + Format.rateTiny(rates.write)

  readonly property var sensorReadings: {
    var out = []
    Prefs.words(Prefs.value(settings, "barSensors")).forEach(function(id) {
      var r = Hardware.sensorReading(frame, id, unit)
      if (r) out.push(r)
    })
    if (out.length === 0) {
      var cpuTemp = Hardware.sensorReading(frame, "cpu", unit)
      if (cpuTemp) out.push(cpuTemp)
    }
    return out
  }

  readonly property string name: tag || entry.short
  readonly property string glyph: module === "battery" && battery ? Hardware.batteryGlyph(battery.percent, charging) : entry.glyph

  // Graph series straight from the service's histories.
  readonly property var graphSeries: {
    if (!service) return []
    if (module === "cpu") return [service.series("cpu.user"), service.series("cpu.system")]
    if (module === "memory") return [service.series("mem.used")]
    if (module === "gpu") return [service.series("gpu.load:" + gpuKey)]
    if (module === "vram") return [service.series("gpu.vram:" + gpuKey)]
    return []
  }
  readonly property var upSeries: !service ? [] : module === "network" ? service.series("net.tx") : service.series(singleDisk ? "disk.read:" + disk : "disk.read")
  readonly property var downSeries: !service ? [] : module === "network" ? service.series("net.rx") : service.series(singleDisk ? "disk.write:" + disk : "disk.write")

  // --------------------------------------------------------------- tooltip

  function describe() {
    if (!ready) return entry.label + ": starting the collector…"
    var bits = []
    switch (module) {
      case "cpu":
        bits.push("CPU " + Format.percent(cpu.usage ? cpu.usage.total : 0))
        if (Format.clockText(cpu.mhz)) bits.push(Format.clockText(cpu.mhz))
        if (Format.known(cpu.temp)) bits.push(Format.degreesLong(cpu.temp, unit))
        return bits.join(" · ") + "\nLoad " + Format.loadAverage(cpu.load) + " · up " + Format.duration(cpu.uptime)
      case "gpu":
      case "vram":
        if (!gpu) return "No graphics card found"
        bits.push(Hardware.gpuName(gpu) + (Hardware.gpuHasLoad(gpu) ? " " + Format.percent(gpu.load) : ""))
        if (gpu.asleep) bits.push("asleep")
        else if (!Hardware.gpuHasLoad(gpu)) bits.push("load not reported")
        if (Format.clockText(gpu.mhz)) bits.push(Format.clockText(gpu.mhz))
        if (Format.known(gpu.temp)) bits.push(Format.degreesLong(gpu.temp, unit))
        if (Format.wattsText(gpu.watts)) bits.push(Format.wattsText(gpu.watts))
        if (vram !== null) bits.push("VRAM " + Format.sizePair(gpu.vramUsed, gpu.vramTotal))
        return bits.join(" · ")
      case "memory":
        return "Memory " + Format.percent(memoryPercent) + " · " + Format.sizePair(memory.used, memory.total)
          + (Format.number(memory.swapUsed) > 0 ? "\nSwap " + Format.sizeText(memory.swapUsed) : "")
      case "network":
        return (network.defaultIface || "Network") + " · ↓ " + Format.rateText(network.rx) + " · ↑ " + Format.rateText(network.tx)
      case "disks":
        var speed = "read " + Format.rateText(rates.read) + " · write " + Format.rateText(rates.write)
        var used = space.size > 0 ? Format.sizePair(space.used, space.size) + " used (" + Format.percent(space.fraction * 100) + ")" : "no volumes mounted"
        var title = singleDisk ? disk : "All disks"
        return diskShow === "used" ? title + " · " + used + "\n" + speed : title + " · " + speed + "\n" + used
      case "sensors":
        sensorReadings.forEach(function(r) { bits.push(r.label + " " + (r.kind === "temp" ? Format.degreesLong(r.celsius, unit) : r.text + (r.unit ? " " + r.unit : ""))) })
        var fans = Hardware.fanSummary(frame)
        if (bits.length < 3 && fans.count > 0) bits.push(fans.average > 0 ? "Fans " + Math.round(fans.average) + " rpm" : "Fans off")
        return bits.length ? bits.join(" · ") : "No sensors"
      case "battery":
        if (!battery) return "No battery"
        bits.push("Battery " + Format.percent(battery.percent), String(battery.status || ""))
        if (battery.status === "Discharging" && Format.number(battery.timeToEmpty) > 0) bits.push(Format.hoursMinutes(battery.timeToEmpty) + " left")
        if (battery.status === "Charging" && Format.number(battery.timeToFull) > 0) bits.push(Format.hoursMinutes(battery.timeToFull) + " to full")
        return bits.join(" · ")
    }
    return entry.label
  }

  // ---------------------------------------------------------------- frame

  labelVisible: false
  hasVisualContent: true
  text: entry.glyph
  horizontalMargin: 5
  fixedWidth: vertical ? -1 : across.implicitWidth + scaledHorizontalMargin * 2
  fixedHeight: vertical ? Math.max(Style.bar.iconSlot, down.implicitHeight + Style.space(7)) : -1
  tooltipText: describe()

  readonly property real cell: Math.max(12, barSize - Style.space(6))
  readonly property real pictureWidth: vertical ? cell : (host ? host.graphWidth : 36)
  readonly property real pictureHeight: vertical ? Style.space(8)
    : (labels === "text" && !isSensor && letters.height > 0 ? letters.height : Math.max(8, barSize - Style.space(11)))

  TextMetrics { id: wide; font.family: readout.fontFamily; font.pixelSize: Style.font.body; text: "100%" }
  TextMetrics { id: wideSmall; font.family: readout.fontFamily; font.pixelSize: Style.font.caption; text: "↓ 999K" }

  component BarText: Text {
    textFormat: Text.PlainText
    color: readout.foreground
    font.family: readout.fontFamily
    font.pixelSize: Style.font.body
    renderType: Text.NativeRendering
  }

  // Fits a narrow vertical bar by shrinking rather than clipping.
  component FitText: BarText {
    width: parent ? parent.width : 0
    horizontalAlignment: Text.AlignHCenter
    fontSizeMode: Text.HorizontalFit
    minimumPixelSize: 6
    font.pixelSize: Style.font.caption
  }

  // ------------------------------------------------------- horizontal bar

  Row {
    id: across
    visible: !readout.vertical
    anchors.centerIn: parent
    spacing: Style.space(4)

    BarText {
      visible: !readout.isSensor && readout.labels === "icon"
      anchors.verticalCenter: parent.verticalCenter
      text: readout.glyph
      font.pixelSize: Style.bar.iconFont
    }

    Kit.Letters {
      id: letters
      visible: !readout.isSensor && readout.labels === "text"
      anchors.verticalCenter: parent.verticalCenter
      text: readout.name
      color: readout.foreground
      family: readout.fontFamily
      maxHeight: readout.barSize - Style.space(3)
    }

    BarText {
      visible: !readout.isSensor && readout.labels === "inline"
      anchors.verticalCenter: parent.verticalCenter
      text: readout.name
      font.bold: true
    }

    Loader {
      active: !readout.vertical && readout.drawGraph
      visible: active
      anchors.verticalCenter: parent.verticalCenter
      sourceComponent: readout.twoSided ? mirrorGraph : barGraph
    }

    Loader {
      active: !readout.vertical && readout.drawRing
      visible: active
      anchors.verticalCenter: parent.verticalCenter
      sourceComponent: ring
    }

    Loader {
      active: !readout.vertical && (readout.drawRates || (readout.module === "network" && readout.drawFigure))
      visible: active
      anchors.verticalCenter: parent.verticalCenter
      sourceComponent: Column {
        BarText { width: Math.ceil(wideSmall.advanceWidth); text: readout.upper; font.pixelSize: Style.font.caption; lineHeight: 0.95 }
        BarText { width: Math.ceil(wideSmall.advanceWidth); text: readout.lower; font.pixelSize: Style.font.caption; lineHeight: 0.95 }
      }
    }

    BarText {
      visible: readout.drawFigure && readout.module !== "network"
      anchors.verticalCenter: parent.verticalCenter
      width: Math.ceil(wide.advanceWidth)
      horizontalAlignment: Text.AlignRight
      text: readout.figure
      color: readout.figureColor
    }

    // Sensors: a label and figure for each chosen sensor.
    Repeater {
      model: (!readout.vertical && readout.isSensor ? readout.sensorReadings : []).length

      Row {
        required property int index
        readonly property var modelData: (!readout.vertical && readout.isSensor ? readout.sensorReadings : [])[index] || ({})
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(3)

        BarText {
          visible: readout.labels === "icon"
          anchors.verticalCenter: parent.verticalCenter
          text: modelData.glyph
          font.pixelSize: Style.bar.iconFont
        }
        Kit.Letters {
          visible: readout.labels === "text"
          anchors.verticalCenter: parent.verticalCenter
          text: modelData.short
          color: readout.foreground
          family: readout.fontFamily
          maxHeight: readout.barSize - Style.space(3)
        }
        BarText {
          visible: readout.labels === "inline"
          anchors.verticalCenter: parent.verticalCenter
          text: modelData.short
          font.bold: true
        }
        TextMetrics { id: sensorWide; font.family: readout.fontFamily; font.pixelSize: Style.font.body; text: modelData.kind === "fan" ? "9999" : "100°" }
        BarText {
          anchors.verticalCenter: parent.verticalCenter
          width: Math.ceil(sensorWide.advanceWidth)
          horizontalAlignment: Text.AlignRight
          text: readout.ready ? modelData.text : "…"
        }
      }
    }
  }

  // --------------------------------------------------------- vertical bar

  Column {
    id: down
    visible: readout.vertical
    anchors.centerIn: parent
    width: readout.cell
    spacing: Style.space(2)

    FitText {
      visible: !readout.isSensor && readout.labels !== "icon"
      text: readout.name
      font.bold: true
    }
    BarText {
      visible: !readout.isSensor && readout.labels === "icon"
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: readout.glyph
      font.pixelSize: Style.bar.iconFont
    }

    Loader {
      active: readout.vertical && readout.drawGraph
      visible: active
      anchors.horizontalCenter: parent.horizontalCenter
      sourceComponent: readout.twoSided ? mirrorGraph : barGraph
    }
    Loader {
      active: readout.vertical && readout.drawRing
      visible: active
      anchors.horizontalCenter: parent.horizontalCenter
      sourceComponent: ring
    }

    // Speeds one per line, coloured like their half of the graph, since the
    // arrows do not fit across a vertical bar.
    FitText {
      visible: readout.drawRates || (readout.module === "network" && readout.drawFigure)
      text: readout.ready ? Format.rateTiny(readout.module === "network" ? readout.network.tx : readout.rates.read) : "…"
      color: readout.s2
    }
    FitText {
      visible: readout.drawRates || (readout.module === "network" && readout.drawFigure)
      text: readout.ready ? Format.rateTiny(readout.module === "network" ? readout.network.rx : readout.rates.write) : "…"
      color: readout.s1
    }
    FitText {
      visible: readout.drawFigure && readout.module !== "network"
      text: readout.figure
      color: readout.figureColor
    }

    Repeater {
      model: (readout.vertical && readout.isSensor ? readout.sensorReadings : []).length
      Column {
        required property int index
        readonly property var modelData: (readout.vertical && readout.isSensor ? readout.sensorReadings : [])[index] || ({})
        width: parent.width
        FitText { visible: readout.labels !== "icon"; text: modelData.short; font.bold: true }
        BarText {
          visible: readout.labels === "icon"
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          text: modelData.glyph
          font.pixelSize: Style.bar.iconFont
        }
        FitText { text: readout.ready ? modelData.text : "…" }
      }
    }
  }

  // -------------------------------------------------------------- pictures

  Component {
    id: barGraph
    Kit.Bars {
      width: readout.pictureWidth
      height: readout.pictureHeight
      barWidth: 1
      gap: 1
      ceiling: 100
      series: readout.graphSeries
      revision: readout.revision
      colors: readout.module === "vram" ? [readout.s2] : [readout.s1, readout.s2]
      grader: readout.grader
      baseColor: Util.alpha(readout.foreground, 0.28)
    }
  }

  Component {
    id: mirrorGraph
    Kit.Mirror {
      width: readout.pictureWidth
      height: readout.pictureHeight
      barWidth: 1
      gap: 1
      up: readout.upSeries
      down: readout.downSeries
      revision: readout.revision
      upColor: readout.s2
      downColor: readout.s1
      floor: readout.module === "network" ? 10240 : 262144
      midline: Util.alpha(readout.foreground, 0.32)
    }
  }

  Component {
    id: ring
    Kit.Arc {
      size: readout.vertical ? Style.space(14) : Math.max(10, readout.barSize - Style.space(10))
      thickness: Style.spaceReal(2.4)
      value: readout.fill
      color: readout.fillColor
      track: Util.alpha(readout.foreground, 0.22)
    }
  }
}

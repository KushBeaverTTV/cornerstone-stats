import QtQuick
import qs.Commons
import qs.Commons as Commons
import "../../lib/Prefs.js" as Prefs

// What every tab starts from: the widget that hosts it, the latest frame,
// its settings and the graph colours. A tab stacks its cards top to bottom.
Column {
  property var host: null

  readonly property var service: host ? host.service : null
  readonly property var frame: service ? service.frame : ({})
  readonly property bool ready: !!(service && service.ready)
  readonly property int revision: service ? service.revision : 0
  readonly property var settings: host ? host.settings : ({})
  readonly property string unit: host ? host.unit : "Celsius"
  readonly property var procs: frame.procs || null
  readonly property bool lightTheme: Commons.Color.popups.background.hslLightness > 0.5

  readonly property color ink: Commons.Color.popups.text
  readonly property color s1: service ? service.series1 : Commons.Color.accent
  readonly property color s2: service ? service.series2 : Commons.Color.accent
  readonly property color s3: service ? service.series3 : Commons.Color.accent
  readonly property color warn: service ? service.warn : Commons.Color.urgent
  readonly property color danger: service ? service.danger : Commons.Color.urgent
  readonly property color good: service ? service.good : Commons.Color.accent
  readonly property color quiet: Util.alpha(ink, 0.55)

  function flag(name) { return Prefs.flag(settings, name) }
  function series(name) { return service ? service.series(name) : [] }

  width: parent ? parent.width : implicitWidth
  spacing: Style.space(10)
}

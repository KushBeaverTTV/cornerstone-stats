import QtQuick
import qs.Commons
import qs.Commons as Commons

// A figure with its unit set smaller beside it: "13.8 GB", "63°".
Row {
  id: amount

  property string value: ""
  property string unit: ""
  property bool bold: true
  property real fade: 0.95
  property real size: Style.font.body
  property color ink: Commons.Color.popups.text

  spacing: Style.space(2)

  Label {
    id: figure
    text: amount.value
    fade: amount.fade
    color: amount.ink
    font.pixelSize: amount.size
    font.bold: amount.bold
  }

  Label {
    visible: amount.unit !== ""
    anchors.baseline: figure.baseline
    text: amount.unit
    fade: 0.5
    color: amount.ink
    font.pixelSize: Math.max(8, Math.round(amount.size * 0.75))
  }
}

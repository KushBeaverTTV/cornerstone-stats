import QtQuick
import qs.Commons

// One big figure with its name under it, centred in its width: "155 KB/s"
// over "Upload".
Column {
  id: hero

  property string value: ""
  property string unit: ""
  property string label: ""

  spacing: 0

  Amount {
    anchors.horizontalCenter: parent.horizontalCenter
    value: hero.value
    unit: hero.unit
    size: Style.font.heading
  }

  Label {
    anchors.horizontalCenter: parent.horizontalCenter
    text: hero.label
    fade: 0.5
    font.pixelSize: Style.font.caption
  }
}

import QtQuick
import qs.Commons
import qs.Commons as Commons

// A legend under a graph: "● User  14%   ● System  5%". Each entry takes an
// equal share of the width with its figure at the right end of its share.
// entries: [{color, label, value, unit}]
Row {
  id: key

  property var entries: []

  width: parent ? parent.width : implicitWidth
  height: Style.space(18)
  spacing: Style.space(12)

  Repeater {
    model: (key.entries).length

    Item {
      required property int index
      readonly property var modelData: (key.entries)[index] || ({})
      readonly property int count: Math.max(1, key.entries.length)
      width: (key.width - key.spacing * (count - 1)) / count
      height: key.height

      Rectangle {
        id: swatch
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(8)
        height: width
        radius: width / 2
        color: modelData.color || Commons.Color.popups.text
      }

      Label {
        anchors.left: swatch.right
        anchors.leftMargin: Style.space(6)
        anchors.right: figure.left
        anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        text: String(modelData.label || "")
        fade: 0.85
        font.pixelSize: Style.font.bodySmall
      }

      Amount {
        id: figure
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        value: String(modelData.value === undefined ? "" : modelData.value)
        unit: String(modelData.unit || "")
        size: Style.font.bodySmall
      }
    }
  }
}

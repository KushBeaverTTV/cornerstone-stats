import QtQuick
import qs.Commons
import qs.Commons as Commons

// A card's first line: its subject in accent capitals on the left, a short
// status ("4.16 GHz, 62°") on the right.
Item {
  id: title

  property string text: ""
  property string detail: ""

  width: parent ? parent.width : implicitWidth
  implicitHeight: Math.max(name.implicitHeight, side.implicitHeight)

  Heading {
    id: name
    anchors.left: parent.left
    anchors.verticalCenter: parent.verticalCenter
    width: Math.min(implicitWidth, title.width - (side.text ? side.implicitWidth + Style.space(12) : 0))
    text: title.text
    font.pixelSize: Style.font.body
    font.capitalization: Font.MixedCase
    font.letterSpacing: 0.4
  }

  Label {
    id: side
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    text: title.detail
    fade: 0.6
    font.pixelSize: Style.font.bodySmall
  }
}

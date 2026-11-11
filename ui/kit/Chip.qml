import QtQuick
import qs.Commons
import qs.Commons as Commons

// A small rounded button: a filter (selected), an action, or a confirmed
// action in a strong colour ("End task").
Rectangle {
  id: chip

  property string text: ""
  property bool selected: false
  property bool strong: false
  property color tint: Commons.Color.popups.text
  property bool big: false
  signal clicked()

  readonly property color ink: Commons.Color.popups.text

  width: words.implicitWidth + Style.space(big ? 14 : 12)
  height: Style.space(big ? 20 : 18)
  radius: height / 2
  color: strong ? Qt.rgba(tint.r, tint.g, tint.b, hover.containsMouse ? 1 : 0.82)
       : selected ? Util.alpha(ink, 0.24)
       : Util.alpha(ink, hover.containsMouse ? 0.22 : (big ? 0.09 : 0.14))

  Label {
    id: words
    anchors.centerIn: parent
    text: chip.text
    color: chip.strong ? "#ffffff" : chip.ink
    fade: chip.selected || chip.strong || hover.containsMouse ? 1 : 0.8
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  MouseArea {
    id: hover
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: chip.clicked()
  }
}

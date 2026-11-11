import QtQuick
import qs.Commons
import qs.Commons as Commons

// "● Label  note ........ 13.8 GB ◔": one reading on one row. The dot keys it
// to a graph series; the trailing slot takes a small picture (an Arc).
Item {
  id: line

  property string label: ""
  property string note: ""
  property string value: ""
  property string unit: ""
  property color dot: "transparent"
  property bool keepDotSpace: false
  property real labelFade: 0.85
  property bool bold: true
  property color ink: Commons.Color.popups.text
  default property alias trailing: tail.data

  readonly property bool hasDot: dot.a > 0 || keepDotSpace
  readonly property real dotRoom: hasDot ? Style.space(15) : 0

  width: parent ? parent.width : implicitWidth
  implicitHeight: Math.max(Style.space(20), name.implicitHeight)

  Rectangle {
    visible: line.dot.a > 0
    anchors.left: parent.left
    anchors.verticalCenter: parent.verticalCenter
    width: Style.space(8)
    height: width
    radius: width / 2
    color: line.dot
  }

  Label {
    id: name
    anchors.left: parent.left
    anchors.leftMargin: line.dotRoom
    anchors.verticalCenter: parent.verticalCenter
    width: Math.min(implicitWidth, line.width - line.dotRoom - right.width - noteText.room - Style.space(10))
    text: line.label
    fade: line.labelFade
  }

  Label {
    id: noteText
    readonly property real room: visible ? implicitWidth + Style.space(7) : 0
    visible: line.note !== ""
    anchors.left: name.right
    anchors.leftMargin: Style.space(7)
    anchors.verticalCenter: parent.verticalCenter
    text: line.note
    fade: 0.45
    font.pixelSize: Style.font.caption
  }

  Row {
    id: right
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(8)

    Amount {
      anchors.verticalCenter: parent.verticalCenter
      value: line.value
      unit: line.unit
      bold: line.bold
      ink: line.ink
    }

    Item {
      id: tail
      visible: children.length > 0
      anchors.verticalCenter: parent.verticalCenter
      width: childrenRect.width
      height: childrenRect.height
    }
  }
}

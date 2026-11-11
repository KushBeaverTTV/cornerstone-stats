import QtQuick
import qs.Commons
import qs.Commons as Commons

// A large ring with its reading inside: an optional word on top ("CPU"),
// the figure and unit, a name under it and a faint line under that.
Item {
  id: dial

  property real value: 0
  property color color: Commons.Color.accent
  property var parts: []
  property string caption: ""
  property string figure: ""
  property string unit: ""
  property string name: ""
  property string sub: ""
  property real figureSize: Style.font.display
  property real size: Style.space(86)

  width: size
  height: size

  Arc {
    anchors.fill: parent
    size: dial.size
    value: dial.value
    color: dial.color
    parts: dial.parts
    thickness: Style.spaceReal(5)
  }

  Column {
    anchors.centerIn: parent
    width: dial.size - Style.space(16)
    spacing: 0

    Label {
      visible: dial.caption !== ""
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: dial.caption
      fade: 0.7
      font.pixelSize: Style.font.caption
      font.bold: true
    }

    Amount {
      anchors.horizontalCenter: parent.horizontalCenter
      value: dial.figure
      unit: dial.unit
      size: dial.figureSize
    }

    Label {
      visible: dial.name !== ""
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: dial.name
      fade: 0.7
      font.pixelSize: Style.font.caption
      font.bold: true
      font.capitalization: Font.AllUppercase
      font.letterSpacing: 0.8
    }

    Label {
      visible: dial.sub !== ""
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: dial.sub
      fade: 0.5
      font.pixelSize: Style.font.caption
    }
  }
}

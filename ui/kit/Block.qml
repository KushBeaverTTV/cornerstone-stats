import QtQuick
import qs.Commons
import qs.Commons as Commons

// A card: one topic of a page on a faint fill with a hairline edge.
// Children stack top to bottom.
Rectangle {
  id: block

  property real inset: Style.space(12)
  property alias spacing: stack.spacing
  default property alias content: stack.data

  width: parent ? parent.width : implicitWidth
  implicitHeight: stack.implicitHeight + inset * 2
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.035)
  border.width: 1
  border.color: Util.alpha(Commons.Color.popups.text, 0.09)

  Column {
    id: stack
    x: block.inset
    y: block.inset
    width: block.width - block.inset * 2
    spacing: Style.space(8)
  }
}

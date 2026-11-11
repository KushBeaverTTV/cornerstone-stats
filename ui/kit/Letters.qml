import QtQuick
import QtQuick.Window
import qs.Commons

// A short name stacked one capital per row (C over P over U), the way iStat
// labels its menu bar readouts. Each row is exactly one capital high and
// rows are one device pixel apart, so three letters fit a 26 px bar.
Item {
  id: letters

  property string text: ""
  property color color: "white"
  property string family: Style.font.family
  property real preferredSize: Style.spaceReal(10)
  property real maxHeight: 0

  readonly property int count: text.length
  readonly property real ratio: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
  // Capitals are about 0.73 of the pixel size in the bar fonts; shrink the
  // size until the stack fits the room it has.
  readonly property real pixelSize: maxHeight > 0 && count > 0
    ? Math.max(6, Math.min(preferredSize, Math.floor((maxHeight - (count - 1) / ratio) / count / 0.75)))
    : preferredSize
  readonly property real capHeight: Math.ceil(capital.tightBoundingRect.height * ratio) / ratio
  readonly property real capTop: Math.round((capital.tightBoundingRect.y - capital.boundingRect.y) * ratio) / ratio
  readonly property real step: capHeight + 1 / ratio

  implicitWidth: Math.ceil(capital.advanceWidth) + 1
  implicitHeight: count > 0 ? capHeight + (count - 1) * step : 0
  width: implicitWidth
  height: implicitHeight

  TextMetrics {
    id: capital
    font.family: letters.family
    font.pixelSize: letters.pixelSize
    font.bold: true
    text: "W"
  }

  Repeater {
    model: letters.text.split("")

    Text {
      required property string modelData
      required property int index
      y: index * letters.step - letters.capTop
      width: letters.width
      horizontalAlignment: Text.AlignHCenter
      textFormat: Text.PlainText
      text: modelData
      color: letters.color
      font.family: letters.family
      font.pixelSize: letters.pixelSize
      font.bold: true
      renderType: Text.NativeRendering
    }
  }
}

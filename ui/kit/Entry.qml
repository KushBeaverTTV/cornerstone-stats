import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// A one-line search box that builds its text from key presses. Qt's own
// text inputs read the clipboard whenever another program changes it, so a
// focused one would let any program make the shell read what it copied.
// This box never touches the clipboard: it takes printable keys, Backspace
// (Ctrl+Backspace or Ctrl+U clears), Return and Escape, and draws its own
// caret. Escape clears the text, or gives focus back when it is empty.
Item {
  id: entry

  property string text: ""
  property string placeholder: ""
  property int limit: 200
  property real pad: Style.space(4)
  signal accepted()

  readonly property color ink: Commons.Color.popups.text
  readonly property bool hot: pointer.containsMouse
  readonly property var edge: Border.controlSpec(activeFocus ? "focus" : (hot ? "hover-cursor" : "normal"), ink, Commons.Color.accent)

  function printable(event) {
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return ""
    var s = String(event.text || "")
    for (var i = 0; i < s.length; i++) {
      var code = s.charCodeAt(i)
      if (code < 32 || code === 127) return ""
    }
    return s
  }

  activeFocusOnTab: true
  width: parent ? parent.width : Style.space(160)
  implicitHeight: words.implicitHeight + pad * 2 + Border.top(edge) + Border.bottom(edge)

  Keys.onPressed: function(event) {
    var ctrl = (event.modifiers & Qt.ControlModifier) !== 0
    if (event.key === Qt.Key_Backspace) {
      entry.text = ctrl ? "" : entry.text.slice(0, -1)
    } else if (ctrl && event.key === Qt.Key_U) {
      entry.text = ""
    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      entry.accepted()
    } else if (event.key === Qt.Key_Escape) {
      if (entry.text !== "") entry.text = ""
      else entry.focus = false
    } else {
      var typed = entry.printable(event)
      if (typed === "") return
      entry.text = (entry.text + typed).slice(0, entry.limit)
    }
    event.accepted = true
  }

  BorderSurface {
    anchors.fill: parent
    color: Style.controlFill(entry.activeFocus, entry.hot, entry.ink, Commons.Color.accent)
    borderSpec: entry.edge
    radius: Style.cornerRadius
  }

  Item {
    id: field
    anchors.fill: parent
    anchors.leftMargin: Style.spacing.controlPaddingX + Border.left(entry.edge)
    anchors.rightMargin: Style.spacing.controlPaddingX + Border.right(entry.edge)
    clip: true

    Label {
      id: words
      anchors.verticalCenter: parent.verticalCenter
      // Long text keeps its end, where typing happens, in view.
      x: Math.min(0, field.width - implicitWidth - caret.width)
      elide: Text.ElideNone
      text: entry.text !== "" ? entry.text : entry.placeholder
      fade: entry.text !== "" ? 1 : 0.45
      font.pixelSize: Style.font.bodySmall
    }

    Rectangle {
      id: caret
      visible: entry.activeFocus && blink.lit
      x: entry.text !== "" ? words.x + words.implicitWidth + 1 : 0
      anchors.verticalCenter: parent.verticalCenter
      width: Math.max(1, Style.spaceReal(1))
      height: words.font.pixelSize * 1.2
      color: entry.ink
    }
  }

  Timer {
    id: blink
    property bool lit: true
    interval: 530
    repeat: true
    running: entry.activeFocus
    onRunningChanged: lit = true
    onTriggered: lit = !lit
  }

  MouseArea {
    id: pointer
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.IBeamCursor
    onClicked: entry.forceActiveFocus()
  }
}

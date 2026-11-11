import QtQuick
import qs.Commons
import qs.Commons as Commons

// Plain text in the panel's font and ink. Everything shown to the user goes
// through PlainText, so a process title or unit description can never be
// read as markup.
Text {
  property real fade: 1

  textFormat: Text.PlainText
  color: Commons.Color.popups.text
  opacity: fade
  font.family: Style.font.family
  font.pixelSize: Style.font.body
  elide: Text.ElideRight
}

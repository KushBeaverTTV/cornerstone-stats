import QtQuick
import qs.Commons
import qs.Commons as Commons

// A section name in small accent capitals: PROCESSES, TEMPERATURE.
Label {
  color: Commons.Color.accent
  font.pixelSize: Style.font.caption
  font.bold: true
  font.letterSpacing: 1.1
  font.capitalization: Font.AllUppercase
  // Nerd Font outlines rise above the em box; keep the top row whole when a
  // list clips.
  topPadding: Math.ceil(Style.font.caption * 0.15)
}

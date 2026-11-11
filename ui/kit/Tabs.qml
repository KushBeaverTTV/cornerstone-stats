import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../../lib/Prefs.js" as Prefs

// The row of tabs across the top of the panel, Settings last. Every slot is
// as wide as the longest name needs, so names are never cut; the panel asks
// for `neededWidth`. The open tab is filled, a hovered one tinted.
Item {
  id: strip

  property var tabs: []
  property string current: ""
  property int pointed: -1
  signal picked(string id)
  signal pointedAt(int index, bool inside)

  readonly property var all: tabs.concat(["settings"])
  readonly property real slot: all.length > 0 ? width / all.length : width
  readonly property real neededWidth: (Math.ceil(longest.advanceWidth) + Style.space(14)) * all.length
  readonly property color ink: Commons.Color.popups.text

  TextMetrics {
    id: longest
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
    font.bold: true
    text: strip.all.map(function(id) { return (Prefs.entry(id) || {}).label || id })
      .reduce(function(a, b) { return b.length > a.length ? b : a }, "")
  }

  width: parent ? parent.width : implicitWidth
  implicitHeight: Style.space(28)

  Repeater {
    model: strip.all

    Item {
      id: tab
      required property string modelData
      required property int index
      readonly property bool active: strip.current === modelData
      readonly property bool hot: strip.pointed === index || mouse.containsMouse

      x: index * strip.slot
      width: strip.slot
      height: strip.height

      Rectangle {
        anchors.centerIn: parent
        width: Math.max(Style.space(24), strip.slot - Style.space(4))
        height: parent.height
        radius: Style.cornerRadius
        color: tab.active ? Style.selectedFillFor(strip.ink, Commons.Color.accent)
             : (tab.hot ? Style.hoverFillFor(strip.ink, Commons.Color.accent) : "transparent")
        border.width: tab.active ? 1 : 0
        border.color: Util.alpha(strip.ink, 0.12)
        Behavior on color { ColorAnimation { duration: 90 } }
      }

      Label {
        anchors.centerIn: parent
        width: Math.min(implicitWidth, strip.slot - Style.space(8))
        horizontalAlignment: Text.AlignHCenter
        text: (Prefs.entry(tab.modelData) || {}).label || tab.modelData
        fade: tab.active || tab.hot ? 1 : 0.7
        font.pixelSize: Style.font.caption
        font.bold: true
        renderType: Text.NativeRendering
      }

      MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: strip.picked(tab.modelData)
        onContainsMouseChanged: strip.pointedAt(tab.index, containsMouse)
      }
    }
  }
}

import QtQuick
import qs.Commons
import qs.Commons as Commons
import "../../lib/Format.js" as Format
import "../../lib/Hardware.js" as Hardware

// The PROCESSES block of a page: the busiest few, one or two figures each.
// "Show all" unfolds every process, sorted by this page's figure and
// searchable; whether it is unfolded and what is searched live on the host,
// so they follow you from page to page. Hovering a row offers End.
Column {
  id: list

  property var host: null
  property var rows: []
  property var allRows: []
  property int total: 0
  // [{key, kind: "share" | "size" | "rate", title}]
  property var columns: [{ key: "cpu", kind: "share", title: "" }]
  property string sortKey: columns.length > 0 ? String(columns[0].key) : "cpu"
  property string title: "Processes"
  property string caption: ""
  property string empty: "Nothing to show yet"
  property bool expandable: true
  property bool endable: true
  property real figureWidth: Style.space(66)
  property int limit: 200

  readonly property bool open: expandable && !!host && host.processesOpen === true
  readonly property string query: host ? String(host.processQuery || "") : ""
  readonly property var shown: open ? Hardware.processRows(allRows, query, sortKey) : (Array.isArray(rows) ? rows : [])
  readonly property bool waiting: open && (!Array.isArray(allRows) || allRows.length === 0)

  function figure(row, column) {
    var raw = row ? row[column.key] : 0
    if (column.kind === "share") return Format.share(raw)
    if (column.kind === "rate") return Format.rateOrDash(raw)
    if (column.kind === "percent") return Format.percent(raw)
    return Format.sizeText(raw)
  }

  function unfold(on) {
    if (host) host.showAllProcesses(on)
  }

  width: parent ? parent.width : implicitWidth
  spacing: Style.space(4)

  Item {
    width: parent.width
    height: Style.space(18)

    Heading {
      id: heading
      anchors.left: parent.left
      anchors.verticalCenter: parent.verticalCenter
      text: list.title + (list.open && list.total > 0 ? " · " + list.total : "")
    }

    Label {
      visible: list.expandable
      anchors.left: heading.right
      anchors.leftMargin: Style.space(10)
      anchors.baseline: heading.baseline
      text: list.open ? "Show less" : "Show all"
      fade: more.containsMouse ? 0.9 : 0.5
      font.pixelSize: Style.font.caption
      font.bold: true

      MouseArea {
        id: more
        anchors.fill: parent
        anchors.margins: -Style.space(4)
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: list.unfold(!list.open)
      }
    }

    Row {
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(8)
      Repeater {
        model: list.columns
        Label {
          required property var modelData
          width: list.figureWidth
          horizontalAlignment: Text.AlignRight
          text: String(modelData.title || "")
          fade: 0.5
          font.pixelSize: Style.font.caption
          font.bold: true
        }
      }
    }
  }

  Label {
    visible: list.caption !== ""
    width: parent.width
    text: list.caption
    fade: 0.45
    font.pixelSize: Style.font.caption
    wrapMode: Text.WordWrap
  }

  Entry {
    id: search
    visible: list.open
    height: visible ? implicitHeight : 0
    placeholder: "Search processes"
    text: list.query
    onTextChanged: if (list.host && list.host.processQuery !== text) list.host.processQuery = text
    onActiveFocusChanged: if (list.host) list.host.typing = activeFocus
    // The host arrives after the page is built, so register when it does.
    function register() { if (list.host && list.expandable) list.host.searchBox = search }
    Connections { target: list; function onHostChanged() { search.register() } }
    Component.onCompleted: register()
    Component.onDestruction: if (list.host && list.host.searchBox === search) list.host.searchBox = null
  }

  Label {
    visible: list.waiting || list.shown.length === 0
    text: list.waiting ? "Collecting…" : (list.open && list.query ? "No match" : list.empty)
    fade: 0.45
    font.pixelSize: Style.font.bodySmall
  }

  Repeater {
    model: list.waiting ? 0 : Math.min(list.shown.length, list.limit)

    Item {
      id: row
      required property int index
      readonly property var proc: list.shown[index] || ({})
      readonly property bool hot: hover.containsMouse || ender.asking

      width: list.width
      height: Style.space(20)

      MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }

      Rectangle {
        visible: row.hot && list.endable
        anchors.fill: parent
        anchors.leftMargin: -Style.space(4)
        anchors.rightMargin: -Style.space(4)
        radius: Style.space(4)
        color: Util.alpha(Commons.Color.popups.text, 0.07)
      }

      Row {
        anchors.left: parent.left
        anchors.right: ender.visible ? ender.left : figures.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(6)

        Label {
          width: Math.min(implicitWidth, parent.width - (copies.visible ? copies.implicitWidth + parent.spacing : 0))
          text: String(row.proc.name || "")
          fade: 0.92
        }
        Label {
          id: copies
          visible: Format.number(row.proc.count, 1) > 1
          anchors.verticalCenter: parent.verticalCenter
          text: "×" + Format.number(row.proc.count, 1)
          fade: 0.4
          font.pixelSize: Style.font.caption
        }
      }

      Ender {
        id: ender
        anchors.right: figures.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        pid: Format.number(row.proc.pid)
        shown: list.endable && hover.containsMouse
        visible: list.endable && (shown || asking)
      }

      Row {
        id: figures
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Repeater {
          model: list.columns
          Label {
            required property var modelData
            width: list.figureWidth
            horizontalAlignment: Text.AlignRight
            text: list.figure(row.proc, modelData)
            fade: 0.9
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }
  }

  Label {
    visible: list.shown.length > list.limit
    text: "and " + (list.shown.length - list.limit) + " more. Search to narrow it down."
    fade: 0.45
    font.pixelSize: Style.font.caption
  }
}

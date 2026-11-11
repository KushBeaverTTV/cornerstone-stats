import QtQuick
import qs.Commons
import qs.Commons as Commons

// A history drawn as thin upright bars, newest on the right; several series
// stack in the order given (user under system). The arrays are the service's
// own, updated in place, so `revision` says when to paint again. Edges snap
// to device pixels so one-pixel bars stay sharp at any scale.
Canvas {
  id: bars

  property var series: []          // [[number|null]]
  property var colors: []          // one per series
  property int revision: 0
  property real ceiling: 0         // fixed top, or 0 to fit the visible samples
  property real floor: 1           // lowest automatic top, keeps idle noise flat
  property int barWidth: 2
  property int gap: 1
  property var grader: null        // percent -> colour, or null: one colour per series
  property real secondFade: 0.62   // later series under a grader keep their place
  property color baseColor: Util.alpha(Commons.Color.popups.text, 0.14)

  readonly property int pitch: Math.max(1, barWidth + gap)
  readonly property int slots: Math.max(1, Math.floor((width + gap) / pitch))

  antialiasing: false
  renderStrategy: Canvas.Cooperative

  function refresh() { if (visible && width > 0 && height > 0) requestPaint() }
  onRevisionChanged: refresh()
  onSeriesChanged: refresh()
  onColorsChanged: refresh()
  onGraderChanged: refresh()
  onCeilingChanged: refresh()
  onWidthChanged: refresh()
  onHeightChanged: refresh()
  onVisibleChanged: refresh()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    var ratio = Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    function snap(v) { return Math.round(v * ratio) / ratio }
    var px = 1 / ratio
    var lists = Array.isArray(series) ? series : []
    var length = 0
    for (var s = 0; s < lists.length; s++) if (lists[s] && lists[s].length > length) length = lists[s].length
    var first = Math.max(0, length - slots)
    var room = Math.max(1, height - 1)

    var top = ceiling
    if (!(top > 0)) {
      top = Math.max(1e-6, floor)
      for (var i = first; i < length; i++) {
        var sum = 0
        for (var k = 0; k < lists.length; k++) sum += Number(lists[k] && lists[k][i]) || 0
        if (sum > top) top = sum
      }
      top *= 1.06
    }

    for (var j = first; j < length; j++) {
      var x = snap(width - (length - j) * pitch + gap)
      var y = height - 1
      var total = 0
      for (var t = 0; t < lists.length; t++) total += Number(lists[t] && lists[t][j]) || 0
      var graded = grader ? grader(total) : null
      for (var c = 0; c < lists.length; c++) {
        var v = Number(lists[c] && lists[c][j]) || 0
        if (v <= 0) continue
        var h = Math.max(px, Math.min(room, v / top * room))
        var upper = snap(y - h)
        var lower = snap(y)
        if (lower - upper < px) upper = lower - px
        ctx.globalAlpha = graded && c > 0 ? secondFade : 1
        ctx.fillStyle = graded || colors[c] || Commons.Color.accent
        ctx.fillRect(x, upper, Math.max(px, snap(barWidth)), lower - upper)
        y -= h
      }
    }
    ctx.globalAlpha = 1
    ctx.fillStyle = baseColor
    ctx.fillRect(0, height - 1, width, 1)
  }
}

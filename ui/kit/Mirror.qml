import QtQuick
import qs.Commons
import qs.Commons as Commons

// Two histories on one axis: `up` rises from the middle line, `down` hangs
// from it, each scaled to its own peak in view. Upload over download, read
// over write. The peaks are published for the legend.
Canvas {
  id: mirror

  property var up: []
  property var down: []
  property int revision: 0
  property color upColor: Commons.Color.accent
  property color downColor: Commons.Color.accent
  property real floor: 10240       // bytes/s: below this the graph stays low
  property int barWidth: 2
  property int gap: 1
  property color midline: Util.alpha(Commons.Color.popups.text, 0.18)

  property real peakUp: 0
  property real peakDown: 0

  readonly property int pitch: Math.max(1, barWidth + gap)
  readonly property int slots: Math.max(1, Math.floor((width + gap) / pitch))

  antialiasing: false
  renderStrategy: Canvas.Cooperative

  function peak(list) {
    var top = 0
    var a = Array.isArray(list) ? list : []
    for (var i = Math.max(0, a.length - slots); i < a.length; i++) if (Number(a[i]) > top) top = Number(a[i])
    return top
  }

  // Peaks follow the data even while the graph is hidden.
  function measure() {
    peakUp = peak(up)
    peakDown = peak(down)
  }

  function refresh() {
    measure()
    if (visible && width > 0 && height > 0) requestPaint()
  }
  onRevisionChanged: refresh()
  onUpChanged: refresh()
  onDownChanged: refresh()
  onWidthChanged: refresh()
  onHeightChanged: refresh()
  onVisibleChanged: refresh()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    var ratio = Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    function snap(v) { return Math.round(v * ratio) / ratio }
    var px = 1 / ratio
    var middle = snap(height / 2)
    var half = Math.max(1, height / 2 - 1)

    function draw(list, scale, rising, color) {
      var a = Array.isArray(list) ? list : []
      ctx.fillStyle = color
      for (var i = Math.max(0, a.length - slots); i < a.length; i++) {
        var v = Number(a[i]) || 0
        if (v <= 0) continue
        var h = Math.max(px, Math.min(half, v / scale * half))
        var x = snap(width - (a.length - i) * pitch + gap)
        if (rising) ctx.fillRect(x, snap(middle - h), Math.max(px, barWidth), snap(h))
        else ctx.fillRect(x, middle + 1, Math.max(px, barWidth), snap(h))
      }
    }

    draw(up, Math.max(floor, peakUp) * 1.06, true, upColor)
    draw(down, Math.max(floor, peakDown) * 1.06, false, downColor)
    ctx.fillStyle = midline
    ctx.fillRect(0, middle, width, 1)
  }
}

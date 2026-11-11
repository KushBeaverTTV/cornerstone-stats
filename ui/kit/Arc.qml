import QtQuick
import qs.Commons
import qs.Commons as Commons

// A small ring filled clockwise from the top: a core's load, a drive's
// fullness, a fan's speed. `parts` stacks several fills ([{value, color}]).
Canvas {
  id: arc

  property real value: 0                 // 0..1
  property color color: Commons.Color.accent
  property var parts: []
  property color track: Util.alpha(Commons.Color.popups.text, 0.16)
  property real thickness: Style.spaceReal(2.6)
  property real size: Style.space(18)

  width: size
  height: size
  antialiasing: true
  renderStrategy: Canvas.Cooperative

  onValueChanged: requestPaint()
  onColorChanged: requestPaint()
  onPartsChanged: requestPaint()
  onTrackChanged: requestPaint()
  onWidthChanged: requestPaint()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    var r = (Math.min(width, height) - thickness) / 2
    var cx = width / 2, cy = height / 2
    ctx.lineWidth = thickness
    ctx.lineCap = "butt"
    ctx.strokeStyle = track
    ctx.beginPath()
    ctx.arc(cx, cy, r, 0, Math.PI * 2)
    ctx.stroke()
    var fills = Array.isArray(parts) && parts.length > 0 ? parts : [{ value: value, color: color }]
    var start = -Math.PI / 2
    for (var i = 0; i < fills.length; i++) {
      var v = Math.max(0, Math.min(1, Number(fills[i].value) || 0))
      if (v <= 0) continue
      var end = start + v * Math.PI * 2
      ctx.strokeStyle = fills[i].color
      ctx.beginPath()
      ctx.arc(cx, cy, r, start, Math.min(end, start + Math.PI * 2 - 0.0001))
      ctx.stroke()
      start = end
    }
  }
}

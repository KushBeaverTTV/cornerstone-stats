.pragma library

// Graph colours from the active Omarchy theme. The accent draws the main
// series; the second series takes the theme colour that stands furthest from
// it on the colour wheel and still reads on the panel background, so two
// series never blur together whatever the theme. Colours stay hex strings so
// this file runs (and is tested) without Qt.

// name = "#rrggbb" lines of a theme's colors.toml.
function parseToml(text) {
  var out = {}
  String(text || "").split("\n").forEach(function(line) {
    var m = line.match(/^\s*([A-Za-z0-9_-]+)\s*=\s*["']?(#[0-9A-Fa-f]{6})/)
    if (m) out[m[1]] = m[2].toLowerCase()
  })
  return out
}

// {h: 0-360 or -1 for grey, s, l} from "#rrggbb" (or Qt's "#aarrggbb").
function hsl(hex) {
  var s = String(hex || "").replace("#", "")
  if (s.length === 8) s = s.slice(2)
  if (!/^[0-9a-fA-F]{6}$/.test(s)) return { h: -1, s: 0, l: 0.5 }
  var r = parseInt(s.slice(0, 2), 16) / 255, g = parseInt(s.slice(2, 4), 16) / 255, b = parseInt(s.slice(4, 6), 16) / 255
  var max = Math.max(r, g, b), min = Math.min(r, g, b)
  var l = (max + min) / 2
  if (max - min < 1e-6) return { h: -1, s: 0, l: l }
  var d = max - min
  var sat = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  var h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return { h: h * 60, s: sat, l: l }
}

function hex(h, s, l) {
  function channel(n) {
    var k = (n + h / 30) % 12
    var a = s * Math.min(l, 1 - l)
    var v = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))
    var x = Math.round(v * 255).toString(16)
    return x.length < 2 ? "0" + x : x
  }
  return "#" + channel(0) + channel(8) + channel(4)
}

function hueGap(a, b) {
  if (a < 0 || b < 0) return 0
  var d = Math.abs(a - b) % 360
  return d > 180 ? 360 - d : d
}

// The accent turned around the wheel, for themes without a usable colour.
function turned(accent, degrees) {
  var c = hsl(accent)
  var h = c.h < 0 ? 210 : (c.h + degrees) % 360
  return hex(h, Math.max(0.5, c.s), Math.max(0.45, Math.min(0.7, c.l)))
}

var CANDIDATES = ["blue", "magenta", "cyan", "green", "yellow", "orange", "red"]
// Cool hues read as "the other series" best beside a warm accent and
// vice versa; prefer them when they are far enough away.
var PREFERRED = { blue: 1, magenta: 1, cyan: 1 }

// {series1, series2, series3, warn, danger, good}, all "#rrggbb".
// accent/background/urgent may be Qt colours; they are read by their string.
function pick(theme, accent, background, urgent) {
  var t = theme || {}
  var a = String(accent)
  var aHue = hsl(a).h
  var bgLight = hsl(String(background)).l
  var usable = []
  CANDIDATES.forEach(function(name) {
    if (!t[name]) return
    var c = hsl(t[name])
    if (c.s < 0.2 || Math.abs(c.l - bgLight) < 0.25) return
    var gap = hueGap(aHue, c.h)
    usable.push({ name: name, color: t[name], hue: c.h, gap: gap, score: gap + (PREFERRED[name] && gap >= 50 ? 60 : 0) })
  })
  usable.sort(function(x, y) { return y.score - x.score })
  var second = usable.length > 0 && usable[0].gap >= 30 ? usable[0] : null
  var series2 = second ? second.color : turned(a, 180)
  var s2Hue = hsl(series2).h
  var third = null
  for (var i = 0; i < usable.length; i++) {
    if (usable[i] === second) continue
    if (usable[i].gap >= 35 && hueGap(usable[i].hue, s2Hue) >= 35) { third = usable[i].color; break }
  }
  return {
    series1: a,
    series2: series2,
    series3: third || t.yellow || turned(a, 120),
    warn: t.yellow || t.orange || third || turned(a, 60),
    danger: t.red || String(urgent),
    good: t.green || a
  }
}

.pragma library

// Turning numbers into the short text the bar and the panel show.
// Every function takes raw collector units (bytes, bytes per second,
// percent, Celsius, MHz, watts, seconds) and never throws on null.

var DASH = "–"

function number(value, fallback) {
  var n = Number(value)
  return value !== null && value !== undefined && value !== "" && isFinite(n) ? n : (fallback === undefined ? 0 : fallback)
}

function known(value) {
  return value !== null && value !== undefined && value !== "" && isFinite(Number(value))
}

function clamp(value, low, high) {
  var n = Number(value)
  if (!isFinite(n)) return low
  return n < low ? low : (n > high ? high : n)
}

// ------------------------------------------------------------- sizes

var SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"]

// {value, unit} in powers of 1024: "640 KB", "1.4 GB", "312 GB".
// One decimal below 100 from KB/MB upward keeps four characters at most.
function size(bytes) {
  var v = Math.max(0, number(bytes))
  var step = 0
  while (v >= 1024 && step < SIZE_UNITS.length - 1) { v /= 1024; step++ }
  var text = step >= 2 && v < 100 ? v.toFixed(1) : String(Math.round(v))
  if (text === "1024" && step < SIZE_UNITS.length - 1) { text = "1.0"; step++ }
  return { value: text, unit: SIZE_UNITS[step] }
}

function sizeText(bytes) {
  var s = size(bytes)
  return s.value + " " + s.unit
}

// "6.4 / 20.0 GB": one unit when both sides share it.
function sizePair(used, total) {
  var a = size(used), b = size(total)
  return a.unit === b.unit ? a.value + " / " + b.value + " " + b.unit : a.value + " " + a.unit + " / " + b.value + " " + b.unit
}

function rate(bytesPerSecond) {
  var s = size(bytesPerSecond)
  return { value: s.value, unit: s.unit + "/s" }
}

function rateText(bytesPerSecond) {
  var r = rate(bytesPerSecond)
  return r.value + " " + r.unit
}

// For a bar cell: "0", "48K", "1.2M". Below half a kilobyte is noise.
function rateTiny(bytesPerSecond) {
  var v = number(bytesPerSecond)
  if (v < 512) return "0"
  var s = size(v)
  return s.value + s.unit.charAt(0)
}

// A rate, or a dash when nothing moves: the process lists read better that way.
function rateOrDash(bytesPerSecond) {
  return number(bytesPerSecond) >= 1 ? rateText(bytesPerSecond) : "–"
}

// ---------------------------------------------------------- percents

function percent(value) {
  return Math.round(clamp(value, 0, 100)) + "%"
}

// Process figures: one decimal under ten so small loads are not all "0%".
function share(value) {
  var n = Math.max(0, number(value))
  return (n >= 10 ? String(Math.round(n)) : n.toFixed(1)) + "%"
}

// -------------------------------------------------------- temperature

function toUnit(celsius, unit) {
  var c = Number(celsius)
  return unit === "Fahrenheit" ? c * 9 / 5 + 32 : c
}

// "63°", or a dash.
function degrees(celsius, unit) {
  if (!known(celsius)) return DASH
  return Math.round(toUnit(celsius, unit)) + "°"
}

// "63 °C" for tooltips, where the scale is not obvious.
function degreesLong(celsius, unit) {
  if (!known(celsius)) return DASH
  return Math.round(toUnit(celsius, unit)) + (unit === "Fahrenheit" ? " °F" : " °C")
}

// ------------------------------------------------------------- clocks

function clock(mhz) {
  var m = number(mhz)
  if (m <= 0) return null
  return m >= 1000 ? { value: (m / 1000).toFixed(2), unit: "GHz" } : { value: String(Math.round(m)), unit: "MHz" }
}

function clockText(mhz) {
  var c = clock(mhz)
  return c ? c.value + " " + c.unit : ""
}

// For a bar cell when a card has no load figure: "2.5G", "96M".
function clockTiny(mhz) {
  var m = number(mhz)
  if (m <= 0) return DASH
  return m >= 1000 ? (m / 1000).toFixed(1) + "G" : Math.round(m) + "M"
}

// -------------------------------------------------------------- power

function watts(value) {
  if (!known(value) || Number(value) < 0) return null
  var w = Number(value)
  return { value: w < 10 ? w.toFixed(1) : String(Math.round(w)), unit: "W" }
}

function wattsText(value) {
  var w = watts(value)
  return w ? w.value + " W" : ""
}

// --------------------------------------------------------------- time

// "3d 4h", "2h 05m", "12m", "<1m".
function duration(seconds) {
  var s = Math.max(0, Math.floor(number(seconds)))
  var d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60)
  if (d > 0) return d + "d " + h + "h"
  if (h > 0) return h + "h " + (m < 10 ? "0" : "") + m + "m"
  if (m > 0) return m + "m"
  return "<1m"
}

// Battery estimates come in minutes: "1:35".
function hoursMinutes(minutes) {
  var m = Math.max(0, Math.round(number(minutes)))
  var r = m % 60
  return Math.floor(m / 60) + ":" + (r < 10 ? "0" : "") + r
}

// "30 s", "4 min", "1 h" for the history span.
function span(seconds) {
  var s = Math.max(0, Math.round(number(seconds)))
  if (s < 60) return s + " s"
  if (s % 3600 === 0) return (s / 3600) + " h"
  var minutes = s / 60
  return (minutes === Math.round(minutes) ? String(minutes) : minutes.toFixed(1)) + " min"
}

function interval(seconds) {
  var s = number(seconds, 1)
  return (s < 1 ? s.toFixed(1) : String(Math.round(s))) + " s"
}

function loadAverage(load) {
  if (!Array.isArray(load) || load.length < 3) return DASH
  return load.map(function(v) { return number(v).toFixed(2) }).join("  ")
}

function latency(ms) {
  if (!known(ms)) return DASH
  var v = Number(ms)
  if (v < 1) return (Math.round(v * 100) / 100) + " ms"
  if (v < 10) return (Math.round(v * 10) / 10) + " ms"
  return Math.round(v) + " ms"
}

// ------------------------------------------------------------- words

function plural(count, one, many) {
  return count + " " + (count === 1 ? one : (many || one + "s"))
}

function capitalize(text) {
  var s = String(text || "")
  return s.charAt(0).toUpperCase() + s.slice(1)
}

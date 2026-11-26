//! The coincident arc guard (issue #3442): one rule, in the prelude every
//! post script sees, for an arc record whose end point, as the script
//! writes it, is its start point.
//!
//! Provenance: original. RS-274 / ISO 6983 circular interpolation: an
//! arc block whose end point equals the current position, with a centre
//! off that point, is a full circle (the reading of the Fanuc-family,
//! LinuxCNC and grbl controls' published programming manuals, behaviour
//! only). The output resolution is the script's own word format, so the
//! comparison is made on the words as written, never on the stream's
//! millimetres.
//!
//! The rule (decision `docs/decisions/3442-coincident-arc-blocks.md`):
//! - the written in-plane ends differ: the arc posts as before;
//! - they coincide and the record turns more than half a circle (or its
//!   ends are equal in the stream: a full turn stated as one record),
//!   or it states no centre (the script refuses it by name):
//!   the full circle is the arc within the output resolution, and the
//!   arc posts as before;
//! - they coincide on a short arc: the tool moves less than the output
//!   resolution in the plane, so the record posts as a straight move
//!   when the plane's normal axis (a helix) or a rotary still moves, and
//!   as no block at all when nothing changes as written;
//! - the written ends differ, but on one ray from the written centre
//!   (issue #3474, `docs/decisions/3474-same-angle-arc.md`): a short
//!   record posts as a straight move, because a control that reads the
//!   sweep from the end angles sees none or a full turn. The written
//!   centre is the one the control computes: the written start plus the
//!   written I J K increments (the script passes `inc`), or the written
//!   absolute centre (a script that writes one passes no `inc`).

/// Appended to [`super::js::PRELUDE`]: plain JS, no host calls.
pub const ARC_GUARD_JS: &str = r#"
// #3442 (arc_guard.rs): what an arc record may become in the output.
// `word(axis, value)` is the script's own text for a position on "X", "Y"
// or "Z" (origin, unit and precision applied; "" for an axis it never
// writes). Returns "arc", "linear" (post the record as a straight move)
// or "none" (write no motion block).
function _arcNum(v) { return typeof v === "number" && isFinite(v); }
function arcShapeOf(p) {
  var c = p.IJKIncrement || {};
  return {
    plane: p.CircularPlane || "XY",
    ccw: p.CircularDirection !== "CW",
    start: { X: p.StartX, Y: p.StartY, Z: p.StartZ },
    end: { X: p.X, Y: p.Y, Z: p.Z },
    center: { X: c.I, Y: c.J, Z: c.K },
    rotary: _arcNum(p.A) || _arcNum(p.B) || _arcNum(p.C)
  };
}
// The record's own sweep (radians, 0..2pi) in the frame (u, v).
function _arcSweep(a, u, v) {
  var s = a.start, e = a.end, c = a.center;
  var a0 = Math.atan2(s[v] - c[v], s[u] - c[u]), a1 = Math.atan2(e[v] - c[v], e[u] - c[u]);
  var sweep = a.ccw ? a1 - a0 : a0 - a1;
  if (sweep < 0) sweep += 2 * Math.PI;
  return sweep;
}
// The number of a written word ("X10.011", "X+10.011", "10.") or NaN.
function _arcWordNum(t) {
  var m = /[-+]?(\d+\.?\d*|\.\d+)/.exec(String(t));
  return m ? parseFloat(m[0]) : NaN;
}
// #3474: the written ends differ, but the written end is on the written
// start's ray from the written centre (the same angle), or one of them is
// on the written centre. A control that reads the sweep from the angles
// sees none, or a full turn. On a short record (half a circle or less)
// the tool moves under the output step across the radius only: a straight
// move. The test uses the cross product's zero and the dot product's sign
// only, so a mirrored or scaled word (a lathe diameter) reads the same.
// The written centre is the one the control computes. A script that
// writes the centre as increments from the start (I J K) passes `inc(axis,
// delta)`: the written increment word's number, in the scale of that
// axis's position word (a lathe doubles a radius I under diameter X).
// The centre is then the written start plus that number, never the
// rounded absolute centre: the two can differ by one output step. A
// script that writes an absolute centre passes no `inc`.
function _arcWrittenCentre(a, k, word, inc) {
  var c = a.center[k];
  if (typeof inc !== "function") return _arcWordNum(word(k, c));
  return _arcWordNum(word(k, a.start[k])) + Number(inc(k, c - a.start[k]));
}
// `fmt(delta)` is the script's own increment text: the inc for a script
// whose increment words use the position words' scale.
function arcIncOf(fmt) { return function (k, d) { return _arcWordNum(fmt(d)); }; }
function sameAngleShort(a, u, v, word, inc) {
  var s = a.start, e = a.end, c = a.center;
  if (!_arcNum(c[u]) || !_arcNum(c[v])) return false;
  var cu = _arcWrittenCentre(a, u, word, inc), cv = _arcWrittenCentre(a, v, word, inc);
  var su = _arcWordNum(word(u, s[u])) - cu, sv = _arcWordNum(word(v, s[v])) - cv;
  var eu = _arcWordNum(word(u, e[u])) - cu, ev = _arcWordNum(word(v, e[v])) - cv;
  if (!isFinite(su) || !isFinite(sv) || !isFinite(eu) || !isFinite(ev)) return false;
  var scale = Math.sqrt((su * su + sv * sv) * (eu * eu + ev * ev));
  var onRay = scale === 0 || (Math.abs(su * ev - sv * eu) <= 1e-9 * scale && su * eu + sv * ev > 0);
  // A record whose own ends are on one ray (a zero sweep in the stream)
  // states a full turn, as emitArcWithRadius reads it: the arc stays.
  var sweep = _arcSweep(a, u, v);
  return onRay && sweep > 1e-9 && sweep <= Math.PI;
}
function arcFateOf(a, word, inc) {
  // The right-handed in-plane frame (u, v), u x v = +normal: CCW (seen
  // from the positive normal) is the ordinary atan2 sense in each plane.
  var ax = a.plane === "XZ" ? ["Z", "X", "Y"] : (a.plane === "YZ" ? ["Y", "Z", "X"] : ["X", "Y", "Z"]);
  var u = ax[0], v = ax[1], h = ax[2], s = a.start, e = a.end, c = a.center;
  if (!_arcNum(s[u]) || !_arcNum(s[v]) || !_arcNum(e[u]) || !_arcNum(e[v])) return "arc";
  function same(k, p, q) { return String(word(k, p)) === String(word(k, q)); }
  if (!same(u, s[u], e[u]) || !same(v, s[v], e[v])) return sameAngleShort(a, u, v, word, inc) ? "linear" : "arc";
  // The written ends coincide. With no centre the record is not an arc
  // this rule can read: the script's own code refuses it by name.
  if (!_arcNum(c[u]) || !_arcNum(c[v])) return "arc";
  // A centre that stands off the start as written makes the block a
  // full circle: right for a full turn. A full turn is a record whose
  // ends are equal in the stream (the one-record form the reader takes,
  // and the form the writer gives a near-full turn on its lattice), or
  // whose sweep from its direction and centre is more than half a turn.
  // Never ends that are only close: a true short arc under any band is
  // still short.
  var round = !(same(u, c[u], s[u]) && same(v, c[v], s[v]));
  if (round) {
    var full = e[u] === s[u] && e[v] === s[v];
    if (!full) full = _arcSweep(a, u, v) > Math.PI;
    if (full) return "arc";
  }
  // A short arc under the output resolution: no arc block.
  if (_arcNum(e[h]) && !(_arcNum(s[h]) && same(h, s[h], e[h]))) return "linear";
  if (a.rotary) return "linear";
  return "none";
}
function arcFate(word, inc) { return arcFateOf(arcShapeOf(properties), word, inc); }
"#;

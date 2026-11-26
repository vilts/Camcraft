// Generic ISO abrasive-waterjet post (#1882). Provenance: the ISO 6983 /
// RS-274 word set only (G0, G1, G2, G3, G4, G17, G20, G21, G40, G54-G59,
// G90, G94, M30) and general abrasive-waterjet practice (the jet opens
// over the pierce point, the valve needs a lag before the stream is full,
// the abrasive starts on the open jet, the pierce dwells before the cut
// moves, the abrasive stops before the water). No control vendor's post,
// manual text or CAM output was read. Behaviour only, per this repo's
// Clean-Room Protocol. The hook contract is our engine's
// (assets/posts/AGENTS.md, docs/CNC-INTERMEDIATE.md "Waterjet records");
// the structure follows our own tormach-plasma.cps. The decision record is
// docs/decisions/1882-waterjet-post.md.
//
// No standard fixes the M numbers that open the water valve and the
// abrasive feed: they are declared options. The defaults (M10/M11 water,
// M12/M13 abrasive) are placeholders - set them to your control's numbers
// before a program runs on a machine.
description = "Generic ISO abrasive waterjet (G20/G21, G90, G0/G1, G2/G3 or chords, water and abrasive M-codes, G4 pierce dwell)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
serves = ["waterjet"];     // #1266: this script serves a waterjet table only
coolantModes = true;       // reads PARAM Coolant - to refuse every ask but Off
workOffsetIndex = true;    // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 6;         // G54..G59
programUnits = "mm inch";  // #902: writes the document's unit (postOptions.Units), G21 / G20
lineEnding = "lf";         // #1882: onOpen sets "crlf" from the lineEnding option
linearizeArcs = false;     // #1882: onOpen sets true from the arcs option
chordTolerance = 0.01;     // mm, the sagitta of the chords when arcs are linear
declaredOptions = {
  waterOn: { type: "integer", title: "Water on M-code", group: "Jet", description: "M number that opens the water valve", min: 0, max: 999, default: 10 },
  waterOff: { type: "integer", title: "Water off M-code", group: "Jet", description: "M number that closes the water valve", min: 0, max: 999, default: 11 },
  abrasiveOn: { type: "integer", title: "Abrasive on M-code", group: "Jet", description: "M number that starts the abrasive feed", min: 0, max: 999, default: 12 },
  abrasiveOff: { type: "integer", title: "Abrasive off M-code", group: "Jet", description: "M number that stops the abrasive feed", min: 0, max: 999, default: 13 },
  dwellWord: {
    type: "enum",
    title: "Dwell word",
    group: "Format",
    description: "How G4 states the dwell time",
    choices: [
      { id: "p-seconds", title: "G4 P seconds" },
      { id: "x-seconds", title: "G4 X seconds" },
      { id: "p-milliseconds", title: "G4 P milliseconds" },
    ],
    default: "p-seconds",
  },
  arcs: {
    type: "enum",
    title: "Arcs",
    group: "Format",
    description: "Write arcs as G2/G3, or as G1 chords for a control without circular interpolation",
    choices: [
      { id: "circular", title: "G2/G3" },
      { id: "linear", title: "G1 chords" },
    ],
    default: "circular",
  },
  lineEnding: {
    type: "enum",
    title: "Line ending",
    group: "Format",
    description: "The end of each program line",
    choices: [
      { id: "lf", title: "LF" },
      { id: "crlf", title: "CR LF" },
    ],
    default: "lf",
  },
};

// Determinism law: no timestamps, no randomness - the same stream always
// posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var inch = false;          // the document's unit is inch (postOptions.Units)
var wcsWord = "G54";       // from the WorkOffset PARAM
var orgX = 0, orgY = 0;    // the WCS slot origin (OriginX/Y PARAMs)
var sectionOpen = false;   // the section's preamble is written
var lit = false;           // the jet is on (onJetOn .. onJetOff)
var abrasiveLit = false;   // the abrasive feeds (inside a jet span)
var lastXY = "";           // the X Y words of the last G0, to skip a Z-only rapid
var lastF = "";            // the F word in force
var codes = null;          // the M numbers, read once in onOpen
var opName = "", opKind = "", setupName = "", setupSaid = "", toolName = "";
var refused = {};          // one named refusal per key, not one per line

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fu(v) { return trim(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3)); }
// A feed keeps three decimals in mm/min (a whole feed stays whole) and two
// in in/min; a positive feed that rounds to zero is refused in feedWord.
function ff(v) { return trim(Number(inch ? v / 25.4 : v).toFixed(inch ? 2 : 3)); }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function arcWord(k, v) { return k === "Z" ? "" : wo(k, v, k === "X" ? orgX : orgY); }  // #3442: a position word as written (no Z), for arcFate
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + s + ")"); }
function refuse(key, s) {
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
// Comment text: ( ) delimit a comment, % and ; can end a program on
// some controls - all four become spaces; printable ASCII, 40 chars.
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function note(s) { block(["(" + s + ")"]); }
// #2996 header notes and WARN records: one comment, delimiters removed.
function onComment(s) { note(commentText(s)); }
function opt(key, dflt) {
  var o = typeof postOptions !== "undefined" && postOptions ? postOptions[key] : undefined;
  return o === undefined || o === null ? dflt : o;
}
function mCode(key, dflt) {
  var n = Number(opt(key, dflt));
  if (!(n >= 0 && n <= 999 && Math.floor(n) === n)) {
    refuse("code" + key, "the " + key + " option is " + opt(key, dflt) + " - an M number is a whole number 0-999");
    return "";
  }
  return "M" + n;
}
function dwell(seconds) {
  if (!has(seconds) || seconds <= 0) return;
  var form = opt("dwellWord", "p-seconds");
  if (form === "x-seconds") block(["G4", "X" + trim(Number(seconds).toFixed(3))]);
  else if (form === "p-milliseconds") block(["G4", "P" + Math.round(seconds * 1000)]);
  else block(["G4", "P" + trim(Number(seconds).toFixed(3))]);
}

// ------------------------------------------------------------- machine
// The M numbers here open a water valve and an abrasive feed: a machine
// that does not declare waterjet is refused by name. A waterjet table
// cuts flat sheet in X and Y - a stated rotary has no word here.
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  var nm = (m && (m.name || m.model)) || "";
  if (!m || m.kind !== "waterjet") {
    var why = nm
      ? nm + " is not declared a waterjet machine"
      : "no machine is stated, so none is declared a waterjet machine";
    refuse("kind", "the generic waterjet post writes jet words (water and abrasive M-codes) only a waterjet table reads - " + why);
    return false;
  }
  if (m.rotaries && m.rotaries.length > 0) {
    refuse("rotary", (nm || "the machine") + " states a rotary axis - a waterjet table cuts flat sheet in X and Y, and this post writes no rotary word");
    return false;
  }
  return true;
}

// The cut must stay inside the stroke the machine file states (#1882:
// programBounds against machine.travel). An unstated stroke is not judged.
function travelOk() {
  var b = typeof programBounds !== "undefined" ? programBounds : null;
  var t = typeof machine !== "undefined" && machine ? machine.travel : null;
  if (!b || !t) return;
  var axes = [["X", b.minX, b.maxX], ["Y", b.minY, b.maxY]];
  for (var i = 0; i < axes.length; i++) {
    var r = t[axes[i][0]];
    if (!r || r.length !== 2) continue;
    if (axes[i][1] < r[0] || axes[i][2] > r[1]) {
      refuse("travel" + axes[i][0], "the cut runs " + axes[i][0] + " " + fu(axes[i][1]) + " to " + fu(axes[i][2]) +
        " - outside the machine's stated " + axes[i][0] + " travel " + fu(r[0]) + " to " + fu(r[1]));
    }
  }
}

// -------------------------------------------------------------- header
// The preamble states the modal state once, never assumed: the unit,
// absolute distance, feed per minute, the XY plane, no cutter
// compensation (the kerf is in the moves).
function onOpen() {
  inch = opt("Units", "mm") === "inch";
  if (opt("lineEnding", "lf") === "crlf") lineEnding = "crlf";
  if (opt("arcs", "circular") === "linear") linearizeArcs = true;
  codes = {
    waterOn: mCode("waterOn", 10), waterOff: mCode("waterOff", 11),
    abrasiveOn: mCode("abrasiveOn", 12), abrasiveOff: mCode("abrasiveOff", 13),
  };
  var nm = (typeof machine !== "undefined" && (machine.name || machine.model)) || "";
  var title = typeof program !== "undefined" && program.comment ? program.comment : nm;
  note(clean(title || "CAMCRAFT WATERJET"));
  var b = typeof programBounds !== "undefined" ? programBounds : null;
  if (b) note("EXTENT X" + fu(b.minX) + " TO " + fu(b.maxX) + " Y" + fu(b.minY) + " TO " + fu(b.maxY));
  travelOk();
  block([inch ? "G20" : "G21"]);
  block(["G90", "G94", "G17", "G40"]);
}

function workOffsetName(n) {  // offset n -> this control's word; "" when it has none
  return n >= 1 && n <= 6 && n === Math.floor(n) ? "G" + (53 + n) : "";
}

// No spindle turns on a waterjet: an rpm (S, and with it G95 feed per
// revolution) or a surface speed (G96) is refused by name.
function onParameter(name, value) {
  if (name === "WorkOffset" && value) {
    var w = workOffsetName(Number(value));
    if (w) { if (w !== wcsWord) lastXY = ""; wcsWord = w; }
    else msg("work offset " + value + " - this post expresses work offsets 1-" + workOffsetMax + " only");
  } else if (name === "OriginX" && has(value)) {
    if (orgX !== Number(value)) lastXY = "";
    orgX = Number(value);
  } else if (name === "OriginY" && has(value)) {
    if (orgY !== Number(value)) lastXY = "";
    orgY = Number(value);
  } else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - a waterjet turns no spindle (no S word, no G95 feed per revolution)");
  } else if (name === "SurfaceSpeed" && has(value) && Number(value) > 0) {
    refuse("css" + opName, who() + "states a constant surface speed - a waterjet turns no spindle (no G96)");
  } else if (name === "Coolant" && String(value) !== "off") {
    refuse("coolant" + opName, who() + "asks " + value + " coolant - a waterjet table has no coolant");
  }
}

function onToolChange(number, name) { toolName = clean(name); }

// The section's preamble: refused unless the machine is a waterjet and
// the section carries a WATERJET header (a mill, lathe or plasma path is
// not a waterjet cut).
function headerOk() {
  if (properties.WaterjetAbrasive !== undefined) return true;
  refuse("op" + opName, who() + (opKind ? "a " + opKind + " operation" : "a path with no WATERJET header") +
    " is not waterjet cutting - this post writes only waterjet paths");
  return false;
}

function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  if (!machineOk()) return;
  if (!headerOk()) return;
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + opName + (opKind ? " - " + opKind.toUpperCase() : ""));
  note((toolName || "NOZZLE") + " - " + (properties.WaterjetAbrasive ? "ABRASIVE" : "PURE WATER"));
  block([wcsWord]);
}

// A jet word or a cut with no header in force is refused by name, never
// dropped without a warning.
function jetOk() {
  return sectionOpen && !refused.kind && !refused.rotary && headerOk();
}

function onSection() { startPreamble(); }
function onSectionEnd() {
  sectionOpen = false;
  opName = ""; opKind = "";
}

// ------------------------------------------------------------- the jet
// Per contour: the water valve opens over the pierce point, the valve
// lag dwell lets the stream build, the abrasive starts, the pierce
// dwells; then the cut moves. At the end the abrasive stops first, then
// the water, so no abrasive stands in a closed nozzle.
function onJetOn() {
  startPreamble();
  if (!jetOk()) return;
  var p = properties;
  note("C" + p.Contour + " " + String(p.ContourRole || "").toUpperCase());
  block([codes.waterOn]);
  dwell(p.ValveLagDwell);
  if (p.WaterjetAbrasive) { block([codes.abrasiveOn]); abrasiveLit = true; }
  dwell(p.PierceDwell);
  lit = true;
}

function onJetOff() {
  if (!jetOk()) { lit = false; abrasiveLit = false; return; }
  if (abrasiveLit) block([codes.abrasiveOff]);
  block([codes.waterOff]);
  lit = false;
  abrasiveLit = false;
}

// ------------------------------------------------------------- motions
// A waterjet table states X and Y only: a rapid writes X Y (a Z-only
// rapid writes nothing), and a cutting move that changes Z is refused.
function onRapid() {
  var p = properties;
  startPreamble();
  if (!jetOk()) return;
  if (has(p.A)) { refuse("tilt" + opName, who() + "a tilted move - a waterjet table states no rotary axis"); return; }
  var xy = [wo("X", p.X, orgX), wo("Y", p.Y, orgY)].join(" ");
  if (xy === lastXY) return;
  block(["G0", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
  lastXY = xy;
}

function feedWord(p) {
  if (!has(p.FeedRate) || p.FeedRate <= 0) {
    refuse("nofeed" + opName, who() + "a cutting move states no feed - refused");
    return "";
  }
  var f = "F" + ff(p.FeedRate);
  if (f === "F0") {
    refuse("tinyfeed" + opName, who() + "a cutting move states a feed of " + p.FeedRate + " mm/min - below the program's feed resolution (F0) - refused");
    return "";
  }
  if (f === lastF) return "";
  lastF = f;
  return f;
}

function cutOk(p) {
  startPreamble();
  if (!jetOk()) return false;
  if (has(p.A)) { refuse("tilt" + opName, who() + "a tilted move - a waterjet table states no rotary axis"); return false; }
  if (!lit) {
    refuse("dry" + opName, who() + "a cutting move with the jet off - every waterjet feed rides a JET ON span");
    return false;
  }
  var dz = has(p.OriginalZChange) ? p.OriginalZChange : 0;
  if (Math.abs(dz) > 0.0005 || has(p.StartZ) && has(p.Z) && Math.abs(p.Z - p.StartZ) > 0.0005) {
    refuse("z" + opName, who() + "a cutting move changes Z - a waterjet program cuts in X and Y only");
    return false;
  }
  return true;
}

function onLinear() {
  var p = properties;
  if (!cutOk(p)) return;
  block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), feedWord(p)]);
  lastXY = "";
}

function onCircular() {
  var p = properties;
  if (!cutOk(p)) return;
  if (p.CircularPlane && p.CircularPlane !== "XY") {
    refuse("plane" + opName, who() + "an arc in the " + p.CircularPlane + " plane - a waterjet cut is flat");
    return;
  }
  // #3442: an arc whose written ends coincide is no arc block (arcFate,
  // PRELUDE) - a control reads it as a full circle, cut with the jet on.
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  var c = p.IJKIncrement || {};
  block([
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY),
    has(c.I) && has(p.StartX) ? "I" + fu(c.I - p.StartX) : "",
    has(c.J) && has(p.StartY) ? "J" + fu(c.J - p.StartY) : "",
    feedWord(p),
  ]);
  lastXY = "";
}

function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - a waterjet has no drilling cycle (G81-G89 are mill words)");
}

// ------------------------------------------------------------ footer
function onClose() {
  block(["M30"]);
}

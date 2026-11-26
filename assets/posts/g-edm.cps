// G-EDM wire EDM post, 2-axis contour, for G-RBLv3 controllers (#2700,
// part of #2697). Provenance: the public Grbl_Esp32 G-code documentation
// (the words its parser reads: G20 / G21, G90, G17, G0, G1, G2 / G3 with
// incremental I J, `(...)` comments, M30; no G41 / G42, and a bare `%`
// line is a parser error on a raw Grbl stream), and the public G-EDM
// pages (gedm.org, github.com/G-EDM/G-RBL): wire mode runs 2D G0 / G1 /
// G2 / G3 contours, the spark power is set at the controller and not by
// a G-code word, and the gap servo sets the feed. No G-RBLv3 source file
// was read (its licence is private use only), and no other CAM product's
// post or output. NOT RUN ON A G-EDM CONTROLLER: check the output before
// cutting. What is and is not confirmed is in
// docs/decisions/2700-g-edm-post.md.
//
// Written: G21 / G20 G90 G17, G0 to the threading point, G1, G2 / G3 with
// I J, M30, and comments: the header's reminders and each section's
// condition (the operator sets its power at the controller).
// Never written (each a named refusal or the operator's task): a `%`
// fence, an O word, a work offset word (G54 is the controller's default
// and the only one accepted), G40 / G41 / G42, F, S, Z, U V, M0 / M1
// stops, threading or wire-cut words, a return move.
description = "G-EDM wire EDM, 2-axis (Grbl-family words for G-RBLv3; spark power set at the controller; not verified on a control)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
wireEdm = true;            // voices PARAM EdmCondition (the engine's door)
afterModes = true;         // reads PARAM AfterOperation (every stop and home refused)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20

// Determinism law: no timestamps, no randomness.

// ---------------------------------------------------------------- state
var inch = false;
var wcsWord = "";
var orgX = 0, orgY = 0;
var opName = "", opKind = "", setupName = "";
var condition = "", side = "", afterOp = "";
var inSection = false;     // between onSection and onSectionEnd
var sectionOk = false;     // the section is one this post can write
var threaded = false;      // the wire is through the work (the first cut began)
var cutZ = null;           // the section's one programmed plane (2-axis)
var taper = 0;             // the section's stated taper, degrees (0: vertical)
var fourAxis = "";         // the section's 4-axis pairing rule ("": one profile)
var angleWord = false;     // a WireAngle param, a stated zero included
var leaning = false;       // the next move states an upper guide offset
var controlOffset = false; // a COMP record refuses this section
var spindleAsk = false;    // the section states a turning spindle
var wireSections = 0;      // wire sections begun in this program
var machineGood = false;
var refused = {};

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
// A coordinate keeps its decimal point, as the other wire posts write it.
function fu(v) {
  var s = Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3).replace(/0+$/, "");
  return (s === "-0." || s === "-.") ? "0." : s;
}
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else note(s); }
function refuse(key, s) {
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
// Grbl ends a comment at the first `)` and has no nesting; `;` starts a
// line comment and `%` is a parser error. A name is cut to 48 characters.
function plain(s) {
  return String(s === null || s === undefined ? "" : s)
    .replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
}
function clean(s) {
  var t = plain(s);
  return t.length > 48 ? t.substring(0, 48).trim() : t;
}
function note(s) { block(["(" + plain(s) + ")"]); }
// Program notes use this dialect without truncating the stated bound.
function onComment(s) { note(commentText(s)); }

// ------------------------------------------------------------- machine
function machineName() {
  var m = typeof machine !== "undefined" ? machine : null;
  return (m && (m.model || m.name)) || "no machine";
}
// The machine check, once per program: a wire EDM with no rotary and no
// automatic threading (a G-EDM build has no G-code word for either).
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  if (!m || m.kind !== "wire_edm") {
    refuse("kind", "the G-EDM wire post writes wire words only a wire EDM reads - machine " + machineName() + " is not declared kind wire_edm");
    return false;
  }
  var ok = true;
  if (m.edm && m.edm.awt === true) {
    refuse("awt", "machine " + machineName() + " states edm.awt true - the G-EDM wire post has no G-code word to thread or cut the wire; set edm.awt to false");
    ok = false;
  }
  if (m.rotaries && m.rotaries.length > 0) {
    refuse("rotary", "machine " + machineName() + " states a rotary axis - the G-EDM wire post writes a 2-axis wire cut only");
    ok = false;
  }
  return ok;
}

// -------------------------------------------------------------- header
// No `%` fence and no O word: a raw Grbl stream rejects both.
function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions && postOptions.Units === "inch"; // #902: the document's unit, never the machine file's
  machineGood = machineOk();
  note(clean(machineName()));
  note("G-EDM WIRE 2-AXIS");
  note("NOT VERIFIED ON A CONTROL - CHECK BEFORE CUTTING");
  note("SPARK POWER IS SET AT THE G-EDM CONTROLLER");
  note("FEED IS SET BY THE GAP SERVO - NO F WORD");
  note("WIRE-CENTRE PATH - NO G41 G42 IS USED");
  note("WORK ZERO IS THE ACTIVE G54 - SET IT AT THE CONTROLLER");
  note("THE WIRE STAYS THREADED AT THE END - NO RETURN MOVE");
  block([inch ? "G20" : "G21", "G90", "G17"]);
}

function onParameter(name, value) {
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "EdmCondition") {
    condition = String(value === null || value === undefined ? "" : value);
    // With the wire through the work, a condition is the next skim's
    // (#538). G-RBLv3 has no confirmed pause to change the power.
    if (inSection && threaded) {
      refuse("skim" + opName, who() + "has a skim pass - the G-EDM wire post writes one pass only; spark power changes at the controller and no pause word is confirmed for G-RBLv3");
      sectionOk = false;
    }
  }
  else if (name === "WireSide") side = clean(value);
  else if (name === "AfterOperation") afterOp = String(value);
  else if (name === "WireTaper") taper = has(value) ? Number(value) : NaN;
  else if (name === "WireFourAxis") fourAxis = clean(value);
  else if (name === "WireU" || name === "WireV") leaning = true;
  else if (name === "WireAngle") {
    angleWord = true;
    refuse("angle" + opName, who() + "an angle word - the G-EDM wire post writes a vertical 2-axis cut only");
    sectionOk = false;
  }
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - a wire EDM turns no spindle");
    spindleAsk = true;
    sectionOk = false;
  }
}

function onToolChange(number, name) {
  refuse("tool" + opName, who() + "changes to tool " + number + " - a wire EDM mounts no tool");
}

// Grbl_Esp32 does not read G41 / G42. The path is the wire-centre path:
// every COMP record (OFF included) refuses the section.
function onRadiusCompensation() {
  controlOffset = true;
  sectionOk = false;
  refuse("offset" + opName, who() + "control-side wire offset (G41 / G42) - G-RBLv3 does not read it; post a generated wire-centre path without control or wear compensation");
}

// ------------------------------------------------------------- section
function onSection() {
  inSection = true;
  sectionOk = false;
  threaded = false;
  cutZ = null;
  if (condition === "") {
    refuse("op" + opName, who() + "is not wire EDM work (no EDM condition) - the G-EDM wire post posts Wire Contour operations only");
    return;
  }
  wireSections++;
  var ok = machineGood && !controlOffset && !angleWord && !spindleAsk;
  // No confirmed stop to cut the wire and thread it again: one section.
  if (wireSections > 1) {
    refuse("second" + opName, who() + "the G-EDM wire post writes one wire section per program - no pause to cut and thread the wire is confirmed for G-RBLv3; post each operation as its own program");
    ok = false;
  }
  if (fourAxis !== "") {
    refuse("four" + opName, who() + "is a 4-axis cut - the G-EDM wire post writes a vertical 2-axis cut only (no U V words)");
    ok = false;
  } else if (taper !== 0) {
    refuse("taper" + opName, who() + "is tapered - the G-EDM wire post writes a vertical 2-axis cut only (no U V words)");
    ok = false;
  }
  if (wcsWord !== "" && wcsWord !== "G54") {
    refuse("wcs" + opName, who() + "work offset " + JSON.stringify(wcsWord) + " is not G54 - the G-EDM wire post writes no work offset word; use offset 1 (G54) and set the work zero at the controller");
    ok = false;
  }
  if (afterOp === "home") {
    refuse("home" + opName, who() + "asks a trip home - the G-EDM wire post states no home position and moves no threaded wire; set the operation to Continue");
    ok = false;
  } else if (afterOp === "stop" || afterOp === "optional-stop") {
    refuse("stop" + opName, who() + "asks a program stop - M0 / M1 are not confirmed for G-RBLv3; set the operation to Continue");
    ok = false;
  }
  if (!ok) return;
  sectionOk = true;
  note(opName + (opKind ? " - " + opKind : ""));
  if (setupName) note("SETUP " + setupName);
  if (side) note("SIDE " + side.toUpperCase() + " - KERF IN THE PATH");
  note("CONDITION " + clean(condition) + " - SET ITS POWER AT THE CONTROLLER");
}

// The first motion of a section: the G0 to the threading point. The
// program has no confirmed pause, so the operator threads the wire there
// before the start; the G0 is then a move of zero length.
function startCut(p) {
  if (threaded) return true;
  if (!sectionOk) return false;
  if (!has(p.StartX) || !has(p.StartY)) {
    refuse("start" + opName, who() + "the path states no threading point - regenerate it");
    sectionOk = false;
    return false;
  }
  var x = wo("X", p.StartX, orgX), y = wo("Y", p.StartY, orgY);
  note("THREAD THE WIRE AT " + x + " " + y + " BEFORE START");
  block(["G0", x, y]);
  threaded = true;
  cutZ = has(p.StartZ) ? p.StartZ : null;
  return true;
}

// A cut: flat (2-axis: one plane, no pose, no lean).
function cutOk(p) {
  var lean = leaning;
  leaning = false;
  if (!inSection || condition === "") {
    refuse("free" + opName, who() + "a move outside a wire section - refused");
    return false;
  }
  if (!sectionOk) return false;
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("tilt" + opName, who() + "a posed move - the G-EDM wire post writes a 2-axis (vertical wire) cut only");
    sectionOk = false;
    return false;
  }
  if (lean) {
    refuse("uv" + opName, who() + "a leaning move (upper guide offset) - the G-EDM wire post writes a vertical 2-axis cut only");
    sectionOk = false;
    return false;
  }
  if (!startCut(p)) return false;
  if (has(p.Z) && cutZ !== null && Math.abs(p.Z - cutZ) > 0.0005) {
    refuse("z" + opName, who() + "a move changes Z - a 2-axis wire cut rides one programmed plane");
    sectionOk = false;
    return false;
  }
  return true;
}

function onRapid() {
  var lean = leaning;
  leaning = false;
  if (threaded) {
    refuse("rapid" + opName, who() + "a rapid with the wire threaded through the work - refused");
    sectionOk = false;
    return;
  }
  // Before the first cut: the G0 to the threading point (startCut) stands
  // for every positioning move.
  if (!inSection || condition === "") {
    refuse("free" + opName, who() + "a move outside a wire section - refused");
    return;
  }
  if (lean) {
    refuse("uvrapid" + opName, who() + "a leaning positioning move (upper guide offset) - the G-EDM wire post writes a vertical 2-axis cut only");
    sectionOk = false;
  }
}

function onLinear() {
  var p = properties;
  if (!cutOk(p)) return;
  // No F word: the gap servo sets the rate.
  block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
}

function onCircular() {
  var p = properties;
  if (p.CircularPlane && p.CircularPlane !== "XY") {
    refuse("plane" + opName, who() + "an arc in the " + p.CircularPlane + " plane - a 2-axis wire cut is flat");
    sectionOk = false;
    return;
  }
  if (!cutOk(p)) return;
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J) || !has(p.StartX) || !has(p.StartY)) {
    refuse("arcc" + opName, who() + "an arc states no centre - refused");
    sectionOk = false;
    return;
  }
  block([
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY),
    "I" + fu(c.I - p.StartX), "J" + fu(c.J - p.StartY),
  ]);
}

function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - a wire EDM has no drilling cycle");
}

function onSectionEnd() {
  inSection = false;
  sectionOk = false;
  threaded = false;
  condition = "";
  side = "";
  afterOp = "";
  taper = 0;
  fourAxis = "";
  angleWord = false;
  leaning = false;
  controlOffset = false;
  spindleAsk = false;
}

// ------------------------------------------------------------ footer
function onClose() {
  block(["M30"]);
}

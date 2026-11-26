// Generic ISO-style wire EDM post (#533, part of #290). Provenance: the
// RS-274 / ISO 6983 motion words only (G0/G1/G2/G3 with I J centre
// increments, G17, G90, G20/G21, G54-style work offsets, M00/M01
// program stops, M30 program end, % tape fences) and #290's decisions
// D2, D5 and D10. Every EDM-specific word - condition select,
// compensation cancel, automatic wire threading and cutting, wire-tip
// disposal - is the machine's own text from its `.mch` `edm.codes`
// (D5): no vendor dialect, manual table or post was read or copied,
// and nothing here invents an EDM code. Skim passes (#538) select
// their own condition mid-section, the wire left threaded. Behaviour only, per this repo's
// Clean-Room Protocol. The decision record is
// docs/decisions/533-wire-iso-post.md; the goldens lock this file.
// A constant taper (#536): the app hands each leaning move's X Y at the
// lower guide and the upper guide's offset from it (PARAM WireU and
// WireV, from
// the pose and the machine's guide planes); the words that switch the
// taper on and off and that write U V are the machine's edm.codes.
// A 4-axis cut (#537, PARAM WireFourAxis: its pairing rule) leans the
// same way: the same words, the same U V, no stated angle.
// An angle-word machine (#1164, PARAM WireTaperDialect "angle", its
// corner rule PARAM WireCorner): the app hands each move's X Y on the
// programmed plane (PARAM WirePlane, the work's top) and the angle word
// before it (PARAM WireAngle, signed by the machine's side rule, 0 for
// the move that stands the wire upright); the post writes taper_on and
// the taper_angle template ({angle}) before the first leaning move, and
// taper_off before the upright one - the control ramps the lean over
// the move after each.
description = "Generic ISO wire EDM (RS-274 motion; every EDM code from the machine's edm.codes)";
vendor = "Camcraft";
legal = "Original clean-room work; public standards only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
wireEdm = true;            // voices PARAM EdmCondition (the engine's door)
afterModes = true;         // reads PARAM AfterOperation (M01 / M00; home refused)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
explains = "paren"; // #1389: explain ISO words only; machine EDM codes keep their own meanings

// Determinism law: no timestamps, no randomness.

// ---------------------------------------------------------------- codes
// The codes every wire section needs. The path is the wire-CENTRE path
// (#532 puts the kerf in it), so the control must not offset it again:
// compensation is cancelled before every cut and never engaged - the
// machine's left/right compensation codes are not read.
var REQUIRED = ["condition", "comp_off"];
// With automatic wire threading (`edm.awt: true`): thread at the start
// hole, cut the wire after the cut, and dispose of the cut tip. Without
// it the operator threads and cuts at an M00 the program states.
var AWT_REQUIRED = ["thread", "cut", "tip_disposal"];
// A tapered section (#536) also needs the taper's switch and its U V
// words; `uv` is a template holding {u} and {v}.
var TAPER_REQUIRED = ["taper_on", "taper_off", "uv"];
// An angle-word taper (#1164): the switch and the angle template.
var ANGLE_REQUIRED = ["taper_on", "taper_off", "taper_angle"];

// ---------------------------------------------------------------- state
var inch = false;
var wcsWord = "";
var orgX = 0, orgY = 0;
var opName = "", opKind = "", setupName = "";
var condition = "", side = "", afterOp = "";
var inSection = false;     // between onSection and onSectionEnd
var threaded = false;      // the wire is threaded through the work
var cutZ = null;           // the section's one programmed plane (2-axis)
var skim = 0;             // skim passes begun in this section (#538)
var skimPending = false;   // a skim's condition arrived; select it at its first move
var awt = false;
var taper = 0;             // the section's stated taper, degrees (0: vertical)
var fourAxis = "";         // the section's 4-axis pairing rule ("": one profile)
var uv = null;             // the next move's upper guide offset [U, V] (WireU, WireV)
var tapering = false;      // the taper switch is on
var taperOk = true;
var dialect = "";          // "angle": the machine tapers by angle word (#1164)
var corner = "", plane = null;
var angleNext = null;      // the next move's angle word (WireAngle)
// A wire number the stream states but that is no finite number (#3236):
// the param's name, "" for none. A non-finite f64 reaches the stream as
// "null" (or "nan" / "inf" in a hand-written one); skipped, it would turn
// a leaning move into a plain G1. Refused, and no cut of the section is
// written.
var WIRE_NUMBERS = ["WireTaper", "WirePlane", "WireAngle", "WireU", "WireV"];
var notFinite = "";
var codesOk = false;
var refused = {};

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
// A dimension word always carries its decimal point (#3206): many wire
// controls read a number without one in their least input unit, so
// "X5" would be 5 um there, not 5 mm.
function pt(s) {
  s = s.replace(/0+$/, "");
  return (s === "-0." || s === "-.") ? "0." : s;
}
function fu(v) { return pt(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3)); }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function arcWord(k, v) { return k === "Z" ? "" : wo(k, v, k === "X" ? orgX : orgY); }  // #3442: a position word as written (no Z), for arcFate
function block(words, machineCode) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) {
    var line = a.join(" "), c = machineCode ? "" : explain(a);
    if (c && line.length + 1 + c.length <= 80) line += " " + c;
    writeln(line);
  }
}
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + s + ")"); }
function refuse(key, s) {
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 48 ? t.substring(0, 48).trim() : t;
}
function note(s) { block(["(" + s + ")"]); }
// Program notes use this dialect without truncating the stated bound.
function onComment(s) { note(commentText(s)); }

// ------------------------------------------------------------- machine
function machineName() {
  var m = typeof machine !== "undefined" ? machine : null;
  return (m && (m.model || m.name)) || "no machine";
}
function codes() {
  var m = typeof machine !== "undefined" ? machine : null;
  return (m && m.edm && m.edm.codes && typeof m.edm.codes === "object") ? m.edm.codes : {};
}
function code(k) {
  var v = codes()[k];
  return (typeof v === "string" && v.replace(/\s/g, "") !== "") ? v : null;
}
// The machine check, once per program: a wire EDM, and every code this
// post needs stated - each missing one refused by its name (D5).
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  if (!m || m.kind !== "wire_edm") {
    refuse("kind", "the generic wire EDM post writes wire words only a wire EDM reads - machine " + machineName() + " is not declared kind wire_edm");
    return false;
  }
  awt = !!(m.edm && m.edm.awt === true);
  var need = REQUIRED.concat(awt ? AWT_REQUIRED : []);
  var ok = true;
  for (var i = 0; i < need.length; i++) {
    if (code(need[i]) === null) {
      refuse("code" + need[i], "machine " + machineName() + " states no edm.codes." + need[i] + " - the wire post never invents an EDM code");
      ok = false;
    }
  }
  var c = code("condition");
  if (c !== null && c.indexOf("{id}") < 0) {
    refuse("condid", "machine " + machineName() + ": edm.codes.condition states no {id} place for the condition's id");
    ok = false;
  }
  if (m.rotaries && m.rotaries.length > 0) {
    refuse("rotary", "machine " + machineName() + " states a rotary axis - this post writes a 2-axis wire cut only");
    ok = false;
  }
  return ok;
}

// -------------------------------------------------------------- header
// ISO 6983 tape fence, the unit and absolute positions; the program
// starts with the wire NOT threaded, which the header says.
function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions && postOptions.Units === "inch"; // #902: the document's unit, never the machine file's
  codesOk = machineOk();
  writeln("%");
  note(clean(machineName()));
  note("WIRE NOT THREADED AT START");
  // #1499: a tank or flushing the machine file states is the Sodick
  // post's to write; this post has no edm.codes word for either, so it
  // says so rather than drop the ask silently.
  var e = (typeof machine !== "undefined" && machine && machine.edm) || {};
  if (e.tank) note("TANK " + clean(e.tank).toUpperCase() + " IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL");
  if (e.flushing) note("FLUSHING " + clean(e.flushing).toUpperCase() + " IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL");
  if (e.skim_flushing) note("SKIM FLUSHING " + clean(e.skim_flushing).toUpperCase() + " IS STATED BUT NOT PROGRAMMED - SET IT AT THE CONTROL"); // #2686
  block([inch ? "G20" : "G21", "G90", "G17"]);
}

function onParameter(name, value) {
  if (WIRE_NUMBERS.indexOf(name) >= 0 && !has(value)) {
    notFinite = name;
    refuse("nf" + name + opName, who() + "PARAM " + name + " is not a finite number (" + (value === null || value === undefined ? "null" : clean(value)) + ") - refused, the wire post writes no cut from it");
    return;
  }
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "EdmCondition") {
    // With the wire threaded, a condition is the next skim's (#538):
    // selected at that pass's first move, the wire left threaded.
    condition = String(value === null || value === undefined ? "" : value);
    if (threaded) skimPending = true;
  }
  else if (name === "WireSide") side = clean(value);
  else if (name === "AfterOperation") afterOp = String(value);
  else if (name === "WireTaper" && has(value)) taper = Number(value);
  else if (name === "WireFourAxis") fourAxis = clean(value);
  else if (name === "WireTaperDialect") dialect = clean(value);
  else if (name === "WireCorner") corner = clean(value);
  else if (name === "WirePlane" && has(value)) plane = Number(value);
  else if (name === "WireAngle" && has(value)) angleNext = Number(value);
  else if (name === "WireU" && has(value)) uv = [Number(value), uv !== null ? uv[1] : null];
  else if (name === "WireV" && has(value) && uv !== null) uv = [uv[0], Number(value)];
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - a wire EDM turns no spindle");
  }
}

function onToolChange(number, name) {
  refuse("tool" + opName, who() + "changes to tool " + number + " - a wire EDM mounts no tool (the wire is the condition's)");
}

// ------------------------------------------------------------- section
function onSection() {
  inSection = true;
  threaded = false;
  skim = 0;
  skimPending = false;
  cutZ = null;
  if (condition === "") {
    refuse("op" + opName, who() + "is not wire EDM work (no EDM condition) - the wire post posts Wire Contour operations only");
    return;
  }
  if (!/^[A-Za-z0-9._-]+$/.test(condition)) {
    refuse("cid" + opName, who() + "condition id " + JSON.stringify(condition) + " is not one word the control can read");
  }
  note(opName + (opKind ? " - " + opKind : ""));
  if (setupName) note("SETUP " + setupName);
  if (side) note("SIDE " + side.toUpperCase() + " - KERF IN THE PATH");
  taperOk = true;
  if (dialect === "angle" && (taper === 0 || fourAxis !== "")) {
    refuse("angle4" + opName, who() + "an angle-word taper needs one stated angle - refused");
    taperOk = false;
  } else if (dialect === "angle") {
    for (var k = 0; k < ANGLE_REQUIRED.length; k++) {
      if (code(ANGLE_REQUIRED[k]) === null) {
        refuse("acode" + ANGLE_REQUIRED[k] + opName, who() + "is tapered by angle word, and machine " + machineName() + " states no edm.codes." + ANGLE_REQUIRED[k] + " - the wire post never invents an EDM code");
        taperOk = false;
      }
    }
    var at = code("taper_angle");
    if (at !== null && at.indexOf("{angle}") < 0) {
      refuse("angtpl" + opName, "machine " + machineName() + ": edm.codes.taper_angle states no {angle} place for the taper angle");
      taperOk = false;
    }
    note("TAPER " + trim(Number(taper).toFixed(3)) + " DEG BY ANGLE WORD - XY ON THE PROGRAMMED PLANE Z" + (has(plane) ? trim(fu(plane)) : "?") + ", CORNERS " + corner.toUpperCase());
  } else if (taper !== 0 || fourAxis !== "") {
    for (var i = 0; i < TAPER_REQUIRED.length; i++) {
      if (code(TAPER_REQUIRED[i]) === null) {
        refuse("tcode" + TAPER_REQUIRED[i] + opName, who() + (fourAxis !== "" ? "is a 4-axis cut" : "is tapered") + ", and machine " + machineName() + " states no edm.codes." + TAPER_REQUIRED[i] + (TAPER_REQUIRED[i] === "uv" ? " (nor an angle-word taper, edm.taper_angle)" : "") + " - the wire post never invents an EDM code");
        taperOk = false;
      }
    }
    var t = code("uv");
    if (t !== null && (t.indexOf("{u}") < 0 || t.indexOf("{v}") < 0)) {
      refuse("uvtpl" + opName, "machine " + machineName() + ": edm.codes.uv states no {u} and {v} places for the upper guide's offset");
      taperOk = false;
    }
    if (fourAxis !== "") note("4-AXIS UPPER AND LOWER PROFILES PAIRED BY " + fourAxis.toUpperCase() + " - XY LOWER GUIDE, UV UPPER GUIDE OFFSET");
    else note("TAPER " + trim(Number(taper).toFixed(3)) + " DEG - XY LOWER GUIDE, UV UPPER GUIDE OFFSET");
  }
  if (wcsWord) block([wcsWord]);
}

// The first motion of a section: go to the start hole with the wire
// cut, thread there, select the condition, cancel compensation. An open
// run (#1109, side left / right of travel) has no start hole: its path
// starts at its run-in point in the air outside the stock, and it is
// threaded there; it ends at its run-out point past the stock's far
// edge, where onSectionEnd cuts the wire - the post writes the path's
// moves only, so no return to the threading point is ever written.
function openRun() { return side === "left" || side === "right"; }
function startCut(p) {
  if (threaded) return skimPending ? startSkim() : true;
  if (!codesOk || condition === "") return false;
  if (!has(p.StartX) || !has(p.StartY)) {
    refuse("start" + opName, who() + "the path states no threading point - regenerate it");
    return false;
  }
  block(["G0", wo("X", p.StartX, orgX), wo("Y", p.StartY, orgY)]);
  if (awt) block([code("thread")], true);
  else {
    note(openRun() ? "THREAD THE WIRE AT THE RUN-IN POINT OUTSIDE THE STOCK" : "THREAD THE WIRE AT THE START HOLE");
    block(["M00"]);
  }
  block([code("condition").split("{id}").join(condition)], true);
  block([code("comp_off")], true);
  threaded = true;
  cutZ = has(p.StartZ) ? p.StartZ : null;
  return true;
}

// A skim pass (#538): the previous pass ended back at the threading
// point; select the skim's condition and cancel compensation again
// before its first move. Its offset is in the path, as the rough's.
function startSkim() {
  skimPending = false;
  if (condition === "" || !/^[A-Za-z0-9._-]+$/.test(condition)) {
    refuse("skim" + opName, who() + "skim condition id " + JSON.stringify(condition) + " is not one word the control can read");
    return false;
  }
  skim++;
  note("SKIM " + skim);
  block([code("condition").split("{id}").join(condition)], true);
  block([code("comp_off")], true);
  return true;
}

// A cut: flat (2-axis: one plane, no pose), from a threaded wire.
function cutOk(p) {
  if (!inSection || condition === "") {
    refuse("free" + opName, who() + "a move outside a wire section - refused");
    return false;
  }
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("tilt" + opName, who() + "a posed move - this post writes a 2-axis (vertical wire) cut only");
    return false;
  }
  if (notFinite !== "") return false; // refused by name in onParameter (#3236)
  if (!startCut(p)) return false;
  if (has(p.Z) && cutZ !== null && Math.abs(p.Z - cutZ) > 0.0005) {
    refuse("z" + opName, who() + "a move changes Z - a 2-axis wire cut rides one programmed plane");
    return false;
  }
  return true;
}

function onRapid() {
  var p = properties;
  if (threaded) {
    refuse("rapid" + opName, who() + "a rapid with the wire threaded through the work - refused");
    return;
  }
  // Before the first cut the wire is not threaded: the G0 to the first
  // cut's start (the start hole, written by startCut) stands for every
  // positioning move before it.
  if (!inSection || condition === "") refuse("free" + opName, who() + "a move outside a wire section - refused");
}

function onLinear() {
  var p = properties;
  if (!cutOk(p)) return;
  var w = uv;
  uv = null;
  var ang = angleNext;
  angleNext = null;
  if (ang !== null) {
    if (dialect !== "angle" || !taperOk) {
      refuse("ang" + opName, who() + "an angle word in a section that states no angle-word taper - refused");
      return;
    }
    if (ang !== 0 && !tapering) {
      block([code("taper_on")], true);
      block([code("taper_angle").split("{angle}").join(pt(Number(ang).toFixed(3)))], true);
      tapering = true;
    } else if (ang === 0 && tapering) {
      block([code("taper_off")], true);
      tapering = false;
    }
    block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    return;
  }
  if (w !== null) {
    if ((taper === 0 && fourAxis === "") || !taperOk || !has(w[1])) {
      refuse("uv" + opName, who() + "a leaning move in a section that states no taper - refused");
      return;
    }
    if (!tapering) {
      block([code("taper_on")], true);
      tapering = true;
    }
    block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), code("uv").split("{u}").join(fu(w[0])).split("{v}").join(fu(w[1]))], true);
    return;
  }
  // No F word: the condition's generator sets the cutting rate.
  block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
}

function onCircular() {
  var p = properties;
  if (p.CircularPlane && p.CircularPlane !== "XY") {
    refuse("plane" + opName, who() + "an arc in the " + p.CircularPlane + " plane - a 2-axis wire cut is flat");
    return;
  }
  if (!cutOk(p)) return;
  if (uv !== null) {
    refuse("uvarc" + opName, who() + "a tapered arc - this post writes straight tapered moves only");
    return;
  }
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  var c = p.IJKIncrement || {};
  block([
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY),
    has(c.I) && has(p.StartX) ? "I" + fu(c.I - p.StartX) : "",
    has(c.J) && has(p.StartY) ? "J" + fu(c.J - p.StartY) : "",
  ]);
}

function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - a wire EDM has no drilling cycle");
}

// The section's end: cut the wire (and dispose of its tip), then the
// operator's stop if the op asks one.
function onSectionEnd() {
  // The wire stands upright again (the lead-out's U0 V0) before the
  // taper is switched off.
  if (tapering) block([code("taper_off")], true);
  if (threaded) {
    if (awt) {
      block([code("cut")], true);
      block([code("tip_disposal")], true);
    } else {
      note(openRun() ? "CUT THE WIRE AT THE RUN-OUT POINT" : "CUT THE WIRE");
      block(["M00"]);
    }
  }
  if (afterOp === "optional-stop") block(["M01"]);
  else if (afterOp === "stop") block(["M00"]);
  else if (afterOp === "home") {
    refuse("home" + opName, who() + "asks a trip home - the generic wire post states no home position; set the operation to Continue");
  }
  inSection = false;
  threaded = false;
  skim = 0;
  skimPending = false;
  condition = "";
  side = "";
  afterOp = "";
  taper = 0;
  fourAxis = "";
  uv = null;
  tapering = false;
  dialect = "";
  corner = "";
  plane = null;
  angleNext = null;
  notFinite = "";
}

// ------------------------------------------------------------ footer
function onClose() {
  if (threaded) refuse("open", "the program ends with the wire threaded - refused");
  block(["M30"]);
  writeln("%");
}

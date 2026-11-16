// Sodick wire EDM post, 2-axis contour (#617). Provenance: Sodick Inc.,
// "Wire EDM Machine Operation" (operator's manual, copyright November
// 1998; A320D, A530, A530D, AP350, AP500, A600, A750 and A1000 with the
// Mark 21 / EX21 control), read on the open web
// (manualslib.com/manual/2816797/Sodick-A320d.html) - its program
// example (G54, G90, C condition, T91, G01, G40, M02), its word list (X Y
// decimal point, I J incremental arc centre, C three-digit condition) and
// its G, T and M code tables; and RS-274 / ISO 6983 for the motion words.
// No other CAM product's post, post library or output was read, and no
// program posted by one (the sample attached to #617 included). NOT RUN
// ON A SODICK CONTROL: check the output at the machine before cutting.
// What is and is not confirmed is in docs/decisions/617-sodick-wire-post.md.
//
// Written: G21 / G20, G90, the setup's G54-G59, G00 to the threading
// point, T91 (thread the wire) and T90 (cut the wire) on a machine with
// automatic threading, the C condition word, G40 on the G00 to the
// threading point (before any motion of the section), G01,
// G02 / G03 with I J, M00 / M01, M02.
// Never written (each a named refusal or an operator's task): taper
// (G50-G52, A), 4-axis (G74 / G75, U V), the control's wire offset
// (G41 / G42, H - the path is the wire-centre path with the kerf in it),
// G92 and G29, the tank, flushing and wire-drive T codes, M03, M33 / M43.
description = "Sodick wire EDM, 2-axis (Mark 21 / EX21 words from the 1998 operation manual; not verified on a control)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
wireEdm = true;            // voices PARAM EdmCondition (the engine's door)
afterModes = true;         // reads PARAM AfterOperation (M01 / M00; home refused)
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
var threaded = false;      // the wire is threaded through the work
var compOff = false;       // the section's first cut has been written
var cutZ = null;           // the section's one programmed plane (2-axis)
var awt = false;
var taper = 0;             // the section's stated taper, degrees (0: vertical)
var angleWord = false;    // WireAngle is unsupported, including a stated zero
var controlOffset = false; // a COMP record refuses this section, even before SECTION
var fourAxis = "";         // the section's 4-axis pairing rule ("": one profile)
var leaning = false;       // the next move states an upper guide offset
var machineGood = false;
var refused = {};

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
// A coordinate always carries its decimal point: the control reads a
// number without one in its smallest unit (manual, "X5." against
// "X50000"), so "X5" would be a different place.
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
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + s + ")"); }
function refuse(key, s) {
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;\/]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 48 ? t.substring(0, 48).trim() : t;
}
function note(s) { block(["(" + s + ")"]); }

// ------------------------------------------------------------- machine
function machineName() {
  var m = typeof machine !== "undefined" ? machine : null;
  return (m && (m.model || m.name)) || "no machine";
}
// The machine check, once per program: a wire EDM with no rotary.
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  if (!m || m.kind !== "wire_edm") {
    refuse("kind", "the Sodick wire EDM post writes wire words only a wire EDM reads - machine " + machineName() + " is not declared kind wire_edm");
    return false;
  }
  awt = !!(m.edm && m.edm.awt === true);
  if (m.rotaries && m.rotaries.length > 0) {
    refuse("rotary", "machine " + machineName() + " states a rotary axis - the Sodick wire post writes a 2-axis wire cut only");
    return false;
  }
  return true;
}

// -------------------------------------------------------------- header
// The unit and absolute positions. The program starts with the wire NOT
// threaded, and it states what it leaves to the operator.
function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions && postOptions.Units === "inch"; // #902: the document's unit, never the machine file's
  machineGood = machineOk();
  note(clean(machineName()));
  note("SODICK MARK 21 WORDS - NOT VERIFIED ON A CONTROL - CHECK BEFORE CUTTING");
  note("WIRE NOT THREADED AT START");
  note("WORK OFFSET SET AT THE CONTROL - NO G92 IS WRITTEN");
  note("TANK FILL AND FLUSHING ARE NOT PROGRAMMED - SET THEM AT THE CONTROL");
  note("WIRE-CENTRE PATH - NO WIRE OFFSET G41 G42 IS USED");
  note("EACH C CONDITION NAMED BELOW MUST EXIST IN THE CONTROL CONDITION TABLE BEFORE START");
  block([inch ? "G20" : "G21"]);
  block(["G90"]);
}

function onParameter(name, value) {
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "EdmCondition") {
    // #538: the C word is written once, when the wire is threaded
    // (startCut). A condition stated after the section's first cut is a
    // skim pass's; this post has no public source for how the control
    // takes a condition change mid-program (#617), so it is refused by
    // name - never cut on the rough pass's C word.
    if (inSection && (threaded || compOff)) {
      refuse("skims" + opName, who() + "Sodick: a condition change between passes (skims) is not supported by this post - post the skims through wire-iso or as separate operations");
      sectionOk = false;
      return;
    }
    condition = String(value === null || value === undefined ? "" : value);
  }
  else if (name === "WireSide") side = clean(value);
  else if (name === "AfterOperation") afterOp = String(value);
  else if (name === "WireTaper" && has(value)) taper = Number(value);
  else if (name === "WireFourAxis") fourAxis = clean(value);
  else if (name === "WireU" || name === "WireV") leaning = true;
  else if (name === "WireAngle") {
    angleWord = true;
    // #1496: guard the move itself even when the section omits WireTaper.
    // The angle stream's plane/corner mapping is not confirmed for Mark 21.
    refuse("angle" + opName, who() + "an angle-word move - the Sodick wire post writes a vertical 2-axis cut only; programmed-plane and corner mapping for G51 / G52 A is not confirmed");
    sectionOk = false;
  }
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - a wire EDM turns no spindle");
  }
}

function onToolChange(number, name) {
  refuse("tool" + opName, who() + "changes to tool " + number + " - a wire EDM mounts no tool (the wire is the condition's)");
}

// #1498: the wire laws judge the generated wire-centre path. No stored
// H-register value ties a control-offset path to that judged geometry.
// Refuse every COMP record (including wear and OFF); OFF cannot make a
// refused section safe again. Section end resets the refusal.
function onRadiusCompensation() {
  controlOffset = true;
  sectionOk = false;
  refuse("offset" + opName, who() + "Sodick: control-side wire offset (G41/G42 H) is not supported - no verified H-register value is stored; post a generated wire-centre path without control or wear compensation");
}

// ------------------------------------------------------------- section
function onSection() {
  inSection = true;
  sectionOk = false;
  threaded = false;
  compOff = false;
  cutZ = null;
  if (condition === "") {
    refuse("op" + opName, who() + "is not wire EDM work (no EDM condition) - the Sodick wire post posts Wire Contour operations only");
    return;
  }
  var ok = machineGood && !controlOffset;
  // The manual's condition word: C and three digits (C000 to C999). The
  // operation's condition id is that word; anything else is refused.
  if (!/^C[0-9]{3}$/.test(condition)) {
    refuse("cid" + opName, who() + "condition id " + JSON.stringify(condition) + " is not a Sodick condition word (C and three digits, C000 to C999) - name the machine file's condition that way");
    ok = false;
  }
  if (fourAxis !== "") {
    refuse("four" + opName, who() + "is a 4-axis cut - the Sodick wire post writes a vertical 2-axis cut only (no G74 / G75 or U V words)");
    ok = false;
  } else if (taper !== 0) {
    refuse("taper" + opName, who() + "is tapered - the Sodick wire post writes a vertical 2-axis cut only (no G51 / G52 or A words)");
    ok = false;
  }
  if (angleWord) {
    refuse("angle" + opName, who() + "an angle-word move - the Sodick wire post writes a vertical 2-axis cut only; programmed-plane and corner mapping for G51 / G52 A is not confirmed");
    ok = false;
  }
  if (!/^G5[4-9]$/.test(wcsWord)) {
    refuse("wcs" + opName, who() + "work offset " + JSON.stringify(wcsWord) + " is not G54 to G59 - the Sodick wire post writes no G92, so the setup must state one");
    ok = false;
  }
  if (!ok) return;
  sectionOk = true;
  note(opName + (opKind ? " - " + opKind : ""));
  if (setupName) note("SETUP " + setupName);
  if (side) note("SIDE " + side.toUpperCase() + " - KERF IN THE PATH");
  // The C word selects a row of the control's own condition table; the
  // program carries no table, so the operator checks the row exists.
  note("CONDITION " + condition + " MUST EXIST IN THE CONTROL CONDITION TABLE - CHECK ITS VALUES");
  block([wcsWord]);
}

// The first motion of a section: go to the threading point with the
// wire cut, thread there (T91, or the operator at an M00), select the
// condition. An open run (side left / right of travel) threads at its
// run-in point outside the stock and is cut at its run-out point.
function openRun() { return side === "left" || side === "right"; }
function startCut(p) {
  if (threaded) return true;
  if (!sectionOk) return false;
  if (!has(p.StartX) || !has(p.StartY)) {
    refuse("start" + opName, who() + "the path states no threading point - regenerate it");
    return false;
  }
  // G40 rides the section's first motion, the G00 to the threading point
  // (the manual cancels offset on a move, "G40 Y-.02"; RS-274 puts G40 in
  // its own modal group, so it shares a block with G00, and cancelling an
  // offset that is already off is legal): a wire offset left active by an
  // earlier program would shift the positioning, the threading point and
  // every cut after it.
  block(["G40", "G00", wo("X", p.StartX, orgX), wo("Y", p.StartY, orgY)]);
  if (awt) block(["T91"]);
  else {
    note(openRun() ? "THREAD THE WIRE AT THE RUN-IN POINT OUTSIDE THE STOCK" : "THREAD THE WIRE AT THE START HOLE");
    block(["M00"]);
  }
  block([condition]);
  threaded = true;
  cutZ = has(p.StartZ) ? p.StartZ : null;
  return true;
}

// A cut: flat (2-axis: one plane, no pose, no lean), from a threaded wire.
function cutOk(p) {
  var lean = leaning;
  leaning = false;
  if (!inSection || condition === "") {
    refuse("free" + opName, who() + "a move outside a wire section - refused");
    return false;
  }
  if (!sectionOk) return false;
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("tilt" + opName, who() + "a posed move - the Sodick wire post writes a 2-axis (vertical wire) cut only");
    return false;
  }
  if (lean) {
    refuse("uv" + opName, who() + "a leaning move (upper guide offset) - the Sodick wire post writes a vertical 2-axis cut only");
    return false;
  }
  if (!startCut(p)) return false;
  if (has(p.Z) && cutZ !== null && Math.abs(p.Z - cutZ) > 0.0005) {
    refuse("z" + opName, who() + "a move changes Z - a 2-axis wire cut rides one programmed plane");
    return false;
  }
  return true;
}

function onRapid() {
  leaning = false;
  if (threaded) {
    refuse("rapid" + opName, who() + "a rapid with the wire threaded through the work - refused");
    return;
  }
  // Before the first cut the wire is not threaded: the G00 to the first
  // cut's start (written by startCut) stands for every positioning move.
  if (!inSection || condition === "") refuse("free" + opName, who() + "a move outside a wire section - refused");
}

function onLinear() {
  var p = properties;
  if (!cutOk(p)) return;
  // No F word: the condition sets the rate. G40 was written on the G00
  // to the threading point (startCut).
  block(["G01", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
  compOff = true;
}

function onCircular() {
  var p = properties;
  if (p.CircularPlane && p.CircularPlane !== "XY") {
    refuse("plane" + opName, who() + "an arc in the " + p.CircularPlane + " plane - a 2-axis wire cut is flat");
    return;
  }
  if (!threaded && !compOff && sectionOk && inSection && condition !== "") {
    refuse("arcfirst" + opName, who() + "the first cut is an arc - the Sodick wire post starts every cut with a straight move from the threading point (an arc start is not shown in its source); give the cut a straight lead-in");
    sectionOk = false;
    return;
  }
  if (!cutOk(p)) return;
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J) || !has(p.StartX) || !has(p.StartY)) {
    refuse("arcc" + opName, who() + "an arc states no centre - refused");
    return;
  }
  block([
    p.CircularDirection === "CW" ? "G02" : "G03",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY),
    "I" + fu(c.I - p.StartX), "J" + fu(c.J - p.StartY),
  ]);
}

function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - a wire EDM has no drilling cycle");
}

// The section's end: cut the wire (T90, or the operator at an M00), then
// the operator's stop if the op asks one.
function onSectionEnd() {
  if (threaded) {
    if (awt) block(["T90"]);
    else {
      note(openRun() ? "CUT THE WIRE AT THE RUN-OUT POINT" : "CUT THE WIRE");
      block(["M00"]);
    }
  }
  if (afterOp === "optional-stop") block(["M01"]);
  else if (afterOp === "stop") block(["M00"]);
  else if (afterOp === "home") {
    refuse("home" + opName, who() + "asks a trip home - the Sodick wire post states no home position; set the operation to Continue");
  }
  inSection = false;
  sectionOk = false;
  threaded = false;
  compOff = false;
  condition = "";
  side = "";
  afterOp = "";
  taper = 0;
  angleWord = false;
  controlOffset = false;
  fourAxis = "";
  leaning = false;
}

// ------------------------------------------------------------ footer
function onClose() {
  if (threaded) refuse("open", "the program ends with the wire threaded - refused");
  block(["M02"]);
}

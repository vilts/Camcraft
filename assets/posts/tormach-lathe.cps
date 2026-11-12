// Tormach PathPilot (LinuxCNC) 2-axis LATHE post (#612). Provenance:
// the G-dialect from the public LinuxCNC G-code reference
// (linuxcnc.org/docs/html/gcode/g-code.html: G7/G8, G18, G33, G33.1,
// G76, G95/G94, G96 D/S, G97, G53, G28/G30, G64) and the LinuxCNC
// "Lathe User Information" page (linuxcnc.org/docs/html/lathe/
// lathe-user.html: under G7 the X word is a diameter and the arc's I
// offset stays a radius), with Tormach's public PathPilot lathe
// programming pages (knowledgebase.tormach.com/15l/programming-15l and
// its 8L twin: T word = tool + geometry offset, optional second pair
// the wear line, no M6 on a lathe; G7 always, G8 unsupported; G18
// only; G20/G21 before any motion; the supported G and M code tables -
// no G43/G49, no G80/G81/G83 on the lathe; G76 I J K diameter values
// under G7; G30 to a safe place before tool changes; M30 ends) -
// behavior only, per this repo's Clean-Room Protocol. No vendor post
// script text and no CAM post's output used; original work. Structure
// follows our own fanuc-lathe.cps and tormach.cps; the goldens lock
// this file's decisions (crates/app/tests/golden/nc/tormach-lathe/),
// the reasons are in docs/decisions/612-pathpilot-lathe-post.md.
description = "Tormach PathPilot lathe (LinuxCNC: G7 diameter X, G18, G20/G21 from the machine, G95/G96 D, T0101, G33 or one-block G76 threading)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 2; // reference contract: integer feature mask (bit 2 = turning)
coolantModes = true; // reads PARAM Coolant (#112/#150)
afterModes = true;   // reads PARAM AfterOperation (#110)
threadHands = true;  // reads PARAM SpindleDirection (#629): M4 when the thread's hand needs it
// liveTooling is NOT declared: PathPilot's lathe tables list no C axis
// and no live-tool words, so the engine refuses a C-axis section by
// name before this script ever sees it.

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).
//
// The lathe laws this script states, so a reviewer need not infer them:
//  - G7 DIAMETER mode, stated in the header (PathPilot runs G7 only):
//    the X word is twice the stream's radius. The stream speaks world
//    mm on ONE meridian half-plane of the spindle axis; the radius is
//    that plane's signed in-plane coordinate, origin-relative. A move
//    with BOTH in-plane coordinates off the axis is off every meridian
//    - milling work - refused by name, never projected.
//  - G40 throughout, and the stream's point IS the control point
//    (#630/#641): the app has already moved a nose-radiused insert's
//    path to its imaginary tip for the tool's orientation (PARAM
//    ToolOrientation, the LinuxCNC 1-9 tip chart), and a blade's to its
//    touched-off corner. This script never shifts a point; it writes
//    the orientation in the tool note so the setup can be checked
//    against the tool table's Q.
//  - Units from the machine: G20 (every length / 25.4, four decimals,
//    feeds per rev at five, pitches at six) or G21 (mm, three decimals,
//    feeds per rev at four, pitches at six). Angles and rpm never scale.
//  - G95 feed per revolution for every cut: F = mm/min / rpm (/ 25.4 on
//    an inch machine), so the cut speed the generator planned is the
//    one the control runs. No rpm, no per-rev feed - refused by name.
//    Nothing this post writes needs G94 (no live tooling, no C axis).
//  - G97 S constant rpm from the stream's SpindleSpeed; G96 constant
//    surface speed ONLY when the stream states a SurfaceSpeed PARAM
//    (m/min; ft/min under G20), with D = the lower of the machine's
//    stated maximum rpm and the tool's own (PARAM MaxSpindleSpeed,
//    #891) as the cap; with neither stated the G96 is refused by name
//    (an unclamped CSS runs away toward the centre). A section that
//    posted G96 cancels it at its end (G97 after M5, #1173), so every
//    section opens under G97 unless it states G96 itself, and the
//    program never ends in CSS. M3, unless the stream states PARAM
//    SpindleDirection reverse (#629, a thread whose hand needs it):
//    then M4, with a note to mount the insert for reverse rotation.
//  - Arcs: G18 ZX arcs with I K centre increments (G91.1, stated), I a
//    RADIUS under G7 (LinuxCNC lathe page). Direction by the view-
//    from-+Y law: a YZ-stated meridian arc flips CW<->CCW, an XZ one
//    does not. An XY arc is a thread half-turn or a refusal.
//  - Threading, default: G33 Z K - the LinuxCNC spindle-synchronized
//    move, K = distance per revolution (the lead). The threading
//    kernel spells a pass as XY half-turn arcs about the axis (toward
//    the chuck, -Z, by default); the half-turns of one pass at one
//    radius are merged into ONE G33 block per pass, so synchronization
//    starts once per pass on the spindle index (never a G32 - LinuxCNC
//    has none). K is ONE number per thread: the THREAD record's lead,
//    never re-derived from a block's rounded Z travel.
//  - The thread's HAND (#629) is never dropped. The stream's +Z is the
//    free end, the chuck below. A half-turn clockwise seen from +Z is
//    the tool's motion about a part turning M3; counter-clockwise needs
//    M4 - the stream says so beforehand (PARAM SpindleDirection
//    reverse). A half-turn turning against the section's spindle is
//    refused by name (it would cut the other hand), and so is a G76
//    record whose hand and Z direction disagree with it. G33 and G76
//    both cut from where the tool stands toward their Z word, so the
//    direction is the stream's own.
//  - Threading, postOptions.Threading = "G76": the record becomes the
//    one-block LinuxCNC cycle G76 P Z I J K R Q H (the word order of
//    PathPilot's own conversational threading), the passes inside it
//    skipped. Only what the record states is written: R1 for its equal
//    rungs (LinuxCNC: "R1.0 selects constant depth"), Q0 for its radial
//    infeed, H its spring passes; no finish allowance (G76 has no word
//    for one), no pull-out (no E/L taper), an external thread cut from
//    the +X side. I J K are diameters on the control's resolution
//    grid, rounded so the last pass never goes deeper than the
//    record's minor diameter. Anything else is refused by name, never
//    posted as a different thread.
//  - Drilling on centre: PathPilot's lathe lists no G81/G83 (and under
//    G18 LinuxCNC drills along Y), so a hole posts as the stream's own
//    expanded chain - plain G0/G1 moves on X0 - with a G4 dwell (P in
//    SECONDS) at the floor when the cycle asks one. A tap (the chain
//    cannot reverse the spindle), a back-bore and a hole off the axis
//    are refused by name.
//  - Tool change: G30 (the operator's preset safe position, Tormach's
//    own advice before a tool change), then T<tool><offset> - T0101 -
//    which indexes the turret or pauses for a manual tool, and applies
//    the geometry and wear offsets. No M6, no G43 on a PathPilot lathe.
//  - No % fences and no O word (LinuxCNC reads O as a subroutine label);
//    comments are parenthesized and cleaned of parentheses - LinuxCNC
//    has no nested comments.

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var sRpm = 0;              // S/M3 waits for the section (PARAMs, TOOL, SECTION order)
var sCss = 0;              // SurfaceSpeed PARAM (m/min), 0 = none stated
var cssOn = false;         // this section posted G96: its end cancels it (#1173)
var sMax = 0;              // MaxSpindleSpeed PARAM (#891): the tool's own rpm ceiling, 0 = none
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var sectionOpen = false;
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var homedAt = -1;          // linesWritten() just after a trip home (#504)
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (world -> program)
var inThread = false;      // a G76 cycle stands for the passes being skipped
var threadSeen = false;    // inside a THREAD record's span (cycle or not)
var threadLead = 0;        // ... and that record's lead (mm/rev), 0 = none stated
var spinRev = false;       // PARAM SpindleDirection reverse (#629): this section's spindle runs M4
var refused = {};          // each refusal counted once per program, by name
var EPS = 0.0005;          // half a micron: the stream's own lattice
var inch = false;          // the machine is programmed in inches (onOpen)
var pass = null;           // the G33 pass being merged: { r, z0, z, halves, lead, dir }
var drillDwell = 0;        // the open hole's floor dwell, seconds (0 = none)
var drillFloor = null;     // ... and its floor Z (world mm)
var drillIgnore = false;   // the open hole was refused: its chain posts nothing

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function trim0(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the machine's unit
  if (!has(v)) return "0";
  return trim0(Number(len(v)).toFixed(inch ? 4 : 3));
}
function fr(v) {  // a feed per revolution (mm/rev in), in the machine's unit
  if (!has(v)) return "0";
  return trim0(Number(len(v)).toFixed(inch ? 5 : 4));
}
function fp(v) {  // a lead / pitch (mm/rev in): six decimals - 24 TPI is no round number
  if (!has(v)) return "0";
  return trim0(Number(len(v)).toFixed(6));
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function secs(s) { return trim0(Number(s).toFixed(3)); }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + clean(s, 200) + ")"); }
// Refuse by name, once per reason per program (per-line is noise).
function refuse(why) {
  if (!refused[why]) { refused[why] = true; msg(why); }
  return null;
}

// The signed meridian coordinate of a world point, origin-relative:
// the lathe's radius. null (and a counted refusal) when the point
// stands off every meridian half-plane - milling work on a lathe post.
function radiusOf(x, y, what) {
  var dx = has(x) ? x - orgX : 0;
  var dy = has(y) ? y - orgY : 0;
  if (Math.abs(dx) <= EPS) return dy;
  if (Math.abs(dy) <= EPS) return dx;
  return refuse(what + " off the spindle axis meridian (both X and Y move) - milling work, not turning");
}
function xWord(r) { return has(r) ? "X" + fu(2 * r) : ""; }         // G7: DIAMETER
function zWord(z) { return has(z) ? "Z" + fu(z - orgZ) : ""; }
// G95: feed per revolution from the stream's mm/min and the spindle rpm.
function feedWord(f) {
  if (!has(f) || f <= 0) return "";                                  // modal feed stands
  if (!(sRpm > 0)) { refuse("feed per revolution needs a spindle speed - none stated"); return ""; }
  return "F" + fr(f / sRpm);
}
function posed(p) {
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("posed move (A/B/C words) on a 2-axis lathe - no rotary to command");
    return true;
  }
  return false;
}

// The .mch tool-station coolant list carries names + M-codes (the
// same law as every shipped post, #112/#150): a flood ask takes the
// machine's Flood entry, failing one the first coded entry of no other
// known family; a codeless Flood is the operator's own valve, silent;
// a list of only other kinds is refused by name. M9 is the fixed stop.
var COOLANT_KINDS = ["flood", "mist", "through-spindle", "through-tool"];
function coolantWho() { return coolantOp ? "operation " + coolantOp + ": " : ""; }
function coolantName(c) {
  return String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
}
function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var other = "", floodNamed = false;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = coolantName(c);
    if (name === "flood") { if (code) return code; floodNamed = true; continue; }
    if (code && !other && COOLANT_KINDS.indexOf(name) < 0) other = code;
  }
  if (other || floodNamed) return other;
  if (cs.length > 0) msg(coolantWho() + "the machine states no flood coolant code - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "";
}
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []);
  if (want.length === 0) { msg(coolantWho() + "unknown coolant ask " + ask); return ""; }
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    if (code && want.indexOf(coolantName(c)) >= 0) return code;
  }
  msg(coolantWho() + "the machine states no " + ask + " coolant code - pick another coolant for the operation or add it to the machine");
  return "";
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool.
// User text is cleaned for LinuxCNC: a parenthesis would end (or nest)
// the comment, ; opens an end-of-line comment and % is a file fence,
// so all become spaces; printable ASCII only, capped.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0, toolQ = 0;
function clean(s, cap) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  var n = cap || 40;
  return t.length > n ? t.substring(0, n).trim() : t;
}
function joinDash(parts) {
  var a = [];
  for (var i = 0; i < parts.length; i++) if (parts[i]) a.push(parts[i]);
  return a.join(" - ");
}
function note(s) { block(["(" + s + ")"]); }
function commentParam(name, value) {
  if (name === "SetupName") setupName = clean(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value).toUpperCase();
  else if (name === "ToolKind") toolKind = clean(value).toUpperCase();
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "ToolCornerRadius" && has(value)) toolCr = Number(value);
  else if (name === "ToolOrientation" && has(value)) toolQ = Math.round(Number(value));
}
function opNotes() {
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
// A lathe blade's library diameter is no size the operator measures:
// its note names the kind alone (rotating tools keep their D).
var BLADE_KINDS = ["TURNING INSERT", "GROOVING BLADE", "THREADING INSERT", "FORM GAUGE", "BORING BAR"];
function tWord() { return "T" + pad2(tNo) + pad2(tNo); }
function toolNote() {
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0 && BLADE_KINDS.indexOf(toolKind) < 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  if (toolQ > 0) desc.push("ORIENTATION " + toolQ);  // #630: the tip the program's points are for
  note(joinDash([tWord() + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0; toolQ = 0;
}
// A WARN record (only a gate-refused program carries one) is voiced as
// a cleaned comment - never a bare line, never a nested parenthesis.
function onComment(s) { flushPass(); note(clean(s, 200)); }
// The bar-feeder lifecycle (M5.2): PathPilot states no feeder words, so
// the intent is a comment, the advance in the program's own unit.
function onPartLoading() {
  flushPass();
  var a = properties.Advance;
  note("PART LOAD" + (has(a) && a > 0 ? " ADVANCE " + fu(a) + (inch ? " IN" : " MM") : ""));
}
function onPartUnloading() { flushPass(); note("PART UNLOAD"); }

// -------------------------------------------------------------- header
// One identity comment and the modal state every later line relies
// on: G7 diameter X, G18 ZX plane, the unit (before any motion, as
// Tormach asks), absolute positions, incremental arc centres (G91.1,
// stated, never assumed), no cutter comp, feed per revolution. Path
// blending gets an explicit tolerance: a bare G64 is "no tolerance" in
// plain LinuxCNC (0.001 in, or its 0.025 mm twin).
function onOpen() {
  inch = typeof machine !== "undefined" && machine.units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  note(clean(nm));
  block(["G7", "G18", inch ? "G20" : "G21", "G90", "G91.1", "G40", "G95"]);
  block(["G64", inch ? "P0.001" : "P0.025"]);
}

// Cutter compensation in the control: on a lathe G41/G42 is tool NOSE
// radius compensation with the tool table's orientation - a different
// law, not stated by this post; refused by name, never a centre path
// posted as the part.
function onRadiusCompensation() {
  msg("control cutter compensation on a lathe is nose-radius compensation - not supported by the PathPilot lathe post; choose In computer");
}

function onParameter(name, value) {
  flushPass();
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "SurfaceSpeed" && has(value)) sCss = Number(value);
  else if (name === "MaxSpindleSpeed" && has(value)) sMax = Number(value);
  else if (name === "SpindleDirection") spinRev = String(value || "") === "reverse";
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg("spindle rpm over machine maximum");
  }
}

// PathPilot's lathe T word is two digits of tool (and geometry offset)
// and two of wear line: T01-T99. Anything else is an error there, so
// it is refused by name, never posted.
function onToolChange(number, name) {
  flushPass();
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1 || number > 99 || Math.round(number) !== Number(number)) {
    msg("tool number " + number + " is outside PathPilot's lathe T word (T01-T99)");
  }
}

// The rpm ceiling a G96 section runs under (#891): the lower of the
// machine's stated spindle maximum and the tool's own (PARAM
// MaxSpindleSpeed); 0 when neither is stated - and then no G96 posts.
function cssCap() {
  var mm = (typeof machine !== "undefined" && machine.spindleMax > 0) ? machine.spindleMax : 0;
  var tm = sMax > 0 ? sMax : 0;
  if (mm > 0 && tm > 0) return Math.min(mm, tm);
  return mm > 0 ? mm : tm;
}
var UNCLAMPED = "constant surface speed (G96) with no rpm clamp - the machine states no spindle maximum and the tool no max rpm; an unclamped G96 runs away toward the centre";

// The preamble closes over everything a raw motion line needs to be a
// legal program: offset, tool + offsets, spindle, coolant.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block([wcsWord]);
  if (toolWait) {
    toolNote();
    block(["G30"]);          // the operator's preset safe spot, then the tool
    block([tWord()]);        // T0101: tool 1, geometry 1, wear line 1
    toolWait = false;
  }
  if (spindleWait) {
    var m = spinRev ? "M4" : "M3";
    if (spinRev) note("SPINDLE M4 - THREAD HAND NEEDS REVERSE ROTATION - MOUNT THE INSERT FOR M4");
    if (sCss > 0) {
      var cap = cssCap();                              // machine and tool ceilings (#891)
      var css = inch ? sCss / 0.3048 : sCss;          // m/min -> ft/min under G20
      if (!(cap > 0)) refuse(UNCLAMPED);
      else { block(["G96", "D" + ff(cap), "S" + ff(css), m]); cssOn = true; }
    } else {
      block(["G97", "S" + ff(sRpm), m]);
    }
    spindleWait = false;
  }
  coolant = coolantFor(coolantAsk);
  if (coolant) block([coolant]);
}

// The op's after-action (#110), once its spindle has stopped. Home is
// G30, the same operator-preset safe position the tool change uses.
function afterOp() {
  var ask = afterAsk;
  afterAsk = "";
  if (!ask) return;
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    block(["G30"]);
    homedAt = linesWritten();
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  flushPass();
  if (!sectionOpen) return;
  sectionOpen = false;
  drillDwell = 0; drillFloor = null; drillIgnore = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";
  block(["M5"]);
  if (cssOn) block(["G97"]); // #1173: no section opens under a G96 it did not state
  cssOn = false;
  spinRev = false;           // the next section states its own
  sCss = 0;                  // and its own spindle mode: no stale G96 (#772)
  sMax = 0;                  // and its own clamp (#891)
  afterOp();
}

function onSection() { flushPass(); startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
function onRapid() {
  flushPass();
  startPreamble();
  var p = properties;
  if (posed(p) || inThread || drillIgnore) return;
  var r = radiusOf(p.X, p.Y, "rapid");
  if (r === null) return;
  block(["G0", xWord(r), zWord(p.Z)]);
}
function onLinear() {
  flushPass();
  startPreamble();
  var p = properties;
  if (posed(p) || inThread || drillIgnore) return;
  var r = radiusOf(p.X, p.Y, "feed move");
  if (r === null) return;
  block(["G1", xWord(r), zWord(p.Z), feedWord(p.FeedRate)]);
  // The hole's floor dwell (the chain has no move for it): G4 P in
  // SECONDS, LinuxCNC's unit, once the feed reaches the floor on axis.
  if (drillDwell > 0 && drillFloor !== null && Math.abs(r) <= EPS && Math.abs(p.Z - drillFloor) <= EPS) {
    block(["G4", "P" + secs(drillDwell)]);
  }
}

// A ZX-plane arc (G18): centre as I K increments from the start, I a
// radius (G7 keeps I a radius), direction per the view-from-+Y law.
function meridianArc(p) {
  var c = p.IJKIncrement || {};
  var yz = p.CircularPlane === "YZ";
  var r0 = radiusOf(p.StartX, p.StartY, "arc start");
  var r1 = radiusOf(p.X, p.Y, "arc end");
  var rc = yz ? radiusOf(orgX, c.J, "arc centre") : radiusOf(c.I, orgY, "arc centre");
  if (r0 === null || r1 === null || rc === null) return;
  if (!has(c.K)) { refuse("arc without a K centre word"); return; }
  var cw = p.CircularDirection === "CW";
  if (yz) cw = !cw;                       // the +Y meridian seen from +Y is mirrored
  block([
    cw ? "G2" : "G3",
    xWord(r1), zWord(p.Z),
    "I" + fu(rc - r0), "K" + fu(c.K - p.StartZ),
    feedWord(p.FeedRate),
  ]);
}

// An XY arc is a thread half-turn: centre on the axis, one radius at
// both ends, antipodal ends - a synchronized half lead of Z. Half-
// turns continuing one pass (same radius, same lead, same direction)
// merge into the pass being built; flushPass writes it as one G33.
// "Same lead" allows the stream's lattice: each Z is rounded to half a
// micron, so a half-turn's 2 x dZ may differ from the true lead by up
// to 2 um (24 TPI's half lead is no lattice number) - LEAD_SLOP.
var LEAD_SLOP = 0.0025;
function threadHalfTurn(p) {
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J)) { flushPass(); refuse("thread half-turn without an I J centre"); return; }
  var ok = Math.abs(c.I - orgX) <= EPS && Math.abs(c.J - orgY) <= EPS;
  var rho0 = Math.sqrt(Math.pow(p.StartX - orgX, 2) + Math.pow(p.StartY - orgY, 2));
  var rho1 = Math.sqrt(Math.pow(p.X - orgX, 2) + Math.pow(p.Y - orgY, 2));
  ok = ok && Math.abs(rho0 - rho1) <= EPS;
  ok = ok && Math.abs(p.StartX + p.X - 2 * c.I) <= EPS && Math.abs(p.StartY + p.Y - 2 * c.J) <= EPS;
  if (!ok) { flushPass(); refuse("XY arc that is not a thread half-turn about the spindle axis"); return; }
  var dz = p.Z - p.StartZ;
  var lead = 2 * Math.abs(dz);
  if (!(lead > 0)) { flushPass(); refuse("thread half-turn with no lead"); return; }
  if (g76Wanted() && !threadSeen) {
    flushPass();
    refuse("G76 threading cycle chosen but the stream carries no thread cycle record for these passes - regenerate the thread, or choose G33 passes");
    return;
  }
  // The hand (#629): clockwise seen from +Z is M3's, counter-clockwise
  // M4's. A half-turn against the section's spindle would cut the
  // other hand - refused, never posted.
  var cw = p.CircularDirection === "CW";
  if (cw === spinRev) {
    flushPass();
    refuse("thread half-turn turns " + (cw ? "clockwise" : "counter-clockwise") + " seen from +Z but the spindle runs " + (spinRev ? "M4" : "M3") + " - it would cut the other hand; regenerate the thread");
    return;
  }
  if (threadLead > 0 && Math.abs(lead - threadLead) > LEAD_SLOP) {
    flushPass();
    refuse("thread half-turn's Z travel does not match the thread record's lead - regenerate the thread");
    return;
  }
  var dir = dz > 0 ? 1 : -1;
  if (pass && Math.abs(pass.r - rho1) <= EPS && Math.abs(pass.lead - lead) <= 2 * LEAD_SLOP && pass.dir === dir
      && Math.abs(pass.z - p.StartZ) <= EPS) {
    pass.z = p.Z;
    pass.halves += 1;
    return;
  }
  flushPass();
  pass = { r: rho1, z0: p.StartZ, z: p.Z, halves: 1, lead: lead, dir: dir };
}
// The merged pass: G33 Z K, X modal (the plunge put the tool on the
// pass's radius). K is the lead - LinuxCNC's distance per revolution -
// ONE number for the thread: the THREAD record's lead when the stream
// states one (it rides at six decimals, never re-derived from a
// block's rounded Z), else the pass's whole Z travel over its turns.
function flushPass() {
  if (!pass) return;
  var q = pass;
  pass = null;
  var k = threadLead > 0 ? threadLead : 2 * Math.abs(q.z - q.z0) / q.halves;
  block(["G33", zWord(q.z), "K" + fp(k)]);
}

function onCircular() {
  startPreamble();
  var p = properties;
  if (posed(p) || inThread || drillIgnore) { flushPass(); return; }
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") { flushPass(); meridianArc(p); }
  else threadHalfTurn(p);
}

// ---------------------------------------------------------- G76 cycle
function g76Wanted() { return typeof postOptions !== "undefined" && postOptions.Threading === "G76"; }

// The G76 words are diameters on the control's resolution grid (G21:
// 0.001 mm, G20: 0.0001 in - the digits this post writes). Every value
// is taken to the grid ONCE, as a whole count of grid steps, and the
// cycle's words are differences of those counts - so the drive line X,
// the peak (X + I) and the root (X + I - K) the control computes are
// exactly the grid numbers chosen here, never three roundings that
// disagree. The root is rounded OUTWARD (never deeper than the
// record's minor diameter - a deeper root is a gouge of the thread
// form); J, the first depth, is rounded shallower.
function gridStep() { return inch ? 0.0001 : 0.001; }
function gridOf(mm) { return len(mm) / gridStep(); }               // mm -> grid steps (real)
function gridWord(n) { return trim0(Number(n * gridStep()).toFixed(inch ? 4 : 3)); }
// The record's numbers ride the stream's half-micron lattice one by one
// (each length rounded on its own), so two of them may disagree by up
// to a couple of microns: LAT is that allowance on a diameter. Wider is
// never needed - the record states one thread.
var LAT = 0.0025;

// Why the record cannot be a LinuxCNC G76 (null: it can). r0 / r1 are
// the start (drive line) and end radii on the meridian.
function g76Why(p, r0, r1) {
  var nums = [p.Lead, p.ThreadHeight, p.FirstDepth, p.MinimumDepth, p.FinishAllowance,
    p.InfeedAngle, p.SpringPasses, p.PullOut, p.MajorDiameter, p.MinorDiameter, p.Z, p.EndZ];
  for (var i = 0; i < nums.length; i++) if (!has(nums[i])) return "the thread record is missing a number";
  if (!(p.Lead > 0)) return "no lead";
  if (!(p.ThreadHeight > 0)) return "no thread height";
  if (!(p.FirstDepth > 0 && p.FirstDepth <= p.ThreadHeight + EPS)) return "first-pass depth outside the thread height";
  if (Math.abs(p.MinimumDepth - Math.min(p.FirstDepth, p.ThreadHeight)) > EPS) {
    return "the passes are not equal rungs (minimum depth differs from the first) - only LinuxCNC's constant-depth degression (R1) states the record's ladder exactly";
  }
  if (Math.abs(p.InfeedAngle) > 1e-6) return "infeed angle " + p.InfeedAngle + " - a compound infeed (Q) is not posted on this control yet (radial only, Q0)";
  if (Math.abs(p.FinishAllowance) > EPS) return "finish allowance - LinuxCNC's G76 has no finish-pass allowance word";
  if (Math.abs(p.PullOut) > EPS) return "pull-out - an angled thread exit (E/L taper) is not posted on this control yet";
  if (!(p.SpringPasses >= 0 && Math.round(p.SpringPasses) === p.SpringPasses)) return "spring passes not a whole count";
  if (!(p.MinorDiameter < p.MajorDiameter)) return "not an external thread (minor diameter at or over the major)";
  if (Math.abs(p.MajorDiameter - p.MinorDiameter - 2 * p.ThreadHeight) > LAT) return "the diameters and the thread height disagree";
  if (!(r0 > 0 && r1 > 0)) return "the thread is not cut from the +X side of the axis";
  if (Math.abs(2 * r1 - p.MinorDiameter) > LAT) return "the end point does not stand on the minor diameter";
  if (!(r0 > p.MajorDiameter / 2 + EPS)) return "the drive line stands at or inside the crest";
  if (!(Math.abs(p.EndZ - p.Z) > EPS)) return "no thread length";
  // The hand (#629): right-hand toward the chuck (-Z) and left-hand away
  // from it turn clockwise seen from +Z - M3; the other two need M4.
  // G76 cuts from the drive line's Z toward its Z word, so the record's
  // own Z -> EndZ is the direction the control cuts.
  if (p.Hand !== "right" && p.Hand !== "left") return "the thread record states no hand";
  var rev = (p.Hand === "right") === (p.EndZ > p.Z);
  if (rev !== spinRev) {
    return "a " + p.Hand + "-hand thread cut " + (p.EndZ < p.Z ? "toward" : "away from") + " the chuck needs " + (rev ? "M4" : "M3") + ", the section's spindle runs " + (spinRev ? "M4" : "M3");
  }
  return null;
}

// The cycle's grid counts: X drive line, P peak, root; I J K from them.
// null with a reason when the grid cannot state the thread.
function g76Grid(p, r0) {
  var x = Math.round(gridOf(2 * r0));
  var peak = Math.round(gridOf(p.MajorDiameter));
  var root = Math.ceil(gridOf(p.MinorDiameter) - 1e-6);   // outward: never deeper than the minor
  var j = Math.floor(gridOf(2 * p.FirstDepth) + 1e-6);     // shallower: never a heavier first cut
  var k = peak - root;
  if (!(x - peak >= 1)) return { why: "the drive line is not clear of the crest at the control's resolution" };
  if (!(k >= 1)) return { why: "the thread depth is below the control's resolution" };
  if (!(j >= 1)) return { why: "the first-pass depth is below the control's resolution" };
  return { x: x, i: peak - x, j: Math.min(j, k), k: k };
}

// A THREAD record: with G76 chosen, a rapid to the drive line at the
// thread's start and the one G76 block, in PathPilot's conversational
// word order (P pitch, Z end; I peak offset, J first depth, K full depth
// - diameters under G7, negative I = external; R1 equal rungs; Q0
// radial; H spring passes); the passes inside are skipped. With G33
// (the default) nothing is written here; the record's lead is kept as
// the passes' K.
function onThreadCycle() {
  flushPass();
  threadSeen = true;
  var p = properties;
  threadLead = has(p.Lead) && p.Lead > 0 ? Number(p.Lead) : 0;
  if (!g76Wanted()) return;
  startPreamble();
  if (posed(p)) return;
  var r0 = radiusOf(p.X, p.Y, "thread cycle start");
  var r1 = radiusOf(p.EndX, p.EndY, "thread cycle end");
  if (r0 === null || r1 === null) return;
  var why = g76Why(p, r0, r1);
  var g = why ? null : g76Grid(p, r0);
  if (g && g.why) why = g.why;
  if (why) { refuse("G76 threading cycle refused - " + why); return; }
  block(["G0", "X" + gridWord(g.x), zWord(p.Z)]);
  block([
    "G76", "P" + fp(p.Lead), zWord(p.EndZ),
    "I" + gridWord(g.i),
    "J" + gridWord(g.j),
    "K" + gridWord(g.k),
    "R1",
    "Q0",
    "H" + String(Math.round(p.SpringPasses)),
  ]);
  inThread = true;
}
function onThreadCycleEnd() { flushPass(); inThread = false; threadSeen = false; threadLead = 0; }

// ------------------------------------------------------------- drilling
// No canned cycle on the PathPilot lathe: the CYCLE record is checked
// (on the axis, a kind plain moves can cut) and its dwell remembered;
// the hole's own expanded chain that follows posts as plain moves.
function onCycle() {
  flushPass();
  startPreamble();
  var p = properties;
  drillDwell = 0; drillFloor = null; drillIgnore = false;
  var r = radiusOf(p.X, p.Y, "cycle");
  if (r === null) { drillIgnore = true; return; }
  if (Math.abs(r) > EPS) { drillIgnore = true; refuse("drilling cycle off the spindle axis - a 2-axis lathe drills on centre only"); return; }
  if (p.CycleType === "BACKBORE") { drillIgnore = true; refuse("back-boring cycle - no such move on a 2-axis lathe"); return; }
  if (p.CycleType === "TAP") {
    drillIgnore = true;
    refuse(coolantWho() + "tapping - the PathPilot lathe post does not write a rigid tap (G33.1) yet, and plain moves cannot reverse the spindle");
    return;
  }
  drillDwell = has(p.P) && p.P > 0 ? Number(p.P) : 0;  // P is seconds
  drillFloor = has(p.Z) ? Number(p.Z) : null;
}

// ------------------------------------------------------------ footer
// G30 to the operator's preset safe position (unless a trailing Go home
// just went there, #504), M30 ends and rewinds. No % fence.
function onClose() {
  endPreamble();
  if (linesWritten() !== homedAt) block(["G30"]);
  block(["M30"]);
}

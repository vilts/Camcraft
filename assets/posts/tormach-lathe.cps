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
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 9;      // LinuxCNC G-code reference: G54-G59.3 select coordinate systems 1-9 (#931)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
// Explain G-code (#1399): T0101 is tool 1, offset 1 (the PathPilot lathe
// T word above), never "tool 101". Read only when Explain is on.
explainTool = "station-offset";
explains = "paren"; // #1389: PathPilot comments and LinuxCNC extended coordinate systems
explainWords = {
  "G59.1": "WORK OFFSET 7", "G59.2": "WORK OFFSET 8", "G59.3": "WORK OFFSET 9"
};
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
//  - Units from the document (#902): G20 (every length / 25.4, four decimals,
//    feeds per rev at five, pitches at six) or G21 (mm, three decimals,
//    feeds per rev at four, pitches at six). Angles and rpm never scale.
//  - G95 feed per revolution for every cut: F = mm/min / rpm (/ 25.4 on
//    an inch program), so the cut speed the generator planned is the
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
//    infeed, H its spring passes; a finishing step only when it is the
//    equal ladder's own last one (G76 has no word for any other, #417),
//    no alternating flanks, no repeat of each roughing depth (#418),
//    no pull-out (no E/L taper), an external thread cut from
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
//  - Around every G30 (#1604): the post never knows where the operator
//    stored G30, so no two-axis rapid touches it. After G30 the first
//    point is reached one axis at a time - X first, then Z, when the
//    point stands outside the stock's diameter; Z first (to the point,
//    or to the section's ClearancePlane when the point is on the face),
//    then X, when it stands in front of the face. Before G30 the tool
//    leaves +X to the stock's diameter, then +Z to the face (or to the
//    ClearancePlane), so the control's own G30 move starts from the
//    stock's outer corner. The stock is the section's StockTurnRadius
//    (the radius it sweeps about the axis, #1666; never less than the
//    StockXY* box's largest half-width) and StockZTop (the face).
//    No stock, or no stated plane where one is needed: refused by name.
//  - No % fences and no O word (LinuxCNC reads O as a subroutine label);
//    comments are parenthesized and cleaned of parentheses - LinuxCNC
//    has no nested comments.

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
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
var inch = false;          // the document's unit is inch (onOpen, #902: postOptions.Units, never the machine file's)
var pass = null;           // the G33 pass being merged: { r, z0, z, halves, lead, dir }
var drillDwell = 0;        // the open hole's floor dwell, seconds (0 = none)
var drillFloor = null;     // ... and its floor Z (world mm)
var drillIgnore = false;   // the open hole was refused: its chain posts nothing
var stk = null;            // #1604: the section's stock box (world mm) { l, r, f, b, top, turn }, null = none stated
var clearZ = null;         // #1604: the section's ClearancePlane (world Z), null = none stated
var fromG30 = false;       // #1604: the tool stands at G30 - the next point is approached axis by axis
var lastR = null;          // #1604: the last written point: its radius (origin-relative) ...
var lastZ = null;          // ... and its world Z; null = unknown (at G30, or nothing written yet)
var departWords = null;    // #1604: the moves that leave the stock before the next G30

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function trim0(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the program's unit
  if (!has(v)) return "0";
  return trim0(Number(len(v)).toFixed(inch ? 4 : 3));
}
function fr(v) {  // a feed per revolution (mm/rev in), in the program's unit
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
  if (a.length > 0) {
    var line = a.join(" "), c = explain(a);
    if (c && line.length + 1 + c.length <= 80) line += " " + c;
    writeln(line);
  }
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
var COOLANT_KINDS = ["flood", "mist", "air", "air-blast", "through-spindle", "through-tool"];
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
  var want = ask === "mist" ? ["mist"] : (ask === "air" ? ["air", "air-blast"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []));
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
function onComment(s) { flushPass(); note(commentText(s)); }
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
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
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

// #931: PARAM WorkOffset arrives as the 1-based offset number (the engine
// refuses one above workOffsetMax by setup name); the word is ours to write.
function workOffsetName(n) {  // offset n -> this control's word; "" when it has none
  if (!(n >= 1 && n <= 9 && n === Math.floor(n))) return "";
  return n <= 6 ? "G" + (53 + n) : "G59." + (n - 6);  // LinuxCNC: G59.1..G59.3 are systems 7-9
}
function offsetWord(value) {
  var n = Number(value), w = workOffsetName(n);
  if (!w) { msg("work offset " + value + " - this post expresses work offsets 1-" + workOffsetMax + " only"); return wcsWord; }
  wcsIndex = n;
  return w;
}

function onParameter(name, value) {
  flushPass();
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "SurfaceSpeed" && has(value)) sCss = Number(value);
  else if (name === "MaxSpindleSpeed" && has(value)) sMax = Number(value);
  else if (name === "SpindleDirection") spinRev = String(value || "") === "reverse";
  else if (name === "ClearancePlane" && has(value)) clearZ = Number(value);
  else stockParam(name, value);
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
  if (toolWait) leave();     // #1604: out of the last section's stock before its G30
  opNotes();
  block([wcsWord]);
  if (toolWait) {
    toolNote();
    g30();                   // the operator's preset safe spot (#1604: left axis by axis), then the tool
    block([tWord()]);        // T0101: tool 1, geometry 1, wear line 1
    toolWait = false;
  }
  departWords = null;        // no G30 here: the section goes on from where the last one ended
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
    g30();
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
  departWords = departure(); // #1604: computed in this section's frame, written before a G30
  stk = null;                // and its own stock and planes (#1604)
  clearZ = null;
  afterOp();
}

function onSection() { flushPass(); startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ---------------------------------------------------- G30 (#1604)
// LinuxCNC's G30 with no axis words moves every axis to the stored
// position (params 5181-5189) in one straight line, and the operator
// stores it - so the post cannot know if the line from or to it
// crosses the stock. The law: the moves after a G30 and before one are
// single-axis, and they run only where the stock is not - outside its
// diameter, or in front of its face. Then the control's own G30 line
// starts at the stock's outer corner, from which a straight line to
// any point outside the stock's envelope stays outside it.
function stockParam(name, value) {
  var keys = { StockXYLeft: "l", StockXYRight: "r", StockXYFront: "f", StockXYBack: "b", StockZTop: "top", StockTurnRadius: "turn" };
  var k = keys[name];
  if (!k || !has(value)) return;
  if (stk === null) stk = {};
  stk[k] = Number(value);
}
// The stock about the spindle axis: { r: its radius, top: its face Z }
// in world mm, null when the section states no stock box. The radius
// is the one the stock sweeps about the axis, StockTurnRadius (#1666: a
// square bar's corners stand out to half its diagonal), and never less
// than the box's largest half-width from the origin - a round bar's own,
// and all a stream that states no turning radius gives.
function stockEnv() {
  var s = stk;
  if (s === null || !has(s.l) || !has(s.r) || !has(s.f) || !has(s.b) || !has(s.top)) return null;
  if (!(s.r - s.l > EPS) || !(s.b - s.f > EPS)) return null;   // an empty box: no stock
  var r = Math.max(Math.abs(s.l - orgX), Math.abs(s.r - orgX), Math.abs(s.f - orgY), Math.abs(s.b - orgY));
  if (has(s.turn) && isFinite(s.turn) && s.turn > r) r = s.turn;
  return { r: r, top: s.top };
}
function clearOfFace(env) { return has(clearZ) && clearZ > env.top + EPS ? clearZ : null; }
var NO_STOCK = "no stock stated - the post orders the moves before and after G30 by the stock's diameter and face; define the setup's stock";
// The departure goes out in the frame it was computed in: before the
// next section's notes and work offset word.
function leave() {
  var w = departWords || [];
  for (var i = 0; i < w.length; i++) writeln(w[i]);
  departWords = null;
}
function g30() {
  leave();
  block(["G30"]);
  fromG30 = true;
  lastR = null; lastZ = null;
}
function at(r, z) { lastR = r; if (has(z)) lastZ = Number(z); }
// The approach from G30 to the section's first point (r, z): X first
// when the point stands outside the stock's diameter (the Z move then
// runs outside it), else Z first when it stands in front of the face
// (the X move then runs in front of it) - to the point's own Z, or, for
// a point ON the face, to the ClearancePlane first. xw is the point's X
// word (the G76 drive line rides its own grid). False when refused: the
// caller writes nothing, and no later move until a point is reached.
function approach(r, z, xw) {
  var who = coolantWho() + "the first point after G30";
  if (!has(z)) { refuse(who + " states no Z - the post will not approach it from an unknown G30"); return false; }
  var env = stockEnv();
  if (env === null) { refuse(coolantWho() + NO_STOCK); return false; }
  if (r > env.r + EPS) {
    block(["G0", xw]);
    block(["G0", zWord(z)]);
  } else {
    var zc = z > env.top + EPS ? z : clearOfFace(env);
    if (zc === null) {
      refuse(who + " stands inside the stock's diameter (" + xWord(env.r) + ") and not in front of its face (" + zWord(env.top)
        + "), and no ClearancePlane in front of the face is stated - the post will not approach it from G30 across the stock");
      return false;
    }
    block(["G0", zWord(zc)]);
    block(["G0", xw]);
    if (Math.abs(zc - z) > EPS) block(["G0", zWord(z)]);
  }
  fromG30 = false;
  at(r, z);
  return true;
}
// The way out before a G30, from the section's last point: +X to the
// stock's diameter, then +Z to the face - the ClearancePlane when the
// corner reached is not clear of the stock in either axis. null when
// nothing was written (the tool is where G30 left it); [] when refused.
function departure() {
  if (lastR === null || lastZ === null) return null;
  var env = stockEnv();
  if (env === null) { refuse(coolantWho() + NO_STOCK); return []; }
  var radial = lastR > env.r - EPS, front = lastZ > env.top - EPS;
  if (!radial && !front) {
    refuse(coolantWho() + "the section ends inside the stock's envelope (" + xWord(lastR) + " " + zWord(lastZ)
      + ") - the post cannot tell which way leaves the stock before G30; end the operation outside the stock");
    return [];
  }
  var xd = Math.max(lastR, env.r), zd = Math.max(lastZ, env.top);
  if (!(xd > env.r + EPS) && !(zd > env.top + EPS)) {
    zd = clearOfFace(env);
    if (zd === null) {
      refuse(coolantWho() + "the section ends on the stock's corner and no ClearancePlane in front of the face is stated - the post will not leave for G30 across the stock");
      return [];
    }
  }
  var w = [];
  if (xd > lastR + EPS) w.push("G0 " + xWord(xd));
  if (zd > lastZ + EPS) w.push("G0 " + zWord(zd));
  return w;
}
// A feed or an arc cannot be the first motion after G30: where it
// starts is the stored G30, unknown.
function fedFromG30(what) {
  if (!fromG30) return false;
  refuse(coolantWho() + what + " is the first motion after G30 - the post approaches a section's start with rapids only; regenerate the operation");
  return true;
}

// ------------------------------------------------------------- motions
function onRapid() {
  flushPass();
  startPreamble();
  var p = properties;
  if (posed(p) || inThread || drillIgnore) return;
  var r = radiusOf(p.X, p.Y, "rapid");
  if (r === null) return;
  if (fromG30) { approach(r, p.Z, xWord(r)); return; }
  block(["G0", xWord(r), zWord(p.Z)]);
  at(r, p.Z);
}
function onLinear() {
  flushPass();
  startPreamble();
  var p = properties;
  if (posed(p) || inThread || drillIgnore) return;
  var r = radiusOf(p.X, p.Y, "feed move");
  if (r === null) return;
  if (fedFromG30("a feed move")) return;
  block(["G1", xWord(r), zWord(p.Z), feedWord(p.FeedRate)]);
  at(r, p.Z);
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
  if (fedFromG30("an arc")) return;  // #1604
  // #3442: an arc whose written ends coincide is no arc block (arcFate,
  // PRELUDE), on this post's own X (radius) and Z words; the stream's
  // direction seen in the (Z, radius) frame is the block's own.
  var cwB = p.CircularDirection === "CW";
  if (yz) cwB = !cwB;
  var fate = arcFateOf({
    plane: "XZ", ccw: !cwB, rotary: false,
    start: { X: r0, Y: 0, Z: p.StartZ }, end: { X: r1, Y: 0, Z: p.Z }, center: { X: rc, Y: 0, Z: c.K }
  }, function (k, v) { return k === "X" ? xWord(v) : (k === "Z" ? zWord(v) : ""); },
  // #3474: the centre the control reads: the written start plus the
  // written I (a radius, doubled to the diameter X scale) and K.
  function (k, d) { return (k === "X" ? 2 : 1) * _arcWordNum(fu(d)); });
  if (fate !== "arc") {
    // #3474: a short arc whose written ends differ on one ray: the feed move.
    if (fate === "linear") block(["G1", xWord(r1), zWord(p.Z), feedWord(p.FeedRate)]);
    at(r1, p.Z);
    return;
  }
  var cw = p.CircularDirection === "CW";
  if (yz) cw = !cw;                       // the +Y meridian seen from +Y is mirrored
  block([
    cw ? "G2" : "G3",
    xWord(r1), zWord(p.Z),
    "I" + fu(rc - r0), "K" + fu(c.K - p.StartZ),
    feedWord(p.FeedRate),
  ]);
  at(r1, p.Z);
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
  if (fedFromG30("a thread half-turn")) { flushPass(); return; }  // #1604
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
  at(q.r, q.z);
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
// form); J, the first depth, is rounded shallower - unless the
// shallower J would make the control cut one pass more than the record
// lays (#1619, below).
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
  if (p.Alternating === true) return "alternating flank infeed - LinuxCNC's G76 infeeds along one line (Q) only";
  // #418: the record's passes cut every roughing depth twice; the
  // cycle cuts each depth once, so it would not run the verified passes.
  if (p.RepeatEachDepth === true) return "repeat each roughing depth - LinuxCNC's G76 cuts each depth once; choose G33 passes, or turn the repeat off";
  // The record's finishing pass cuts FinishAllowance below the last
  // roughing pass (#417). R1's equal ladder (J, 2J, ... clamped at K)
  // has no word for that step: its finishing pass takes the ladder's
  // own last step, so the record must state exactly that one.
  var rungs = Math.max(1, Math.ceil(p.ThreadHeight / p.FirstDepth - 1e-9));
  var lastStep = p.ThreadHeight - (rungs - 1) * p.FirstDepth;
  if (Math.abs(p.FinishAllowance - lastStep) > 2 * EPS) {
    return "finish allowance " + p.FinishAllowance + " - LinuxCNC's G76 has no finish-pass word; its finishing pass takes the equal ladder's own last step (" + trim0(lastStep.toFixed(4)) + "); regenerate the threading operation to refresh cached thread records";
  }
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
  // #2952 (the law of #2878): the public LinuxCNC G-code reference's
  // G76 section says that, with no entry taper (this post writes no E
  // / L), the tool rapids to the cut depth at the start Z, then
  // synchronizes and cuts. If the start point stands above
  // material (not in free air in front of the end face or in a relief
  // groove wider than the insert), refuse the cycle by name and never
  // write it. A record without the value (an older cache) reads false.
  if (p.StartInAir !== true) {
    return "the start point is above material (G76 moves the insert to pass depth at rapid traverse) - start the thread in free air in front of the end face or in a relief groove wider than the insert, regenerate the operation, or choose G33 passes";
  }
  return null;
}

// The cycle's grid counts: X drive line, P peak, root; I J K from them.
// null with a reason when the grid cannot state the thread.
function g76Grid(p, r0) {
  var x = Math.round(gridOf(2 * r0));
  var peak = Math.round(gridOf(p.MajorDiameter));
  var root = Math.ceil(gridOf(p.MinorDiameter) - 1e-6);   // outward: never deeper than the minor
  var jRec = gridOf(2 * p.FirstDepth);                     // the record's first depth, real
  var k = peak - root;
  if (!(x - peak >= 1)) return { why: "the drive line is not clear of the crest at the control's resolution" };
  if (!(k >= 1)) return { why: "the thread depth is below the control's resolution" };
  // #1619: R1 (LinuxCNC: "R1.0 selects constant depth") cuts J, 2J, ...
  // and clamps the last pass at K, so the control cuts ceil(K/J)
  // passes. The record lays ceil(height / first depth) equal rungs
  // (g76Why holds the equal ladder); J must make the control's count
  // that same number: (n-1)J < K <= nJ. J is the grid's shallower
  // rounding when that keeps the count; when it would add a pass, the
  // one grid step up - never a full step heavier than the record's
  // first depth. Anything else is refused by name.
  var n = Math.max(1, Math.ceil(p.ThreadHeight / p.FirstDepth - 1e-9));
  var lo = Math.ceil(k / n);                                // nJ >= K
  var hi = n > 1 ? Math.floor((k - 1) / (n - 1)) : k;       // (n-1)J < K
  var j = Math.min(Math.floor(jRec + 1e-6), hi);            // shallower: the record's first cut or less
  if (j < lo) j = lo;                                       // shallower adds a pass: one step up
  if (!(j >= 1)) return { why: "the first-pass depth is below the control's resolution" };
  if (!(j <= hi && j - jRec < 1 - 1e-6)) {
    return { why: "no first depth J on the control's grid makes R1's equal ladder cut the record's " + n + " passes (K " + gridWord(k) + ")" };
  }
  return { x: x, i: peak - x, j: j, k: k };
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
  if (fromG30) { if (!approach(r0, p.Z, "X" + gridWord(g.x))) return; }  // #1604
  else block(["G0", "X" + gridWord(g.x), zWord(p.Z)]);
  at(r0, p.Z);                       // G76 ends back at its drive line
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
  if (linesWritten() !== homedAt) g30();
  block(["M30"]);
}

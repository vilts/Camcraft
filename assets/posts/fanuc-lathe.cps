// Fanuc 2-axis LATHE post - the first turning .cps we ship (M13.5).
// Provenance: the G-dialect from public RS-274 / ISO 6983 documentation
// and the publicly documented Fanuc 0i-T style vocabulary (G18 ZX
// plane, G96/G97/G50 spindle words, G99 feed per revolution, the G83/
// G84/G85 face cycles, G32 thread cutting, the two-block G76 multi-pass
// threading cycle, T0101-style offsets) - behavior
// only, per this repo's Clean-Room Protocol; no vendor post text. The hook
// contract is the one assets/posts/AGENTS.md states. Original work.
description = "Fanuc lathe (2-axis turning, G18 ZX, diameter X, G97/G99, G83 face drilling, G32 or G76 threading)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 2; // reference contract: integer feature mask (bit 2 = turning)
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it
liveTooling = true;  // voices PARAM LiveTool sections (issue #33): the engine refuses them for a post without it
threadHands = true;  // reads PARAM SpindleDirection (#629): the engine refuses a reverse-spindle thread for a post without it

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).
//
// The lathe laws this script states, so a reviewer need not infer them:
//  - DIAMETER programming: the X word is twice the stream's radius. The
//    stream speaks world mm on ONE meridian half-plane of the spindle
//    axis (the turning kernel's frame); the radius is that plane's
//    signed in-plane coordinate, origin-relative.
//  - G40 throughout, and the stream's point IS the control point (#630):
//    for a nose-radiused insert the app has already moved the kernel's
//    nose-centre path to the tool's imaginary tip for its orientation
//    (PARAM ToolOrientation, LinuxCNC's 1-9 chart) - the point the
//    operator touches the tool off on. A grooving/parting blade's
//    stream is likewise already its touched-off CORNER (#641: the
//    orientation names the corner - 1 right/+Z, 2 left/-Z, 6 the width
//    centre). This script never shifts a point, and writes the
//    orientation in the tool note so the setup can be checked against
//    it.
//    A move with BOTH in-plane coordinates off the axis is off every
//    meridian - milling work - and is refused by name, never projected.
//  - G97 constant rpm, from the stream's SpindleSpeed; G96 constant
//    surface speed ONLY when the stream states a SurfaceSpeed (m/min)
//    PARAM - the post never invents a surface speed from an rpm. G50
//    clamps the rpm once per program to the machine's stated maximum,
//    and again before a G96 section whose tool states a lower max rpm
//    (PARAM MaxSpindleSpeed, #891). A G96 with neither clamp stated is
//    refused by name - unclamped CSS runs away toward the centre.
//    A section that posted G96 cancels it at its end (G97 after M05,
//    #1173), so every section opens under G97 unless it states G96
//    itself: a live tool's, a drill's or a thread's S is read as rpm,
//    whatever kind of section follows, and the program never ends -
//    or hands the next program - a constant-surface-speed mode.
//  - G99 feed per revolution: F = mm/min / rpm, so the cut speed the
//    generator planned is the cut speed the control runs. No rpm, no
//    per-rev feed - refused by name.
//  - G18: every arc is a ZX-plane arc with I K centre increments in
//    RADIUS values (the 0i-T convention). A meridian arc that rides the
//    kernel's +Y half-plane changes its handedness when it is viewed as
//    ZX from +Y, so a YZ-stated arc flips CW<->CCW; an XZ-stated one
//    does not. An XY arc is a thread half-turn (see G32) or a refusal.
//  - G32: the threading kernel spells a synchronized helix as XY
//    half-turn arcs about the axis, half a pitch of Z each (toward the
//    chuck, -Z, by default). Two half-turns at one radius are one lead
//    of Z travel under G32 with F = lead (mm/rev). Anything else in XY
//    is refused.
//  - The thread's HAND (#629) is never dropped. The stream's +Z is the
//    free end, the chuck below. A half-turn clockwise seen from +Z is
//    the tool's motion about a part turning M03 (clockwise seen from
//    the headstock); counter-clockwise needs M04. So a right-hand thread
//    cut toward the chuck posts under M03 (the standard), a left-hand
//    one toward the chuck under M04 - the stream says so beforehand with
//    PARAM SpindleDirection reverse, and the section's spindle starts
//    that way. A half-turn turning against the spindle the section
//    started is refused by name (it would cut the other hand), and so
//    is a G76 record whose hand and Z direction disagree with it.
//  - G76 (user option postOptions.Threading = "G76"; G32 is the
//    default): the stream's THREAD record becomes the two-block cycle
//      G76 P(m)(r)(a) Q(dmin) R(d)
//      G76 X(root dia) Z(end) P(k) Q(first depth) F(lead)
//    m = finishing passes (1 + the record's spring passes), r = pull-out
//    in tenths of the lead, a = infeed angle (0 radial, 29 30 55 60 80),
//    dmin / k / first depth radius values in whole microns, d the finish
//    allowance in mm; the explicit passes inside the record are skipped.
//    A straight external thread only; a record out of range, or thread
//    passes with no record, is refused by name - never posted as G32.
//  - Face cycles: G83 (Q peck in whole microns, the 0i-T word; P dwell
//    in ms), G84 tap, G85 bore, cancelled by G80. A cycle off the
//    spindle axis and a back-bore are refused - a 2-axis lathe has no
//    such cycle. The cycle replaces only the chain on its hole (on
//    centre, at or below R); the first rapid leaving it - the exit to
//    clearance - ends the group with G80 and posts as the stream says
//    (#1011, the hop law of #994), so the tool leaves the hole at the
//    verified exit level, never at R. A cycle read with the tool off
//    its seat (on centre at R) is seated by the same law (#1026): Z out
//    to R first when below it, then X onto the centre, then down -
//    never one diagonal X+Z rapid; one the post cannot seat (its
//    section's first motion, no R, R not above the bottom) is refused.
//  - M03 unless the stream states PARAM SpindleDirection reverse (#629,
//    a thread whose hand needs it): then M04, with a note for the
//    operator that the threading insert must be mounted for reverse
//    rotation. Nothing else in the stream turns the spindle backwards.
//  - C-axis live tooling (issue #33): a section carrying PARAM LiveTool
//    is cut by a radial live tool with the spindle positioned as C.
//    The stream's X Y Z stay the tip in the PART frame and C is the
//    stated angle Rz(C) that turns that tip onto the station (+X half-
//    plane): the post rotates the tip by C, refuses it if it does not
//    land there (C and XY disagree), and writes X = 2r, Z, C. The
//    engage / release / live-spindle words are the machine's own
//    (tool_station.live_tool) - builder-specific, never guessed: absent,
//    the section is refused by name. Feed per minute (G98) in the
//    section, G99 restored after; F is scaled so the control's
//    composite rate - X radius and Z in mm, C in degrees counted as mm,
//    the rotary feed law of Fanuc-style controls - moves the tip at the
//    planned mm/min along the part-frame chord - capped so the X/Z part
//    of the move stays within the machine's stated feed and the C part
//    within the C rate it states (machine.rotaries[].speeds, deg/min;
//    unstated, C is not capped). A machine that states no C rate has
//    nothing to cap a C-turning rapid by - whether it states a C travel
//    (so its path may carry rewind orbits, #33) or not: that rapid is
//    refused by name, never posted at an uncapped feed (#906, #966).
//    A rapid is a G01 at the machine's stated maximum feed, never a
//    G00: a G00 with C runs each axis on its own and follows no
//    verified path. Arcs and cycles in a C section are refused; the
//    first move must be a rapid (C stands there before X and Z move).

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var sRpm = 0;              // S/M3 waits for the section (PARAMs, TOOL, SECTION order)
var sCss = 0;              // SurfaceSpeed PARAM (m/min), 0 = none stated
var sMax = 0;              // MaxSpindleSpeed PARAM (#891): the tool's own rpm ceiling, 0 = none
var clampS = 0;            // the G50 S in force (#891), 0 = none posted yet
var cssOn = false;         // this section posted G96: its end cancels it (#1173)
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R level (world, #1011)
var seatKnown = false;     // #1026: this section moved, so currentPosition is where the tool is
var sectionOpen = false;
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (world -> program)
var clampDone = false;     // G50 once per program
var inThread = false;      // a G76 cycle stands for the passes being skipped
var threadSeen = false;    // inside a THREAD record's span (cycle or not)
var spinRev = false;       // PARAM SpindleDirection reverse (#629): this section's spindle runs M04
var refused = {};          // each refusal counted once per program, by name
var EPS = 0.0005;          // half a micron: the stream's own lattice
var liveAsk = false;       // the section's PARAM LiveTool (issue #33)
var liveOk = false;        // the C section engaged (codes stated, C on the machine)
var liveCodes = null;      // the machine's live_tool words for the open section
var lastC = null;          // the C a C section last commanded
var lastR = 0, lastZ = 0;  // ... and the station radius / Z it stood at
var liveCRate = 0;         // the machine's stated C rate (deg/min), 0 = unstated
var liveCTravel = false;   // the machine states a C travel (#906)
var C_TOL = 0.005;         // mm a C-turned tip may stand off the station (lattice + milli-degree words)

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fr(v) {  // mm/rev at four decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(4);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function um(v) { return String(Math.round(Number(v) * 1000)); } // whole microns, no point
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else writeln("; " + s); }
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
function xWord(r) { return has(r) ? "X" + fu(2 * r) : ""; }         // DIAMETER programming
function zWord(z) { return has(z) ? "Z" + fu(z - orgZ) : ""; }
// G99: feed per revolution from the stream's mm/min and the spindle rpm.
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

// The .mch tool-station coolant list carries names + M-codes. A
// flood ask takes the machine's Flood entry; failing one, the first
// coded entry of no other known family (a shop's own "Brine" is its
// flood); a stated codeless Flood is the operator's own valve, silent.
// A list that states only other kinds (Mist, Through-spindle) has no
// flood code: refused by name (#150), never posted as mist or dry. An
// unstated list stays silent (the unstated-limit law). M9 (stop) is
// the RS-274 fixed pair.
var COOLANT_KINDS = ["flood", "mist", "through-spindle", "through-tool"];
function coolantWho() { return coolantOp ? "operation " + coolantOp + ": " : ""; }
function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var other = "", floodNamed = false;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (name === "flood") { if (code) return code; floodNamed = true; continue; }
    if (code && !other && COOLANT_KINDS.indexOf(name) < 0) other = code;
  }
  if (other || floodNamed) return other;
  if (cs.length > 0) msg(coolantWho() + "the machine states no flood coolant code - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "";
}

// The op's coolant ask (#112): the stream says nothing for flood (every
// older stream), or carries PARAM Coolant off / mist / through-tool. The
// code is the machine's own entry of that family - the machine is truth;
// an ask the machine states no code for is refused by name, naming the
// operation (#150), never silently posted dry.
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []);
  if (want.length === 0) { msg(coolantWho() + "unknown coolant ask " + ask); return ""; }
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (code && want.indexOf(name) >= 0) return code;
  }
  msg(coolantWho() + "the machine states no " + ask + " coolant code - pick another coolant for the operation or add it to the machine");
  return "";
}

// -------------------------------------------------------------- header
// The program's O-word (#111): O0001 for the whole job, and one file
// per operation counts up (the engine's programNumber); a host that
// publishes none posts O0001.
function oWord() {
  var n = typeof programNumber !== "undefined" ? Math.round(Number(programNumber) || 1) : 1;
  var t = String(Math.max(1, Math.min(n, 9999)));
  while (t.length < 4) t = "0" + t;
  return "O" + t;
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius / ToolOrientation (#630) and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: a parenthesis would end the comment
// early, % is the tape's end mark and ; opens a comment on some
// controls, so all three become spaces; printable ASCII only, capped at
// 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0, toolQ = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {  // the non-empty parts, " - " between
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
function opNotes() {  // at the section's preamble: the setup once, the op each time
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
// A lathe blade's library diameter is no size the operator measures:
// its note names the kind alone (rotating tools keep their D).
var BLADE_KINDS = ["TURNING INSERT", "GROOVING BLADE", "THREADING INSERT", "FORM GAUGE", "BORING BAR"];
function toolNote() {  // just before the tool change it describes
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0 && BLADE_KINDS.indexOf(toolKind) < 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  if (toolQ > 0) desc.push("ORIENTATION " + toolQ);  // #630: the tip the program's points are for
  note(joinDash(["T" + pad2(tNo) + pad2(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0; toolQ = 0;
}

function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  writeln("%");
  // #594: the name is user text from the machine file - cleaned like
  // every other comment text, so `Tormach 770MX (ML)` never nests.
  block([oWord(), "(" + (clean(nm) || "CAMCRAFT") + ")"]);
  block(["G21", "G18", "G40", "G80", "G99"]);   // mm, ZX plane, no nose comp, feed/rev
}

// Cutter compensation in the control (M13.11): on a lathe G41/G42 is
// tool NOSE-RADIUS compensation (the insert's ground tip, G41/G42 with
// the tip-direction number) - a different law, out of scope; refused by
// name, never a centre path posted as the part.
function onRadiusCompensation() {
  msg("control cutter compensation on a lathe is nose-radius compensation - not supported; choose In computer");
}

function onParameter(name, value) {
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "LiveTool") { liveAsk = true; return; }
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

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
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
// legal program: offset, tool + wear offset, spindle, coolant.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block([wcsWord]);
  if (toolWait) {
    toolNote();
    block(["T" + pad2(tNo) + pad2(tNo)]);     // T0101: station 1, offset 1
    toolWait = false;
  }
  if (!clampDone && typeof machine !== "undefined" && machine.spindleMax > 0) {
    block(["G50", "S" + ff(machine.spindleMax)]);
    clampS = machine.spindleMax;
    clampDone = true;
  }
  if (liveAsk) {
    liveStart();
  } else if (spindleWait) {
    var m = spinRev ? "M04" : "M03";
    if (spinRev) note("SPINDLE M04 - THREAD HAND NEEDS REVERSE ROTATION - MOUNT THE INSERT FOR M04");
    if (sCss > 0) {
      var cap = cssCap();
      if (!(cap > 0)) refuse(UNCLAMPED);
      else {
        if (cap !== clampS) { block(["G50", "S" + ff(cap)]); clampS = cap; }   // a tool rated under the machine (#891)
        block(["G96", "S" + ff(sCss), m]);
        cssOn = true;
      }
    } else {
      // A tool's lower clamp is lifted back to the machine's before a
      // constant-rpm section, so the program's S means what it says.
      // With no machine maximum there is nothing to lift back to, and
      // the tool's G50 stays in force into the G97 section - harmless,
      // since G50 S caps the spindle only under G96 (#891, pinned by
      // lathe_post_css's no-machine-max carry-over test).
      if (clampS > 0 && typeof machine !== "undefined" && machine.spindleMax > 0 && clampS !== machine.spindleMax) {
        block(["G50", "S" + ff(machine.spindleMax)]);
        clampS = machine.spindleMax;
      }
      block(["G97", "S" + ff(sRpm), m]);
    }
    spindleWait = false;
  }
  coolant = coolantFor(coolantAsk);
  if (coolant) block([coolant]);
}

// The op's after-action (#110), written once its spindle has stopped:
// the stream says nothing to carry on, or PARAM AfterOperation
// optional-stop / stop / home. The ask resets here, so the next
// section states its own.
function afterOp() {
  var ask = afterAsk;
  afterAsk = "";
  if (!ask) return;
  if (ask === "optional-stop") { block(["M01"]); return; }
  if (ask === "stop") { block(["M00"]); return; }
  if (ask === "home") {
    block(["G28", "U0"]);         // X clear of the part first, then Z
    block(["G28", "W0"]);
    homedAt = linesWritten();
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  sCss = 0;                  // and its own spindle mode: no stale G96 (#772)
  sMax = 0;                  // and its own clamp (#891)
  if (liveAsk) {
    // The live tool and the C axis are released before the op's stop or
    // trip home (#1188): the operator never meets a driven tool still
    // turning, and the home move never drags an engaged C.
    if (liveOk) {
      block([liveCodes.spindleOff]);
      block([liveCodes.cAxisOff]);
      block(["G99"]);
    }
    liveAsk = false; liveOk = false; liveCodes = null; lastC = null;
    spindleWait = false;
  } else {
    block(["M05"]);
    if (cssOn) block(["G97"]); // #1173: no section opens under a G96 it did not state
    cssOn = false;
    spinRev = false;           // the next section states its own
  }
  afterOp();                 // #1188: every section ends with its own ask, live or not
}

// --------------------------------------------- C-axis live tooling (#33)
// Engage the section: the machine's words, or a refusal by name.
function liveStart() {
  spindleWait = false;
  var m = (typeof machine !== "undefined") ? machine : {};
  var lt = m.liveTool;
  var ok = lt && lt.cAxisOn && lt.cAxisOff && lt.spindleOn && lt.spindleOff;
  if (!ok) {
    refuse(coolantWho() + "C-axis live-tool work, but the machine states no live_tool words (c_axis_on, c_axis_off, spindle_on, spindle_off) in its tool station - builder-specific codes are never guessed");
    return;
  }
  var rs = m.rotaries || [];
  var hasC = false;
  liveCRate = 0;
  liveCTravel = false;
  for (var i = 0; i < rs.length; i++) {
    if (String(rs[i].name || "").toUpperCase() !== "C") continue;
    hasC = true;
    // A G01 is held to the C's stated feed rate; failing that, to its
    // stated rapid. Neither stated: the C rate is unbounded by data.
    var sp = rs[i].speeds || {};
    var cr = Number(sp.normal) > 0 ? Number(sp.normal) : Number(sp.rapid);
    if (cr > 0) liveCRate = cr;
    if (rs[i].travel !== null && rs[i].travel !== undefined) liveCTravel = true;
  }
  if (rs.length > 0 && !hasC) {
    refuse(coolantWho() + "C-axis live-tool work, but the machine states no C axis among its rotaries");
    return;
  }
  if (!(sRpm > 0)) { refuse(coolantWho() + "C-axis live-tool work needs a live-tool speed - none stated"); return; }
  liveCodes = lt; liveOk = true; lastC = null;
  block(["G98"]);                               // feed per minute: no turning spindle to count revolutions
  block([lt.cAxisOn]);
  block([lt.spindleOn, "S" + ff(sRpm)]);
}
function trim0(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/[.]$/, "");
  return s === "-0" ? "0" : s;
}
function fc(v) { return trim0(Number(v).toFixed(3)); }  // degrees, three decimals
function fm(v) { return trim0(Number(v).toFixed(1)); }  // mm/min, one decimal
// One straight move of a C section: the tip turned by C onto the station.
function liveMove(p, rapid) {
  if (!liveOk) return;
  if ((has(p.A) && Math.abs(p.A) > 1e-6) || (has(p.B) && Math.abs(p.B) > 1e-6)) {
    refuse("A/B words in a C-axis section - the live tool's station is fixed");
    return;
  }
  if (!has(p.C)) { refuse("a move without a C word in a C-axis section - the spindle angle is unstated"); return; }
  var c = Number(p.C) * Math.PI / 180;
  var dx = p.X - orgX, dy = p.Y - orgY;
  var xr = dx * Math.cos(c) - dy * Math.sin(c);
  var yr = dx * Math.sin(c) + dy * Math.cos(c);
  if (Math.abs(yr) > C_TOL || xr < -C_TOL) {
    refuse("C move whose tip does not turn onto the live-tool station (C and X Y disagree) - refused, never projected");
    return;
  }
  var r = Math.max(xr, 0);
  if (lastC === null) {
    if (!rapid) { refuse("a C-axis section that opens with a cut - the first move must position C"); return; }
    block(["G00", "C" + fc(p.C)]);
    block(["G00", xWord(r), zWord(p.Z)]);
  } else {
    // The control's composite rate counts C degrees as mm; the planned
    // rate is along the part-frame chord. Scale F by their ratio.
    var dc = Number(p.C) - lastC, dr = r - lastR, dz = p.Z - lastZ;
    var lcmd = Math.sqrt(dr * dr + dz * dz + dc * dc);
    var sx = p.X - p.StartX, sy = p.Y - p.StartY, sz = p.Z - p.StartZ;
    var ltrue = Math.sqrt(sx * sx + sy * sy + sz * sz);
    if (lcmd <= EPS && ltrue <= EPS) return;   // nothing moves
    var f = rapid ? Number(machine.feedRateMax) : Number(p.FeedRate);
    if (!(f > 0)) {
      refuse(rapid ? "a C-axis rapid needs the machine's maximum feed - a G00 with C follows no verified path" : "a C-axis cut with no feed");
      return;
    }
    if (!(ltrue > EPS)) { refuse("a C move that turns the part with the tip on the spindle line"); return; }
    // #906, #966: a rapid's F is the machine's maximum linear feed,
    // which a turn of C would multiply past anything the file states.
    // No C rate stated - with a C travel (rewinds may ride here) or
    // without one (link rapids still turn C) - refuse by name.
    if (rapid && !(liveCRate > 0) && Math.abs(dc) > 1e-9) {
      refuse("a C-axis rapid that turns C, but the machine file states " + (liveCTravel ? "a C travel but no C rate" : "no C rate") + " - its feed would be uncapped; state the C axis's speeds");
      return;
    }
    // Under a composite F the control drives X/Z at F * lin / lcmd and
    // C at F * |dc| / lcmd: F is capped so neither outruns what the
    // machine states - the linear part by its fastest stated feed, C
    // by its stated rate (unstated: no cap the data can justify).
    var fc_ = f * lcmd / ltrue;
    var lin = Math.sqrt(dr * dr + dz * dz);
    var fmax = Number(machine.feedRateMax);
    if (fmax > 0 && lin > EPS) fc_ = Math.min(fc_, fmax * lcmd / lin);
    if (liveCRate > 0 && Math.abs(dc) > 1e-9) fc_ = Math.min(fc_, liveCRate * lcmd / Math.abs(dc));
    block(["G01", xWord(r), zWord(p.Z), "C" + fc(p.C), "F" + fm(fc_)]);
  }
  lastC = Number(p.C); lastR = r; lastZ = p.Z;
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); }

// A WARN record (only a gate-refused program ever carries one) is
// voiced as a parenthesized comment - the control's own comment
// syntax, never a bare text line it would reject.
function onComment(s) { writeComment(s); }

// ------------------------------------------------------------- motions
// While a face-cycle group is open, its expanded chain (plunge, pecks,
// retract to R) is the cycle block's business only. The chain stays on
// its hole: the hole's XY, at or below its R level. #1011: a rapid that
// leaves it (the exit up to clearance) ends the group - G80, then the
// rapid posted as the stream says - so the tool leaves the hole at the
// level the path was verified at, never at R. A cut leaving the hole is
// no chain a cycle replaces: refused by name.
function onHole(p) {
  return Math.abs(p.X - cycX) <= EPS && Math.abs(p.Y - cycY) <= EPS && p.Z <= cycR + EPS;
}
function chainRapid(p) {  // true: the open group's own move, swallowed
  if (!inCycle) return false;
  if (onHole(p)) return true;
  block(["G80"]);
  inCycle = false;
  return false;
}
function chainCut(p) {  // true: the cut is the cycle's (or refused)
  if (!inCycle) return false;
  if (!onHole(p)) refuse("a cutting move leaves its hole inside a face-cycle group");
  return true;
}
function onRapid() {
  startPreamble();
  var p = properties;
  seatKnown = true;
  if (liveAsk) { liveMove(p, true); return; }
  if (posed(p) || inThread || chainRapid(p)) return;
  var r = radiusOf(p.X, p.Y, "rapid");
  if (r === null) return;
  block(["G00", xWord(r), zWord(p.Z)]);
}
function onLinear() {
  startPreamble();
  var p = properties;
  seatKnown = true;
  if (liveAsk) { liveMove(p, false); return; }
  if (posed(p) || inThread || chainCut(p)) return;
  var r = radiusOf(p.X, p.Y, "feed move");
  if (r === null) return;
  block(["G01", xWord(r), zWord(p.Z), feedWord(p.FeedRate)]);
}

// A ZX-plane arc (G18): centre as I K increments from the start, in
// radius values; direction per the view-from-+Y law stated above.
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
    cw ? "G02" : "G03",
    xWord(r1), zWord(p.Z),
    "I" + fu(rc - r0), "K" + fu(c.K - p.StartZ),
    feedWord(p.FeedRate),
  ]);
}

// An XY arc is a thread half-turn: centre on the axis, one radius at
// both ends, antipodal ends - a synchronized half lead of Z. G32 with
// F = lead; the X word stays modal (the plunge put the tool there).
function threadHalfTurn(p) {
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J)) { refuse("thread half-turn without an I J centre"); return; }
  var ok = Math.abs(c.I - orgX) <= EPS && Math.abs(c.J - orgY) <= EPS;
  var rho0 = Math.sqrt(Math.pow(p.StartX - orgX, 2) + Math.pow(p.StartY - orgY, 2));
  var rho1 = Math.sqrt(Math.pow(p.X - orgX, 2) + Math.pow(p.Y - orgY, 2));
  ok = ok && Math.abs(rho0 - rho1) <= EPS;
  ok = ok && Math.abs(p.StartX + p.X - 2 * c.I) <= EPS && Math.abs(p.StartY + p.Y - 2 * c.J) <= EPS;
  if (!ok) { refuse("XY arc that is not a thread half-turn about the spindle axis"); return; }
  var lead = 2 * Math.abs(p.Z - p.StartZ);
  if (!(lead > 0)) { refuse("thread half-turn with no lead"); return; }
  if (g76Wanted() && !threadSeen) {
    refuse("G76 threading cycle chosen but the stream carries no thread cycle record for these passes - regenerate the thread, or choose G32 passes");
    return;
  }
  // The hand (#629): clockwise seen from +Z is M03's, counter-clockwise
  // M04's. A half-turn against the section's spindle would cut the
  // other hand - refused, never posted.
  var cw = p.CircularDirection === "CW";
  if (cw === spinRev) {
    refuse("thread half-turn turns " + (cw ? "clockwise" : "counter-clockwise") + " seen from +Z but the spindle runs " + (spinRev ? "M04" : "M03") + " - it would cut the other hand; regenerate the thread");
    return;
  }
  block(["G32", zWord(p.Z), "F" + fr(lead)]);
}

function onCircular() {
  startPreamble();
  var p = properties;
  seatKnown = true;
  if (liveAsk) { refuse("an arc in a C-axis section - only straight moves ride a C turn"); return; }
  if (posed(p) || inThread || chainCut(p)) return;
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") meridianArc(p);
  else threadHalfTurn(p);
}

// ---------------------------------------------------------- G76 cycle
function g76Wanted() { return typeof postOptions !== "undefined" && postOptions.Threading === "G76"; }

// Why the record cannot be a G76 (null: it can). r0 / r1 are the start
// and end radii on the meridian.
function g76Why(p, r0, r1) {
  var nums = [p.Lead, p.ThreadHeight, p.FirstDepth, p.MinimumDepth, p.FinishAllowance,
    p.InfeedAngle, p.SpringPasses, p.PullOut, p.MajorDiameter, p.MinorDiameter, p.Z, p.EndZ];
  for (var i = 0; i < nums.length; i++) if (!has(nums[i])) return "the thread record is missing a number";
  if (!(p.Lead > 0)) return "no lead";
  if (!(p.ThreadHeight > 0)) return "no thread height";
  if (!(p.FirstDepth > 0 && p.FirstDepth <= p.ThreadHeight + EPS)) return "first-pass depth outside the thread height";
  if (!(p.MinimumDepth > 0 && p.MinimumDepth <= p.ThreadHeight + EPS)) return "minimum depth outside the thread height";
  if (!(p.FinishAllowance >= 0 && p.FinishAllowance < p.ThreadHeight)) return "finish allowance outside the thread height";
  if ([0, 29, 30, 55, 60, 80].indexOf(Math.round(p.InfeedAngle)) < 0 || Math.abs(p.InfeedAngle - Math.round(p.InfeedAngle)) > 1e-6) {
    return "infeed angle " + p.InfeedAngle + " is not a G76 tool angle (0, 29, 30, 55, 60, 80)";
  }
  if (!(p.SpringPasses >= 0 && p.SpringPasses <= 98 && Math.round(p.SpringPasses) === p.SpringPasses)) return "spring passes outside 0..98";
  var r = Math.round(10 * p.PullOut / p.Lead);
  if (!(p.PullOut >= 0) || r > 99 || Math.abs(r * p.Lead / 10 - p.PullOut) > EPS) return "pull-out is not 0..9.9 leads in tenths";
  if (!(p.MinorDiameter < p.MajorDiameter)) return "not an external thread (minor diameter at or over the major)";
  if (!(r0 * r1 > 0)) return "start and end stand on different sides of the axis";
  if (Math.abs(2 * Math.abs(r1) - p.MinorDiameter) > 2 * EPS) return "the end point does not stand on the minor diameter";
  if (!(Math.abs(r0) > p.MajorDiameter / 2)) return "the start point stands inside the crest";
  if (!(Math.abs(p.EndZ - p.Z) > EPS)) return "no thread length";
  // The hand (#629): right-hand toward the chuck (-Z) and left-hand away
  // from it turn clockwise seen from +Z - M03; the other two need M04.
  if (p.Hand !== "right" && p.Hand !== "left") return "the thread record states no hand";
  var rev = (p.Hand === "right") === (p.EndZ > p.Z);
  if (rev !== spinRev) {
    return "a " + p.Hand + "-hand thread cut " + (p.EndZ < p.Z ? "toward" : "away from") + " the chuck needs " + (rev ? "M04" : "M03") + ", the section's spindle runs " + (spinRev ? "M04" : "M03");
  }
  return null;
}

// A THREAD record: with G76 chosen, one positioning rapid to the cycle's
// start point and the two G76 blocks; the passes inside the record are
// then skipped (the control cuts them). With G32 (the default) nothing
// is written here and the passes post as G32.
function onThreadCycle() {
  threadSeen = true;
  if (!g76Wanted()) return;
  startPreamble();
  var p = properties;
  if (posed(p)) return;
  var r0 = radiusOf(p.X, p.Y, "thread cycle start");
  var r1 = radiusOf(p.EndX, p.EndY, "thread cycle end");
  if (r0 === null || r1 === null) return;
  var why = g76Why(p, r0, r1);
  if (why) { refuse("G76 threading cycle refused - " + why); return; }
  var m = Math.round(p.SpringPasses) + 1;
  var r = Math.round(10 * p.PullOut / p.Lead);
  block(["G00", xWord(r0), zWord(p.Z)]);
  block(["G76", "P" + pad2(m) + pad2(r) + pad2(Math.round(p.InfeedAngle)), "Q" + um(p.MinimumDepth), "R" + fu(p.FinishAllowance)]);
  block(["G76", xWord(r1), zWord(p.EndZ), "P" + um(p.ThreadHeight), "Q" + um(p.FirstDepth), "F" + fr(p.Lead)]);
  inThread = true;
}
function onThreadCycleEnd() { inThread = false; threadSeen = false; }

// ------------------------------------------------------- canned cycles
// The face cycle is read with the tool seated on its R level on centre
// (the 0i-T cycle returns to the level it was read at, so that level
// must be R). The group's expanded chain on its hole is suppressed
// while the cycle is open; the rapid leaving the hole closes it
// (#1011, above).
// #1026, #994's cycleSeat on the lathe: a tool that stands off that
// seat is seated by the hop law - Z out to R first when it stands
// below R (nearer the face), then X onto the centre, then Z down to R
// when it stands above - never one diagonal X+Z rapid. "Away from the
// work" is +Z: the program's Z is the spindle axis with the face at
// the free end, and a face cycle drills toward -Z from R (its bottom
// below R), which the post checks. A cycle with no stated R, whose R
// is not above its bottom, or that is its section's first motion (no
// stream position to seat from) and stands off the seat cannot be
// seated: refused by name, never positioned by a guess.
function cycleSeat(p, r) {  // true: seated (blocks written as needed)
  var cp = (typeof currentPosition !== "undefined") ? currentPosition : {};
  var known = seatKnown && has(cp.X) && has(cp.Y) && has(cp.Z);
  var onAxis = known && Math.abs(cp.X - orgX) <= EPS && Math.abs(cp.Y - orgY) <= EPS;
  if (!has(p.R)) {
    if (onAxis) return true;  // a legacy record: the chain stands on centre
    refuse("a face cycle with no R level starts off the spindle axis - the post cannot seat it");
    return false;
  }
  var lo = known && cp.Z < p.R - EPS;
  var hi = known && cp.Z > p.R + EPS;
  if (onAxis && !lo && !hi) return true;  // already on its seat
  if (!known) {
    refuse("a face cycle is its section's first motion - the post cannot tell where the tool stands to seat it on R; approach the hole first");
    return false;
  }
  if (!(has(p.Z) && p.Z < p.R - EPS)) {
    refuse("a face cycle whose bottom is not below its R level - the post cannot tell which way is clear of the face");
    return false;
  }
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (lo) block(["G00", zWord(p.R)]);        // out of the work first
  if (!onAxis) block(["G00", xWord(r)]);     // onto the centre at the safe level
  if (hi) block(["G00", zWord(p.R)]);        // down to the seat
  return true;
}
function onCycle() {
  startPreamble();
  var p = properties;
  if (liveAsk) { refuse("a drilling cycle in a C-axis section - not supported"); return; }
  var r = radiusOf(p.X, p.Y, "cycle");
  if (r === null) return;
  if (Math.abs(r) > EPS) { refuse("drilling cycle off the spindle axis - a 2-axis lathe drills on centre only"); return; }
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G83";
  if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G85";
  else if (p.CycleType === "BACKBORE") { refuse("back-boring cycle - no such face cycle on a 2-axis lathe"); return; }
  if (!cycleSeat(p, r)) return;
  var words = [g, zWord(p.Z)];
  if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
    words.push("Q" + Math.round(p.Q * 1000));    // whole microns, no decimal point
  }
  if (ms > 0 && g !== "G84") words.push("P" + ms);
  words.push(feedWord(p.FeedRate));
  block(words);
  // A legacy record with no R word keeps the whole on-axis chain.
  cycX = p.X; cycY = p.Y; cycR = has(p.R) ? Number(p.R) : Infinity;
  seatKnown = true;
  inCycle = true;
}

// ------------------------------------------------------------ footer
function onClose() {
  endPreamble();
  // #504: a trip home with nothing written after it (a trailing Go
  // home) already is the program-end home - written once, not twice.
  if (linesWritten() !== homedAt) {
    block(["G28", "U0", "W0"]);
  }
  block(["M30"]);
  writeln("%");
}

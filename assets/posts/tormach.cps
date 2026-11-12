// Tormach PathPilot 3X milling post (#131). Provenance: the G-dialect
// from Tormach's "UM10587: 770MX Operator's Manual" (Version 0626A),
// chapter 10 (Programming: G-codes, canned cycles, M-codes), and §7.2.7 /
// §7.5.10 (G30); Tormach's published PathPilot pages; and the public
// LinuxCNC G-code reference (linuxcnc.org/docs, the interpreter PathPilot
// is built on) - behavior only, per this repo's Clean-Room Protocol.
// Cross-checked against programs PathPilot's own Conversational screens
// write. The 4th axis (#785) from Tormach's public owner's guides TD10749
// (microARC 4, 1223A), TD10552 (M/MX 4th axis, 0626A) and TD10374 (PCNC
// 1100/770 4th axis, 0626A) and the LinuxCNC reference's rotary-axis and
// G93/G94 sections - behaviour only. The hook contract from Autodesk's published post docs. No
// vendor post script text and no CAM post's output used; original work.
// Structure follows our own haas.cps; the goldens lock this file's
// decisions, not any vendor's.
description = "Tormach PathPilot (3-axis mill, A indexing about X, G20/G21 from the machine, G73/G81/G82/G83/G84/G86/G89, G2/G3, G99, G38.2 probing)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var coolant = "";          // the M-code this program started, if any
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var inch = false;          // the machine is programmed in inches (onOpen)

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch machine gets every length and
// feed divided by 25.4 and written at four decimals (a tenth of a
// thousandth); a metric one keeps mm at three. Angles and rpm never scale.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the machine's unit
  if (!has(v)) return "0";
  return trim(Number(len(v)).toFixed(inch ? 4 : 3));
}
function ff(v) {  // a feed: whole mm/min, or in/min at two decimals
  if (!has(v) || v <= 0) return "0";
  return inch ? trim((v / 25.4).toFixed(2)) : String(Math.round(v));
}
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; } // integer word
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // join the non-empty words onto one line
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function xyz(p) {
  if (indexed()) {  // #785: the whole tip, turned into the indexed frame
    var q = tip(p.X, p.Y, p.Z);
    return ["X" + fu(q[0]), "Y" + fu(q[1]), "Z" + fu(q[2])];
  }
  return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)];
}
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}
var tapRefused = false;   // the rigid-tap refusal, once per program

// The .mch tool-station coolant list carries names + M-codes; the
// first Flood entry (else the first entry at all) is this machine's
// cutting-fluid-on code. M9 (stop) is the RS-274 fixed pair.
function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var first = "";
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = (typeof c === "string") ? c : (c.name || "");
    if (!code) continue;
    if (name === "Flood") return code;
    if (!first) first = code;
  }
  return first;
}

// ------------------------------------------------------------ the plane
// The arc's plane (G17 XY, G18 XZ, G19 YZ), stated modally: written only
// on change, taken back to G17 before any canned cycle and at section end.
var planeG = 17;
function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}
function planeWord(g) {  // "" while modal; the word (and the new state) on change
  if (g === planeG) return "";
  planeG = g;
  return "G" + g;
}
function restorePlane() { var pw = planeWord(17); if (pw) block([pw]); }
// The center words of the arc's plane as increments from the start
// point (G91.1, stated in the header): a shared translation cancels in
// an increment, so the origin must NOT enter them.
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: a parenthesis would end the comment
// early, % is the tape's end mark and ; opens a comment on some
// controls, so all three become spaces; printable ASCII only, capped at
// 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
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
// #594: LinuxCNC reads some comments as commands - (MSG,..) (DEBUG,..)
// (PRINT,..) (LOG..) (PROBEOPEN ..) (ABORT,..) and AXIS's (AXIS,..), the
// keyword in any case (LinuxCNC G-code manual, "Comments", "Messages",
// "Probe Logging", "Logging", "Debug Messages"). A comment whose text
// would open with one is led by "- ", so user text stays inert.
var ACTIVE = /^\s*(MSG|DEBUG|PRINT|LOG|LOGOPEN|LOGAPPEND|LOGCLOSE|PROBEOPEN|PROBECLOSE|ABORT|AXIS|PY|PYRUN)\b/i;
function note(s) { block(["(" + (ACTIVE.test(s) ? "- " + s : s) + ")"]); }
function commentParam(name, value) {
  if (name === "SetupName") setupName = clean(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value).toUpperCase();
  else if (name === "ToolKind") toolKind = clean(value).toUpperCase();
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "ToolCornerRadius" && has(value)) toolCr = Number(value);
}
function opNotes() {  // at the section's preamble: the setup once, the op each time
  secOp = opName;
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
function toolNote() {  // just before the tool change it describes
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  note(joinDash(["T" + ni(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// -------------------------------------------------------------- header
// Tormach's post guidelines ask for no % or $ at the program's start or
// end, and LinuxCNC reads an O word as a subroutine label, not a program
// number - so the header is one identity comment and the modal state
// every later line relies on: the unit, absolute positions, incremental
// arc centers (G91.1, stated, never assumed), feed per minute. Path
// blending gets an explicit tolerance on its own line: a bare G64 is
// "no tolerance" in plain LinuxCNC, and a stated P reads the same on
// every build (0.001 in, or its 0.025 mm twin).
function onOpen() {
  inch = typeof machine !== "undefined" && machine.units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  // #594: the name is user text from the machine file - cleaned like
  // every other comment text, so `Tormach 770MX (ML)` never nests.
  note(clean(nm) || "CAMCRAFT");
  block([inch ? "G20" : "G21", "G17", "G90", "G91.1", "G94", "G40", "G49", "G80"]);
  block(["G64", blendP()]);
  var z = safeZ();
  if (z !== null && z !== 0) block(["(SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE)"]);
}

// #495: where "straight up" is, in machine coordinates. G53 G0 Z0 is the
// top of Z travel only where Z homes at the top with machine Z0 there -
// the PathPilot convention this post was written for (a Tormach homes Z
// at the top of its travel). A LinuxCNC machine homed with Z0 at the bottom
// would rapid DOWN into the part on that line. So the machine file
// states the height (its Z part's `safe_retract`, `machine.safeRetractZ`,
// mm); a document with no machine, or a Tormach model that states none,
// keeps the PathPilot convention (0); any other machine stating none is
// refused by name - never a guess about which way is up. Judged once.
var safeZDone = false;
var safeZValue = null;     // null = refused, nothing is written
function safeZ() {
  if (safeZDone) return safeZValue;
  safeZDone = true;
  var m = typeof machine !== "undefined" ? machine : null;
  var stated = m ? m.safeRetractZ : null;
  if (has(stated)) {
    safeZValue = Number(stated);
    return safeZValue;
  }
  var nm = (m && (m.name || m.model)) || "";
  if (nm === "" || /^\s*tormach\b/i.test(nm)) {
    safeZValue = 0;
  } else {
    msg("machine " + nm + " does not state its safe Z retract - this post lifts Z with G53 G0 before each tool change and at the end, and machine Z0 is the top of travel only on a Tormach; state the Z axis's safe_retract (machine coordinates) in the machine file");
  }
  return safeZValue;
}

// Z straight up in machine coordinates, then G30 to the operator's preset
// tool-change spot. The G30 rides PathPilot's convention with the Z0
// retract: a machine that states a different safe Z is not on it, and an
// unset G30 position (LinuxCNC's default is the machine origin) could
// drive Z back down - so there the retract stands alone and M6 / the
// control's own tool-change position does the rest.
function retract() {
  var z = safeZ();
  if (z === null) return;
  block(["G53", "G0", "Z" + fu(z)]);
  if (z === 0) {
    block(["G30"]);
    // #785 (review of PR #798): a G30 with no axis words moves EVERY
    // axis to the stored G30 position (LinuxCNC params 5181-5189), A
    // included - the A the control held is gone, so the next section
    // always restates it.
    aNow = null;
  }
}

// Cutter compensation in the control: not stated by this post yet -
// refused by name, never a centre path under a Control op.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by this post - choose In computer, or the Fanuc post");
}

function onParameter(name, value) {
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  commentParam(name, value);
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg("spindle rpm over machine maximum");
  }
}

// PathPilot's tool table numbers tools 1-1000; anything else has no
// offset to apply, so it is refused by name, never posted.
function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1 || number > 1000) {
    msg("tool number " + ni(number) + " is outside PathPilot's tool table (1-1000)");
  }
}

// The belt range is a physical belt move that no G or M code can make,
// and PathPilot's Spindle Range setting must match it. The ranges are
// the machine's own (`.mch` spindle.ranges, #170 - e.g. the 770MX's
// low 175-3250 rpm and high 525-10000 rpm). When a speed fits only one
// range the program says which, as a comment the operator sees before
// the spindle starts; a speed several ranges serve says nothing, and a
// machine stating fewer than two ranges has no belt to name. A speed
// past every range names the nearest one (the rpm check refuses it).
function beltNote(rpm) {
  var rs = (typeof machine !== "undefined" && machine.spindleRanges) || [];
  if (rs.length < 2 || !(rpm > 0)) return "";
  var lo = 0, hi = 0, serving = [];
  for (var i = 0; i < rs.length; i++) {
    if (rs[i][0] < rs[lo][0]) lo = i;
    if (rs[i][1] > rs[hi][1]) hi = i;
    if (rpm >= rs[i][0] && rpm <= rs[i][1]) serving.push(i);
  }
  var pick = -1;
  if (serving.length === 1) pick = serving[0];
  else if (serving.length === 0) pick = rpm > rs[hi][1] ? hi : (rpm < rs[lo][0] ? lo : -1);
  if (pick < 0) return "";
  var name = rs.length === 2 ? (pick === lo ? "LOW" : "HIGH")
    : ni(rs[pick][0]) + "-" + ni(rs[pick][1]) + " RPM";
  return "(SPINDLE " + ni(rpm) + " RPM NEEDS THE " + name + " BELT RANGE)";
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: plane, offsets, tool, spindle, coolant. Tormach
// recommends a G30 before every tool change (no Tormach document says
// M6 moves to a safe position on its own), run in G90 as the guidelines
// require. A bare G30 may move X, Y and Z together, so Z goes straight
// up first (G53 G0 Z0, the top of travel in machine coordinates, or the
// machine's stated safe Z - #495, safeZ) - the
// same Z-first order PathPilot's own programs use between tools - and
// only then G30. M6 then runs the changer, or pauses for a manual change
// when the tool is not in the tray, and G43 H applies the length the
// tool table holds (measured by the tool setter).
// #616: per-operation smoothing, only when the document turned it on
// (the stream then carries PARAM Smoothing / SmoothingMode). LinuxCNC path blending: G64 P is the deviation the trajectory planner may take.
// Written after the preamble, undone at the section's end so no
// tolerance leaks into the next operation.
// The header's path-blending tolerance (#616: shared by the header and
// the per-section reset, one constant).
function blendP() { return inch ? "P0.001" : "P0.025"; }
function smoothOn() {
  if (!(smoothTol > 0)) return;
  block(["G64", "P" + fu(smoothTol)]);
  smoothActive = true;
}
function smoothOff() {
  if (smoothActive) block(["G64", blendP()]);  // back to the header's blending
  smoothActive = false;
  smoothTol = 0;
  smoothMode = "";
}

function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  // #574: a touch probe (a stated speed of 0) states its spindle
  // stopped before its first move, whatever the tool change did.
  var probeStop = spindleWait && !(sRpm > 0);
  opNotes();
  block(["G17", "G90", wcsWord, "G40", "G49", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    retract();
    xyUnknown = true;        // G30 (or the changer) may have moved X and Y
    block(["T" + ni(tNo), "M6"]);
    block(["G43", "H" + ni(tNo)]);
    toolWait = false;
  }
  // A touch probe (#108, #362) states 0 rpm: no spindle start, no belt
  // note and no cutting fluid on a probe stylus.
  var touchProbe = spindleWait && !(sRpm > 0);
  if (spindleWait && sRpm > 0) {
    block([beltNote(sRpm)]);
    block(["S" + ni(sRpm), "M3"]);
  }
  if (probeStop) block(["M5"]);
  spindleWait = false;
  coolant = touchProbe ? "" : coolantCode();
  if (coolant) block([coolant]);
  smoothOn();
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // the plane never leaks past its section
  sectionOpen = false;
  aSection = null;           // #785: the next section indexes afresh
  aNow = null;               // ... and restates A: a section boundary is
                             // where an operator may stop and jog (M0/M1,
                             // a probe check), so no section inherits A
  smoothOff();
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  block(["M5"]);
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); hopZ = null; }  // #1028: the next section states its own hop

// ------------------------------------------------------- the 4th axis
// #785. A rotary table about X (Tormach's microARC 4 or the older 4th-axis
// tables) indexes the part between operations; nothing here is RTCP.
// - The machine file states it: a rotary named A whose `axis` lies along
//   X. The vector's sign is the direction sense: [1,0,0] reads the RS-274 /
//   ISO 841 way - a + A turns the TOOL right-handed about +X relative to
//   the part (the table turns the part the other way) - so the stream's
//   tool angle A posts as itself; [-1,0,0] posts it negated.
// - Indexing only. A moves alone, as G0 A.., after G53 G0 Z to the
//   machine's safe Z, at a section's first motion; no A word ever rides a
//   cutting move. A section's tilted path is then written in the indexed
//   frame: the tip relative to the work offset turned by -A about X, so
//   WCS Y0 Z0 must lie on the A centerline (the note at the first index).
// - Refused by name: a tilt the A axis cannot reach (any B or C), an A
//   past the stated travel, an A that changes inside an operation
//   (simultaneous 4-axis: PathPilot's rollover and its combined-feed
//   handling are not confirmed on a microARC 4 - TD10749 is silent, and
//   LinuxCNC leaves rollover to the machine's configuration), an indexed
//   arc (a tilted path is chords) and a tilted move on a machine with no
//   A about X.
// - No clamp code: none of the three guides names an M-code; the microARC
//   4 has no clamp, the older tables clamp by hand (loosened to rotate).
// A machine with an A states it at every section's first motion (G0 A0
// for an upright section): a program never assumes where the last run -
// or the last section, or a G30 - left the table.
var A_EPS = 1e-6;
var aAxis;               // undefined: not judged; null: no A about X stated
var aNow = null;         // the A word the control holds (null: unknown)
var aSection = null;     // the tool angle this section is indexed to
var aRefused = false;    // one named 4th-axis refusal per program
var aNoted = false;      // the operator note, at the first index
var secOp = "";          // the section's operation name, for refusals
function fa(v) { return trim(Number(v).toFixed(3)); }  // degrees, never scaled
function mName() { return (typeof machine !== "undefined" && machine.name) || "this machine"; }
function aRotary() {
  if (aAxis !== undefined) return aAxis;
  aAxis = null;
  var rs = (typeof machine !== "undefined" && machine.rotaries) || [];
  for (var i = 0; i < rs.length; i++) {
    var r = rs[i], d = r.direction;
    if (String(r.name || "").toUpperCase() !== "A" || !d || d.length !== 3) continue;
    if (!has(d[0]) || !has(d[1]) || !has(d[2])) continue;
    var n2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if (!(n2 > 0) || d[0] * d[0] < 0.998 * n2) continue;  // not along X
    var t = r.travel;
    var travel = (t && t.length === 2 && has(t[0]) && has(t[1]) && t[1] > t[0]) ? [Number(t[0]), Number(t[1])] : null;
    aAxis = { sign: d[0] > 0 ? 1 : -1, travel: travel };
    break;
  }
  return aAxis;
}
function aRefuse(s) {
  if (!aRefused) { aRefused = true; msg(s); }
  return false;
}
function inOp() { return secOp ? " in operation " + secOp : ""; }
// The tool angle about X a tilted record stands for, or null when no A
// about X reaches it. The pose law (`cl::pose`): the tool axis is
// Rz(C)·Ry(B)·Rx(A)·Z. An A about X reaches exactly the axes with no X
// component, (0, -sin a, cos a), so the letters are read through the
// axis: the pose's canonical A 0 B 180 (the face underneath) is a = 180.
// C spins the part about Z, which this machine cannot do.
function toolAngle(a0, b0, c0) {
  if (Math.abs(c0) > A_EPS) return null;
  var r = Math.PI / 180;
  var sa = Math.sin(a0 * r), ca = Math.cos(a0 * r), sb = Math.sin(b0 * r), cb = Math.cos(b0 * r);
  var ax = ca * sb, ay = -sa, az = ca * cb;
  if (Math.abs(ax) > 1e-9) return null;
  var a = Math.atan2(-ay, az) / r;
  if (a <= -180 + A_EPS) a = 180;     // one name for the face underneath
  return Math.abs(a) < 1e-9 ? 0 : a;
}
// Called before every motion: true when the record may post. Indexes the
// table at the section's first motion, when its angle is not the held one.
function indexFor(p) {
  var tilted = has(p.A);
  var ax = aRotary();
  if (!ax) {
    if (!tilted) return true;
    return aRefuse("tilted move" + inOp() + " - " + mName() + " states no A rotary about X; the Tormach PathPilot post indexes only an A about X and has no RTCP output");
  }
  var a = 0;
  if (tilted) {
    var a0 = Number(p.A), b0 = has(p.B) ? Number(p.B) : 0, c0 = has(p.C) ? Number(p.C) : 0;
    var t = toolAngle(a0, b0, c0);
    if (t === null) {
      return aRefuse("tilt A" + fa(a0) + " B" + fa(b0) + " C" + fa(c0) + inOp() + " - the A axis about X cannot reach it; " + mName() + " indexes A only");
    }
    if (!(has(p.X) && has(p.Y) && has(p.Z))) {
      return aRefuse("indexed move" + inOp() + " without X Y Z - the indexed frame needs the whole tip");
    }
    a = t;
  }
  if (aSection !== null) {
    if (Math.abs(a - aSection) > A_EPS) {
      return aRefuse("simultaneous 4-axis motion" + inOp() + " (A changes inside the operation) - not confirmed on PathPilot; this post indexes A between operations only");
    }
    return true;
  }
  var word = ax.sign * a;
  if (ax.travel && (word < ax.travel[0] - A_EPS || word > ax.travel[1] + A_EPS)) {
    return aRefuse("A" + fa(word) + inOp() + " is beyond the A axis travel " + fa(ax.travel[0]) + ".." + fa(ax.travel[1]) + " degrees");
  }
  if (aNow === null || Math.abs(word - aNow) > A_EPS) {
    var z = safeZ();
    if (z === null) return false;  // safeZ refused by name already
    if (!aNoted) {
      aNoted = true;
      note("4TH AXIS A ABOUT X - " + wcsWord + " Y0 Z0 ON THE A CENTERLINE");
      note("NO A CLAMP CODE - RELEASE ANY MANUAL ROTARY CLAMP");
    }
    block(["G53", "G0", "Z" + fu(z)]);   // straight up before the table turns
    block(["G0", "A" + fa(word)]);
    aNow = word;
    xyUnknown = true;                    // the next move restates X Y first
  }
  aSection = a;
  return true;
}
// The tip relative to the work offset, in the section's indexed frame:
// turned by -A about X (the part's turn that stands the tool axis
// (0, -sin A, cos A) up along Z). An upright section is the plain offset.
function tip(x, y, z) {
  var rx = x - orgX, ry = y - orgY, rz = z - orgZ;
  if (!aSection) return [rx, ry, rz];
  var t = aSection * Math.PI / 180, c = Math.cos(t), s = Math.sin(t);
  return [rx, ry * c + rz * s, -ry * s + rz * c];
}
function indexed() { return !!aSection; }

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after every G30: the
// first move from there would start wherever the table happens to be, a
// diagonal from an unknown point (PathPilot warns "Axis XY not specified
// by this or previous motions"). So that move is preceded by a G0 to its
// X Y alone, made at the current - safe, Z-up - height, and only then
// does Z come down: PathPilot's own XY-first order. A rapid positions at
// its target; a feed or arc at the point the stream says it starts from
// (the target when the stream names none); a cycle at its first hole.
var xyUnknown = true;
function xyFirst(x, y, z) {
  if (!xyUnknown || !has(x) || !has(y)) return;
  if (indexed()) {  // #785: X Y of the turned tip, which Z enters
    if (!has(z)) return;
    var q = tip(x, y, z);
    block(["G0", "X" + fu(q[0]), "Y" + fu(q[1])]);
  } else {
    block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  }
  xyUnknown = false;
}
// A feed or arc as the first move after a tool change starts, in the
// stream, from the generator's own start point - its clearance height
// (a face plunges from START). After the G30 the tool stands at the top
// of Z travel, so feeding from there would crawl the whole stroke at the
// plunge rate. Once X Y are positioned, a rapid takes Z down to that
// start height - where the generator says the tool already is - and the
// feed runs only the stretch it planned (proven on the 770MX, #131).
function startXY(p) {
  if (!xyUnknown) return;
  if (has(p.StartX) && has(p.StartY)) xyFirst(p.StartX, p.StartY, p.StartZ);
  else xyFirst(p.X, p.Y, p.Z);
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z) {
    if (indexed() && has(p.StartX) && has(p.StartY)) {
      // #785: "above" is the indexed frame's Z, the tool axis.
      var s0 = tip(p.StartX, p.StartY, p.StartZ), e0 = tip(p.X, p.Y, p.Z);
      if (s0[2] > e0[2]) block(["G0", "Z" + fu(s0[2])]);
    } else if (!indexed()) {
      block(["G0", wo("Z", p.StartZ, orgZ)]);
    }
  }
}

// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only.
// The chain stays on its hole: the hole's XY, at or below its R plane.
// #994: a rapid that leaves it (the hop up, the traverse, the exit)
// ends the group - G80, then the rapid posted as the stream says - so
// the program moves between holes at the hop the path was verified at,
// never at R. A cut leaving the hole is no chain a cycle replaces:
// refused by name.
function onHole(p) {
  return Math.abs(p.X - cycX) < 1e-6 && Math.abs(p.Y - cycY) < 1e-6 && p.Z <= cycR + 1e-6;
}
function chainRapid() {  // true: the open group's own retract, swallowed
  seatKnown = true;
  if (!inCycle) return false;
  if (onHole(properties)) return true;
  block(["G80"]);
  inCycle = false;
  return false;
}
function chainCut() {  // true: the cut is the cycle's (or refused)
  seatKnown = true;
  if (!inCycle) return false;
  if (!onHole(properties)) msg("a cutting move leaves its hole inside a canned-cycle group");
  return true;
}
// #1028: a probing move (a touch, its back-off, a protected positioning
// move) moves the tool like any other motion: it ends an open cycle
// group (G80 - no probe move is a hole's chain) and the tool stands
// where the stream says, so a cycle read after it is seated.
function probeSeat() {
  seatKnown = true;
  if (!inCycle) return;
  block(["G80"]);
  inCycle = false;
}
function onRapid() {
  startPreamble();
  if (chainRapid() || !indexFor(properties)) return;
  xyFirst(properties.X, properties.Y, properties.Z);
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (chainCut() || !indexFor(properties)) return;
  startXY(properties);
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  if (chainCut() || !indexFor(properties)) return;
  var p = properties;
  if (indexed()) {  // #785: a tilted path is chords; an arc here is unstated
    aRefuse("arc on an indexed A" + inOp() + " - the indexed frame posts lines only");
    return;
  }
  startXY(p);
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------- protected probe moves
// #174. The positioning feeds of a probe cycle (the descent to a touch's
// start, the travel between touches, the rise) are protected moves:
// G38.3 - the LinuxCNC G-code reference's "G38.n Straight Probe": probe
// toward the point, stop on contact, NO error when no contact comes -
// then #5070 (G38 probe result: 1 when the probe tripped, set by
// G38.3/G38.5) is tested in an `o<n> if [...]` / `o<n> endif` block
// (O-codes, same reference), and a trip shows a (MSG,...) and ends the
// program with M2: an obstacle where the model has none stops the run
// instead of the stylus being driven into it, and is never resumed past
// as M0 could be. G38.3 is itself an error when the probe is already
// tripped at the start - the control's own guard. Tormach's PathPilot
// machine-code reference lists G38.2-G38.5 and M2 as supported. The
// O-word numbers run from 5001, a range of their own.
var guardO = 5000;
function onProbeGuard() {
  startPreamble();
  probeSeat();               // #1028
  var p = properties;
  if (!indexFor(p)) return;  // #785: probing is upright (A0 on a 4th axis)
  startXY(p);
  block(["G38.3", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  guardO += 1;
  block(["o" + guardO, "if", "[#5070 EQ 1]"]);
  block(["(MSG, PROBE HIT ON POSITIONING)"]);
  block(["M2"]);
  block(["o" + guardO, "endif"]);
}

// ------------------------------------------------------- canned cycles
// One block per hole, holes of an unchanged cycle continuing modally
// with bare X Y lines. LinuxCNC's dwell P is in SECONDS (not the
// milliseconds Fanuc-family controls read).
// #994: G99, stated as each group opens - each hole returns to its R
// plane, where the stream's chain ends (RS-274 / ISO 6983: G98 returns
// to the level the tool stood at when the cycle was read, G99 to R).
// The hop and traverse to the next hole are the stream's own rapids,
// posted after G80 (above), so no hole's retract level is left to where
// the tool happened to be.
function secs(s) { return trim(Number(s).toFixed(3)); }
// #994: the cycle is read with the tool on its R plane over the hole,
// where the stream's chain starts, so the level a hole returns to is R
// and never the height the tool happened to stand at. A stream whose
// tool stands elsewhere (its anchor on the rapid down to R) is seated
// here: up to R first when below it, then over the hole, then down -
// never under the straight rapid the stream itself states. A cycle
// that is its section's first motion has no stream position to seat
// from: the cycle positions over the hole from the height it is at.
function cycleSeat(p) {
  if (!seatKnown) return;
  var lo = has(p.StartZ) && p.StartZ < p.R - 1e-6;
  var away = !(has(p.StartX) && has(p.StartY)) || Math.abs(p.StartX - p.X) >= 1e-6 || Math.abs(p.StartY - p.Y) >= 1e-6;
  var hi = !has(p.StartZ) || p.StartZ > p.R + 1e-6;
  if (!lo && !away && !hi) return;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (away) {
    // #1028: no stream rapid took the tool to this hole (two cycle
    // records back to back, or a cycle straight after a probe), so
    // the traverse runs at the section's hop: up to its RetractPlane
    // when below it, over the hole, then down to R - never lower than
    // the hop the path was verified at. A section that states no
    // RetractPlane has no hop to seat at: refused by name.
    if (!has(hopZ)) {
      msg("a canned cycle is read away from its hole with no rapid to it, and the section states no RetractPlane to traverse at");
      return;
    }
    var top = Math.max(hopZ, p.R);
    var at = has(p.StartZ) ? p.StartZ : -Infinity;
    if (at < top - 1e-6) { block(["G0", wo("Z", top, orgZ)]); at = top; }
    block(["G0", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    if (at > p.R + 1e-6) block(["G0", wo("Z", p.R, orgZ)]);
    return;
  }
  if (lo || hi) block(["G0", wo("Z", p.R, orgZ)]);
}
function onCycle() {
  startPreamble();
  restorePlane();            // a cycle drills along G17's Z
  var p = properties;
  if (!indexFor(p)) return;  // #785: a cycle drills upright (A0 on a 4th axis)
  var dwell = has(p.P) && p.P > 0 ? p.P : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = dwell > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
  // Tormach documents G84 for PathPilot: F is the pitch times the rpm
  // (the stream's synthesized tap feed), no P - PathPilot sizes the
  // bottom dwell itself, and the MX mills tap from the spindle encoder.
  else if (p.CycleType === "TAP") {
    // A machine that says it cannot rigid-tap (the R8 770M, 1100M, PCNC 440
    // and the Mach3-era PCNC 770 / 1100: tapping heads, tension/compression
    // holders and thread milling only) refuses the
    // cycle by name rather than posting a G84 its spindle cannot follow.
    if (typeof machine !== "undefined" && machine.rigidTapping === false) {
      if (!tapRefused) tapRefused = true; else return;
      msg("rigid tapping (G84) is not available on " + (machine.name || "this machine") + " - use a tapping head or thread milling");
      return;
    }
    g = "G84";
  }
  xyFirst(p.X, p.Y);         // a cycle as the first move: its first hole
  cycleSeat(p);
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q;
  if (inCycle && sig === cycleSig) {
    block([wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
  } else {
    if (inCycle) block(["G80"]);
    block(["G99"]);            // #994: each hole returns to R
    // Q is an increment (peck depth), not a position: no origin shift.
    var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
    if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
      words.push(w("Q", p.Q));
    }
    if (dwell > 0 && (g === "G82" || g === "G86" || g === "G89")) words.push("P" + secs(dwell));
    words.push(fw(p.FeedRate));
    block(words);
    cycleSig = sig;
  }
  cycX = p.X; cycY = p.Y; cycR = p.R;
  seatKnown = true;
  inCycle = true;
}

// ------------------------------------------------------------ probing
// #362. PathPilot probes the LinuxCNC way, not with a G31 skip. Words
// from the LinuxCNC G-code reference (linuxcnc.org/docs, "G38.n Straight
// Probe", "G10 L20", numbered parameters, O-code conditionals, comment
// messages) and Tormach's PathPilot post-processor guidelines / machine
// code reference (G38.2-G38.5, G10 L2/L20, M0/M2 listed as supported):
// - the touch is G38.2: probe toward the part, stop on contact, and the
//   CONTROL raises an error and halts the program when no contact comes
//   (and when the probe is already tripped at the start) - so no
//   post-side no-contact test is needed, and a probe that never touched
//   can never back off and measure air. G38.3 + a #5070 test would hand
//   that safety to our own branch instead of the control's.
// - the trip lands in #5061-#5063, "in the coordinate system in which
//   the G38 took place" (the section's own WCS, current program units,
//   the controlled point with the tool length offset in effect) - the
//   same frame and unit this post writes every coordinate in. A measured
//   trip k is kept in #(100+3k)..#(102+3k).
// - the back-off is incremental (G91 ... G90), so a part found nearer
//   than the model is never driven into.
// - a deviation past the tolerance shows a (MSG,...) and ends the program
//   (M2) inside an `o<n> if [...]` / `o<n> endif` block, one unique
//   number per test. (ABORT,...) is not in the stable LinuxCNC reference
//   and M0 could be resumed onto a wrong offset, so M2 is the alarm.
// - the work offset is written with G10 L20, "the current coordinates
//   become the given values": the current position #5420-#5422 (active
//   WCS, current program units) minus the deviation. Exact whatever unit
//   the control stores #5221.. in (the reference does not say) - G10 L2
//   from #5221 + 20 x slot would mix units on a G21 program run on an
//   inch-configured control.
// - #575: each axis's deviation (#190.., found - expected: the value the
//   G10 L20 write subtracts) is reported, after its tolerance test and
//   before the offset write (or the measure-only stop), as a
//   (DEBUG, PROBE <axis> ERROR #19n MM) comment - IN in a G20 program:
//   the value is in the program's unit, and the line says which. The LinuxCNC G-code
//   reference ("Messages" / "Comments" section): DEBUG shows its text like
//   MSG - in the operator's status display, without stopping the program -
//   with each #n / #<name> in it replaced by the parameter's value.
// - arming: PathPilot's probe input is always live (the probe type is a
//   Settings-tab choice); no M-code arms it, and none is written.
var probeBase = 100;
var probeIf = 900;        // the next O-word number for a test block
function probeAxis(a) { return a === "X" ? 0 : (a === "Y" ? 1 : 2); }
function onProbeMove() {
  startPreamble();
  probeSeat();               // #1028
  var p = properties;
  if (!indexFor(p)) return;  // #785: probing is upright (A0 on a 4th axis)
  xyFirst(p.X, p.Y);       // never a probe from an unknown X Y
  block(["G38.2", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  if (has(p.Measure) && p.Measure >= 0) {
    var b = probeBase + 3 * p.Measure;
    block(["#" + b + "=#5061"]);
    block(["#" + (b + 1) + "=#5062"]);
    block(["#" + (b + 2) + "=#5063"]);
  }
}
function onProbeRetreat() {
  probeSeat();               // #1028
  var p = properties;
  block(["G91", "G1", w("X", p.DeltaX), w("Y", p.DeltaY), w("Z", p.DeltaZ), fw(p.Feed)]);
  block(["G90"]);
}
function nextO() { probeIf += 1; return "o" + probeIf; }  // unique per test block
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 190;  // #190.. hold each axis's deviation, found - expected
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i];
    var ai = probeAxis(a.Axis);
    var sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("#" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "[[" + sum.join("+") + "]/" + a.Measures.length + "]" : sum[0];
    // LinuxCNC evaluates arithmetic only inside brackets: an assignment
    // is `#n=[expr]` (PathPilot Hub refused the bare `#190=#102-0.7874`
    // with "Bad character '-'"). The sign is folded so no `--` is written.
    var ex = a.Expected - org[ai];
    var op = ex < 0 ? "+" : "-";
    block(["#" + (dev + i) + "=[" + mean + op + fu(Math.abs(ex)) + "]"]);
    var o = nextO();
    block([o, "if", "[ABS[#" + (dev + i) + "] GT " + fu(p.Tolerance) + "]"]);
    block(["(MSG, PROBE " + a.Axis + " OUT OF TOLERANCE)"]);
    block(["M2"]);
    block([o, "endif"]);
    // #575: the deviation that passed is reported before it is written,
    // in the program's own unit (the G20/G21 onOpen wrote), named so the
    // operator never reads an inch value as mm.
    block(["(DEBUG, PROBE " + a.Axis + " ERROR #" + (dev + i) + (inch ? " IN" : " MM") + ")"]);
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var words = ["G10", "L20", "P" + (Number(p.SetWorkOffset) + 1)];
    for (var j = 0; j < axes.length; j++) {
      words.push(axes[j].Axis + "[#" + (5420 + probeAxis(axes[j].Axis)) + "-#" + (dev + j) + "]");
    }
    block(words);
  } else {
    block(["(MSG, CHECK PROBE RESULT #" + dev + "..)"]);
    block(["M0"]);
  }
}

// ------------------------------------------------------------ footer
// G53 G0 Z0 lifts the spindle straight up to the top of Z travel in
// machine coordinates (Tormach's Z home is at the top; another machine
// states its own height, #495 - see safeZ), whatever the
// stored G30 position holds; then G30 takes the machine to the
// operator's preset G30 location (the unload spot), the way PathPilot's
// own programs end. M30 ends the program (no % fence).
//
// #596: LinuxCNC's M2 / M30 (linuxcnc.org/docs, M-codes, "M2, M30
// Program End") resets offsets to G54, the plane to G17, G90, G94, M48,
// G40, M5, G1 and M9 - but not the length unit, so a G21 program leaves
// an inch-run control in mm (DRO and MDI included). The user's post
// option `EndUnits` ("inch" / "mm"; absent = leave the program's unit)
// names the unit the control should be left in: when it differs from
// the program's, its word goes after the final retract, right before
// M30 - after the last move, so no position is read in the new unit.
function endUnitsWord() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch" && !inch) return "G20";
  if (want === "mm" && inch) return "G21";
  return "";
}
function onClose() {
  endPreamble();
  retract();
  var u = endUnitsWord();
  if (u) block([u]);
  block(["M30"]);
}

// Makera Carvera / Carvera Air 3-axis milling post (#707). Provenance:
// the words from the Carvera firmware's supported-codes tables (Makera's
// Smoothieware-based firmware; the tables as documented at
// carvera-community.gitbook.io, firmware -> supported commands -> GCodes
// and MCodes, read 2026-11-10) and the Carvera instruction manual
// (coordinates, tool changer, wireless probe) - behavior only, per this
// repo's Clean-Room Protocol. Where M6 leaves the spindle (#1616): the
// firmware's open GPL source on GitHub (MakeraInc/CarveraFirmware and
// Carvera-Community/Carvera_Community_Firmware, ATCHandler.cpp and
// Robot.cpp, read 2026-11-14), read for behavior only. The hook contract from Autodesk's published
// post docs. No vendor post script text and no CAM post's output used;
// original work. Structure follows our own tormach.cps; drilling as its
// expanded chain follows our own heidenhain.cps.
description = "Makera Carvera (3-axis mill, G21 only, M6 Tn auto tool change, drilling as plain moves, G2/G3 in G17/G18/G19)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 6;      // G54..G59; extended offsets not verified for this firmware (#931)
coolantModes = true; // reads PARAM Coolant (#869): the op's ask is posted, or refused by name
programUnits = "mm"; // #902: millimetres only - the engine refuses an inch document by name

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var tNo = 0;
var coolant = "";          // the M-code this section started, if any
var coolantAsk = "flood";  // the section's PARAM Coolant (#869), flood when unstated
var coolantOp = "";        // the section's OperationName, for refusals
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var pendingDwell = null;   // a drilling dwell owed at its hole's bottom
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"

// ------------------------------------------------------------ helpers
// The firmware is millimetres only (its G20 "will halt the machine"), so
// every length is mm at three decimals and every feed whole mm/min; an
// inch document is refused by the engine before this script runs
// (`programUnits = "mm"`, #902) - never mm numbers for an inch program.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fu(v) { return has(v) ? trim(Number(v).toFixed(3)) : "0"; }
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
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
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}

// #1359 (from #1357): a cut needs a turning spindle. spindleOn is true
// from the preamble's spindle start (M3 S..) to the next M5 (every
// section end, and a touch probe's stop). A touch probe's section feeds
// its stylus with the spindle stopped on purpose: a section is a
// probe's only when it states 0 rpm (#574) AND its tool is a touch
// probe (PARAM ToolKind) - a cutter whose speed reads 0 is a stopped
// spindle, not a probe. A feed, arc or cycle with the spindle stopped
// anywhere else is refused by name and not posted: a stream that states
// no spindle speed would otherwise drag a stopped cutter through the
// part. Once per program.
var spindleOn = false;
var probeSection = false;
var kindProbe = false;    // this section's PARAM ToolKind names a touch probe
var spindleRefused = false;
function spindleStopped(what) {  // true: refused, nothing is posted
  if (spindleOn || probeSection) return false;
  if (!spindleRefused) {
    spindleRefused = true;
    msg(what + " with the spindle stopped - no spindle start (M3 S..) precedes it; state the operation's spindle speed");
  }
  return true;
}
var tiltRefused = false;  // one named refusal per program, not per line
function tiltOk(p) {  // this post is 3-axis: a tilted record is refused
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - the Carvera post is 3-axis (the 4th axis module is not supported yet)"); }
  return false;
}

// The machine file's coolant list (#869): the op's ask is the code the
// machine states for it - the Carvera's air blast is its `Air` entry (M7
// "starts the airflow", M9 stops it). A flood ask takes the Flood entry,
// failing one the first coded entry of no known family (a shop's own
// name); a stated codeless Flood is the operator's own valve, silent. A
// list that states only other kinds (an Air-only Carvera) has no flood
// code: refused by name - never M7 posted for a flood ask, never M8 for
// an air ask, never silently dry. An unstated list stays silent.
var COOLANT_KINDS = ["flood", "mist", "air", "air-blast", "through-spindle", "through-tool"];
function coolantWho() { return coolantOp ? "operation " + coolantOp + ": " : ""; }
function coolantName(c) {
  return String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
}
function coolantFor(ask) {
  if (ask === "off") return "";
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var flood = !ask || ask === "flood";
  var want = flood ? ["flood"] : (ask === "mist" ? ["mist"] : (ask === "air" ? ["air", "air-blast"]
    : (ask === "through-tool" ? ["through-spindle", "through-tool"] : [])));
  if (want.length === 0) { msg(coolantWho() + "unknown coolant ask " + ask); return ""; }
  var other = "", named = false;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = coolantName(c);
    if (want.indexOf(name) >= 0) { if (code) return code; if (flood) named = true; continue; }
    if (flood && code && !other && COOLANT_KINDS.indexOf(name) < 0) other = code;
  }
  if (flood && (other || named || cs.length === 0)) return other;
  msg(coolantWho() + "the machine states no " + (flood ? "flood" : ask) + " coolant code - pick the operation's coolant from the machine's list (Air for the Carvera's air blast; Off cuts dry) or add it to the machine");
  return "";
}

// ------------------------------------------------------------ the plane
// G17 / G18 / G19 are listed; written only on change, back to G17 at
// section end. The firmware's arc centres are relative to the start
// point (the I J K increments), its default - no G91.1 word exists.
var planeG = 17;
function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}
function planeWord(g) {
  if (g === planeG) return "";
  planeG = g;
  return "G" + g;
}
function restorePlane() { var pw = planeWord(17); if (pw) block([pw]); }
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// ------------------------------------------------------------ comments
// Program commentary (#257): setup, operation and tool, for the operator.
// User text is cleaned: a parenthesis would end the comment early and ;
// opens a line comment, so both become spaces; printable ASCII only,
// capped at 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
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
  else if (name === "ToolKind") { toolKind = clean(value).toUpperCase(); kindProbe = toolKind === "TOUCH PROBE"; }
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "ToolCornerRadius" && has(value)) toolCr = Number(value);
}
function opNotes() {
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
function toolNote() {
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  note(joinDash(["T" + ni(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// -------------------------------------------------------------- header
// One identity comment, then the modal state every later line relies on
// - only words the supported-codes table lists: G21 (mm), G17, G90
// (absolute), G94 (feed per minute). No G40 / G49 / G80: the table lists
// none of them (no cutter comp, no tool length word, no canned cycles).
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  note(clean(nm) || "CAMCRAFT");
  block(["G21", "G17", "G90", "G94"]);
}

// No G41 / G42 in the firmware's table: refused by name, never a centre
// path under a Control op.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not in the Carvera firmware - choose In computer");
}

// #931: PARAM WorkOffset arrives as the 1-based offset number (the engine
// refuses one above workOffsetMax by setup name); the word is ours to write.
function workOffsetName(n) {  // offset n -> this control's word; "" when it has none
  return n >= 1 && n <= 6 && n === Math.floor(n) ? "G" + (53 + n) : "";
}
function offsetWord(value) {
  var n = Number(value), w = workOffsetName(n);
  if (!w) { msg("work offset " + value + " - this post expresses work offsets 1-" + workOffsetMax + " only"); return wcsWord; }
  wcsIndex = n;
  return w;
}

function onParameter(name, value) {
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "OperationName") coolantOp = String(value || "");
  commentParam(name, value);
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;  // #1616
  else if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1616
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg("spindle rpm over machine maximum");
  }
}

// M6 Tn: T0 is the wireless probe and T-1 "no tool" in the firmware's
// numbering, so a cutting tool is 1 or above; a tool number the
// changer does not hold is the machine check's refusal (the C1's
// tooling.number_of_tools).
function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1) {
    msg("tool number " + ni(number) + " is not a Carvera cutting tool (T0 is the wireless probe) - number tools from 1");
  }
}

// #616 / #757: per-operation smoothing. The firmware's supported-codes
// tables list no path-blending or smoothing word (no G64, G61, G5.1 or
// similar), so the ask is named once and nothing is written - never a
// guessed word.
var smoothNamed = false;
function smoothOn() {
  if (!(smoothTol > 0) || smoothNamed) return;
  note("SMOOTHING NOT WRITTEN - NO CARVERA FORM STATED");
  smoothNamed = true;
}
function smoothOff() {
  smoothTol = 0;
  smoothMode = "";
}

// The preamble closes over everything a raw motion line needs: the work
// offset, the tool, the spindle, the air. M6 Tn is the firmware's auto
// tool change (the Air's quick changer prompts the operator through the
// same word): the firmware fetches the tool and measures its length on
// its own, so no length-offset word follows.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block(["G17", "G90", wcsWord]);
  planeG = 17;
  if (toolWait) {
    toolNote();
    xyUnknown = true;          // the changer moved X and Y
    block(["M6", "T" + ni(tNo)]);
    toolWait = false;
  }
  var touchProbe = spindleWait && !(sRpm > 0);   // a stated speed of 0
  probeSection = touchProbe && kindProbe;
  if (spindleWait && sRpm > 0) { block(["M3", "S" + ni(sRpm)]); spindleOn = true; }
  spindleWait = false;
  // #995: a touch probe section starts no coolant (nor air), whatever
  // its ask - the Carvera posts no probing, but the law holds here too.
  coolant = touchProbe ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  smoothOn();
}

function endPreamble() {
  if (!sectionOpen) return;
  owedDwell();
  restorePlane();
  sectionOpen = false;
  smoothOff();
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); clearZ = null; hopZ = null; }  // #1616: the next section states its own planes

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after a tool change, so
// the first move positions X Y alone - never a diagonal from wherever the
// changer left the spindle - with its height STATED (#1616, the #1614
// hazard): a Z-only rapid to the section's clearance - its
// ClearancePlane, else its RetractPlane, never below the move's own Z.
// A setup that cannot reach it is stopped by the control's Z limit
// instead of rapiding through the part. A stream that states neither
// plane has no verified height to traverse at: refused by name.
// Z first everywhere, right after M6 T too (#1616). The firmware's
// public source (MakeraInc/CarveraFirmware and the Carvera Community
// firmware, ATCHandler::on_main_loop) ends every M6 with
// G53 G0 Z<coordinate.clearance_z> - a few mm under the top of Z travel,
// where Z homes - then G53 G0 X Y back to the machine X/Y it read when the
// M6 arrived. The spindle is not left over the tool rack: it stands where
// the program put it before the change (the previous section's last
// move), so the Z-only rapid down to the clearance is made there. The
// firmware's soft Z limit halts a G0 Z above its top, so a tool too long
// to reach the clearance stops the program before the traverse instead
// of crossing the part (docs/decisions/1616-carvera-after-m6.md).
// A feed's first move then rapids Z to the height the generator says the
// tool starts from, and feeds only the stretch planned.
var xyUnknown = true;
var clearZ = null;            // #1616: the section's ClearancePlane, null when unstated
var hopZ = null;              // #1616: the section's RetractPlane, null when unstated
var traverseRefused = false;  // #1616: once per program
var traversedAt = null;       // #1616: the Z the last first-traverse was made at
function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}
// #1682: false when refused - the caller then writes nothing for its
// move, nor for any later one while X/Y stay unknown (the rest of the
// section), so no combined X/Y/Z move from an unknown height reaches the
// text (the Tormach and Fanuc-family posts' rule, #1614 / #1633).
function xyFirst(x, y, z) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  traversedAt = null;
  var tz = traverseZ(z);
  if (tz === null) {
    if (!traverseRefused) {
      traverseRefused = true;
      msg(coolantWho() + "first X/Y traverse with no clearance height stated (no ClearancePlane or RetractPlane) - the post will not rapid X/Y at an unknown height");
    }
    return false;
  }
  block(["G0", wo("Z", tz, orgZ)]);
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  traversedAt = tz;
  xyUnknown = false;
  return true;
}
function startXY(p) {  // false: refused (#1682), the caller writes nothing
  if (!xyUnknown) return true;
  var ok = (has(p.StartX) && has(p.StartY)) ? xyFirst(p.StartX, p.StartY, p.StartZ) : xyFirst(p.X, p.Y, p.Z);
  if (!ok) return false;
  // #1616: not when the stated traverse height already is it.
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z &&
      !(has(traversedAt) && Math.abs(traversedAt - p.StartZ) < 1e-9)) {
    block(["G0", wo("Z", p.StartZ, orgZ)]);
  }
  return true;
}

function onRapid() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z)) return;  // #1682: refused - the move is not written
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (spindleStopped("feed move G1")) return;
  if (!startXY(properties)) return;  // #1682: refused - the move is not written
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
  dwellAtBottom(properties.Z);
}
function onCircular() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G2" : "G3"))) return;
  var p = properties;
  if (!startXY(p)) return;  // #1682: refused - the move is not written
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------------------------ drilling
// The firmware's G-code table lists no canned cycle (no G73 / G81-G89),
// so no cycle is written: every hole posts as its expanded chain - the
// generator's own plunges, peck retracts and exits, the exact motion the
// simulation verified. A dwell becomes G4 P<seconds> (the table's
// "Dwell P<seconds>") right after the move that reaches the hole's
// bottom. Tapping cannot be plain moves (the feed out of a tap needs the
// spindle reversed in step with it, and the stock machine has no M4), so
// TAP is refused by name.
var DWELL_TOL = 1e-6;
function onCycle() {
  startPreamble();
  owedDwell();
  var p = properties;
  if (p.CycleType === "TAP") {
    msg("tapping is not possible on the Carvera (no canned cycles, and the spindle cannot reverse) - refused, not posted as plain moves");
    return;
  }
  if (has(p.P) && p.P > 0) pendingDwell = { z: Number(p.Z), s: Number(p.P) };
}
function dwellAtBottom(z) {
  if (pendingDwell === null || !has(z) || Math.abs(z - pendingDwell.z) > DWELL_TOL) return;
  block(["G4", "P" + trim(Number(pendingDwell.s).toFixed(3))]);
  pendingDwell = null;
}
// A dwell whose chain never reached its bottom is refused, never lost.
function owedDwell() {
  if (pendingDwell === null) return;
  pendingDwell = null;
  msg("drilling dwell without a feed move to the hole bottom - refused, not dropped");
}

// ------------------------------------------------------------ probing
// Deliberately no onProbeMove / onProbeGuard / onProbeResult. The
// firmware probes with G38.2-G38.5 and its own M46x macros, but how it
// reports a trip (which variables, which frame) is not in the sources
// this post is specced from - so the engine refuses a Probe operation by
// name for this post rather than guess.

// ------------------------------------------------------------ footer
// The table's program end is M2 ("end program, stop spindle and air");
// M30 is listed as "no action on the Carvera", so it is not the end word.
// The program ends where its last retract left the tool - no machine-
// coordinate move is guessed (the manual puts machine zero at the top
// right, but states no safe retract height for a G53 move).
function onClose() {
  endPreamble();
  block(["M2"]);
}

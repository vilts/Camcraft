// Grbl 1.1 3-axis milling post (#2481). Provenance: the G-code and
// M-code set that Grbl 1.1 documents as supported in its public wiki
// ("Grbl v1.1 Commands", "Grbl v1.1 Interface": behaviour only; no Grbl
// source read, it is GPL) and RS-274 / ISO 6983 for the shared words.
// Derived from our own grblhal.cps (#710); the differences are Grbl 1.1's
// smaller word set: no canned cycles (drill, peck, bore and ream cycles
// are written as G0 / G1 / G4 moves), no M6 (a tool change is a stop,
// M0, with the tool named in a comment), and an 80-character line
// buffer. No vendor or CAM post script text and no other CAM's output
// used; original work. The goldens lock this file's decisions.
description = "Grbl 1.1 (3-axis mill, G20/G21 from the document, G2/G3, drill cycles written as G0/G1/G4 moves, M0 tool change; no G41/G42, no tapping, no probing)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
workOffsetIndex = true; // reads the 1-based WorkOffset number (#1385)
workOffsetMax = 6;      // G54..G59, per the Grbl 1.1 command list
coolantModes = true; // reads PARAM Coolant (#112/#150)
afterModes = true;   // reads PARAM AfterOperation (#110)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
endUnits = "G20 G21";     // #2189: switches end units before M30 when asked

declaredOptions = {
  showSequenceNumbers: {
    type: "enum",
    title: "Sequence numbers",
    group: "Format",
    description: "Where to emit N-word block numbers",
    choices: [
      { id: "off", title: "Off" },
      { id: "every-block", title: "Every block" },
      { id: "tool-change", title: "On tool change" },
      { id: "op-start", title: "On operation start" },
    ],
    default: "off",
  },
  sequenceNumberStart: { type: "integer", title: "First sequence number", group: "Format", description: "Starting N number", min: 1, max: 999999, default: 10 },
  sequenceNumberIncrement: { type: "integer", title: "Sequence number increment", group: "Format", description: "Step between sequence numbers", min: 1, max: 10000, default: 10 },
  separateWordsWithSpace: { type: "boolean", title: "Separate words with space", group: "Format", description: "Insert a space between G-code words on a block", default: true },
  writeMachine: { type: "boolean", title: "Write machine header", group: "Header & comments", description: "Emit a comment naming the machine in the header", default: false },
  writeToolList: { type: "boolean", title: "Write tool list", group: "Header & comments", description: "Emit a summary comment per tool at program start", default: false },
  writeVersion: { type: "boolean", title: "Write post version", group: "Header & comments", description: "Emit post version metadata comment in the header", default: false },
  optionalStop: { type: "boolean", title: "Optional stop (M1)", group: "Behavior", description: "Write M1 before tool changes after the first operation", default: false },
  useRadiusArcs: { type: "boolean", title: "Use radius (R) arcs", group: "Cycles & arcs", description: "Write R word instead of I/J/K arc centers", default: false },
};

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var toolsLoaded = 0;       // tools named so far: the first needs no stop
var tNo = 0;
var loadedTool = null;     // #3445: the tool the operator last loaded; null = unknown
var coolant = "";          // the M-code this section started, if any
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var setupNext = null;      // #1647: null means no marker; a cleaned name may be empty
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var inch = false;          // the document's unit is inch (onOpen, #902: postOptions.Units)
var smoothTol = 0;         // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";       // #616: "finish" or "rough"
var smoothNamed = false;   // the not-written note, once per program

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch document gets every length and
// feed divided by 25.4 and written at four decimals; an mm one keeps
// mm at three. Angles and rpm never scale.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the document's unit
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
var seqNum = 0;
var seqNextForce = false;
var secStarted = 0;
function optBool(k, def) {
  return typeof postOptions !== "undefined" && postOptions && typeof postOptions[k] === "boolean" ? postOptions[k] : def;
}
function optInt(k, def) {
  return typeof postOptions !== "undefined" && postOptions && typeof postOptions[k] === "number" && isFinite(postOptions[k]) ? Math.round(postOptions[k]) : def;
}
function optStr(k, def) {
  return typeof postOptions !== "undefined" && postOptions && typeof postOptions[k] === "string" ? postOptions[k] : def;
}
function nextSeq() {
  if (seqNum <= 0) seqNum = optInt("sequenceNumberStart", 10);
  else seqNum += optInt("sequenceNumberIncrement", 10);
  return "N" + seqNum;
}
function isCommentOnly(a) {
  return a.length === 1 && String(a[0]).charAt(0) === "(" && String(a[0]).charAt(String(a[0]).length - 1) === ")";
}
function block(words) {  // join the non-empty words onto one line
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) {
    var commentOnly = isCommentOnly(a);
    var sep = !commentOnly && !optBool("separateWordsWithSpace", true) ? "" : " ";
    var body = a.join(sep);
    var prefix = "";
    if (!commentOnly) {
      var mode = optStr("showSequenceNumbers", "off");
      if (mode === "every-block" || ((mode === "tool-change" || mode === "op-start") && seqNextForce)) {
        prefix = nextSeq() + " ";
        seqNextForce = false;
      }
    }
    var line = prefix + body;
    if (line.length > MAX_LINE) {
      if (commentOnly) {
        // A comment is cut to fit; the text inside stays a comment.
        line = line.substring(0, MAX_LINE - 1).replace(/\s+$/, "") + ")";
      } else {
        lineRefuse(line);
        return;
      }
    }
    writeln(line);
  }
}
// Grbl 1.1 reads each line into an 80-byte buffer (the "Interface"
// page): a longer line is refused by the control with an overflow
// error, part way through a program. So every line, terminator
// included, fits in 80: 79 characters at most. A comment is shortened;
// a motion line that does not fit is refused by name, never split.
var MAX_LINE = 79;
var lineRefused = false;
function lineRefuse(line) {
  if (lineRefused) return;
  lineRefused = true;
  msg("a block is " + line.length + " characters long - Grbl 1.1 reads at most " + MAX_LINE + " characters per line: " + line.substring(0, 30) + "...");
}
function writeHeaderExtras() {
  var nm = (typeof machine !== "undefined" && machine.name) || "";
  if (optBool("writeMachine", false) && nm) note("MACHINE: " + clean(nm));
  if (optBool("writeVersion", false)) note("POST: " + clean(description));
  if (optBool("writeToolList", false) && typeof programTools !== "undefined" && programTools.length) {
    for (var i = 0; i < programTools.length; i++) {
      var pt = programTools[i];
      var desc = [];
      if (pt.kind) desc.push(clean(String(pt.kind).toUpperCase()));
      if (has(pt.diameter) && pt.diameter > 0) desc.push("D" + fu(pt.diameter));
      if (pt.name) desc.push(clean(pt.name));
      note("T" + ni(pt.number) + (desc.length ? " - " + desc.join(" ") : ""));
    }
  }
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function arcWord(k, v) { return wo(k, v, k === "X" ? orgX : (k === "Y" ? orgY : orgZ)); }  // #3442: a position word as written, for arcFate
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}

// #1359 (from #1357): a cut needs a turning spindle. spindleOn is true
// from the preamble's spindle start (S.. M3) to the next M5 (every
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
    msg(what + " with the spindle stopped - no spindle start (S.. M3) precedes it; state the operation's spindle speed");
  }
  return true;
}
var tapRefused = false;   // the tapping refusal, once per program
var tiltRefused = false;  // one named refusal per program, not per line
function tiltOk(p) {  // this post is 3-axis: a tilted record is refused
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - the Grbl 1.1 post is 3-axis and has no rotary or RTCP output"); }
  return false;
}

// ------------------------------------------------------------ coolant
// The .mch tool-station coolant list carries names + M-codes. A flood
// ask takes the machine's Flood entry; failing one, the first coded
// entry of no other known family; a stated codeless Flood is the
// operator's own valve, silent. A list that states only other kinds has
// no flood code: refused by name (#150). An unstated list stays silent
// (the unstated-limit law). Grbl 1.1 lists M7 / M8 / M9; any other
// code is the machine file's own statement about its build.
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
// The op's coolant ask (#112): flood when unstated, or off / mist /
// through-tool - the machine's own entry of that family, refused by
// name (naming the operation) when the machine states none.
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

// ------------------------------------------------------------ the plane
// The arc's plane (G17 XY, G18 XZ, G19 YZ - all three in Grbl 1.1's list),
// stated modally: written only on change, taken back to G17 before any
// canned cycle and at section end.
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
// User text is cleaned for this dialect: a parenthesis would end the
// comment early (grbl-family comments do not nest), % is the tape's end
// mark and ; opens a line comment, so all three become spaces; printable
// ASCII only, capped at 40 characters (grbl-family controls read a
// bounded line buffer).
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
// Grbl 1.1 shows a (MSG,..) comment as a message, and grbl-family
// senders and forks read more comment words as commands. A comment whose
// text would open with one is led by "- ", so user text stays inert.
var ACTIVE = /^\s*(MSG|DEBUG|PRINT|LOG|LOGOPEN|LOGAPPEND|LOGCLOSE|PROBEOPEN|PROBECLOSE|ABORT|AXIS|PY|PYRUN)\b/i;
function note(s) { block(["(" + (ACTIVE.test(s) ? "- " + s : s) + ")"]); }
// Program notes use this dialect without truncating the stated bound.
function onComment(s) { note(commentText(s)); }
function commentParam(name, value) {
  if (name === "SetupName") setupName = clean(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value).toUpperCase();
  else if (name === "ToolKind") { toolKind = clean(value).toUpperCase(); kindProbe = toolKind === "TOUCH PROBE"; }
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "ToolCornerRadius" && has(value)) toolCr = Number(value);
}
function opNotes() {  // at the section's preamble: the setup once, the op each time
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
// One identity comment, then the modal state every later line relies on:
// the unit, the plane, absolute positions, incremental arc centers
// (G91.1, stated, never assumed), feed per minute, no compensation, no
// cycle. No O number (not in Grbl 1.1), no % fence, no N numbers unless
// asked, and no G64: Grbl 1.1 has no path-blending word.
//
// No G43.1 / G49 either: Grbl 1.1 has no tool table, and its dynamic
// length offset (G43.1) is set by the operator or the sender when a tool
// is touched off. The usual practice on these machines is to set the
// work Z zero again after each tool change; the tool-change stop says
// so. A G49 from the program would throw away an offset the sender has
// just set, so the post never touches the offset.
function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions && postOptions.Units === "inch"; // #902: the document's unit, never the machine file's
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  note(clean(nm) || "CAMCRAFT");
  writeHeaderExtras();
  block([inch ? "G20" : "G21", "G17", "G90", "G91.1", "G94", "G40", "G80"]);
}

// #495 / #710: where "straight up" is. On Grbl 1.1 machine Z0 is only a
// height when the machine has homed: with homing off ($22=0) the control
// takes its power-on position as machine zero, so `G53 G0 Z0` goes to
// wherever Z stood at switch-on - possibly DOWN into the part. So:
// - a machine file stating its Z part's `safe_retract`
//   (`machine.safeRetractZ`, mm) says it is homed and where up is: the
//   lift is `G53 G0 Z<that>`, in machine coordinates;
// - otherwise (no machine, or one stating none) the lift stays in the
//   work offset: `G0 Z<h>`, h the highest Z the program has proven clear -
//   the section's ClearancePlane, or any higher Z it has already
//   commanded. A height that is unknown, or not above the stock top, is
//   refused by name - never a guess about which way is up.
var liftZ = null;          // world mm: the highest proven-clear Z so far
var stockTop = null;       // world mm: the stock's top, when stated
function seeZ(z) { if (has(z) && (liftZ === null || z > liftZ)) liftZ = Number(z); }
function machineSafeZ() {
  var m = typeof machine !== "undefined" ? machine : null;
  return m && has(m.safeRetractZ) ? Number(m.safeRetractZ) : null;
}
var liftRefused = false;   // one named refusal per program
function liftRefuse(why) {
  if (liftRefused) return;
  liftRefused = true;
  msg(why + " - this post lifts Z in the work offset before each tool change and at the end, because without homing machine Z0 is the power-on height; set the operation's clearance height above the stock, or state the Z axis's safe_retract (machine coordinates) in the machine file if the machine homes");
}

// No G28 / G30: their stored positions are controller settings the post
// cannot see, and an unset one is machine zero on all three axes.
function retract() {
  var mz = machineSafeZ();
  if (mz !== null) { block(["G53", "G0", "Z" + fu(mz)]); return; }
  if (liftZ === null) { liftRefuse("no clearance height is known for the Z lift"); return; }
  if (stockTop !== null && !(liftZ > stockTop)) {
    liftRefuse("the clearance height Z" + fu(liftZ - orgZ) + " is not above the stock top Z" + fu(stockTop - orgZ));
    return;
  }
  block(["G0", wo("Z", liftZ, orgZ)]);
}

// Grbl 1.1 lists G40 only: there is no cutter compensation in the
// control, so a Control (or Wear) op is refused by name.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by Grbl 1.1 - choose In computer");
}

// #1385: the engine validates the offset index before this script receives it.
function workOffsetName(n) { return "G" + (53 + n); }

function onParameter(name, value) {
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  if (name === "WorkOffset" && value) wcsWord = workOffsetName(Number(value));
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "ClearancePlane") { seeZ(value); clearZ = has(value) ? Number(value) : null; }  // #1616
  else if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1616
  else if (name === "StockZTop" && has(value)) stockTop = Number(value);
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

// #616: per-operation smoothing. Grbl 1.1 states path control G61
// (exact path) only - no G64 P and no other word that bounds the
// deviation by a tolerance - so the ask is named once per program and
// nothing is written: never an unbounded blending word.
function smoothOn() {
  if (!(smoothTol > 0) || smoothNamed) return;
  note("SMOOTHING NOT WRITTEN - GRBL 1.1 HAS NO SMOOTHING TOLERANCE");
  smoothNamed = true;
}
function smoothOff() {  // the next section states its own ask
  smoothTol = 0;
  smoothMode = "";
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: plane, offset, tool, spindle, coolant. Grbl 1.1 has no
// M6 and reads no T word as a change, so a tool change is a stop: Z goes
// up, the spindle is already off (M5 at the end of every section), a
// comment names the tool to load and asks for the Z zero to be set
// again, then M0 holds the program until the operator resumes. The
// first tool is loaded before the start: it is named, with no stop.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  var secIdx = secStarted;
  secStarted += 1;
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  opNotes();
  block(["G17", "G90", wcsWord, "G40", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    retract();
    // #3445: a tool loaded for this program, with no stop since, is still
    // in the spindle: no stop to load it again.
    if (loadedTool !== tNo) {
      if (secIdx > 0 && optBool("optionalStop", false)) block(["M1"]);
      if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
      if (toolsLoaded > 0) {
        note("TOOL CHANGE - LOAD T" + ni(tNo) + " - SET Z ZERO - RESUME");
        block(["M0"]);
      } else {
        note("LOAD T" + ni(tNo) + " AND SET Z ZERO BEFORE THE START");
      }
      toolsLoaded += 1;
      loadedTool = tNo;
    }
    xyUnknown = true;        // a manual change may have jogged X and Y
    toolWait = false;
  }
  // A touch probe states 0 rpm: no spindle start and no cutting fluid.
  var touchProbe = spindleWait && !(sRpm > 0);
  if (spindleWait && sRpm > 0) { block(["S" + ni(sRpm), "M3"]); spindleOn = true; }
  probeSection = touchProbe && kindProbe;
  spindleWait = false;
  coolant = touchProbe ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  smoothOn();
}

// The op's after-action (#110), written once its spindle has stopped:
// M1 / M0 are in Grbl 1.1's program-flow list. A trip home is refused by
// name: G28 / G30 go to stored positions that are controller settings
// (machine zero when unset), and homing itself ($H) is not a G-code.
function afterOp() {
  var ask = afterAsk;
  var next = setupNext;
  afterAsk = "";
  setupNext = null;
  if (!ask) return;
  if (next !== null && (ask === "optional-stop" || ask === "stop")) {
    // #1647: lift Z before inviting the operator to re-clamp the part.
    retract();
    note("SETUP CHANGE - NEXT: " + (next || "UNNAMED SETUP"));
  }
  // The operator may take the tool out at either stop (#3445).
  if (ask === "optional-stop") { block(["M1"]); loadedTool = null; return; }
  if (ask === "stop") { block(["M0"]); loadedTool = null; return; }
  if (ask === "home") {
    msg(coolantWho() + "going home after an operation is not posted by the Grbl 1.1 post - the G28/G30 positions are controller settings it cannot see; pick Optional stop or Program stop");
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // the plane never leaks past its section
  sectionOpen = false;
  smoothOff();
  owedDwell();
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
  afterOp();
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); clearZ = null; hopZ = null; }  // #1616: the next section states its own planes

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after every tool change
// (a manual change may jog the spindle anywhere): the first move from
// there is preceded by a Z-only G0 to the section's stated clearance
// (#1616, below), then a G0 to its X Y alone at that height, and only
// then does Z come down. A rapid positions at its target; a feed or arc
// at the point the stream says it starts from (the target when the
// stream names none); a cycle at its first hole.
var xyUnknown = true;
// #1616 (the #1614 hazard): the height that traverse is made at is
// STATED, never assumed. The lift before T M6 is the machine's safe
// retract or the work-offset clearance, but a manual change, a tool
// setter run or a long tool on a high work offset can leave the tip
// anywhere: a bare X Y rapid there may cross the part. So the first
// traverse is preceded by a Z-only rapid to the section's clearance -
// its ClearancePlane, else its RetractPlane, never below the move's
// own Z. A setup that cannot reach it is stopped by the control's Z
// limit instead of rapiding through the part. A stream that states
// neither plane has no verified height to traverse at: refused by name.
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
  traversedAt = tz;
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  xyUnknown = false;
  return true;
}
// A feed or arc as the first move after a tool change starts, in the
// stream, from the generator's own start point (its clearance height).
// The tool stands at the traverse's clearance, so feeding from there
// would crawl the whole way down: once X Y are positioned, a rapid takes
// Z down to that start height (unless the traverse is already there) and
// the feed runs only the stretch it planned.
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

// A drill cycle's expanded hole chain posts as these plain moves.
function onRapid() {
  startPreamble();
  seeZ(properties.Z);
  if (!tiltOk(properties)) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z)) return;  // #1682: refused - the move is not written
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  seeZ(properties.Z); seeZ(properties.StartZ);
  if (!tiltOk(properties)) return;
  if (spindleStopped("feed move G1")) return;
  if (!startXY(properties)) return;  // #1682: refused - the move is not written
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
  dwellAtBottom(properties.Z);
}
function onCircular() {
  startPreamble();
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  if (!tiltOk(properties)) return;
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G2" : "G3"))) return;
  var p = properties;
  seeZ(p.Z); seeZ(p.StartZ);
  if (!startXY(p)) return;  // #1682: refused - the move is not written
  var g = p.CircularDirection === "CW" ? "G2" : "G3";
  if (optBool("useRadiusArcs", false) && emitArcWithRadius(p, g, fw(p.FeedRate))) return;
  block([
    planeWord(planeOf(p)),
    g,
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}
function emitArcWithRadius(p, g, fwd) {
  var c = p.IJKIncrement || {};
  var axes = p.CircularPlane === "XZ" ? [2, 0, 1] : (p.CircularPlane === "YZ" ? [1, 2, 0] : [0, 1, 2]);
  var u = axes[0], v = axes[1], h = axes[2];
  var from = [p.StartX, p.StartY, p.StartZ], to = [p.X, p.Y, p.Z], center = [c.I, c.J, c.K];
  var org = [orgX, orgY, orgZ];
  if (!has(from[u]) || !has(from[v]) || !has(to[u]) || !has(to[v]) || !has(center[u]) || !has(center[v])) return false;
  var du = from[u] - center[u], dv = from[v] - center[v];
  var r0 = Math.sqrt(du * du + dv * dv);
  var eu = to[u] - center[u], ev = to[v] - center[v];
  var r1 = Math.sqrt(eu * eu + ev * ev);
  if (!(r0 > 0) || !isFinite(r0) || !(r1 > 0) || !isFinite(r1)) return false;
  var r = (r0 + r1) * 0.5;
  if (Math.abs(r0 - r1) > Math.min(0.02, Math.max(0.002, r * 0.005))) return false;
  var a0 = Math.atan2(dv, du), ccw = p.CircularDirection !== "CW";
  var full = Math.abs(to[u] - from[u]) < 1e-5 && Math.abs(to[v] - from[v]) < 1e-5;
  var sweep = 2 * Math.PI;
  if (!full) {
    var a1 = Math.atan2(ev, eu);
    sweep = ccw ? a1 - a0 : a0 - a1;
    if (sweep <= 0) sweep += 2 * Math.PI;
  }
  // #2191: near-180 deg arcs lose center accuracy and can put chord > 2R
  // after rounding; near-360 deg arcs collapse or cannot take one R.
  // Split into quadrant-sized segments and verify every posted segment.
  var nearPi = Math.PI / 9;
  var n = 1;
  if (full || sweep >= 2 * Math.PI - nearPi) n = 4;
  else if (Math.abs(sweep - Math.PI) <= nearPi) n = 2;
  function buildSegs(count) {
    var segs = [], dir = ccw ? 1 : -1, prev = [from[0], from[1], from[2]];
    var rSigned = sweep / count > Math.PI + 1e-6 ? -r : r;
    var rp = Number(fu(r));
    if (!(rp > 0) || !isFinite(rp)) return null;
    for (var k = 1; k <= count; k++) {
      var next;
      if (k === count) {
        next = [to[0], to[1], to[2]];
      } else {
        var ak = a0 + dir * sweep * (k / count);
        next = [from[0], from[1], from[2]];
        next[u] = center[u] + r * Math.cos(ak);
        next[v] = center[v] + r * Math.sin(ak);
        if (has(from[h]) && has(to[h])) next[h] = from[h] + (to[h] - from[h]) * (k / count);
        else if (has(to[h])) next[h] = to[h];
      }
      var su = Number(fu(prev[u] - org[u])), sv = Number(fu(prev[v] - org[v]));
      var pu = Number(fu(next[u] - org[u])), pv = Number(fu(next[v] - org[v]));
      var qdu = pu - su, qdv = pv - sv;
      var chord = Math.sqrt(qdu * qdu + qdv * qdv);
      if (!(chord > 0) || chord > 2 * rp - 1e-7) return null;
      if (count === 1 && chord > 2 * rp * 0.99) return null;
      segs.push({ pt: next, rSigned: rSigned });
      prev = next;
    }
    return segs;
  }
  var segs = buildSegs(n);
  if (!segs && n === 1) {
    segs = buildSegs(Math.max(2, Math.ceil(sweep / (Math.PI / 2))));
  }
  if (!segs) return false;
  for (var i = 0; i < segs.length; i++) {
    var pt = segs[i].pt;
    var words = i === 0 ? [planeWord(planeOf(p)), g] : [g];
    words.push(wo("X", pt[0], orgX), wo("Y", pt[1], orgY), wo("Z", pt[2], orgZ), w("R", segs[i].rSigned), fwd);
    block(words);
  }
  return true;
}

// ------------------------------------------------------- drilling
// Grbl 1.1 has no canned cycles (G80 is the only word of that group in
// its "Commands" list; G81-G89 are refused like any other unlisted G
// code), so a CYCLE record writes nothing and the hole's expanded chain
// posts as the plain moves it is - the generator's own rapid onto the
// retract plane, plunges, peck retracts and exit, the exact motion the
// simulation verified (as tinyg.cps does). A dwell becomes `G4 P<seconds>`
// (Grbl 1.1 reads P as seconds) right after the move that reaches the
// hole's bottom. A bore needs the spindle stopped at the bottom before
// the rapid out, which the expanded chain does not carry, and a tap
// needs the spindle reversed in step with the feed out: both are
// refused by name.
var DWELL_TOL = 1e-6;
var pendingDwell = null; // {z, s}: the open hole's dwell, owed at its bottom
function onCycle() {
  startPreamble();
  owedDwell();
  var p = properties;
  seeZ(p.R);
  if (p.CycleType === "BORE") {
    msg(coolantWho() + "bore (G86) requires a spindle stop before rapid retract - Grbl 1.1 has no canned cycles and expanded boring is not supported by this post; refused, not posted");
    return;
  }
  if (p.CycleType === "TAP") {
    if (!tapRefused) {
      tapRefused = true;
      msg("tapping (G84) is not in Grbl 1.1's G-code set and cannot be plain moves - refused, not posted; use a tapping head with a plain drill cycle, or thread milling");
    }
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

// Probing: Grbl 1.1 has G38.2-G38.5, but the result test, the stored trip
// and the offset arithmetic a probe cycle needs are parameters and
// expressions it does not have. So this post defines no probe hook
// and the engine refuses a probing operation by name.

// ------------------------------------------------------------ footer
// Z straight up in machine coordinates, then M30 (no % fence). M30 ends
// the program the RS-274 way; the unit is not reset by it, so the user's
// post option `EndUnits` ("inch" / "mm"; absent = leave the program's
// unit) names the unit the control should be left in, written after the
// last move, right before M30.
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

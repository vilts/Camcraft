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
// G93/G94 sections - behaviour only. Wrapped A (#796) from PathPilot Hub
// readings (v2.14.4, 770MX SIM) recorded on issue #796. The hook contract from Autodesk's published post docs. No
// vendor post script text and no CAM post's output used; original work.
// Structure follows our own haas.cps; the goldens lock this file's
// decisions, not any vendor's.
description = "Tormach PathPilot (3-axis mill, A indexing and wrapped A about X (G93), G20/G21 from the machine, G73/G81/G82/G83/G84/G86/G89, G2/G3, G99, G38.2 probing)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
axisInversion = true; // verified on the engine's mirrored stream for a flipped WCS X/Y and an inverted machine X/Y (#2475): trace equivalence test + golden
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 9;      // LinuxCNC G-code reference: G54-G59.3 select coordinate systems 1-9 (#931)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
endUnits = "G20 G21";     // #596 / #2189: switches end units before M30 when asked
coolantModes = true;      // #3446: reads PARAM Coolant (#112/#150)

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
afterModes = true; // #1532: reads PARAM AfterOperation (stop / optional stop) and PARAM SetupChange; a trip home is refused by name
rotariesBeforeTraverse = true; // #3100: each section turns A at the safe Z (G53 G0 Z, then G0 A) before its first X/Y traverse (indexFor, xyFirst); pinned by posting_rotaries_first.rs
uprightReturn = true; // #3477: the same G53 G0 Z, then G0 A turn writes A0 at a section after a tilted section; pinned by upright_after_tilt.rs
explains = "paren"; // #1387: LinuxCNC parenthesized comments; shared helper keeps them inert
// Original wording; inherited mill meanings cite RS-274 / LinuxCNC in explain.rs.
explainWords = {
  "G38.2": "PROBE TOWARD CONTACT", // LinuxCNC G-code reference, G38.2-G38.5: probing toward contact
  "G59.1": "WORK OFFSET 7", // LinuxCNC G-code reference, G54-G59.3: coordinate system 7
  "G59.2": "WORK OFFSET 8", // LinuxCNC G-code reference, G54-G59.3: coordinate system 8
  "G59.3": "WORK OFFSET 9"  // LinuxCNC G-code reference, G54-G59.3: coordinate system 9
};

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var tNo = 0;
var loadedTool = null;     // #1626: only the tool this program emitted M6 for is known
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var clearZ = null;         // #1614: the section's ClearancePlane, null when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#1532), none when unstated
var setupNext = "";        // the section's PARAM SetupChange (#1532): the setup the program changes to after it
var coolant = "";          // the M-code this program started, if any
var coolantAsk = "flood";  // #3446: the section's PARAM Coolant, flood when unstated
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var wcsFlip = "";          // #1649: the flipped WCS axes ("X", "Y", "XY") - the engine mirrored the section
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var inch = false;          // the document's unit is inch (onOpen, #902: postOptions.Units, never the machine file's)

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch program gets every length and
// feed divided by 25.4 and written at four decimals (a tenth of a
// thousandth); a metric one keeps mm at three. Angles and rpm never scale.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the program's unit
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
// #3699: a tap feed is pitch x rpm, kept at 0.001 mm/min by the stream, and
// the control cuts the pitch F / S. G84 writes the feed at three mm or four
// inch decimals; a tap whose written F and S do not hold the pitch to 0.01
// percent is refused by name.
function tapFeed(v) { return fu(v); }
function tapPitchHolds(feed) {
  var speed = Math.round(sRpm);
  var written = Number(fu(feed)) * (inch ? 25.4 : 1);
  var want = sRpm > 0 ? feed / sRpm : 0;
  var got = speed > 0 ? written / speed : 0;
  if (!(want > 0 && Math.abs(got - want) / want <= 1e-4)) {
    msg("canned cycle G84 (tap)" + inOp() + " feed " + feed + " mm/min at " + sRpm +
      " rpm posts as F" + tapFeed(feed) + " S" + speed + " - refused: the written pitch differs by more than 0.01 percent");
    return false;
  }
  return true;
}
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
    var c = explain(a);
    var prefix = "";
    if (!commentOnly) {
      var mode = optStr("showSequenceNumbers", "off");
      if (mode === "every-block" || ((mode === "tool-change" || mode === "op-start") && seqNextForce)) {
        prefix = nextSeq() + " ";
        seqNextForce = false;
      }
    }
    writeln(prefix + body + (c ? " " + c : ""));
  }
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
function xyz(p) {
  if (indexed()) {  // #785: the whole tip, turned into the indexed frame
    var q = tip(p.X, p.Y, p.Z);
    return ["X" + fu(q[0]), "Y" + fu(q[1]), "Z" + fu(q[2])];
  }
  return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)];
}
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function arcWord(k, v) { return wo(k, v, k === "X" ? orgX : (k === "Y" ? orgY : orgZ)); }  // #3442: a position word as written, for arcFate
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}
var tapRefused = false;   // the rigid-tap refusal, once per program

// #1357: a cut needs a turning spindle. spindleOn is true from the
// preamble's S.. M3 to the next M5 (every section end); a touch probe's
// section feeds its stylus with the spindle stopped on purpose. A
// section is a probe's only when it states 0 rpm (#574) AND its tool is
// a touch probe (PARAM ToolKind) - a cutter whose speed reads 0 is not
// a probe, it is a stopped spindle. A feed, arc or cycle with the
// spindle stopped anywhere else is refused by name - a stream that
// states no spindle speed (a bare legacy .cl) would otherwise post a
// program that drags a stopped cutter through the part. Once per program.
var spindleOn = false;
var probeSection = false;
var kindProbe = false;    // this section's PARAM ToolKind names a touch probe
var spindleRefused = false;
function spindleStopped(what) {  // true: refused, nothing is posted
  if (spindleOn || probeSection) return false;
  if (!spindleRefused) {
    spindleRefused = true;
    msg(what + inOp() + " with the spindle stopped - no spindle start (S.. M3) precedes it; state the operation's spindle speed");
  }
  return true;
}
var feedRefused = false;  // #1357: the cycle-feed refusal, once per program

// The .mch tool-station coolant list carries names + M-codes. A flood ask
// takes the machine's Flood entry; failing one, the first coded entry of
// no other known family (a shop's own "Brine" is its flood); a stated
// codeless Flood is the operator's own valve, silent. A list that states
// only other kinds (Mist, Air - #869) has no flood code: refused by name,
// never posted as mist or air. An unstated list stays silent. M9 (stop)
// is the RS-274 fixed pair.
var COOLANT_KINDS = ["flood", "mist", "air", "air-blast", "through-spindle", "through-tool"];
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
  if (other || floodNamed || cs.length === 0) return other;
  msg("the machine states no flood coolant code" + inOp() + " - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "";
}
// #3446: the op's coolant ask (#112) - off writes no coolant word; mist,
// air or through-tool take the machine's own entry of that family,
// refused by name (naming the operation) when the machine states none.
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : ask === "air" ? ["air", "air-blast"]
    : ask === "through-tool" ? ["through-spindle", "through-tool"] : [];
  if (want.length === 0) { msg("unknown coolant ask " + ask + inOp()); return ""; }
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    if (code && want.indexOf(coolantName(c)) >= 0) return code;
  }
  msg("the machine states no " + ask + " coolant code" + inOp() + " - pick another coolant for the operation or add it to the machine");
  return "";
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

// -------------------------------------------------------------- header
// The document's program identity (#258), from the engine's `program`
// global: its number (1 when the document states none; one file per
// operation counts up from it, #111), whether the document stated it,
// and its program comment ("" for none - the header then names the
// machine, as before #258). A host that publishes none posts O0001.
function progNo() {
  var n = typeof program !== "undefined" ? program.number : (typeof programNumber !== "undefined" ? programNumber : 1);
  return Math.round(Number(n) || 1);
}
function progNumbered() { return typeof program !== "undefined" && program.numbered === true; }
function progComment() { return clean(typeof program !== "undefined" ? program.comment : ""); }
// No O-word (#258): PathPilot runs the LinuxCNC interpreter, where an O
// word opens a subroutine or a flow-control block (LinuxCNC G-code
// reference, "O Codes") - a program number on a block of its own is not
// a legal line. A stated number rides the header comment instead.
function headTitle(nm) {
  var t = progComment() || clean(nm) || "CAMCRAFT";
  return progNumbered() ? "PROGRAM " + progNo() + " - " + t : t;
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
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  // #594: the machine name is user text - cleaned like every other
  // comment text, so `Tormach 770MX (ML)` never nests.
  note(headTitle(nm));
  invertedNotes();
  writeHeaderExtras();
  block([inch ? "G20" : "G21", "G17", "G90", "G91.1", "G94", "G40", "G49", "G80"]);
  block(["G64", blendP()]);
  var z = safeZ();
  if (z !== null && z !== 0) block(["(SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE)"]);
}
// The machine's inverted linear axes (#375): the engine has mirrored the
// stream into the controller's frame, and the header says so, one note
// for each axis. A standard machine lists none.
function invertedNotes() {
  var a = (typeof machine !== "undefined" && machine && machine.invertedAxes) || [];
  var where = { X: "LEFT", Y: "TOWARD THE FRONT", Z: "DOWN" };
  for (var i = 0; i < a.length; i++) {
    var l = String(a[i]);
    note("MACHINE " + l + "+ MOVES " + (where[l] || "THE OTHER WAY") + " - INVERTED AXIS");
  }
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

// Z straight up in machine coordinates, the one retract both the tool
// change and the program end start with (#1594 split them: only the
// tool change goes on to G30, see onClose).
function liftZ() {
  var z = safeZ();
  if (z === null) return null;
  block(["G53", "G0", "Z" + fu(z)]);
  return z;
}

// Before a tool change: liftZ, then G30 to the operator's preset
// tool-change spot. The G30 rides PathPilot's convention with the Z0
// retract: a machine that states a different safe Z is not on it, and an
// unset G30 position (LinuxCNC's default is the machine origin) could
// drive Z back down - so there the retract stands alone and M6 / the
// control's own tool-change position does the rest.
function retract() {
  var z = liftZ();
  if (z === null) return;
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
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;  // #1614
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }  // #3446
  commentParam(name, value);
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "WcsFlip") wcsFlip = String(value || "");
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
  var secIdx = secStarted;
  secStarted += 1;
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  // #574: a touch probe (a stated speed of 0) states its spindle
  // stopped before its first move, whatever the tool change did.
  var probeStop = spindleWait && !(sRpm > 0);
  opNotes();
  // #1649: the operator reads why every sign on a flipped axis is the other one.
  if (wcsFlip) note("WCS " + wcsFlip.split("").join(" AND ") + " FLIPPED - PROGRAM MIRRORED");
  block(["G17", "G90", wcsWord, "G40", "G49", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    // The initial spindle tool is unknown. Only a repeated tool that this
    // program already loaded can skip the trip to G30 and the tool change.
    if (loadedTool !== tNo) {
      retract();
      if (secIdx > 0 && optBool("optionalStop", false)) block(["M1"]);
      if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
      block(["T" + ni(tNo), "M6"]);
      loadedTool = tNo;
    }
    xyUnknown = true;        // restate clearance after G43/lift, even without a change
    block(["G43", "H" + ni(tNo)]);
    // #1596: Z straight up again (G53, so the G43 just applied does not
    // shift it) before the section's first X/Y or A move. The G30 above
    // leaves Z at the operator's stored G30 height, and one stored below
    // the part top (a PathPilot Hub run had machine Z -1.5611) would
    // otherwise carry the tool sideways at that height into the part or
    // the vise. Also after a non-G30 retract: M6 may move Z.
    liftZ();
    toolWait = false;
  }
  // A touch probe (#108, #362) states 0 rpm: no spindle start, no belt
  // note and no cutting fluid on a probe stylus.
  var touchProbe = spindleWait && !(sRpm > 0);
  if (spindleWait && sRpm > 0) {
    block([beltNote(sRpm)]);
    block(["S" + ni(sRpm), "M3"]);
    spindleOn = true;
  }
  if (probeStop) { block(["M5"]); spindleOn = false; }
  probeSection = touchProbe && kindProbe;
  spindleWait = false;
  coolant = touchProbe ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  smoothOn();
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // the plane never leaks past its section
  g94();                     // #796: no section leaves G93 behind
  sectionOpen = false;
  aSection = null;           // #785: the next section indexes afresh
  aNow = null;               // ... and restates A: a section boundary is
                             // where an operator may stop and jog (M0/M1,
                             // a probe check), so no section inherits A
  smoothOff();
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  coolantAsk = "flood";      // #3446: the next section states its own ask
  block(["M5"]);
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
  afterOp();
}

// The op's after-action (#1532), written once its spindle has stopped:
// M1 / M0 from PARAM AfterOperation. At a setup change (PARAM
// SetupChange) Z goes straight up first and a PathPilot (MSG, ..) names
// the next setup, the way the probe check asks the operator (UM10587
// ch. 10, M0/M1; LinuxCNC "Messages"). A trip home is not written: the
// G30 spot is the operator's preset, so it is refused by name.
function afterOp() {
  var ask = afterAsk;
  var next = setupNext;
  afterAsk = "";
  setupNext = "";
  if (!ask) return;
  if (ask !== "optional-stop" && ask !== "stop") {
    msg(ask === "home"
      ? "a trip home is asked after the operation" + inOp() + " - this post does not write one (the G30 spot is the operator's preset); set it to Pause (M0) or Continue"
      : "unknown after-operation ask " + ask);
    return;
  }
  if (next) {
    liftZ();
    block(["(MSG, SETUP CHANGE - NEXT: " + next + ")"]);
  }
  block([ask === "stop" ? "M0" : "M1"]);
  // The operator may remove the tool at either stop, even an optional one.
  loadedTool = null;
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); wcsFlip = ""; hopZ = null; clearZ = null; }  // #1028: the next section states its own hop

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
//   past the stated travel, an indexed arc (a tilted path is chords),
//   a tilted move on a machine with no A about X, and a move with no A
//   word inside an operation indexed away from A0 (#2473).
// - An A that changes inside an operation (simultaneous 4-axis, #796) is
//   posted as wrapped A - see "Simultaneous A" below.
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
// A later record whose angle differs turns A inside the section: aNext is
// then set to the unwrapped target ({a, word}) for onRapid / onLinear to
// post (turnMove); any other motion (`what` names it) is refused by name
// when it would turn A, and so is a record with no A word while the
// section holds an A other than 0 (#2473).
var aNext = null;
function indexFor(p, what) {
  aNext = null;
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
    // aSection is the unwrapped tool angle the control holds; the turn to
    // `a` is taken the short way (-180..180), so A counts on through 360.
    var d = a - aSection;
    d = d - 360 * Math.round(d / 360);
    if (Math.abs(d) <= A_EPS) return true;
    // #2473: a record with no A word does not state a turn. The stream
    // reads a move without an axis as "keep the pose" (post::dump), so a
    // turn to A0 here would swing the part under the tool. Refused by name.
    if (!tilted) {
      return aRefuse((what || "move") + inOp() + " states no A angle while the operation holds A" + fa(ax.sign * aSection) + " - the post turns A inside an operation only on a move that states its A angle");
    }
    if (what) {
      return aRefuse(what + inOp() + " turns A - the post writes a turning A on straight G0 / G1 moves only");
    }
    if (!(rotaryRate("A") > 0)) {
      return aRefuse("simultaneous 4-axis motion" + inOp() + " (A changes inside the operation) - " + mName() + " states no A feed rate (the A axis's max_normal_speed), so the post cannot hold it");
    }
    var to = aSection + d, tw = ax.sign * to;
    if (ax.travel && (tw < ax.travel[0] - A_EPS || tw > ax.travel[1] + A_EPS)) {
      return aRefuse("A" + fa(tw) + inOp() + " is beyond the A axis travel " + fa(ax.travel[0]) + ".." + fa(ax.travel[1]) + " degrees - A counts on through 360 (PathPilot does not roll A over), so the operation turns too far in one direction; use a machine with verified A travel wide enough for the whole operation. For Rotary Offset, cutting levels cover two laps plus seam padding");
    }
    aNext = { a: to, word: tw };
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

// ---------------------------------------------- simultaneous A (#796)
// An A that turns inside an operation (Rotary Parallel, a wrapped
// contour) is posted from the readings mkinney took in the PathPilot Hub
// (v2.14.4, 770MX SIM with the 4th axis on, recorded on issue #796):
// 1. A is linear degrees with no rollover: from A350, G1 A10 turned BACK
//    340 degrees and read 10.000. So A is written cumulative: each target
//    is the equivalent of the stream's angle nearest the held one (359 ->
//    361, never back through 0), and the stated travel bounds the count
//    (refused by name past it - the control will not wrap it back).
// 2. G94 on an X+A move: F is the X Y Z distance only, A follows in the
//    same time; on an A-only move F is degrees per minute.
// 3. G93 inverse time is accepted and timed (G93 G1 X4 A180 F5: 12 s).
// So a move that turns A and moves X Y Z is written in G93, F = 1 / its
// time in minutes; the time is the longer of the tool tip's path
// RELATIVE TO THE PART at the programmed feed and the A turn at the
// machine's stated A rate (`rotaryRate`, the #885 rule) - neither the
// cutting feed nor the A rate is passed. The path is bounded from the
// rounded posted XYZ/A trajectory, with one extra length step of slack,
// and F is floored, never rounded: rounding only slows a move. A
// move that turns A alone (its X Y Z words unchanged) stays in G94 with
// F in degrees per minute under the same time law; a move that does not
// turn A is the ordinary G94 move. G94 is restated before any motion that
// is not a G93 feed, and at the section's end. A machine that states no
// A rate is refused by name. Rapids turn A with G0 (no F).
// The machine moves X Y Z in a straight line while A turns, so the tip
// relative to the part bows off the stream's straight segment: each
// turning move is split into 2^k equal pieces until that bow, sampled at
// a quarter, half and three quarters of every piece, is within TURN_TOL
// (at most TURN_MAX pieces, else refused by name).
// Between operations A is reset, never carried: each section restates its
// first angle as -180..180 (G53 G0 Z to the safe Z, then G0 A - the
// index above), so a wrapped operation that ended at A370 unwinds at the
// top of Z before the next operation starts.
var TURN_TOL = 0.005;    // mm (0.0002 in)
var TURN_MAX = 256;
var g93 = false;         // the control is in G93 (inverse time)
function g94() { if (g93) { block(["G94"]); g93 = false; } }
// The motion gate every motion hook calls: indexFor, then G94 back for
// any motion that will not be a turning move.
function motionFor(p, what) {
  if (!indexFor(p, what)) return false;
  if (!aNext) g94();
  return true;
}
// tip() at an explicit tool angle, and its inverse (machine words back
// to the part frame, world coordinates).
function tipAt(x, y, z, ang) {
  var rx = x - orgX, ry = y - orgY, rz = z - orgZ;
  var t = ang * Math.PI / 180, c = Math.cos(t), s = Math.sin(t);
  return [rx, ry * c + rz * s, -ry * s + rz * c];
}
function partAt(m, ang) {
  var t = ang * Math.PI / 180, c = Math.cos(t), s = Math.sin(t);
  return [m[0] + orgX, m[1] * c - m[2] * s + orgY, m[1] * s + m[2] * c + orgZ];
}
function lerp3(a, b, u) { return [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u]; }
function dist3(a, b) {
  var dx = a[0] - b[0], dy = a[1] - b[1], dz = a[2] - b[2];
  return Math.sqrt(dx * dx + dy * dy + dz * dz);
}
// The largest bow (mm) of the move s -> e turning a0 -> a1, split in n.
function turnBow(s, e, a0, a1, n) {
  var worst = 0;
  for (var i = 0; i < n; i++) {
    var u0 = i / n, u1 = (i + 1) / n;
    var p0 = lerp3(s, e, u0), p1 = lerp3(s, e, u1);
    var b0 = a0 + (a1 - a0) * u0, b1 = a0 + (a1 - a0) * u1;
    var m0 = tipAt(p0[0], p0[1], p0[2], b0), m1 = tipAt(p1[0], p1[1], p1[2], b1);
    for (var k = 1; k <= 3; k++) {
      var tau = k / 4;
      var q = partAt(lerp3(m0, m1, tau), b0 + (b1 - b0) * tau);
      var d = dist3(q, lerp3(p0, p1, tau));
      if (!(d <= worst)) worst = d;   // a NaN reads as the worst
    }
  }
  return worst;
}
function floorWord(letter, v) {  // v floored to 0.001; "" when under 0.001
  var t = Math.floor(Number(v) * 1000 + 1e-9) / 1000;
  return (t >= 0.001 && isFinite(t)) ? letter + trim(t.toFixed(3)) : "";
}
// Bound the length of q(u) = R(b0 + w*u) * (m0 + v*u), 0 <= u <= 1,
// in the part frame. Rotation preserves norms. Its second derivative is
// R(2*w*J*v + w*w*J*J*m), so |q''| <= M below (the radius of a straight
// segment is at most its largest endpoint radius). Hence
// |q'(u)| <= |q'(1/2)| + M*|u-1/2|, whose integral is |q'(1/2)| + M/4.
// Use the actual rounded words, including the machine's A sign and inch
// conversion: neither a stream chord nor a rounding allowance bounds
// the curved trajectory when the stream endpoints nearly coincide.
function turnLength(x0, y0, z0, x1, y1, z1, b0, b1) {
  var unit = inch ? 25.4 : 1;
  var m0 = [Number(x0.slice(1)) * unit, Number(y0.slice(1)) * unit, Number(z0.slice(1)) * unit];
  var m1 = [Number(x1.slice(1)) * unit, Number(y1.slice(1)) * unit, Number(z1.slice(1)) * unit];
  var vx = m1[0] - m0[0], vy = m1[1] - m0[1], vz = m1[2] - m0[2];
  var w = (b1 - b0) * Math.PI / 180;
  var dy = vy - w * (m0[2] + m1[2]) / 2;
  var dz = vz + w * (m0[1] + m1[1]) / 2;
  var r0 = Math.sqrt(m0[1] * m0[1] + m0[2] * m0[2]);
  var r1 = Math.sqrt(m1[1] * m1[1] + m1[2] * m1[2]);
  var bound = 2 * Math.abs(w) * Math.sqrt(vy * vy + vz * vz) + w * w * Math.max(r0, r1);
  return Math.sqrt(vx * vx + dy * dy + dz * dz) + bound / 4;
}
// Posts a G0 / G1 that turns A to aNext. False when refused by name.
function turnMove(p, rapid) {
  var ax = aRotary();
  if (xyUnknown || !ax || !(has(p.StartX) && has(p.StartY) && has(p.StartZ) && has(p.X) && has(p.Y) && has(p.Z))) {
    return aRefuse("a move that turns A" + inOp() + " from a start point the post does not know - its path and time cannot be judged");
  }
  var f = Number(p.FeedRate);
  if (!rapid && !(f > 0)) return aRefuse("a feed that turns A" + inOp() + " has no feed rate");
  var s = [Number(p.StartX), Number(p.StartY), Number(p.StartZ)], e = [Number(p.X), Number(p.Y), Number(p.Z)];
  var a0 = aSection, a1 = aNext.a;
  var n = 1;
  while (n < TURN_MAX && !(turnBow(s, e, a0, a1, n) <= TURN_TOL)) n *= 2;
  var bow = turnBow(s, e, a0, a1, n);
  if (!(bow <= TURN_TOL)) {
    return aRefuse("a move that turns A" + inOp() + " still bows " + fu(bow) + " off its path split " + TURN_MAX + " times - shorten the toolpath's segments");
  }
  var rate = rotaryRate("A");
  var slack = (inch ? 0.00254 : 0.001) * Math.sqrt(3);
  var bPrev = a0;
  var m0 = tipAt(s[0], s[1], s[2], a0);
  var xw0 = "X" + fu(m0[0]), yw0 = "Y" + fu(m0[1]), zw0 = "Z" + fu(m0[2]);
  for (var i = 1; i <= n; i++) {
    var u = i / n;
    var p1 = i === n ? e : lerp3(s, e, u);
    var b1 = i === n ? a1 : a0 + (a1 - a0) * u;
    var m = tipAt(p1[0], p1[1], p1[2], b1);
    var xw = "X" + fu(m[0]), yw = "Y" + fu(m[1]), zw = "Z" + fu(m[2]);
    var aw = "A" + fa(ax.sign * b1);
    var dA = Math.abs(Number(fa(ax.sign * b1)) - Number(fa(ax.sign * bPrev)));
    if (rapid) {
      g94();
      block(["G0", xw, yw, zw, aw]);
    } else if (!(dA > 0)) {                   // the turn rounds away: plain G94
      g94();
      block(["G1", xw, yw, zw, fw(f)]);
    } else {
      var length = turnLength(xw0, yw0, zw0, xw, yw, zw,
        Number(fa(ax.sign * bPrev)) / ax.sign, Number(fa(ax.sign * b1)) / ax.sign);
      var t = Math.max((length + slack) / f, dA / rate);  // minutes
      var pure = xw === xw0 && yw === yw0 && zw === zw0;
      var word = floorWord("F", pure ? dA / t : 1 / t);
      if (!word) {
        return aRefuse("a feed that turns A" + inOp() + " would take over 1000 minutes - its F word cannot be written");
      }
      if (pure) {
        g94();
        block(["G1", aw, word]);              // F in degrees per minute
      } else {
        block([g93 ? "" : "G93", "G1", xw, yw, zw, aw, word]);
        g93 = true;
      }
    }
    bPrev = b1; xw0 = xw; yw0 = yw; zw0 = zw;
  }
  aSection = a1;
  aNow = ax.sign * a1;
  return true;
}

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
// #1614: the height that traverse is made at is STATED, never assumed.
// The top of Z travel (G53 G0 Z0, #1596) clears the part only while the
// tool is short enough for the work offset: a PathPilot Hub run with a
// 5" T4 and G54 Z zero 4.3" below the top had the tip 0.68" under the
// part top after G43, and the bare X Y rapid crossed the part there.
// So an upright section first rapids to its clearance - the stream's
// ClearancePlane, else its RetractPlane (the hop, #1028), never below
// the move's own Z. (Not StartZ: the engine starts every stream at 0
// when it states no start, so that height is not the stream's.) On a
// setup that can clear, that is an ordinary move down to clearance; on
// one that cannot, the control refuses the block as past its Z limit
// and stops, instead of rapiding through the part. A stream that states
// neither has no verified height to traverse at: refused by name.
function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}
var traverseRefused = false;  // #1614: once per program
var traversedAt = null;       // #1614: the Z the last first-traverse was made at
// False when refused: the caller then writes nothing for its move - nor
// for any later one while X/Y stay unknown (the rest of the section) -
// so no combined X/Y/Z move from an unknown height reaches the text.
function xyFirst(x, y, z) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  traversedAt = null;
  if (!indexed()) {
    var tz = traverseZ(z);
    if (tz === null) {
      if (!traverseRefused) {
        traverseRefused = true;
        msg("first X/Y traverse" + inOp() + " with no clearance height stated (no ClearancePlane or RetractPlane) - the post will not rapid X/Y at an unknown height");
      }
      return false;
    }
    block(["G0", wo("Z", tz, orgZ)]);
    traversedAt = tz;
  }
  if (indexed()) {  // #785: X Y of the turned tip, which Z enters
    if (!has(z)) return true;
    var q = tip(x, y, z);
    block(["G0", "X" + fu(q[0]), "Y" + fu(q[1])]);
  } else {
    block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  }
  xyUnknown = false;
  return true;
}
// A feed or arc as the first move after a tool change starts, in the
// stream, from the generator's own start point - its clearance height
// (a face plunges from START). After the G30 the tool stands at the top
// of Z travel, so feeding from there would crawl the whole stroke at the
// plunge rate. Once X Y are positioned, a rapid takes Z down to that
// start height - where the generator says the tool already is - and the
// feed runs only the stretch it planned (proven on the 770MX, #131).
function startXY(p) {  // false: refused (#1614), the caller writes nothing
  if (!xyUnknown) return true;
  var ok = (has(p.StartX) && has(p.StartY)) ? xyFirst(p.StartX, p.StartY, p.StartZ) : xyFirst(p.X, p.Y, p.Z);
  if (!ok) return false;
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z) {
    if (indexed() && has(p.StartX) && has(p.StartY)) {
      // #785: "above" is the indexed frame's Z, the tool axis.
      var s0 = tip(p.StartX, p.StartY, p.StartZ), e0 = tip(p.X, p.Y, p.Z);
      if (s0[2] > e0[2]) block(["G0", "Z" + fu(s0[2])]);
    } else if (!indexed() && !(has(traversedAt) && Math.abs(traversedAt - p.StartZ) < 1e-9)) {
      // #1614: not when the stated traverse height already is it.
      block(["G0", wo("Z", p.StartZ, orgZ)]);
    }
  }
  return true;
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
  if (chainRapid() || !motionFor(properties, "")) return;
  if (aNext) { turnMove(properties, true); return; }
  if (!xyFirst(properties.X, properties.Y, properties.Z)) return;  // #1614: refused - the move is not written
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (chainCut() || !motionFor(properties, "")) return;
  if (spindleStopped("feed move G1")) return;
  if (aNext) { turnMove(properties, false); return; }
  if (!startXY(properties)) return;  // #1614: refused - the move is not written
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  if (chainCut() || !motionFor(properties, "arc move")) return;
  var p = properties;
  if (indexed()) {  // #785: a tilted path is chords; an arc here is unstated
    aRefuse("arc on an indexed A" + inOp() + " - the indexed frame posts lines only");
    return;
  }
  if (spindleStopped("arc move " + (p.CircularDirection === "CW" ? "G2" : "G3"))) return;
  if (!startXY(p)) return;  // #1614: refused - the move is not written
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
  if (!motionFor(p, "probe move")) return;  // #785: probing is upright (A0 on a 4th axis)
  if (!startXY(p)) return;  // #1614: refused - the move is not written
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
  if (!motionFor(p, "canned cycle")) return;  // #785: a cycle drills upright (A0 on a 4th axis)
  var dwell = has(p.P) && p.P > 0 ? p.P : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = dwell > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "BORE") g = "G86";
  // #2360: a back-bore with no dwell is G85 (feed in, feed out): a G89
  // with no P word would leave the dwell to the control's modal P.
  else if (p.CycleType === "BACKBORE") g = dwell > 0 ? "G89" : "G85";
  else if (p.CycleType === "REAM") g = "G85";   // #1445: feed in, feed out at F, no dwell
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
  else {
    // #1445: a cycle kind this post does not know is refused by name,
    // never posted as the G81 default (a rapid out it never asked for).
    msg("canned cycle " + p.CycleType + " is not known to this post - refused, not posted as another cycle");
    return;
  }
  // #1357: a canned cycle feeds at its own F word. Without one LinuxCNC
  // alarms on a zero feed ("Cannot feed with zero feed rate") or, after
  // an earlier F, drills at whatever feed was last modal - so a cycle
  // the stream gives no positive feed is refused by name.
  if (!(has(p.FeedRate) && p.FeedRate > 0)) {
    if (!feedRefused) {
      feedRefused = true;
      msg("canned cycle " + g + inOp() + " has no feed rate - refused: without F the control drills at whatever feed was last modal");
    }
    return;
  }
  // #2609: PathPilot's and LinuxCNC's G73 and G85 formats have no P word.
  if ((g === "G73" || g === "G85") && dwell > 0) {
    var cycleName = g === "G73" ? "peck" : "ream";
    msg("canned cycle " + g + " (" + cycleName + ") dwell is not supported by this post - a " + dwell + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  // #2348: PathPilot's and LinuxCNC's G83 formats have no P word.
  if (g === "G83" && dwell > 0) {
    msg("canned cycle G83 (deep peck) dwell is not supported by this post - a " + dwell + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  if (spindleStopped("canned cycle " + g)) return;
  if (g === "G84" && !tapPitchHolds(Number(p.FeedRate))) return;
  if (!xyFirst(p.X, p.Y)) return;  // a cycle as the first move: its first hole (#1614: refused - not written)
  cycleSeat(p);
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q;
  // #2360 (as #2350 for Haas): P is sticky in the dwell cycles, so a
  // changed dwell reopens the cycle and each hole posts its own P.
  if (g === "G82" || g === "G86" || g === "G89") sig += "|" + secs(dwell);
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
    // #2362: LinuxCNC / PathPilot requires a non-negative P on the first
    // G86 block of a cycle, even when the dwell is 0 (P0).
    if (g === "G86" || (dwell > 0 && (g === "G82" || g === "G89"))) words.push("P" + secs(dwell));
    words.push(g === "G84" ? "F" + tapFeed(p.FeedRate) : fw(p.FeedRate));
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
  if (!motionFor(p, "probe move")) return;  // #785: probing is upright (A0 on a 4th axis)
  if (!xyFirst(p.X, p.Y)) return;  // never a probe from an unknown X Y (#1614: refused - not written)
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
    // #2683: the write is exact (target = active offset + deviation) only
    // in unrotated systems. With an XY rotation (R of G10 L2, parameter
    // 5210 + 20 x the system number) the deviation lies along rotated
    // axes, and a target with another rotation gets an origin that is
    // right at this position only. The control holds R, so the program
    // tests it and ends by name. A Z-only result needs no test.
    var xy = false;
    for (var q = 0; q < axes.length; q++) if (axes[q].Axis !== "Z") xy = true;
    if (xy) {
      var tgt = Number(p.SetWorkOffset) + 1;
      var rotA = "[#" + (5210 + 20 * wcsIndex) + " NE 0]";
      var g = nextO();
      block([g, "if", tgt === wcsIndex ? rotA : "[" + rotA + " OR [#" + (5210 + 20 * tgt) + " NE 0]]"]);
      block(["(MSG, PROBE: " + wcsWord + (tgt === wcsIndex ? "" : " OR " + workOffsetName(tgt)) + " XY ROTATION NOT ZERO - NO OFFSET WRITTEN)"]);
      block(["M2"]);
      block([g, "endif"]);
    }
    var words = ["G10", "L20", "P" + (Number(p.SetWorkOffset) + 1)];
    for (var j = 0; j < axes.length; j++) {
      words.push(axes[j].Axis + "[#" + (5420 + probeAxis(axes[j].Axis)) + "-#" + (dev + j) + "]");
    }
    block(words);
  } else {
    block(["(MSG, CHECK PROBE RESULT #" + dev + "..)"]);
    block(["M0"]);
    loadedTool = null;       // a probe check gives the operator access to the tool
  }
}

// ------------------------------------------------------------ footer
// G53 G0 Z0 lifts the spindle straight up to the top of Z travel in
// machine coordinates (Tormach's Z home is at the top; another machine
// states its own height, #495 - see safeZ), and the program ends there:
// no G30 follows it and no rotary return (#1594). A G30 with no axis
// words moves every axis to the stored G30 position (LinuxCNC params
// 5181-5189), so an operator's G30 stored below the top of travel
// brought the tool straight back down after the lift. The user's
// PathPilot Hub run (770MX sim, 2026-11-13, stored G30 at machine
// Z -1.5611) showed it: an end of G53 G0 Z0 then G30, or G30 alone, drove
// the tool down below the part top; G53 G0 Z0 alone stayed at the top of
// travel, the ending the user chose. A G0 A0 after the lift was tried
// and not wanted. The G30 before each tool change (retract) is
// unchanged. M30 ends the program (no % fence).
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
  liftZ();               // #1594: straight up and stay there - no G30
  var u = endUnitsWord();
  if (u) block([u]);
  block(["M30"]);
}

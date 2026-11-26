// Generic LinuxCNC 3-axis milling post (#2303). Provenance: the public
// LinuxCNC G-code reference and M-code reference (linuxcnc.org/docs,
// version 2.9: "G Codes", "M Codes", "O Codes", "Comments", "Messages",
// numbered parameters) and RS-274 / ISO 6983 for the words both share -
// behavior only, per this repo's Clean-Room Protocol. The hook contract
// from Autodesk's published post docs. No vendor or CAM post script text
// and no other CAM's output used; original work. Structure follows our
// own tormach.cps (the PathPilot post, which runs the same interpreter),
// with the PathPilot assumptions taken out: no machine Z0-is-the-top
// convention, no G30 before each tool change, no belt-range notes, no
// PathPilot tool-table range, no 4th axis. Coolant handling follows our
// own grblhal.cps. The goldens lock this file's decisions.
description = "LinuxCNC (3-axis mill, G20/G21 from the document, G73/G81/G82/G83/G84 floating/G85/G86/G89, G33.1 rigid tap, G2/G3 with G91.1, G99, G38.2 probing)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "ngc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
axisInversion = true; // verified on the engine's mirrored stream for a flipped WCS X/Y and an inverted machine X/Y (#2475): trace equivalence test + golden
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 9;      // LinuxCNC G-code reference: G54-G59.3 select coordinate systems 1-9
coolantModes = true;    // reads PARAM Coolant (#112/#150)
afterModes = true;      // reads PARAM AfterOperation (stop / optional stop) and PARAM SetupChange; a trip home is refused by name
floatingTapping = true; // a floating-holder tap posts as G84 (LinuxCNC: not spindle-synchronized)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
endUnits = "G20 G21";     // #2189: switches end units before the program end when asked
explains = "paren";       // LinuxCNC parenthesized comments; the shared helper keeps them inert
explainWords = {
  "G33.1": "RIGID TAP, SPINDLE SYNCED", // LinuxCNC G-code reference, G33.1 Rigid Tapping
  "G38.2": "PROBE TOWARD CONTACT",      // LinuxCNC G-code reference, G38.2-G38.5
  "G38.3": "PROBE TOWARD CONTACT, NO ERROR", // LinuxCNC G-code reference, G38.2-G38.5
  "G59.1": "WORK OFFSET 7", // LinuxCNC G-code reference, G54-G59.3: coordinate system 7
  "G59.2": "WORK OFFSET 8", // LinuxCNC G-code reference, G54-G59.3: coordinate system 8
  "G59.3": "WORK OFFSET 9"  // LinuxCNC G-code reference, G54-G59.3: coordinate system 9
};

declaredOptions = {
  retractMode: {
    type: "enum",
    title: "Retract before tool change and at the end",
    group: "Machine",
    description: "How Z goes up before each tool change and at the program end. G53 uses the machine file's safe Z (machine coordinates); G28 / G30 move Z only to the control's stored position; None leaves it to the control (the INI tool-change position or the M6 remap)",
    choices: [
      { id: "g53", title: "G53 G0 Z (machine file's safe Z)" },
      { id: "g28", title: "G28 stored position (Z only)" },
      { id: "g30", title: "G30 stored position (Z only)" },
      { id: "none", title: "None (the control moves)" },
    ],
    default: "g53",
  },
  writeToolLength: { type: "boolean", title: "Write G43 H after M6", group: "Machine", description: "Apply the tool length offset with G43 H<n>; turn off when the M6 remap applies the tool length itself", default: true },
  pathControl: {
    type: "enum",
    title: "Path control",
    group: "Machine",
    description: "G64 P blends within the tolerance; G61 stops exactly at each point",
    choices: [
      { id: "blend", title: "G64 P (blend within tolerance)" },
      { id: "exact", title: "G61 (exact path)" },
    ],
    default: "blend",
  },
  blendTolerance: { type: "number", title: "Blend tolerance (mm)", group: "Machine", description: "The G64 P deviation, in mm (written in the program's unit)", min: 0.001, max: 5, default: 0.025 },
  programEnd: {
    type: "enum",
    title: "Program end",
    group: "Format",
    description: "The word that ends the program",
    choices: [
      { id: "M30", title: "M30" },
      { id: "M2", title: "M2" },
    ],
    default: "M30",
  },
  percentDelimiters: { type: "boolean", title: "% on the first and last line", group: "Format", description: "Write the % file delimiters LinuxCNC reads when the first line is %", default: false },
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
  writeMachine: { type: "boolean", title: "Write machine header", group: "Header & comments", description: "Emit a comment naming the machine in the header", default: false },
  writeToolList: { type: "boolean", title: "Write tool list", group: "Header & comments", description: "Emit a summary comment per tool at program start", default: false },
  optionalStop: { type: "boolean", title: "Optional stop (M1)", group: "Behavior", description: "Write M1 before tool changes after the first operation", default: false },
  useRadiusArcs: { type: "boolean", title: "Use radius (R) arcs", group: "Cycles & arcs", description: "Write R word instead of I/J/K arc centers", default: false },
};

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var wcsIndex = 1;          // the offset number wcsWord writes (#2683: its rotation register)
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var tNo = 0;
var loadedTool = null;     // only the tool this program emitted M6 for is known
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var clearZ = null;         // #1614: the section's ClearancePlane, null when unstated
var afterAsk = "";         // the section's PARAM AfterOperation, none when unstated
var setupNext = "";        // the section's PARAM SetupChange: the setup the program changes to after it
var coolant = "";          // the M-code this section started, if any
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var wcsFlip = "";          // #1649: the flipped WCS axes ("X", "Y", "XY") - the engine mirrored the section
var smoothTol = 0;         // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothActive = false;  // #616: this section wrote its smoothing words
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var inch = false;          // the document's unit is inch (onOpen, #902: postOptions.Units)

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch program gets every length and
// feed divided by 25.4 and written at four decimals; a metric one keeps
// mm at three. Rpm never scales.
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
// the floating G84 cuts the pitch F / S. G84 writes the feed at three mm or
// four inch decimals; a tap whose written F and S do not hold the pitch to
// 0.01 percent is refused by name.
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
function optNum(k, def) {
  return typeof postOptions !== "undefined" && postOptions && typeof postOptions[k] === "number" && isFinite(postOptions[k]) ? postOptions[k] : def;
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
    var body = a.join(" ");
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
var secOp = "";            // the section's operation name, for refusals
function inOp() { return secOp ? " in operation " + secOp : ""; }

// #1357: a cut needs a turning spindle. spindleOn is true from the
// preamble's S.. M3 to the next M5 (every section end); a touch probe's
// section (0 rpm AND a touch-probe tool kind) feeds its stylus with the
// spindle stopped on purpose. A feed, arc or cycle with the spindle
// stopped anywhere else is refused by name, once per program.
var spindleOn = false;
var probeSection = false;
var kindProbe = false;
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
var tapRefused = false;   // the rigid-tap refusal, once per program

// ------------------------------------------------------------ coolant
// The .mch tool-station coolant list carries names + M-codes (LinuxCNC's
// M7 mist, M8 flood, M9 off). A flood ask takes the machine's Flood
// entry; failing one, the first coded entry of no other known family; a
// stated codeless Flood is the operator's own valve, silent. A list that
// states only other kinds has no flood code: refused by name (#150). An
// unstated list stays silent (the unstated-limit law).
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
  if (other || floodNamed) return other;
  if (cs.length > 0) msg("the machine states no flood coolant code" + inOp() + " - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "";
}
// The op's coolant ask (#112): flood when unstated, or off / mist /
// through-tool - the machine's own entry of that family, refused by
// name (naming the operation) when the machine states none.
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []);
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
// point (G91.1, stated in the header - LinuxCNC's default, never
// assumed): a shared translation cancels in an increment, so the origin
// must NOT enter them.
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// -------------------------------------------------------------- header
// The document's program identity (#258). No O-word: the LinuxCNC
// interpreter reads an O word as a subroutine or flow-control label
// ("O Codes"), never a program number - a stated number rides the
// header comment instead.
function progNo() {
  var n = typeof program !== "undefined" ? program.number : (typeof programNumber !== "undefined" ? programNumber : 1);
  return Math.round(Number(n) || 1);
}
function progNumbered() { return typeof program !== "undefined" && program.numbered === true; }
function progComment() { return clean(typeof program !== "undefined" ? program.comment : ""); }
function headTitle(nm) {
  var t = progComment() || clean(nm) || "CAMCRAFT";
  return progNumbered() ? "PROGRAM " + progNo() + " - " + t : t;
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// User text is cleaned for this dialect: a parenthesis would end the
// comment early, % is the file delimiter and ; opens a comment, so all
// three become spaces; printable ASCII only, capped at 40 characters.
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
// LinuxCNC reads some comments as commands - (MSG,..) (DEBUG,..)
// (PRINT,..) (LOG..) (PROBEOPEN ..) (ABORT,..) and AXIS's (AXIS,..), the
// keyword in any case ("Comments", "Messages", "Probe Logging",
// "Logging", "Debug Messages"). A comment whose text would open with one
// is led by "- ", so user text stays inert.
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
// The optional % delimiters ("Comments" / file format: a first line of %
// makes the interpreter read to the next %), one identity comment, and
// the modal state every later line relies on: the unit, plane, absolute
// positions, incremental arc centers (G91.1, stated, never assumed),
// feed per minute, compensation, tool length and cycle cancelled. Path
// control on its own line: G64 with a stated P (a bare G64 is "best
// possible speed, no tolerance"), or G61 exact path.
function pathWords() {
  if (optStr("pathControl", "blend") === "exact") return ["G61"];
  return ["G64", "P" + fu(optNum("blendTolerance", 0.025))];
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

function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
  if (optBool("percentDelimiters", false)) writeln("%");
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  note(headTitle(nm));
  invertedNotes();
  writeHeaderExtras();
  block([inch ? "G20" : "G21", "G17", "G90", "G91.1", "G94", "G40", "G49", "G80"]);
  block(pathWords());
  var mode = retractMode();
  if (mode === "g53") {
    var z = safeZ();
    if (z !== null) note("SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE");
  } else if (mode === "g28" || mode === "g30") {
    note("RETRACT TO THE STORED " + mode.toUpperCase() + " Z - SET IT ABOVE THE WORK");
  } else if (mode === "none") {
    note("NO RETRACT WRITTEN - THE CONTROL MOVES Z AT M6");
  }
}

// ------------------------------------------------------------- retract
// #435 / #495: machine Z0 is NOT always the top of travel on a LinuxCNC
// machine (the INI's HOME and limits decide), so this post never writes
// G53 G0 Z0 on its own. The retract is the user's choice (option
// retractMode):
// - g53: G53 G0 Z<the machine file's safe_retract> (machine
//   coordinates, `machine.safeRetractZ`). Unstated - also with no
//   machine at all - is refused by name.
// - g28 / g30: G91 G28 Z0 (or G30) then G90 - the intermediate point is
//   the current Z (an increment of 0), then only Z goes to the position
//   stored in parameters 5163 / 5183. The stored position is the
//   operator's statement; an unset one is the machine origin.
// - none: nothing is written; the control's M6 (the INI tool-change
//   position or a remap) moves the tool.
function retractMode() {
  var m = optStr("retractMode", "g53");
  if (m === "g53" || m === "g28" || m === "g30" || m === "none") return m;
  return "g53";
}
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
  msg((nm ? "machine " + nm + " does not state" : "no machine states") + " its safe Z retract - the LinuxCNC post lifts Z with G53 G0 Z<safe_retract> before each tool change and at the end, and machine Z0 is not always the top of travel; state the Z axis's safe_retract (machine coordinates) in the machine file, or set the post option Retract to G28, G30 or None");
  return safeZValue;
}
// Z straight up, in the chosen form. False when refused (nothing written).
function liftZ() {
  var mode = retractMode();
  if (mode === "none") return true;
  if (mode === "g28" || mode === "g30") {
    block(["G91", mode === "g28" ? "G28" : "G30", "Z0"]);
    block(["G90"]);
    return true;
  }
  var z = safeZ();
  if (z === null) return false;
  block(["G53", "G0", "Z" + fu(z)]);
  return true;
}

// Cutter compensation in the control: not stated by this post -
// refused by name, never a centre path under a Control op.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by the LinuxCNC post - choose In computer");
}

// #931: PARAM WorkOffset arrives as the 1-based offset number (the engine
// refuses one above workOffsetMax by setup name); the word is ours to write.
function workOffsetName(n) {  // offset n -> this control's word; "" when it has none
  if (!(n >= 1 && n <= 9 && n === Math.floor(n))) return "";
  return n <= 6 ? "G" + (53 + n) : "G59." + (n - 6);  // LinuxCNC: G59.1..G59.3 are systems 7-9
}
function offsetWord(value) {
  var n = Number(value), wd = workOffsetName(n);
  if (!wd) { msg("work offset " + value + " - this post expresses work offsets 1-" + workOffsetMax + " only"); return wcsWord; }
  wcsIndex = n;
  return wd;
}

function onParameter(name, value) {
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;  // #1614
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") return;
  commentParam(name, value);
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
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

// LinuxCNC's T word selects a tool-table entry; H on G43 must be a valid
// tool number (the control errors on one it does not hold). A number
// that is not a positive integer has no entry: refused by name.
function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1 || number !== Math.floor(number)) {
    msg("tool number " + ni(number) + " is not a positive whole number - LinuxCNC has no tool-table entry for it");
  }
}

// #616: per-operation smoothing, only when the document turned it on
// (PARAM Smoothing): G64 P is the deviation the trajectory planner may
// take. Written after the preamble, undone at the section's end so no
// tolerance leaks into the next operation.
function smoothOn() {
  if (!(smoothTol > 0)) return;
  block(["G64", "P" + fu(smoothTol)]);
  smoothActive = true;
}
function smoothOff() {
  if (smoothActive) block(pathWords());  // back to the header's path control
  smoothActive = false;
  smoothTol = 0;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: plane, offsets, tool, spindle, coolant. Before a tool
// change Z goes up (liftZ, the user's retract choice), then T<n> M6 runs
// the control's tool change (manual, or an INI remap / ATC), then G43
// H<n> applies the length the tool table holds - left out when the M6
// remap applies it (option writeToolLength); that choice also leaves G49
// out of each section's safe block, so a repeated tool keeps the length
// the remap applied.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  var secIdx = secStarted;
  secStarted += 1;
  var tlo = optBool("writeToolLength", true);
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  // #574: a touch probe (a stated speed of 0) states its spindle
  // stopped before its first move, whatever the tool change did.
  var probeStop = spindleWait && !(sRpm > 0);
  opNotes();
  // #1649: the operator reads why every sign on a flipped axis is the other one.
  if (wcsFlip) note("WCS " + wcsFlip.split("").join(" AND ") + " FLIPPED - PROGRAM MIRRORED");
  block(["G17", "G90", wcsWord, "G40", tlo ? "G49" : "", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    if (loadedTool !== tNo) {
      liftZ();
      if (secIdx > 0 && optBool("optionalStop", false)) block(["M1"]);
      if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
      block(["T" + ni(tNo), "M6"]);
      loadedTool = tNo;
    }
    xyUnknown = true;        // restate clearance after the change, even without one
    if (tlo) block(["G43", "H" + ni(tNo)]);
    // #1596: Z up again before the section's first X/Y move - M6 (a
    // manual change, an ATC remap) may leave Z anywhere.
    liftZ();
    toolWait = false;
  }
  var touchProbe = spindleWait && !(sRpm > 0);
  if (spindleWait && sRpm > 0) {
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
  sectionOpen = false;
  smoothOff();
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
  afterOp();
}

// The op's after-action, written once its spindle has stopped: M1 / M0
// from PARAM AfterOperation. At a setup change (PARAM SetupChange) Z goes
// up first and a (MSG, ..) names the next setup (LinuxCNC "Messages"). A
// trip home is refused by name: G28 / G30 go to stored positions this
// post cannot see.
function afterOp() {
  var ask = afterAsk;
  var next = setupNext;
  afterAsk = "";
  setupNext = "";
  if (!ask) return;
  if (ask !== "optional-stop" && ask !== "stop") {
    msg(ask === "home"
      ? "a trip home is asked after the operation" + inOp() + " - the LinuxCNC post does not write one (the G28/G30 positions are the control's settings); set it to Pause (M0) or Continue"
      : "unknown after-operation ask " + ask);
    return;
  }
  if (next) {
    liftZ();
    block(["(MSG, SETUP CHANGE - NEXT: " + next + ")"]);
  }
  block([ask === "stop" ? "M0" : "M1"]);
  loadedTool = null;         // the operator may remove the tool at either stop
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); wcsFlip = ""; hopZ = null; clearZ = null; }  // #1028: the next section states its own hop

// ------------------------------------------------------------ 3 axes only
// A tilted record (an A, B or C word) has no axis to go to on this
// 3-axis post: refused by name, once per program, never reinterpreted.
var tiltRefused = false;
function upright(p) {
  if (!has(p.A) && !has(p.B) && !has(p.C)) return true;
  if (!tiltRefused) {
    tiltRefused = true;
    msg("tilted move" + inOp() + " - the LinuxCNC post is 3-axis and writes no rotary words");
  }
  return false;
}

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after every tool
// change: the first move from there is preceded by a G0 to its clearance
// height (#1614: the stream's ClearancePlane, else its RetractPlane,
// never below the move's own Z), then a G0 to its X Y alone, and only
// then does Z come down. A stream that states neither plane has no
// verified height to traverse at: refused by name, and the move (with
// every later one while X/Y stay unknown) is not written.
var xyUnknown = true;
function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}
var traverseRefused = false;
var traversedAt = null;
function xyFirst(x, y, z) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  traversedAt = null;
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
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  xyUnknown = false;
  return true;
}
// A feed or arc as the first move after a tool change starts from the
// generator's own start point: once X Y are positioned, a rapid takes Z
// down to that start height, so the feed runs only the stretch planned.
function startXY(p) {  // false: refused (#1614), the caller writes nothing
  if (!xyUnknown) return true;
  var ok = (has(p.StartX) && has(p.StartY)) ? xyFirst(p.StartX, p.StartY, p.StartZ) : xyFirst(p.X, p.Y, p.Z);
  if (!ok) return false;
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z
      && !(has(traversedAt) && Math.abs(traversedAt - p.StartZ) < 1e-9)) {
    block(["G0", wo("Z", p.StartZ, orgZ)]);
  }
  return true;
}

// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only: the hole's XY, at or below its R plane.
// #994: a rapid that leaves it ends the group (G80, then the rapid as the
// stream says), so the program moves between holes at the verified hop.
// A cut leaving the hole is no chain a cycle replaces: refused by name.
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
// #1028: a probing move ends an open cycle group and seats the next.
function probeSeat() {
  seatKnown = true;
  if (!inCycle) return;
  block(["G80"]);
  inCycle = false;
}
function onRapid() {
  startPreamble();
  if (chainRapid() || !upright(properties)) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z)) return;
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (chainCut() || !upright(properties)) return;
  if (spindleStopped("feed move G1")) return;
  if (!startXY(properties)) return;
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  if (chainCut() || !upright(properties)) return;
  var p = properties;
  var g = p.CircularDirection === "CW" ? "G2" : "G3";
  if (spindleStopped("arc move " + g)) return;
  if (!startXY(p)) return;
  if (optBool("useRadiusArcs", false) && emitArcWithRadius(p, g, fw(p.FeedRate))) return;
  // A helix is the same block with the plane's third axis moving.
  block([
    planeWord(planeOf(p)),
    g,
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}
// R-word arcs (option useRadiusArcs): near-180 and near-360 degree arcs
// are split into segments that each take one R, and every posted segment
// is verified against the rounded end points (#2191).
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
// The positioning feeds of a probe cycle are protected moves: G38.3
// ("G38.n Straight Probe": stop on contact, no error without one), then
// #5070 (1 when the probe tripped) is tested in an `o<n> if` block, and a
// trip shows a (MSG,...) and ends the program with M2.
var guardO = 5000;
function onProbeGuard() {
  startPreamble();
  probeSeat();
  var p = properties;
  if (!upright(p)) return;
  if (!startXY(p)) return;
  block(["G38.3", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  guardO += 1;
  block(["o" + guardO, "if", "[#5070 EQ 1]"]);
  block(["(MSG, PROBE HIT ON POSITIONING)"]);
  block(["M2"]);
  block(["o" + guardO, "endif"]);
}

// ------------------------------------------------------- canned cycles
// One block per hole, holes of an unchanged cycle continuing modally
// with bare X Y lines; G99 as each group opens (#994: each hole returns
// to R). LinuxCNC's dwell P is in SECONDS. G73 / G83 need Q > 0 (the
// reference makes Q <= 0 an error).
// Tapping: a rigid tap is G33.1 ("Rigid Tapping"), spindle-synchronized
// - it needs the spindle encoder wired to spindle.N.index-enable, so it
// is written only on a machine that states rigid_tapping true; on any
// other it is refused by name. A floating-holder tap (TappingMode
// floating) is G84, which LinuxCNC documents as NOT synchronized, F =
// rpm x pitch.
function secs(s) { return trim(Number(s).toFixed(3)); }
function cycleSeat(p) {  // false: refused (#1028), nothing is written
  if (!seatKnown) return true;
  var lo = has(p.StartZ) && p.StartZ < p.R - 1e-6;
  var away = !(has(p.StartX) && has(p.StartY)) || Math.abs(p.StartX - p.X) >= 1e-6 || Math.abs(p.StartY - p.Y) >= 1e-6;
  var hi = !has(p.StartZ) || p.StartZ > p.R + 1e-6;
  if (!lo && !away && !hi) return true;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (away) {
    if (!has(hopZ)) {
      msg("a canned cycle is read away from its hole with no rapid to it, and the section states no RetractPlane to traverse at");
      return false;
    }
    var top = Math.max(hopZ, p.R);
    var at = has(p.StartZ) ? p.StartZ : -Infinity;
    if (at < top - 1e-6) { block(["G0", wo("Z", top, orgZ)]); at = top; }
    block(["G0", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    if (at > p.R + 1e-6) block(["G0", wo("Z", p.R, orgZ)]);
    return true;
  }
  if (lo || hi) block(["G0", wo("Z", p.R, orgZ)]);
  return true;
}
function rigidTapOk() {
  if (typeof machine !== "undefined" && machine.rigidTappingStated === true) return true;
  if (!tapRefused) {
    tapRefused = true;
    msg("rigid tapping (G33.1)" + inOp() + " needs a spindle encoder, and " + ((typeof machine !== "undefined" && machine.name) || "this machine") + " does not state one - state rigid_tapping true on the machine's spindle if it has one, or tap with a floating holder (G84) or thread milling");
  }
  return false;
}
function onCycle() {
  startPreamble();
  restorePlane();            // a cycle drills along G17's Z
  var p = properties;
  if (!upright(p)) return;
  var dwell = has(p.P) && p.P > 0 ? p.P : 0; // P is seconds
  var g;
  var rigid = false;
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = dwell > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "BORE") g = "G86";
  // #2360: a back-bore with no dwell is G85 (feed in, feed out): a G89
  // with no P word would leave the dwell to the control's modal P.
  else if (p.CycleType === "BACKBORE") g = dwell > 0 ? "G89" : "G85";
  else if (p.CycleType === "REAM") g = "G85";
  else if (p.CycleType === "TAP") {
    if (p.TappingMode === "floating") g = "G84";
    else {
      if (!rigidTapOk()) return;
      g = "G33.1";
      rigid = true;
    }
  }
  else {
    msg("canned cycle " + p.CycleType + " is not known to the LinuxCNC post - refused, not posted as another cycle");
    return;
  }
  // #1357: a canned cycle feeds at its own F word (G33.1 derives its
  // pitch from it), so a cycle with no positive feed is refused by name.
  if (!(has(p.FeedRate) && p.FeedRate > 0)) {
    if (!feedRefused) {
      feedRefused = true;
      msg("canned cycle " + g + inOp() + " has no feed rate - refused: without F the control drills at whatever feed was last modal");
    }
    return;
  }
  if ((g === "G73" || g === "G83") && !(has(p.Q) && p.Q > 0)) {
    msg("canned cycle " + g + inOp() + " has no positive peck depth Q - LinuxCNC refuses Q of zero or less");
    return;
  }
  // #2609: LinuxCNC's G73 and G85 formats have no P word.
  if ((g === "G73" || g === "G85") && dwell > 0) {
    var cycleName = g === "G73" ? "peck" : "ream";
    msg("canned cycle " + g + " (" + cycleName + ") dwell is not supported by this post - a " + dwell + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  // #2348: LinuxCNC's G83 format has no P word.
  if (g === "G83" && dwell > 0) {
    msg("canned cycle G83 (deep peck) dwell is not supported by this post - a " + dwell + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  if (spindleStopped("canned cycle " + g)) return;
  if (g === "G84" && !tapPitchHolds(Number(p.FeedRate))) return;
  var traversed = xyUnknown;
  if (!xyFirst(p.X, p.Y)) return;  // a cycle as the first move: its first hole
  if (rigid) { rigidTap(p, traversed); return; }
  if (!cycleSeat(p)) return;
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q + "|" + p.P + "|" + p.FeedRate;
  if (inCycle && sig === cycleSig) {
    block([wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
  } else {
    if (inCycle) block(["G80"]);
    block(["G99"]);            // #994: each hole returns to R
    // Q is an increment (peck depth), not a position: no origin shift.
    var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
    if (g === "G73" || g === "G83") words.push(w("Q", p.Q));
    // #2362: LinuxCNC requires a non-negative P on the first G86 block of
    // a cycle, even when the dwell is 0 (P0).
    if (g === "G86" || (dwell > 0 && (g === "G82" || g === "G84" || g === "G89"))) words.push("P" + secs(dwell));
    words.push(g === "G84" ? "F" + tapFeed(p.FeedRate) : fw(p.FeedRate));
    block(words);
    cycleSig = sig;
  }
  cycX = p.X; cycY = p.Y; cycR = p.R;
  seatKnown = true;
  inCycle = true;
}
// G33.1 is one synchronized move, not a modal cycle: from where the tool
// stands to Z at K per revolution, spindle reversed, back to the start
// Z. So the tool is first seated over the hole on its R plane - by the
// stream (cycleSeat), or, as the section's first motion, straight down
// from the traverse height xyFirst stated - and the block names only Z:
// the move stays on the hole axis. K = feed / rpm (the stream's tap feed
// is rpm x pitch), in the program's unit at the control's precision.
function rigidTap(p, traversed) {
  if (!(sRpm > 0)) { msg("rigid tapping (G33.1)" + inOp() + " needs a spindle speed to state its pitch"); return; }
  if (seatKnown) {
    if (!cycleSeat(p)) return;           // on R over the hole, or refused
  } else if (traversed) {
    block(["G0", wo("Z", p.R, orgZ)]);   // xyFirst stood it over the hole at clearance
  } else {
    // The section's first motion with X Y known from the last section:
    // up to the traverse height, over the hole, down to R - the order a
    // canned cycle positions in, with the height stated (#1614).
    var tz = traverseZ(p.R);
    if (tz === null) {
      msg("rigid tapping (G33.1)" + inOp() + " as the section's first move with no clearance height stated (no ClearancePlane or RetractPlane)");
      return;
    }
    block(["G0", wo("Z", tz, orgZ)]);
    block(["G0", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    block(["G0", wo("Z", p.R, orgZ)]);
  }
  var pitch = len(Number(p.FeedRate) / sRpm);
  block(["G33.1", wo("Z", p.Z, orgZ), "K" + trim(pitch.toFixed(inch ? 6 : 4))]);
  cycX = p.X; cycY = p.Y; cycR = p.R;
  cycleSig = "";             // never a modal continuation: each hole seats again
  seatKnown = true;
  inCycle = true;            // the hole's expanded chain is the G33.1's
}

// ------------------------------------------------------------ probing
// LinuxCNC probing ("G38.n Straight Probe", numbered parameters, O-code
// conditionals, comment messages):
// - the touch is G38.2: the CONTROL raises an error and halts when no
//   contact comes, so a probe that never touched never measures air.
// - the trip lands in #5061-#5063 in the coordinate system of the G38
//   (the section's own WCS and unit); a measured trip k is kept in
//   #(100+3k)..#(102+3k).
// - the back-off is incremental (G91 ... G90).
// - a deviation past the tolerance shows a (MSG,...) and ends the program
//   (M2) inside an `o<n> if [...]` / `o<n> endif` block.
// - each axis's deviation is reported as a (DEBUG, ...) comment in the
//   program's unit, then the work offset is written with G10 L20 from
//   the current position #5420-#5422 minus the deviation.
var probeBase = 100;
var probeIf = 900;
function probeAxis(a) { return a === "X" ? 0 : (a === "Y" ? 1 : 2); }
function onProbeMove() {
  startPreamble();
  probeSeat();
  var p = properties;
  if (!upright(p)) return;
  if (!xyFirst(p.X, p.Y)) return;
  block(["G38.2", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  if (has(p.Measure) && p.Measure >= 0) {
    var b = probeBase + 3 * p.Measure;
    block(["#" + b + "=#5061"]);
    block(["#" + (b + 1) + "=#5062"]);
    block(["#" + (b + 2) + "=#5063"]);
  }
}
function onProbeRetreat() {
  probeSeat();
  var p = properties;
  block(["G91", "G1", w("X", p.DeltaX), w("Y", p.DeltaY), w("Z", p.DeltaZ), fw(p.Feed)]);
  block(["G90"]);
}
function nextO() { probeIf += 1; return "o" + probeIf; }
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
    // LinuxCNC evaluates arithmetic only inside brackets; the sign is
    // folded so no `--` is written.
    var ex = a.Expected - org[ai];
    var op = ex < 0 ? "+" : "-";
    block(["#" + (dev + i) + "=[" + mean + op + fu(Math.abs(ex)) + "]"]);
    var o = nextO();
    block([o, "if", "[ABS[#" + (dev + i) + "] GT " + fu(p.Tolerance) + "]"]);
    block(["(MSG, PROBE " + a.Axis + " OUT OF TOLERANCE)"]);
    block(["M2"]);
    block([o, "endif"]);
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
    loadedTool = null;
  }
}

// ------------------------------------------------------------ footer
// The spindle and coolant stopped at the last section's end (M5, M9);
// Z goes up in the chosen form, the end-unit word (#2189: M2 / M30 reset
// much modal state but not the length unit) and the program end (M30 or
// M2, option programEnd), then the closing % when the file opened with one.
function endUnitsWord() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch" && !inch) return "G20";
  if (want === "mm" && inch) return "G21";
  return "";
}
function onClose() {
  endPreamble();
  liftZ();
  var u = endUnitsWord();
  if (u) block([u]);
  block([optStr("programEnd", "M30") === "M2" ? "M2" : "M30"]);
  if (optBool("percentDelimiters", false)) writeln("%");
}

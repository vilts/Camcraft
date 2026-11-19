// Mazak 3X milling post. Provenance: the G-dialect from public
// RS-274/ISO 6983 documentation and the control family's published
// EIA/ISO programming conventions, which follow the Fanuc words (the
// spindle is S.. M3 / M4 and stopped by M5, #1411; behavior only,
// per this repo's Clean-Room Protocol); the hook contract from
// Autodesk's published post docs. No vendor script text; original
// work. Where a public convention leaves a hole, the house style
// (see fanuc.cps) decides, and the conformance goldens lock that
// decision, not any vendor's.
description = "Mazak (3-axis mill, G73/G81/G82/G83/G84/G86/G89, G2/G3, G99)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 6;      // G54..G59; G54.1 P extended offsets not verified here (#931)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
wcsRotation = true;        // #760: writes G68 X0. Y0. R / G69 in Control mode from PARAM WcsYaw & WcsRotationMode
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
  safeRetract: {
    type: "enum",
    title: "Safe retract method",
    group: "Behavior",
    description: "Z retract command before tool changes and at program end",
    choices: [
      { id: "G28", title: "G28 reference return" },
      { id: "G53", title: "G53 machine Z" },
    ],
    default: "G28",
  },
  useRadiusArcs: { type: "boolean", title: "Use radius (R) arcs", group: "Cycles & arcs", description: "Write R word instead of I/J/K arc centers", default: false },
};
millSpindleReverse = true; // reads PARAM MillSpindle (#988, #1411): M4 for a left-hand-cut mill; the engine refuses a reverse mill spindle for a post without it

// #1388: optional explanations; conservative house budgets, not control maxima.
explains = "paren";
commentMax = 60;
explainBlockMax = 80;

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
var sRpm = 0;              // S.. M3 waits for the section: the stream
var spindleWait = false;   // order is PARAMs, TOOL, SECTION
var spinRev = false;       // PARAM MillSpindle reverse (#988): this section's spindle runs M4
var toolWait = false;
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var coolant = "";          // the M-code this program started, if any
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var setupNext = null;      // #1647: null means no marker; a cleaned name may be empty
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var compG = "";            // G41/G42/G40 pending for the next straight block (#39)
var compD = "";            // the D word riding the engage block ("" when the tool call selects it)
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var compNoted = false;     // the section's operator note was written
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var wcsYaw = 0;            // #760: Z rotation (deg) for Control mode (G68)
var wcsRotMode = "";       // #760: "control" | "bake" | ""
var wcsRotActive = false;  // #760: G68 is currently engaged in this section

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
// #902: the program's unit is the document's (postOptions.Units, "mm"
// or "inch"; absent = mm), never the machine file's. The stream speaks
// mm and mm/min: an inch program writes every length / 25.4 at four
// decimals and every feed in in/min at two; angles, seconds and rpm
// carry no length and are written as they come.
var inch = false;
function trimNum(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
// #1836: a length word always carries its decimal point (`Z14.`, never
// `Z14`). An EIA/ISO control of the Fanuc family can read a word with no
// point in its least increment (Fanuc parameter 3401#0 DPI=0: Z14 is
// 0.014 mm). With the point, the control reads the same value whatever
// that setting is.
function pt(s) { return s.indexOf(".") >= 0 ? s : s + "."; }
function fn(v) {  // a length as text (comments): mm at three decimals, or inch at four
  if (!has(v)) return "0";
  return trimNum(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3));
}
function fu(v) { return pt(fn(v)); }  // a length word, with its point
function fa(v) { return pt(has(v) ? trimNum(Number(v).toFixed(3)) : "0"); }  // angle (deg) at 3 decimals with point
function ff(v) {  // a feed: whole mm/min, or in/min at two decimals
  if (!(has(v) && v > 0)) return "0";
  return inch ? trimNum((v / 25.4).toFixed(2)) : String(Math.round(v));
}
function eps() { return inch ? "0.0001" : "0.001"; } // one output step, the probe checks' band
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; }
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
var seqNum = 0;
var seqNextForce = false;
var secStarted = 0;
var safeRetractRefused = false;
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
function rawBlock(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) {
    var line = a.join(" "), c = explain(a);
    if (c && line.length + 1 + c.length <= explainBlockMax) line += " " + c;
    writeln(line);
  }
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
    var line = prefix + body;
    // Omit an explanation that does not fit; never cut executable words.
    if (c && line.length + 1 + c.length <= explainBlockMax) line += " " + c;
    writeln(line);
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
function retractHome(xyToo) {
  if (optStr("safeRetract", "G28") === "G53") {
    if (typeof machine === "undefined" || !has(machine.safeRetractZ)) {
      if (!safeRetractRefused) {
        safeRetractRefused = true;
        msg("safeRetract G53 requires machine.safeRetractZ in the machine definition - refusing to guess Z0");
      }
      return;
    }
    block(["G53", "G00", w("Z", machine.safeRetractZ)]);
    return;
  }
  block(["G91", "G28", "Z0"]);
  if (xyToo) block(["G28", "X0", "Y0"]);
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}

// #1359 (from #1357): a cut needs a turning spindle. spindleOn is true
// from the preamble's spindle start (S.. M3 / M4) to the next M5 (every
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
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Mazak post has no RTCP output"); }
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
var COOLANT_KINDS = ["flood", "mist", "air", "air-blast", "through-spindle", "through-tool"];
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
// older stream), or carries PARAM Coolant off / mist / air (#869) / through-tool. The
// code is the machine's own entry of that family - the machine is truth;
// an ask the machine states no code for is refused by name, naming the
// operation (#150), never silently posted dry.
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "air" ? ["air", "air-blast"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []));
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

// ------------------------------------------------------------ the plane
// M11.2 A: the arc's plane (RS-274 / ISO 6983 G17 XY, G18 XZ, G19 YZ),
// stated modally. G17 is the state every program starts in (the
// control's power-on plane; a preamble that writes G17 restates it),
// so a program whose arcs are all XY writes no new word. A G18/G19 is
// taken back to G17 before the next XY arc, before any canned cycle
// (a drilling cycle runs along the plane's normal axis) and at section
// end (the next tool's length offset must not meet a stale plane).
// Direction needs no translation: the stream's CW/CCW is G2/G3 by the
// same view-from-the-positive-normal rule the control applies.
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
// #391: a control offsets the cutter in the ACTIVE plane, and every mill
// generator compensates in XY - so the block that writes G41/G42/G40
// states G17 when an XZ/YZ arc left G18/G19 modal, and no arc leaves
// G17 while compensation is engaged (refused by name, see onCircular).
function compPlaneWord(cg) { return cg ? planeWord(17) : ""; }
function compPlaneRefuse(g) {
  if (!compOn || g === 17) return false;
  msg("an " + (g === 18 ? "XZ (G18)" : "YZ (G19)") + " arc while cutter compensation is engaged - the control would compensate in that plane; cancel compensation (G40) first");
  return true;
}
// The center words of the arc's plane - I J (XY), I K (XZ), J K (YZ) -
// as increments from the start point: a shared translation cancels in
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
// The program's O-word (#111, #258). Mazak's published EIA/ISO
// programming manuals number an EIA program by its O-word; this post
// writes the four-digit O0001-O9999 every Fanuc-style EIA control in
// the family reads (the wider work-number range is not assumed).
// A number outside the range is refused by name, never clamped onto
// another program's number.
function oWord() {
  var n = progNo();
  if (n < 1 || n > 9999) throw new Error("program number " + n + " is outside O0001-O9999, the control's program numbers");
  var t = String(n);
  while (t.length < 4) t = "0" + t;
  return "O" + t;
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
function note(s) { block(["(" + s + ")"]); }
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
  if (toolDia > 0) desc.push("D" + fn(toolDia));
  if (toolCr > 0) desc.push("R" + fn(toolCr));
  note(joinDash(["T" + ni(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// #901: the unit this post writes stated in a block of its own before
// any motion, so a control left in the other unit by power-on or the
// last program never misreads the numbers (RS-274 / ISO 6983: G20 inch,
// G21 mm; the family's EIA/ISO conventions follow the Fanuc words, the
// unit word in a block of its own at the program's start). #902: the
// unit is the document's.
function unitBlock() {
  block([inch ? "G20" : "G21"]);
}

function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  writeln("%");
  // #594: the machine name is user text - cleaned like every other
  // comment text, so `Tormach 770MX (ML)` never nests.
  rawBlock([oWord(), "(" + (progComment() || clean(nm) || "CAMCRAFT") + ")"]);
  writeHeaderExtras();
  unitBlock();
}

// Cutter radius compensation in the control (#39; RS-274 / ISO 6983
// G41 left / G42 right / G40 cancel, and the family's published EIA/ISO
// programming conventions, which follow the Fanuc words: the D word
// names the tool-offset row whose radius the control offsets by). The
// stream's COMP record says which side; the NEXT motion record is the
// engage (or cancel) block - the generator's law makes it a straight
// lead at least a radius long, so the control builds (or withdraws)
// the offset over it. D = the tool number, the row the H word reads. A
// comp word that would land on a rapid or an arc is refused by name
// (counted, gate-blocked) - never written where the control would
// misread it. Under wear (CompensationType "wear", the centre path)
// the row must hold only the wear delta: the section says so once.
function onRadiusCompensation(side) {
  var s = String(side || properties.Compensation || "off");
  if (s === "off") {
    compG = compOn ? "G40" : "";
    compD = "";
    compOn = false;
    return;
  }
  if (inCycle) { msg("cutter compensation inside a canned-cycle group"); return; }
  compG = (s === "left") ? "G41" : "G42";
  compD = "D" + ni(tNo);
  compOn = true;
  if (properties.CompensationType === "wear" && !compNoted) {
    block(["(WEAR COMP - " + compD + " HOLDS THE WEAR DELTA ONLY, NOT THE TOOL RADIUS)"]);
    compNoted = true;
  }
}
function compRefuse(what) {
  if (compG) {
    msg("cutter compensation must engage or cancel on a straight G01 move, not " + what);
    compG = ""; compD = ""; compOn = false;
  }
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
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;  // #1633
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  commentParam(name, value);
  if (name === "OperationName") { coolantOp = String(value || ""); spinRev = false; }
  if (name === "MillSpindle") { spinRev = String(value || "") === "reverse"; return; }
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "WcsYaw" && has(value)) wcsYaw = Number(value);
  else if (name === "WcsRotationMode") wcsRotMode = String(value || "");
}

function wcsRotControl() {
  var m = wcsRotMode || ((typeof postOptions !== "undefined" && postOptions.WcsRotation) ? String(postOptions.WcsRotation) : "");
  return m === "control" && has(wcsYaw) && Math.abs(wcsYaw) > 1e-6;
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

// The preamble closes over everything a raw motion line needs to be a
// legal program. #1411: the spindle words are the RS-274 / ISO 6983
// ones - S.. with M3 (clockwise) or M4 (counter-clockwise, a left-hand-
// cut mill) on one block, M5 to stop - so a G84 right-hand tap meets a
// spindle stated clockwise. Never the multi-letter MOTON / MOTOFF this
// post once wrote: no public document gives them, and they are no
// address-letter-and-number word of the grammar this header names.
// #616: per-operation smoothing. No Mazak smoothing form is stated from
// its public documentation yet, so the ask is named once and nothing is
// written - never a guessed word.
var smoothNamed = false;
function smoothOn() {
  if (!(smoothTol > 0) || smoothNamed) return;
  note("SMOOTHING NOT WRITTEN - NO MAZAK FORM STATED");
  smoothNamed = true;
}
function smoothOff() {
  smoothActive = false;
  smoothTol = 0;
  smoothMode = "";
}

function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  // #574: a touch probe (a stated speed of 0) states its spindle
  // stopped before its first move, whatever the tool change did.
  var probeStop = spindleWait && !(sRpm > 0);
  opNotes();
  block(["G17", "G90", wcsWord, "G40", "G49", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    if (secStarted > 0 && optBool("optionalStop", false)) block(["M1"]);
    toolNote();
    if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
    block(["T" + ni(tNo), "M6"]);
    block(["G43", "H" + ni(tNo)]);
    xyUnknown = true;          // #1633: the first traverse states its height again
    toolWait = false;
  }
  secStarted += 1;
  if (spindleWait && sRpm > 0) {   // a touch probe (#173) states no spindle
    block(["S" + ni(sRpm), spinRev ? "M4" : "M3"]);
    spindleOn = true;
    spindleWait = false;
  }
  if (probeStop) { block(["M5"]); spindleOn = false; }
  probeSection = probeStop && kindProbe;
  // #995: a touch probe section starts no coolant, whatever its ask -
  // fluid on the stylus and the measured faces spoils the reading.
  coolant = probeStop ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  smoothOn();
  if (wcsRotControl()) {
    block(["G68", "X0.", "Y0.", "R" + fa(wcsYaw)]);
    wcsRotActive = true;
  }
}

// The op's after-action (#110), written once its spindle has stopped:
// the stream says nothing to carry on, or PARAM AfterOperation
// optional-stop / stop / home. The ask resets here, so the next
// section states its own.
function afterOp() {
  var ask = afterAsk;
  var next = setupNext;
  afterAsk = "";
  setupNext = null;
  if (!ask) return;
  if (next !== null && (ask === "optional-stop" || ask === "stop")) {
    // #1647: lift Z before inviting the operator to re-clamp the part.
    retractHome(false);
    if (optStr("safeRetract", "G28") !== "G53") block(["G90"]);
    note("SETUP CHANGE - NEXT: " + (next || "UNNAMED SETUP"));
  }
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    retractHome(true);
    if (optStr("safeRetract", "G28") !== "G53") block(["G90"]); // absolute again before the next section
    homedAt = linesWritten();
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // M11.2 A: the plane never leaks past its section
  sectionOpen = false;
  smoothOff();
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (wcsRotActive) { block(["G69"]); wcsRotActive = false; }
  wcsYaw = 0;
  wcsRotMode = "";
  if (compOn || compG) {
    msg("cutter compensation still engaged at section end - no cancel block");
    compG = ""; compD = ""; compOn = false;
  }
  compNoted = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
  afterOp();
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); hopZ = null; clearZ = null; }  // #1028, #1633: the next section states its own planes

// ------------------------------------------------------------- motions
// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only - the chain is the generator's truth,
// not a second copy of output.
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
// ------------------------------------------------- the first traverse
// #1633 (#1616's second half; the #1614 hazard): X and Y are unknown at
// the program's start and after every tool change, and so is the height
// the change left the tip at - a long tool on a high work offset can
// stand below the part top there, and a first move written as one
// combined rapid (`G00 X20 Y10 Z-3`) is a straight line from that point,
// across the part. So the first X/Y traverse is made at a STATED
// height: a Z-only rapid to the section's clearance - its
// ClearancePlane, else its RetractPlane, never below the move's own Z -
// then X/Y alone at it, then the move as the stream states it. A setup
// that cannot reach that height is stopped by the control's Z limit
// instead of rapiding through the part. A stream that states neither
// plane has no verified height to traverse at: refused by name.
// This dialect refuses tilted records (tiltOk), so the rapids are always
// in the frame of the section's upright words.
var clearZ = null;            // #1633: the section's ClearancePlane, null when unstated
var xyUnknown = true;         // #1633: at the program's start and after each tool change
var traverseRefused = false;  // #1633: once per program
function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}
// (x, y, z): a rapid's target (a cycle's hole at its R plane), which the
// record itself then goes to; or, with `descend`, where the stream says a
// feed, an arc or a probing move starts - rapided down to when it lies
// below the clearance, so the move runs only the stretch planned.
// False when refused: the caller then writes nothing for its move - nor
// for any later one while X/Y stay unknown (the rest of the section) -
// so no combined X/Y/Z rapid from an unknown height reaches the text.
function xyFirst(x, y, z, descend) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  var tz = traverseZ(z);
  if (tz === null) {
    if (!traverseRefused) {
      traverseRefused = true;
      msg(coolantWho() + "first X/Y traverse with no clearance height stated (no ClearancePlane or RetractPlane) - the post will not rapid X/Y at an unknown height");
    }
    return false;
  }
  block(["G00", wo("Z", tz, orgZ)]);
  // A rapid already at the clearance is the traverse itself.
  if (descend || !has(z) || Math.abs(Number(z) - tz) > 1e-9) block(["G00", wo("X", x, orgX), wo("Y", y, orgY)]);
  if (descend && has(z) && Number(z) < tz - 1e-9) block(["G00", wo("Z", z, orgZ)]);
  xyUnknown = false;
  return true;
}
function onRapid() {
  startPreamble();
  compRefuse("a rapid");
  if (!tiltOk(properties) || chainRapid()) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z, false)) return;  // #1633: refused - the move is not written
  block(["G00"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || chainCut()) return;
  if (spindleStopped("feed move G01")) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  // The pending comp word rides this straight block (#39).
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G01", cg].concat(xyz(properties), [cd, fw(properties.FeedRate)]));
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
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  if (!tiltOk(properties) || chainCut()) return;
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G02" : "G03"))) return;
  var p = properties;
  if (compPlaneRefuse(planeOf(p))) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var g = p.CircularDirection === "CW" ? "G02" : "G03";
  if (optBool("useRadiusArcs", false) && emitArcWithRadius(p, g, fw(p.FeedRate))) return;
  block([
    planeWord(planeOf(p)),
    g,
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------------------- canned cycles
// The cycle record replaces its whole move group: one block per hole,
// holes of an unchanged cycle continuing modally with bare X Y lines.
// #994: G99, stated as each group opens - each hole returns to its R
// plane, where the stream's chain ends (RS-274 / ISO 6983: G98 returns
// to the level the tool stood at when the cycle was read, G99 to R).
// The hop and traverse to the next hole are the stream's own rapids,
// posted after G80 (above), so no hole's retract level is left to where
// the tool happened to be.
// Feed word comes from the cycle (the generator synthesizes it from
// the group's chain, so a tap's F is rpm x pitch by the agreement
// law); a legacy marker carries none, and the modal feed stands.
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
    if (at < top - 1e-6) { block(["G00", wo("Z", top, orgZ)]); at = top; }
    block(["G00", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    if (at > p.R + 1e-6) block(["G00", wo("Z", p.R, orgZ)]);
    return;
  }
  if (lo || hi) block(["G00", wo("Z", p.R, orgZ)]);
}
function onCycle() {
  startPreamble();
  restorePlane();            // M11.2 A: a cycle drills along G17's Z
  var p = properties;
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = ms > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G86";
  // #2360: a back-bore with no dwell is G85 (feed in, feed out): a G89
  // with no P word would leave the dwell to the control's modal P.
  else if (p.CycleType === "BACKBORE") g = ms > 0 ? "G89" : "G85";
  else if (p.CycleType === "REAM") g = "G85";   // #1445: feed in, feed out at F, no dwell
  else {
    // #1445: a cycle kind this post does not know is refused by name,
    // never posted as the G81 default (a rapid out it never asked for).
    msg("canned cycle " + p.CycleType + " is not known to this post - refused, not posted as another cycle");
    return;
  }
  // #1357: a canned cycle feeds at its own F word. Without one the
  // control drills at whatever feed was last modal (or alarms on a zero
  // feed), so a cycle the stream gives no positive feed is refused by name.
  if (!(has(p.FeedRate) && p.FeedRate > 0)) {
    msg("canned cycle " + g + " has no feed rate - refused: without F the control drills at whatever feed was last modal");
    return;
  }
  // #1445: G86 (feed in, spindle stop, rapid out) has no dwell word on
  // this control (Fanuc's and Haas's published G86 formats are X Y Z R F,
  // with P only on G82/G88/G89). A P written on G86 is not read, so a
  // bore dwell is refused by name, never dropped. Sinumerik's ISO mode is
  // not verified for a G86 P either, so it refuses the same.
  if (g === "G86" && ms > 0) {
    msg("canned cycle G86 (bore) has no dwell word on this control - a " + (ms / 1000) + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  // #1820: Matrix 2 section 13-2-14 omits P from the G83 format;
  // its summary mentions P, so dwell support is not verified here.
  if (g === "G83" && p.P > 0) {
    msg("canned cycle G83 (deep peck) dwell is not supported by this post - a " + p.P + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  // #2356: this post does not map G73/G84 dwell across control modes.
  // Use the raw positive ask so rounding and the open-cycle shortcut cannot drop it.
  if ((g === "G73" || g === "G84") && p.P > 0) {
    msg("canned cycle " + g + " dwell is not supported by this post - a " + p.P + " s dwell is refused, not dropped; set the dwell to 0");
    return;
  }
  if (spindleStopped("canned cycle " + g)) return;
  if (!xyFirst(p.X, p.Y, p.R, false)) return;  // #1633: refused - the move is not written
  cycleSeat(p);
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q;
  // #2360 (as #2350 for Haas): P is modal in the dwell cycles, so a
  // changed dwell reopens the cycle and each hole posts its own P.
  if (g === "G82" || g === "G89") sig += "|" + ms;
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
    if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
    words.push(fw(p.FeedRate));
    block(words);
    cycleSig = sig;
  }
  cycX = p.X; cycY = p.Y; cycR = p.R;
  seatKnown = true;
  inCycle = true;
}

// ------------------------------------------------------------ probing
// #173. Words from Mazak's published EIA/ISO programming manual (the
// Fanuc-compatible G-code mode): G31 skip function, the user-macro
// system variables #5061-#5063 (skip position X Y Z, work coordinates),
// #3000 (alarm with message), #5221 + 20 x slot + axis (the G54..G59
// work offsets) and G10 L2 P1..P6 (programmed work-offset input). The
// branch is the macro's IF[..]GOTOn form with a unique label per test,
// not IF..THEN (not every Mazak EIA generation documents THEN). A G31
// that never trips ends at its target, so that alarms at once - a probe
// that never touched must not back off and measure air. A measured
// trip k is kept in #(100+3k)..#(102+3k); the back-off is incremental
// (G91). Without a work offset to set, the program stops (M00) for the
// operator to read #190... Arming the probe is the machine's own.
var probeBase = 100;
var probeLabel = 9000;   // N labels the IF..GOTO tests jump past
function axisIx(a) { return a === "X" ? 0 : (a === "Y" ? 1 : 2); }
function travelAxis(p) {  // where the target and the nominal trip differ most
  var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
  return d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
}
function alarmUnless(cond, n, text) {  // alarm #3000=n unless cond holds
  probeLabel += 1;
  block(["IF[" + cond + "]GOTO" + probeLabel]);
  block(["#3000=" + n + "(" + text + ")"]);
  block(["N" + probeLabel]);
}
function minusU(expr, v) {  // expr - v, the sign folded into the operator
  var s = fn(v);  // macro-B text: a constant is not scaled by DPI (#1836)
  if (s === "0") return expr;
  return s.charAt(0) === "-" ? expr + "+" + s.substring(1) : expr + "-" + s;
}
function onProbeMove() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a probing move");
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var p = properties;
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  var ai = travelAxis(p);
  var tgt = [p.X - orgX, p.Y - orgY, p.Z - orgZ][ai];
  alarmUnless("ABS[" + minusU("#" + (5061 + ai), tgt) + "]GE" + eps(), 2, "PROBE NO CONTACT");
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
  block(["G91", "G01", w("X", p.DeltaX), w("Y", p.DeltaY), w("Z", p.DeltaZ), fw(p.Feed)]);
  block(["G90"]);
}
// #174. A protected positioning move (the descent to a touch's start,
// the travel between touches, the rise) is the same G31 skip read the
// other way round (Mazak EIA/ISO manual, as above): with no skip signal
// the block ends at its target and #5061-#5063 hold that end point; a
// trip on the way stops it short, so any moving axis whose skip
// position is not the target alarms (#3000=3, distinct from
// no-contact's 2), in the same IF[..]GOTOn form - an obstacle where the
// model has none stops the program instead of the stylus being driven
// into it.
function onProbeGuard() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a protected probe move");
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var p = properties;
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  var to = [p.X, p.Y, p.Z], from = [p.StartX, p.StartY, p.StartZ], org = [orgX, orgY, orgZ];
  for (var ai = 0; ai < 3; ai++) {
    if (has(from[ai]) && Math.abs(to[ai] - from[ai]) < 1e-6) continue;  // an axis that holds still
    alarmUnless("ABS[" + minusU("#" + (5061 + ai), to[ai] - org[ai]) + "]LE" + eps(), 3, "PROBE HIT ON POSITIONING");
  }
}
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 190;  // #190.. hold each axis's deviation, found - expected
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i];
    var ai = axisIx(a.Axis);
    var sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("#" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "[[" + sum.join("+") + "]/" + a.Measures.length + "]" : sum[0];
    // The sign is folded into the operator so a negative expected value
    // (edge, outside corner) never writes a doubled sign like `--`.
    var ex = a.Expected - org[ai];
    var op = ex < 0 ? "+" : "-";
    block(["#" + (dev + i) + "=" + mean + op + fn(Math.abs(ex))]);
    alarmUnless("ABS[#" + (dev + i) + "]LE" + fu(p.Tolerance), 1, "PROBE " + a.Axis + " OUT OF TOLERANCE");
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var act = 5221 + 20 * (wcsIndex - 1);
    var words = ["G10", "L2", "P" + (Number(p.SetWorkOffset) + 1)];
    for (var j = 0; j < axes.length; j++) {
      var aj = axisIx(axes[j].Axis);
      words.push(axes[j].Axis + "[#" + (act + aj) + "+#" + (dev + j) + "]");
    }
    block(words);
  } else {
    block(["(CHECK PROBE RESULT #" + dev + "..)"]);
    block(["M00"]);
  }
}

// ------------------------------------------------------------ footer
// #2189: modal EIA/ISO unit word (G20 / G21) after the final retract,
// right before M30, when postOptions.EndUnits asks for the other unit.
function endUnitsWord() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch" && !inch) return "G20";
  if (want === "mm" && inch) return "G21";
  return "";
}
function onClose() {
  endPreamble();
  // #504: a trip home with nothing written after it (a trailing Go
  // home) already is the program-end home - written once, not twice.
  if (linesWritten() !== homedAt) {
    retractHome(true);
  }
  var u = endUnitsWord();
  if (u) block([u]);
  block(["M30"]);
  writeln("%");
}

// Siemens 840D 3X milling post. Provenance: the G-dialect from public
// DIN 66025 / ISO 6983 documentation and the control family's
// published operator conventions (behavior only, per this repo's
// Clean-Room Protocol); the hook contract from Autodesk's published
// post docs. No vendor script text; original work. Where a public
// convention leaves a hole, the house style (see fanuc.cps) decides,
// and the conformance goldens lock that decision, not any vendor's.
description = "Siemens 840D (3-axis mill, DIN/ISO cycles G73-G89, G2/G3)";
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
workOffsetMax = 6;      // G54..G57, G505, G506 (#934); more G505+ frames need the machine's frame count (#931)
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G710 / G700

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
var sRpm = 0;
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var inCycle = false;
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var setupNext = null;      // #1647: null means no marker; a cleaned name may be empty
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var compG = "";            // G41/G42/G40 pending for the next straight block (#39)
var compD = "";            // the D word riding the engage block ("" when the tool call selects it)
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var compNoted = false;     // the section's operator note was written
var orgX = 0, orgY = 0, orgZ = 0;
var nLine = 0;             // the numbered-block dialect: every written
                           // block advances N by 10 (gap left for hand edits)

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
function fu(v) {  // a length: mm at three decimals, or inch at four
  if (!has(v)) return "0";
  return trimNum(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3));
}
function ff(v) {  // a feed: whole mm/min, or in/min at two decimals
  if (!(has(v) && v > 0)) return "0";
  return inch ? trimNum((v / 25.4).toFixed(2)) : String(Math.round(v));
}
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; }
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // one numbered line per block - the DIN law
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) {
    nLine += 10;
    writeln("N" + nLine + " " + a.join(" "));
  }
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
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
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Sinumerik post has no RTCP output"); }
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

// -------------------------------------------------------------- header
// The DIN family comments with ';' and needs no fence; identity is one
// comment line, and the first real block carries the plane.
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

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: a semicolon opens the comment, so a
// second one, and % (the tape's start mark) and parentheses, become
// spaces; the line rides unnumbered, as the machine-name line does;
// printable ASCII only, capped at 40 characters.
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
function note(s) { writeln("; " + s); }
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
// The document's program identity (#258), from the engine's `program`
// global: its number (1 when the document states none; one file per
// operation counts up from it, #111), whether the document stated it,
// and its program comment ("" for none - the header then names the
// machine, as before #258).
function progNo() {
  var n = typeof program !== "undefined" ? program.number : (typeof programNumber !== "undefined" ? programNumber : 1);
  return Math.round(Number(n) || 1);
}
function progNumbered() { return typeof program !== "undefined" && program.numbered === true; }
function progComment() { return clean(typeof program !== "undefined" ? program.comment : ""); }
// #258: the control addresses a main program by its name. Siemens'
// published SINUMERIK 840D sl / 828D programming manual (Fundamentals,
// "Program names") states the rules: letters, digits and underscores
// only, the first two characters letters (or an underscore and a
// letter), at most 24 characters; a program read in as text opens with
// the `%_N_<name>_MPF` line naming it a main program. So a stated
// program comment becomes that name (spaces and every other character
// to `_`), and a name the rules refuse is refused by name here, never
// renamed into another program. A stated number has no word on this
// control - it rides the header comment. None stated: the header is
// the machine's name, as before.
function mpfName(said) {
  var t = said.replace(/[^A-Za-z0-9_]/g, "_");
  if (!/^([A-Za-z]{2}|_[A-Za-z])/.test(t) || t.length > 24) {
    throw new Error("program name \"" + said + "\" cannot name a SINUMERIK program (" + t
      + "): it must start with two letters (or _ and a letter) and be at most 24 characters");
  }
  return t;
}
function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  var said = progComment();
  if (said) writeln("%_N_" + mpfName(said) + "_MPF");
  var t = said || clean(nm) || "CAMCRAFT"; // #594: cleaned
  writeln("; " + (progNumbered() ? "PROGRAM " + progNo() + " - " + t : t));
  var z = safeZ();
  if (z !== null && z !== 0) note("SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE");
}

// #503 (#495's rule): where "straight up" is, in machine coordinates.
// G53 Z0 is the top of Z travel only where the builder put the machine
// zero there; the family's public documentation leaves the reference
// point and machine zero to the machine builder's machine data, so the
// post cannot assume it. The machine file states the height (its Z
// part's `safe_retract`, `machine.safeRetractZ`, mm - this dialect
// writes mm); a document with no machine keeps today's Z0; any machine
// stating none is refused by name and no retract is guessed. Judged once.
var safeZDone = false;
var safeZValue = null;     // null = refused, no retract is written
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
  if (nm === "") {
    safeZValue = 0;
  } else {
    msg("machine " + nm + " does not state its safe Z retract - this post lifts Z with G53 for a home move and at the end, and where machine Z0 sits is the machine builder's choice on this control; state the Z axis's safe_retract (machine coordinates) in the machine file");
  }
  return safeZValue;
}

// Cutter radius compensation in the control (#39; DIN 66025 / ISO 6983
// G41 left / G42 right / G40 cancel, and the family's published
// programming fundamentals: the radius is the ACTIVE cutting edge's -
// the D the tool call already selected (T n D n) - so no D word rides
// the engage block; the approach NORM runs straight to the first point
// and stands the tool perpendicular to the path tangent at that start
// point, the start-up the generator's lead law assumes - stated on the
// engage block, never left to a modal KONT). The stream's COMP
// record says which side; the NEXT motion record is the engage (or
// cancel) block, a straight lead at least a radius long. A comp word
// that would land on a rapid or an arc is refused by name (counted,
// gate-blocked) - never written where the control would misread it.
// Under wear (CompensationType "wear", the centre path) the edge must
// hold only the wear delta: the section says so once, in a comment.
function onRadiusCompensation(side) {
  var s = String(side || properties.Compensation || "off");
  if (s === "off") {
    compG = compOn ? "G40" : "";
    compD = "";
    compOn = false;
    return;
  }
  if (inCycle) { msg("cutter compensation inside a canned-cycle group"); return; }
  compG = ((s === "left") ? "G41" : "G42") + " NORM";  // NORM stated, never a stale KONT
  compD = "";
  compOn = true;
  if (properties.CompensationType === "wear" && !compNoted) {
    block(["; WEAR COMP - D" + ni(tNo) + " HOLDS THE WEAR DELTA ONLY, NOT THE TOOL RADIUS"]);
    compNoted = true;
  }
}
function compRefuse(what) {
  if (compG) {
    msg("cutter compensation must engage or cancel on a straight G1 move, not " + what);
    compG = ""; compD = ""; compOn = false;
  }
}

// #931: PARAM WorkOffset arrives as the 1-based offset number (the engine
// refuses one above workOffsetMax by setup name); the word is ours to write.
function workOffsetName(n) {  // offset n -> this control's word; "" when it has none
  if (!(n >= 1 && n <= 6 && n === Math.floor(n))) return "";
  return n <= 4 ? "G" + (53 + n) : "G" + (500 + n);  // settable frames G54..G57, then G505..
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
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
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

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: plane, offsets, tool, spindle, coolant. The tool call
// selects tool and geometry together (T n D n) - no G43 word exists
// in this family.
// #616: per-operation smoothing, only when the document turned it on
// (the stream then carries PARAM Smoothing / SmoothingMode). Siemens CYCLE832 high-speed settings: tolerance plus _ROUGH/_FINISH.
// Written after the preamble, undone at the section's end so no
// tolerance leaks into the next operation.
function smoothOn() {
  if (!(smoothTol > 0)) return;
  block(["CYCLE832(" + fu(smoothTol) + "," + (smoothMode === "finish" ? "_FINISH" : "_ROUGH") + ",1)"]);
  smoothActive = true;
}
function smoothOff() {
  if (smoothActive) block(["CYCLE832()"]);  // deselect
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
  // #901: G710 - metric for lengths AND feeds (the family's published
  // programming manual: G71 switches only geometric data, so F would
  // still read in the basic system's unit; G710 switches both). Stated
  // in the opening block before any motion, whatever the control's
  // basic system or the last program left modal. #902: the unit is the
  // document's - an inch program states G700, inch for lengths and
  // feeds alike, by the same law.
  block(["G17", inch ? "G700" : "G710", "G90", wcsWord, "G40", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    block(["T" + ni(tNo), "D" + ni(tNo), "M6"]);
    xyUnknown = true;          // #1633: the first traverse states its height again
    toolWait = false;
  }
  if (spindleWait && sRpm > 0) {   // a touch probe (#173) states no spindle
    block(["S" + ni(sRpm), "M3"]);
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
    var setupZ = safeZ();
    if (setupZ === null) return;
    block(["G0", "G53", "Z" + fu(setupZ)]);
    note("SETUP CHANGE - NEXT: " + (next || "UNNAMED SETUP"));
  }
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    var z = safeZ();            // refused (named once): no Z lift, so no XY move either
    if (z === null) return;
    block(["G0", "G53", "Z" + fu(z)]);   // machine coordinates, Z first (onClose's law)
    block(["G0", "G53", "X0", "Y0"]);
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
// combined rapid (`G0 X20 Y10 Z-3`) is a straight line from that point,
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
  block(["G0", wo("Z", tz, orgZ)]);
  // A rapid already at the clearance is the traverse itself.
  if (descend || !has(z) || Math.abs(Number(z) - tz) > 1e-9) block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  if (descend && has(z) && Number(z) < tz - 1e-9) block(["G0", wo("Z", z, orgZ)]);
  xyUnknown = false;
  return true;
}
function onRapid() {
  startPreamble();
  compRefuse("a rapid");
  if (!tiltOk(properties) || chainRapid()) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z, false)) return;  // #1633: refused - the move is not written
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || chainCut()) return;
  if (spindleStopped("feed move G1")) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  // The pending comp word rides this straight block (#39).
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G1", cg].concat(xyz(properties), [cd, fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  if (!tiltOk(properties) || chainCut()) return;
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G2" : "G3"))) return;
  var p = properties;
  if (compPlaneRefuse(planeOf(p))) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------------------- canned cycles
// The cycle record replaces its whole move group: one block per hole,
// holes of an unchanged cycle continuing modally with bare X Y lines.
// The DIN cycles retract to the R plane by the documented default of
// this family's plain G81-G89 forms - there is no G98 word to write,
// so none is invented; every cycle block restates the hole position.
// #994: the hop and traverse to the next hole are the stream's own
// rapids, posted after G80 (above); and with the tool seated on R
// (below) a control returning to the initial level lands on R too -
// no return mode is assumed.
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
  restorePlane();            // M11.2 A: a cycle drills along G17's Z
  var p = properties;
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = ms > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
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
  if (spindleStopped("canned cycle " + g)) return;
  if (!xyFirst(p.X, p.Y, p.R, false)) return;  // #1633: refused - the move is not written
  cycleSeat(p);
  var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
  if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
    words.push(w("Q", p.Q)); // an increment (peck depth): no origin shift
  }
  if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
  words.push(fw(p.FeedRate));
  block(words);
  cycX = p.X; cycY = p.Y; cycR = p.R;
  seatKnown = true;
  inCycle = true;
}

// ------------------------------------------------------------ probing
// #173. Words from Siemens' published SINUMERIK 840D sl programming
// manuals (Fundamentals; Job Planning): MEAS=1 (measure with probe 1,
// rising edge, deleting distance-to-go) on a G1 block, $AC_MEA[1] (the
// probe responded, 1, or not, 0), $AA_MW[axis] (the trip position in
// workpiece coordinates), IF..ENDIF, MSG, SETAL (user cycle alarms from
// 65000), the settable frames $P_UIFR[n,axis,TR|FI] (G54..G57 = 1..4)
// whose new value takes effect when the G5x is programmed again.
// Reading a main-run variable stops preprocessing implicitly, so the
// trip is read after the move ends. A measuring block that never trips
// alarms at once - a probe that never touched must not back off and
// measure air. A measured trip k is kept in R(50+3k)..R(52+3k) and the
// deviations in R90.., inside R0..R99 (the R parameters every 840D
// carries; more are a machine-data option). The back-off is incremental
// (G91). The fifth and sixth work offsets are the settable frames
// G505/G506 here (workOffsetName, #934/#931), so a probe sets those frames.
// Arming the probe is the machine's own.
var probeBase = 50;
function axisIx(a) { return a === "X" ? 0 : (a === "Y" ? 1 : 2); }
function travelAxis(p) {  // where the target and the nominal trip differ most
  var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
  return d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
}
function stopAlarm(cond, n, text) {  // message, alarm and stop when cond holds
  block(["IF " + cond]);
  block(["MSG(\"" + text + "\")"]);
  block(["SETAL(" + n + ")"]);
  block(["M0"]);
  block(["ENDIF"]);
}
function onProbeMove() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a probing move");
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var p = properties;
  block(["MEAS=1", "G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  stopAlarm("$AC_MEA[1]==0", 65000, "PROBE NO CONTACT");
  if (has(p.Measure) && p.Measure >= 0) {
    if (p.Measure > 12) { msg("probing: measurement " + p.Measure + " - only 13 trips fit R50..R88"); return; }
    var b = probeBase + 3 * p.Measure;
    block(["R" + b + "=$AA_MW[X]"]);
    block(["R" + (b + 1) + "=$AA_MW[Y]"]);
    block(["R" + (b + 2) + "=$AA_MW[Z]"]);
  }
}
function onProbeRetreat() {
  probeSeat();               // #1028
  var p = properties;
  block(["G91", "G1", w("X", p.DeltaX), w("Y", p.DeltaY), w("Z", p.DeltaZ), fw(p.Feed)]);
  block(["G90"]);
}
// #174. A protected positioning move (the descent to a touch's start,
// the travel between touches, the rise) is the same MEAS=1 G1 block,
// read the other way round (840D sl Job Planning, measurement as
// above): MEAS deletes the distance-to-go on a trip and sets
// $AC_MEA[1] to 1, so a guard block that ends with the probe responded
// was stopped short by contact - it alarms (SETAL 65002, distinct from
// no-contact's 65000) and stops, so an obstacle where the model has
// none stops the program instead of the stylus being driven into it.
// $AC_MEA[1] is the test, not $AA_MW: the measured value is only
// written on a trip, so it is meaningless when the move ran clear.
function onProbeGuard() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a protected probe move");
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var p = properties;
  block(["MEAS=1", "G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  stopAlarm("$AC_MEA[1]==1", 65002, "PROBE HIT ON POSITIONING");
}
// #934/#931: on the 840D the settable zero offsets are G54..G57 and
// then G505..G599 (840D sl Fundamentals, "settable zero offset"); G58/G59
// are axial programmable offsets that select no frame. Offset n is
// settable frame n - $P_UIFR[n] - written by workOffsetName: offsets 5
// and 6 are G505/G506.
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 90;   // R90.. hold each axis's deviation, found - expected
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i];
    var ai = axisIx(a.Axis);
    var sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("R" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "(" + sum.join("+") + ")/" + a.Measures.length : sum[0];
    // The sign is folded into the operator so a negative expected value
    // (edge, outside corner) never writes a doubled sign like `--`.
    var ex = a.Expected - org[ai];
    var op = ex < 0 ? "+" : "-";
    block(["R" + (dev + i) + "=" + mean + op + fu(Math.abs(ex))]);
    stopAlarm("ABS(R" + (dev + i) + ")>" + fu(p.Tolerance), 65001, "PROBE " + a.Axis + " OUT OF TOLERANCE");
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var to = Number(p.SetWorkOffset) + 1, from = wcsIndex;  // SetWorkOffset is 0-based
    var want = workOffsetName(to) || ("work offset " + to);
    if (!workOffsetName(to) || !workOffsetName(from)) {
      msg("probing: setting " + want + " from " + wcsWord + " - only the settable frames G54..G57 / G505..G599 can be set on this control");
      return;
    }
    for (var j = 0; j < axes.length; j++) {
      var ax = axes[j].Axis;
      block(["$P_UIFR[" + to + "," + ax + ",TR]=$P_UIFR[" + from + "," + ax + ",TR]+R" + (dev + j)]);
      if (to !== from) block(["$P_UIFR[" + to + "," + ax + ",FI]=$P_UIFR[" + from + "," + ax + ",FI]"]);
    }
    block([wcsWord]);  // re-select: a written frame takes effect on its G5x
  } else {
    block(["MSG(\"CHECK PROBE RESULT R" + dev + "..\")"]);
    block(["M0"]);
  }
}

// ------------------------------------------------------------ footer
// Homing rides machine coordinates (G53), not a stored home point; the
// height is the machine's stated safe Z (#503, safeZ).
function onClose() {
  endPreamble();
  // #504: a trip home with nothing written after it (a trailing Go
  // home) already is the program-end home - written once, not twice.
  if (linesWritten() !== homedAt) {
    var z = safeZ();
    if (z !== null) block(["G53", "Z" + fu(z)]);
  }
  block(["M2"]);
}

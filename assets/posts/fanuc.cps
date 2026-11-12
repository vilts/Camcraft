// Fanuc 3X milling post - the first .cps we ship. Provenance: the
// G-dialect from public RS-274/ISO 6983 documentation; the hook
// contract from Autodesk's published post docs (behavior only) per
// this repo's Clean-Room Protocol. No vendor script text; original
// work. A .cps is plain JS: the manifest is assignments and the
// helpers below are local - the script needs only writeln/write and
// the properties object from any host (our engine publishes more,
// the reference base less; this text runs on either unchanged).
description = "Fanuc (3-axis mill, G73/G81/G82/G83/G84/G86/G89, G2/G3, G99)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it
axisInversion = true; // verified on the engine's mirrored stream for an inverted X/Y (#374): goldens + equivalence test
probeAxisInversion = true; // probing blocks verified on the mirrored stream (#392): macro interpreter on a simulated inverted machine

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var sRpm = 0;              // S/M3 waits for the section: the stream
var spindleWait = false;   // order is PARAMs, TOOL, SECTION
var toolWait = false;      // T../M6 and G43 wait the same way
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var cycX = 0, cycY = 0, cycR = 0; // the open group's hole XY and R plane (world, #994)
var seatKnown = false;     // #994: this section moved, so the stream's Start is where the tool is
var hopZ = null;           // #1028: the section's RetractPlane (its hop), null when unstated
var coolant = "";          // the M-code this program started, if any
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var tcpmOn = false;        // G43.4 armed (stage D RTCP): re-armed at tool change
var lastA = 0, lastB = 0, lastC = 0; // angle words the last block commanded (deg)
var tiltRefused = false;   // a refusal the gate counts once, not per line
var compG = "";            // G41/G42/G40 pending for the next straight block (M13.11)
var compD = "";            // the D word riding the engage block
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var wearNoted = false;     // the wear-register comment was written this section (#38)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs):
// the stream speaks world coordinates; the machine measures its program
// from the work offset. A legacy .cl carries no PARAMs, so these stay 0.

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
// #377: `expr - v` as a macro-B expression with v's sign folded into the
// operator, so a negative v never writes a doubled sign (`#100--15`).
// Macro B's arithmetic form is `#i=#j-#k` / `#i=#j+#k` (Fanuc custom
// macro B, published operator documentation); a value that formats to 0
// (including -0 and anything under half a micron) adds no term.
function minusU(expr, v) {
  var s = fu(v);
  if (s === "0") return expr;
  return s.charAt(0) === "-" ? expr + "+" + s.substring(1) : expr + "-" + s;
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // join the non-empty words onto one line
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}

// RTCP output (stage D). A tilted record locates the TOOL TIP in the
// work offset while the machine turns - G43.4 delegates the kinematics
// to the controller, so the post never does pivot math; it arms the
// mode, voices the commanded angles modally, and refuses by name
// (counted warning, gate-blocked) wherever it cannot honor one. Law:
// a tilted record states all three angles (the stream's parser zeros
// unstated letters), so the presence of A decides; angle memory
// resets with the arm, so re-arming always commands every stated
// rotary's word (#997: a letter with no rotary takes none).
// A prismatic record never touches the mode - the tilt stays modal.
function statesRotary(letter) {  // does the machine state a rotary of this name?
  var rs = (typeof machine !== "undefined" && machine.rotaries) || [];
  for (var j = 0; j < rs.length; j++) {
    if (String(rs[j].name || "").toUpperCase() === letter) return true;
  }
  return false;
}
function missingLetter(p) {  // the first turning letter no rotary is named for, else ""
  var ls = ["A", "B", "C"];
  for (var i = 0; i < ls.length; i++) {
    var v = p[ls[i]];
    if (!has(v) || Math.abs(v) <= 1e-9) continue;
    if (!statesRotary(ls[i])) return ls[i];
  }
  return "";
}
// #997: the angle words name only the rotaries the machine states. The
// stream zeros an unstated letter, and missingLetter has refused one
// that turns, so a letter with no rotary here is a stated zero - a word
// for it (C0 on an A/B head) is an address the control has no axis for.
function angleWord(letter, v) { return statesRotary(letter) ? w(letter, v) : ""; }
// #797 review: the machine check judges a lone rotary about X by the
// angle the table must hold, signed by its stated direction - a [-1,0,0]
// table holds the negated angle (`judge.rs` `x_rotary_problem`). This
// dialect writes the stream's A as it comes and never negates it, so on
// such a table the check's number and the posted one would differ: the
// pose it passed against the travel is not the word this post writes.
// Refused by name rather than posted outside the judged travel. A0 is
// the same word on either sign, so an upright A0 still posts.
function reversedLoneXRotary() {  // the rotary's name, else ""
  var rs = (typeof machine !== "undefined" && machine.rotaries) || [];
  if (rs.length !== 1) return "";
  var d = rs[0].direction;
  if (!d || d.length !== 3 || !has(d[0]) || !has(d[1]) || !has(d[2])) return "";
  var n2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
  if (!(n2 > 0) || d[0] * d[0] < 0.998 * n2 || d[0] > 0) return "";
  return String(rs[0].name || "A").toUpperCase();
}
function tilt(p) {  // words to append, [] if none, null if refused
  if (!has(p.A)) return [];
  if (typeof machine === "undefined" || machine.rtcp !== true) {
    if (!tiltRefused) { tiltRefused = true; msg("tilted move on a machine without a tilting rotary"); }
    return null;
  }
  if (inCycle) { msg("tilted move inside a canned-cycle group"); return null; }
  // #797: the words go out as the stream's letters, so a letter that
  // turns needs a rotary of that name. The machine check reads a 4th-axis
  // table's pose through the tool axis (A0 B180 is its A180), which this
  // dialect does not re-express - so the refusal by letter lives here too.
  var lost = missingLetter(p);
  if (lost) {
    if (!tiltRefused) { tiltRefused = true; msg("tilted move commands " + lost + " - the machine states no " + lost + " rotary"); }
    return null;
  }
  var rev = reversedLoneXRotary();
  if (rev && Math.abs(p.A) > 1e-9) {
    if (!tiltRefused) { tiltRefused = true; msg("tilted move on rotary " + rev + ", stated along -X - this dialect posts the stream's angle unnegated, so it would leave the travel the machine check judged"); }
    return null;
  }
  if (!tcpmOn) {
    block(["G43.4", "H" + pad2(tNo)]);
    tcpmOn = true;
    lastA = p.A; lastB = p.B; lastC = p.C;
    return [angleWord("A", p.A), angleWord("B", p.B), angleWord("C", p.C)];
  }
  var words = [];
  if (p.A !== lastA) { words.push(angleWord("A", p.A)); lastA = p.A; }
  if (p.B !== lastB) { words.push(angleWord("B", p.B)); lastB = p.B; }
  if (p.C !== lastC) { words.push(angleWord("C", p.C)); lastC = p.C; }
  return words;
}

// Cutter radius compensation in the control (M13.11, RS-274 / ISO
// 6983 G41 left / G42 right / G40 cancel). The stream's COMP record
// says which side and the radius the register holds; the NEXT motion
// record is the engage (or cancel) block - the generator's law makes
// it a straight lead at least a radius long, so the control builds
// (or withdraws) the offset over it. The D word is the tool's offset
// register: the tool library states none, so it is the tool number,
// the same slot the H word reads. A comp word that would land on a
// rapid or an arc is refused by name (counted, gate-blocked) - never
// written where the control would misread it. Wear compensation (#38,
// properties.CompensationType "wear") writes the same words over the
// cutter-centre path: the register must hold only the wear delta, so
// the section says so once, in a comment the operator reads.
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
  compD = "D" + pad2(tNo);
  compOn = true;
  if (properties.CompensationType === "wear" && !wearNoted) {
    block(["(WEAR COMP - " + compD + " HOLDS THE WEAR DELTA ONLY, NOT THE TOOL RADIUS)"]);
    wearNoted = true;
  }
}
function compRefuse(what) {
  if (compG) {
    msg("cutter compensation must engage or cancel on a straight G01 move, not " + what);
    compG = ""; compD = ""; compOn = false;
  }
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
  else if (name === "ToolKind") toolKind = clean(value).toUpperCase();
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
  note(joinDash(["T" + pad2(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// The machine's inverted linear axes (#375): the engine has mirrored the
// stream into the controller's frame, and the header says so in the
// operator's words, one comment per axis. A standard machine lists none.
function invertedNotes() {
  var a = (typeof machine !== "undefined" && machine.invertedAxes) || [];
  var where = { X: "LEFT", Y: "TOWARD THE FRONT", Z: "DOWN" };
  for (var i = 0; i < a.length; i++) {
    var l = String(a[i]);
    block(["(MACHINE " + l + "+ MOVES " + (where[l] || "THE OTHER WAY") + " - INVERTED AXIS)"]);
  }
}

// #901: the unit this post writes - millimetres - stated in a block of
// its own before any motion, so a control left in inch by power-on or
// the last program never reads mm numbers as inches (RS-274 / ISO 6983:
// G20 inch, G21 mm; the family's published manual puts G20 / G21 in a
// block of its own at the program's start). Where the program's unit is
// chosen, and inch output, is #902.
function unitBlock() {
  block(["G21"]);
}

function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  writeln("%");
  // #594: the name is user text from the machine file - cleaned like
  // every other comment text, so `Tormach 770MX (ML)` never nests.
  block([oWord(), "(" + (clean(nm) || "CAMCRAFT") + ")"]);
  invertedNotes();
  unitBlock();
}

function onParameter(name, value) {
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
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

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  tcpmOn = false;              // a new tool re-arms TCPM (stage D)
  lastA = 0; lastB = 0; lastC = 0;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: offsets, tool, spindle, coolant. v1 streams deliver
// it at onSection; a legacy .cl (no records at all) gets it at its
// first motion - a posted program without an offset or a spindle is
// not a posted program.
// #616: per-operation smoothing. Fanuc's G5.1 Q1 (AI contour control)
// states no tolerance - the deviation is the control's own parameter, so
// a word written here could not be held to the op's ask (review of
// #635). The ask is named once and nothing is written until a bounded
// form is stated per control (#614's Doosan DNM).
var smoothNamed = false;
function smoothOn() {
  if (!(smoothTol > 0) || smoothNamed) return;
  note("SMOOTHING NOT WRITTEN - G5.1 STATES NO TOLERANCE");
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
  // #574: a touch probe (a stated speed of 0) states its spindle
  // stopped before its first move, whatever the tool change did.
  var probeStop = spindleWait && !(sRpm > 0);
  opNotes();
  block(["G90", wcsWord, "G40", "G49", "G80"]);
  if (toolWait) {
    toolNote();
    block(["T" + pad2(tNo), "M6"]);
    block(["G43", "H" + pad2(tNo)]);
    toolWait = false;
  }
  if (spindleWait && sRpm > 0) {   // a touch probe (#108) states no spindle
    block(["S" + ff(sRpm), "M3"]);
    spindleWait = false;
  }
  if (probeStop) block(["M5"]);
  coolant = coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  smoothOn();
}

// The op's after-action (#110), written once its spindle has stopped:
// the stream says nothing to carry on, or PARAM AfterOperation
// optional-stop / stop / home. The ask resets here, so the next
// section states its own.
function afterOp() {
  var ask = afterAsk;
  afterAsk = "";
  if (!ask) return;
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    // A reference return under TCPM is one many controls alarm on:
    // G49 drops it (and the length offset) first; the next preamble
    // states G49 and its own H again, so nothing carries over.
    if (tcpmOn) { block(["G49"]); tcpmOn = false; }
    block(["G91", "G28", "Z0"]);  // straight up to the reference point first
    block(["G28", "X0", "Y0"]);
    block(["G90"]);               // absolute again before the next section
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
  wearNoted = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  afterOp();
  // The next preamble carries G49, which cancels TCPM too - so the
  // arm and the angle memory reset here, and a later tilt re-arms by
  // commanding all three words (stage D).
  tcpmOn = false;
  lastA = 0; lastB = 0; lastC = 0;
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); hopZ = null; }  // #1028: the next section states its own hop

// ------------------------------------------------------------- motions
// While a canned-cycle group is open its expanded hole chain (rapid,
// plunge, feed, retract) is the cycle block's business only - the
// chain is the generator's truth, not a second copy of output. The
// chain stays on its hole: the hole's XY, at or below its R plane.
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
  compRefuse("a rapid");
  var tw = tilt(properties);
  if (tw === null || chainRapid()) return;
  block(["G00"].concat(xyz(properties), tw));
}
function onLinear() {
  startPreamble();
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  // The pending comp word rides this straight block (M13.11).
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G01", cg].concat(xyz(properties), [cd], tw, [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  var p = properties;
  if (compPlaneRefuse(planeOf(p))) return;
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G02" : "G03",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat(tw, [fw(p.FeedRate)]));
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
  else if (p.CycleType === "BACKBORE") g = "G89";
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
// #108. A probing move is the G31 skip function (Fanuc's published
// operator documentation, custom macro B): a straight feed toward the
// target that stops when the probe's skip signal fires. The skip
// position lands in system variables #5061-#5063 (X, Y, Z, workpiece
// coordinates); when no signal fires the block simply ends at its
// target, so that is checked at once and alarms (#3000) - a probe that
// never touched must not back off and measure air. A measured trip k is
// kept in #(100+3k)..#(102+3k). The back-off after a trip is
// incremental (G91), so a part nearer than the model is never driven
// into. The offset arithmetic reads the workpiece offsets #5221 + 20 x
// slot + axis and writes them with G10 L2.
var probeBase = 100;
function onProbeMove() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a probing move");
  var p = properties;
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  // The travel axis: where the target and the nominal trip differ most.
  var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
  var ai = d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
  var tgt = [p.X - orgX, p.Y - orgY, p.Z - orgZ][ai];
  block(["IF[ABS[" + minusU("#" + (5061 + ai), tgt) + "]LT0.001]THEN#3000=2(PROBE NO CONTACT)"]);
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
// the travel between touches, the rise) is the same G31 skip, read the
// other way round: with no skip signal the block ends at its target and
// #5061-#5063 hold that end point; a trip on the way stops it short, so
// any moving axis whose skip position is not the target alarms (#3000)
// - an obstacle where the model has none stops the program instead of
// the stylus being driven into it. Same public sources as above.
function onProbeGuard() {
  startPreamble();
  probeSeat();               // #1028
  compRefuse("a protected probe move");
  var p = properties;
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  var to = [p.X, p.Y, p.Z], from = [p.StartX, p.StartY, p.StartZ], org = [orgX, orgY, orgZ];
  for (var ai = 0; ai < 3; ai++) {
    if (has(from[ai]) && Math.abs(to[ai] - from[ai]) < 1e-6) continue;  // an axis that holds still
    block(["IF[ABS[" + minusU("#" + (5061 + ai), to[ai] - org[ai]) + "]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)"]);
  }
}
function slotOf(word) {  // "G55" -> 1
  var n = Number(String(word).replace(/[^0-9]/g, "")) - 54;
  return n >= 0 && n <= 5 ? n : 0;
}
// #575: no deviation report is written before the offset write. Fanuc's
// published custom macro B documentation (also the Haas and Mazak EIA
// macro conventions this post family follows) gives an operator message
// only as #3000 (an alarm, which stops) or #3006 (stop with a message);
// DPRNT/POPEN go to an external I/O device, not the control's screen. No
// documented message shows a value without stopping, so nothing is
// emitted - a stop on every good probe is not a report. The measure-only
// #3006 below stops by intent: there the operator must act on #190.. .
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 190;  // #190.. hold each axis's deviation, found - expected
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i];
    var ai = a.Axis === "X" ? 0 : (a.Axis === "Y" ? 1 : 2);
    var sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("#" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "[[" + sum.join("+") + "]/" + a.Measures.length + "]" : sum[0];
    block(["#" + (dev + i) + "=" + minusU(mean, a.Expected - org[ai])]);
    block(["IF[ABS[#" + (dev + i) + "]GT" + fu(p.Tolerance) + "]THEN#3000=1(PROBE " + a.Axis + " OUT OF TOLERANCE)"]);
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var act = 5221 + 20 * slotOf(wcsWord);
    var words = ["G10", "L2", "P" + (Number(p.SetWorkOffset) + 1)];
    for (var j = 0; j < axes.length; j++) {
      var aj = axes[j].Axis === "X" ? 0 : (axes[j].Axis === "Y" ? 1 : 2);
      words.push(axes[j].Axis + "[#" + (act + aj) + "+#" + (dev + j) + "]");
    }
    block(words);
  } else {
    block(["#3006=1(CHECK PROBE RESULT #" + dev + "..)"]);
  }
}

// ------------------------------------------------------------ footer
function onClose() {
  endPreamble();
  // #504: a trip home with nothing written after it (a trailing Go
  // home) already is the program-end home - written once, not twice.
  if (linesWritten() !== homedAt) {
    block(["G91", "G28", "Z0"]);
    block(["G28", "X0", "Y0"]);
  }
  block(["M30"]);
  writeln("%");
}

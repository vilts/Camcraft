// Heidenhain TNC 3X milling post - the control family's documented
// DIN/ISO programming mode, not the native conversational language
// (a KLAR-style post is a separate, later candidate; see
// RECOMMENDED.md M3.3 B). Provenance: HEIDENHAIN "TNC 640 User's
// Manual, ISO programming", ID 892909-25, 9/2016 (public user
// documentation, behavior only, per this repo's Clean-Room Protocol)
// for the block form, words and functions (#49, #50); the hook
// contract from Autodesk's published post docs. No vendor script
// text; original work. Where the manual leaves a hole, the post
// refuses by name or the house style decides, and the conformance
// goldens lock that decision, not any vendor's.
description = "Heidenhain TNC (3-axis mill, DIN/ISO G200/G205/G206/G207 cycles and G247 presets)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
// TNC 640 User's Manual, ISO programming (ID 892909-25) p. 143: an
// ISO program carries the .I extension (#52).
extension = "I";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
axisInversion = true; // verified on the engine's mirrored stream for a flipped WCS X/Y and an inverted machine X/Y (#2475): trace equivalence test + golden
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it
floatingTapping = true; // #51: G206 requires the operation's explicit floating-holder choice
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 6;      // preset rows 1..6; more rows depend on the machine's preset table, not verified here (#931)
// #902: millimetres only for now - the engine refuses an inch document by
// name. The TNC runs inch (G70), but the feed unit its inch programs and
// cycle Q feeds take was not confirmed from the public manual this post
// is specced from, and a feed off by a factor is no program to post.
programUnits = "mm";
endUnits = "note"; // #2189: mm-only post; notes once when asked for inch

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var preset = null, activePreset = null;
var sRpm = 0;
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var setupStockTop = null; // #1647: the current sections stated stock top
var setupNext = null;      // #1647: null means no marker; a cleaned name may be empty
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;
var wcsFlip = "";          // #1649: the flipped WCS axes ("X", "Y", "XY") - the engine mirrored the section
var smoothTol = 0;          // #616: the section's PARAM Smoothing, mm (0: none asked)
var smoothMode = "";        // #616: "finish" or "rough"
var smoothActive = false;   // #616: this section wrote its smoothing words
var modalWait = false;  // G40 G90 rides the section's first motion block
var pendingDwell = null; // {z, s}: the open hole's dwell, owed at its bottom
var inCycle = false, cycleSig = "", cycleX = 0, cycleY = 0, cycleR = 0;
var orgX = 0, orgY = 0, orgZ = 0;
var pgmName = "CAMCRAFT";
var blockNo = 0;

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; }
// #50 - the block form of the manual (ID 892909-25): "The TNC numbers
// the blocks of a part program" (p. 130), each block a line ending in
// `*`, the first `%name G71 *`, the last `N99999999 %name G71 *`, and
// every coordinate word signed (`X+0 Y+38.5`, `Z-40`; pp. 131, 134).
// N steps by 10 as the manual's own programs do; a program long
// enough to reach the end block's number is refused, never wrapped.
var N_STEP = 10, N_LAST = 99999999;
var nRefused = false;
function sgn(letter, v) {  // a signed coordinate word, e.g. X+25 / Z-1.5
  var s = fu(v);
  return letter + (s.charAt(0) === "-" ? s : "+" + s);
}
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // one numbered block: N.. words *
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length === 0) return;
  blockNo += N_STEP;
  if (blockNo >= N_LAST && !nRefused) {
    nRefused = true;
    msg("program too long for TNC block numbers (N reaches 99999999, the end block's own number)");
  }
  writeln("N" + blockNo + " " + a.join(" ") + "*");
}
// A motion block: the section's first one also states G40 G90 (the
// manual's positioning form, `G00 G40 G90 Z+250*`, p. 66).
function motion(g, words) {
  var head = [g];
  if (modalWait) { head.push("G40", "G90"); modalWait = false; }
  block(head.concat(words));
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? sgn(letter, v - o) : ""; }
function arcWord(k, v) { return wo(k, v, k === "X" ? orgX : (k === "Y" ? orgY : orgZ)); }  // #3442: a position word as written, for arcFate
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}

// #1359 (from #1357): a cut needs a turning spindle. spindleOn is true
// from the preamble's spindle start (M3) to the next M5 (every
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
    msg(what + " with the spindle stopped - no spindle start (M3) precedes it; state the operation's spindle speed");
  }
  return true;
}
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Heidenhain post has no RTCP output"); }
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

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: the block ends at * and the comment
// opens at ;, so neither (nor %, parentheses) survives in the text; a
// comment is a numbered block of its own, `N.. ; text*`, as the TNC
// numbers every block; the work offset is left out, since this dialect
// posts the active preset, not a G54 word; printable ASCII only, capped
// at 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;*]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {  // the non-empty parts, " - " between
  var a = [];
  for (var i = 0; i < parts.length; i++) if (parts[i]) a.push(parts[i]);
  return a.join(" - ");
}
function note(s) { block(["; " + s]); }
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
  if (opName) note("OPERATION: " + joinDash([opName, opKind]));
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

// -------------------------------------------------------------- header
// #50: "The first block of a program is identified by %, the program
// name and the active unit of measure" (p. 130): `%NAME G71 *`, G71 =
// mm (p. 755, "at start of program"). The stream carries no program
// name, so the machine's name stands in, cut to the file-name
// characters the manual permits (A-Z a-z 0-9 . _ -, p. 143). The old
// parenthetical identity line is gone: `( )` is no TNC comment form.
// #258: the TNC addresses a program by its name, so the document's
// program comment IS the name when it states one (cut to the same
// characters); else a stated number names it (a name of digits is a
// legal file name); else the machine's, as before. A stated number the
// name does not carry is the operator's comment block after the begin.
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  var said = progComment();
  if (said) pgmName = said.replace(/[^A-Za-z0-9._-]/g, "_");
  else if (progNumbered()) pgmName = String(progNo());
  else pgmName = String(nm).replace(/[^A-Za-z0-9._-]/g, "_") || "CAMCRAFT";
  writeln("%" + pgmName + " G71 *");
  if (said && progNumbered()) note("PROGRAM " + progNo());
  invertedNotes();
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

// Cutter compensation in the control (M13.11, #39): refused by name,
// never a centre path under a Control op (the stream's contour is the
// PART contour). The TNC's documented engage rule (the manuals: the
// block that activates G41/G42 ends with the tool perpendicular to the
// programmed point, compensation taking effect only at its end) is
// not stated against the next element the way the Fanuc start-up is,
// so a perpendicular lead's engage point would be a guess; and the
// radius is always the tool table's R plus DR, so the wear variant's
// "the register holds only the wear delta" has no TNC form.
// docs/decisions/39-comp-dialects.md records the call.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by this post - choose In computer, or the Fanuc, Haas, Mazak or Siemens 840D post");
}

// In DIN/ISO G54 is DATUM SHIFT, not an offset register. Explicit
// app offsets instead select preset rows through G247 (#51): offset n
// is preset-table row n (#931: PARAM WorkOffset is that number).
function workOffsetName(n) {  // offset n -> what the control calls it; "" when none
  return n >= 1 && n <= 6 && n === Math.floor(n) ? "preset " + n : "";
}
function onParameter(name, value) {
  if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;  // #1633
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1633
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "StockZTop") { setupStockTop = has(value) ? Number(value) : null; return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "WorkOffset" && value) {
    // House mapping: offsets 1..6 select preset-table rows 1..6.
    // Legacy streams without WorkOffset keep the control's active preset.
    var n = Number(value);
    if (!workOffsetName(n)) { msg("work offset " + value + " has no configured TNC preset row"); return; }
    preset = n;
  }
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

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program. #50: the tool call is the T block, `T5 G17 S4500*`
// (pp. 71, 218: number, spindle axis, speed; it applies the tool's
// length) - M6 is machine-dependent (p. 392) and not written. A speed
// change without a tool change is a T block of the speed only (p.
// 201). G40 G90 ride the first positioning block. The old preamble
// words are gone: G54 (datum shift), G80 (WORKING PLANE, p. 756) are
// not what a TNC reads them as.
// #616: per-operation smoothing, only when the document turned it on
// (the stream then carries PARAM Smoothing / SmoothingMode). Heidenhain ISO G62 (the ISO form of the tolerance cycle): T the path deviation.
// Written after the preamble, undone at the section's end so no
// tolerance leaks into the next operation.
function smoothOn() {
  if (!(smoothTol > 0)) return;
  block(["G62", "T" + fu(smoothTol)]);
  smoothActive = true;
}
function smoothOff() {
  if (smoothActive) block(["G62"]);  // tolerance cycle with no T: the default again
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
  // #1649: the operator reads why every sign on a flipped axis is the other one.
  if (wcsFlip) note("WCS " + wcsFlip.split("").join(" AND ") + " FLIPPED - PROGRAM MIRRORED");
  modalWait = true;
  // Cycle 247 activates on definition; never call it as a hole cycle.
  // It resets coordinate transformations (892905-25 §10.4, p.275).
  if (preset !== null && preset !== activePreset) {
    block(["G247", "Q339=" + preset]);
    activePreset = preset;
    cycleSig = "";
  }
  if (toolWait) {
    toolNote();
    block(["T" + ni(tNo), "G17", spindleWait && sRpm > 0 ? "S" + ff(sRpm) : ""]);
    xyUnknown = true;          // #1633: the first traverse states its height again
    toolWait = false;
  } else if (spindleWait && sRpm > 0) {
    block(["S" + ff(sRpm)]);
  }
  if (spindleWait && sRpm > 0) {   // a touch probe (#523) states no spindle
    block(["M3"]);
    spindleOn = true;
  }
  if (probeStop) { block(["M5"]); spindleOn = false; }
  probeSection = probeStop && kindProbe;
  spindleWait = false;
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
  var top = setupStockTop;
  setupStockTop = null;
  if (!ask) return;
  if (next !== null && (ask === "optional-stop" || ask === "stop")) {
    // #1647: lift Z before inviting the operator to re-clamp the part.
    // No trusted machine datum: stay in the verified work clearance.
    if (!has(clearZ) || !has(top) || !(clearZ > top)) {
      msg(coolantWho() + "setup change needs a ClearancePlane above StockZTop - no safe Z lift is known");
      return;
    }
    var setupZ = has(properties.Z) ? Math.max(clearZ, Number(properties.Z)) : clearZ;
    motion("G00", [wo("Z", setupZ, orgZ)]);
    note("SETUP CHANGE - NEXT: " + (next || "UNNAMED SETUP"));
  }
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    // #50: this dialect has no home move it can trust - G53 is a
    // datum-table shift and M91 rides a builder-set datum - so a
    // trip home is refused by name, never guessed.
    msg(coolantWho() + "the Heidenhain post cannot go home after an operation (no machine-referenced home it can trust) - choose a stop instead");
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  sectionOpen = false;
  inCycle = false;
  cycleSig = "";
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
function onSectionEnd() { endPreamble(); wcsFlip = ""; clearZ = null; hopZ = null; }  // #1633: the next section states its own planes

// ------------------------------------------------------------- motions
// The manual's two-digit G00/G01/G02/G03 (p.752). Native cycles
// suppress only their replaced axial cuts; safe linking stays explicit.
// ------------------------------------------------- the first traverse
// #1633 (#1616's second half; the #1614 hazard): X and Y are unknown at
// the program's start and after every tool call, and so is the height
// the change left the tip at - a long tool on a high preset can stand
// below the part top there, and a first move written as one combined
// rapid (`G00 G40 G90 X+20 Y+10 Z-3*`) is a straight line from that
// point, across the part. So the first X/Y traverse is made at a STATED
// height: a Z-only rapid to the section's clearance - its
// ClearancePlane, else its RetractPlane, never below the move's own Z -
// in the manual's own positioning form (`G00 G40 G90 Z+250*`, p. 66;
// the first motion block carries G40 G90), then X/Y alone at it, then
// the move as the stream states it. A setup that cannot reach that
// height is stopped by the control's Z limit instead of rapiding
// through the part. A stream that states neither plane has no verified
// height to traverse at: refused by name. This dialect refuses tilted
// records (tiltOk), so the rapids are in the frame of the section's
// upright words.
var clearZ = null;            // #1633: the section's ClearancePlane, null when unstated
var hopZ = null;              // #1633: the section's RetractPlane, null when unstated
var xyUnknown = true;         // #1633: at the program's start and after each tool call
var traverseRefused = false;  // #1633: once per program
function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}
// (x, y, z): a rapid's target (a drilling cycle's hole at its R plane),
// which the record itself then goes to; or, with `descend`, where the
// stream says a feed or an arc starts - rapided down to when it lies
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
  motion("G00", [wo("Z", tz, orgZ)]);
  // A rapid already at the clearance is the traverse itself.
  if (descend || !has(z) || Math.abs(Number(z) - tz) > 1e-9) motion("G00", [wo("X", x, orgX), wo("Y", y, orgY)]);
  if (descend && has(z) && Number(z) < tz - 1e-9) motion("G00", [wo("Z", z, orgZ)]);
  xyUnknown = false;
  return true;
}
function onRapid() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (inCycle && sameHole(properties) && properties.Z <= cycleR + 1e-6) return;
  inCycle = false; // retain the generator's explicit safe lifts/traverses
  if (!xyFirst(properties.X, properties.Y, properties.Z, false)) return;  // #1633: refused - the move is not written
  motion("G00", xyz(properties));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (inCycle) {
    if (!sameHole(properties) || properties.Z > cycleR + 1e-6) msg("native drilling cycle has an inconsistent expanded cutting chain");
    return;
  }
  if (spindleStopped("feed move G01")) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  motion("G01", xyz(properties).concat([fw(properties.FeedRate)]));
  dwellAtBottom(properties.Z);
}
// M11.2 A - arcs off the XY plane. This family's DIN/ISO mode binds
// the main plane to the spindle axis the T block sets (the published
// ISO programming manuals: a circle belongs to the main plane defined
// with the tool axis - G17 for a Z spindle; circles off it are the
// tilted-plane function's or Q-parameter work). A free G18/G19 word is
// therefore not this dialect's way to state an XZ/YZ arc, and writing
// one would be re-interpreting the program's tool axis. The house
// choice: a vertical-plane arc posts as its chord polyline, G01 blocks
// whose sagitta stays within ARC_CHORD_TOL (1 um - under the 3-decimal
// word resolution), the normal coordinate linear along the sweep. The
// motion is the arc's own to the output resolution; the plane word
// is never touched, so G17 stays the program's only plane.
// #18 made this final: the public manuals allow G2/G3 with axes off
// the working plane only as a spatial-arc case and state no rotation
// sense for it (its I/J/K are absolute centres, #49) -
// a native XZ/YZ arc would be a guess at a mirrorable direction.
var ARC_CHORD_TOL = 0.001;
var ARC_MAX_CHORDS = 1024;
function verticalArc(p) {
  var c = p.IJKIncrement || {};
  var yz = p.CircularPlane === "YZ";
  // The right-handed in-plane frame (u, v) with u x v = +normal:
  // XZ is (Z, X) about +Y, YZ is (Y, Z) about +X - so CCW (G3, seen
  // from the positive normal) is the ordinary atan2 sense in both.
  var cu = yz ? c.J : c.K, cv = yz ? c.K : c.I;
  var u0 = yz ? p.StartY : p.StartZ, v0 = yz ? p.StartZ : p.StartX;
  var w0 = yz ? p.StartX : p.StartY;
  var u1 = yz ? p.Y : p.Z, v1 = yz ? p.Z : p.X, w1 = yz ? p.X : p.Y;
  var ra = Math.sqrt((u0 - cu) * (u0 - cu) + (v0 - cv) * (v0 - cv));
  var rb = Math.sqrt((u1 - cu) * (u1 - cu) + (v1 - cv) * (v1 - cv));
  var r = Math.max(ra, rb);
  var a0 = Math.atan2(v0 - cv, u0 - cu), a1 = Math.atan2(v1 - cv, u1 - cu);
  var ccw = p.CircularDirection !== "CW";
  var d = ccw ? a1 - a0 : a0 - a1;
  d = ((d % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  // #3442: equal ends are a full turn stated as one record (the form the
  // stream reader takes), never a zero-length chord.
  if (u1 === u0 && v1 === v0) d = 2 * Math.PI;
  var n = 1;
  if (has(r) && has(d) && r > 1e-12 && d > 0) {
    var step = 2 * Math.acos(Math.max(-1, Math.min(1, 1 - Math.min(ARC_CHORD_TOL / r, 1))));
    n = step > 0 ? Math.ceil(d / step - 1e-9) : ARC_MAX_CHORDS;
    n = Math.max(1, Math.min(ARC_MAX_CHORDS, n));
  }
  for (var j = 1; j <= n; j++) {
    var x = p.X, y = p.Y, z = p.Z;
    if (j < n) {
      var f = j / n, th = a0 + (ccw ? d * f : -d * f);
      var u = cu + r * Math.cos(th), v = cv + r * Math.sin(th), wn = w0 + (w1 - w0) * f;
      if (yz) { x = wn; y = u; z = v; } else { x = v; y = wn; z = u; }
    }
    motion("G01", [wo("X", x, orgX), wo("Y", y, orgY), wo("Z", z, orgZ), fw(p.FeedRate)]);
  }
}

function onCircular() {
  startPreamble();
  if (!tiltOk(properties)) return;
  if (inCycle) { msg("native drilling cycle cannot replace an arc cutting chain"); return; }
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord);
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G02" : "G03"))) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  var p = properties;
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") {
    verticalArc(p);
    return;
  }
  // TNC 640 ISO manual 892909-25, pp. 271-272: a helix uses polar
  // interpolation. This post has no polar helix output; Cartesian
  // G02/G03 must stay in the working plane, including under a mirror.
  if (!has(p.StartZ) || !has(p.Z) || p.Z !== p.StartZ) {
    msg(coolantWho() + "Heidenhain helix requires polar interpolation - Cartesian G02/G03 with changing Z is not supported");
    return;
  }
  // #49: in this dialect I/J are the circle centre's Cartesian
  // coordinates - ABSOLUTE unless the word carries G91 (TNC 640 ISO
  // Programming User's Manual, ID 892909-25, 9/2016, pp. 260-261:
  // "Circle center I, J"; "Entering the circle center incrementally"
  // is relative to the last programmed position only when entered
  // incrementally; G91 is written before the word, p. 128). The
  // manual's form sets the centre in its own block before the
  // G02/G03 ("you must first enter the circle center I, J"), so it is
  // posted that way. Absolute: the work origin shifts the centre like
  // any position word (wo), never a start-relative difference.
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J)) {
    msg("arc without an XY centre - refused, not guessed");
    return;
  }
  block([wo("I", c.I, orgX), wo("J", c.J, orgY)]);
  motion(p.CircularDirection === "CW" ? "G02" : "G03", [
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
    fw(p.FeedRate),
  ]);
}

// ------------------------------------------------------------ drilling
// #51 provenance: TNC 640 Cycle Programming, ID 892905-25, 9/2016,
// §§3.3/3.8/4.2/4.3/10.4 (pp.81-82,95-98,111-116,275).
// G200/G205/G206/G207 define; M99 calls ONCE, on a positioning
// block (§2.1 p.60). No M89 modal calls and no G80 (working plane).
// Native cycles return to R; the expanded chain's higher lifts and
// XY traverses remain explicit. Only its drilling moves are replaced.
var DWELL_TOL = 1e-6;
function sameHole(p) {
  return Math.abs(p.X - cycleX) < DWELL_TOL && Math.abs(p.Y - cycleY) < DWELL_TOL;
}
// #3102: the top of the documented Q208 range (mm/min), the fastest retract.
var RETRACT_FEED_MAX = 99999.999;
function qword(n, v) {
  // Q values may need four decimals (not integer feed rounding, which
  // would change a tap's feed/pitch relation). Numeric ranges checked below.
  var s = Number(v).toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
  if (s === "-0" || !s) s = "0";
  return "Q" + n + "=" + (s.charAt(0) === "-" ? s : "+" + s);
}
function cycleBad(why) {
  msg(coolantWho() + "Heidenhain drilling/tapping: " + why);
}
function onCycle() {
  startPreamble();
  owedDwell();
  inCycle = false;
  var p = properties, type = p.CycleType;
  if (type === "BORE") {
    msg(coolantWho() + "bore (G86) requires a spindle stop before rapid retract - expanded boring is not supported by this post; refused, not posted");
    return;
  }
  // Back boring keeps its established explicit-chain contract.
  // #1445: REAM (feed in, feed out, no dwell) rides the same chain.
  if (type === "BACKBORE" || type === "REAM") {
    if (has(p.P) && p.P > 0) pendingDwell = { z: Number(p.Z), s: Number(p.P) };
    cycleSig = "";
    return;
  }
  cycleX = p.X; cycleY = p.Y; cycleR = p.R;
  inCycle = true; // even refused taps must never fall through to plain feeds
  if (!has(p.X) || !has(p.Y) || !has(p.R) || !has(p.Z) || !has(p.EntryZ)) {
    cycleBad("missing finite hole geometry"); return;
  }
  var clearance = p.R - p.EntryZ, depth = p.Z - p.EntryZ;
  var surface = p.EntryZ - orgZ;
  var dwell = has(p.P) ? Number(p.P) : 0;
  // #1357: the cycle's own feed, else its own hole chain's - never the
  // last modal feed of some other move; with neither it is refused below.
  var feed = has(p.FeedRate) ? Number(p.FeedRate) : (has(p.ChainFeedRate) ? Number(p.ChainFeedRate) : null);
  if (clearance < 0 || clearance > 99999.9999 || depth >= 0 || depth < -99999.9999 || Math.abs(surface) > 99999.9999) {
    cycleBad("needs a nonnegative R clearance and a negative surface-relative depth within the manual's range"); return;
  }
  if (!has(feed) || feed <= 0 || feed > 99999.999 || dwell < 0 || dwell > 3600) {
    cycleBad("missing/invalid positive cutting feed or bottom dwell"); return;
  }
  if (typeof machine !== "undefined" && machine.feedRateMax > 0 && feed > machine.feedRateMax) {
    cycleBad("cutting feed over machine maximum"); return;
  }
  var tap = type === "TAP", floating = p.TappingMode === "floating";
  if (has(p.ChainBottomZ) && Math.abs(p.ChainBottomZ - p.Z) > DWELL_TOL) {
    cycleBad("drilling dwell/depth disagrees with the expanded chain bottom"); return;
  }
  var g = tap ? (floating ? "G206" : "G207") : (type === "PECK" ? "G205" : "G200");
  var words = [g, qword(200, clearance), qword(201, depth)];
  if (tap) {
    if (!has(sRpm) || sRpm <= 0 || sRpm !== Math.round(sRpm)) { cycleBad("tapping needs a positive integral spindle speed to resolve pitch"); return; }
    var pitch = feed / sRpm;
    // Only G207 consumes Q239. G206 derives pitch from its feed and rpm.
    if (!floating && (pitch < 0.0001 || pitch > 99.9999)) { cycleBad("tapping pitch is outside the documented range"); return; }
    if (!floating && Math.abs(pitch - Math.round(pitch * 10000) / 10000) > 0.0000005) {
      cycleBad("tapping feed/spindle ratio cannot be represented at the control's pitch resolution"); return;
    }
    if (floating && Math.abs(feed - Math.round(feed * 1000) / 1000) > 0.0000005) {
      cycleBad("floating tapping feed cannot be represented at the control's feed resolution"); return;
    }
    if (floating && dwell > 0.5) { cycleBad("G206 floating tapping dwell must be at most 0.5 seconds to avoid tool wedging"); return; }
    // The current tool model is right-hand only (positive pitch and M3).
    // Do not infer a left-hand thread from a negative feed or rpm.
    if (!floating && typeof machine !== "undefined" && machine.rigidTapping === false) {
      cycleBad("rigid tapping is unavailable on this machine; select a floating holder explicitly if installed"); return;
    }
    if (!floating && dwell !== 0) { cycleBad("G207 rigid tapping has no dwell parameter"); return; }
    if (floating) words.push(qword(206, feed), qword(211, dwell));
    else words.push(qword(239, pitch));
  } else {
    if (type !== "SPOT" && type !== "DRILL" && type !== "PECK" && type !== "DEEP") {
      cycleBad("unsupported cycle kind " + type); return;
    }
    var pecked = type === "PECK" || type === "DEEP";
    // The generator counts pecks from R, the control from the surface.
    // A nonzero gap would change the first infeed: refuse rather than
    // silently deepen it. Single-plunge drills/taps allow that gap.
    if (pecked && Math.abs(clearance) > DWELL_TOL) {
      // Preserve the already-supported explicit chain when its air-gap
      // first infeed cannot be expressed by a constant native peck.
      inCycle = false; cycleSig = "";
      if (dwell > 0) pendingDwell = { z: Number(p.Z), s: dwell };
      return;
    }
    var step = pecked ? Number(p.Q) : -depth;
    if (!has(step) || step <= 0 || step > 99999.9999) { cycleBad("invalid peck/infeed depth"); return; }
    words.push(qword(206, feed), qword(202, step));
    if (g === "G200") words.push(qword(210, 0)); // no top-dwell intent
  }
  words.push(qword(203, surface), qword(204, clearance));
  if (g === "G205") {
    // Constant chip-break pecks, no decrement or deepened start. A
    // conservative feed re-approach avoids inventing an unverified rapid.
    // #3102: Q208 (retraction feed) is a number, never the word FMAX. An
    // iTNC 530 (606425 04) DIN/ISO import refuses the G205 block with
    // Q208=FMAX. The iTNC 530 cycle manual (670388-25, cycle 205) gives
    // the range 0..99999.999 and writes Q208=99999 in its own example;
    // the TNC 640 manual (892905-25) takes the same number. The top of
    // the range asks for the fastest retract that the control permits.
    words.push(qword(212, 0), qword(205, p.Q), qword(258, 0), qword(259, 0),
      qword(257, p.Q), qword(256, Math.min(1, p.Q)), qword(211, dwell),
      qword(379, 0), qword(253, feed), qword(208, RETRACT_FEED_MAX), qword(395, 0));
  } else if (g === "G200") words.push(qword(211, dwell), qword(395, 0));
  // #1359: checked last, so a tap's own named refusals (rpm, pitch)
  // still speak; inCycle stays set, so the hole's chain is swallowed too.
  if (spindleStopped("drilling cycle " + g)) return;
  var sig = words.join("|");
  if (sig !== cycleSig) { block(words); cycleSig = sig; }
  // G40/G90 accompanies explicit hole positioning, never a cycle definition.
  if (!xyFirst(p.X, p.Y, p.R, false)) return;  // #1633: refused - the move is not written
  motion("G00", [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.R, orgZ), "M99"]);
}
function dwellAtBottom(z) {
  if (pendingDwell === null || !has(z) || Math.abs(z - pendingDwell.z) > DWELL_TOL) return;
  block(["FUNCTION DWELL TIME" + fu(pendingDwell.s)]);
  pendingDwell = null;
}
// A dwell whose chain never reached its bottom is refused, never lost.
function owedDwell() {
  if (pendingDwell === null) return;
  pendingDwell = null;
  msg("drilling dwell without a feed move to the hole bottom - refused, not dropped");
}

// ------------------------------------------------------------ probing
// #173: deliberately no onProbeMove / onProbeRetreat / onProbeResult.
// The ISO user's manual this post is specced from documents no free
// skip/measuring move and no variable holding a trip position; TNC
// probing is the touch-probe cycles, whose call form and result Q
// parameters live in the separate touch-probe cycle manual, not the
// source of this post. Rather than guess a cycle number or a Q
// parameter, the engine refuses a Probe operation by name for this
// post (crates/app/tests/probe_dialects.rs pins it).

// ------------------------------------------------------------ footer
// #50: the program ends M2 (p. 66: `G00 Z+250 M2*`), then the end
// block `N99999999 %NAME G71 *` (p. 130). The old optional `/G53 Z0`
// homing is gone: G53 is a datum-table shift in this mode (p. 753),
// and machine-referenced moves (M91, p. 393) depend on a datum the
// machine builder sets - the program ends where its last retract left
// the tool, as the manual's own programs do.
// #2189: Heidenhain post is mm-only; note an inch ask once before M2.
function noteEndUnits() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch") {
    note("END UNITS NOT WRITTEN - HEIDENHAIN POST IS MM ONLY");
  }
}
function onClose() {
  endPreamble();
  noteEndUnits();
  block(["M2"]);
  writeln("N" + N_LAST + " %" + pgmName + " G71 *");
}

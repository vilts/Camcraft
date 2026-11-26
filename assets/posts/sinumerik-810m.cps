// Siemens Sinumerik 810M 3-axis milling post (#2888). Provenance:
// public ISO 6983-1 / DIN 66025 G/M-code block structure; published Siemens
// SINUMERIK 810M / 820M GA3 Programming Instructions (%MPF1-%MPF9999 main
// program header, ( ) comments with 41-char display / 120-char block limit,
// N1-N9999 4-digit block numbers, G70/G71 geometry-only units, G54-G57
// settable zero offsets vs G58/G59 programmable shifts, G53 non-modal
// suppression, T/D/M6, G04 F seconds dwell, L81-L89 R-parameter cycles);
// published EMCO WinNC Sinumerik 810M/820M cycle parameter docs (L84 R08
// -3 encoder vs -4 compensating chuck); hook contract from Autodesk's
// published post docs (behavior only) under Camcraft's Clean-Room Protocol.
// Original clean-room implementation; no vendor post or Fusion output used.
description = "Siemens 810M (3-axis mill, G71 mm, L81-L89 R-param cycles, G2/G3 I/J/K)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "mpf";
setCodePage = "utf-8";
capabilities = 1; // reference contract: bit 1 = milling
coolantModes = true;
afterModes = true;
axisInversion = true;
workOffsetIndex = true;
workOffsetMax = 4; // 810M settable frames are G54-G57 only; G58/G59 are programmable shifts
millSpindleReverse = true;
floatingTapping = true; // L84 R08=-3 (with encoder) and R08=-4 (floating holder, no encoder)
programUnits = "mm"; // 810M G70/G71 converts geometry only, leaving F feedrate unchanged
endUnits = "note"; // refuse silent unit switch where F feedrate stays metric

// Parenthesized comments; 41-char CRT comment line display cap on 810M.
explains = "paren";
commentMax = 41;
explainWords = { G71: "MM GEOMETRY INPUT", G70: "INCH GEOMETRY INPUT", G58: "", G59: "" };

declaredOptions = {
  showSequenceNumbers: {
    type: "enum",
    title: "Sequence numbers",
    group: "Format",
    description: "Where to emit N-word block numbers (omitted by default to save 810M RAM)",
    choices: [
      { id: "off", title: "Off" },
      { id: "every-block", title: "Every block" },
      { id: "tool-change", title: "On tool change" },
      { id: "op-start", title: "On operation start" },
    ],
    default: "off",
  },
  sequenceNumberStart: { type: "integer", title: "First sequence number", group: "Format", description: "Starting N number (1..9999)", min: 1, max: 9999, default: 10 },
  sequenceNumberIncrement: { type: "integer", title: "Sequence number increment", group: "Format", description: "Step between sequence numbers", min: 1, max: 1000, default: 10 },
  separateWordsWithSpace: { type: "boolean", title: "Separate words with space", group: "Format", description: "Insert a space between words (false trims RAM usage)", default: true },
  writeMachine: { type: "boolean", title: "Write machine header", group: "Header & comments", description: "Emit a comment naming the machine in the header", default: false },
  writeToolList: { type: "boolean", title: "Write tool list", group: "Header & comments", description: "Emit a summary comment per tool at program start", default: false },
  writeVersion: { type: "boolean", title: "Write post version", group: "Header & comments", description: "Emit post version metadata comment in the header", default: false },
  optionalStop: { type: "boolean", title: "Optional stop (M1)", group: "Behavior", description: "Write M1 before tool changes after the first operation", default: false },
};

// Determinism law: no timestamps or randomness.
// ---------------------------------------------------------------- limits & state
var BLOCK_MAX = 120;         // Sinumerik 810M GA3 maximum line length (characters)
var COMMENT_MAX = 39;        // "(" + 39 + ")" = 41 chars displayed on CRT comment line
var N_MAX = 9999;            // 810M N-word 4-digit limit

var wcsWord = "G54";
var wcsIndex = 1;
var sRpm = 0;
var spindleWait = false;
var spinRev = false;
var toolWait = false;
var tNo = 0;
var inCycle = false;
var cycX = 0, cycY = 0, cycR = 0;
var seatKnown = false;
var curX = null, curY = null, curZ = null;
var zAtSafe = false;
var hopZ = null;
var clearZ = null;
var coolant = "";
var coolantAsk = "flood";
var afterAsk = "";
var setupNext = "";
var homedAt = -1;
var coolantOp = "";
var sectionOpen = false;
var smoothTol = 0;
var smoothNamed = false;
var tiltRefused = false;
var compG = "";
var compOn = false;
var wearNoted = false;
var wcsFlip = "";
var orgX = 0, orgY = 0, orgZ = 0;
var xyUnknown = true;
var traverseRefused = false;
var retractRefused = false;
var nRefused = false;
var blockRefused = false;
var toolRefused = false;
var progEnded = false;
var seqNum = 0;
var seqNextForce = false;
var secStarted = 0;
var spindleOn = false;
var spindleRefused = false;
var probeSection = false;
var kindProbe = false;
var planeG = 17;

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trimNum(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fu(v) { return has(v) ? trimNum(Number(v).toFixed(3)) : "0"; }
function f4(v) { return has(v) ? trimNum(Number(v).toFixed(4)) : "0"; }
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; }
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function wo(letter, v, org) { return has(v) ? letter + fu(Number(v) - org) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }

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
  var start = optInt("sequenceNumberStart", 10);
  var inc = Math.max(1, optInt("sequenceNumberIncrement", 10));
  if (seqNum <= 0) seqNum = start;
  else seqNum += inc;
  if (seqNum < 1 || seqNum > N_MAX) {
    if (!nRefused) {
      nRefused = true;
      msg("sequence number N" + seqNum + " exceeds Sinumerik 810M 4-digit N9999 limit; disable block numbers or lower start/increment");
    }
    return "";
  }
  return "N" + seqNum;
}

function prefixN(text) {
  var force = seqNextForce;
  seqNextForce = false;
  var mode = optStr("showSequenceNumbers", "off");
  if (mode !== "every-block" && !force) return text;
  var n = nextSeq();
  return n ? n + " " + text : text;
}

function rawWriteln(text) {
  if (text.length > BLOCK_MAX) {
    if (!blockRefused) {
      blockRefused = true;
      msg("block exceeds Sinumerik 810M " + BLOCK_MAX + "-character limit (" + text.length + " chars)");
    }
    return;
  }
  writeln(text);
}

function emitWords(arr, allowN) {
  var sep = optBool("separateWordsWithSpace", true) ? " " : "";
  var out = [];
  for (var i = 0; i < arr.length; i++) if (arr[i]) out.push(arr[i]);
  if (out.length === 0) return;
  var c = typeof explain === "function" ? explain(out) : "";
  var line = out.join(sep);
  if (allowN) line = prefixN(line);
  if (c) line += (line ? " " : "") + c;
  rawWriteln(line);
}

function block() {
  var arr = (arguments.length === 1 && Array.isArray(arguments[0])) ? arguments[0] : arguments;
  emitWords(arr, true);
}

function rawBlock() {
  var arr = (arguments.length === 1 && Array.isArray(arguments[0])) ? arguments[0] : arguments;
  emitWords(arr, false);
}

function msg(s) {
  note("WARNING: " + clean(s));
  if (typeof message === "function") message(s);
}

// ------------------------------------------------------------ header & comments
function cleanRaw(s) {
  return String(s === null || s === undefined ? "" : s)
    .replace(/[()%;]/g, " ")
    .replace(/[^\x20-\x7E]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}
function clean(s) {
  var t = cleanRaw(s);
  return t.length > COMMENT_MAX ? t.substring(0, COMMENT_MAX).trim() : t;
}
function note(s) {
  var t = cleanRaw(s);
  if (!t) return;
  if (t.length <= BLOCK_MAX - 2) {
    rawWriteln("(" + t + ")");
    return;
  }
  var maxLen = BLOCK_MAX - 2;
  for (var i = 0; i < t.length; i += maxLen) {
    rawWriteln("(" + t.substring(i, i + maxLen) + ")");
  }
}
function commentText(s) {
  return String(s === null || s === undefined ? "" : s)
    .replace(/[()%;]/g, " ")
    .replace(/[^\x20-\x7E]/g, "?");
}
function onComment(s) {
  var t = commentText(s);
  if (!t) return;
  var maxLen = BLOCK_MAX - 2;
  if (t.length <= maxLen) {
    rawWriteln("(" + t + ")");
    return;
  }
  for (var i = 0; i < t.length; i += maxLen) {
    rawWriteln("(" + t.substring(i, i + maxLen) + ")");
  }
}

function joinDash(parts) {
  var a = [];
  for (var i = 0; i < parts.length; i++) if (parts[i]) a.push(parts[i]);
  return a.join(" - ");
}

function progNo() {
  var n = typeof program !== "undefined" ? program.number : (typeof programNumber !== "undefined" ? programNumber : 1);
  return Math.round(Number(n) || 1);
}
function progNumbered() { return typeof program !== "undefined" && program.numbered === true; }
function progComment() { return clean(typeof program !== "undefined" ? program.comment : ""); }
function mpfHeader() {
  var n = progNo();
  if (n < 1 || n > N_MAX) {
    throw new Error("program number " + n + " is outside %MPF1-%MPF9999, the Sinumerik 810M main program numbers");
  }
  return "%MPF" + n;
}

var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
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
  note(joinDash(["T" + tNo + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

function invertedNotes() {
  var a = (typeof machine !== "undefined" && machine && machine.invertedAxes) || [];
  var where = { X: "LEFT", Y: "TOWARD THE FRONT", Z: "DOWN" };
  for (var i = 0; i < a.length; i++) {
    var l = String(a[i]);
    note("MACHINE " + l + "+ MOVES " + (where[l] || "THE OTHER WAY") + " - INVERTED AXIS");
  }
}

function writeHeaderExtras() {
  if (optBool("writeMachine", false)) {
    var nm = clean((typeof machine !== "undefined" && machine && (machine.model || machine.name)) || "");
    if (nm) note("MACHINE: " + nm);
  }
  if (optBool("writeVersion", false)) note("POST: CAMCRAFT SINUMERIK-810M");
  if (optBool("writeToolList", false) && typeof program !== "undefined" && Array.isArray(program.tools)) {
    for (var i = 0; i < program.tools.length; i++) {
      var t = program.tools[i];
      if (!t) continue;
      var line = "T" + ni(t.number) + (t.name ? " " + clean(t.name) : "");
      if (has(t.diameter) && t.diameter > 0) line += " D" + fu(t.diameter);
      note(line);
    }
  }
}

function onOpen() {
  var nm = (typeof machine !== "undefined" && machine && (machine.model || machine.name)) || "CAMCRAFT";
  rawWriteln(mpfHeader());
  var pc = progComment();
  var hdr = progNumbered() ? "PROGRAM " + progNo() + (pc ? " - " + pc : "") : (pc || clean(nm) || "CAMCRAFT");
  note(hdr);
  writeHeaderExtras();
  invertedNotes();
}

// ------------------------------------------------------------ machine & offsets
function safeZ() {
  var m = typeof machine !== "undefined" ? machine : null;
  var stated = m ? m.safeRetractZ : null;
  if (has(stated)) return Number(stated);
  var nm = clean((m && (m.model || m.name)) || "");
  if (!nm && (!m || !(m.spindleMax > 0))) return 0;
  if (!retractRefused) {
    retractRefused = true;
    msg((nm ? "machine " + nm + " does not state" : "no machine states") +
        " its safe Z retract - Sinumerik 810M lifts Z with G0 G53 Z<safe_retract> D0 before tool changes and at end");
  }
  return null;
}

function retractSafeZ() {
  if (zAtSafe) return;
  var z = safeZ();
  if (z === null) return;
  block(["G0", "G53", "Z" + fu(z), "D0"]);
  zAtSafe = true;
  curZ = null;
}

function workOffsetName(index) {
  if (index >= 1 && index <= 4) return "G" + (53 + index);
  return "";
}

function coolantWho() {
  return (setupName ? setupName + " / " : "") + (coolantOp ? coolantOp + ": " : "");
}

function coolantFor(ask) {
  if (!ask || ask === "none") return "";
  var list = (typeof machine !== "undefined" && machine && machine.coolants) || null;
  var norm = function(s) { return String(s || "").toLowerCase().replace(/[\s_]+/g, "-"); };
  var target = norm(ask);
  if (!list) return target === "flood" ? "M8" : (msg(coolantWho() + "no machine states coolant " + ask + " - refused, not guessed"), "");
  for (var i = 0; i < list.length; i++) {
    if (norm(list[i].name) === target || (target === "air" && norm(list[i].name) === "air-blast")) {
      return list[i].mCode || "";
    }
  }
  if (target === "flood" && list.length === 0) return "";
  msg(coolantWho() + "machine does not state coolant \"" + ask + "\" - refused by name");
  return "";
}

function spindleStopped(what) {
  if (probeSection || spindleOn) return false;
  if (!spindleRefused) {
    spindleRefused = true;
    msg(coolantWho() + what + " with the spindle stopped (spindle speed is 0 or not stated)");
  }
  return true;
}

// ------------------------------------------------------------ parameters & preamble
function onParameter(name, value) {
  commentParam(name, value);
  if (name === "OperationName") coolantOp = clean(value);
  else if (name === "SpindleSpeed" && has(value)) { sRpm = Number(value); spindleWait = true; }
  else if (name === "MillSpindle") spinRev = String(value) === "reverse";
  else if (name === "WorkOffset" && has(value)) {
    var idx = Math.round(Number(value));
    var wName = workOffsetName(idx);
    if (wName) {
      wcsIndex = idx;
      wcsWord = wName;
    } else {
      msg("Sinumerik 810M supports settable zero offsets G54-G57 only (work offset " + idx + "; G58/G59 are programmable shifts) - refused");
    }
  }
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "ClearancePlane" && has(value)) clearZ = Number(value);
  else if (name === "RetractPlane" && has(value)) hopZ = Number(value);
  else if (name === "Coolant") coolantAsk = String(value || "none");
  else if (name === "AfterOperation") afterAsk = String(value || "none");
  else if (name === "SetupChange") setupNext = clean(value);
  else if (name === "Smoothing" && has(value) && Number(value) > 0) smoothTol = Number(value);
  else if (name === "WcsFlip") wcsFlip = String(value || "");
}

function onToolChange(number, name) {
  tNo = Math.round(Number(number) || 0);
  toolName = clean(name);
  toolWait = true;
  if (tNo < 1 || tNo > 99) {
    if (!toolRefused) {
      toolRefused = true;
      msg("tool number " + tNo + " is outside D1-D99, the Sinumerik 810M tool offset registers");
    }
  }
}

function noteSmoothing() {
  if (!(smoothTol > 0) || smoothNamed) return;
  note("SMOOTHING NOT WRITTEN - NO 810M FORM STATED");
  smoothNamed = true;
}

function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  var idx = secStarted;
  secStarted += 1;
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  opNotes();
  if (wcsFlip) note("WCS " + wcsFlip.split("").join(" AND ") + " FLIPPED - PROGRAM MIRRORED");
  block(["G17", "G71", "G90", "G94", wcsWord, "G40", "G80"]);
  planeG = 17;
  if (toolWait) {
    if (idx > 0 && optBool("optionalStop", false)) {
      retractSafeZ();
      block(["M1"]);
    }
    toolNote();
    retractSafeZ();
    xyUnknown = true;
    curX = null; curY = null;
    if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
    block(["T" + ni(tNo), "M6"]);
    block(["D" + ni(tNo)]);
    toolWait = false;
  }
  var touchProbe = spindleWait && !(sRpm > 0);
  probeSection = touchProbe && kindProbe;
  if (spindleWait && sRpm > 0) {
    block(["S" + ni(sRpm), spinRev ? "M4" : "M3"]);
    spindleOn = true;
  }
  spindleWait = false;
  coolant = touchProbe ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);
  noteSmoothing();
}

function restorePlane() {
  if (planeG !== 17) {
    block(["G17"]);
    planeG = 17;
  }
}

function endPreamble() {
  if (!sectionOpen) return;
  if (inCycle) { block(["G80"]); inCycle = false; }
  restorePlane();
  sectionOpen = false;
  smoothTol = 0;
  wcsFlip = "";
  if (compOn || compG) {
    msg("cutter compensation still engaged at section end - no cancel block");
    compG = ""; compOn = false;
  }
  wearNoted = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";
  block(["M5"]);
  spindleOn = false;
  spinRev = false;
  probeSection = false;
  kindProbe = false;
  afterOp();
}

function afterOp() {
  var ask = afterAsk, next = setupNext;
  afterAsk = ""; setupNext = "";
  if (next && (!ask || ask === "none")) ask = "stop";
  if (!ask || ask === "none") return;
  retractSafeZ();
  if (ask === "home") {
    var z = safeZ();
    if (z === null) return;
    block(["G0", "G53", "X0", "Y0"]);
    xyUnknown = true;
    curX = null; curY = null;
    if (next) {
      note("SETUP CHANGE - NEXT: " + next);
      block(["M0"]);
    }
    homedAt = linesWritten();
    return;
  }
  if (next) note("SETUP CHANGE - NEXT: " + next);
  if (ask === "optional-stop") block(["M1"]);
  else if (ask === "stop") block(["M0"]);
  else msg("unknown AfterOperation ask: " + ask);
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); hopZ = null; clearZ = null; }

// ------------------------------------------------------------ geometry & comp
function tiltOk(p) {
  if ((has(p.A) && Math.abs(p.A) > 1e-6) || (has(p.B) && Math.abs(p.B) > 1e-6) || (has(p.C) && Math.abs(p.C) > 1e-6)) {
    if (!tiltRefused) {
      tiltRefused = true;
      msg("Sinumerik 810M is a 3-axis milling post - rotary/tilted moves (A/B/C) are refused by name");
    }
    return false;
  }
  return true;
}

function onRadiusCompensation(side) {
  startPreamble();
  if (inCycle) { msg("cutter compensation inside a canned-cycle group"); return; }
  if (side === "left" || side === "right") {
    if (planeG !== 17) {
      msg("cutter compensation requires G17 XY plane on Sinumerik 810M (refused on G" + planeG + ")");
      return;
    }
    compG = side === "left" ? "G41" : "G42";
    compOn = true;
    if (properties.CompensationType === "wear" && !wearNoted) {
      note("WEAR COMP - D" + ni(tNo) + " HOLDS WEAR DELTA ONLY");
      wearNoted = true;
    }
  } else {
    compG = compOn ? "G40" : "";
    compOn = false;
  }
}

function compRefuse(what) {
  if (!compG) return;
  msg("pending cutter compensation (" + compG + ") before " + what + " - refused, not guessed");
  compG = "";
}

function xyz(p) {
  return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)];
}

function trackPos(x, y, z) {
  if (has(x)) curX = Number(x);
  if (has(y)) curY = Number(y);
  if (has(z)) curZ = Number(z);
  zAtSafe = false;
}

function onHole(p) {
  return Math.abs(p.X - cycX) < 1e-6 && Math.abs(p.Y - cycY) < 1e-6 && p.Z <= cycR + 1e-6;
}
function chainRapid() {
  seatKnown = true;
  if (!inCycle) return false;
  if (onHole(properties)) return true;
  inCycle = false;
  return false;
}
function chainCut() {
  seatKnown = true;
  if (!inCycle) return false;
  if (!onHole(properties)) msg("a cutting move leaves its hole inside a canned-cycle group");
  return true;
}

function traverseZ(z) {
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) return null;
  return has(z) ? Math.max(c, Number(z)) : c;
}

function xyFirst(x, y, z, descend) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  var tz = traverseZ(z);
  if (tz === null) {
    if (!traverseRefused) {
      traverseRefused = true;
      msg(coolantWho() + "first X/Y traverse with no clearance height stated (no ClearancePlane or RetractPlane) - refused");
    }
    return false;
  }
  block(["G0", wo("Z", tz, orgZ)]);
  curZ = tz; zAtSafe = false;
  if (descend || !has(z) || Math.abs(Number(z) - tz) > 1e-9) {
    block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
    curX = Number(x); curY = Number(y);
  }
  if (descend && has(z) && Number(z) < tz - 1e-9) {
    block(["G0", wo("Z", z, orgZ)]);
    curZ = Number(z);
  }
  xyUnknown = false;
  return true;
}

function onRapid() {
  startPreamble();
  compRefuse("a rapid");
  if (!tiltOk(properties) || chainRapid()) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z, false)) return;
  block(["G0"].concat(xyz(properties)));
  trackPos(properties.X, properties.Y, properties.Z);
}

function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || chainCut()) return;
  if (spindleStopped("feed move G1")) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;
  var cg = compG;
  compG = "";
  block([cg, "G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
  trackPos(properties.X, properties.Y, properties.Z);
}

function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}

function centerWords(p, pl) {
  var c = p.IJKIncrement || {};
  var dx = has(c.I) && has(p.StartX) ? c.I - p.StartX : c.I;
  var dy = has(c.J) && has(p.StartY) ? c.J - p.StartY : c.J;
  var dz = has(c.K) && has(p.StartZ) ? c.K - p.StartZ : c.K;
  if (pl === 18) return [w("I", dx), w("K", dz)];
  if (pl === 19) return [w("J", dy), w("K", dz)];
  return [w("I", dx), w("J", dy)];
}

function onCircular() {
  startPreamble();
  compRefuse("an arc");
  if (!tiltOk(properties) || inCycle) return;
  var g = properties.CircularDirection === "CW" ? "G2" : "G3";
  if (spindleStopped("arc move " + g)) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;
  var pl = planeOf(properties);
  var pw = "";
  if (pl !== planeG) {
    if (compOn) {
      msg("plane change G" + pl + " while cutter compensation is active - refused");
      return;
    }
    pw = "G" + pl;
    planeG = pl;
  }
  block([pw, g].concat(xyz(properties), centerWords(properties, pl), [fw(properties.FeedRate)]));
  trackPos(properties.X, properties.Y, properties.Z);
}

function onDwell(seconds) {
  if (inCycle) return; // cycle dwells ride R04 in L82/L83/L84/L85/L86/L89
  if (has(seconds) && Number(seconds) > 0) {
    block(["G4", "F" + fu(seconds)]);
  }
}

// ------------------------------------------------------------ drilling L81-L89
function onCommand(cmd) {
  return true;
}

function onCycle() {
  startPreamble();
  restorePlane();
  compRefuse("a canned cycle");
  if (!tiltOk(properties)) return;
  var p = properties;
  var type = String(p.CycleType || "DRILL");
  var lCode = "L81";
  if (type === "SPOT" || type === "DRILL") lCode = has(p.P) && Number(p.P) > 0 ? "L82" : "L81";
  else if (type === "PECK" || type === "DEEP") lCode = "L83";
  else if (type === "TAP" || type === "FLOATINGTAP") lCode = "L84";
  else if (type === "REAM") lCode = "L85";
  else if (type === "BORE") lCode = "L86";
  else if (type === "BACKBORE") lCode = "L89";
  else {
    msg("unsupported canned cycle type " + type + " on Sinumerik 810M");
    return;
  }
  if (spindleStopped("canned cycle " + lCode)) return;
  var feed = has(p.FeedRate) ? Number(p.FeedRate) : (has(p.ChainFeedRate) ? Number(p.ChainFeedRate) : 0);
  if (!(feed > 0)) {
    msg(coolantWho() + "canned cycle " + lCode + " has no feed rate - refused");
    return;
  }
  var rPlane = Number(p.R), depth = Number(p.Z);
  if (!has(rPlane) || !has(depth) || !(depth < rPlane)) {
    msg(coolantWho() + "canned cycle " + lCode + " requires bottom Z below reference plane R");
    return;
  }
  var hop = has(hopZ) ? hopZ : (has(clearZ) ? clearZ : null);
  if (hop === null) {
    msg(coolantWho() + "canned cycle " + lCode + " requires a RetractPlane or ClearancePlane");
    return;
  }
  var retract = Math.max(hop, rPlane);
  var dwell = has(p.P) ? Math.max(0, Number(p.P)) : 0;

  if (!xyFirst(p.X, p.Y, retract, false)) return;
  if (!has(curX) || !has(curY) || Math.abs(curX - p.X) > 1e-6 || Math.abs(curY - p.Y) > 1e-6) {
    if (has(curZ) && curZ < retract - 1e-6) {
      block(["G0", wo("Z", retract, orgZ)]);
      curZ = retract;
    }
    block(["G0", wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
    curX = Number(p.X); curY = Number(p.Y);
  }
  if (has(curZ) && curZ < rPlane - 1e-6) {
    block(["G0", wo("Z", retract, orgZ)]);
    curZ = retract;
  }

  var rRef = "R02=" + fu(rPlane - orgZ);
  var rDep = "R03=" + fu(depth - orgZ);
  var rRet = "R10=" + fu(retract - orgZ);

  if (lCode === "L81") {
    block([rRef, rDep, rRet, fw(feed)]);
    block(["L81", "P1"]);
  } else if (lCode === "L82") {
    block([rRef, rDep, "R04=" + fu(dwell), rRet, fw(feed)]);
    block(["L82", "P1"]);
  } else if (lCode === "L83") {
    var peck = has(p.Q) ? Number(p.Q) : 0;
    if (!(peck > 0)) {
      msg(coolantWho() + "canned cycle L83 requires a positive peck increment Q");
      return;
    }
    var chipMode = type === "DEEP" ? "1" : "0";
    block(["R00=0", "R01=" + fu(peck), rRef, rDep, "R04=" + fu(dwell)]);
    block(["R05=0", rRet, "R11=" + chipMode, fw(feed)]);
    block(["L83", "P1"]);
  } else if (lCode === "L84") {
    var m = typeof machine !== "undefined" ? machine : null;
    var floating = String(p.TappingMode || "") === "floating" || String(type) === "FLOATINGTAP" || (m && m.rigidTapping === false);
    if (!floating) {
      if (dwell > 0) {
        msg(coolantWho() + "canned cycle L84 with encoder (R08=-3) does not support bottom dwell R04; set dwell to 0 or use floating tapping");
        return;
      }
      var pitch = sRpm > 0 ? feed / sRpm : 0;
      if (!(pitch > 0)) {
        msg(coolantWho() + "canned cycle L84 requires positive spindle RPM and feed rate to derive pitch R09");
        return;
      }
      block([rRef, rDep, "R06=0", "R08=-3", "R09=" + f4(pitch), rRet, "R11=3"]);
    } else {
      block([rRef, rDep, "R04=" + fu(dwell), "R06=" + (spinRev ? "3" : "4"), "R07=" + (spinRev ? "4" : "3"), "R08=-4", rRet, "R11=3", fw(feed)]);
    }
    block(["L84", "P1"]);
  } else if (lCode === "L85") {
    block([rRef, rDep, "R04=" + fu(dwell), rRet, "R16=" + ff(feed), "R17=" + ff(feed), fw(feed)]);
    block(["L85", "P1"]);
  } else if (lCode === "L86") {
    block([rRef, rDep, "R04=" + fu(dwell), "R07=" + (spinRev ? "4" : "3"), rRet, "R12=0", "R13=0", fw(feed)]);
    block(["L86", "P1"]);
  } else if (lCode === "L89") {
    block([rRef, rDep, "R04=" + fu(dwell), rRet, fw(feed)]);
    block(["L89", "P1"]);
  }
  cycX = Number(p.X);
  cycY = Number(p.Y);
  cycR = rPlane;
  curZ = retract;
  zAtSafe = false;
  seatKnown = true;
  inCycle = true;
}

function onCyclePoint() {}
function onCycleEnd() {}

// ------------------------------------------------------------ footer
function noteEndUnits() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch") {
    note("END UNITS NOT WRITTEN - 810M G70 LEAVES FEED IN MM/MIN");
  }
}

function onClose() {
  if (progEnded) return;
  progEnded = true;
  endPreamble();
  if (linesWritten() !== homedAt) {
    retractSafeZ();
  }
  noteEndUnits();
  block(["M30"]);
}

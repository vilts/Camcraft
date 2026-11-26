// Provenance: original clean-room RichAuto DSP A11 3-axis router post (#1914), specced from OEM A11 controller documentation (G-code/M-code, AbsCntr Off, Read/Ign G54, VFD S1-S8 multi-speed wiring), ISO 6983, and the block format of a program that a Laguna Swift A11 runs (#1907: .mmg, N step 10, no spaces, three decimals, % end); no vendor post text used.

description = "RichAuto DSP A11 (metric, pendant work origin, single-tool 3-axis router; AbsCntr Off; expanded drilling)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;

extension = "mmg";
setCodePage = "ascii";
capabilities = 1;
serves = ["mill"];
programUnits = "mm";
endUnits = "note";
workOffsetIndex = true;
workOffsetMax = 1;
coolantModes = true;

declaredOptions = {
  SpindleDwellMs: {
    type: "number",
    title: "Spindle dwell (ms)",
    group: "Spindle",
    description: "Optional dwell in milliseconds (G04 P<ms>, 0..2147483647) after S.. M3 spindle start",
    min: 0,
    max: 2147483647,
    default: 0
  }
};

// ------------------------------------------------------------------- state
var failed = false;
var tiltRefused = false;
var spindleDwellMs = 0;
var endUnitsNoted = false;

var firstTool = null;
var curTool = null;
var toolName = "";
var toolDia = 0;
var toolCommented = false;

var orgX = 0;
var orgY = 0;
var orgZ = 0;
var clearZ = null;
var stockTop = null;

var rpm = 0;
var rpmSet = false;
var spindleOn = false;

var coolantAsk = "flood";
var coolantOn = false;
var opName = "";
var coolantOp = "";
var badOffset = null;

var sectionOpen = false;
var started = false;
var xyUnknown = true;
var posKnown = false;
var startParamKnown = false;
var plane = 17;
var lastFeed = null;

var pendingCycle = null;
var needBoreRestart = null;

// #1907: every block carries a sequence number, N10 and up in steps of 10.
var seq = 0;
// Modal axis words in work coordinates (formatted); null = unknown.
var lastWord = { X: null, Y: null, Z: null };
var lastFeedWord = null;

// ----------------------------------------------------------------- helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }

function fail(reason) {
  if (!failed) failed = true;
  message(reason);
}

function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 48 ? t.substring(0, 48).trim() : t;
}

function note(s) {
  var c = clean(s);
  if (c) writeln("(" + c + ")");
}

// Header notes and WARN records (#2996): the A11's comment form, the
// full text kept (no 48-character cap), the delimiters removed.
function onComment(s) {
  var c = commentText(s);
  if (c) writeln("(" + c + ")");
}

// #1907: lengths with three decimals (Z5.080, Z0.000).
function fu(v) {
  var s = Number(v).toFixed(3);
  return /^-0\.0+$/.test(s) ? s.substring(1) : s;
}

function ni(v) { return String(Math.round(Number(v))); }

function wo(letter, v, o) { return has(v) ? letter + fu(Number(v) - o) : ""; }
function arcWord(k, v) { return wo(k, v, k === "X" ? orgX : (k === "Y" ? orgY : orgZ)); }  // #3442: a position word as written, for arcFate

function forgetPosition() {
  lastWord = { X: null, Y: null, Z: null };
}

// Modal axis words: a word equal to the last one written is left out.
function modalAxis(letter, v, o) {
  var w = wo(letter, v, o);
  if (w === "" || w === lastWord[letter]) return "";
  lastWord[letter] = w;
  return w;
}

function xyz(p) {
  return [modalAxis("X", p.X, orgX), modalAxis("Y", p.Y, orgY), modalAxis("Z", p.Z, orgZ)];
}

// #1907: no spaces between words, N on every block (comments excepted).
function block(words) {
  var out = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) out.push(words[i]);
  }
  if (out.length === 0) return;
  seq += 10;
  writeln("N" + seq + out.join(""));
}

// A motion block whose axis words are all modal (no move) is not written.
function motion(g, axes, tail) {
  var any = false;
  for (var i = 0; i < axes.length; i++) if (axes[i] !== "") any = true;
  if (!any) return;
  block([g].concat(axes, tail || []));
}

function parseSpindleDwell() {
  if (typeof postOptions === "undefined" || !postOptions) return 0;
  var v = postOptions.SpindleDwellMs;
  if (v === undefined || v === null) return 0;
  if (typeof v !== "number" || !isFinite(v) || v < 0 || v > 2147483647) {
    fail("SpindleDwellMs dwell must be a finite number in 0..2147483647 ms, got " + String(v));
    return 0;
  }
  return Math.ceil(v);
}

function checkUnitsOption() {
  if (typeof postOptions === "undefined" || !postOptions) return;
  if (postOptions.Units === "inch") {
    fail("RichAuto DSP A11 is metric-only (G21); inch programs are refused");
  }
}

function validateSpindle(r) {
  if (!rpmSet || !isFinite(r) || r <= 0) {
    fail("invalid spindle rpm: must be > 0");
    return false;
  }
  var maxRpm = (typeof machine !== "undefined" && machine && has(machine.spindleMax)) ? Number(machine.spindleMax) : 0;
  if (maxRpm > 0 && r > maxRpm) {
    fail("spindle rpm " + ni(r) + " exceeds machine spindleMax " + ni(maxRpm));
    return false;
  }
  return true;
}

function spinStart() {
  if (!validateSpindle(rpm)) return false;
  block(["M03", "S" + ni(rpm)]);
  if (spindleDwellMs > 0) block(["G04", "P" + ni(spindleDwellMs)]);
  spindleOn = true;
  return true;
}

function coolantWho() { return coolantOp ? "operation " + coolantOp + ": " : ""; }

function coolantCode() {
  var ask = String(coolantAsk || "flood").toLowerCase();
  if (ask === "off") return "";
  if (ask !== "flood") {
    fail(coolantWho() + "coolant " + ask + " is unsupported by RichAuto DSP A11 (use flood or off)");
    return "";
  }
  var cs = (typeof machine !== "undefined" && machine && machine.coolants) ? machine.coolants : [];
  if (!cs || cs.length === 0) return "";
  var floodEntry = null;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var name = String(typeof c === "string" ? c : (c && c.name) || "").trim().toLowerCase();
    if (name === "flood") {
      floodEntry = c;
      break;
    }
  }
  if (!floodEntry) {
    fail(coolantWho() + "machine coolants list has no Flood entry for coolant flood ask");
    return "";
  }
  var code = typeof floodEntry === "string" ? "" : String(floodEntry.mCode || "").trim().toUpperCase();
  if (code !== "M8") {
    fail(coolantWho() + "RichAuto DSP A11 flood coolant requires M8, got " + (code || "empty"));
    return "";
  }
  return "M8";
}

// Offset 1 is the work origin that the operator zeroed on the pendant.
// No G54 word is written (#1907: the A11 program carries none), so the
// pendant's Read G54 / Ign G54 setting cannot move the origin.
function workOffsetName(n) {
  return n === 1 ? "pendant work origin" : "";
}

function tiltOk(p) {
  if (!has(p.A) && !has(p.B) && !has(p.C)) return true;
  if (!tiltRefused) {
    tiltRefused = true;
    fail("rotary or tilted axis move is unsupported by the 3-axis RichAuto DSP A11 post");
  }
  return false;
}

function clearanceWorkZ() {
  if (!has(clearZ)) {
    fail("missing ClearancePlane for safe traverse");
    return null;
  }
  if (has(stockTop) && Number(clearZ) < Number(stockTop)) {
    fail("sub-stock ClearancePlane (" + fu(clearZ) + " < StockZTop " + fu(stockTop) + ") is unsafe");
    return null;
  }
  return Number(clearZ) - orgZ;
}

function startSectionBody() {
  if (failed) return false;
  if (started) return true;
  if (badOffset !== null) {
    fail("work offset " + badOffset + " is unsupported; RichAuto DSP A11 posts offset 1 (the pendant work origin) only");
    return false;
  }
  if (curTool === null) {
    fail("no tool selected for section");
    return false;
  }
  var cz = clearanceWorkZ();
  if (cz === null) return false;

  started = true;
  if (opName) {
    note("OPERATION: " + opName);
    opName = "";
  }
  if (!toolCommented) {
    var td = toolDia > 0 ? " D" + fu(toolDia) : "";
    note("T" + ni(curTool) + (toolName ? " " + toolName : "") + td);
    toolCommented = true;
  }
  if (!spinStart()) return false;
  var cc = coolantCode();
  if (failed) return false;
  if (cc) {
    block([cc]);
    coolantOn = true;
  }
  forgetPosition();
  motion("G00", [modalAxis("Z", cz, 0)]);
  return !failed;
}

function owedFloor() {
  if (pendingCycle === null) return;
  var pc = pendingCycle;
  pendingCycle = null;
  fail(
    "expanded drilling cycle at Z" + fu(pc.z) +
    (pc.ms > 0 ? " with dwell P" + ni(pc.ms) : "") +
    " never reached hole floor"
  );
}

function restorePlane() {
  if (plane !== 17) {
    block(["G17"]);
    plane = 17;
  }
}

function endSectionBody() {
  if (!started) return;
  owedFloor();
  if (needBoreRestart !== null && !failed) {
    needBoreRestart = null;
    fail("bore (G86) stopped spindle at floor without rapid retract to R plane");
  }
  restorePlane();
  if (!failed && has(clearZ)) {
    motion("G00", [modalAxis("Z", clearZ, orgZ)]);
  }
  if (spindleOn) {
    block(["M05"]);
    spindleOn = false;
  }
  if (coolantOn) {
    block(["M9"]);
    coolantOn = false;
  }
  started = false;
}

function xyFirst(x, y, z) {
  if (!xyUnknown) return true;
  if (!has(x) || !has(y)) return true;
  motion("G00", [modalAxis("X", x, orgX), modalAxis("Y", y, orgY)]);
  if (has(z)) motion("G00", [modalAxis("Z", z, orgZ)]);
  xyUnknown = false;
  return true;
}

function ensureFeed(rate) {
  if (has(rate)) {
    if (Number(rate) <= 0) {
      fail("cut feed rate must be positive, got " + String(rate));
      return null;
    }
    lastFeed = Number(rate);
    return lastFeed;
  }
  if (lastFeed !== null && lastFeed > 0) return lastFeed;
  fail("unknown feed rate before cutting move");
  return null;
}

// #1907: F in mm/min with one decimal (F1524.0), written only on change.
function feedWord(f) {
  var w = "F" + Number(f).toFixed(1);
  if (w === "F0.0") w = "F" + Number(f).toFixed(3);
  return w;
}

function modalFeed(f) {
  var w = feedWord(f);
  if (w === lastFeedWord) return "";
  return w;
}

function wroteFeed(w) {
  if (w !== "") lastFeedWord = w;
}

function feedMotion(g, axes, f) {
  var any = false;
  for (var i = 0; i < axes.length; i++) if (axes[i] !== "") any = true;
  if (!any) return;
  var fw = modalFeed(f);
  block([g].concat(axes, [fw]));
  wroteFeed(fw);
}

// ------------------------------------------------------------------- hooks
function onOpen() {
  checkUnitsOption();
  spindleDwellMs = parseSpindleDwell();
  if (failed) return;
  var title = (typeof program !== "undefined" && program && program.comment) ? program.comment : "";
  note("Filename: " + (clean(title) || "RICHAUTO DSP A11"));
  note("A11 MM - ABSCNTR OFF - INCREMENTAL I J K");
  note("WORK ORIGIN: THE ZERO SET ON THE PENDANT, NO G54");
  note("VFD MULTI-SPEED: 8 STEPS ON INPUTS S1-S8");
  block(["G21", "G90", "G17"]);
}

function onParameter(name, value) {
  if (name === "OperationName") {
    opName = clean(value);
    coolantOp = (value === null || value === undefined) ? "" : String(value).trim();
  }
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "WorkOffset") {
    var n = Number(value);
    if (n === 1 || String(value).trim().toUpperCase() === "G54") {
      badOffset = null;
    } else {
      badOffset = value;
    }
  }
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;
  else if (name === "StockZTop") stockTop = has(value) ? Number(value) : null;
  else if (name === "Coolant") coolantAsk = String(value || "flood");
  else if (name === "StartX" || name === "StartY") startParamKnown = true;
  else if (name === "SurfaceSpeed" && has(value) && Number(value) > 0) {
    fail("constant surface speed (G96) is unsupported by the 3-axis RichAuto DSP A11 post");
  }
}

function onToolChange(number, name) {
  var n = Number(number);
  if (!isFinite(n) || n < 1 || n !== Math.floor(n)) {
    fail("invalid tool number " + String(number) + "; use an integer >= 1");
    return;
  }
  if (firstTool !== null && n !== firstTool) {
    fail("single-tool RichAuto DSP A11 refuses mid-program tool change from T" + ni(firstTool) + " to T" + ni(n));
    return;
  }
  firstTool = n;
  curTool = n;
  toolName = clean(name);
}

function onSpindleSpeed() {
  rpm = Number(properties.SpindleSpeed) || 0;
  rpmSet = true;
}

function onRadiusCompensation() {
  fail("cutter radius compensation (G41/G42) is unsupported by RichAuto DSP A11; use In computer");
}

function onSection() {
  sectionOpen = true;
  started = false;
  xyUnknown = true;
  posKnown = startParamKnown;
  plane = 17;
  lastFeed = null;
  lastFeedWord = null;
  forgetPosition();
}

function onSectionEnd() {
  endSectionBody();
  sectionOpen = false;
  clearZ = null;
  stockTop = null;
  startParamKnown = false;
  coolantAsk = "flood";
  opName = "";
  coolantOp = "";
}

function onRapid() {
  var p = properties;
  if (!startSectionBody()) return;
  if (!tiltOk(p)) return;
  if (pendingCycle !== null) {
    var sameXY = Math.hypot((Number(p.X) - orgX) - pendingCycle.x, (Number(p.Y) - orgY) - pendingCycle.y) <= 1e-4;
    if (!sameXY || (has(p.Z) && (Number(p.Z) - orgZ) > pendingCycle.r + 1e-4)) {
      owedFloor();
      if (failed) return;
    }
  }
  posKnown = true;
  if (xyUnknown) {
    xyFirst(p.X, p.Y, p.Z);
    return;
  }
  motion("G00", xyz(p));
  if (needBoreRestart !== null && has(p.Z) && (Number(p.Z) - orgZ) >= needBoreRestart.r - 1e-4) {
    needBoreRestart = null;
    spinStart();
  }
}

function onLinear() {
  var p = properties;
  if (!startSectionBody()) return;
  if (!tiltOk(p)) return;
  if (needBoreRestart !== null) {
    fail("cannot feed move while spindle stopped waiting for bore rapid retract");
    return;
  }
  if (!posKnown) {
    fail("missing start position before cut; lead section with rapid positioning");
    return;
  }
  var f = ensureFeed(p.FeedRate);
  if (f === null) return;
  if (xyUnknown && has(p.StartX) && has(p.StartY)) {
    xyFirst(p.StartX, p.StartY, p.StartZ);
  } else if (xyUnknown) {
    xyUnknown = false;
  }
  feedMotion("G1", xyz(p), f);
  if (pendingCycle !== null && has(p.Z) && Math.abs((Number(p.Z) - orgZ) - pendingCycle.z) <= 1e-4) {
    var pc = pendingCycle;
    pendingCycle = null;
    if (pc.ms > 0) block(["G04", "P" + ni(pc.ms)]);
    if (pc.bore) {
      block(["M05"]);
      spindleOn = false;
      needBoreRestart = { r: pc.r };
    }
  }
}

function planeSpec(circularPlane) {
  if (circularPlane === "XZ") return { code: 18, u: "Z", v: "X", w: "Y", cu: "K", cv: "I" };
  if (circularPlane === "YZ") return { code: 19, u: "Y", v: "Z", w: "X", cu: "J", cv: "K" };
  return { code: 17, u: "X", v: "Y", w: "Z", cu: "I", cv: "J" };
}

function axisVal(p, letter, off) { return Number(p[letter]) - off; }
function orgOf(letter) { return letter === "X" ? orgX : (letter === "Y" ? orgY : orgZ); }

function onCircular() {
  owedFloor();
  var p = properties;
  if (!startSectionBody()) return;
  if (!tiltOk(p)) return;
  if (needBoreRestart !== null) {
    fail("cannot arc move while spindle stopped waiting for bore rapid retract");
    return;
  }
  if (!posKnown) {
    fail("missing start position before arc; lead section with rapid positioning");
    return;
  }
  // #3442: an arc whose written ends coincide is no arc block (arcFate,
  // PRELUDE): the A11 would read it as a full circle. The refusals above
  // stay first. The plane word and the feed are not touched for "none".
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  var f = ensureFeed(p.FeedRate);
  if (f === null) return;
  if (xyUnknown && has(p.StartX) && has(p.StartY)) {
    xyFirst(p.StartX, p.StartY, p.StartZ);
  } else if (xyUnknown) {
    xyUnknown = false;
  }

  var sp = planeSpec(p.CircularPlane);
  var pw = "";
  if (sp.code !== plane) {
    plane = sp.code;
    pw = "G" + plane;
  }

  var c = p.IJKIncrement || {};
  var oU = orgOf(sp.u), oV = orgOf(sp.v), oW = orgOf(sp.w);
  var u0 = axisVal(p, "Start" + sp.u, oU), v0 = axisVal(p, "Start" + sp.v, oV), w0 = axisVal(p, "Start" + sp.w, oW);
  var u1 = axisVal(p, sp.u, oU), v1 = axisVal(p, sp.v, oV), w1 = axisVal(p, sp.w, oW);
  if (!has(c[sp.cu]) || !has(c[sp.cv])) {
    fail("arc center words missing for G" + sp.code);
    return;
  }
  var cU = Number(c[sp.cu]) - oU, cV = Number(c[sp.cv]) - oV;
  var du0 = u0 - cU, dv0 = v0 - cV;
  var r = Math.hypot(du0, dv0);
  if (!isFinite(r) || r <= 1e-6) {
    fail("zero-radius arc is invalid");
    return;
  }

  var cw = p.CircularDirection === "CW";
  var a0 = Math.atan2(dv0, du0), a1 = Math.atan2(v1 - cV, u1 - cU);
  var isFull = Math.hypot(u1 - u0, v1 - v0) <= 1e-6;
  var sweep;
  if (isFull) {
    sweep = cw ? -2 * Math.PI : 2 * Math.PI;
  } else {
    sweep = a1 - a0;
    if (cw) { while (sweep > 1e-7) sweep -= 2 * Math.PI; }
    else { while (sweep < -1e-7) sweep += 2 * Math.PI; }
  }
  var steps = Math.abs(sweep) > Math.PI + 1e-5 ? Math.ceil(Math.abs(sweep) / Math.PI) : 1;
  var g = cw ? "G2" : "G3";

  for (var k = 1; k <= steps; k++) {
    var pu = k === 1 ? u0 : cU + r * Math.cos(a0 + sweep * ((k - 1) / steps));
    var pv = k === 1 ? v0 : cV + r * Math.sin(a0 + sweep * ((k - 1) / steps));
    var nu = k === steps ? u1 : cU + r * Math.cos(a0 + sweep * (k / steps));
    var nv = k === steps ? v1 : cV + r * Math.sin(a0 + sweep * (k / steps));
    var nw = k === steps ? w1 : w0 + (w1 - w0) * (k / steps);
    var coords = {};
    coords[sp.u] = nu;
    coords[sp.v] = nv;
    coords[sp.w] = nw;
    // An arc block states its whole end point: X, Y and Z.
    var ends = ["X" + fu(coords.X), "Y" + fu(coords.Y), "Z" + fu(coords.Z)];
    lastWord = { X: ends[0], Y: ends[1], Z: ends[2] };
    var fw = modalFeed(f);
    block([k === 1 ? pw : "", g].concat(ends, [sp.cu + fu(cU - pu), sp.cv + fu(cV - pv), fw]));
    wroteFeed(fw);
  }
}

function onCycle() {
  var p = properties;
  owedFloor();
  if (!startSectionBody()) return;
  if (needBoreRestart !== null) {
    fail("cannot start drilling cycle while spindle stopped waiting for bore rapid retract");
    return;
  }
  var kind = String(p.CycleType || "").toUpperCase();
  if (kind === "TAP" || kind === "FLOATINGTAP") {
    fail("tapping cycle (G84 / floating tap) is unsupported by RichAuto DSP A11; use thread milling");
    return;
  }
  if (!has(p.FeedRate) || Number(p.FeedRate) <= 0) {
    fail("drilling cycle requires a positive feed rate");
    return;
  }
  var dwellMs = has(p.P) ? Math.ceil(Number(p.P) * 1000) : 0;
  if (!isFinite(dwellMs) || dwellMs < 0 || dwellMs > 2147483647) {
    fail("drilling dwell must be in 0..2147483647 ms, got " + String(dwellMs));
    return;
  }
  var bore = kind === "BORE";
  if (dwellMs > 0 || bore) {
    pendingCycle = {
      x: Number(p.X) - orgX,
      y: Number(p.Y) - orgY,
      z: Number(p.Z) - orgZ,
      r: Number(p.R) - orgZ,
      ms: dwellMs,
      bore: bore
    };
  }
}

function noteEndUnits() {
  if (endUnitsNoted) return;
  if (typeof postOptions !== "undefined" && postOptions && postOptions.EndUnits === "inch") {
    note("END UNITS NOT WRITTEN - RICHAUTO A11 IS MM ONLY");
    endUnitsNoted = true;
  }
}

function onClose() {
  endSectionBody();
  noteEndUnits();
  block(["M30"]);
  // #1907: the A11 program ends with a % line.
  writeln("%");
}

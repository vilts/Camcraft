// Clean-room Mach4 Mill postprocessor (3-axis G94 and 3+2 indexed AC trunnion in G94).
// Provenance: public Mach4 Mill GCode Programming & RS-274/NGC circular interpolation,
// canned cycles, G31 probing, and Custom Macro B conventions; Camcraft #1962 XYZAC
// trunnion schema & Rodrigues kinematics. No third-party post consulted.

description = "Mach4";
vendor = "Camcraft";
extension = "nc";
capabilities = 1;
programUnits = "mm inch";
explains = "paren";
coolantModes = true;
afterModes = true;
workOffsetIndex = true;
workOffsetMax = 254;
millSpindleReverse = true;

declaredOptions = {
  safeRetract: {
    type: "enum", title: "Safe retract method", group: "Behavior",
    description: "Z retract before tool changes, indexing and program end",
    choices: [{ id: "G28", title: "G28 reference return" }, { id: "G30", title: "G30 reference return" }, { id: "G53", title: "G53 machine Z" }],
    default: "G28"
  },
  rolloverMode: {
    type: "enum", title: "C rotary rollover", group: "Rotary",
    description: "Use the machine rollover setting or explicitly select wrapped or unwrapped C angles",
    choices: [{ id: "auto", title: "Machine setting" }, { id: "rollover_360", title: "Wrap to 0..360 degrees" }, { id: "unwrapped", title: "Unwrapped angles" }],
    default: "auto"
  },
  rotaryUnclamp: { type: "string", title: "Rotary unclamp code", group: "Rotary", description: "Machine unclamp block; set together with rotaryClamp", default: "" },
  rotaryClamp: { type: "string", title: "Rotary clamp code", group: "Rotary", description: "Machine clamp block; set together with rotaryUnclamp", default: "" },
  programNumber: { type: "integer", title: "Program number", group: "Header", description: "Fallback O number when the host supplies no program number", min: 1, default: 1001 }
};

var COOLANT_KINDS = ["flood", "mist", "air", "air-blast", "through-spindle", "through-tool"];
var inch = false;
var safeRetract = "G28";
var rolloverMode = "auto";
var clampCode = "", unclampCode = "";
var progNumber = 1001;

var tNo = 1, toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
var sRpm = 0, spinRev = false;
var orgX = 0, orgY = 0, orgZ = 0;
var wcsWord = "G54", wcsIndex = 1;
var setupName = "", setupSaid = "", opName = "", opKind = "";
var coolantAsk = "flood", coolantOp = "", coolant = "", afterAsk = "", setupNext = "";

var toolWait = false, spindleWait = false, spindleOn = false, isFirstTool = true;
var sectionOpen = false, kindProbe = false, probeSection = false;
var clearZ = null, hopZ = null, xyUnknown = true, traverseRefused = false, retractRefused = false;
var justRetracted = false, homedAt = -1;

var planeG = 17;
var inCycle = false, cycleSig = "", seatKnown = false, cycX = 0, cycY = 0, cycR = 0;
var smoothTol = 0, smoothMode = "";
var compG = "", compD = "", compOn = false, wearNoted = false;
var probeBase = 100;

var trunnionCfg = null, trunnionFailed = false, hasTilted = false;
var modalA = null, modalC = null;
var secStarted = false, secPose = null, secSimRefused = false, secHasCut = false;
var rotCur = { x: 0, y: 0, z: 0 };

function has(v) { return typeof v !== "undefined" && v !== null; }
function num(v, d) { return has(v) && isFinite(Number(v)) ? Number(v) : d; }
function who() { return opName ? "operation " + opName + ": " : (coolantOp ? "operation " + coolantOp + ": " : ""); }
function msg(s) { if (typeof message === "function") message(s); else writeln("(MSG: " + s + ")"); }
function clean(s) { return String(s || "").replace(/[()]/g, " ").replace(/\s+/g, " ").trim(); }
function note(s) { if (s) block(["(" + clean(s) + ")"]); }
function block(ws) {
  var xs = [];
  for (var i = 0; i < ws.length; i++) if (ws[i]) xs.push(ws[i]);
  if (xs.length) writeBlock(xs.join(" "));
}
function ni(n) { return String(Math.round(n)); }
function progComment() {
  return typeof program !== "undefined" && program && program.comment ? clean(program.comment) : "";
}
function oWord() {
  var n = typeof program !== "undefined" && program && program.number > 0 ? Math.round(program.number) : progNumber;
  var s = String(Math.max(1, n));
  while (s.length < 4) s = "0" + s;
  return "O" + s;
}
function trimDec(val, decimals, keepDot) {
  var n = Number(val);
  if (!isFinite(n)) n = 0;
  var s = n.toFixed(decimals);
  if (Number(s) === 0) s = (0).toFixed(decimals);
  if (s.indexOf(".") >= 0) s = s.replace(/(\.\d*?[1-9])0+$/, "$1").replace(/\.0+$/, ".");
  if (keepDot) return s.indexOf(".") >= 0 ? s : s + ".";
  return s.replace(/\.$/, "");
}
function fu(mm) { return trimDec(inch ? mm / 25.4 : mm, inch ? 4 : 3, true); }
function fa(deg) { return trimDec(deg, 3, true); }
function ff(mmMin) { return "F" + trimDec(inch ? mmMin / 25.4 : mmMin, inch ? 2 : 1, false); }
function fn(mm) { return trimDec(inch ? mm / 25.4 : mm, inch ? 4 : 3, false); }
function eps() { return inch ? "0.0005" : "0.005"; }
function minusU(expr, mm) {
  var v = Number(fn(mm));
  if (v === 0) return expr;
  return v > 0 ? "[" + expr + "-" + trimDec(v, inch ? 4 : 3, false) + "]" : "[" + expr + "+" + trimDec(-v, inch ? 4 : 3, false) + "]";
}

function vDot(a, b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
function vAdd(a, b) { return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]; }
function vSub(a, b) { return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]; }
function vScale(a, s) { return [a[0] * s, a[1] * s, a[2] * s]; }
function vCross(a, b) {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}
function vNorm(a) { return Math.sqrt(vDot(a, a)); }
function rodrigues(k, angRad, v) {
  var s = Math.sin(angRad), c = Math.cos(angRad), kv = vDot(k, v), kxv = vCross(k, v), omc = 1 - c;
  return [
    v[0] * c + kxv[0] * s + k[0] * kv * omc,
    v[1] * c + kxv[1] * s + k[1] * kv * omc,
    v[2] * c + kxv[2] * s + k[2] * kv * omc,
  ];
}
function isVec3(v) {
  return Array.isArray(v) && v.length === 3 && isFinite(Number(v[0])) && isFinite(Number(v[1])) && isFinite(Number(v[2]));
}
function normAngleRad(r) {
  var twoPi = 2 * Math.PI;
  r = r % twoPi;
  if (r > Math.PI) r -= twoPi;
  if (r < -Math.PI) r += twoPi;
  return r;
}
function mod360(d) {
  var r = d % 360;
  if (r < 0) r += 360;
  if (r >= 359.9995 || r < 0.0005) r = 0;
  return r;
}
function axisFromStreamABC(aDeg, bDeg, cDeg) {
  var d2r = Math.PI / 180;
  var sa = Math.sin(aDeg * d2r), ca = Math.cos(aDeg * d2r);
  var sb = Math.sin(bDeg * d2r), cb = Math.cos(bDeg * d2r);
  var sc = Math.sin(cDeg * d2r), cc = Math.cos(cDeg * d2r);
  return [ca * sb * cc + sa * sc, ca * sb * sc - sa * cc, ca * cb];
}
function isVerticalAxis(u) {
  return Math.abs(u[0]) < 1e-6 && Math.abs(u[1]) < 1e-6 && u[2] > 1 - 1e-6;
}

function resolveTrunnion() {
  if (trunnionCfg) return trunnionCfg;
  if (trunnionFailed) return null;
  function fail(reason) {
    trunnionFailed = true;
    msg(who() + reason);
    return null;
  }
  var m = typeof machine !== "undefined" ? machine : null;
  if (!m) return fail("tilted move: no machine loaded (Mach4 3+2 requires an AC trunnion machine with controller.rtcp = false, A and C rotaries, A.pivot, and C.offset)");
  var mn = m.name || m.model || "machine";
  if (m.rtcp === true) {
    return fail("tilted move: " + mn + " states controller.rtcp = true, but Mach4 lacks native G43.4 RTCP (set controller.rtcp = false for 3+2 G94 posting)");
  }
  var rots = Array.isArray(m.rotaries) ? m.rotaries : [];
  var rotA = null, rotC = null, idxA = -1, idxC = -1;
  for (var i = 0; i < rots.length; i++) {
    if (rots[i].name === "A") { rotA = rots[i]; idxA = i; }
    else if (rots[i].name === "C") { rotC = rots[i]; idxC = i; }
  }
  if (!rotA || !rotC) {
    return fail("missing trunnion geometry on " + mn + ": requires both A and C rotaries for 3+2 positioning");
  }
  if (rotA.parent === "C" || rotC.parent !== "A" || idxA > idxC) {
    return fail("invalid trunnion parent hierarchy on " + mn + ": C.parent must be A (got " + (rotC.parent || "none") + ")");
  }
  if (rotA.turnsPart === false || rotC.turnsPart === false) {
    return fail("head-mounted rotary (turnsPart = false) on " + mn + " is not an AC table-table trunnion");
  }
  if (!isVec3(rotA.direction) || !isVec3(rotC.direction)) {
    return fail("missing rotary axis direction vector on " + mn);
  }
  var aN = vNorm(rotA.direction), cN = vNorm(rotC.direction);
  if (aN < 1e-6 || cN < 1e-6) return fail("zero rotary axis direction vector on " + mn);
  var kA = vScale(rotA.direction, 1 / aN), kC = vScale(rotC.direction, 1 / cN);
  var sAxis = [0, 0, 1];
  if (vNorm(vCross(kA, kC)) < 1e-6) return fail("rotary axes A and C on " + mn + " are parallel");
  if (vNorm(vCross(kA, sAxis)) < 1e-6) return fail("primary rotary axis A on " + mn + " is parallel to spindle Z");
  if (!isVec3(rotA.pivot)) return fail("missing trunnion A.pivot on " + mn + " (required for 3+2 coordinate transform)");
  if (!isVec3(rotC.offset)) return fail("missing trunnion C.offset on " + mn + " (required for 3+2 coordinate transform)");
  function parseTravel(r, label) {
    if (!has(r.travel)) return null;
    if (!Array.isArray(r.travel) || r.travel.length !== 2 || !isFinite(Number(r.travel[0])) || !isFinite(Number(r.travel[1])) || Number(r.travel[0]) > Number(r.travel[1])) {
      return "invalid";
    }
    return [Number(r.travel[0]), Number(r.travel[1])];
  }
  var aTrav = parseTravel(rotA, "A"), cTrav = parseTravel(rotC, "C");
  if (aTrav === "invalid" || cTrav === "invalid") return fail("invalid rotary travel bounds on " + mn);
  var aConv = rotA.signConvention || m.signConvention || "tool_rh";
  var cConv = rotC.signConvention || m.signConvention || "tool_rh";
  trunnionCfg = {
    name: mn,
    kA: kA,
    kC: kC,
    s: sAxis,
    aPivot: [Number(rotA.pivot[0]), Number(rotA.pivot[1]), Number(rotA.pivot[2])],
    cOffset: [Number(rotC.offset[0]), Number(rotC.offset[1]), Number(rotC.offset[2])],
    aTravel: aTrav,
    cTravel: cTrav,
    aZero: has(rotA.zeroOffset) && isFinite(Number(rotA.zeroOffset)) ? Number(rotA.zeroOffset) : 0,
    cZero: has(rotC.zeroOffset) && isFinite(Number(rotC.zeroOffset)) ? Number(rotC.zeroOffset) : 0,
    aSign: aConv === "table_rh" ? 1 : -1,
    cSign: cConv === "table_rh" ? 1 : -1,
    cRollover: rolloverMode === "rollover_360" ? true : (rolloverMode === "unwrapped" ? false : Boolean(rotC.rollover))
  };
  return trunnionCfg;
}

function inBounds(val, bounds) {
  if (!bounds) return true;
  return val >= bounds[0] - 1e-6 && val <= bounds[1] + 1e-6;
}

function solveAcPose(cfg, u, aRef, cRef) {
  var c0 = vDot(cfg.kC, cfg.kA) * vDot(cfg.kA, cfg.s);
  var c1 = vDot(cfg.kC, cfg.s) - c0;
  var c2 = -vDot(cfg.kC, vCross(cfg.kA, cfg.s));
  var rhs = vDot(cfg.kC, u) - c0;
  var amp = Math.sqrt(c1 * c1 + c2 * c2);
  if (amp < 1e-12) return null;
  var cosVal = rhs / amp;
  if (cosVal > 1 + 1e-5 || cosVal < -1 - 1e-5) return null;
  if (cosVal > 1) cosVal = 1;
  if (cosVal < -1) cosVal = -1;
  var delta = Math.atan2(c2, c1), alpha = Math.acos(cosVal);
  var phiAs = alpha > 1e-7 ? [normAngleRad(delta + alpha), normAngleRad(delta - alpha)] : [normAngleRad(delta)];
  var r2d = 180 / Math.PI, d2r = Math.PI / 180;
  var best = null, bestScore = Infinity;

  for (var i = 0; i < phiAs.length; i++) {
    var phiA = phiAs[i];
    var w = rodrigues(cfg.kA, -phiA, cfg.s);
    var uPerp = vSub(u, vScale(cfg.kC, vDot(cfg.kC, u)));
    var phiC;
    if (vNorm(uPerp) <= 1e-6) {
      phiC = normAngleRad(cfg.cSign * (cRef - cfg.cZero) * d2r);
    } else {
      var wPerp = vSub(w, vScale(cfg.kC, vDot(cfg.kC, w)));
      phiC = Math.atan2(vDot(wPerp, vCross(cfg.kC, uPerp)), vDot(wPerp, uPerp));
    }
    var aBase = cfg.aZero + cfg.aSign * (phiA * r2d);
    var cBase = cfg.cZero + cfg.cSign * (phiC * r2d);
    for (var ka = -2; ka <= 2; ka++) {
      var aCand = aBase + 360 * ka;
      if (!inBounds(aCand, cfg.aTravel)) continue;
      if (cfg.cRollover) {
        var cWrap = mod360(cBase);
        if (!inBounds(cWrap, cfg.cTravel)) continue;
        var sc = Math.abs(aCand - aRef) + Math.abs(cWrap - cRef);
        if (sc < bestScore) {
          bestScore = sc;
          best = { aCmd: aCand, cCmd: cWrap, phiA: cfg.aSign * (aCand - cfg.aZero) * d2r, phiC: cfg.cSign * (cWrap - cfg.cZero) * d2r };
        }
      } else {
        for (var kc = -3; kc <= 3; kc++) {
          var cCand = cBase + 360 * kc;
          if (!inBounds(cCand, cfg.cTravel)) continue;
          var sc2 = Math.abs(aCand - aRef) + Math.abs(cCand - cRef);
          if (sc2 < bestScore) {
            bestScore = sc2;
            best = { aCmd: aCand, cCmd: cCand, phiA: cfg.aSign * (aCand - cfg.aZero) * d2r, phiC: cfg.cSign * (cCand - cfg.cZero) * d2r };
          }
        }
      }
    }
  }
  return best;
}

function transformPt(pose, cfg, wx, wy, wz) {
  var tip = [wx - orgX, wy - orgY, wz - orgZ];
  if (!pose || !pose.tilted) return tip;
  var pC = rodrigues(cfg.cUnit || cfg.kC, pose.phiC, tip);
  var pA = rodrigues(cfg.kA, pose.phiA, vAdd(cfg.cOffset, pC));
  return vSub(pA, cfg.cOffset);
}

function checkSafeRetractZ() {
  if (safeRetract !== "G53") return 0;
  var m = typeof machine !== "undefined" ? machine : null;
  if (m && !has(m.safeRetractZ)) {
    if (!retractRefused) {
      retractRefused = true;
      var mn = m.name || m.model || "machine";
      msg(who() + mn + " does not state safeRetractZ (needed for G53 safe retract)");
    }
    return null;
  }
  return m && has(m.safeRetractZ) ? Number(m.safeRetractZ) : 0;
}

function safeRetractBlock() {
  if (safeRetract === "G53") {
    var sz = checkSafeRetractZ();
    if (sz === null) return false;
    block(["G53", "G00", "Z" + fu(sz)]);
  } else if (safeRetract === "G30") {
    block(["G91", "G30", "Z0."]);
    block(["G90"]);
  } else {
    block(["G91", "G28", "Z0."]);
    block(["G90"]);
  }
  justRetracted = true;
  return true;
}

function onOpen() {
  var o = typeof postOptions !== "undefined" && postOptions ? postOptions : {};
  inch = o.Units === "inch";
  if (has(o.safeRetract)) {
    safeRetract = String(o.safeRetract).toUpperCase();
    if (safeRetract !== "G28" && safeRetract !== "G53" && safeRetract !== "G30") {
      msg("unknown safeRetract option " + o.safeRetract + " - expected G28, G53, or G30");
      safeRetract = "G28";
    }
  }
  if (has(o.rolloverMode)) {
    rolloverMode = String(o.rolloverMode);
    if (rolloverMode !== "auto" && rolloverMode !== "rollover_360" && rolloverMode !== "unwrapped") {
      msg("unknown rolloverMode option " + o.rolloverMode + " - expected auto, rollover_360, or unwrapped");
      rolloverMode = "auto";
    }
  }
  clampCode = has(o.rotaryClamp) ? clean(o.rotaryClamp) : "";
  unclampCode = has(o.rotaryUnclamp) ? clean(o.rotaryUnclamp) : "";
  if ((clampCode && !unclampCode) || (!clampCode && unclampCode)) {
    msg("both rotaryClamp and rotaryUnclamp must be set together (or both left unset)");
  }
  if (has(o.programNumber) && Number(o.programNumber) > 0) progNumber = Math.round(Number(o.programNumber));

  var nm = (typeof machine !== "undefined" && machine && (machine.name || machine.model)) || "CAMCRAFT";
  writeln("%");
  block([oWord(), "(" + (progComment() || clean(nm) || "CAMCRAFT") + ")"]);
  var inv = (typeof machine !== "undefined" && machine && machine.invertedAxes) || [];
  var dirDesc = { X: "LEFT", Y: "TOWARD THE FRONT", Z: "DOWN" };
  for (var i = 0; i < inv.length; i++) {
    var ax = String(inv[i]);
    block(["(MACHINE " + ax + "+ MOVES " + (dirDesc[ax] || "THE OTHER WAY") + " - INVERTED AXIS)"]);
  }
  block([inch ? "G20" : "G21"]);
  block(["G90", "G94", "G91.1", "G40", "G49", "G17", "G64"]);
  checkSafeRetractZ();
}

function workOffsetName(n) {
  if (n !== Math.floor(n) || n < 1 || n > workOffsetMax) return "";
  if (n <= 6) return "G" + (53 + n);
  return "G54.1 P" + (n - 6);
}
function offsetWord(value) {
  var n = Number(value), w = workOffsetName(n);
  if (!w) { msg("work offset " + value + " - Mach4 supports offsets 1-" + workOffsetMax + " only (G54..G59, G54.1 P1..P248)"); return wcsWord; }
  wcsIndex = n;
  return w;
}

function onParameter(name, value) {
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;
  if (name === "ClearancePlane") clearZ = has(value) ? Number(value) : null;
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingMode") { smoothMode = String(value || ""); return; }
  if (name === "SetupName") setupName = clean(value);
  else if (name === "OperationName") { opName = clean(value); coolantOp = String(value || ""); spinRev = false; }
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "ToolKind") { toolKind = clean(value).toUpperCase(); kindProbe = toolKind === "TOUCH PROBE"; }
  else if (name === "ToolDiameter") toolDia = Number(value) || 0;
  else if (name === "ToolCornerRadius") toolCr = Number(value) || 0;
  else if (name === "MillSpindle") spinRev = String(value || "") === "reverse";
  else if (name === "Coolant") coolantAsk = String(value || "flood");
  else if (name === "AfterOperation") afterAsk = String(value || "");
  else if (name === "SetupChange") setupNext = clean(value);
  else if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg(who() + "spindle rpm " + sRpm + " exceeds machine maximum " + machine.spindleMax);
  }
}

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (compOn) msg(who() + "tool change before cutter compensation cancel (G40)");
}

function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine && machine.coolants) || [];
  var other = "", floodNamed = false;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var nm = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (nm === "flood") { if (code) return code; floodNamed = true; continue; }
    if (code && !other && COOLANT_KINDS.indexOf(nm) < 0) other = code;
  }
  if (other || floodNamed) return other;
  if (cs.length > 0) msg(who() + "the machine states no flood coolant code");
  return "";
}

function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "air" ? ["air", "air-blast"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []));
  if (want.length === 0) { msg(who() + "unknown coolant ask " + ask); return ""; }
  var cs = (typeof machine !== "undefined" && machine && machine.coolants) || [];
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var nm = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (code && want.indexOf(nm) >= 0) return code;
  }
  msg(who() + "the machine states no " + ask + " coolant code");
  return "";
}

function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  var probeStop = (spindleWait && !(sRpm > 0)) || !(sRpm > 0);
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + opName + (opKind ? " - " + opKind : "") + " - " + wcsWord);
  opName = ""; opKind = "";
  if (toolWait) {
    if (spindleOn) { block(["M5"]); spindleOn = false; }
    if (coolant) { block(["M9"]); coolant = ""; }
    if (!isFirstTool) safeRetractBlock();
    isFirstTool = false;
    var desc = [];
    if (toolKind) desc.push(toolKind);
    if (toolDia > 0) desc.push("D" + fn(toolDia));
    if (toolCr > 0) desc.push("R" + fn(toolCr));
    note("T" + ni(tNo) + (toolName ? " " + toolName : "") + (desc.length ? " - " + desc.join(" ") : ""));
    toolKind = ""; toolDia = 0; toolCr = 0;
    block(["T" + ni(tNo), "M6"]);
    block(["G43", "H" + ni(tNo)]);
    xyUnknown = true;
    toolWait = false;
    if (sRpm > 0) spindleWait = true;
  }
  block(["G90", wcsWord]);
  if (spindleWait && sRpm > 0) {
    block(["S" + ni(sRpm), spinRev ? "M4" : "M3"]);
    spindleOn = true;
    spindleWait = false;
  }
  if (spindleWait && !(sRpm > 0)) {
    block(["M5"]);
    spindleOn = false;
    spindleWait = false;
  }
  probeSection = probeStop && kindProbe;
  var nextCoolant = probeStop ? "" : coolantFor(coolantAsk);
  if (nextCoolant !== coolant) {
    if (coolant) block(["M9"]);
    coolant = nextCoolant;
    if (coolant) block([coolant]);
  }
}

function restorePlane() {
  if (planeG !== 17) { block(["G17"]); planeG = 17; }
}
function endCycle() {
  if (inCycle) { block(["G80"]); inCycle = false; cycleSig = ""; }
}
function onHole(p) {
  return Math.abs(Number(p.X) - cycX) < 1e-6 && Math.abs(Number(p.Y) - cycY) < 1e-6 && Number(p.Z) <= cycR + 1e-6;
}
function chainRapid() {
  seatKnown = true;
  if (!inCycle) return false;
  if (onHole(properties)) return true;
  endCycle();
  return false;
}
function chainCut() {
  seatKnown = true;
  if (!inCycle) return false;
  if (!onHole(properties)) msg(who() + "cutting move leaves hole inside canned-cycle group");
  return true;
}
function probeSeat() {
  seatKnown = true;
  endCycle();
}
function onSection() {
  seatKnown = false;
  secStarted = false;
  secPose = null;
  secSimRefused = false;
  secHasCut = false;
}
function onSectionEnd() {
  endCycle();
  restorePlane();
  if (compOn || compG) {
    msg(who() + "section ends with cutter compensation active (missing G40)");
    compG = ""; compD = ""; compOn = false;
  }
  wearNoted = false;
  smoothTol = 0; smoothMode = "";
  sectionOpen = false;
  seatKnown = false;
  hopZ = null; clearZ = null;
  coolantAsk = "flood";
  kindProbe = false;
  probeSection = false;
  if (afterAsk) {
    if (coolant) { block(["M9"]); coolant = ""; }
    if (spindleOn) { block(["M5"]); spindleOn = false; if (sRpm > 0) spindleWait = true; }
    if (setupNext && (afterAsk === "optional-stop" || afterAsk === "stop")) {
      safeRetractBlock();
      note("SETUP CHANGE - NEXT: " + setupNext);
    }
    if (afterAsk === "optional-stop") block(["M1"]);
    else if (afterAsk === "stop") block(["M0"]);
    else if (afterAsk === "home") { safeRetractBlock(); homedAt = linesWritten(); }
    afterAsk = ""; setupNext = "";
  }
}

function spindleStopped(what) {
  if (spindleOn) return false;
  msg(who() + what + " with the spindle stopped - refused");
  return true;
}

function preparePose(p, isCutting) {
  var hasABC = has(p.A) || has(p.B) || has(p.C);
  if (!hasABC && secStarted && secPose) {
    if (isCutting) secHasCut = true;
    return secPose;
  }
  var u = hasABC ? axisFromStreamABC(num(p.A, 0), num(p.B, 0), num(p.C, 0)) : [0, 0, 1];
  var tilted = hasABC && (!isVerticalAxis(u) || Math.abs(num(p.C, 0)) > 1e-6);

  if (tilted || hasTilted) {
    if (!resolveTrunnion()) return null;
  }

  if (secStarted && secPose) {
    var diff = Math.abs(vDot(u, secPose.partU) - 1) > 1e-6 || tilted !== secPose.tilted;
    if (diff && (isCutting || secHasCut)) {
      if (!secSimRefused) {
        secSimRefused = true;
        msg(who() + "simultaneous 4/5-axis move (needs #1966: inverse-time feed) - Mach4 3+2 requires constant orientation within an indexed section");
      }
      return null;
    }
    if (!diff) {
      if (isCutting) secHasCut = true;
      return secPose;
    }
  }

  secStarted = true;
  if (isCutting) secHasCut = true;
  // A repeated program can start with the table still indexed by its last run.
  var rotaries = typeof machine !== "undefined" && machine && Array.isArray(machine.rotaries) ? machine.rotaries : [];
  var configuredA = false, configuredC = false;
  for (var ri = 0; ri < rotaries.length; ri++) {
    if (rotaries[ri].name === "A") configuredA = true;
    if (rotaries[ri].name === "C") configuredC = true;
  }
  if (!tilted && !hasTilted && !(configuredA && configuredC)) {
    secPose = { tilted: false, partU: [0, 0, 1], phiA: 0, phiC: 0, aCmd: 0, cCmd: 0 };
    return secPose;
  }
  var cfg = resolveTrunnion();
  if (!cfg) return null;
  hasTilted = true;
  var aRef = modalA !== null ? modalA : cfg.aZero;
  var cRef = modalC !== null ? modalC : cfg.cZero;
  // Vertical coordinates are unrotated, so restore both kinematic zeros.
  // Retaining C at the pole would rotate XY even though the tool axis is vertical.
  var sol = tilted ? solveAcPose(cfg, u, aRef, cRef) : {
    aCmd: cfg.aZero, cCmd: cfg.cRollover ? mod360(cfg.cZero) : cfg.cZero,
    phiA: 0, phiC: 0
  };
  if (sol && (!inBounds(sol.aCmd, cfg.aTravel) || !inBounds(sol.cCmd, cfg.cTravel))) sol = null;
  if (!sol) {
    msg(who() + "orientation is unreachable within trunnion travel limits on " + cfg.name);
    return null;
  }
  var needIndex = modalA === null || Math.abs(sol.aCmd - modalA) > 1e-5 || Math.abs(sol.cCmd - modalC) > 1e-5;
  if (needIndex) {
    if (!justRetracted && !safeRetractBlock()) return null;
    if (unclampCode) block([unclampCode]);
    block(["G00", "A" + fa(sol.aCmd), "C" + fa(sol.cCmd)]);
    if (clampCode) block([clampCode]);
    modalA = sol.aCmd;
    modalC = sol.cCmd;
    xyUnknown = true;
    justRetracted = false;
  }
  secPose = { tilted: tilted, partU: u, phiA: sol.phiA, phiC: sol.phiC, aCmd: sol.aCmd, cCmd: sol.cCmd };
  return secPose;
}

function xyFirst(x, y, z, descend, pose, cfg) {
  if (!xyUnknown || !has(x) || !has(y)) return true;
  var c = has(clearZ) ? clearZ : (has(hopZ) ? hopZ : null);
  if (c === null) {
    if (!traverseRefused) {
      traverseRefused = true;
      msg(who() + "first X/Y traverse with no ClearancePlane or RetractPlane stated");
    }
    return false;
  }
  if (pose && pose.tilted) {
    var pt = transformPt(pose, cfg, Number(x), Number(y), has(z) ? Number(z) : c);
    block(["G00", "X" + fu(pt[0]), "Y" + fu(pt[1])]);
    if (descend && has(z)) block(["G00", "Z" + fu(pt[2])]);
  } else {
    var tz = has(z) ? Math.max(c, Number(z)) : c;
    block(["G00", "Z" + fu(tz - orgZ)]);
    if (descend || !has(z) || Math.abs(Number(z) - tz) > 1e-9) {
      block(["G00", "X" + fu(Number(x) - orgX), "Y" + fu(Number(y) - orgY)]);
    }
    if (descend && has(z) && Number(z) < tz - 1e-9) {
      block(["G00", "Z" + fu(Number(z) - orgZ)]);
    }
  }
  xyUnknown = false;
  justRetracted = false;
  return true;
}

function emitPtWords(pose, cfg, p) {
  var q = transformPt(pose, cfg, num(p.X, 0), num(p.Y, 0), num(p.Z, 0));
  rotCur.x = q[0]; rotCur.y = q[1]; rotCur.z = q[2];
  var ws = [];
  if (has(p.X) || (pose && pose.tilted)) ws.push("X" + fu(q[0]));
  if (has(p.Y) || (pose && pose.tilted)) ws.push("Y" + fu(q[1]));
  if (has(p.Z) || (pose && pose.tilted)) ws.push("Z" + fu(q[2]));
  return ws;
}

function onRadiusCompensation(side) {
  var s = String(side || properties.Compensation || "off");
  if (s === "off") { compG = compOn ? "G40" : ""; compD = ""; compOn = false; return; }
  if (inCycle) { msg(who() + "cutter compensation inside a canned cycle"); return; }
  compG = s === "left" ? "G41" : "G42";
  compD = "D" + ni(tNo);
  compOn = true;
  if (properties.CompensationType === "wear" && !wearNoted) {
    note("WEAR COMP - " + compD + " HOLDS WEAR DELTA ONLY");
    wearNoted = true;
  }
}

function onRapid() {
  startPreamble();
  if (compG) { msg(who() + "cutter compensation must engage or cancel on G01, not G00"); compG = ""; compD = ""; compOn = false; }
  var pose = preparePose(properties, false);
  if (!pose || chainRapid()) return;
  var cfg = trunnionCfg;
  if (!xyFirst(properties.X, properties.Y, properties.Z, false, pose, cfg)) return;
  block(["G00"].concat(emitPtWords(pose, cfg, properties)));
  seatKnown = true;
  justRetracted = false;
}

function onLinear() {
  startPreamble();
  var pose = preparePose(properties, true);
  if (!pose || chainCut()) return;
  if (spindleStopped("feed move G01")) return;
  var cfg = trunnionCfg;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true, pose, cfg)) return;
  var cg = compG, cd = compD; compG = ""; compD = "";
  var prefix = cg && planeG !== 17 ? ["G17", "G01", cg] : (cg ? ["G01", cg] : ["G01"]);
  if (cg) planeG = 17;
  var fwd = has(properties.FeedRate) ? ff(properties.FeedRate) : "";
  block(prefix.concat(emitPtWords(pose, cfg, properties), [cd, fwd]));
  seatKnown = true;
  justRetracted = false;
}

function onCircular() {
  startPreamble();
  if (compG) { msg(who() + "cutter compensation must engage or cancel on G01, not an arc"); compG = ""; compD = ""; compOn = false; }
  var pose = preparePose(properties, true);
  if (!pose || chainCut()) return;
  var cw = properties.CircularDirection === "CW";
  if (spindleStopped("arc move " + (cw ? "G02" : "G03"))) return;
  var p = properties, cfg = trunnionCfg;
  var planeName = p.CircularPlane || "XY";
  var nPart = planeName === "XZ" ? [0, 1, 0] : (planeName === "YZ" ? [1, 0, 0] : [0, 0, 1]);
  var nMach = nPart;
  if (pose.tilted) {
    nMach = rodrigues(cfg.kA, pose.phiA, rodrigues(cfg.kC, pose.phiC, nPart));
  }
  var machPlaneG = 0, flipDir = false;
  if (Math.abs(nMach[2]) > 1 - 1e-5) { machPlaneG = 17; flipDir = nMach[2] < 0; }
  else if (Math.abs(nMach[1]) > 1 - 1e-5) { machPlaneG = 18; flipDir = nMach[1] < 0; }
  else if (Math.abs(nMach[0]) > 1 - 1e-5) { machPlaneG = 19; flipDir = nMach[0] < 0; }
  else {
    msg(who() + "oblique arc in tilted 3+2 section cannot map cleanly to G17/G18/G19");
    return;
  }
  if (!xyFirst(p.StartX, p.StartY, p.StartZ, true, pose, cfg)) return;
  var sx = has(p.StartX) ? Number(p.StartX) : orgX, sy = has(p.StartY) ? Number(p.StartY) : orgY, sz = has(p.StartZ) ? Number(p.StartZ) : orgZ;
  var inc = p.IJKIncrement || {}, cx = has(inc.I) ? Number(inc.I) : sx, cy = has(inc.J) ? Number(inc.J) : sy, cz = has(inc.K) ? Number(inc.K) : sz;
  var qS = transformPt(pose, cfg, sx, sy, sz);
  var qC = transformPt(pose, cfg, cx, cy, cz);
  var qE = transformPt(pose, cfg, num(p.X, 0), num(p.Y, 0), num(p.Z, 0));
  var centerWs = [];
  if (machPlaneG === 17) centerWs = ["I" + fu(qC[0] - qS[0]), "J" + fu(qC[1] - qS[1])];
  else if (machPlaneG === 18) centerWs = ["I" + fu(qC[0] - qS[0]), "K" + fu(qC[2] - qS[2])];
  else centerWs = ["J" + fu(qC[1] - qS[1]), "K" + fu(qC[2] - qS[2])];
  var gPlane = machPlaneG !== planeG ? "G" + machPlaneG : "";
  planeG = machPlaneG;
  var gArc = (cw !== flipDir) ? "G02" : "G03";
  var fwd = has(p.FeedRate) ? ff(p.FeedRate) : "";
  block([gPlane, gArc, "X" + fu(qE[0]), "Y" + fu(qE[1]), "Z" + fu(qE[2])].concat(centerWs, [fwd]));
  seatKnown = true;
  justRetracted = false;
}

function cycleSeat(p, pose, cfg, qR) {
  if (!seatKnown) return true;
  var lo = has(p.StartZ) && Number(p.StartZ) < Number(p.R) - 1e-6;
  var away = !has(p.StartX) || !has(p.StartY) || Math.abs(p.StartX - p.X) >= 1e-6 || Math.abs(p.StartY - p.Y) >= 1e-6;
  var hi = !has(p.StartZ) || Number(p.StartZ) > Number(p.R) + 1e-6;
  if (!lo && !away && !hi) return true;
  endCycle();
  if (away) {
    if (!has(hopZ)) { msg(who() + "cycle away from hole without RetractPlane"); return false; }
    var top = Math.max(hopZ, Number(p.R));
    var at = has(p.StartZ) ? Number(p.StartZ) : -Infinity;
    if (at < top - 1e-6) {
      var qHop = transformPt(pose, cfg, Number(p.StartX || p.X), Number(p.StartY || p.Y), top);
      block(["G00", "Z" + fu(qHop[2])]);
      at = top;
    }
    block(["G00", "X" + fu(qR[0]), "Y" + fu(qR[1])]);
    if (at > Number(p.R) + 1e-6) block(["G00", "Z" + fu(qR[2])]);
    return true;
  }
  if (lo || hi) block(["G00", "Z" + fu(qR[2])]);
  return true;
}

function onCycle() {
  startPreamble();
  restorePlane();
  var p = properties;
  var secDwell = has(p.P) && Number(p.P) > 0 ? Number(p.P) : 0;
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = secDwell > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "TAP") {
    if (typeof machine !== "undefined" && machine && machine.rigidTapping === false) {
      msg(who() + "rigid tapping G84 refused: machine declares rigidTapping = false");
      return;
    }
    g = "G84";
  } else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
  else if (p.CycleType === "REAM") g = "G85";
  else { msg(who() + "canned cycle " + p.CycleType + " is not supported by Mach4 post"); return; }

  if (secDwell > 0 && g !== "G82" && g !== "G89") {
    msg(who() + "canned cycle " + g + " cannot write a dwell on Mach4");
    return;
  }
  if (!(has(p.FeedRate) && Number(p.FeedRate) > 0)) {
    msg(who() + "canned cycle " + g + " has no feed rate");
    return;
  }
  if (spindleStopped("canned cycle " + g)) return;
  var pose = preparePose(p, true);
  if (!pose) return;
  var cfg = trunnionCfg;
  if (!xyFirst(p.X, p.Y, p.R, false, pose, cfg)) return;
  var qBot = transformPt(pose, cfg, Number(p.X), Number(p.Y), Number(p.Z));
  var qR = transformPt(pose, cfg, Number(p.X), Number(p.Y), Number(p.R));
  if (!cycleSeat(p, pose, cfg, qR)) return;
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q + "|" + secDwell;
  if (inCycle && sig === cycleSig) {
    block(["X" + fu(qBot[0]), "Y" + fu(qBot[1])]);
  } else {
    endCycle();
    block(["G99"]);
    var ws = [g, "X" + fu(qBot[0]), "Y" + fu(qBot[1]), "Z" + fu(qBot[2]), "R" + fu(qR[2])];
    if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && Number(p.Q) > 0) ws.push("Q" + fu(Number(p.Q)));
    if (secDwell > 0 && (g === "G82" || g === "G89")) ws.push("P" + trimDec(secDwell, 3, false));
    ws.push(ff(Number(p.FeedRate)));
    block(ws);
    cycleSig = sig;
  }
  cycX = Number(p.X); cycY = Number(p.Y); cycR = Number(p.R);
  seatKnown = true; inCycle = true; justRetracted = false;
}

function prepareProbePose() {
  startPreamble();
  probeSeat();
  // Probe coordinates and G54 updates are expressed in the vertical part frame.
  // Share the retract/index guard, including its named refusal on unsafe changes.
  return preparePose({ A: 0, B: 0, C: 0 }, false) !== null;
}

// After a tool change or rotary reset the stream start is stale. Seat X/Y
// at clearance and let G31 protect the descent from that known retract state.
function onProbe() {
  if (!prepareProbePose()) return;
  var p = properties;
  if (!xyFirst(p.X, p.Y, null, false, null, null)) return;
  block(["G31", "X" + fu(num(p.X, orgX) - orgX), "Y" + fu(num(p.Y, orgY) - orgY), "Z" + fu(num(p.Z, orgZ) - orgZ), ff(num(p.FeedRate, num(p.Feed, 100)))]);
}
function onProbeMove() {
  if (!prepareProbePose()) return;
  var p = properties;
  if (!xyFirst(p.StartX, p.StartY, null, false, null, null)) return;
  block(["G31", "X" + fu(p.X - orgX), "Y" + fu(p.Y - orgY), "Z" + fu(p.Z - orgZ), ff(p.Feed)]);
  var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
  var ai = d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
  var tgt = [p.X - orgX, p.Y - orgY, p.Z - orgZ][ai];
  block(["IF[ABS[" + minusU("#" + (5061 + ai), tgt) + "]LT" + eps() + "]THEN#3000=2(PROBE NO CONTACT)"]);
  if (has(p.Measure) && p.Measure >= 0) {
    var b = probeBase + 3 * p.Measure;
    block(["#" + b + "=#5061"]);
    block(["#" + (b + 1) + "=#5062"]);
    block(["#" + (b + 2) + "=#5063"]);
  }
}
function onProbeRetreat() {
  if (!prepareProbePose()) return;
  var p = properties;
  block(["G91", "G01", "X" + fu(p.DeltaX), "Y" + fu(p.DeltaY), "Z" + fu(p.DeltaZ), ff(p.Feed)]);
  block(["G90"]);
}
function onProbeGuard() {
  if (!prepareProbePose()) return;
  var p = properties;
  if (!xyFirst(p.StartX, p.StartY, null, false, null, null)) return;
  block(["G31", "X" + fu(p.X - orgX), "Y" + fu(p.Y - orgY), "Z" + fu(p.Z - orgZ), ff(p.Feed)]);
  // The clearance approach can change the actual start from the stream start.
  // Validate every commanded coordinate, including a descent back to StartZ.
  var to = [p.X, p.Y, p.Z], org = [orgX, orgY, orgZ];
  for (var ai = 0; ai < 3; ai++) {
    block(["IF[ABS[" + minusU("#" + (5061 + ai), to[ai] - org[ai]) + "]GT" + eps() + "]THEN#3000=3(PROBE HIT ON POSITIONING)"]);
  }
}
function onProbeResult() {
  if (!prepareProbePose()) return;
  var p = properties, org = [orgX, orgY, orgZ], axes = p.Axes || [], dev = 190;
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i], ai = a.Axis === "X" ? 0 : (a.Axis === "Y" ? 1 : 2), sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("#" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "[[" + sum.join("+") + "]/" + a.Measures.length + "]" : sum[0];
    block(["#" + (dev + i) + "=" + minusU(mean, a.Expected - org[ai])]);
    block(["IF[ABS[#" + (dev + i) + "]GT" + fn(p.Tolerance) + "]THEN#3000=1(PROBE " + a.Axis + " OUT OF TOLERANCE)"]);
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    if (wcsIndex > 6) { msg(who() + "probing offset update currently supports G54..G59 (offsets 1..6) only"); return; }
    var act = 5221 + 20 * (wcsIndex - 1);
    var ws = ["G10", "L2", "P" + (Number(p.SetWorkOffset) + 1)];
    for (var j = 0; j < axes.length; j++) {
      var aj = axes[j].Axis === "X" ? 0 : (axes[j].Axis === "Y" ? 1 : 2);
      ws.push(axes[j].Axis + "[#" + (act + aj) + "+#" + (dev + j) + "]");
    }
    block(ws);
  } else {
    block(["#3006=1(CHECK PROBE RESULT #" + dev + "..)"]);
  }
}

function onClose() {
  onSectionEnd();
  if (coolant) { block(["M9"]); coolant = ""; }
  if (spindleOn) { block(["M5"]); spindleOn = false; }
  if (linesWritten() !== homedAt) safeRetractBlock();
  block(["M30"]);
  writeln("%");
}

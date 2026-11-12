// Doosan DNM milling post (Fanuc 0i / 31i-M).
// Provenance:
// - Fanuc Series 0i-MODEL F Operator's Manual (B-64604EN-2/01):
//   * Section 3.3 (p. 19/33, p. 20/34), Section 14.1 (p. 377-386): Machining Condition Selecting Function
//     G5.1 Q1 Rx (R1 to R10), G5.1 Q0 (cancel). Requires Fanuc AICC I (A02B-0340-S603) or AICC II (A02B-0340-S605).
//   * Section 3.3.2 (p. 22/36): Canned cycle cancel (G80) required, G5.1 Q0 required before drilling cycles (G73-G89).
//   * G-code dialect and Fanuc command structure from public ISO 6983 / RS-274.
// - Renishaw MP700 Probe System Installation and User's Guide (H-2000-5142-02-A):
//   * Chapter 4 (p. 4-2 to 4-17): Optical machine start / turn on sequence G65 P9832 (probe on) and G65 P9833 (probe off).
//   * Chapter 5: Renishaw Inspection Plus macros G65 P9810 (protected positioning) and G65 P9811 (surface measurement).
// - DN Solutions Co., Ltd. (formerly Doosan Machine Tools)
//   Operation Manual: Introduction / Precaution / Basic Operation (Vertical Machining Center)
//   Document No.: MV0009OPE241_v0 (Revision v0, Published 2024-06-11, Build MMYP0003)
//   Applicable models: DNM 4000/4500/5700/6700 series (Fanuc 0i-MF / 31i-B controls):
//   * Part 5 §3 (pp. 180–186) "M-Code List":
//     - M10: 4th axis clamp (table clamp) (p. 180)
//     - M11: 4th axis unclamp (table unclamp) (p. 180)
//     - M07: Mist coolant ON (p. 180)
//     - M08: Flood coolant ON (p. 180)
//     - M09: Coolant OFF (p. 180)
//     - M24: Chip conveyor run (forward) (p. 180)
//     - M25: Chip conveyor stop (p. 180)
//     - (Optional custom chip conveyor / auger packages: M31 forward, M33 stop)
//   * Part 8 §3 (p. 232) "Alarm List":
//     - Alarm 2323 "4TH AXIS CLAMP/UNCLAMP ALARM": verifies M10/M11 clamp/unclamp status timer,
//       mandating that M11 precedes rotary motion and M10 follows on separate blocks.
//   * Part 6 §2.6 (p. 217) "Coolant Supply":
//     - M88: High-Pressure Through-Spindle Coolant (TSC) ON
//     - M89: High-Pressure Through-Spindle Coolant (TSC) OFF
// - Real-world machine G-code verification:
//   * Production sample program from @spikeypineapple (NeoTech/Camcraft#614) stored at
//     docs/evidence/614-dnm5700-sample.nc, confirming G5.1 Q1 R5 / G5.1 Q0, M07, and
//     G65 P9832 / P9833 / P9810 / P9814 on Doosan DNM 5700 (Fanuc 0i-MF).
// Clean-Room Protocol: Original clean-room work; behavior specced only from public manuals.
// No vendor script text.
description = "Doosan DNM (Fanuc 0i/31i-M, 3/4-axis mill, TSC, G5.1 AICC, rotary brake, G73/G81-G89, G2/G3, G99)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public Doosan DNM & Fanuc documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it

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
var tcpmOn = false;        // G43.4 armed (stage D RTCP): re-armed at tool change
var lastA = 0, lastB = 0, lastC = 0; // angle words the last block commanded (deg)
var tiltRefused = false;   // a refusal the gate counts once, not per line
var compG = "";            // G41/G42/G40 pending for the next straight block (M13.11)
var compD = "";            // the D word riding the engage block
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var wearNoted = false;     // the wear-register comment was written this section (#38)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)

// Doosan DNM specific features:
var rotaryAClamped = true;       // A-axis rotary brake state: true = M10 clamped, false = M11 unclamped
var rotaryRefused = false;       // counted warning once for undeclared rotary clamp codes
var smoothingArmed = false;      // G5.1 Q1 requested for this section
var smoothingActive = false;     // G5.1 Q1 actively engaged in output
var smoothingLevel = 5;          // Machining condition selection R1..R10 (default R5 per DNM sample)
var smoothingRefused = false;    // counted warning once for out-of-range smoothing level
var probeTurnedOn = false;       // G65 P9832 optical turn-on state for Inspection Plus
var probeStyleRefused = false;   // counted warning once for unknown probing style
var chipConveyorStarted = false; // M31 chip conveyor running

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
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

// ------------------------------------------------ Rotary Brake (M10/M11)
// Doosan DNM 4th-axis rotary table brake control:
// Manual reference: DN Solutions Operation Manual MV0009OPE241_v0 Part 5 §3 (p. 180), Part 8 §3 (p. 232).
// M11 = Unclamp A-axis brake before rotating
// M10 = Clamp A-axis brake before cutting / after positioning
// Strict refusal: requires explicit declaration via options/properties (rotaryClamp/rotaryUnclamp
// or rotaryBrake: true), or machine definition. Never inferred from machine name.
function getRotaryCodes() {
  var clamp = "", unclamp = "";
  if (typeof postOptions !== "undefined") {
    if (postOptions.rotaryClamp) clamp = String(postOptions.rotaryClamp);
    if (postOptions.rotaryUnclamp) unclamp = String(postOptions.rotaryUnclamp);
    if (!clamp && !unclamp && postOptions.rotaryBrake) {
      clamp = "M10";
      unclamp = "M11";
    }
  }
  if (typeof properties !== "undefined") {
    if (properties.rotaryClamp) clamp = String(properties.rotaryClamp);
    if (properties.rotaryUnclamp) unclamp = String(properties.rotaryUnclamp);
    if (!clamp && !unclamp && properties.rotaryBrake) {
      clamp = "M10";
      unclamp = "M11";
    }
  }
  if (typeof machine !== "undefined") {
    if (machine.rotaryClamp) clamp = String(machine.rotaryClamp);
    if (machine.rotaryUnclamp) unclamp = String(machine.rotaryUnclamp);
    if (!clamp && !unclamp && machine.rotaryBrake) {
      clamp = "M10";
      unclamp = "M11";
    }
  }
  return { clamp: clamp, unclamp: unclamp };
}

// --------------------------------------------- Chip Conveyor (M31/M33)
// Manual reference: DN Solutions Operation Manual MV0009OPE241_v0 Part 5 §3 (p. 180) / Part 5 §2.13 (p. 137)
// chipConveyorStart (e.g. M31) / chipConveyorStop (e.g. M33)
var conveyorRefused = false;
function getChipConveyorCodes() {
  var start = "", stop = "";
  if (typeof postOptions !== "undefined") {
    if (postOptions.chipConveyorStart) start = String(postOptions.chipConveyorStart);
    if (postOptions.chipConveyorStop) stop = String(postOptions.chipConveyorStop);
  }
  if (typeof properties !== "undefined") {
    if (properties.chipConveyorStart) start = String(properties.chipConveyorStart);
    if (properties.chipConveyorStop) stop = String(properties.chipConveyorStop);
  }
  if (typeof machine !== "undefined") {
    if (machine.chipConveyorStart) start = String(machine.chipConveyorStart);
    if (machine.chipConveyorStop) stop = String(machine.chipConveyorStop);
  }
  return { start: start, stop: stop };
}

function ensureRotaryUnclamped() {
  if (!rotaryAClamped) return;
  var rc = getRotaryCodes();
  if (!rc.unclamp) {
    if (!rotaryRefused) {
      rotaryRefused = true;
      msg("rotary clamp/unclamp codes not declared for this machine or post options");
    }
    return;
  }
  block([rc.unclamp]);
  rotaryAClamped = false;
}

function ensureRotaryClamped() {
  if (rotaryAClamped) return;
  var rc = getRotaryCodes();
  if (!rc.clamp) {
    if (!rotaryRefused) {
      rotaryRefused = true;
      msg("rotary clamp/unclamp codes not declared for this machine or post options");
    }
    return;
  }
  block([rc.clamp]);
  rotaryAClamped = true;
}

// --------------------------------------- High-Speed Smoothing (G5.1 Q1 Rx)
// Fanuc AI Contour Control (AICC / Nano Smoothing) on Doosan DNM:
// Manual reference: FANUC Series 0i-MODEL F Operator's Manual (B-64604EN-2/01)
// - Section 3.3 (p. 19/33, p. 20/34), Section 14.1 (p. 377-386):
//   G5.1 Q1 Rx enables AICC with machining condition selection (R1 to R10).
//   Level 1 = roughing / high-speed, Level 10 = fine finishing / high-precision.
//   Default R5 per Doosan DNM 5700 sample program (docs/evidence/614-dnm5700-sample.nc).
//   G5.1 Q0 cancels the mode.
// - Section 3.3.2 (p. 22/36): Canned drilling cycles (G73-G89) cannot execute
//   with AICC active; G5.1 Q0 must precede them.
function checkSmoothing() {
  var want = false;
  var lvl = null;
  if (typeof postOptions !== "undefined") {
    if (postOptions.smoothing === true || postOptions.Smoothing === true) want = true;
    if (postOptions.smoothing === false || postOptions.Smoothing === false) want = false;
    if (has(postOptions.smoothingLevel)) lvl = Number(postOptions.smoothingLevel);
    else if (has(postOptions.SmoothingLevel)) lvl = Number(postOptions.SmoothingLevel);
  }
  if (typeof properties !== "undefined") {
    if (properties.smoothing === true || properties.Smoothing === true) want = true;
    if (has(properties.smoothingLevel)) lvl = Number(properties.smoothingLevel);
  }
  if (typeof smoothTol !== "undefined" && smoothTol > 0) want = true;

  if (lvl !== null) {
    if (lvl < 1 || lvl > 10 || !isFinite(lvl) || Math.floor(lvl) !== lvl) {
      if (!smoothingRefused) {
        smoothingRefused = true;
        msg("smoothing R level must be between 1 and 10, got " + lvl);
      }
      return false;
    }
    smoothingLevel = lvl;
  } else {
    smoothingLevel = 5;
  }
  return want;
}

function enableSmoothing() {
  if (smoothingArmed && !smoothingActive) {
    block(["G5.1", "Q1", "R" + smoothingLevel]);
    smoothingActive = true;
  }
}

function disableSmoothing() {
  if (smoothingActive) {
    block(["G5.1", "Q0"]);
    smoothingActive = false;
  }
}

// ------------------------------------------------------ RTCP & 4th Axis
// #797: the machine check reads a lone rotary about X through the tool
// axis (A0 B180 is its A180) and signs it by the stated direction (a
// [-1,0,0] table holds the negated angle). The RTCP path below writes the
// stream's letters as they come, never negated, so two poses the check
// passes would post wrong: a B or C word on a machine with no rotary of
// that name, and a non-zero A on a table stated along -X. Both are
// refused by name here, as fanuc.cps does. (A [-1,0,0] rotary leans off
// Z, so the mirror sets machine.rtcp and the indexing path never sees it.)
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
    var rc = getRotaryCodes();
    if (!rc.clamp || !rc.unclamp) {
      if (!rotaryRefused) {
        rotaryRefused = true;
        msg("rotary clamp/unclamp codes not declared for this machine or post options");
      }
      return null;
    }
    if (has(p.B) && p.B !== 0) {
      if (!tiltRefused) { tiltRefused = true; msg("tilted move on a machine without a tilting rotary"); }
      return null;
    }
    // #1155: the index names only a rotary the machine states, as the
    // RTCP path does - a turning letter with no rotary of that name is
    // refused by name, never posted as an address the control lacks; a
    // stated C that turns is refused too, as this path writes only A.
    var lostIx = missingLetter(p);
    if (lostIx) {
      if (!tiltRefused) { tiltRefused = true; msg("indexed move commands " + lostIx + " - the machine states no " + lostIx + " rotary"); }
      return null;
    }
    if (has(p.C) && Math.abs(p.C) > 1e-9) {
      if (!tiltRefused) { tiltRefused = true; msg("indexed move commands C - this dialect's indexing path writes only an A rotary"); }
      return null;
    }
    var rotWords = [];
    var aChanged = !has(lastA) || p.A !== lastA;
    if (aChanged) {
      rotWords.push(angleWord("A", p.A));
    }
    return rotWords;
  }
  if (inCycle) { msg("tilted move inside a canned-cycle group"); return null; }
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

// --------------------------------------------------- Cutter Compensation
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

// -------------------------------------------------------------- Coolant
// Doosan DNM coolant:
// - Flood: M08 (off M09)
// - Mist / Air blast: M07 (off M09)
// - Through-Spindle Coolant (TSC):
//   If machine defines through-spindle code, use it.
//   Otherwise, if useM8ForTSC option is set, use M08.
//   Default Doosan high-pressure TSC M-code is M88 (off M89 / M09).
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
  if (other || floodNamed) return other || "M8";
  if (cs.length > 0) msg(coolantWho() + "the machine states no flood coolant code - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "M8";
}

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
  // If through-tool requested but not explicitly in machine table:
  if (ask === "through-tool") {
    var useM8 = false;
    if (typeof postOptions !== "undefined" && postOptions.useM8ForTSC) useM8 = true;
    if (typeof properties !== "undefined" && properties.useM8ForTSC) useM8 = true;
    if (useM8) return "M8";
    msg(coolantWho() + "the machine states no through-tool coolant code - pick another coolant for the operation or add it to the machine");
    return "";
  }
  msg(coolantWho() + "the machine states no " + ask + " coolant code - pick another coolant for the operation or add it to the machine");
  return "";
}

// ------------------------------------------------------------ the plane
var planeG = 17;
function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}
function planeWord(g) {
  if (g === planeG) return "";
  planeG = g;
  return "G" + g;
}
function restorePlane() { var pw = planeWord(17); if (pw) block([pw]); }
function compPlaneWord(cg) { return cg ? planeWord(17) : ""; }
function compPlaneRefuse(g) {
  if (!compOn || g === 17) return false;
  msg("an " + (g === 18 ? "XZ (G18)" : "YZ (G19)") + " arc while cutter compensation is engaged - the control would compensate in that plane; cancel compensation (G40) first");
  return true;
}
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// -------------------------------------------------------------- header
function oWord() {
  var n = typeof programNumber !== "undefined" ? Math.round(Number(programNumber) || 1) : 1;
  var t = String(Math.max(1, Math.min(n, 9999)));
  while (t.length < 4) t = "0" + t;
  return "O" + t;
}

// ------------------------------------------------------------ comments
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {
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
  note(joinDash(["T" + pad2(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}
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
// G20 inch, G21 mm; a Fanuc control, whose published manual puts G20 /
// G21 in a block of its own at the program's start). Where the program's
// unit is chosen, and inch output, is #902.
function unitBlock() {
  block(["G21"]);
}

function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "DOOSAN DNM";
  writeln("%");
  block([oWord(), "(" + nm + ")"]);
  invertedNotes();
  unitBlock();
}

function onParameter(name, value) {
  if (name === "RetractPlane") hopZ = has(value) ? Number(value) : null;  // #1028
  if (name === "Smoothing") { smoothTol = Number(value) || 0; return; }
  if (name === "SmoothingLevel") { smoothingLevel = Number(value) || 5; return; }
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
  tcpmOn = false;
  lastA = 0; lastB = 0; lastC = 0;
  rotaryAClamped = true;
}

// ----------------------------------------------------------- Preamble
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block(["G90", "G17", wcsWord, "G40", "G49", "G80"]);
  if (toolWait) {
    toolNote();
    block(["T" + pad2(tNo), "M6"]);
    block(["G43", "H" + pad2(tNo)]);
    toolWait = false;
  }
  if (spindleWait && sRpm > 0) {
    block(["S" + ff(sRpm), "M3"]);
    spindleWait = false;
  }
  // #574 / #1035: a touch probe (a stated speed of 0, which leaves
  // spindleWait set) states its spindle stopped before its first move,
  // whatever the tool change did - a same-tool continuation keeps it
  // turning.
  if (spindleWait && !(sRpm > 0)) block(["M5"]);

  // Chip conveyor start if requested
  var wantConveyor = false;
  if (typeof postOptions !== "undefined" && postOptions.chipConveyor) wantConveyor = true;
  if (typeof properties !== "undefined" && properties.chipConveyor) wantConveyor = true;
  if (wantConveyor) {
    var cc = getChipConveyorCodes();
    if (!cc.start || !cc.stop) {
      if (!conveyorRefused) {
        conveyorRefused = true;
        msg("chip conveyor codes not declared for this machine or post options");
      }
    } else if (!chipConveyorStarted) {
      block([cc.start]);
      chipConveyorStarted = true;
    }
  }
  coolant = coolantFor(coolantAsk);
  if (coolant) block([coolant]);

  // High-speed smoothing arm (Fanuc AICC G5.1 Q1 Rx)
  smoothingArmed = checkSmoothing();
  if (smoothingArmed) enableSmoothing();
}

function afterOp() {
  var ask = afterAsk;
  afterAsk = "";
  if (!ask) return;
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    if (tcpmOn) { block(["G49"]); tcpmOn = false; }
    block(["G91", "G28", "Z0"]);
    block(["G28", "X0", "Y0"]);
    block(["G90"]);
    homedAt = linesWritten();
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();
  ensureProbeOff();
  disableSmoothing();
  ensureRotaryClamped();
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (compOn || compG) {
    msg("cutter compensation still engaged at section end - no cancel block");
    compG = ""; compD = ""; compOn = false;
  }
  wearNoted = false;
  if (coolant) {
    if (coolant === "M88") block(["M89"]); // TSC pump off (DN Solutions Operation Manual MV0009OPE241_v0 Part 6 §2.6)
    block(["M9"]);                         // Main coolant off
  }
  coolant = "";
  coolantAsk = "flood";
  block(["M5"]);
  afterOp();
  tcpmOn = false;
  lastA = 0; lastB = 0; lastC = 0;
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); hopZ = null; }  // #1028: the next section states its own hop

// ------------------------------------------------------------- motions
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
  compRefuse("a rapid");
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainRapid()) return;
  if (aChanged) {
    ensureRotaryUnclamped();
  }
  block(["G00"].concat(xyz(properties), tw));
  if (aChanged) {
    lastA = properties.A;
    ensureRotaryClamped();
  }
}
function onLinear() {
  startPreamble();
  enableSmoothing();
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  if (aChanged) {
    ensureRotaryUnclamped();
    lastA = properties.A;
  }
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G01", cg].concat(xyz(properties), [cd], tw, [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  enableSmoothing();
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  if (aChanged) {
    ensureRotaryUnclamped();
    lastA = properties.A;
  }
  var p = properties;
  if (compPlaneRefuse(planeOf(p))) return;
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G02" : "G03",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat(tw, [fw(p.FeedRate)]));
}

// ------------------------------------------------------- canned cycles
// One block per hole, holes of an unchanged cycle continuing modally
// with bare X Y lines.
// #994: G99, stated as each group opens - each hole returns to its R
// plane, where the stream's chain ends (RS-274 / ISO 6983: G98 returns
// to the level the tool stood at when the cycle was read, G99 to R).
// The hop and traverse to the next hole are the stream's own rapids,
// posted after G80 (above), so no hole's retract level is left to where
// the tool happened to be.
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
  disableSmoothing(); // Fanuc requires G5.1 Q0 before canned drilling cycles (B-64604EN-2/01 p. 22)
  restorePlane();
  var p = properties;
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0;
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
// Standard Fanuc G31 macro-B skip and Renishaw Inspection Plus support
// Manual reference: Renishaw MP700 Guide (H-2000-5142-02-A) Chapter 4 & 5:
// - G65 P9832: Probe On (optical transmission start)
// - G65 P9833: Probe Off (optical transmission stop / sleep)
// - G65 P9810: Protected positioning
// - G65 P9811: Surface touch measurement
var probeBase = 100;

function isInspectionPlus() {
  var style = "";
  if (typeof postOptions !== "undefined" && postOptions.probingStyle !== undefined) {
    style = String(postOptions.probingStyle);
  } else if (typeof properties !== "undefined" && properties.probingStyle !== undefined) {
    style = String(properties.probingStyle);
  }
  if (!style || style === "standard" || style === "g31") return false;
  if (style === "inspection-plus") return true;
  if (!probeStyleRefused) {
    probeStyleRefused = true;
    msg("unknown probing style " + style);
  }
  return false;
}

function ensureProbeOn() {
  if (isInspectionPlus() && !probeTurnedOn) {
    block(["G65", "P9832"]);
    probeTurnedOn = true;
  }
}

function ensureProbeOff() {
  if (probeTurnedOn) {
    block(["G65", "P9833"]);
    probeTurnedOn = false;
  }
}

function onProbeMove() {
  startPreamble();
  probeSeat();               // #1028
  disableSmoothing();
  compRefuse("a probing move");
  var p = properties;
  if (isInspectionPlus()) {
    ensureProbeOn();
    // Renishaw Inspection Plus single surface touch macro
    var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
    var ai = d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
    var ax = ["X", "Y", "Z"][ai];
    var pos = [p.X - orgX, p.Y - orgY, p.Z - orgZ][ai];
    block(["G65", "P9811", ax + fu(pos), fw(p.Feed)]);
    return;
  }
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
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
function onProbeGuard() {
  startPreamble();
  probeSeat();               // #1028
  disableSmoothing();
  compRefuse("a protected probe move");
  var p = properties;
  if (isInspectionPlus()) {
    ensureProbeOn();
    block(["G65", "P9810", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
    return;
  }
  block(["G31", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  var to = [p.X, p.Y, p.Z], from = [p.StartX, p.StartY, p.StartZ], org = [orgX, orgY, orgZ];
  for (var ai = 0; ai < 3; ai++) {
    if (has(from[ai]) && Math.abs(to[ai] - from[ai]) < 1e-6) continue;
    block(["IF[ABS[" + minusU("#" + (5061 + ai), to[ai] - org[ai]) + "]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)"]);
  }
}
function slotOf(word) {
  var n = Number(String(word).replace(/[^0-9]/g, "")) - 54;
  return n >= 0 && n <= 5 ? n : 0;
}
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 190;
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
  if (chipConveyorStarted) {
    var cc = getChipConveyorCodes();
    if (cc.stop) {
      block([cc.stop]);
    }
    chipConveyorStarted = false;
  }
  if (linesWritten() !== homedAt) {
    block(["G91", "G28", "Z0"]);
    block(["G28", "X0", "Y0"]);
  }
  block(["M30"]);
  writeln("%");
}

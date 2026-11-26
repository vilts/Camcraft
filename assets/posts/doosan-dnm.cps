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
axisInversion = true; // verified on the engine's mirrored stream for a flipped WCS X/Y and an inverted machine X/Y (#2475): trace equivalence test + golden
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it
workOffsetIndex = true; // reads PARAM WorkOffset as the 1-based offset number (#931)
workOffsetMax = 6;      // G54..G59; G54.1 P extended offsets are an option not verified here (#931)
wcsRotation = true;        // #760: writes G68 X0. Y0. R / G69 in Control mode from PARAM WcsYaw & WcsRotationMode
uprightReturn = true;      // #3477: uprightAfterTilt() retracts (G28 Z or G53 Z) and writes the rotaries to zero at each section after a tilted section (#3466); pinned by upright_after_tilt.rs
programUnits = "mm inch"; // #902: writes the document's unit (postOptions.Units), G21 / G20
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
  rotaryBrake: { type: "boolean", title: "Use rotary brake codes", group: "Rotary & 4th axis", description: "Command default rotary clamp/unclamp M-codes around indexing moves", default: false },
  rotaryClamp: { type: "string", title: "Rotary clamp M-code", group: "Rotary & 4th axis", description: "Custom M-code to clamp rotary axis (empty uses machine default)", default: "" },
  rotaryUnclamp: { type: "string", title: "Rotary unclamp M-code", group: "Rotary & 4th axis", description: "Custom M-code to unclamp rotary axis (empty uses machine default)", default: "" },
  chipConveyor: { type: "boolean", title: "Chip conveyor", group: "Machine", description: "Start and stop the chip conveyor during the program", default: false },
  chipConveyorStart: { type: "string", title: "Chip conveyor start code", group: "Machine", description: "Custom M-code to start the chip conveyor", default: "" },
  chipConveyorStop: { type: "string", title: "Chip conveyor stop code", group: "Machine", description: "Custom M-code to stop the chip conveyor", default: "" },
  useM8ForTSC: { type: "boolean", title: "Use M8 for through-spindle coolant", group: "Coolant", description: "Fall back to M8 when the machine declares no through-tool code", default: false },
  smoothingLevel: { type: "integer", title: "AICC smoothing level", group: "Smoothing", description: "Fanuc G5.1 Q1 Rx contour level (1..10)", min: 1, max: 10, default: 5 },
  probingStyle: {
    type: "enum",
    title: "Probing style",
    group: "Probing",
    description: "Skip (G31) or optical Inspection Plus macro calls",
    choices: [
      { id: "standard", title: "Standard (G31)" },
      { id: "g31", title: "G31 skip" },
      { id: "inspection-plus", title: "Inspection Plus (G65 P981x)" },
    ],
    default: "standard",
  },
};

// #1388: optional explanations; conservative house budgets, not control maxima.
explains = "paren";
commentMax = 60;
explainBlockMax = 80;

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var wcsIndex = 1;          // the offset number wcsWord writes (#931)
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
var setupNext = null;      // #1647: null means no marker; a cleaned name may be empty
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var wcsFlip = "";          // #1649: the flipped WCS axes ("X", "Y", "XY") - the engine mirrored the section
var tcpmOn = false;        // G43.4 armed (stage D RTCP): re-armed at tool change
var lastA = null, lastB = null, lastC = null; // angle words the last block commanded (deg); null: none written yet, so the next A is written (#950)
var tiltRefused = false;   // a refusal the gate counts once, not per line
var rotAt = { A: null, B: null, C: null }; // #885: the angles the control holds (null: not known) - never reset, a tool change turns nothing
var compG = "";            // G41/G42/G40 pending for the next straight block (M13.11)
var compD = "";            // the D word riding the engage block
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var wearNoted = false;     // the wear-register comment was written this section (#38)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var wcsYaw = 0;            // #760: Z rotation (deg) for Control mode (G68)
var wcsRotMode = "";       // #760: "control" | "bake" | ""
var wcsRotActive = false;  // #760: G68 is currently engaged in this section

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
// #1836: a length or angle word always carries its decimal point (`Z14.`,
// never `Z14`). Fanuc parameter 3401 bit 0 (DPI, Series 0i parameter
// manual) decides how the control reads a word with no point: DPI=0 reads
// it in least increments (Z14 is 0.014 mm under IS-B), DPI=1 in mm or
// inch. With the point, both settings read the same value.
function pt(s) { return s.indexOf(".") >= 0 ? s : s + "."; }
function fn(v) {  // a length as text (comments): mm at three decimals, or inch at four
  if (!has(v)) return "0";
  return trimNum(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3));
}
function fu(v) { return pt(fn(v)); }  // a length word, with its point
function fa(v) {  // an angle (degrees) at three decimals, with its point
  return pt(has(v) ? trimNum(Number(v).toFixed(3)) : "0");
}
function minusU(expr, v) {
  var s = fn(v);  // macro-B text: a constant is not scaled by DPI (#1836)
  if (s === "0") return expr;
  return s.charAt(0) === "-" ? expr + "+" + s.substring(1) : expr + "-" + s;
}
function ff(v) {  // a feed: whole mm/min, or in/min at two decimals
  if (!(has(v) && v > 0)) return "0";
  // #3690: a whole inch F without a point can use 0.01 in/min units.
  // FANUC B-64604EN-2/01, section 5.2.1, Unit of F command.
  return pt(inch ? trimNum((v / 25.4).toFixed(2)) : String(Math.round(v)));
}
function eps() { return inch ? "0.0001" : "0.001"; } // one output step, the probe checks' band
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; } // integer word (rpm)
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
// #3699: a tap feed is pitch x rpm, kept at 0.001 mm/min by the stream, and
// the control cuts the pitch F / S. G84 writes the feed at three mm or four
// inch decimals, with its point; a tap whose written F and S do not hold
// the pitch to 0.01 percent is refused by name.
function tapFeed(v) { return pt(fn(v)); }
function tapPitchHolds(feed) {
  var speed = Math.round(sRpm);
  var written = Number(fn(feed)) * (inch ? 25.4 : 1);
  var want = sRpm > 0 ? feed / sRpm : 0;
  var got = speed > 0 ? written / speed : 0;
  if (!(want > 0 && Math.abs(got - want) / want <= 1e-4)) {
    msg("canned cycle G84 (tap)" + (coolantOp ? " in operation " + coolantOp : "") + " feed " + feed + " mm/min at " + sRpm +
      " rpm posts as F" + tapFeed(feed) + " S" + speed + " - refused: the written pitch differs by more than 0.01 percent");
    return false;
  }
  return true;
}
function wa(letter, v) { return has(v) ? letter + fa(v) : ""; } // an angle word
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
function arcWord(k, v) { return wo(k, v, k === "X" ? orgX : (k === "Y" ? orgY : orgZ)); }  // #3442: a position word as written, for arcFate
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
function angleWord(letter, v) { return statesRotary(letter) ? wa(letter, v) : ""; }  // degrees: never the document unit
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
    var rs = (typeof machine !== "undefined" && machine.rotaries) || [];
    if (rs.length > 1) {
      if (!tiltRefused) { tiltRefused = true; msg("tilted move on a multi-axis machine without controller RTCP"); }
      return null;
    }
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
  if (other || floodNamed) return other || "M8";
  if (cs.length > 0) msg(coolantWho() + "the machine states no flood coolant code - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "M8";
}

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
function opNotes() {
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
function toolNote() {
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0) desc.push("D" + fn(toolDia));
  if (toolCr > 0) desc.push("R" + fn(toolCr));
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

// #901: the unit this post writes stated in a block of its own before
// any motion, so a control left in the other unit by power-on or the
// last program never misreads the numbers (RS-274 / ISO 6983: G20 inch,
// G21 mm; a Fanuc control, whose published manual puts G20 / G21 in a
// block of its own at the program's start). #902: the unit is the
// document's.
function unitBlock() {
  block([inch ? "G20" : "G21"]);
}

function onOpen() {
  inch = typeof postOptions !== "undefined" && postOptions.Units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "DOOSAN DNM";
  writeln("%");
  rawBlock([oWord(), "(" + nm + ")"]);
  writeHeaderExtras();
  invertedNotes();
  unitBlock();
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
  if (name === "SmoothingLevel") { smoothingLevel = Number(value) || 5; return; }
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "SetupChange") { setupNext = clean(value); return; }
  if (name === "WorkOffset" && value) wcsWord = offsetWord(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "WcsFlip") wcsFlip = String(value || "");
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
  tcpmOn = false;
  lastA = null; lastB = null; lastC = null; // #950: never assume A0 - the first A after this is written
  rotaryAClamped = true;
}

// #3466: a section that follows a tilted (or indexed) one starts upright.
// The angle words are modal and a tilted section's exit keeps its lean, so
// a section whose moves state no angle would cut under the old one. Each
// rotary the control holds (rotAt) away from zero is commanded back to
// zero - after G49 (the preamble's line), at the reference height, before
// the tool change and the first X/Y traverse, which states its height
// again. A turning A is unclamped and clamped around it, as onRapid does.
function uprightAfterTilt() {
  if (!heldRotaries().length) return;
  retractHome(false);
  if (optStr("safeRetract", "G28") !== "G53") block(["G90"]);
  rotariesToZero();
  xyUnknown = true;
}
// The rotaries the control holds away from zero (rotAt).
function heldRotaries() {
  var held = [], ls = ["A", "B", "C"];
  for (var i = 0; i < ls.length; i++) {
    var v = rotAt[ls[i]];
    if (has(v) && Math.abs(v) > 1e-9 && statesRotary(ls[i])) held.push(ls[i]);
  }
  return held;
}
// One block that turns each held rotary to zero; no X/Y/Z word. A
// turning A is unclamped before it and clamped after it.
function rotariesToZero() {
  var held = heldRotaries(), words = [];
  for (var i = 0; i < held.length; i++) { words.push(angleWord(held[i], 0)); rotAt[held[i]] = 0; }
  if (!words.length) return;
  var turnsA = held.indexOf("A") >= 0;
  if (turnsA) ensureRotaryUnclamped();
  block(["G00"].concat(words));
  if (turnsA) ensureRotaryClamped();
}

// #3540: the program ends upright too, so the operator unloads at zero
// and the next program does not start under the old lean. Same order as
// the return above: G49 (TCPM and the length offset off), Z to the
// reference height, the angles to zero, then X/Y home - incremental
// (G91 G28), so the via point is the current position, never work X0 Y0.
// A trailing Go home already lifted Z (homedAt): only the angles follow.
function uprightAtEnd() {
  if (!heldRotaries().length) return false;
  var homed = linesWritten() === homedAt;
  var g28 = optStr("safeRetract", "G28") !== "G53";
  if (!homed) { block(["G49"]); retractHome(false); }
  if (g28 && !homed) block(["G90"]);  // a Go home wrote its own G90
  rotariesToZero();
  if (!homed && g28) block(["G91", "G28", "X0", "Y0"]);
  return true;
}

// ----------------------------------------------------------- Preamble
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  if (optStr("showSequenceNumbers", "off") === "op-start") seqNextForce = true;
  var touchProbe = spindleWait && !(sRpm > 0);   // a stated speed of 0 (#523)
  opNotes();
  // #1649: the operator reads why every sign on a flipped axis is the other one.
  if (wcsFlip) note("WCS " + wcsFlip.split("").join(" AND ") + " FLIPPED - PROGRAM MIRRORED");
  block(["G90", "G17", wcsWord, "G40", "G49", "G80"]);
  uprightAfterTilt();
  if (toolWait) {
    if (secStarted > 0 && optBool("optionalStop", false)) block(["M1"]);
    toolNote();
    if (optStr("showSequenceNumbers", "off") === "tool-change") seqNextForce = true;
    block(["T" + pad2(tNo), "M6"]);
    block(["G43", "H" + pad2(tNo)]);
    xyUnknown = true;          // #1633: the first traverse states its height again
    toolWait = false;
  }
  secStarted += 1;
  if (spindleWait && sRpm > 0) {
    block(["S" + ni(sRpm), "M3"]);
    spindleOn = true;
    spindleWait = false;
  }
  // #574 / #1035: a touch probe (a stated speed of 0, which leaves
  // spindleWait set) states its spindle stopped before its first move,
  // whatever the tool change did - a same-tool continuation keeps it
  // turning.
  if (spindleWait && !(sRpm > 0)) { block(["M5"]); spindleOn = false; }
  probeSection = touchProbe && kindProbe;

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
  // #995: a touch probe section starts no coolant, whatever its ask -
  // fluid on the stylus and the measured faces spoils the reading.
  coolant = touchProbe ? "" : coolantFor(coolantAsk);
  if (coolant) block([coolant]);

  // High-speed smoothing arm (Fanuc AICC G5.1 Q1 Rx)
  smoothingArmed = checkSmoothing();
  if (smoothingArmed) enableSmoothing();
  if (wcsRotControl()) {
    block(["G68", "X0.", "Y0.", "R" + fa(wcsYaw)]);
    wcsRotActive = true;
  }
}

function afterOp() {
  var ask = afterAsk;
  var next = setupNext;
  afterAsk = "";
  setupNext = null;
  if (!ask) return;
  if (next !== null && (ask === "optional-stop" || ask === "stop")) {
    // #1647: lift Z before inviting the operator to re-clamp the part.
    if (tcpmOn) { block(["G49"]); tcpmOn = false; }
    retractHome(false);
    if (optStr("safeRetract", "G28") !== "G53") block(["G90"]);
    note("SETUP CHANGE - NEXT: " + (next || "UNNAMED SETUP"));
  }
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    if (tcpmOn) { block(["G49"]); tcpmOn = false; }
    retractHome(true);
    if (optStr("safeRetract", "G28") !== "G53") block(["G90"]);
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
  if (wcsRotActive) { block(["G69"]); wcsRotActive = false; }
  wcsYaw = 0;
  wcsRotMode = "";
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
  spindleOn = false;
  probeSection = false;
  kindProbe = false;         // the next section states its own tool kind
  afterOp();
  tcpmOn = false;
  lastA = null; lastB = null; lastC = null; // #950: never assume A0 - the first A after this is written
}

function onSection() { seatKnown = false; startPreamble(); }
function onSectionEnd() { endPreamble(); wcsFlip = ""; hopZ = null; clearZ = null; }  // #1028, #1633: the next section states its own planes

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
// The frame: written after the record's own tilt(), so under G43.4 when
// that record arms it - Z is then the tip's Z in the work offset, the
// frame the stream states its planes and its words in - and under G43
// otherwise (the indexing path), the frame of the record's own words,
// with the rotary still clamped. The rotaries turn only on the record
// itself, at its target, as before.
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
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainRapid()) return;
  if (!xyFirst(properties.X, properties.Y, properties.Z, false)) return;  // #1633: refused - the move is not written
  if (aChanged) {
    ensureRotaryUnclamped();
  }
  block(["G00"].concat(xyz(properties), tw));
  heldAngles(properties, tw);
  if (aChanged) {
    lastA = properties.A;
    ensureRotaryClamped();
  }
}
// #885: a feed that turns a rotary is capped at the rates the machine
// states, read the way the control reads F (the prelude's rotaryFeed):
// under G43.4 F is the tool tip's speed relative to the work, the
// rotaries carried along (FANUC 30i-B operator's manual, tool center
// point control); without it the Fanuc 0i-F times a combined block over
// one composite length that counts the A degrees as mm (FANUC 0i-F
// operator's manual B-64604EN, feedrate for rotation axes). Unstated
// rates cap nothing. The F word, or null when refused by name.
function feedWord(p, arc) {
  // Any rotary letter on the record, not A alone: a record that turns only
  // B or C is capped the same way (#951 review).
  if (!has(p.A) && !has(p.B) && !has(p.C)) return fw(p.FeedRate);
  if (typeof rotaryFeed !== "function") return fw(p.FeedRate);
  var tcp = typeof machine !== "undefined" && machine.rtcp === true;
  var r = rotaryFeed(p, rotAt, tcp ? ["A", "B", "C"] : ["A"], tcp ? "path" : "composite", arc, inch ? 0.00254 : 0.001, [orgX, orgY, orgZ], inch);
  if (r.refuse) { rotRefuse(r.refuse); return null; }
  if (!r.capped) return fw(p.FeedRate);
  var word = rotaryFeedWord(r.feed, inch);  // the post's own unit, as ff() writes F
  if (!word) { rotRefuse("a feed that turns a rotary over too short a move - the stated rotary rate leaves under 0.1 mm/min"); return null; }
  return pt(word);  // #3690: capped feeds need the same explicit unit point
}
var rotRefused = {};  // #885: each rate refusal counted once per program, not per line
function rotRefuse(s) { if (!rotRefused[s]) { rotRefused[s] = true; msg(s); } }
// The angles the control holds after a block: only the letters the block
// actually wrote (`tw`, the tilt words) - a word held back as modal
// leaves the control where the last written one put it.
function heldAngles(p, tw) {
  for (var i = 0; i < tw.length; i++) {
    var L = String(tw[i]).charAt(0);
    if ((L === "A" || L === "B" || L === "C") && has(p[L])) rotAt[L] = Number(p[L]);
  }
}
function onLinear() {
  startPreamble();
  enableSmoothing();
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  if (spindleStopped("feed move G01")) return;
  var fwd = feedWord(properties, false);
  if (fwd === null) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  if (aChanged) {
    ensureRotaryUnclamped();
    lastA = properties.A;
  }
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G01", cg].concat(xyz(properties), [cd], tw, [fwd]));
  heldAngles(properties, tw);
}
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  // #3442: an arc whose written ends coincide is no arc block (arcFate, PRELUDE).
  var fate = arcFate(arcWord, arcIncOf(fu));  // #3474: I J K are increments from the start
  if (fate !== "arc") { if (fate === "linear") onLinear(); return; }
  enableSmoothing();
  var aChanged = has(properties.A) && (!has(lastA) || properties.A !== lastA);
  var tw = tilt(properties);
  if (tw === null || chainCut()) return;
  if (spindleStopped("arc move " + (properties.CircularDirection === "CW" ? "G02" : "G03"))) return;
  var p = properties;
  var fwd = feedWord(p, true);
  if (fwd === null) return;
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
  if (aChanged) {
    ensureRotaryUnclamped();
    lastA = properties.A;
  }
  if (compPlaneRefuse(planeOf(p))) return;
  var g = p.CircularDirection === "CW" ? "G02" : "G03";
  if (optBool("useRadiusArcs", false) && emitArcWithRadius(p, g, tw, fwd)) {
    heldAngles(properties, tw);
    return;
  }
  block([
    planeWord(planeOf(p)),
    g,
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat(tw, [fwd]));
  heldAngles(properties, tw);
}
function emitArcWithRadius(p, g, tw, fwd) {
  // Keep the original I/J/K arc when it writes rotary angles. Splitting
  // would put the complete turn on the first segment and invalidate its feed cap.
  if (tw && tw.length) return false;
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
    words.push(wo("X", pt[0], orgX), wo("Y", pt[1], orgY), wo("Z", pt[2], orgZ), w("R", segs[i].rSigned));
    if (i === 0 && tw && tw.length) words = words.concat(tw);
    words.push(fwd);
    block(words);
  }
  return true;
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
  // #1820: standard Fanuc G83 (B-64604EN-2/01, 5.1.6) has no P.
  // Doosan OEM MV0009OPE241_v0 dwell behavior is not publicly verified.
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
  if (g === "G84" && !tapPitchHolds(Number(p.FeedRate))) return;
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
    var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
    if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
      words.push(w("Q", p.Q));
    }
    if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
    words.push(g === "G84" ? "F" + tapFeed(p.FeedRate) : fw(p.FeedRate));
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
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
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
  block(["IF[ABS[" + minusU("#" + (5061 + ai), tgt) + "]LT" + eps() + "]THEN#3000=2(PROBE NO CONTACT)"]);
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
  if (!xyFirst(properties.StartX, properties.StartY, properties.StartZ, true)) return;  // #1633: refused - the move is not written
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
    block(["IF[ABS[" + minusU("#" + (5061 + ai), to[ai] - org[ai]) + "]GT" + eps() + "]THEN#3000=3(PROBE HIT ON POSITIONING)"]);
  }
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
    block(["IF[ABS[#" + (dev + i) + "]GT" + fn(p.Tolerance) + "]THEN#3000=1(PROBE " + a.Axis + " OUT OF TOLERANCE)"]);
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var act = 5221 + 20 * (wcsIndex - 1);
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
// #2189: Fanuc modal Group 06 unit word (G20 / G21) after the final
// retract, right before M30, when postOptions.EndUnits asks for the
// other unit.
function endUnitsWord() {
  var want = typeof postOptions !== "undefined" && postOptions ? postOptions.EndUnits : undefined;
  if (want === "inch" && !inch) return "G20";
  if (want === "mm" && inch) return "G21";
  return "";
}
function onClose() {
  endPreamble();
  if (chipConveyorStarted) {
    var cc = getChipConveyorCodes();
    if (cc.stop) {
      block([cc.stop]);
    }
    chipConveyorStarted = false;
  }
  if (!uprightAtEnd() && linesWritten() !== homedAt) {
    retractHome(true);
  }
  var u = endUnitsWord();
  if (u) block([u]);
  block(["M30"]);
  writeln("%");
}

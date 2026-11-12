// Tormach PathPilot plasma post for the 1300PL (#671). Provenance: the
// plasma dialect from Tormach's public "UM10720: 1300PL Operator's
// Manual" (Version 1223A) - §6.2.1-6.2.2 (the test program and its code
// breakdown, pp. 50-51), §6.3-6.3.1 (AutoFS and the hand-stated block,
// pp. 51-52), §7.1 (G15, G16, p. 54) and §7.2 (M200-M213, pp. 54-55) -
// and Tormach's public "PathPilot Post Processor Guidelines" knowledge-
// base page (its Plasma column: what PathPilot's plasma control accepts
// and refuses). Behaviour only, per this repo's Clean-Room Protocol; no
// vendor post script, CAM post or CAM output was read. The hook contract
// is our engine's (assets/posts/AGENTS.md, docs/CNC-INTERMEDIATE.md);
// structure follows our own tormach.cps. The goldens lock this file's
// decisions; the decision record is docs/decisions/671-plasma-post.md.
description = "Tormach PathPilot plasma (1300PL: M200 AutoFS, G15 probe, G16 pierce, M205 torch off, G30)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
coolantModes = true;       // reads PARAM Coolant - to refuse every ask but Off

// Determinism law: no timestamps, no randomness - the same stream always
// posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var inch = false;          // the machine is programmed in inches (onOpen)
var wcsWord = "G54";       // from the WorkOffset PARAM
var orgX = 0, orgY = 0;    // the WCS slot origin (OriginX/Y PARAMs)
var sectionOpen = false;   // preamble written (a legacy .cl has no SECTION)
var lit = false;           // the torch is struck (onTorchOn .. onTorchOff)
var compWord = "";         // G41.1/G42.1 D.. or G40, written before the next G1
var compOn = false;        // the control is compensating
var lastF = "";            // the F word in force since the last M200
var atG30 = false;         // the last block written was G30
var opName = "", opKind = "", setupName = "", setupSaid = "", toolName = "";
var refused = {};          // one named refusal per key, not one per line

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch machine gets every length and
// feed divided by 25.4 and written at four decimals; a metric one keeps
// mm at three.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fu(v) { return trim(Number(inch ? v / 25.4 : v).toFixed(inch ? 4 : 3)); }
function ff(v) { return inch ? trim((v / 25.4).toFixed(2)) : String(Math.round(v)); }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) { writeln(a.join(" ")); atG30 = a.length === 1 && a[0] === "G30"; }
}
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + s + ")"); }
function refuse(key, s) {  // once per key (an operation's name rides the key)
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
// Comment text: PathPilot reads ( ) as a comment, and the guidelines ask
// for no % and no ; - all three become spaces; printable ASCII, 40 chars.
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function note(s) { block(["(" + s + ")"]); }

// ------------------------------------------------------------- machine
// This post writes words only PathPilot's plasma control reads (G15, G16,
// M200, M205): a machine that does not declare plasma is refused by name.
// A plasma table cuts flat sheet - a stated rotary has no word here.
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  var nm = (m && (m.name || m.model)) || "";
  if (!m || m.kind !== "plasma") {
    // The engine always publishes a machine object, with an empty name when
    // the document states none: that fallback made the sentence a double
    // negative ("no machine is not declared ..."). State it plainly (#998).
    var why = nm
      ? nm + " is not declared a plasma machine"
      : "no machine is stated, so none is declared a plasma machine";
    refuse("kind", "the Tormach 1300PL plasma post writes torch words (M200, G15, G16, M205) only a plasma table reads - " + why);
    return false;
  }
  if (m.rotaries && m.rotaries.length > 0) {
    refuse("rotary", (nm || "the machine") + " states a rotary axis - a plasma table cuts flat sheet in X and Y, and this post writes no rotary word");
    return false;
  }
  return true;
}

// -------------------------------------------------------------- header
// UM10720 §6.2.2: the preamble resets the modal state - absolute
// distance, path blending, the XY plane, the work offset. The guidelines:
// no % or $ fence and no ; at all, G64's P in INCHES whatever the program
// unit (so P0.005 Q0.005 - the manual's own numbers - on a G21 program
// too). Stated here once, never assumed: the unit, incremental arc
// centres (G91.1), feed per minute (G94; G95 is refused on plasma), no
// cutter compensation (G40), no length offset yet (G49), no cycle (G80).
function onOpen() {
  inch = typeof machine !== "undefined" && machine.units === "inch";
  var nm = (typeof machine !== "undefined" && (machine.name || machine.model)) || "CAMCRAFT";
  note(clean(nm));
  block([inch ? "G20" : "G21"]);
  block(["G90", "G91.1", "G94", "G17", "G40", "G49", "G80"]);
  block(["G64", "P0.005", "Q0.005"]);
}

function onParameter(name, value) {
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    // The guidelines: S, M3, M4 and M5 are not plasma words.
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - a plasma torch turns no spindle (PathPilot plasma takes no S, M3, M4 or M5)");
  } else if (name === "Coolant" && String(value) !== "off") {
    // The guidelines: M7, M8 and M9 are not plasma words.
    refuse("coolant" + opName, who() + "asks " + value + " coolant - a plasma table has none (PathPilot plasma takes no M7, M8 or M9)");
  }
}

function onToolChange(number, name) { toolName = clean(name); }

// The section's preamble: the work offset, the torch's length offset and
// the G30 height. The guidelines: "for plasma ... always H1" (Tool 1 is
// the torch, UM10720 §6.2.2), whatever the library numbers the torch. M6
// is NOT written: the guidelines list M6 as unsupported on plasma (the
// manual's test program carries T1 M6 - the conflict and this choice are
// recorded in the decision file). G30 sends the torch to the operator's
// G30 height before the first XY move (§6.2.2, "Initial Move").
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  if (!machineOk()) return;
  if (!has(properties.PlasmaKerf)) {
    refuse("op" + opName, who() + (/plasma/i.test(opKind)
      ? "the Plasma Profile path carries no plasma record (kerf, torch on and off) - regenerate it"
      : (opKind ? "a " + opKind + " operation" : "a path with no PLASMA header") +
        " is not plasma cutting - this post writes only Plasma Profile paths"));
    return;
  }
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + opName + (opKind ? " - " + opKind.toUpperCase() : ""));
  note("T1 " + (toolName ? toolName + " - " : "") + "PLASMA TORCH - KERF " + fu(properties.PlasmaKerf));
  block([wcsWord]);
  block(["G43", "H1"]);
  block(["G30"]);
}

function plasmaOk() { return sectionOpen && has(properties.PlasmaKerf) && !refused.kind && !refused.rotary; }

function onSection() { startPreamble(); }
function onSectionEnd() {
  sectionOpen = false;
  compWord = ""; compOn = false;
  opName = ""; opKind = "";
}

// ------------------------------------------------------------- the torch
// Per contour, UM10720 §6.2.1 and §6.3.1: rapid to the pierce point at
// the G30 height, then
//   M200  AutoFS: feed, THC voltage, pierce height and delay and the
//         amperage from the material picked in PathPilot (§6.3, §7.2);
//   M207 P / M208 P / M209 P  the pierce height, cut height (program
//         unit) and pierce delay (seconds) the op states - after M200,
//         which sets all three, so a stated number overrides it (§7.2);
//   G15   probe down to the plate, the WCS Z set to 0 there (§7.1);
//   G16   raise to the pierce height, fire, wait the delay, drop to cut
//         height, THC on (§7.1).
// The cut is XY only: the control owns every Z (§6.3; §7.1). M210/M211
// (THC volts, amps) are never stated - the op has no such numbers, M200
// supplies them. M203 (Start Torch) is not used: G16 strikes the arc.
var pending = null;        // the rapid target the next pierce stands at
function onTorchOn() {
  startPreamble();
  if (!plasmaOk()) return;
  var p = properties;
  var c = "C" + p.Contour + " " + String(p.ContourRole || "").toUpperCase();
  if (has(p.ParentContour)) c += " IN C" + p.ParentContour;
  if (has(p.SmallHole)) {
    c += p.PlasmaFeed === "auto"
      ? " - SMALL HOLE AT THE AUTOFS FEED"
      : " - SMALL HOLE " + Math.round(p.SmallHole * 100) + "% FEED";
  }
  note(c);
  var x = has(p.StartX) ? p.StartX : (pending ? pending[0] : null);
  var y = has(p.StartY) ? p.StartY : (pending ? pending[1] : null);
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  pending = null;
  block(["M200"]);
  if (has(p.PierceHeight)) block(["M207", "P" + fu(p.PierceHeight)]);
  if (has(p.CutHeight)) block(["M208", "P" + fu(p.CutHeight)]);
  if (has(p.PierceDelay)) block(["M209", "P" + trim(Number(p.PierceDelay).toFixed(3))]);
  block(["G15"]);
  block(["G16"]);
  lit = true;
  lastF = "";              // M200 set the feed: the op's F is stated again
}

// M205 puts the torch out and returns Z to the G30 height (§7.2); G30
// follows it, as the issue's per-contour block and the manual's own
// program end (§6.2.1) write it.
// A compensation word never outlives its torch span: an engage still
// pending here (COMP read, no lit straight G1 carried it) breaks the
// stream's law - the engage rides inside the span, ahead of its lead -
// so it is refused by name, and dropped either way so it can never land
// on the next contour's first G1 (a small hole the computer already
// offset would take the kerf twice).
function onTorchOff() {
  var engage = compWord !== "" && compWord !== "G40";
  compWord = "";
  if (!plasmaOk()) { lit = false; return; }
  if (engage) refuse("comp-pending" + opName, who() + "the torch goes out with a G41/G42 engage no lit straight move carried - the kerf compensation would land on the next contour");
  if (compOn) refuse("comp-open" + opName, who() + "the torch goes out under G41/G42 - the compensation was never cancelled");
  block(["M205"]);
  block(["G30"]);
  lit = false;
}

// --------------------------------------------------- cutter compensation
// The guidelines: G41/G42 (D optional) and G41.1/G42.1 are plasma words.
// G41.1/G42.1 D<diameter> states the kerf from the CL itself (LinuxCNC's
// dynamic compensation: D is the diameter), so the tool table's torch
// diameter cannot silently disagree. The engage and cancel each stand on
// their own line ahead of the straight lead the generator guarantees
// (longer than the kerf radius - the PathPilot entry rule).
function onRadiusCompensation(side) {
  var p = properties;
  if (!plasmaOk()) return;
  if (p.PlasmaCompensation !== "control") {
    refuse("comp" + opName, who() + "a G41/G42 span in a path the computer offset - the stream disagrees with its own plasma header");
    return;
  }
  if (p.CompensationType === "wear") {
    refuse("wear" + opName, who() + "wear compensation is not a plasma mode - the kerf rides G41.1/G42.1 D whole");
    return;
  }
  if (side === "off") { compWord = "G40"; return; }
  var d = 2 * Number(p.CompensationRadius);
  compWord = (side === "left" ? "G41.1" : "G42.1") + " D" + fu(d);
}
function flushComp() {
  if (!compWord) return;
  block([compWord]);
  compOn = compWord !== "G40";
  compWord = "";
}

// ------------------------------------------------------------- motions
// Rapids carry no Z (the control owns it, and M205/G30 left the torch at
// the G30 height): a rapid only names where the next pierce stands; the
// torch-on block rapids there in X Y alone.
function onRapid() {
  var p = properties;
  if (!machineOk()) return;
  if (has(p.A)) { refuse("tilt" + opName, who() + "a tilted move - a plasma table states no rotary axis"); return; }
  pending = [p.X, p.Y];
}

function feedWord(p) {
  if (p.PlasmaFeed === "auto") return "";       // AutoFS's feed (§6.2.2)
  if (!has(p.FeedRate) || p.FeedRate <= 0) {
    refuse("nofeed" + opName, who() + "a lit move states no feed while the op states one - refused");
    return "";
  }
  var f = "F" + ff(p.FeedRate);
  if (f === lastF) return "";
  lastF = f;
  return f;
}

// A cutting move: lit, flat (Z is the THC's), in the XY plane.
function cutOk(p) {
  startPreamble();
  if (!plasmaOk()) return false;
  if (has(p.A)) { refuse("tilt" + opName, who() + "a tilted move - a plasma table states no rotary axis"); return false; }
  if (!lit) {
    refuse("dark" + opName, who() + "a cutting move with the torch off - every plasma feed rides a TORCH ON span");
    return false;
  }
  if (has(p.StartZ) && has(p.Z) && Math.abs(p.Z - p.StartZ) > 0.0005) {
    refuse("z" + opName, who() + "a cutting move changes Z - the control owns the torch height (G15/G16 and THC), a plasma program cuts in X and Y only");
    return false;
  }
  return true;
}

function onLinear() {
  var p = properties;
  if (!cutOk(p)) return;
  flushComp();
  block(["G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), feedWord(p)]);
}

function onCircular() {
  var p = properties;
  if (!cutOk(p)) return;
  if (p.CircularPlane && p.CircularPlane !== "XY") {
    refuse("plane" + opName, who() + "an arc in the " + p.CircularPlane + " plane - a plasma cut is flat");
    return;
  }
  if (compWord) {
    refuse("comparc" + opName, who() + "G41/G42 engages or cancels on an arc - PathPilot needs a straight G1 lead");
    return;
  }
  var c = p.IJKIncrement || {};
  block([
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY),
    has(c.I) && has(p.StartX) ? "I" + fu(c.I - p.StartX) : "",
    has(c.J) && has(p.StartY) ? "J" + fu(c.J - p.StartY) : "",
    feedWord(p),
  ]);
}

// The guidelines list no canned cycle (G73, G76, G81-G89) for plasma.
function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - PathPilot plasma has no drilling cycle (G81-G89 are mill words)");
}

// ------------------------------------------------------------ footer
// UM10720 §6.2.1 ends on G30 (the G30 height and position); M30 ends the
// program (no % fence, per the guidelines).
function onClose() {
  if (!atG30) block(["G30"]);
  block(["M30"]);
}

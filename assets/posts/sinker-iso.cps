// Generic ISO-style sinker EDM post (#542, part of #290). Provenance: the
// RS-274 / ISO 6983 word conventions every ISO-style control shares (the
// G0 rapid, G90 absolute distance, the G54 work offset, the ( ) comment)
// from public G-code standards, and the sinker roles (an electrode picked
// up and referenced, a generator condition selected, a servo burn to a
// depth, an orbit pattern) from general die-sink EDM practice in public
// machine-tool handbooks. Behaviour only, per this repo's Clean-Room
// Protocol; no vendor post script, CAM post, CAM output or EDM manual
// table was read. The umbrella's decision D5 is this file's law: the post
// writes the RS-274 motion words itself, and EVERY EDM-specific code -
// the unit word, the pickup, the reference, the condition select, the
// burn to depth, the program end - comes from the machine file's
// `edm.codes` (main's `Machine.edm_codes`, its names and texts held to
// one law by `post::machine::edm_codes`). A missing code refuses the post
// by name; none is invented here. A condition the machine's
// `edm.conditions` does not list, an orbit (#544, expanded into explicit
// moves the stock sim judged) and a Sinker Side Burn (#545) are refused
// by name: this post writes a straight burn down Z only. Decision record:
// docs/decisions/542-sinker-iso-post.md. Not exercised on a real EDM
// control.
description = "Generic ISO-style sinker EDM (every EDM code from the machine's edm.codes)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1;          // reference contract: integer feature mask
coolantModes = true;       // reads PARAM Coolant - to refuse every ask but Off
serves = ["sinker_edm"];   // posts for sinker EDMs only (#1266 manifest; the app's serves door, #1232)
programUnits = "mm inch"; // #902, #1382: writes the document's unit (postOptions.Units) with edm.codes.metric / edm.codes.inch

// Determinism law: no timestamps, no randomness - the same stream always
// posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var inch = false;          // the document is in inches (postOptions.Units, onOpen)
var wcsWord = "G54";       // from the WorkOffset PARAM
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var sectionOpen = false;   // preamble written (a legacy .cl has no SECTION)
var sectionOk = false;     // the preamble was written whole
var toolNumber = 0, toolName = "";
var opName = "", opKind = "", toolKind = "", setupName = "", setupSaid = "";
var condition = null;      // PARAM SinkerCondition: the control's condition id
var pickup = null;         // PARAM ElectrodePickup: the pickup offset, mm
var orbit = null, orbitRadius = null; // PARAM EdmOrbit / EdmOrbitRadius
var last = { X: null, Y: null, Z: null }; // the modal axis words written
var refused = {};          // one named refusal per key, not one per line

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch program gets every length and
// feed divided by 25.4 and written at four decimals; a metric one keeps
// mm at three. Every length keeps its decimal point: an ISO control may
// read a bare integer in its least increment.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function num(v, dp) {
  var s = Number(v).toFixed(dp);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "");
  if (s === "-0." || s === "-.") s = "0.";
  return s.indexOf(".") >= 0 ? s : s + ".";
}
function fu(v) { return num(inch ? v / 25.4 : v, inch ? 4 : 3); }
function ff(v) { return num(inch ? v / 25.4 : v, inch ? 2 : 1); }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else writeln("(" + s + ")"); }
function refuse(key, s) {
  if (refused[key]) return;
  refused[key] = true;
  msg(s);
}
function who() { return opName ? "operation " + opName + ": " : ""; }
// Comment text: ( ) is the ISO comment; parentheses, ; and % become
// spaces, anything outside printable ASCII a ?, at most 40 characters.
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function note(s) { if (s) block(["(" + s + ")"]); }
function machineName() {
  var m = typeof machine !== "undefined" ? machine : null;
  return (m && (m.name || m.model)) || "the machine";
}

// ------------------------------------------------------- codes as data
// What each code role is for, said in a refusal so the user knows which
// line of the manual to read.
var ROLES = {
  metric: "the word that puts the control in millimetres",
  inch: "the word that puts the control in inches",
  pickup: "the block that picks up the electrode, {tool} its number",
  reference: "the block that states the electrode's pickup offset, {offset}",
  condition: "the block that selects a generator condition, {id}",
  depth: "the servo burn to a depth, {z} the tip's Z (and {f} the feed)",
  end: "the program end"
};
// The template of `role` from machine.edm.codes with its {placeholders}
// filled from `vals`, or null after one named refusal: the code is not
// stated, names a placeholder this post does not fill, or needs a value
// the stream does not state.
function code(role, vals) {
  var e = typeof machine !== "undefined" && machine ? machine.edm : null;
  var t = e && e.codes ? e.codes[role] : undefined;
  if (typeof t !== "string" || t.replace(/\s+/g, "") === "") {
    refuse("code-" + role, machineName() + " states no edm.codes." + role + " (" + ROLES[role] +
      ") - fill it from the control's manual; this post invents no EDM code");
    return null;
  }
  return fill("edm.codes." + role, t, vals || {});
}
function fill(what, t, vals) {
  var bad = null, missing = null;
  var out = t.replace(/\{([A-Za-z]+)\}/g, function (all, k) {
    if (!Object.prototype.hasOwnProperty.call(vals, k)) { bad = bad || k; return ""; }
    if (vals[k] === null || vals[k] === undefined || vals[k] === "") { missing = missing || k; return ""; }
    return String(vals[k]);
  });
  if (bad !== null) {
    var known = Object.keys(vals).map(function (k) { return "{" + k + "}"; }).join(", ");
    refuse("ph-" + what, machineName() + ": " + what + " holds {" + bad + "}, which this post does not fill" +
      (known ? " (it fills " + known + ")" : " (it fills none)"));
    return null;
  }
  if (missing !== null) {
    refuse("val-" + what + opName, who() + what + " needs {" + missing + "}, and the program states no value for it");
    return null;
  }
  if (/[{}]/.test(out)) {
    refuse("brace-" + what, machineName() + ": " + what + " holds an unmatched brace - " + t);
    return null;
  }
  return out;
}
function emit(line) { if (line !== null) writeln(line); return line !== null; }

// ------------------------------------------------------------- machine
// This post writes a sinker's program: a machine not declared
// `kind: sinker_edm` is refused by name (the app's serves door refuses
// it first, from `serves`; this is the script's own word for the same
// law, and the one the CLI meets).
function machineOk() {
  var m = typeof machine !== "undefined" ? machine : null;
  if (!m || m.kind !== "sinker_edm") {
    refuse("kind", "the generic sinker post writes sinker EDM blocks - " + machineName() + " is not declared a sinker EDM");
    return false;
  }
  return true;
}

// -------------------------------------------------------------- header
function onOpen() {
  // #902, #1382: the document's unit, never the machine file's (that is
  // only the unit the machine file was written in).
  inch = typeof postOptions !== "undefined" && postOptions && postOptions.Units === "inch";
  note(clean(machineName()));
  if (!machineOk()) return;
  emit(code(inch ? "inch" : "metric"));
  block(["G90"]);
}

function onParameter(name, value) {
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value);
  else if (name === "ToolKind") toolKind = clean(value);
  else if (name === "SetupName") setupName = clean(value);
  else if (name === "SinkerCondition") condition = value === null || value === undefined ? null : String(value);
  else if (name === "ElectrodePickup" && has(value)) pickup = Number(value);
  else if (name === "EdmOrbit") orbit = value ? String(value) : null;
  else if (name === "EdmOrbitRadius" && has(value)) orbitRadius = Number(value);
  else if (name === "SpindleSpeed" && has(value) && Number(value) > 0) {
    refuse("rpm" + opName, who() + "states " + Math.round(Number(value)) + " rpm - an electrode turns no spindle, and this post writes no spindle word");
  } else if (name === "Coolant" && String(value) !== "off") {
    refuse("coolant" + opName, who() + "asks " + value + " coolant - a sinker flushes dielectric under its condition, and this post writes no coolant word");
  }
}

function onToolChange(number, name) { toolNumber = number; toolName = clean(name); }

// The section's preamble: the electrode picked up and referenced, the
// work offset, the condition selected. Written whole, or refused by
// name - never half.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  sectionOk = false;
  if (!machineOk()) return;
  if (toolKind && !/electrode/i.test(toolKind)) {
    refuse("tool" + opName, who() + "a " + toolKind + " is not an electrode - this post writes sinker burns only");
    return;
  }
  if (/side/i.test(opKind)) {
    refuse("side" + opName, who() + "a Sinker Side Burn moves the electrode sideways - this post writes a straight burn down Z only");
    return;
  }
  if (opKind && !/sinker/i.test(opKind)) {
    refuse("op" + opName, who() + "a " + opKind + " operation is not a sinker burn - this post writes sinker burns only");
    return;
  }
  if (condition === null || condition.replace(/\s+/g, "") === "") {
    refuse("cond" + opName, who() + "the program names no EDM condition - the burn's condition selects the generator");
    return;
  }
  if (!listed(condition)) {
    refuse("unlisted" + opName, who() + "condition " + clean(condition) + " is not in " + machineName() +
      "'s edm.conditions - this post selects only a condition the machine lists");
    return;
  }
  if (orbit !== null) {
    refuse("orbit" + opName, who() + "a " + clean(orbit) + " orbit - CamCraft expands it into the moves the stock sim judged, " +
      "and this post writes a straight burn down Z only (it never swaps them for the control's orbit word); burn without an orbit");
    return;
  }
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + opName + (opKind ? " - " + opKind.toUpperCase() : ""));
  note("T" + toolNumber + (toolName ? " " + toolName : "") + " - ELECTRODE - CONDITION " + clean(condition));
  if (!emit(code("pickup", { tool: toolNumber > 0 ? String(toolNumber) : null }))) return;
  if (!emit(code("reference", { offset: has(pickup) ? fu(pickup) : null }))) return;
  block([wcsWord]);
  if (!emit(code("condition", { id: condition }))) return;
  last = { X: null, Y: null, Z: null };
  sectionOk = true;
}

// Whether the machine's condition table lists `id`, as the table writes
// it (the lookup law of assets/machines/AGENTS.md, #531).
function listed(id) {
  var e = typeof machine !== "undefined" && machine ? machine.edm : null;
  var list = e && e.conditions ? e.conditions : [];
  for (var i = 0; i < list.length; i++) {
    if (list[i] && String(list[i].id) === id) return true;
  }
  return false;
}

function onSection() { startPreamble(); }
function onSectionEnd() {
  sectionOpen = false;
  sectionOk = false;
  opName = ""; opKind = ""; toolKind = "";
  condition = null; pickup = null; orbit = null; orbitRadius = null;
}

// ------------------------------------------------------------- motions
function axis(letter, v, o) {
  if (!has(v)) return "";
  var w = fu(v - o);
  if (last[letter] === w) return "";
  last[letter] = w;
  return letter + w;
}
function moveOk(p) {
  startPreamble();
  if (!sectionOk) return false;
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("tilt" + opName, who() + "a rotary move - the generic sinker post writes X Y Z only");
    return false;
  }
  return true;
}

// A rapid is the RS-274 G0, its changed words only (the first after the
// preamble states all three) - the straight line the stock sim judged.
function onRapid() {
  var p = properties;
  if (!moveOk(p)) return;
  block(["G0", axis("X", p.X, orgX), axis("Y", p.Y, orgY), axis("Z", p.Z, orgZ)]);
}

// A feed is the burn: straight down Z, X and Y where the rapid left
// them, written as the machine's edm.codes.depth. Anything else - a feed
// with X/Y travel, a feed up, an arc, a cycle - is not a sinker burn
// this post can state, and is refused by name.
function onLinear() {
  var p = properties;
  if (!moveOk(p)) return;
  var dx = has(p.StartX) && has(p.X) ? Math.abs(p.X - p.StartX) : 0;
  var dy = has(p.StartY) && has(p.Y) ? Math.abs(p.Y - p.StartY) : 0;
  if (dx > 0.0005 || dy > 0.0005) {
    refuse("xy" + opName, who() + "a feed move with X/Y travel - this post burns straight down Z only");
    return;
  }
  if (!has(p.Z) || (has(p.StartZ) && p.Z >= p.StartZ - 0.0005)) {
    refuse("up" + opName, who() + "a feed move that does not go down Z - a burn feeds down, every retract is a rapid");
    return;
  }
  var z = fu(p.Z - orgZ);
  var f = has(p.FeedRate) && p.FeedRate > 0 ? ff(p.FeedRate) : null;
  if (emit(code("depth", { z: z, f: f }))) last.Z = z;
}

function onCircular() {
  if (!moveOk(properties)) return;
  refuse("arc" + opName, who() + "an arc - a sinker burn feeds straight down Z");
}

function onCycle() {
  refuse("cycle" + opName, who() + "a canned cycle - a sinker burn has none");
}

function onRadiusCompensation(side) {
  refuse("comp" + opName, who() + "cutter compensation - the spark gap is the condition's, placed by the generator, never a G41/G42");
}

// ------------------------------------------------------------ footer
function onClose() {
  if (!machineOk()) return;
  emit(code("end"));
}

// Original CamCraft Klartext post. HEIDENHAIN iTNC 530 Conversational Format,
// 533 190-22 (9/2006), NC 340 490-03: §§4.4, 5.2, 6.4, 7.2, 7.4, 8.9, 14.4.
// https://content.heidenhain.de/doku/tnc_guide/pdf_files/iTNC530/34049x-neu/en/bhb.pdf
// Machine installation selects the qualified language profile and change procedure.
description = "Heidenhain Klartext (metric 3-axis milling)";
vendor = "CamCraft";
legal = "Original clean-room work";
certificationLevel = 1;
minimumRevision = 1;
extension = "H";
setCodePage = "utf-8";
capabilities = 1;
serves = ["mill"];
programUnits = "mm";
coolantModes = true;
failOnWarning = true;
workOffsetIndex = true;
workOffsetMax = 6;

var blockNumber = 0;
var programName = "";
var section = Object.create(null);
var inSection = false;
var moved = false;
var installedTool = null;
var setupSignature = null;
var lastPoint = null;

function refuse(reason) {
  // A script error also prevents the standalone post from saving a partial file.
  throw new Error("Heidenhain Klartext: " + reason);
}
function block(text) {
  writeln(String(blockNumber++) + " " + text);
}
function cleanText(value) {
  // User text appears only in single-line comments.
  return String(value || "").replace(/[^A-Za-z0-9 _.-]/g, " ").trim().slice(0, 48);
}
function required(value, field) {
  if (typeof value !== "number" || !isFinite(value)) refuse("missing or invalid " + field);
  return value;
}
function decimal(value, places, limit, field) {
  required(value, field);
  if (Math.abs(value) > limit) refuse(field + " exceeds the control range");
  // Input coordinates are millimetres, including sub-grid origins and stock.
  // Round once on the control grid after translating into the preset frame.
  return Number(value.toFixed(places));
}
function local(value, axis) {
  return decimal(required(value, axis) - section["Origin" + axis], 4, 99999.9999, axis);
}
function coord(axis, value) {
  var n = local(value, axis);
  return axis + (n < 0 ? "" : "+") + n;
}
function xyz(x, y, z) {
  return coord("X", x) + " " + coord("Y", y) + " " + coord("Z", z);
}
function rate(value, maximum, field) {
  var word = decimal(value, 3, 99999.999, field);
  if (word <= 0 || (maximum > 0 && Math.max(value, word) > maximum)) {
    refuse(field + " must be positive and within the machine maximum");
  }
  return word;
}
function workOffsetName(n) {
  return Number.isInteger(n) && n >= 1 && n <= workOffsetMax ? "preset " + n : "";
}

function onOpen() {
  var controller = machine.controllerIdentity;
  if (!controller || controller.model !== "iTNC 530" || controller.nc_software !== "340490-03") {
    refuse("unsupported or missing controller profile; select iTNC 530 NC 340490-03");
  }
  if (machine.model !== "Fehlmann Picomax 55 (iTNC 530)") {
    refuse("the manual quill procedure requires the Fehlmann Picomax 55 (iTNC 530) installation");
  }
  var axes = Array.isArray(machine.linearAxes) ? machine.linearAxes.slice().sort() : [];
  if (machine.kind !== "mill" || axes.length !== 3 ||
      axes[0] !== "X" || axes[1] !== "Y" || axes[2] !== "Z" ||
      machine.rotaries.length || machine.spindles !== 1) {
    refuse("requires a single-spindle XYZ milling machine");
  }
  if (machine.toolChangeMode !== "manual-retract-top") {
    refuse("requires the manual-retract-top tool change configuration");
  }
  var minimum = required(machine.spindleMin, "minimum spindle speed");
  var maximum = required(machine.spindleMax, "maximum spindle speed");
  if (minimum <= 0 || maximum < minimum) {
    refuse("the installation must state a positive spindle speed range");
  }
  programName = program.numbered ? String(program.number) : "CAMCRAFT";
  block("BEGIN PGM " + programName + " MM");
}
function onParameter(name, value) {
  if (inSection) refuse("unexpected parameter inside a section: " + name);
  // A positive list prevents a newly introduced motion or controller request
  // from silently running through the baseline profile as plain milling.
  switch (name) {
    case "StartX": case "StartY": case "StartZ":
    case "OriginX": case "OriginY": case "OriginZ":
    case "StockXYLeft": case "StockXYRight": case "StockXYFront": case "StockXYBack":
    case "StockZBottom": case "StockZTop": case "ClearancePlane":
    case "SpindleSpeed": case "WorkOffset": case "Coolant":
    case "SetupName": case "OperationName":
      section[name] = value;
      return;
    case "ToolKind": case "ToolDiameter": case "ToolCornerRadius":
      if (section[name] !== undefined && section[name] !== value) {
        refuse("conflicting " + name + " declarations in one section");
      }
      section[name] = value;
      return;
    case "OperationKind":
    case "RetractPlane": case "TopOfStock":
    // #2769: the stock's shape describes the box already stated; this
    // post's BLK FORM reads StockZBottom, not these.
    case "StockShape": case "StockShapeBottom": case "StockShapeDiameter":
      return;
    case "SectionMode":
      if (value === "3-axis") return;
      break;
  }
  refuse("unsupported parameter " + name + " (" + value + ")");
}
function onToolChange(number, name) {
  if (inSection || !Number.isInteger(number) || number < 1 || number > 32767) {
    refuse("missing or invalid tool number");
  }
  if (section.tool !== undefined) refuse("multiple TOOL declarations in one section");
  section.tool = number;
  section.toolName = name;
}
var toolDefinitions = Object.create(null);
function checkToolDefinition() {
  var kind = typeof section.ToolKind === "string" ? section.ToolKind.toLowerCase() : "";
  if (["flat end mill", "ball-nose", "bull-nose", "chamfer mill"].indexOf(kind) === -1) {
    refuse("unsupported milling tool kind " + (kind || "(unstated)"));
  }
  var diameter = required(section.ToolDiameter, "ToolDiameter");
  var cornerRadius = section.ToolCornerRadius === undefined ? 0 : section.ToolCornerRadius;
  if (diameter <= 0) {
    refuse("ToolDiameter must be positive");
  }
  if (required(cornerRadius, "ToolCornerRadius") < 0 || cornerRadius > diameter / 2) {
    refuse("ToolCornerRadius requires a compatible ToolDiameter");
  }
  // Compare the original identity, before shortening it for a program comment.
  // Keep every slot for the entire program, including tools used again later.
  var definition = JSON.stringify([section.toolName, kind, diameter, cornerRadius]);
  if (toolDefinitions[section.tool] !== undefined && toolDefinitions[section.tool] !== definition) {
    refuse("conflicting definitions for tool T" + section.tool + " (name, kind, diameter or corner radius)");
  }
  toolDefinitions[section.tool] = definition;
}
function floodRequested() {
  if (section.Coolant === "off") return false;
  if (section.Coolant !== undefined && section.Coolant !== "flood") {
    refuse("unsupported coolant " + section.Coolant);
  }
  // The installation must actually provide the standard flood circuit.
  if (!machine.coolants.some(function(c) {
    return c && typeof c === "object" && String(c.name).toLowerCase() === "flood" &&
      /^M0?8$/i.test(c.mCode);
  })) refuse("flood requires a machine Flood entry with M8");
  return true;
}
function writeSetup() {
  var lo = [section.StockXYLeft, section.StockXYFront, section.StockZBottom];
  var hi = [section.StockXYRight, section.StockXYBack, section.StockZTop];
  for (var i = 0; i < 3; ++i) {
    var axis = "XYZ"[i];
    required(section["Origin" + axis], "Origin" + axis);
    var size = local(required(hi[i], "stock maximum " + axis), axis) -
      local(required(lo[i], "stock minimum " + axis), axis);
    if (size < 0.05 || size > 99999.999) refuse("BLK FORM stock size outside graphical range on " + axis);
  }
  if (!workOffsetName(section.WorkOffset)) refuse("missing or unsupported preset (WorkOffset)");
  var signature = JSON.stringify([section.SetupName, section.WorkOffset, lo, hi,
    section.OriginX, section.OriginY, section.OriginZ]);
  if (setupSignature !== null) {
    if (signature !== setupSignature) refuse("one program requires one setup, blank and preset");
    return;
  }
  setupSignature = signature;
  block("BLK FORM 0.1 Z " + xyz(lo[0], lo[1], lo[2]));
  block("BLK FORM 0.2 " + xyz(hi[0], hi[1], hi[2]));
  if (program.comment) block("; " + cleanText(program.comment));
  block("; CONFIRM XYZ MODE - QUILL COUPLED - W FIXED");
  block("; CONFIRM SPINDLE STOPPED - NO TILT");
  block("; CANCEL ACTIVE COORDINATE TRANSFORMATIONS");
  block("; CONFIRM QUILL AT UPPER CHANGE POSITION");
  block("; VERIFY +Z RETRACT PATH AND CHANGE SPACE");
  block("; CHECK FULL-RETRACT XY PATH FOR ALL TOOLS");
  block("; VERIFY TOOL TABLE AND PRESET " + section.WorkOffset);
  block("; CHECK PRESET X Y Z FILLED AND ROTATION ZERO");
  block("M0");
  block("CYCL DEF 247 DATUM SETTING ~");
  writeln("  Q339=+" + section.WorkOffset);
  block("M137"); // F is mm/min even if the previous program selected feed per revolution.
}
function onSection() {
  if (inSection) refuse("nested operation sections");
  if (section.tool === undefined) refuse("operation has no tool call");
  checkToolDefinition();
  var rpm = rate(section.SpindleSpeed, machine.spindleMax, "SpindleSpeed");
  if (Math.min(section.SpindleSpeed, rpm) < machine.spindleMin) {
    refuse("SpindleSpeed is below the machine minimum");
  }
  var flood = floodRequested();
  writeSetup();
  var clearance = required(section.ClearancePlane, "ClearancePlane");
  var startZ = required(section.StartZ, "StartZ");
  if (clearance <= section.StockZTop || startZ < clearance ||
      local(clearance, "Z") <= local(section.StockZTop, "Z")) {
    refuse("the operation must start at or above ClearancePlane, above the stock");
  }
  var startXY = coord("X", section.StartX) + " " + coord("Y", section.StartY);
  var startHeight = coord("Z", startZ);
  if (section.OperationName) block("; OPERATION " + cleanText(section.OperationName));
  // At cold start the incoming tool and active axis are unknown. The first
  // stop requires the quill at the change position before the operator loads
  // the tool. Later changes retract under the previous explicit Z tool call.
  if (installedTool !== section.tool) {
    if (installedTool !== null) block("L M140 MB MAX");
    // The active traverse range or DCM can stop M140 short of the physical
    // change position. Check the actual quill and drive state at every change.
    block("; CHECK DRIVES OFF BEFORE TOUCHING TOOL");
    block("; CHECK QUILL AT PHYSICAL CHANGE POSITION");
    block("; ABORT IF EITHER CHECK FAILS");
    block("; THEN INSTALL TOOL " + section.tool + " " + cleanText(section.toolName));
    block("M0");
  }
  block("TOOL CALL " + section.tool + " Z S" + rpm);
  installedTool = section.tool;
  block("L M140 MB MAX"); // Positive Z is now established by TOOL CALL.
  // M140 has no completion signal for the intended tip clearance. The tool
  // may extend farther than its predecessor or stop short of upper travel.
  block("; CHECK ACTUAL TIP HEIGHT AND ENTIRE XY PATH");
  block("; ABORT IF TOOL OR HOLDER CANNOT CLEAR");
  block("M0");
  block("M3");
  block(flood ? "M8" : "M9");
  // Establish the programmed start before the first record, including an arc
  // or a plunge as the first record. Stay at the upper limit during XY travel,
  // then descend only in the column of the verified start point.
  block("L " + startXY + " R0 FMAX");
  block("L " + startHeight + " R0 FMAX");
  lastPoint = [section.StartX, section.StartY, startZ];
  moved = false;
  inSection = true;
}

function motion() {
  if (!inSection) refuse("motion outside an operation section");
  var p = properties;
  if (p.A !== undefined || p.B !== undefined || p.C !== undefined) refuse("rotary or tilted move");
  // Stream starts can be finer than a serialized motion endpoint. A real
  // discontinuity is an error: C arcs start from the last commanded point.
  if (Math.abs(required(p.StartX, "StartX") - lastPoint[0]) > 1e-8 ||
      Math.abs(required(p.StartY, "StartY") - lastPoint[1]) > 1e-8 ||
      Math.abs(required(p.StartZ, "StartZ") - lastPoint[2]) > 1e-8) {
    refuse("motion does not start at the previous programmed position");
  }
  lastPoint = [p.X, p.Y, p.Z];
  moved = true;
  return p;
}
function onRapid() {
  var p = motion();
  block("L " + xyz(p.X, p.Y, p.Z) + " R0 FMAX");
}
function onLinear() {
  var p = motion();
  block("L " + xyz(p.X, p.Y, p.Z) + " R0 F" + rate(p.FeedRate, machine.feedRateMax, "feed"));
}
function onCircular() {
  // #3442: an arc whose written ends coincide is no C block (arcFate,
  // PRELUDE): a short one is an L move, or no block (the last programmed
  // point still moves on). A near-full turn keeps the refusal below.
  var fate = arcFate(function (k, v) { return local(v, k); });
  if (fate === "linear") { onLinear(); return; }
  if (fate === "none") { motion(); return; }
  var p = motion();
  if (p.CircularPlane !== "XY" || p.Z !== p.StartZ) refuse("only constant-Z XY arcs are supported");
  var centre = p.IJKIncrement || {};
  var centreWords = coord("X", centre.I) + " " + coord("Y", centre.J);
  var endWords = coord("X", p.X) + " " + coord("Y", p.Y);
  var cx = local(centre.I, "X"), cy = local(centre.J, "Y");
  var startX = local(p.StartX, "X"), startY = local(p.StartY, "Y");
  var endX = local(p.X, "X"), endY = local(p.Y, "Y");
  if (startX === endX && startY === endY && (p.StartX !== p.X || p.StartY !== p.Y)) {
    refuse("rounding would turn the XY arc into a full circle");
  }
  var radius = Math.hypot(startX - cx, startY - cy);
  // MP7431 permits 0.0001 through 0.016 mm. The machine's selected value
  // is unstated, so use the strictest documented endpoint tolerance.
  if (radius < 0.001 || Math.abs(radius - Math.hypot(endX - cx, endY - cy)) > 0.0001000001) {
    refuse("inconsistent XY arc centre or radius");
  }
  if (p.CircularDirection !== "CW" && p.CircularDirection !== "CCW") refuse("unknown arc direction");
  var direction = p.CircularDirection === "CW" ? "DR-" : "DR+";
  block("CC " + centreWords);
  block("C " + endWords + " " + direction + " F" + rate(p.FeedRate, machine.feedRateMax, "arc feed"));
}
function onSectionEnd() {
  if (!inSection || !moved) refuse("operation section is empty or has no start");
  if (lastPoint[2] < section.ClearancePlane ||
      local(lastPoint[2], "Z") < local(section.ClearancePlane, "Z")) {
    refuse("operation ends below ClearancePlane; provide a verified retract before stopping the spindle");
  }
  block("M9");
  block("M5");
  inSection = false;
  section = Object.create(null);
}
function onClose() {
  if (inSection || !setupSignature || Object.keys(section).length) refuse("incomplete program");
  block("M2");
  block("END PGM " + programName + " MM");
}

// Refuse before the host can substitute an ordinary motion or comment for a
// record that requests a material change, spindle synchronization or probing.
function onCycle() { refuse("unsupported drilling cycle " + properties.CycleType); }
function onDwell() { refuse("unsupported dwell"); }
function onRadiusCompensation() { refuse("unsupported control cutter compensation"); }
function onThreadCycle() { refuse("unsupported threading cycle"); }
function onThreadCycleEnd() { refuse("unsupported threading cycle"); }
function onPartLoading() { refuse("unsupported part loading"); }
function onPartUnloading() { refuse("unsupported part unloading"); }
function onPartTransfer() { refuse("unsupported part transfer"); }
function onProbe() { refuse("unsupported probing"); }
function onProbeMove() { refuse("unsupported probing"); }
function onProbeRetreat() { refuse("unsupported probing"); }
function onProbeGuard() { refuse("unsupported probing"); }
function onProbeResult() { refuse("unsupported probing"); }
function onPlasma() { refuse("unsupported plasma operation"); }
function onTorchOn() { refuse("unsupported torch operation"); }
function onTorchOff() { refuse("unsupported torch operation"); }
// The host also sends informational program notes here. Preserve their complete
// text (including the accuracy bound) and strip control-language delimiters.
// A WARN record still blocks the program through the failOnWarning manifest.
function onComment(text) {
  // Preserve word boundaries in multiline notes before the host replaces
  // non-printable characters with question marks.
  var singleLine = String(text || "").replace(/\s+/g, " ");
  var words = commentText(singleLine).match(/.{1,64}(?: |$)|.{1,64}/g) || [];
  for (var i = 0; i < words.length; ++i) block("; " + words[i].trim());
}

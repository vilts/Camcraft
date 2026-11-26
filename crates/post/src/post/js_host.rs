//! Provenance: original host helpers and RS-274 output conventions from docs/CNC-INTERMEDIATE.md.

use boa_engine::JsValue;

/// The JS shim every script sees before its own text. Original,
/// in-repo; specced by `docs/CNC-INTERMEDIATE.md`.
pub const PRELUDE: &str = r#"
var OUT = [];
var _outputLines = 0;
var _emitLine = (function (global, maxLines, operation) {
  var exceeded = false;
  var count = 0;
  Object.defineProperty(global, "_outputLimitExceeded", {
    get: function () { return exceeded; },
    enumerable: false,
    configurable: false
  });
  Object.defineProperty(global, "_outputLimitReached", {
    get: function () { return exceeded ? count + 1 : 0; },
    configurable: false
  });
  return function (s) {
    if (exceeded || count >= maxLines) {
      exceeded = true;
      throw new Error("post output line limit exceeded: limit " + maxLines +
        ", reached " + (count + 1) + ", operation " + operation);
    }
    count++;
    _outputLines++;
    OUT.push(s);
  };
})(this, _maxOutputLines, _outputOperation);
var MESSAGES = [];
var LINE = "";
var properties = {};
var W = properties;
var currentPosition = { X: 0, Y: 0, Z: 0 };
// The program's number (#111): 1 for a whole-job program; one file
// per operation counts 1, 2, 3... in program order. A dialect that
// writes an O-word reads it at onOpen.
var programNumber = 1;
// The document's program identity (#258): \`number\` is programNumber
// again; \`numbered\` is true only when the document states a number
// (a dialect whose default program writes none keeps writing none);
// \`comment\` is the document's program comment, "" when it states none.
var program = { number: 1, numbered: false, comment: "" };
var postOptions = {};
var machine = { name: "", rapidRate: 0, feedRateMax: 0, spindleMax: 0, coolants: [], rotaries: [], rtcp: false, units: "mm", spindleRanges: [], rigidTapping: true, rigidTappingStated: false, safeRetractZ: null, model: "" };
function _flushLine() { if (LINE !== "") { _emitLine(LINE); LINE = ""; } }
function writeln(s) { if (s !== undefined) LINE += String(s); _flushLine(); }
function write(s) { if (s !== undefined) LINE += String(s); }
// Lines written so far, a pending partial line included (#504): a
// script marks where a block ended and later asks whether anything
// followed it.
function linesWritten() { _flushLine(); return OUT.length; }
function writeBlock() {
  _flushLine();
  var parts = [];
  for (var i = 0; i < arguments.length; i++) {
    var a = arguments[i];
    if (a === undefined || a === null || a === "") continue;
    parts.push(String(a));
  }
  if (parts.length > 0) _emitLine(parts.join(" "));
}
function writeWord(w) { if (w !== undefined && w !== null && w !== "") write(String(w)); }
function writeAddress(letter, value, dp) {
  if (value === undefined || value === null) return;
  write(String(letter) + formatUnit(Number(value), dp));
}
// #594: the default comment is the RS-274 / ISO 6983 `( )` form, so
// its text loses every parenthesis (one would end or nest the comment)
// and every control character (one would end the block).
function writeComment(s) {
  var t = String(s === null || s === undefined ? "" : s);
  writeln("(" + t.replace(/[()]/g, " ").replace(/[\x00-\x1F\x7F]/g, " ") + ")");
}
function writeBlankLine() { _flushLine(); _emitLine(""); }
function writeOutput(s) { write(s); }
function message(s) { MESSAGES.push(String(s)); }
function getProperty(k) { return properties[k]; }
function setProperty(k, v) { properties[k] = v; }
function formatUnit(v, dp) {
  if (typeof v !== "number" || !isFinite(v)) return "0";
  if (dp === undefined) dp = 3;
  var s = v.toFixed(dp);
  if (s.indexOf(".") >= 0) { s = s.replace(/0+$/, "").replace(/\.$/, ""); }
  if (s === "-0") s = "0";
  return s;
}
function formatMotion(v) { return formatUnit(v, 3); }
function formatFeed(v) {
  if (typeof v !== "number" || !isFinite(v) || v <= 0) return "0";
  return String(Math.round(v));
}
function onComment(s) { writeln(s); }
// Full program notes retain their bound; name cleaners may truncate it.
// Remove the delimiter and control characters used by the shipped dialects.
function commentText(s) {
  return String(s === null || s === undefined ? "" : s)
    .replace(/[()%;*\/!~{}$]/g, " ").replace(/[^\x20-\x7E]/g, "?")
    .replace(/\s+/g, " ").trim();
}
// Bar-feeder lifecycle defaults (M5.2 G.2): the events carry no
// motion and no post may invent an M-code, so the default voices
// the intent as a comment - visible to the operator, harmless to
// any controller. A script with a real feeder dialect overrides
// these exactly as it overrides any other hook.
function onPartLoading() {
  var a = properties.Advance;
  writeComment("PART LOAD" + (a > 0 ? " ADVANCE " + formatUnit(a) : ""));
}
function onPartUnloading() { writeComment("PART UNLOAD"); }
// The handoff step of the same lifecycle family (M5.3 C): a comment
// naming the stations, the machine's own dialect by override.
function onPartTransfer() {
  writeComment("PART TRANSFER " + properties.From + "->" + properties.To);
}
// The measurement ask of the same family (M5.3 D): the default states
// the budget as a comment - which probe protocol measures to it (an
// RS238 hand check, a macro cycle, a laser) is the override's
// dialect, and the engine invents none of it.
function onProbe() {
  writeComment("PROBE TOL " + formatUnit(properties.Tolerance));
}
// The reference-era language-version pragma: accepted and ignored,
// because the engine (not the script) decides semantics.
function setLanguageVersion(_v) { return 0; }
"#;

/// Strict boolean tests for hook returns (an `undefined` from a
/// missing hook is neither - only a literal `false` swallows).
#[must_use]
pub fn is_true(v: &JsValue) -> bool {
    v.as_boolean() == Some(true)
}

#[must_use]
pub fn is_false(v: &JsValue) -> bool {
    v.as_boolean() == Some(false)
}

/// The hook names the engine may call, per the format doc.
pub const HOOKS: &[&str] = &[
    "onOpen",
    "onParameter",
    "onToolChange",
    "onSection",
    "onSectionEnd",
    "onCommand",
    "onRapid",
    "onLinear",
    "onCircular",
    "onPlunge",
    "onCycle",
    "onCyclePoint",
    "onCycleEnd",
    "onDwell",
    "onSpindleSpeed",
    "onRadiusCompensation",
    "onThreadCycle",
    "onThreadCycleEnd",
    "onPartLoading",
    "onPartUnloading",
    "onPartTransfer",
    "onProbe",
    "onProbeMove",
    "onProbeRetreat",
    "onProbeGuard",
    "onProbeResult",
    // #671: the plasma header and the torch spans.
    "onPlasma",
    "onTorchOn",
    "onTorchOff",
    // #1882: the waterjet jet spans.
    "onJetOn",
    "onJetOff",
    "onComment",
    "onClose",
];

/// The program's identity a post writes (#258): its number (the
/// O-word of a numbering dialect, 1 when the document states none),
/// whether the document stated that number, and the document's
/// program comment ("" for none - the dialect writes its own default),
/// and the header notes the program must carry (#2996: a model checked
/// at reduced accuracy), each voiced through `onComment` after `onOpen`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramId {
    pub number: u32,
    pub numbered: bool,
    pub comment: String,
    pub notes: Vec<String>,
}

impl Default for ProgramId {
    fn default() -> Self {
        Self { number: 1, numbered: false, comment: String::new(), notes: Vec::new() }
    }
}

/// A finite expansion allowance, shared by emitted and directly appended lines.
pub(crate) struct OutputBudget {
    pub max: usize,
    pub operation: String,
    reached: Option<usize>,
}

impl OutputBudget {
    pub fn new(records: usize, operation: &str) -> Self {
        // Sixteen output lines per input record plus header/footer space.
        // u32::MAX keeps the JS counter exact on all supported host widths.
        let max = records.saturating_mul(16).saturating_add(1024)
            .max(super::js::MAX_OUTPUT_LINES).min(u32::MAX as usize);
        Self { max, operation: operation.to_owned(), reached: None }
    }

    pub fn check(&mut self, ctx: &mut boa_engine::Context) -> Result<(), super::PostError> {
        if self.reached.is_none() {
            let count = ctx.eval(boa_engine::Source::from_bytes(b"_outputLimitReached"))
                .map_err(|e| super::PostError::Script(format!("{e}")))?
                .as_number().map(|n| n.max(0.0) as usize).unwrap_or(0);
            return self.check_count(count);
        }
        self.check_count(0)
    }

    pub fn check_count(&mut self, count: usize) -> Result<(), super::PostError> {
        if self.reached.is_none() && count > self.max {
            self.reached = Some(count);
        }
        if let Some(reached) = self.reached {
            return Err(super::PostError::Script(format!(
                "post output line limit exceeded: limit {}, reached {reached}, operation {}",
                self.max, self.operation
            )));
        }
        Ok(())
    }

    pub fn out_len(&mut self, ctx: &mut boa_engine::Context) -> Result<usize, super::PostError> {
        self.check(ctx)?;
        let result = ctx.eval(boa_engine::Source::from_bytes(b"(_flushLine(), OUT.length)"));
        if result.is_err() {
            self.check(ctx)?;
        }
        let len = result.map_err(|e| super::PostError::Script(format!("{e}")))?
            .as_number().map(|n| n.max(0.0) as usize).unwrap_or(0);
        self.check_count(len)?;
        Ok(len)
    }
}

/// Pull a host buffer after the caller checks and flushes its output budget.
pub(crate) fn pull(ctx: &mut boa_engine::Context, global: &str) -> Result<Vec<String>, super::PostError> {
    let v = ctx.eval(boa_engine::Source::from_bytes(global.as_bytes()))
        .map_err(|e| super::PostError::Script(format!("{e}")))?;
    let j = v.to_json(ctx).map_err(|e| super::PostError::Script(format!("{e}")))?
        .unwrap_or(serde_json::Value::Array(vec![]));
    Ok(match j {
        serde_json::Value::Array(items) => items.iter().map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }).collect(),
        _ => Vec::new(),
    })
}

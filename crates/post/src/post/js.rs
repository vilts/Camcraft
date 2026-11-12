//! The post script host: boa_engine with our own JS prelude.
//!
//! Provenance: the lifecycle-hook *contract* follows the published
//! Fusion 360 post architecture (behavior only, per the Clean-Room
//! Protocol); everything below - the shim text, the property names, the
//! `OUT` buffer design - is ours, written for this repo. No vendor
//! script text or engine code is reproduced.
//!
//! Design note: boa's host functions cannot capture environment
//! (`NativeFunction::from_fn_ptr` is a bare `fn`), so the prelude is
//! pure JS and the Rust side *pulls*: it sets the globals
//! (`properties`, `currentPosition`, `machine`), calls the hooks, and
//! reads the output buffer back as JSON. One direction per seam.

use super::PostError;
use boa_engine::{Context, JsValue, Source};
use serde_json::Value;
use std::collections::BTreeMap;

/// The JS shim every script sees before its own text. Original,
/// in-repo; specced by `docs/CNC-INTERMEDIATE.md`.
pub const PRELUDE: &str = r#"
var OUT = [];
var MESSAGES = [];
var LINE = "";
var properties = {};
var W = properties;
var currentPosition = { X: 0, Y: 0, Z: 0 };
// The program's number (#111): 1 for a whole-job program; one file
// per operation counts 1, 2, 3... in program order. A dialect that
// writes an O-word reads it at onOpen.
var programNumber = 1;
var postOptions = {};
var machine = { name: "", rapidRate: 0, feedRateMax: 0, spindleMax: 0, coolants: [], rotaries: [], rtcp: false, units: "mm", spindleRanges: [], rigidTapping: true, safeRetractZ: null, model: "" };
function _flushLine() { if (LINE !== "") { OUT.push(LINE); LINE = ""; } }
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
  if (parts.length > 0) OUT.push(parts.join(" "));
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
function writeBlankLine() { _flushLine(); OUT.push(""); }
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
    "onComment",
    "onClose",
];

/// One loaded post script.
pub struct Script {
    ctx: Context,
    present: Vec<(&'static str, bool)>,
    /// The `coolantModes = true` manifest flag (#150): the script reads
    /// `PARAM Coolant`. Without it a non-Flood ask is refused by name.
    coolant_modes: bool,
    /// The `liveTooling = true` manifest flag (issue #33): the script
    /// voices C-axis live-tool sections. Without it a `PARAM LiveTool`
    /// section is refused by name.
    live_tooling: bool,
    /// The `afterModes = true` manifest flag (#110): the script reads
    /// `PARAM AfterOperation`. Without it a section asking a stop or a
    /// trip home is refused by name - the script would run straight on.
    after_modes: bool,
    /// Explicit floating-holder tapping, rather than legacy rigid TAP (#51).
    floating_tapping: bool,
    /// The `axisInversion = true` manifest flag (#374): the script's
    /// output is verified (goldens + the equivalence test) on the
    /// engine's mirrored stream for an inverted X / Y axis. Without it a
    /// machine with an inverted axis is refused by name.
    axis_inversion: bool,
    /// The `probeAxisInversion = true` manifest flag (#392): the script's
    /// probing blocks (skip move, trip read, back-off, deviation, work
    /// offset write) are verified on the mirrored stream by a macro
    /// interpreter on a simulated inverted machine. Without it probing
    /// on an inverted axis is refused by name.
    probe_axis_inversion: bool,
    /// The `threadHands = true` manifest flag (issue #629): the script
    /// reads `PARAM SpindleDirection` and states the thread's hand.
    /// Without it a thread that needs the spindle reversed is refused
    /// by name - the script would cut it the other hand under M03.
    thread_hands: bool,
}

impl Script {
    /// Load the prelude plus one `.cps` text. A syntax error is a clean
    /// refusal, never a half-defined script.
    pub fn load(cps: &str) -> Result<Self, PostError> {
        let mut ctx = Context::default();
        let src = format!("{PRELUDE}\n{cps}");
        if let Err(e) = ctx.eval(Source::from_bytes(src.as_bytes())) {
            return Err(PostError::Script(format!("{e}")));
        }
        let mut s = Self {
            ctx,
            present: Vec::new(),
            coolant_modes: false,
            live_tooling: false,
            after_modes: false,
            floating_tapping: false,
            axis_inversion: false,
            probe_axis_inversion: false,
            thread_hands: false,
        };
        let mut present = Vec::new();
        for h in HOOKS {
            let yes = s
                .eval(&format!("typeof {h} === \"function\""))
                .map(|v| is_true(&v))
                .unwrap_or(false);
            present.push((*h, yes));
        }
        s.present = present;
        s.coolant_modes = s
            .eval("typeof coolantModes !== \"undefined\" && coolantModes === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.live_tooling = s
            .eval("typeof liveTooling !== \"undefined\" && liveTooling === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.after_modes = s
            .eval("typeof afterModes !== \"undefined\" && afterModes === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.floating_tapping = s
            .eval("typeof floatingTapping !== \"undefined\" && floatingTapping === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.thread_hands = s
            .eval("typeof threadHands !== \"undefined\" && threadHands === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.axis_inversion = s
            .eval("typeof axisInversion !== \"undefined\" && axisInversion === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.probe_axis_inversion = s
            .eval("typeof probeAxisInversion !== \"undefined\" && probeAxisInversion === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        Ok(s)
    }

    fn has(&self, name: &str) -> bool {
        self.present.iter().any(|(h, yes)| *h == name && *yes)
    }

    fn eval(&mut self, src: &str) -> Result<JsValue, PostError> {
        self.ctx
            .eval(Source::from_bytes(src.as_bytes()))
            .map_err(|e| PostError::Script(format!("{e}")))
    }

    /// Publish the flattened machine object once.
    pub fn set_machine(&mut self, m: &Value) -> Result<(), PostError> {
        self.eval(&format!("machine = {m};"))?;
        Ok(())
    }

    /// Publish the program's number (#111) before `onOpen` reads it.
    pub fn set_program_number(&mut self, n: u32) -> Result<(), PostError> {
        self.eval(&format!("programNumber = {n};"))?;
        Ok(())
    }

    /// A top-level string the script states (a manifest entry such as
    /// `extension`), or `None` when it states none.
    pub fn string_global(&mut self, name: &str) -> Result<Option<String>, PostError> {
        let v = self.eval(&format!("(typeof {name} === \"string\") ? {name} : null"))?;
        Ok(v.as_string().map(|s| s.to_std_string_escaped()))
    }

    /// A top-level number the script states (the manifest's
    /// `capabilities` mask), or `None` when it states none.
    pub fn number_global(&mut self, name: &str) -> Result<Option<f64>, PostError> {
        let v = self.eval(&format!("(typeof {name} === \"number\") ? {name} : null"))?;
        Ok(v.as_number())
    }

    /// Publish the document's post options once (issue #34).
    pub fn set_options(&mut self, o: &Value) -> Result<(), PostError> {
        self.eval(&format!("postOptions = {o};"))?;
        Ok(())
    }

    /// Publish this record's property object (params + record +
    /// position keys), keeping the `W` alias honest.
    pub fn set_properties(&mut self, props: &BTreeMap<String, Value>) -> Result<(), PostError> {
        let obj: serde_json::Map<String, Value> =
            props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        self.eval(&format!(
            "properties = {}; W = properties;",
            Value::Object(obj)
        ))?;
        Ok(())
    }

    pub fn set_current(&mut self, x: f64, y: f64, z: f64) -> Result<(), PostError> {
        self.eval(&format!(
            "currentPosition = {{ X: {}, Y: {}, Z: {} }};",
            lit(x),
            lit(y),
            lit(z)
        ))?;
        Ok(())
    }

    /// Call a hook by name with raw argument text ("" for none); the
    /// missing-hook case is a no-op returning undefined. A JS throw is
    /// a `Script` error - the Prime Directive's refusal shape.
    pub fn call(&mut self, name: &str, args: &str) -> Result<JsValue, PostError> {
        if !self.has(name) {
            return Ok(JsValue::undefined());
        }
        self.eval(&format!("{name}({args})"))
    }

    /// True when `name` exists (used for the onPlunge preference).
    pub fn hook_exists(&self, name: &str) -> bool {
        self.has(name)
    }

    /// True when the script declares `coolantModes = true` - it reads
    /// the section's `PARAM Coolant` (#150).
    pub fn reads_coolant(&self) -> bool {
        self.coolant_modes
    }

    /// True when the script declares `threadHands = true` - it reads
    /// `PARAM SpindleDirection` and voices a thread's hand (#629).
    pub fn voices_thread_hands(&self) -> bool {
        self.thread_hands
    }

    /// True when the script declares `liveTooling = true` - it voices
    /// a C-axis live-tool section (issue #33).
    pub fn voices_live_tooling(&self) -> bool {
        self.live_tooling
    }

    /// True when the script declares `afterModes = true` - it reads
    /// `PARAM AfterOperation` and writes the stop or home (#110).
    pub fn reads_after_operation(&self) -> bool {
        self.after_modes
    }

    pub fn voices_floating_tapping(&self) -> bool {
        self.floating_tapping
    }

    /// True when the script declares `axisInversion = true` - verified
    /// on the engine's mirrored stream for an inverted axis (#374).
    pub fn translates_inverted_axes(&self) -> bool {
        self.axis_inversion
    }

    /// True when the script declares `probeAxisInversion = true` - its
    /// probing blocks are verified on the mirrored stream (#392).
    pub fn translates_inverted_probing(&self) -> bool {
        self.probe_axis_inversion
    }

    /// Lines flushed so far (the driver's section boundary marker).
    /// Open partial lines count as flushed: a hook that ends a record
    /// mid-line still wrote a line of the program.
    pub fn out_len(&mut self) -> Result<usize, PostError> {
        let v = self.eval("(_flushLine(), OUT.length)")?;
        Ok(v.as_number().map(|n| n.max(0.0) as usize).unwrap_or(0))
    }

    /// Pull the finished line buffer and the `message()` log.
    pub fn drain(&mut self) -> Result<(Vec<String>, Vec<String>), PostError> {
        let out = self.pull("OUT")?;
        let msgs = self.pull("MESSAGES")?;
        Ok((out, msgs))
    }

    fn pull(&mut self, global: &str) -> Result<Vec<String>, PostError> {
        self.eval("_flushLine();")?; // any open line is program text, not lost
        let v = self.eval(global)?;
        let j = v
            .to_json(&mut self.ctx)
            .map_err(|e| PostError::Script(format!("{e}")))?
            .unwrap_or(Value::Array(vec![]));
        Ok(match j {
            Value::Array(items) => items
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect(),
            _ => Vec::new(),
        })
    }
}

/// A finite f64 as JS literal text; a non-finite one is the driver's
/// bug, surfaced as zero rather than as invalid JS (`NaN` parses, but
/// `null` is the honest CAM-side shape for "no value").
fn lit(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "null".into()
    }
}

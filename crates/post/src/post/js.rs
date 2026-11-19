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

pub use super::js_host::{is_false, is_true, ProgramId, HOOKS, PRELUDE};

/// Maximum emitted lines in one post run, across all hooks.
pub const MAX_OUTPUT_LINES: usize = 100_000;

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
    /// The `continuousThreading = true` manifest flag (issue #419): the
    /// script posts a thread's passes as one continuous synchronized
    /// chain per pass, chip-break dips included, on a control whose
    /// manual states that consecutive threading blocks keep sync.
    /// Without it a `PARAM ChipBreakDips` section is refused by name.
    continuous_threading: bool,
    /// The `millSpindleReverse = true` manifest flag (#988): the script
    /// reads `PARAM MillSpindle reverse` and starts the spindle M04 for
    /// a left-hand-cut mill. Without it such a section is refused.
    mill_spindle_reverse: bool,
    /// The `wireEdm = true` manifest flag (#533): the script voices a
    /// wire EDM section (`PARAM EdmCondition`). Without it such a
    /// section is refused by name - a cutting post would write a wire
    /// path with no condition, threading or wire words.
    wire_edm: bool,
    /// Latches once any output-limit check trips so caught exceptions cannot pass truncated output.
    output_limit_exceeded: bool,
    /// The `wcsRotation = true` manifest flag (#760): the script writes
    /// controller coordinate rotation (`G68 X0. Y0. R` / `G69` or
    /// `ROT RPL=` / `ROT`) when requested in `Control` mode.
    wcs_rotation: bool,
}

impl Script {
    /// Load the prelude plus one `.cps` text. A syntax error is a clean
    /// refusal, never a half-defined script.
    pub fn load(cps: &str) -> Result<Self, PostError> {
        let mut ctx = Context::default();
        // Apply the same Boa limits on desktop and wasm, including top-level code.
        let limits = ctx.runtime_limits_mut();
        limits.set_loop_iteration_limit(1_000_000);
        limits.set_recursion_limit(64);
        limits.set_stack_size_limit(4096);
        let host = format!(
            "var _maxOutputLines = {MAX_OUTPUT_LINES};\n{PRELUDE}\n{}\n{}",
            super::rotary_cap::ROTARY_CAP,
            super::explain::EXPLAIN_JS
        );
        ctx.eval(Source::from_bytes(host.as_bytes()).with_path(std::path::Path::new("host.js")))
            .map_err(|e| PostError::Script(format!("{e}")))?;
        // A separate source keeps user diagnostics exactly 1-based, without offsets.
        ctx.eval(Source::from_bytes(cps.as_bytes()).with_path(std::path::Path::new("user.cps")))
            .map_err(|e| PostError::Script(format!("{e}")))?;
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
            continuous_threading: false,
            mill_spindle_reverse: false,
            wire_edm: false,
            output_limit_exceeded: false,
            wcs_rotation: false,
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
        s.mill_spindle_reverse = s
            .eval("typeof millSpindleReverse !== \"undefined\" && millSpindleReverse === true")
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
        s.continuous_threading = s
            .eval("typeof continuousThreading !== \"undefined\" && continuousThreading === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        s.wire_edm = s
            .eval("typeof wireEdm !== \"undefined\" && wireEdm === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        super::js_host::check_output_limit(&mut s.ctx, &mut s.output_limit_exceeded)?;
        s.wcs_rotation = s
            .eval("typeof wcsRotation !== \"undefined\" && wcsRotation === true")
            .map(|v| is_true(&v))
            .unwrap_or(false);
        Ok(s)
    }

    fn has(&self, name: &str) -> bool {
        self.present.iter().any(|(h, yes)| *h == name && *yes)
    }

    /// Evaluate a manifest read (#931, `work_offset`) in the script's
    /// context.
    pub(super) fn eval_src(&mut self, src: &str) -> Result<JsValue, PostError> {
        self.eval(src)
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

    /// Publish the program's identity (#258) before `onOpen` reads it:
    /// the `program` global and `programNumber` beside it.
    pub fn set_program(&mut self, id: &ProgramId) -> Result<(), PostError> {
        let obj = serde_json::json!({ "number": id.number, "numbered": id.numbered, "comment": id.comment });
        self.eval(&format!("program = {obj}; programNumber = {};", id.number))?;
        Ok(())
    }

    /// A top-level string the script states (a manifest entry such as
    /// `extension`), or `None` when it states none.
    pub fn string_global(&mut self, name: &str) -> Result<Option<String>, PostError> {
        let v = self.eval(&format!("(typeof {name} === \"string\") ? {name} : null"))?;
        Ok(v.as_string().map(|s| s.to_std_string_escaped()))
    }

    /// A top-level array of strings (or one string) the script states,
    /// e.g. the manifest's `serves`; `None` when it states none. Any
    /// non-string entry makes it `None` too (read as not stated is
    /// wrong - it is returned as an error by the caller's word check),
    /// so entries are stringified and the caller validates the words.
    pub fn words_global(&mut self, name: &str) -> Result<Option<Vec<String>>, PostError> {
        let v = self.eval(&format!(
            "(typeof {name} === \"undefined\") ? null : (Array.isArray({name}) ? JSON.stringify({name}.map(function (w) {{ return String(w); }})) : \
             ((typeof {name} === \"string\") ? JSON.stringify([{name}]) : null))"
        ))?;
        let Some(text) = v.as_string().map(|s| s.to_std_string_escaped()) else {
            return Ok(None);
        };
        serde_json::from_str::<Vec<String>>(&text)
            .map(Some)
            .map_err(|e| PostError::Script(format!("{name}: {e}")))
    }

    /// A top-level number the script states (the manifest's
    /// `capabilities` mask), or `None` when it states none.
    pub fn number_global(&mut self, name: &str) -> Result<Option<f64>, PostError> {
        let v = self.eval(&format!("(typeof {name} === \"number\") ? {name} : null"))?;
        Ok(v.as_number())
    }

    /// Arm `explain()` (#1386) with the manifest's comment form; until
    /// armed the helper says nothing.
    pub fn arm_explain(&mut self, f: super::explain::Form) -> Result<(), PostError> {
        self.eval(&format!("_explainForm = {{ semi: {}, max: {} }};", f.semi, f.max))?;
        Ok(())
    }

    /// Why the script's own word meanings (#1399) cannot be used, or
    /// `None` when they can (or it states none).
    pub fn explain_refusal(&mut self) -> Result<Option<String>, PostError> {
        let v = self.eval("_explainCheck()")?;
        Ok(v.as_string().map(|s| s.to_std_string_escaped()).filter(|s| !s.is_empty()))
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
        super::js_host::check_output_limit(&mut self.ctx, &mut self.output_limit_exceeded)?;
        if !self.has(name) {
            return Ok(JsValue::undefined());
        }
        let res = self.eval(&format!("{name}({args})"));
        super::js_host::check_output_limit(&mut self.ctx, &mut self.output_limit_exceeded)?;
        res
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

    /// True when the script declares `millSpindleReverse = true` (#988).
    pub fn voices_mill_spindle_reverse(&self) -> bool {
        self.mill_spindle_reverse
    }

    /// True when the script declares `wireEdm = true` - it voices a
    /// wire EDM section (#533).
    pub fn voices_wire_edm(&self) -> bool {
        self.wire_edm
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

    /// True when the script declares `continuousThreading = true` - it
    /// posts chip-break dips inside one G32 chain (issue #419).
    pub fn voices_continuous_threading(&self) -> bool {
        self.continuous_threading
    }

    /// True when the script declares `wcsRotation = true` - it writes
    /// controller coordinate rotation (`G68` / `ROT`) in `Control` mode (#760).
    #[must_use]
    pub fn voices_wcs_rotation(&self) -> bool {
        self.wcs_rotation
    }

    /// Lines flushed so far (the driver's section boundary marker).
    /// Open partial lines count as flushed: a hook that ends a record
    /// mid-line still wrote a line of the program.
    pub fn out_len(&mut self) -> Result<usize, PostError> {
        super::js_host::check_output_limit(&mut self.ctx, &mut self.output_limit_exceeded)?;
        let v = self.eval("(_flushLine(), OUT.length)")?;
        let len = v.as_number().map(|n| n.max(0.0) as usize).unwrap_or(0);
        if len > MAX_OUTPUT_LINES {
            self.output_limit_exceeded = true;
            return Err(PostError::Script(super::js_host::OUTPUT_LIMIT_ERROR.into()));
        }
        Ok(len)
    }

    /// Pull the finished line buffer and the `message()` log.
    pub fn drain(&mut self) -> Result<(Vec<String>, Vec<String>), PostError> {
        self.out_len()?;
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

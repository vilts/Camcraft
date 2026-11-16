//! The post driver: records in, hooks out, lines back.
//!
//! Provenance: the call order below is the lifecycle contract of the
//! published Fusion 360 post architecture (behavior only, per the
//! Clean-Room Protocol); this Rust is ours, and so is the intermediate
//! it reads. The control dialect lives in the script, never here.

use super::cnc::{self, Record};
use super::js::{is_false, ProgramId, Script};
use super::machine;
use super::PostError;
use crate::cl::MoveKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One program line, and which stream record wrote it (None for lines
/// outside any record - the header block a post emits in `onOpen`).
#[derive(Clone, Debug, PartialEq)]
pub struct LineInfo {
    pub line: usize,
    pub record: Option<usize>,
}

/// One toolpath record's stretch of program lines (M3.3 stage D,
/// post debugging): the block the trace pane selects and `--trace`
/// logs. `first..last` is half-open into the program's line numbers;
/// `record` is `None` for the header `onOpen` wrote and the footer
/// `onClose` wrote - lines no toolpath record owns.
#[derive(Clone, Debug)]
pub struct TraceBlock {
    pub record: Option<usize>,
    pub label: String,
    pub first: usize,
    pub last: usize,
}

/// The finished program plus the evidence a reviewer needs.
#[derive(Clone, Debug)]
pub struct PostOutput {
    /// The G-code, one line per entry, newline-terminated.
    pub program: String,
    /// Program line -> stream record, the viewer's sync map (the
    /// record's index doubles as the move index for motion records).
    pub line_map: Vec<LineInfo>,
    /// Block-by-block trace: every record that wrote lines (and every
    /// motion record `onCommand` swallowed, which writes none - loss
    /// stays visible, never silent). Records that legitimately emit
    /// nothing (a `PARAM` a post only stores) are absent, so the
    /// trace reads as program, not as the stream echoed back.
    pub blocks: Vec<TraceBlock>,
    /// `WARN` records plus every `message()` the script logged - the
    /// validation gate's counted warnings, in file order.
    pub warnings: Vec<String>,
    /// Records the script swallowed by returning false from
    /// `onCommand` (a post filtering coolant-only records is legal;
    /// silent loss is not - so it is counted).
    pub swallowed: usize,
}

/// Post a stream text with one script. The machine value is the raw
/// `.mch` JSON (flattened here); pass `Value::Null` for none.
pub fn run(cps: &str, mch: &Value, stream: &str) -> Result<PostOutput, PostError> {
    run_with(cps, mch, stream, &Value::Null)
}

/// The file extension the script's manifest states (`extension`), or
/// `nc` when it states none - what a program written by it is named.
pub fn extension_of(cps: &str) -> Result<String, PostError> {
    let mut s = Script::load(cps)?;
    Ok(s.string_global("extension")?.filter(|e| !e.is_empty()).unwrap_or_else(|| "nc".to_owned()))
}

/// [`run`] as program number `program` (#111): one file per operation
/// numbers its programs 1, 2, 3..., and a dialect with an O-word
/// writes it; everything else posts exactly as [`run`].
pub fn run_numbered(cps: &str, mch: &Value, stream: &str, program: u32) -> Result<PostOutput, PostError> {
    run_numbered_with(cps, mch, stream, &Value::Null, program)
}

/// [`run`] with the document's post options (issue #34): a JSON object
/// the script reads as the global `postOptions` - the user's choices
/// for this post (e.g. `{"Threading": "G76"}`). `Value::Null` (or any
/// non-object) leaves the PRELUDE's empty object: every default.
pub fn run_with(cps: &str, mch: &Value, stream: &str, options: &Value) -> Result<PostOutput, PostError> {
    run_numbered_with(cps, mch, stream, options, 1)
}

/// The whole seam: post options (issue #34) and the program number
/// (#111) together - what one file per operation posts each op with.
pub fn run_numbered_with(
    cps: &str,
    mch: &Value,
    stream: &str,
    options: &Value,
    program: u32,
) -> Result<PostOutput, PostError> {
    let id = ProgramId { number: program, ..ProgramId::default() };
    run_program_with(cps, mch, stream, options, &id)
}

/// The whole seam with the document's program identity (#258): the
/// number, whether it was stated, and the program comment - what the
/// script reads as its `program` global.
pub fn run_program_with(
    cps: &str,
    mch: &Value,
    stream: &str,
    options: &Value,
    id: &ProgramId,
) -> Result<PostOutput, PostError> {
    let records = super::normalize_program_order(cnc::parse(stream)?);
    let mut s = Script::load(cps)?;
    // #902: an inch document on a script that cannot write inch.
    if let Some(why) = super::units::refusal(&mut s, options)? {
        return Err(PostError::Refused(why));
    }
    let wo = super::work_offset::Manifest::read(&mut s);
    // #1386: Explain G-code on a script that states no comment form is
    // refused by name here, before any hook runs.
    let explain_form = super::explain::form(&mut s, options)?;
    // #374: an inverted machine axis - the one place the stream is
    // mirrored into the controller's frame, or refused by name.
    let records = super::invert::apply(records, mch, &s)?;
    s.set_machine(&machine::script_props(mch))?;
    // #542: whether the manifest's `serves` (#1266) names a sinker EDM -
    // the one script that may voice a sinker section (below).
    let serves_sinker = s
        .words_global("serves")?
        .is_some_and(|w| w.iter().any(|k| crate::machine::Kind::from_word(k) == Some(crate::machine::Kind::SinkerEdm)));
    s.set_program(id)?;
    if options.is_object() {
        s.set_options(options)?;
    }
    if let Some(f) = explain_form {
        s.arm_explain(f)?;
    }
    s.call("onOpen", "")?;
    let head = s.out_len()?;

    let mut warnings = Vec::new();
    let mut base: BTreeMap<String, Value> = BTreeMap::new();
    let mut pos = (0.0f64, 0.0f64, 0.0f64);
    let mut line_map: Vec<LineInfo> = Vec::new();
    let mut blocks: Vec<TraceBlock> = Vec::new();
    if head > 0 {
        blocks.push(TraceBlock { record: None, label: "header".into(), first: 0, last: head });
    }
    let mut swallowed = 0usize;
    // #174: the operations already refused for want of onProbeGuard -
    // one named refusal per operation, not one per positioning move.
    let mut unguarded: Vec<String> = Vec::new();
    // #671: the plasma header in force and the lit contour.
    let mut torch = super::cnc_torch::TorchLaw::default();

    for (i, r) in records.iter().enumerate() {
        let before = s.out_len()?;
        let mut swallow = false;
        match r {
            Record::Param { name, value } => 'param: {
                // #931: the work offset reaches the script as its number
                // (flagged) or its legacy word (1-6), or is refused by name.
                let translated;
                let value = if name == "WorkOffset" {
                    match wo.translate(value, &base) {
                        Ok(v) => {
                            translated = v;
                            &translated
                        }
                        Err(w) => {
                            warnings.push(w);
                            break 'param;
                        }
                    }
                } else {
                    value
                };
                base.insert(name.clone(), value.clone());
                match name.as_str() {
                    "StartX" => pos.0 = value.as_f64().unwrap_or(0.0),
                    "StartY" => pos.1 = value.as_f64().unwrap_or(0.0),
                    "StartZ" => pos.2 = value.as_f64().unwrap_or(0.0),
                    _ => {}
                }
                // Publish the running param set so `onParameter` (and
                // `onSpindleSpeed` just below) can actually read the
                // value the hook names.
                // A post that does not declare `coolantModes` would post an
                // Off / Mist / Through-tool ask as its own flood (#150):
                // refused by name, a counted warning, never silent.
                if name == "Coolant" && !s.reads_coolant() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}asks {} coolant, but this post script does not declare coolantModes (it does not read PARAM Coolant) - set the operation's coolant to Flood or use a post that reads it",
                        value.as_str().unwrap_or("non-flood")
                    ));
                }
                // A post that does not declare `afterModes` would run
                // straight on past an operator's stop or trip home (#110):
                // refused by name, a counted warning, never silent.
                // #1532: the stop of a setup change (its PARAM SetupChange
                // rides just ahead of the AfterOperation ask) is refused
                // in the re-clamp's own words, once, not as an op's stop.
                let setup_change = i > 0
                    && matches!(records.get(i - 1), Some(Record::Param { name: n, .. }) if n == "SetupChange");
                if name == "SetupChange" && !s.reads_after_operation() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}the program changes to setup '{}' after it, and this post script does not declare afterModes, so it cannot stop for the re-clamp - choose Run straight on or One file per setup under Between setups, or use a post that reads it",
                        value.as_str().unwrap_or("")
                    ));
                }
                if name == "AfterOperation" && !s.reads_after_operation() && !setup_change {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}asks {} after it, but this post script does not declare afterModes (it does not read PARAM AfterOperation) - set the operation to Continue or use a post that reads it",
                        super::AfterOperation::describe_word(value.as_str().unwrap_or(""))
                    ));
                }
                // A C-axis live-tool section (issue #33) posted through a
                // script that cannot voice it would come out as XYZ for
                // a tool that is not there: refused by name.
                if name == "LiveTool" && !s.voices_live_tooling() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}is C-axis live-tool work (the spindle turns as C under a radial live tool), but this post script does not declare liveTooling - use a lathe post that voices the C axis"
                    ));
                }
                // A wire EDM section (#533) through a script that does not
                // voice one would post the wire path as a cutting move.
                if name == "EdmCondition" && !s.voices_wire_edm() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!("{who}is wire EDM work, but this post script does not declare wireEdm - use a wire EDM post"));
                }
                // A sinker burn (#542) through a script that does not
                // serve a sinker would post the burn as a cutting plunge.
                if name == "SinkerCondition" && !serves_sinker {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}is sinker EDM work, but this post script does not serve a sinker EDM (serves = [\"sinker_edm\"]) - use a sinker EDM post"
                    ));
                }
                // A thread whose hand needs the spindle reversed (#629),
                // posted through a script that cannot voice it, would
                // come out under M03 - the other hand: refused by name.
                if name == "SpindleDirection" && value.as_str() == Some("reverse") && !s.voices_thread_hands() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}cuts a thread that needs the spindle reversed (M04) for its hand, but this post script does not declare threadHands - it would cut the other hand; use a lathe post that states the thread's hand"
                    ));
                }
                // A left-hand-cut mill (#988) through a script that
                // cannot start M04 would turn the cutter backwards.
                if name == "MillSpindle" && value.as_str() == Some("reverse") && !s.voices_mill_spindle_reverse() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    let who = if op.is_empty() { String::new() } else { format!("operation {op}: ") };
                    warnings.push(format!(
                        "{who}runs a left-hand-cut mill that needs the spindle reversed (M04), but this post script does not declare millSpindleReverse - it would turn the cutter backwards"
                    ));
                }
                s.set_properties(&merged(&base, &BTreeMap::new(), pos))?;
                s.call("onParameter", &format!("{}, {}", json!(name), value))?;
                if name == "SpindleSpeed" {
                    s.call("onSpindleSpeed", "")?;
                }
            }
            Record::Warn { text } => {
                warnings.push(text.clone());
                // The comment contract (M3.3 A): a warning is also
                // commentary the script may want to voice. The
                // PRELUDE's `onComment` default echoes it verbatim -
                // and the echo only ever lands in a REFUSED output,
                // because the warning count that blocks the write
                // (`posting::post_setup`) reads the same list this
                // arm grows. So the released program is byte-for-byte
                // what it was; a script that overrides `onComment`
                // can restyle or silence its comments, but the data
                // is never withheld from it.
                s.call("onComment", &json!(text).to_string())?;
            }
            Record::Tool { number, name } => {
                base.insert("ToolNumber".into(), json!(*number));
                base.insert("ToolName".into(), json!(name));
                s.set_properties(&merged(&base, &BTreeMap::new(), pos))?;
                s.call("onToolChange", &format!("{}, {}", json!(number), json!(name)))?;
            }
            Record::Section { begin } => {
                s.call(if *begin { "onSection" } else { "onSectionEnd" }, "")?;
                if !*begin {
                    torch.section_end(&mut base, &mut warnings);
                }
            }
            Record::Plasma(p) => {
                // The plasma header (#671): its properties hold for the
                // section; `onPlasma` may read them at once.
                torch.header(&s, &base, &mut warnings);
                base.extend(p.props());
                s.set_properties(&merged(&base, &BTreeMap::new(), pos))?;
                s.call("onPlasma", "")?;
            }
            Record::Torch(t) => {
                // A lifecycle record like COMP: `onCommand` never sees
                // it, and the law refuses what the stream cannot mean.
                if torch.torch(t, &base, &mut warnings) && s.hook_exists("onTorchOn") {
                    s.set_properties(&merged(&base, &t.props(), pos))?;
                    s.set_current(pos.0, pos.1, pos.2)?;
                    super::cnc_torch::fire(&mut s, t)?;
                }
            }
            Record::Move(m) => {
                if m.kind == MoveKind::Rapid {
                    torch.rapid(&base, &mut warnings);
                }
                let p = m.props();
                s.set_properties(&merged(&base, &p, pos))?;
                s.set_current(pos.0, pos.1, pos.2)?;
                let cmd = p.get("command").cloned().unwrap_or(Value::Null);
                if is_false(&s.call("onCommand", &cmd.to_string())?) {
                    swallowed += 1;
                    swallow = true;
                } else {
                    let hook = if m.center.is_some() {
                        "onCircular"
                    } else if m.kind == MoveKind::Rapid {
                        "onRapid"
                    } else if m.kind == MoveKind::Plunge && s.hook_exists("onPlunge") {
                        "onPlunge"
                    } else {
                        "onLinear"
                    };
                    s.call(hook, "")?;
                }
                pos = (m.x, m.y, m.z);
            }
            Record::Cycle(c) => {
                if c.kind == crate::cl::CycleKind::FloatingTap && !s.voices_floating_tapping() {
                    let op = base.get("OperationName").and_then(Value::as_str).unwrap_or("");
                    warnings.push(format!("operation {op}: floating-holder tapping is not supported by this post (floatingTapping is not declared)"));
                }
                let chain: Vec<_> = records[i + 1..].iter()
                    .take_while(|r| !matches!(r, Record::Cycle(_) | Record::Section { begin: false }))
                    .filter_map(|r| match r {
                        Record::Move(m) if m.kind != MoveKind::Rapid => Some(m),
                        _ => None,
                    }).collect();
                let mut props = c.props();
                props.insert("ChainFeedRate".into(), json!(chain.first().and_then(|m| m.feed)));
                props.insert("ChainBottomZ".into(), json!(chain.iter().map(|m| m.z).reduce(f64::min)));
                s.set_properties(&merged(&base, &props, pos))?;
                s.set_current(pos.0, pos.1, pos.2)?;
                if is_false(&s.call("onCommand", &json!("Drilling").to_string())?) {
                    swallowed += 1;
                    swallow = true;
                } else {
                    s.call("onCycle", "")?;
                    s.call("onCyclePoint", "")?;
                    if c.dwell > 0.0 {
                        s.call("onDwell", &json!(c.dwell).to_string())?;
                    }
                    s.call("onCycleEnd", "")?;
                }
                // The canned cycle retracts to its R plane; the chain
                // continues from there.
                pos = (c.x, c.y, c.r_z);
            }
            Record::Part { loading, advance } => {
                // A lifecycle event, like SECTION: it commands no
                // motion, so `onCommand` never sees it - the script's
                // onPartLoading/onPartUnloading is the whole contract,
                // and the PRELUDE's defaults voice the intent as a
                // comment so a program from a bare script still says
                // what it asks the machine to do.
                let mut p = BTreeMap::new();
                p.insert(
                    "command".into(),
                    json!(if *loading { "PartLoading" } else { "PartUnloading" }),
                );
                if *loading {
                    p.insert("Advance".into(), json!(advance));
                }
                s.set_properties(&merged(&base, &p, pos))?;
                s.call(if *loading { "onPartLoading" } else { "onPartUnloading" }, "")?;
            }
            Record::Transfer { from, to } => {
                // The handoff step (M5.3 C): same lifecycle law as
                // the feeder records - no motion, so `onCommand`
                // never sees it, and the script's onPartTransfer is
                // the whole contract. The stations ride as From/To
                // so a dialect can name its own handshake; the
                // engine invents nothing.
                let mut p = BTreeMap::new();
                p.insert("command".into(), json!("PartTransfer"));
                p.insert("From".into(), json!(from));
                p.insert("To".into(), json!(to));
                s.set_properties(&merged(&base, &p, pos))?;
                s.call("onPartTransfer", "")?;
            }
            Record::Comp { side, radius_mm, wear } => {
                // Cutter compensation (M13.11): a lifecycle record like
                // its siblings - no motion, `onCommand` never sees it -
                // but NOT comment-only by default: the records after it
                // are the part contour, and a script that cannot say
                // G41/G42 would post them as the centre path - an
                // undersize part. So a script without the hook is
                // refused here, by name, as a counted warning.
                if !s.hook_exists("onRadiusCompensation") {
                    warnings.push(
                        "control cutter compensation: this post script does not define \
                         onRadiusCompensation - it cannot write G41/G42, so the program is refused"
                            .into(),
                    );
                } else {
                    let mut p = BTreeMap::new();
                    p.insert("command".into(), json!("RadiusCompensation"));
                    let word = match side {
                        Some(crate::cl::CompSide::Left) => "left",
                        Some(crate::cl::CompSide::Right) => "right",
                        None => "off",
                    };
                    p.insert("Compensation".into(), json!(word));
                    p.insert("CompensationRadius".into(), json!(radius_mm));
                    // #38: "wear" - the records between are the centre
                    // path and the register holds the wear delta only;
                    // "control" - they are the part contour.
                    p.insert("CompensationType".into(), json!(if *wear { "wear" } else { "control" }));
                    s.set_properties(&merged(&base, &p, pos))?;
                    s.call("onRadiusCompensation", &json!(word).to_string())?;
                }
            }
            Record::Touch(t) => {
                // A probing move (#108): no PRELUDE default - a script
                // that cannot write a probing block is refused by name,
                // because the only fallback (a feed) drives the stylus
                // through the part.
                if !s.hook_exists("onProbeMove") {
                    warnings.push(
                        "probing: this post script does not define onProbeMove - it cannot write a \
                         probing (skip) move, so the probe operation is refused"
                            .into(),
                    );
                } else {
                    s.set_properties(&merged(&base, &t.props(), pos))?;
                    s.call("onProbeMove", "")?;
                }
                // The machine stops where the stylus trips: nominally
                // the expected point the chain continues from.
                pos = (t.expect[0], t.expect[1], t.expect[2]);
            }
            Record::Retreat(r) => {
                if !s.hook_exists("onProbeRetreat") {
                    warnings.push(format!(
                        "probing: {}: this post script does not define onProbeRetreat - it cannot back \
                         off a probe trip incrementally, so the probe operation is refused",
                        op_name(&base)
                    ));
                } else {
                    s.set_properties(&merged(&base, &r.props(), pos))?;
                    s.call("onProbeRetreat", "")?;
                }
                pos = (r.expect[0], r.expect[1], r.expect[2]);
            }
            Record::Guard(g) => {
                // A protected positioning move (#174): no PRELUDE
                // default - a plain feed would drive the stylus into an
                // unexpected obstacle, so a script without the hook is
                // refused by name (once per operation).
                if s.hook_exists("onProbeGuard") {
                    s.set_properties(&merged(&base, &g.props(), pos))?;
                    s.call("onProbeGuard", "")?;
                } else {
                    let op = op_name(&base);
                    if !unguarded.contains(&op) {
                        warnings.push(format!(
                            "probing: {op}: this post script does not define onProbeGuard - it cannot write \
                             a protected (stop-on-contact) positioning move, so the probe operation is refused"
                        ));
                        unguarded.push(op);
                    }
                }
                pos = (g.to[0], g.to[1], g.to[2]);
            }
            Record::Found(f) => {
                if !s.hook_exists("onProbeResult") {
                    warnings.push(
                        "probing: this post script does not define onProbeResult - it cannot set a \
                         work offset or stop on a probing result, so the probe operation is refused"
                            .into(),
                    );
                } else {
                    s.set_properties(&merged(&base, &f.props(), pos))?;
                    s.call("onProbeResult", "")?;
                }
            }
            Record::Thread(t) => {
                // A threading cycle (issue #34): a lifecycle record like
                // COMP - no motion, `onCommand` never sees it. A script
                // without the hooks posts the explicit passes that
                // follow, exactly as before the record existed.
                match t {
                    Some(t) => {
                        s.set_properties(&merged(&base, &t.props(), pos))?;
                        s.set_current(pos.0, pos.1, pos.2)?;
                        s.call("onThreadCycle", "")?;
                    }
                    None => {
                        s.call("onThreadCycleEnd", "")?;
                    }
                }
            }
            Record::Probe { tol_mm } => {
                // The fourth lifecycle name (M5.3 D): the measurement
                // ask rides as a property, the same law as its
                // siblings - no motion, `onCommand` never sees it,
                // and onProbe is the whole contract. What protocol
                // the probe speaks is the script's dialect; the
                // engine only says that a measurement was asked for,
                // against what budget.
                let mut p = BTreeMap::new();
                p.insert("command".into(), json!("Probe"));
                p.insert("Tolerance".into(), json!(tol_mm));
                s.set_properties(&merged(&base, &p, pos))?;
                s.call("onProbe", "")?;
            }
        }
        let after = s.out_len()?;
        for line in before..after {
            line_map.push(LineInfo { line, record: Some(i) });
        }
        // A swallowed motion record writes nothing, and the trace
        // still shows it - the loss is the story.
        if after > before || swallow {
            let mut label = r.trace_label();
            if after == before {
                label.push_str(" (no output)");
            }
            blocks.push(TraceBlock { record: Some(i), label, first: before, last: after });
        }
    }

    torch.close(&mut base, &mut warnings);
    let before = s.out_len()?;
    s.call("onClose", "")?;
    let after = s.out_len()?;
    for line in before..after {
        line_map.push(LineInfo { line, record: None });
    }
    if after > before {
        blocks.push(TraceBlock { record: None, label: "end".into(), first: before, last: after });
    }

    let (out, msgs) = s.drain()?;
    warnings.extend(msgs);
    let mut program = String::new();
    for l in &out {
        program.push_str(l);
        program.push('\n');
    }
    Ok(PostOutput {
        program,
        line_map,
        blocks,
        warnings,
        swallowed,
    })
}

/// Params + this record's properties + the live position keys.
fn merged(
    base: &BTreeMap<String, Value>,
    rec: &BTreeMap<String, Value>,
    pos: (f64, f64, f64),
) -> BTreeMap<String, Value> {
    let mut m = base.clone();
    m.insert("StartX".into(), json!(pos.0));
    m.insert("StartY".into(), json!(pos.1));
    m.insert("StartZ".into(), json!(pos.2));
    for (k, v) in rec {
        m.insert(k.clone(), v.clone());
    }
    m
}

/// The operation a record belongs to, by the section's `OperationName`
/// PARAM - what a probing refusal names (#108).
fn op_name(base: &BTreeMap<String, Value>) -> String {
    base.get("OperationName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or_else(|| "the probe operation".to_string(), |s| format!("operation {s:?}"))
}

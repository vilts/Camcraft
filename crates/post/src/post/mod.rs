//! Post processing: the engine core of our CPS architecture.
//!
//! Provenance: three file roles and a lifecycle-hook architecture
//! specced from Autodesk's published post-processor documentation
//! (behavior only, never text, per the Clean-Room Protocol) - the
//! `.cps` script, the `.mch` machine, and our own intermediate, all
//! consumed by this Rust engine (a from-scratch boa_engine host; no
//! vendor binary or code is involved). Formats:
//! `docs/CNC-INTERMEDIATE.md`.
//!
//! Hook coverage, honestly stated (M3.3 A): the driver calls
//! `onParameter`+`onSpindleSpeed` for `PARAM`s (the spindle word is
//! itself one), `onToolChange` for `TOOL`, `onSection`/`onSectionEnd`
//! for `SECTION`, `onCommand` then the matching motion hook for every
//! `MOVE` (an `onCommand` returning false swallows the move - the
//! `swallowed` counter in `PostOutput` keeps that visible, never
//! silent), the cycle family for `CYCLE` points, and `onComment` for
//! every `WARN` (the default echoes; a script may restyle it - and
//! only refused outputs ever carry an echo, since warnings block the
//! write upstream). `onRadiusCompensation` (M13.11) fires for every
//! `COMP` record - the engage (left / right, with the register's
//! radius) before the lead-in of a control-compensated span and the
//! cancel before its lead-out; the motion records between are the
//! PART contour the span carries (`cl::CompSpan`), never the cutter
//! centre. A script without the hook is refused by the engine when a
//! `COMP` record arrives - the one way a compensated stream could post
//! as an undersize part.

mod after_op;
pub mod cnc;
pub mod cnc_thread;
pub mod cnc_torch;
mod coolant;
pub mod dialect;
pub mod engine;
pub mod invert;
pub mod js;
pub mod machine;
pub mod nose_tip;
pub mod probe_rec;
mod program;

pub use after_op::AfterOperation;
pub use coolant::CoolantMode;
pub use engine::{LineInfo, PostOutput, TraceBlock};

use crate::cl::{MoveKind, Toolpath};
use cnc::{CycleRec, MoveRec, Record};
use serde_json::{json, Value};

/// Engine failures, all of them clean refusals.
#[derive(Debug, thiserror::Error)]
pub enum PostError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("intermediate: {0}")]
    Stream(#[from] cnc::CncError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("post script: {0}")]
    Script(String),
    /// A refusal by name before any script runs (#374: an inverted axis
    /// the post cannot translate). Nothing is posted.
    #[error("{0}")]
    Refused(String),
}

/// Post a stream text with one script (the CLI's whole job, and the
/// in-app gate's core call).
pub fn post(cps: &str, mch: &Value, stream: &str) -> Result<PostOutput, PostError> {
    engine::run(cps, mch, stream)
}

/// Everything a `Toolpath` needs alongside it to become a stream -
/// the op's resolved context. Built by callers from the world (tool
/// library rank, resolved heights, WCS slot); pure data here, so the
/// dump and its tests never touch ECS.
#[derive(Clone, Debug)]
pub struct Dump<'a> {
    pub op_name: &'a str,
    /// 1-based rank among named tools (our tool-number law).
    pub tool_number: u32,
    pub tool_name: &'a str,
    pub tool_diameter: f64,
    /// Commentary only (#257): the op's kind, its setup's name, the
    /// tool's kind and corner radius - read by the scripts' program
    /// comments, never by motion. Empty / 0 writes no PARAM.
    pub op_kind: &'a str,
    pub setup_name: &'a str,
    pub tool_kind: &'a str,
    pub tool_corner_r: f64,
    /// Commentary too (#630): the lathe tool orientation the section's
    /// moves were converted for ([`nose_tip`]) - the code the operator
    /// touches the tool off by. None writes no PARAM.
    pub tool_orientation: Option<u8>,
    pub rpm: f64,
    pub clearance_z: f64,
    pub retract_z: f64,
    pub top_z: f64,
    pub wcs: &'a str,
    pub origin: [f64; 3],
    /// Stock bounds in mm: left, right, front, back (XY), plus top Z.
    pub stock: [f64; 4],
    pub stock_top_z: f64,
    /// Verification warnings attached to the path (the gate's
    /// evidence, riding the stream).
    pub warnings: Vec<String>,
    /// Bar-feeder signals (M5.2 G.2): `part_load` is Some(advance)
    /// when the op's setup feeds a new bar (the advance stands it
    /// out that many mm; 0 is the act alone), and `part_unload`
    /// asks the catcher for the severed part. They bracket the
    /// section - the machine events a parting op exists to make -
    /// and ride no geometry, so a toolpath with them set still
    /// simulates and verifies exactly as without.
    pub part_load: Option<f64>,
    pub part_unload: bool,
    /// The handoff step (M5.3 C): true asks the machine to move the
    /// severed part from its spindle to the second one - the stream
    /// carries `PART TRANSFER 1 2`, intent again, no geometry, and
    /// the program gate (not the post) says whether the machine
    /// states a second spindle at all.
    pub part_transfer: bool,
    /// The measurement ask (M5.3 D): Some(tol) is the deviation
    /// budget the op's author wrote for probing this part - the
    /// stream carries `PROBE <µm>`, intent again (which protocol
    /// measures is the script's dialect). The stream only voices the
    /// ask: the verdict is the app's POST gate composing every
    /// blade leg into the final part and measuring that under this
    /// budget (`operation::probe_problems`) - no warning rides the
    /// cache for it. None is silence: the op asks no probe.
    pub probe_tol: Option<f64>,
    /// The cutting fluid the op asks for (#112). Flood is the stream's
    /// silent default; any other ask rides a `PARAM Coolant`.
    pub coolant: CoolantMode,
    /// What the machine does once this op is done (#110). Carrying on
    /// is the stream's silent default; any other choice rides a
    /// `PARAM AfterOperation`.
    pub after: AfterOperation,
    /// The probing result (#108): `Some` for a Probe op, whose path
    /// carries touches - the stream then closes the section with a
    /// `FOUND` record built from the touches. None for every cutting op.
    pub probe_result: Option<probe_rec::ProbeResult>,
    /// The op's smoothing ask (#616): `Some` only when the document
    /// turned smoothing on - the stream then carries `PARAM Smoothing
    /// <mm>` and `PARAM SmoothingMode finish|rough` before the section,
    /// and the dialect writes its own words (G64 P, G5.1, G187,
    /// CYCLE832, CYCL DEF 32). None writes nothing: every program a
    /// document posted before #616 is byte-identical.
    pub smoothing: Option<Smoothing>,
    /// Constant surface speed (#891): `Some` only for a turning op whose
    /// tool states turning data - the stream then carries `PARAM
    /// SurfaceSpeed <m/min>` (and `PARAM MaxSpindleSpeed <rpm>` when the
    /// tool states its own clamp) before the section, and a lathe post
    /// that speaks CSS writes G96 under its G50 / D clamp. None writes
    /// nothing: every program posted before #891 is byte-identical.
    pub css: Option<Css>,
}

/// One turning op's constant-surface-speed ask (#891).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Css {
    /// Surface speed, m/min.
    pub vc: f64,
    /// The tool's own rpm ceiling; the post takes the lower of it and
    /// the machine's.
    pub max_rpm: Option<f64>,
}

/// One operation's smoothing ask (#616): the contour deviation the
/// control may take, mm, and whether the op FINISHES (maximum
/// accuracy - the control's precise mode) or ROUGHS (the deviation
/// derived from the stock the op leaves - its aggressive mode).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smoothing {
    pub tol_mm: f64,
    pub finish: bool,
}

impl<'a> Default for Dump<'a> {
    fn default() -> Self {
        Self {
            op_name: "",
            tool_number: 0,
            tool_name: "",
            tool_diameter: 0.0,
            op_kind: "",
            setup_name: "",
            tool_kind: "",
            tool_corner_r: 0.0,
            tool_orientation: None,
            rpm: 0.0,
            clearance_z: 0.0,
            retract_z: 0.0,
            top_z: 0.0,
            wcs: "G54",
            origin: [0.0; 3],
            stock: [0.0; 4],
            stock_top_z: 0.0,
            warnings: Vec::new(),
            part_load: None,
            part_unload: false,
            part_transfer: false,
            probe_tol: None,
            coolant: CoolantMode::Flood,
            after: AfterOperation::None,
            probe_result: None,
            smoothing: None,
            css: None,
        }
    }
}

/// The generator side: a toolpath plus its resolved context, as
/// records in driver order. Cycles are interleaved before their
/// group's first move, so the stream reads in program order whether
/// it came from here or from a legacy `.cl` golden (whose trailing
/// `; CYCLE` markers `engine::run` normalizes the same way).
#[must_use]
pub fn dump(t: &Toolpath, d: &Dump) -> Vec<Record> {
    let mut out = Vec::new();
    if let Some(p) = t.start() {
        out.push(Record::Param { name: "StartX".into(), value: json!(p.x) });
        out.push(Record::Param { name: "StartY".into(), value: json!(p.y) });
        out.push(Record::Param { name: "StartZ".into(), value: json!(p.z) });
    }
    out.push(Record::Param { name: "OperationName".into(), value: json!(d.op_name) });
    out.push(Record::Param { name: "ToolDiameter".into(), value: json!(d.tool_diameter) });
    comment_params(d, &mut out);
    out.push(Record::Param { name: "SpindleSpeed".into(), value: json!(d.rpm) });
    out.push(Record::Param { name: "WorkOffset".into(), value: json!(d.wcs) });
    out.push(Record::Param { name: "OriginX".into(), value: json!(d.origin[0]) });
    out.push(Record::Param { name: "OriginY".into(), value: json!(d.origin[1]) });
    out.push(Record::Param { name: "OriginZ".into(), value: json!(d.origin[2]) });
    out.push(Record::Param { name: "ClearancePlane".into(), value: json!(d.clearance_z) });
    out.push(Record::Param { name: "RetractPlane".into(), value: json!(d.retract_z) });
    out.push(Record::Param { name: "TopOfStock".into(), value: json!(d.top_z) });
    out.push(Record::Param { name: "StockXYLeft".into(), value: json!(d.stock[0]) });
    out.push(Record::Param { name: "StockXYRight".into(), value: json!(d.stock[1]) });
    out.push(Record::Param { name: "StockXYFront".into(), value: json!(d.stock[2]) });
    out.push(Record::Param { name: "StockXYBack".into(), value: json!(d.stock[3]) });
    out.push(Record::Param { name: "StockZTop".into(), value: json!(d.stock_top_z) });
    if let Some(word) = d.coolant.param_word() {
        out.push(Record::Param { name: "Coolant".into(), value: json!(word) });
    }
    if let Some(word) = d.after.param_word() {
        out.push(Record::Param { name: "AfterOperation".into(), value: json!(word) });
    }
    // #616: the smoothing ask, only when the document turned it on - a
    // finite positive deviation, or nothing (a script never sees a
    // tolerance it would have to guess about).
    if let Some(s) = d.smoothing.filter(|s| s.tol_mm.is_finite() && s.tol_mm > 0.0) {
        out.push(Record::Param { name: "Smoothing".into(), value: json!(s.tol_mm) });
        out.push(Record::Param {
            name: "SmoothingMode".into(),
            value: json!(if s.finish { "finish" } else { "rough" }),
        });
    }
    // A path stating spindle C angles (issue #33) is live-tool work on
    // a C turn: the section says so before it opens, so a script can
    // engage C and the live spindle in its preamble - and the engine
    // refuses, by name, a script that does not declare `liveTooling`.
    if dialect::states_live_tool(t) {
        out.push(Record::Param { name: "LiveTool".into(), value: json!("radial") });
    }
    // A thread whose passes need the spindle reversed (#629): the
    // record states hand and direction, and the tool lays its arcs
    // about the part only under M04. Said before the section opens,
    // so the spindle starts the right way; forward (M03) is silent.
    if t.threads.iter().any(|c| c.is_sane(&t.moves) && c.reverse_spindle()) {
        out.push(Record::Param { name: "SpindleDirection".into(), value: json!("reverse") });
    }
    // #891: the CSS ask, only a finite positive one (a script never sees
    // a speed it would have to guess about).
    if let Some(c) = d.css.filter(|c| c.vc.is_finite() && c.vc > 0.0) {
        out.push(Record::Param { name: "SurfaceSpeed".into(), value: json!(c.vc) });
        if let Some(m) = c.max_rpm.filter(|m| m.is_finite() && *m > 0.0) {
            out.push(Record::Param { name: "MaxSpindleSpeed".into(), value: json!(m) });
        }
    }
    for w in &d.warnings {
        out.push(Record::Warn { text: w.clone() });
    }
    if d.tool_number > 0 && !d.tool_name.is_empty() {
        out.push(Record::Tool { number: d.tool_number, name: d.tool_name.into() });
    }
    // The plasma header (#671) precedes the section, so a plasma post
    // knows the section it opens is torch work; a record that does not
    // sit on its moves is not trusted and not written - the post then
    // refuses the section by name for want of it.
    let plasma = match &t.plasma {
        Some(p) if p.is_sane(&t.moves) => {
            out.push(Record::Plasma(cnc_torch::PlasmaRec::from_cut(p)));
            Some(p)
        }
        Some(_) => {
            out.push(Record::Warn { text: "plasma: the torch spans do not sit on the moves - refused".into() });
            None
        }
        None => None,
    };
    out.push(Record::Section { begin: true });
    if let Some(advance) = d.part_load {
        out.push(Record::Part {
            loading: true,
            advance,
        });
    }

    // Hole groups lead with their cycle record: bucket cycles by the
    // move index they anchor at.
    let mut owned: Vec<CycleRec> = t
        .cycles
        .iter()
        .map(|c| CycleRec {
            kind: c.kind,
            x: c.xy.x,
            y: c.xy.y,
            r_z: c.r_z,
            bottom_z: c.bottom_z,
            entry_z: c.entry_z,
            peck: c.peck,
            dwell: c.dwell,
            first_move: c.first_move,
            feed: None,
        })
        .collect();
    owned.sort_by_key(|c| c.first_move);
    // Each cycle's cutting feed is the first cutting move of its
    // group - the chain is the agreement-law witness (M1.6 law A).
    for ci in 0..owned.len() {
        let start = owned[ci].first_move.min(t.moves.len());
        let end = cycles_end(t, &owned, ci).max(start);
        let feed = t
            .moves
            .get(start..end)
            .and_then(|s| s.iter().find(|m| m.kind != MoveKind::Rapid))
            .and_then(|m| m.feed);
        owned[ci].feed = feed;
    }
    let cycles: Vec<&CycleRec> = owned.iter().collect();

    let mut ci = 0usize;
    // Whether the machine stands tilted entering this move: the
    // letters are modal on the control, so a return to the Z column
    // after a lean must be COMMANDED (explicit zero angles), or the
    // machine runs the "upright" move under the old lean.
    let mut tilted = false;
    // Threading cycles (issue #34) bracket their passes: THREAD before
    // the span's first move, THREAD END after its last. A record that
    // does not sit on these moves is not trusted and not written - the
    // passes then post as themselves, and a script asked for the cycle
    // refuses by name for want of the record.
    let mut threads: Vec<&crate::cl::ThreadCycle> = t.threads.iter().filter(|c| c.is_sane(&t.moves)).collect();
    threads.sort_by_key(|c| c.first);
    let mut ti = 0usize;
    let mut open_thread: Option<usize> = None;
    // Probing touches (#108): a touch that does not sit on its move is
    // refused by name - its move would post as a plain feed.
    for p in &t.probes {
        if !p.is_sane(&t.moves) {
            out.push(Record::Warn {
                text: format!("probing: touch at move {} does not run toward its target - refused", p.index),
            });
        }
    }
    let touches: std::collections::BTreeMap<usize, &crate::cl::ProbeTouch> =
        t.probes.iter().filter(|p| p.is_sane(&t.moves)).map(|p| (p.index, p)).collect();
    if d.probe_result.is_some() && touches.is_empty() {
        out.push(Record::Warn { text: "probing: the probe path carries no touch - refused".into() });
    }
    let order = program::program_order(t, &mut out);
    let anchors: Vec<usize> = order.iter().map(|(mi, _, _)| *mi).collect();
    let mut lit_next = 0usize;
    for (k, (mi, m, pre)) in order.into_iter().enumerate() {
        // The torch (#671): struck before the first record of its span
        // (ahead of a G41/G42 engage, which rides the lead-in itself).
        if let Some(sp) = plasma.and_then(|p| p.torches.get(lit_next)).filter(|sp| sp.on <= mi && mi <= sp.off)
            && (k == 0 || anchors[k - 1] < sp.on)
        {
            out.push(Record::Torch(cnc_torch::TorchRec::on(sp)));
        }
        while ci < cycles.len() && cycles[ci].first_move <= mi {
            out.push(Record::Cycle(cycles[ci].clone()));
            ci += 1;
        }
        if open_thread.is_none()
            && let Some(c) = threads.get(ti).filter(|c| c.first == mi)
        {
            out.push(Record::Thread(Some(cnc_thread::ThreadRec::from_cycle(c))));
            open_thread = Some(c.last);
            ti += 1;
        }
        if let Some(r) = pre {
            out.push(r);
        }
        // The move straight back off a trip posts incrementally: a part
        // nearer than the model would otherwise be driven into by an
        // absolute back-off to the nominal point.
        if mi > 0
            && let (Some(prev), Some(f)) = (touches.get(&(mi - 1)), m.feed)
            && m.arc.is_none()
            && prev.direction(&t.moves).is_some_and(|d| {
                let v = m.to - m.from;
                let len = v.dot(v).sqrt();
                len > 1e-9 && (v.dot(d) / len + 1.0).abs() < 1e-6
            })
        {
            out.push(Record::Retreat(probe_rec::RetreatRec {
                delta: [m.to.x - m.from.x, m.to.y - m.from.y, m.to.z - m.from.z],
                expect: [m.to.x, m.to.y, m.to.z],
                feed: f,
            }));
            continue;
        }
        if let (Some(p), Some(f)) = (touches.get(&mi), m.feed) {
            out.push(Record::Touch(probe_rec::TouchRec {
                target: [p.target.x, p.target.y, p.target.z],
                expect: [m.to.x, m.to.y, m.to.z],
                feed: f,
                measure: p.measure,
            }));
            continue;
        }
        // The positioning feeds of a probe path (#174) are protected
        // moves: stop on an unexpected contact and alarm.
        if !touches.is_empty() {
            match probe_rec::guard_of(m) {
                Ok(Some(g)) => {
                    out.push(Record::Guard(g));
                    continue;
                }
                Ok(None) => {}
                Err(text) => out.push(Record::Warn { text }),
            }
        }
        // A C-axis live-tool move (issue #33) commands its stated
        // spindle angle and nothing else: the tool axis is the
        // station's, fixed in the machine, so A/B hold zero. The C law
        // (`cl::stamp_c`) is section-scoped - the post disengages C at
        // the section's end - so no modal lean is left standing.
        let abc = match (m.rotary, m.axis) {
            (Some(c), _) => {
                tilted = false;
                Some([0.0, 0.0, c])
            }
            (None, Some(a)) if !crate::cl::is_vertical(&a) => {
                tilted = true;
                Some(crate::cl::axis_to_angles(a))
            }
            (None, Some(_)) if tilted => {
                tilted = false;
                Some([0.0, 0.0, 0.0])
            }
            _ => None,
        };
        out.push(Record::Move(MoveRec {
            kind: m.kind,
            x: m.to.x,
            y: m.to.y,
            z: m.to.z,
            feed: m.feed,
            center: m.arc.map(|a| (a.center.x, a.center.y)),
            ccw: m.arc.map(|a| a.ccw).unwrap_or(false),
            plane: m.arc.map(|a| a.plane).unwrap_or_default(),
            // Pose (M5.1 stage A): a move that states a leaning axis
            // commands its canonical letter angles (the cl::pose law,
            // converted here once); a prismatic move - no axis, or
            // the Z column itself - commands nothing, byte for byte
            // as the stream always did - EXCEPT the first stated Z
            // column after a lean, which commands the zero angles
            // that take the machine back upright (the letters are
            // modal; silence would keep the lean). A move stating no
            // axis at all keeps the modal pose, as it always did.
            // Generators earn the `Some` side by set_axis; external
            // `.cnc` producers own the letters directly.
            abc,
        }));
        if open_thread == Some(mi) {
            out.push(Record::Thread(None));
            open_thread = None;
        }
        // ...and put out after the last record of its span (a control-
        // compensated span's programmed moves all anchor on its end).
        if let Some(sp) = plasma.and_then(|p| p.torches.get(lit_next))
            && mi <= sp.off
            && mi >= sp.on
            && anchors.get(k + 1).is_none_or(|n| *n > sp.off)
        {
            out.push(Record::Torch(cnc_torch::TorchRec::off(sp.contour)));
            lit_next += 1;
        }
    }
    if open_thread.is_some() {
        out.push(Record::Thread(None));
    }
    for c in &cycles[ci..] {
        out.push(Record::Cycle((*c).clone()));
    }
    // The choreography order (M5.3 C): the handoff comes after the
    // last cut - the part is severed, then it moves - and before the
    // unload, which then takes the part off the spindle now holding
    // it. A transfer with no load or unload is legal: the program's
    // story is only the handoff. The probe (M5.3 D) sits in front of
    // the handoff: the part is measured where it was last machined,
    // before it changes hands - LOAD, cut, PROBE, TRANSFER, UNLOAD.
    if let Some(r) = d.probe_result {
        let axes = probe_rec::found_axes(t);
        if axes.is_empty() {
            out.push(Record::Warn { text: "probing: no measuring touch finds an axis - refused".into() });
        } else {
            out.push(Record::Found(probe_rec::FoundRec { set_wcs: r.set_wcs, tol_mm: r.tol_mm, axes }));
        }
    }
    if let Some(t) = d.probe_tol {
        out.push(Record::Probe { tol_mm: t });
    }
    if d.part_transfer {
        out.push(Record::Transfer { from: 1, to: 2 });
    }
    if d.part_unload {
        out.push(Record::Part {
            loading: false,
            advance: 0.0,
        });
    }
    out.push(Record::Section { begin: false });
    out
}

/// The commentary PARAMs (#257): optional, each written only when
/// stated, so a stream without them reads exactly as before and a
/// script that does not know them ignores them.
fn comment_params(d: &Dump, out: &mut Vec<Record>) {
    let texts = [("OperationKind", d.op_kind), ("SetupName", d.setup_name), ("ToolKind", d.tool_kind)];
    for (name, v) in texts {
        if !v.is_empty() {
            out.push(Record::Param { name: name.into(), value: json!(v) });
        }
    }
    if d.tool_corner_r.is_finite() && d.tool_corner_r > 0.0 {
        out.push(Record::Param { name: "ToolCornerRadius".into(), value: json!(d.tool_corner_r) });
    }
    if let Some(q) = d.tool_orientation {
        out.push(Record::Param { name: "ToolOrientation".into(), value: json!(q) });
    }
}

/// The exclusive move index at which cycle `ci`'s chain group ends:
/// the next cycle's anchor, or the end of the chain.
fn cycles_end(t: &Toolpath, owned: &[CycleRec], ci: usize) -> usize {
    match owned.get(ci + 1) {
        Some(next) => next.first_move.min(t.moves.len()),
        None => t.moves.len(),
    }
}

/// Normalize any parsed stream to program order: move cycle records
/// to just before their `AT`-anchored move. Idempotent on `dump`
/// output; the fix for legacy goldens that append their markers.
#[must_use]
pub fn normalize_program_order(records: Vec<Record>) -> Vec<Record> {
    let has_cycles = records.iter().any(|r| matches!(r, Record::Cycle(_)));
    if !has_cycles {
        return records;
    }
    let mut cycles: Vec<CycleRec> = records
        .iter()
        .filter_map(|r| match r {
            Record::Cycle(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    cycles.sort_by_key(|c| c.first_move); // stable: holes keep stream order

    // Each span (SECTION pair) owns a half-open range of stream-global
    // move indices. A cycle belongs to the span owning its anchor index
    // - never to an earlier one, and never past a later boundary.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut moved = 0usize;
    let mut open: Option<usize> = None;
    for r in &records {
        match r {
            Record::Section { begin: true } => open = Some(moved),
            Record::Section { begin: false } => {
                if let Some(s) = open.take() {
                    spans.push((s, moved));
                }
            }
            Record::Move(_) => moved += 1,
            _ => {}
        }
    }
    let span_of = |t: usize| -> Option<usize> {
        spans
            .iter()
            .position(|(_, e)| t < *e)
            .or_else(|| {
                // Not inside any span's move range: the span that ends
                // last at-or-before the anchor (its trailing hole group),
                // else the first span, else none (a section-less legacy
                // stream seats purely at move boundaries).
                spans
                    .iter()
                    .rposition(|(_, e)| *e > 0)
                    .or_else(|| spans.first().map(|_| 0))
            })
    };

    let mut out = Vec::with_capacity(records.len());
    let mut mi = 0usize;
    let mut ci = 0usize;
    let mut si = 0usize; // span currently open at this point in the walk
    for r in &records {
        if matches!(r, Record::Cycle(_)) {
            continue;
        }
        if matches!(r, Record::Move(_)) {
            while ci < cycles.len() && cycles[ci].first_move <= mi {
                out.push(Record::Cycle(cycles[ci].clone()));
                ci += 1;
            }
            mi += 1;
        }
        // A span end seats every cycle that belongs to it but whose
        // anchor move does not exist inside it (a truncated or
        // hand-made stream): the cycle dies at its own section's
        // boundary, never re-seated into the next one - silent
        // cross-span relocation is loss.
        if matches!(r, Record::Section { begin: false }) {
            while ci < cycles.len() && span_of(cycles[ci].first_move) == Some(si) {
                out.push(Record::Cycle(cycles[ci].clone()));
                ci += 1;
            }
            si += 1;
        }
        out.push(r.clone());
    }
    for c in &cycles[ci..] {
        out.push(Record::Cycle(c.clone()));
    }
    out
}

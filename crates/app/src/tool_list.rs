//! Per-setup tool list: which cutters the program loads, how often, and
//! roughly how long each runs (M3.2 stage C).
//!
//! Provenance: our own design. A tool list / setup sheet is standard
//! shop paperwork - the list of cutters one clamping uses, in the order
//! they enter the spindle, with a change count and a cycle-time
//! estimate; the arithmetic is the general machine-tool handbook law
//! time = distance / rate. No HSMWorks documentation, output, or
//! implementation was consulted for this module.
//!
//! ## The laws here
//!
//! - **One order law, by call.** The walk is
//!   [`crate::setup::operation_order`] - the same sequence the post
//!   ([`crate::posting::program_ops`]), re-machining and the document
//!   sim use - sliced to one setup by the rank equality the ordering
//!   itself uses. A declared setup name that no longer exists ranks 0,
//!   exactly as [`crate::setup::rank`] decides, so the list and the
//!   post can never disagree about which program they are summarizing.
//! - **First-use order.** Rows appear in the order each tool first
//!   enters the spindle in that slice of the program - the order the
//!   magazine actually sees.
//! - **A change is a transition.** A row's `changes` counts the times
//!   the tool is LOADED: 1 for its first use, +1 for every switch back
//!   to it later. The first op of a slice always charges its tool;
//!   consecutive ops naming the same tool never do. The program roll-up
//!   applies the same law to the whole (setup-crossing) sequence: a
//!   setup boundary that keeps the same tool does not re-charge it -
//!   the tool is still sitting in the spindle. Per-setup lists
//!   therefore count each setup's first uses on their own, and the sum
//!   of per-setup changes can exceed the program's (stage E's sheet
//!   prints both, and says which is which).
//! - **The cache is the evidence.** Only operations with a cached
//!   path contribute lengths and times; a never-generated (or
//!   half-typed) operation still gets its entry - listed, zero cut,
//!   zero rapid - because the tool must be in the library for the
//!   program to run. A stale cached path is still on record and is
//!   still counted (this list is an estimate, not the post; the post's
//!   freshness gate is where the strict law lives).
//! - **Time, the same way the timeline does it.** Feed and plunge
//!   moves divide the exact arc-length-over-Z distance by the feed the
//!   move carries (mm/min); a feed move with no usable feed falls back
//!   to the rapid rate - never zero, never a hang, mirroring
//!   `sim::move_secs`. Rapids travel at [`RAPID_MM_S`]. Cycle dwells
//!   are the one non-motion time the representation carries (seconds
//!   at a hole bottom the move chain cannot express) and are added.
//!   Each change adds [`TOOL_CHANGE_S`] seconds to its tool's row, so
//!   summing the rows' minutes is the setup's estimate.
//! - **Raw numbers only.** No rounding here - floats are for the
//!   display layer to format; rounding now would bake a presentation
//!   choice into derived data.
//! - **Reader only.** Nothing here writes the world (AGENTS.md:
//!   systems and UI respond to commands; this is neither - it is a
//!   pure read, so it cannot drift an undoable field). Non-finite
//!   moves from a corrupt cache contribute nothing (their op is still
//!   listed), and a sum that would overflow reads as the largest
//!   finite double, never NaN or a panic.

use flecs_ecs::prelude::World;

use crate::cl::{Cycle, Move, MoveKind};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::model;
use crate::schema::Operation;
use crate::setup;

/// The G0 rate this estimate travels rapids at, mm/s. 6000 mm/min -
/// the shipped 3-axis VMC's XY rapid, the same number
/// [`crate::sim::Traverse::default`] stands in with when no machine
/// file speaks. The tool list is a paperwork estimate, not the
/// timeline: it deliberately does not read the document's `.mch`
/// (whose axis rapids are optional by the unstated-limit law), so one
/// published-typical rate stands for every program and stage E's
/// sheet can say so.
pub const RAPID_MM_S: f64 = 100.0;

/// The allowance charged per tool change, seconds. A general-shop
/// number for an automatic tool change on a small VMC - orient,
/// clear, swap, seat, and a margin for chips - well inside the
/// minutes-long change of a manual swap and longer than a clean
/// machine's tool-to-tool. Deliberately conservative: a list that
/// predicts the shift too long is annoying, one that predicts it too
/// short is a lie to whoever schedules against it. The setup-sheet
/// law (stage E) is the place a shop may want to override it.
pub const TOOL_CHANGE_S: f64 = 20.0;

/// One tool's line on a setup's (or the program's) tool list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolRow {
    /// The tool's entity name, as the operations carry it.
    pub tool: String,
    /// The operations in this slice that use it, in program order.
    pub ops: Vec<String>,
    /// Cutting travel (feed + plunge moves), mm, from the cached paths.
    pub cut_mm: f64,
    /// Rapid travel, mm, from the cached paths.
    pub rapid_mm: f64,
    /// Estimated run minutes: move times by the time law above,
    /// cycle dwells, plus [`TOOL_CHANGE_S`] per load of this tool.
    pub minutes: f64,
    /// Times this tool enters the spindle in this slice.
    pub changes: usize,
}

/// The tool list for one setup's slice of the program. `None` (and a
/// name that no longer wears a setup) means the first setup by name -
/// the implicit-setup law, read through the same
/// [`crate::setup::effective`] the ordering uses.
#[must_use]
pub fn for_setup(world: &World, setup_name: Option<&str>) -> Vec<ToolRow> {
    let order = setup::names(world);
    let want = setup::rank(&order, setup::effective(world, setup_name).as_deref());
    rows(world, move |op| setup::rank(&order, op.setup.as_deref()) == want)
}

/// The whole-program roll-up: every operation, setup boundaries
/// included, changes counted across the full program order.
#[must_use]
pub fn program(world: &World) -> Vec<ToolRow> {
    rows(world, |_: &Operation| true)
}

/// The setup's estimated minutes - the sum of its rows, tool-change
/// allowance included.
#[must_use]
pub fn setup_minutes(world: &World, setup_name: Option<&str>) -> f64 {
    let mut total = 0.0f64;
    for r in for_setup(world, setup_name) {
        total += r.minutes;
    }
    if total.is_finite() {
        total
    } else {
        f64::MAX
    }
}

/// The shared walk: program order, filtered to one slice, accumulated
/// into first-use-order rows. `keep` is the slice predicate (always
/// true for the program roll-up).
fn rows<F>(world: &World, keep: F) -> Vec<ToolRow>
where
    F: Fn(&Operation) -> bool,
{
    let mut out: Vec<ToolRow> = Vec::new();
    let mut prev_tool: Option<String> = None;
    for (name, e) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue; // is_operation said yes; a lost component skips, never panics
        };
        // A wire cut (#1110) mounts no library tool - its wire is the
        // machine's EDM condition, and a wire EDM has no tool table - so
        // it adds no row (never one for the empty tool name). Keyed on
        // the kind alone: a milling op whose tool is gone keeps its row.
        if !keep(&op) || !op.params.kind().info().mounts_tool() {
            continue;
        }
        let idx = match out.iter().position(|r| r.tool == op.tool) {
            Some(i) => i,
            None => {
                out.push(ToolRow {
                    tool: op.tool.clone(),
                    ..ToolRow::default()
                });
                out.len() - 1
            }
        };
        if prev_tool.as_deref() != Some(op.tool.as_str()) {
            out[idx].changes += 1;
        }
        prev_tool = Some(op.tool.clone());
        out[idx].ops.push(name);
        if let Some(cache) = model::toolpath_of(world, &e) {
            let (cut, rap, secs) = tally(&cache.path.moves, &cache.path.cycles);
            let row = &mut out[idx];
            row.cut_mm += cut;
            row.rapid_mm += rap;
            row.minutes += secs / 60.0;
        }
    }
    // The change allowance last, so the travel sums above never pay
    // for a row's ordering.
    for row in out.iter_mut() {
        row.minutes += row.changes as f64 * TOOL_CHANGE_S / 60.0;
        clamp_finite(&mut row.cut_mm);
        clamp_finite(&mut row.rapid_mm);
        clamp_finite(&mut row.minutes);
    }
    out
}

/// The per-move law of the module header: cutting and rapid travel in
/// mm, and the move/cycle time in seconds. Non-finite move lengths
/// contribute nothing (a corrupt cache reads as a shorter estimate
/// plus a listed op, never as NaN).
fn tally(moves: &[Move], cycles: &[Cycle]) -> (f64, f64, f64) {
    let mut cut = 0.0f64;
    let mut rap = 0.0f64;
    let mut secs = 0.0f64;
    for m in moves {
        let l = move_len(m);
        if !l.is_finite() || l <= 0.0 {
            continue;
        }
        match m.kind {
            MoveKind::Rapid => {
                rap += l;
                secs += l / RAPID_MM_S;
            }
            MoveKind::Feed | MoveKind::Plunge => {
                cut += l;
                match m.feed {
                    Some(f) if f.is_finite() && f > 0.0 => secs += 60.0 * l / f,
                    // No usable feed where a feed belongs: the rapid
                    // law, `sim::move_secs`' fallback, never zero.
                    _ => secs += l / RAPID_MM_S,
                }
            }
        }
    }
    for c in cycles {
        if c.dwell.is_finite() && c.dwell > 0.0 {
            secs += c.dwell;
        }
    }
    (cut, rap, secs)
}

/// A move's distance: exact arc length over Z, or the straight 3-D
/// diagonal - the same geometric law `sim::path_len` and the one
/// `Move::chords` flattens. Written here (not called from `sim`,
/// whose copy is private to the timeline) with the identical law so
/// the list and the timeline cannot disagree about what a move is.
fn move_len(m: &Move) -> f64 {
    let dz = m.to.z - m.from.z;
    let dxy = || {
        ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt()
    };
    match m.arc {
        None => (dxy().powi(2) + dz * dz).sqrt(),
        // XZ / YZ (M11.2 A): `cl`'s in-plane length - the timeline's.
        Some(arc) if !arc.plane.is_xy() => arc
            .length(m.from, m.to)
            .unwrap_or_else(|| (dxy().powi(2) + dz * dz).sqrt()),
        Some(arc) => {
            let (r, d) = arc_sweep(m.from, m.to, arc.center, arc.ccw);
            let xy = if d > 0.0 { r * d } else { dxy() };
            (xy * xy + dz * dz).sqrt()
        }
    }
}

/// The XY sweep of an arc about `o` in direction `ccw`, as
/// `(radius, sweep)`; a degenerate answer is `(0.0, 0.0)` -
/// `sim::arc_sweep`'s law.
fn arc_sweep(a: Point3, b: Point3, o: Point2, ccw: bool) -> (f64, f64) {
    let ra = ((a.x - o.x).powi(2) + (a.y - o.y).powi(2)).sqrt();
    let rb = ((b.x - o.x).powi(2) + (b.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = if ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * std::f64::consts::PI);
    if r.is_finite() && d.is_finite() && r > f64::EPSILON && d > 0.0 {
        (r, d)
    } else {
        (0.0, 0.0)
    }
}

/// A pathological (or merely astronomically long) program must read as
/// "very long", never as NaN or infinity leaking into a label.
fn clamp_finite(v: &mut f64) {
    if !v.is_finite() {
        *v = f64::MAX;
    }
}

//! Which spindle a lathe post turns for a section (#1190), and the door
//! that keeps a milling operation off the work spindle.
//!
//! A turning dialect (`fanuc-lathe.cps`, `tormach-lathe.cps`) opens a
//! section one of two ways: a section whose moves state a C angle
//! (`PARAM LiveTool`) starts the live tool and holds the work on C;
//! every other section starts the WORK spindle at the section's speed
//! (`G97 S M03`, or `G96`) and the tool stands still. The stream names
//! no operation family, so a milling path that happens to stay on one
//! meridian (a face pass along Y = 0) reads to the post exactly like a
//! turning pass - and would turn the work at the cutter's rpm while the
//! cutter stands still.
//!
//! The law, stated on what the program gate already holds (the
//! operation's kind, its cached path, its work offset): a section may
//! reach a turning dialect only as
//!
//! 1. live-tool work - its path states C ([`LatheVoice::LiveTool`]);
//! 2. a turning-family kind ([`LatheVoice::WorkSpindle`]);
//! 3. a Drill whose every cutting move and cycle stands on the spindle
//!    axis with the tool along +Z (drill, centre drill, tap, reamer on
//!    centre). There - and only there - it makes no difference to the
//!    cut whether the tool turns or the work does: the two share one
//!    axis, so the relative rotation is the same. That is how a lathe
//!    drills and taps on centre ([`LatheVoice::WorkSpindle`]).
//!
//! Everything else is refused by name before a line is posted: a cutter
//! that moves off the axis must itself turn (every milling kind, the
//! helical Bore, the thread mill), and a probe has no spindle at all.
//!
//! One derivation: [`voice`] is the only place that decides which
//! spindle a lathe post turns for an operation. The post door below and
//! any rule that asks "does this section turn the work?" (the
//! workholding rpm limit) read it - never a second copy.
//!
//! Provenance: our own rule. That a lathe drills and taps on centre
//! with the work turning and the tool held still, and that a milling
//! cutter must itself rotate, is general machining knowledge from the
//! machine-tool handbooks. No vendor material consulted.

use flecs_ecs::prelude::*;

use crate::cl::{MoveKind, Toolpath};
use crate::model;
use crate::operation::spindle_law::SPINDLE_TOL;
use crate::operation::{Family, OpParams};

/// What a turning dialect does with one operation's section.
#[derive(Clone, Debug, PartialEq)]
pub enum LatheVoice {
    /// The work spindle turns at the section's stated speed and the
    /// tool stands still: the turning family and the on-centre Drill.
    WorkSpindle,
    /// The path states C: the live tool turns, the work is held on C.
    LiveTool,
    /// Not a section a lathe post may open; the reason, in the words
    /// the refusal says after the operation's name.
    Refused(String),
}

/// What every refusal ends with: the two acts that fix it.
const FIX: &str = "move it to a setup that posts through a mill post, or suppress the operation";

/// The one derivation (#1190): what a turning dialect does with an
/// operation of these `params` whose cached path is `path`, posted
/// about the work offset at `origin` (world mm - the spindle axis is
/// world Z through it, the lathe posts' own frame).
#[must_use]
pub fn voice(params: &OpParams, path: &Toolpath, origin: [f64; 3]) -> LatheVoice {
    // The post's own order: a section stating C is live-tool work,
    // whatever its kind.
    if crate::post::dialect::states_live_tool(path) {
        return LatheVoice::LiveTool;
    }
    let kind = params.kind();
    if kind.info().family() == Family::Turn {
        return LatheVoice::WorkSpindle;
    }
    match params {
        OpParams::Drill { .. } => match off_centre(path, origin) {
            None => LatheVoice::WorkSpindle,
            Some(what) => LatheVoice::Refused(format!(
                "a Drill operation would be posted on the work spindle with {what} - a lathe post drills with the work \
                 turning and the drill standing still, which is only right for a hole on the spindle axis; keep the \
                 on-centre holes in an operation of their own and {FIX}"
            )),
        },
        OpParams::Probe { .. } => LatheVoice::Refused(format!(
            "a Probe operation cannot be posted through a lathe post - it would open the section on the work spindle \
             and writes no probing; {FIX}"
        )),
        OpParams::PlasmaProfile(_) => LatheVoice::Refused(
            "a Plasma Profile operation cannot be posted through a lathe post - it would open the section on the work \
             spindle, and a torch has none; post it through a plasma post, or suppress the operation"
                .to_owned(),
        ),
        _ => LatheVoice::Refused(format!(
            "a {kind} operation would be posted on the work spindle - a lathe post turns the work at the tool's speed \
             while the cutter stands still, so nothing is milled; cut it as C-axis live tooling (a rotary operation), \
             or {FIX}"
        )),
    }
}

/// Why a drilling path is not an on-centre hole along Z, or `None`
/// when it is: every cutting move and every cycle on the spindle axis
/// (within the posts' own lattice, [`SPINDLE_TOL`]) and no tilted
/// tool. Rapids are free - the tool may come in from the side in air.
fn off_centre(path: &Toolpath, origin: [f64; 3]) -> Option<String> {
    // A NaN is never on the axis: the comparison refuses it.
    let off = |x: f64, y: f64| !((x - origin[0]).abs() <= SPINDLE_TOL && (y - origin[1]).abs() <= SPINDLE_TOL);
    // A pose that is not along +Z (a NaN included) is a tilted tool.
    let upright = |a: [f64; 3]| a[2] >= 1.0 - 1e-9;
    if path.moves.iter().any(|m| m.axis.is_some_and(|a| !upright(a))) {
        return Some("a tilted drill (a cross hole, not along the spindle axis)".to_owned());
    }
    let at = |x: f64, y: f64| {
        format!("a hole off the spindle axis (X{:.3} Y{:.3} from the work offset)", x - origin[0], y - origin[1])
    };
    if let Some(c) = path.cycles.iter().find(|c| off(c.xy.x, c.xy.y)) {
        return Some(at(c.xy.x, c.xy.y));
    }
    path.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| [m.from, m.to])
        .find(|p| off(p.x, p.y))
        .map(|p| at(p.x, p.y))
}

/// [`voice`] for operation entity `e`, read from the document: its
/// kind, its cached path and its own setup's work offset. `None` for an
/// entity that is no operation or has no path yet (the program gate
/// names those).
#[must_use]
pub fn voice_of(world: &World, e: &Entity) -> Option<LatheVoice> {
    let op = model::operation_of(world, e)?;
    let cache = model::toolpath_of(world, e)?;
    let (_slot, origin) = crate::posting::wcs_context(world, op.setup.as_deref());
    Some(voice(&op.params, &cache.path, origin))
}

/// The shared predicate: is this operation's section voiced on the
/// WORK spindle by a turning dialect (the work turns at the section's
/// stated speed)? What the workholding rpm limit judges.
#[must_use]
pub fn on_work_spindle(world: &World, e: &Entity) -> bool {
    voice_of(world, e) == Some(LatheVoice::WorkSpindle)
}

/// The door at post: when `cps` is a turning dialect, refuse by name
/// the first of `ops` (operation entity names, program order) that a
/// lathe post may not open. A milling dialect passes untouched - there
/// `M03` is the tool's own spindle.
///
/// # Errors
/// `"<op>: a <kind> operation would be posted on the work spindle - ..."`,
/// or the script's own load failure.
pub(crate) fn at_post<'a>(world: &World, ops: impl IntoIterator<Item = &'a str>, cps: &str) -> Result<(), String> {
    if !crate::post::dialect::speaks_turning(cps).map_err(|e| format!("post engine: {e}"))? {
        return Ok(());
    }
    for name in ops {
        let Some(e) = model::find_by_name(world, name) else {
            continue;
        };
        if let Some(LatheVoice::Refused(why)) = voice_of(world, &e) {
            return Err(format!("{}: {why}", model::operation_label(world, name, &e)));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lathe_voice_tests.rs"]
mod tests;

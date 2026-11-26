//! The stated rotation-axis resolver (#2772, parts 1 and 2): the
//! lines a rotary kind may turn its work about, taken from
//! STATEMENTS - the operation's own stated direction first, else the
//! mounted machine's stated rotary lines - each anchored through the
//! setup's work offset origin (the indexed post's setup law: the WCS
//! sits on the rotary centreline, the same law the index-swing clause
//! reads). Nothing stated is `None`: today's behaviour, the free fit,
//! stands unchanged. A stated line that leans or is no direction
//! refuses by name, exactly as the swing clause refuses it - never
//! read as unstated, never judged as if level (#818's law).
//!
//! The JUDGE of a stated line against the model's faces is the motion
//! crate's (`multiaxis::axis_stated`): the fit's own concurrency,
//! coarseness and bearing gates about the stated line, with a
//! refusal that names the disagreement. This module only READS the
//! document: the params arm, the mounted machine, the work offset.
//!
//! Provenance: our own reading of our own `.mch` grammar
//! (`assets/machines/AGENTS.md`) and the indexed-post setup law of
//! machine-tool practice; no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::machine::AxisLimits;
use crate::machining2d::GenError;
use crate::multiaxis::axis::Axis;
use crate::multiaxis::axis_stated::canonical;
use crate::operation::OpParams;
use crate::posting::index_swing::{rotary_line, RotaryLine};

/// The operation's own stated rotation axis (`stated_axis`, None on
/// every other kind and every older file): a direction ratio, never
/// unit-scaled - the machine file's own law for its `axis` vector.
#[must_use]
pub fn op_stated(params: &OpParams) -> Option<[f64; 3]> {
    match params {
        OpParams::RotaryContour { stated_axis, .. }
        | OpParams::RotaryParallel { stated_axis, .. }
        | OpParams::RotaryOffset { stated_axis, .. }
        | OpParams::RotaryFlow { stated_axis, .. } => *stated_axis,
        _ => None,
    }
}

/// The stated lines of a rotary op, pure: the op's own direction
/// first, else every work-turning rotary's or the spindle C's stated
/// line (a stated `axis` vector, else the RS-274 letter's line - the
/// swing clause's own reading, `rotary_line`), each anchored through
/// `anchor` (the work offset's world position). `Some(Err)` is a
/// stated line that cannot be meant (it leans, or is no direction) -
/// refused by name, never read as unstated. `None` is NOTHING
/// STATED: today's free fit stands.
///
/// `rotaries` is the mounted machine's rotary inventory (`Machine::
/// rotary_axes` order, the kinematic chain); None is no machine.
#[must_use]
pub fn lines_of(
    op: Option<[f64; 3]>,
    rotaries: Option<&[AxisLimits]>,
    anchor: [f64; 3],
) -> Option<Result<Vec<(String, Axis)>, GenError>> {
    let anchor = crate::geometry::Point3::new(anchor[0], anchor[1], anchor[2]);
    // The operation's own statement wins over the machine's.
    if let Some(d) = op {
        return Some(match canonical(d, anchor) {
            Some(ax) => Ok(vec![("the operation's stated axis".into(), ax)]),
            None => Err(GenError::Invalid(format!(
                "the stated rotation axis ({}, {}, {}) is no direction - write it as three finite numbers, not all zero",
                d[0], d[1], d[2]
            ))),
        });
    }
    let mut out = Vec::new();
    for a in rotaries? {
        if !(a.turns_part == Some(true) || a.name.trim().eq_ignore_ascii_case("C")) {
            continue; // a head's own tilt turns the tool, not the work
        }
        match rotary_line(a) {
            RotaryLine::Horizontal(u) => out.push((a.name.clone(), canonical(u, anchor))),
            RotaryLine::Vertical => out.push((a.name.clone(), canonical([0.0, 0.0, 1.0], anchor))),
            // A stated vector that leans or is no direction: the swing
            // clause's own refusal, at the generation door too.
            RotaryLine::Leaning => {
                return Some(Err(GenError::Invalid(format!(
                    "the machine's rotary {} states an axis that leans or is no direction - the \
                     post cannot judge the work it swings, so the rotary kinds refuse it; state \
                     a level or a vertical line",
                    a.name
                ))));
            }
            // No vector and no RS-274 letter: nothing this resolver
            // may claim (the post's own door names it).
            RotaryLine::Unknown => {}
        }
    }
    // `rotary_line` hands out finite non-zero directions, so no
    // candidate is dropped here; the filter stands for the law.
    let out: Vec<(String, Axis)> = out.into_iter().filter_map(|(n, l)| l.map(|ax| (n, ax))).collect();
    (!out.is_empty()).then_some(Ok(out))
}

/// The stated lines of a rotary operation from the document: the
/// op's params, the setup's mounted machine, the setup's work offset
/// origin. See [`lines_of`] for the reading.
#[must_use]
pub fn axis_lines(
    world: &World,
    setup: Option<&str>,
    params: &OpParams,
) -> Option<Result<Vec<(String, Axis)>, GenError>> {
    let rotaries: Option<Vec<AxisLimits>> = crate::machine::of_setup(world, setup)
        .1
        .map(|m| m.axes.iter().filter(|a| a.rotary).cloned().collect());
    lines_of(
        op_stated(params),
        rotaries.as_deref(),
        crate::posting::wcs_context(world, setup).1,
    )
}

//! The thread-milling operation's app half (#894, external threads #985): the door that judges tool and thread against each other by name, the kernel plan, and the gate on the readings the gouge laws take of a thread mill - a round wall, the thread's own pitch, and on a boss the tooth roots.
//!
//! Provenance: ISO 724 / ISO 965-1 thread depths and public machine-tool
//! handbook thread milling, as `camcraft_tools::thread_mill` and
//! `machining2d::thread_mill` cite them; the door and the verdict
//! readings are our own design. No vendor material consulted.
//!
//! A thread is cut INTO the wall the model shows (CAD draws a tapped
//! hole at its minor diameter), so the tool's full cylinder against the
//! part would read every thread as a gouge. The two readings are #1071's
//! (`thread_rims`, `verify::thread_bore`): the tooth crests may reach the
//! major circle of the model's own wall and no further (the tangency law,
//! against `thread_rims::major_rims`), and the solid law excuses only the
//! thread's own wall and mouth, inside that major cylinder ([`bores`]).
//! Both apply only over a round wall ([`walls_round`], #1205). An
//! external thread (#985) mirrors it on a boss, which CAD draws at its
//! major diameter: the crests may reach in to the minor circle
//! ([`super::thread_boss`], a round boss only), and the roots stay off
//! the drawn wall ([`root_cutter`]); a boss gets no bore grant.

use camcraft_tools::thread_mill as form;

use crate::machining2d::{self, GenError, ThreadHole, ThreadParams};
use crate::tools::Tool;
use crate::geometry::Triangle;
use crate::verify::{self, Cutter, Warning};

use super::heights::ResolvedHeights;
use super::types::OpParams;

/// The thread's radial depth for `pitch`, mm: ISO 724's in a hole, the
/// ISO 965-1 design root on a boss.
fn depth_of(pitch: f64, external: bool) -> f64 {
    pitch * if external { machining2d::EXTERNAL_THREAD_DEPTH_PER_PITCH } else { machining2d::THREAD_DEPTH_PER_PITCH }
}

/// The door (#894): why `tool` cannot cut the thread `params` states, by
/// name, or `None`. Asked when an operation is created, edited or
/// re-tooled, and again at generate. `None` for another kind of
/// operation, for a tool that is no thread mill (the tool law's
/// family door names that one), and for a single-form mill whose pitch
/// is not stated yet (the generator names that one).
#[must_use]
pub fn door(tool: &Tool, params: &OpParams) -> Option<String> {
    let OpParams::Thread { pitch, infeed, holes, external, bottom_z, passes, top_z, .. } = params else {
        return None;
    };
    if !form::is_thread_mill(tool.kind) {
        return None;
    }
    if let Some(why) = form::form_refusal(tool) {
        return Some(why);
    }
    // What the features ask of the tool: air inside a hole; on a boss
    // (#985), a core under the thread and a mill that reaches its foot.
    let no_air = |depth: Option<f64>| {
        if *external {
            // A stated bottom is resolved at generate, which asks again.
            return super::thread_boss::door(tool, holes, depth, bottom_z.is_none().then_some(None));
        }
        holes.iter().find(|x| tool.diameter * 0.5 >= x.radius).map(|x| {
            format!(
                "a {} mm thread mill has no air to enter the {} mm drill hole at ({}, {}) - the mill must be smaller than the thread's minor diameter",
                tool.diameter,
                x.radius * 2.0,
                x.xy.x,
                x.xy.y
            )
        })
    };
    let pitch = match form::resolve_pitch(tool, *pitch) {
        Ok(p) => p,
        // A single-form mill with the pitch not stated yet is an
        // unfinished operation, not an impossible one: a new Thread
        // operation starts that way, and the generator refuses it by
        // name until the pitch is stated. Everything that needs no
        // pitch is still judged here.
        Err(_) if pitch.is_none() => {
            return passes
                .refusal(None, *infeed)
                .or_else(|| no_air(None))
                .or_else(|| super::thread_r_plane::door(*top_z, holes, *external));
        }
        Err(why) => return Some(why),
    };
    let h = depth_of(pitch, *external);
    if tool.diameter * 0.5 <= h {
        return Some(format!(
            "a {} mm thread mill is no wider than twice the {h} mm thread depth of a {pitch} mm pitch - its teeth leave no core",
            tool.diameter
        ));
    }
    if *infeed > h + 1e-9 {
        return Some(format!(
            "the radial infeed of {infeed} mm is more than the {h} mm thread depth a {pitch} mm pitch stands at"
        ));
    }
    // #988: the pass control against this thread's depth.
    if let Some(why) = passes.refusal(Some(h), *infeed) {
        return Some(why);
    }
    no_air(Some(h)).or_else(|| super::thread_r_plane::door(*top_z, holes, *external))
}

/// The kernel's parameters for this operation and tool, heights
/// resolved; every door refusal is the generator's refusal too.
pub(super) fn kernel_params(tool: &Tool, params: &OpParams, hl: &ResolvedHeights) -> Result<ThreadParams, GenError> {
    let OpParams::Thread { infeed, feed, plunge_feed, climb, holes, pitch, hand, external, passes, .. } = params else {
        return Err(GenError::Invalid("not a thread-milling operation".into()));
    };
    if !form::is_thread_mill(tool.kind) {
        return Err(GenError::Invalid(format!(
            "thread milling needs a thread mill or a single-form thread mill - this tool is a {}",
            tool.kind
        )));
    }
    // The pitch first: an unstated one on a single-form mill passes the
    // door (an unfinished operation) and is refused here, by name.
    let pitch = form::resolve_pitch(tool, *pitch).map_err(GenError::Invalid)?;
    if let Some(why) = door(tool, params) {
        return Err(GenError::Invalid(why));
    }
    // #985: the reach beside a boss again, now against the resolved bottom.
    if *external
        && let Some(why) = super::thread_boss::door(tool, holes, Some(depth_of(pitch, true)), Some(hl.bottom))
    {
        return Err(GenError::Invalid(why));
    }
    Ok(ThreadParams {
        holes: holes
            .iter()
            .map(|x| ThreadHole {
                xy: x.xy,
                radius: x.radius,
                top_z: x.entry_z,
                floor_z: hl.bottom.unwrap_or(x.floor_z),
                // A boss has no fold and no exit: the words are a hole's.
                stepped: x.stepped && !*external,
                // A stated bottom is the author's own floor; only the
                // hole's own exit takes the run-through allowance.
                through: x.through && hl.bottom.is_none() && !*external,
            })
            .collect(),
        pitch,
        tool_diameter: tool.diameter,
        rows: form::rows(tool),
        infeed: *infeed,
        external: *external,
        hand: *hand,
        climb: *climb,
        passes: *passes,
        clearance_z: hl.clearance,
        // #1212: the stated R plane is where every rapid stops and the
        // feed begins; the kernel refuses it under a feature's top.
        r_z: hl.top,
        retract_z: hl.retract,
        feed: *feed,
        plunge_feed: *plunge_feed,
    })
}

/// Whether every threaded hole of `plan` is a round wall in the part
/// (#894 x #1205): its wall facets the crests can reach (to the major
/// circle, plus [`verify::WALL_SAG`]) over the thread's span read as a
/// tessellated circle about the hole's axis, by the solid law's own
/// sagitta test. Only then do the two readings below apply; a thread
/// through a polygon's flats keeps its region's own rim and the full
/// tooth radius, so cutting into a flat is a gouge.
#[must_use]
pub(super) fn walls_round(facets: &[Triangle], plan: &ThreadParams) -> bool {
    let depth = plan.depth();
    plan.holes.iter().all(|h| {
        let (lo, hi) = (h.floor_z.min(h.top_z) - plan.tool_diameter, h.floor_z.max(h.top_z));
        verify::hole_is_round(facets, [h.xy.x, h.xy.y], h.radius + depth + verify::WALL_SAG, lo, hi)
    })
}

/// The solid law's thread grant for this operation (#1071 x #894): the
/// bores `thread_rims::thread_bores` reads, at the pitch the THREAD is
/// cut at - a single-form mill's own `pitch` is only its coarsest, and
/// granting that depth would excuse a cut past the real major circle.
/// Empty - no grant, the teeth judged at full radius against every
/// facet - for another kind, for a pitch that does not resolve (refused
/// before any path exists), and for a thread whose holes are not all
/// round walls (`round` false, [`walls_round`]).
#[must_use]
pub(super) fn bores(tool: &Tool, params: &OpParams, round: bool) -> Vec<verify::ThreadBore> {
    let OpParams::Thread { pitch, external, .. } = params else {
        return Vec::new();
    };
    // A boss is no bore (#985): its crests answer to the minor circle
    // and its roots to the narrowed cutter, never to a grant.
    if !round || *external {
        return Vec::new();
    }
    match form::resolve_pitch(tool, *pitch) {
        Ok(p) => super::thread_rims::thread_bores(params, p),
        Err(_) => Vec::new(),
    }
}

/// The solid law's cutter for an external thread (#985): `cutter`
/// narrowed by the external depth at the thread's own pitch, so the
/// body under the teeth - roots, tip, shank and holder - is held off the
/// whole part, the boss's drawn (major) wall and its shoulder included.
/// Every other operation, an internal thread (its grant is [`bores`]:
/// the two are never stacked) and a pitch that does not resolve
/// (refused before any path exists) keep the full cutter.
#[must_use]
pub(super) fn root_cutter(cutter: Cutter, tool: &Tool, params: &OpParams) -> Cutter {
    let OpParams::Thread { pitch, external: true, .. } = params else {
        return cutter;
    };
    match form::resolve_pitch(tool, *pitch) {
        Ok(p) if cutter.radius > depth_of(p, true) => Cutter { radius: cutter.radius - depth_of(p, true), ..cutter },
        _ => cutter,
    }
}

/// The radius the program simulation sweeps for `tool` under `params`:
/// the tool's own certain removal (`tool_law::cutting_radius`), except
/// an external Thread operation (#985), whose tooth roots stand the
/// external depth in at the thread's own pitch - exactly on the boss's
/// drawn wall on the last pass, where the internal depth at the tool's
/// coarsest pitch would read 0.07 x pitch of the boss as cut away.
pub fn sim_radius(tool: &Tool, params: &OpParams) -> Result<f64, String> {
    if let OpParams::Thread { pitch, external: true, .. } = params
        && let Ok(p) = form::resolve_pitch(tool, *pitch)
    {
        let root = tool.diameter * 0.5 - depth_of(p, true);
        if root.is_finite() && root > 0.0 {
            return Ok(root);
        }
    }
    super::tool_law::cutting_radius(tool)
}

/// The crossing law stands down for a Thread operation: every radial
/// pass and axial level retraces the same helix, and the lead arcs of
/// successive passes leave the axis tangent to one another, so feed
/// moves "crossing in XY at shared depth" is the operation itself, not
/// a path defect. What a harmful move would be - a crest past the major
/// circle, the body in the part, a rapid in stock - the crest law, the
/// solid law and the rapid law still judge. Every other kind keeps it.
pub(super) fn stand_down_crossings(params: &OpParams, warns: &mut Vec<Warning>) {
    if matches!(params, OpParams::Thread { .. }) {
        warns.retain(|w| !matches!(w, Warning::Crossing { .. }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::ThreadHand;
    use crate::drilling::Hole;
    use crate::geo2d::Point2;
    use crate::operation::Height;

    fn hole(radius: f64) -> Hole {
        Hole { xy: Point2::new(5.0, 5.0), radius, entry_z: 0.0, floor_z: -10.0, through: false, stepped: false }
    }

    fn thread(pitch: Option<f64>) -> OpParams {
        OpParams::Thread {
            clearance_z: Height::fixed(10.0),
            retract_z: None,
            top_z: Height::fixed(0.0),
            bottom_z: None,
            infeed: 0.2,
            feed: 300.0,
            plunge_feed: 100.0,
            climb: true,
            holes: vec![hole(4.0)],
            pitch,
            hand: ThreadHand::Right,
            external: false,
            passes: crate::machining2d::ThreadPasses::default(),
        }
    }

    /// Any kind that is not Thread: the registry's first row, seeded.
    fn other_kind() -> OpParams {
        (crate::operation::KindInfo::ALL[0].seed)(&crate::operation::Carried::of(&thread(None)))
    }

    #[test]
    fn the_thread_grant_reads_the_threads_own_pitch_and_only_over_a_round_wall() {
        // A multi-form mill: its own pitch, the major circle 0.5413 x 1.5 out.
        let tool = form::thread_mill(6.0, 1.5, 0.0, 12.0);
        let b = bores(&tool, &thread(None), true);
        assert_eq!(b.len(), 1);
        assert!((b[0].major - b[0].minor - 0.5413 * 1.5).abs() < 1e-12, "{b:?}");
        // A single-form mill cutting a 1 mm pitch inside its 0.5 to 1.5
        // range: the grant is the 1 mm thread's depth, not the coarsest's.
        let single = Tool { kind: crate::tools::ToolKind::ThreadMillSingle, pitch_min: 0.5, flute_len: 2.0, ..tool.clone() };
        let b = bores(&single, &thread(Some(1.0)), true);
        assert_eq!(b.len(), 1);
        assert!((b[0].major - b[0].minor - 0.5413).abs() < 1e-12, "{b:?}");
        // An unresolvable pitch, another kind, and a hole that is no round
        // wall (#1205) get no grant: the teeth at full radius.
        assert!(bores(&tool, &thread(Some(1.0)), true).is_empty());
        assert!(bores(&tool, &other_kind(), true).is_empty());
        assert!(bores(&tool, &thread(None), false).is_empty());
    }

    #[test]
    fn an_external_thread_narrows_the_roots_by_the_external_depth() {
        let tool = form::thread_mill(6.0, 1.5, 0.0, 12.0);
        let mut boss = thread(None);
        if let OpParams::Thread { external, .. } = &mut boss {
            *external = true;
        }
        let root = 3.0 - 0.6134 * 1.5;
        assert!((root_cutter(Cutter::of(&tool), &tool, &boss).radius - root).abs() < 1e-12);
        // A boss is no bore: no grant beside the narrowed roots.
        assert!(bores(&tool, &boss, true).is_empty());
        // An internal thread keeps the full cutter (its grant is `bores`).
        assert_eq!(root_cutter(Cutter::of(&tool), &tool, &thread(None)).radius, 3.0);
        assert!((sim_radius(&tool, &boss).expect("root") - root).abs() < 1e-12);
        // An internal thread keeps the tool's own certain removal.
        assert!((sim_radius(&tool, &thread(None)).expect("core") - (3.0 - 0.5413 * 1.5)).abs() < 1e-12);
        assert_eq!(sim_radius(&tool, &other_kind()), Ok(3.0 - 0.5413 * 1.5));
    }

    #[test]
    fn only_a_thread_operation_stands_the_crossing_law_down() {
        let mut warns = vec![Warning::Crossing { moves: [1, 2] }, Warning::Gouge { at: 3 }];
        stand_down_crossings(&other_kind(), &mut warns);
        assert_eq!(warns.len(), 2);
        stand_down_crossings(&thread(None), &mut warns);
        assert_eq!(warns, vec![Warning::Gouge { at: 3 }]);
    }
}

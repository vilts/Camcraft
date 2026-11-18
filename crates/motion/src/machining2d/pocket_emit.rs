//! Planar pocket pass emission shared by [`super::pocket`] and [`super::rest2d`] (#1816).
//!
//! Provenance: extracted from `pocket.rs` (`pocket_over`) - arc fitting
//! (`fit_arcs` / `fit_proved`), corner filleting (`fillet_corners`), level
//! ladder (`run_level_walks`), crossing-free arc enforcement
//! (`post_crossing_free`), and approach refusal checks. Clean-room, no vendor
//! material consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::adaptive::Return;
use super::arc_cross::post_crossing_free;
use super::blend_cross::{Bans, Blend, post_blended_crossing_free, round_walks};
use super::arc_proof::{ProofTally, fit_proved};
use super::arcs::{ARC_FIT_TOL, Seg, fit_arcs};
use super::face::GenError;
use super::fillet::Legal;
use super::fillet_tally::corner_arc_radius;
use super::levels::{Cert, Motion, cut_levels_under, run_level_walks};
use super::pocket::{PocketParams, PocketStrategy};
use super::pocket_hsm::EntryRoom;
use super::predrilled::PreHole;

/// Pre-planned XY cutter-center passes and entry/link certification areas.
pub(super) struct PocketWalks<'a> {
    pub(super) center: &'a Region,
    pub(super) links_center: &'a Region,
    pub(super) strategy: PocketStrategy,
    pub(super) passes: Vec<Vec<Point2>>,
    pub(super) room: Option<EntryRoom>,
    pub(super) joins: Vec<Option<Return>>,
    pub(super) hold_crossings: bool,
    pub(super) holes: &'a [PreHole],
}

/// Fit, optionally fillet, descend, link, and post `walks` across all Z levels.
pub(super) fn emit_pocket_passes(
    mut b: PathBuilder,
    p: &PocketParams,
    walks: PocketWalks<'_>,
) -> Result<Toolpath, GenError> {
    let PocketWalks {
        center,
        links_center,
        strategy,
        passes,
        room,
        joins,
        hold_crossings,
        holes,
    } = walks;
    // The horizontal arc entry (M11.3) lands on the first pass: a closed
    // first ring starts mid its longest edge, where an arc has room.
    let passes = if p.links.approach.arc_xy && room.is_none() {
        super::descent_xy::reseat_first(passes)
    } else {
        passes
    };
    // #1449: a proved hole re-seats the plan on itself (not the HSM plan's own bore).
    let passes = super::predrilled::reseat(
        passes,
        holes,
        center,
        p.links.approach.predrilled && room.is_none(),
    );
    let zs = cut_levels_under(p.top_z, p.links.approach.stock_top, p.bottom_z, p.stepdown)?;
    let fit = match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => None,
        PocketStrategy::Waterline | PocketStrategy::OffsetRings => Some(ARC_FIT_TOL),
        PocketStrategy::Adaptive => Some(ARC_FIT_TOL * 0.25),
    };
    let mut proof = ProofTally::default();
    let mut fit_pass = |pass: &[Point2], tol: f64| -> Vec<Seg> {
        if strategy == PocketStrategy::Adaptive {
            fit_proved(pass, tol, center, &mut proof)
        } else {
            let segs = fit_arcs(pass, tol);
            proof.arcs += segs.iter().filter(|s| matches!(s, Seg::Arc { .. })).count();
            segs
        }
    };
    let helix_r = room.as_ref().map_or(p.helix_radius, |e| e.helix_r);
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
        .with_entry(p.entry, helix_r)
        .with_arc_radius(p.tool_diameter)
        .with_links(p.links, helix_r)
        .with_top(p.links.approach.entry_top(p.top_z))
        .with_xy_arc(p.links.approach.arc_xy)
        .with_microlift(joins)
        .with_holes(holes)
        .with_solid_restart();
    let fitted: Option<Vec<Vec<Seg>>> = fit.map(|tol| passes.iter().map(|p| fit_pass(p, tol)).collect());
    // #1733: the corner arcs asked for. They round the walks as fitted
    // (a pass with no fit: its own chords).
    let blend = if p.blend_turns {
        let legal = Legal { region: center, inside: true, air_left: None };
        let rho = corner_arc_radius(p.blend_radius, p.tool_diameter).map_err(GenError::Invalid)?;
        Some(Blend { rho, legal })
    } else {
        None
    };
    let cert = match &room {
        Some(e) => Cert::Stadium { a: e.seed, b: e.seed, h: e.clear, tess: &e.tess },
        None => Cert::Region(center),
    };
    let post = |b: &mut PathBuilder, runs: Option<&[Vec<Seg>]>| {
        run_level_walks(
            b,
            &passes,
            &zs,
            &motion,
            runs,
            Some(&cert),
            Some(&Cert::Region(links_center)),
        );
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    };
    // #2240: enforce the crossing law on every arc-fitted pocket pass,
    // including Offset rings (used directly or as Adaptive's fallback) and
    // Waterline, so arcs across micron-quantized vertices never cross.
    let hold_fitted = hold_crossings
        || matches!(
            strategy,
            PocketStrategy::Adaptive | PocketStrategy::OffsetRings | PocketStrategy::Waterline
        );
    let mut path = match (blend, fitted) {
        (Some(blend), fitted) => {
            let base = fitted
                .unwrap_or_else(|| passes.iter().map(|pass| vec![Seg::Line(pass.get(1..).unwrap_or(&[]).to_vec())]).collect());
            if hold_fitted {
                // #2317: the law on the rounded walks, one corner arc
                // at a time - a crossing never costs a pass its arcs.
                post_blended_crossing_free(b, &passes, base, &blend, &mut proof, post)
            } else {
                let rounded = round_walks(&passes, &base, &blend, &Bans::default());
                for n in rounded.notes() {
                    b.note(n);
                }
                post(&mut b, Some(rounded.runs.as_slice()));
                b.finish()
            }
        }
        (None, Some(runs)) if hold_fitted => post_crossing_free(b, &passes, runs, &mut proof, post),
        (None, runs) => {
            post(&mut b, runs.as_deref());
            b.finish()
        }
    };
    if let Some(n) = proof.note() {
        path.notes.push(n);
    }
    if let Some(why) = crate::approach::refusal(&path) {
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry3D;
    use crate::geo2d::Loop;
    use crate::links::Links;

    #[test]
    fn offset_rings_across_one_micron_vertex_step_post_crossing_free() {
        // A circular pass with a 1e-6 mm step along its circumference (as
        // produced by Clipper2 integer offsets on STEP-faceted circles like
        // fig8.step): fit_arcs splits into [Arc, 1um-Line, Arc] whose chord
        // brackets cross unless post_crossing_free enforces the crossing law.
        let r = 26.0;
        let n = 175usize;
        let mid = n / 4;
        let mut pass = Vec::with_capacity(n + 2);
        let mut loop_pts = Vec::with_capacity(n);
        for k in 0..n {
            let th = k as f64 * std::f64::consts::TAU / n as f64;
            let p = Point2::new(r * th.cos(), r * th.sin());
            loop_pts.push(p);
            pass.push(p);
            if k == mid {
                // Backward 1e-6 mm tangent jog at 12 o'clock (exact pattern of move [198] on fig8.step).
                pass.push(Point2::new(p.x + 1e-6, p.y));
            }
        }
        pass.push(pass[0]);
        let Ok(ring) = Loop::new(loop_pts) else { panic!("loop") };
        let Ok(center) = Region::new(vec![ring]) else { panic!("region") };
        let pp = PocketParams {
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 20.0,
            bottom_z: 18.0,
            stepdown: 2.0,
            feed: 2400.0,
            plunge_feed: 500.0,
            climb: true,
            strategy: PocketStrategy::OffsetRings,
            entry: Entry3D::Helix,
            helix_radius: 1.0,
            optimal_load: 0.25,
            blend_turns: false,
            blend_radius: None,
            links: Links::PLANAR,
        };
        let b = PathBuilder::approached(Point3::new(0.0, 0.0, pp.clearance_z));
        let Ok(path) = emit_pocket_passes(
            b,
            &pp,
            PocketWalks {
                center: &center,
                links_center: &center,
                strategy: PocketStrategy::OffsetRings,
                passes: vec![pass],
                room: None,
                joins: Vec::new(),
                hold_crossings: false,
                holes: &[],
            },
        ) else {
            panic!("emit_pocket_passes")
        };
        let crossed = crate::verify::crossed_pairs(&path, ARC_FIT_TOL);
        assert!(crossed.is_empty(), "OffsetRings fitted arcs must never cross: {crossed:?}");
    }
}

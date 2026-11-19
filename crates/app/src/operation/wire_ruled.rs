//! The Wire Contour's 4-axis cut (#537, part of #290): the selection
//! read as two profiles at two heights - each pick a face (its one
//! outer ring) or an edge chain lying flat on a horizontal plane - the
//! higher one the upper profile, the lower one the lower; and
//! `wire_edm::wire_contour_ruled` run over them with the op's stated
//! pairing rule.
//!
//! Nothing is inferred: the profiles' heights must be the op's top and
//! bottom planes, a pick that is not flat, a face with holes, a third
//! profile or two at one height are refused by name. A profile stitched
//! from several open runs is not read here - pick a face or one chain.
//!
//! Provenance: our own glue over the motion crate's ruled generator
//! (whose modules cite the public sources) and the feature picks; the
//! umbrella's decisions D2 and D7.

use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2};
use crate::geometry::feature::{self, LoopKind};
use crate::geometry::{transform_point, Point3};
use crate::machining2d::GenError;
use crate::model;
use crate::schema::Operation;
use crate::wire_edm::{wire_contour_ruled, WirePairing, WireParams, WireProfile, WireRuled, WireSync};

use super::types::{FeatureSel, GeoSel, OpParams, WireContourOp, WireFourAxis, WirePairingRule};
use super::wire_op::WireCondition;
use super::{ResolvedHeights, SrcRef};

/// How far (mm) a profile's points may leave its plane, and two planes
/// may stand apart and still be one.
const FLAT: f64 = 1e-6;

/// One picked profile: its height and its ring (closed) or run (open).
#[derive(Clone, Debug)]
pub(super) struct Picked {
    z: f64,
    loops: Vec<Loop>,
    chains: Vec<Vec<Point2>>,
}

/// The two profiles of a 4-axis cut, upper over lower.
#[derive(Clone, Debug)]
pub(super) struct RuledPicks {
    upper: Picked,
    lower: Picked,
}

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

/// The op's two profiles, read from the world: `None` for any op that
/// is not a 4-axis wire cut.
pub(super) fn picks_of(world: &World, op: &Operation) -> Option<Result<RuledPicks, GenError>> {
    let OpParams::WireContour(WireContourOp { four_axis: Some(_), .. }) = &op.params else {
        return None;
    };
    Some(match &op.src {
        SrcRef::Selection(items) => read(world, op.setup.as_deref(), items),
        _ => Err(invalid(
            "a 4-axis cut pairs two picked profiles - pick the upper profile on the top plane and the lower profile on the bottom plane (a face or a closed edge chain each)".into(),
        )),
    })
}

/// One pick as a flat profile.
fn profile(world: &World, setup: Option<&str>, g: &GeoSel) -> Result<Picked, GenError> {
    let label = crate::pick::geo_label(world, g);
    let Some(entity) = model::find_by_name(world, &g.entity) else {
        return Err(GenError::Invalid(format!("the selection names '{}', which is gone", g.entity)));
    };
    let Some(features) = model::features_of(world, &entity) else {
        return Err(GenError::Invalid(format!("'{}' has no geometry to select from", g.entity)));
    };
    let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
    let place = |p: &[f64; 3]| transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
    let gone = || GenError::Invalid(format!("{label} is no longer in the model: re-pick it"));
    let (pts, closed): (Vec<Point3>, bool) = match g.feature {
        FeatureSel::Face(fid) => {
            let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(gone)?;
            let rings: Vec<_> = f.loops.iter().filter(|l| !matches!(l.kind, LoopKind::Open)).collect();
            let [ring] = rings.as_slice() else {
                return Err(invalid(format!(
                    "{label} has {} closed rings - a profile is one ring; pick the edge chain of the ring to cut",
                    rings.len()
                )));
            };
            (ring.verts.iter().filter_map(|&vi| features.verts.get(vi as usize)).map(place).collect(), true)
        }
        FeatureSel::Chain(cid) => {
            let c = crate::model::resolve_chain(&features, cid).ok_or_else(gone)?;
            (feature::chain_points(&features, &c).iter().map(place).collect(), c.closed)
        }
        _ => {
            return Err(invalid(format!("{label} is not a profile - a 4-axis cut pairs two faces or edge chains")));
        }
    };
    let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.z), b.max(p.z)));
    if pts.is_empty() || !(lo.is_finite() && hi.is_finite()) {
        return Err(invalid(format!("{label} has no points to read a profile from")));
    }
    if hi - lo > FLAT {
        return Err(invalid(format!(
            "{label} is not flat (it runs from Z{lo:.3} to Z{hi:.3}) - each profile of a 4-axis cut lies on one horizontal plane"
        )));
    }
    let xy: Vec<Point2> = pts.iter().map(|p| Point2::new(p.x, p.y)).collect();
    let z = (lo + hi) * 0.5;
    Ok(if closed {
        Picked { z, loops: vec![Loop::new(xy).map_err(|e| invalid(format!("{label} is not a usable ring: {e}")))?], chains: Vec::new() }
    } else {
        Picked { z, loops: Vec::new(), chains: vec![xy] }
    })
}

fn read(world: &World, setup: Option<&str>, items: &[GeoSel]) -> Result<RuledPicks, GenError> {
    let mut found = items.iter().map(|g| profile(world, setup, g)).collect::<Result<Vec<_>, _>>()?;
    for pick in found.iter().filter(|p| !p.chains.is_empty()) {
        let runs = found.iter().filter(|p| !p.chains.is_empty() && (p.z - pick.z).abs() <= FLAT).count();
        if runs > 1 {
            return Err(invalid(format!(
                "a profile stitched from {runs} open runs at Z{:.3} is not supported - pick one face or closed edge chain on that plane",
                pick.z
            )));
        }
    }
    if found.len() != 2 {
        return Err(invalid(format!(
            "a 4-axis cut pairs two profiles, and {} are picked - pick the upper profile and the lower profile, one face or edge chain each",
            found.len()
        )));
    }
    found.sort_by(|a, b| a.z.total_cmp(&b.z));
    let (Some(upper), Some(lower)) = (found.pop(), found.pop()) else {
        return Err(invalid("a 4-axis cut pairs two profiles".into()));
    };
    if upper.z - lower.z <= FLAT {
        return Err(invalid(format!(
            "both picked profiles lie at Z{:.3} - a 4-axis cut pairs an upper profile with a lower one at another height",
            lower.z
        )));
    }
    Ok(RuledPicks { upper, lower })
}

/// The 4-axis block's share of the operation's stamp.
pub(super) fn digest(f: &WireFourAxis, h: &mut std::hash::DefaultHasher) {
    h.write(b"four-axis");
    h.write_u8(u8::from(f.pairing == WirePairingRule::SyncPoints));
    h.write(&f.tolerance.to_bits().to_le_bytes());
    // #1170: the widening moves the wire. The verify tolerance does not:
    // the wire verify reads it at each post, never the stored path.
    h.write_u8(u8::from(f.widening == crate::wire_edm::WireWidening::Uniform));
    h.write_usize(f.sync.len());
    for s in &f.sync {
        for q in [s.upper, s.lower] {
            match q {
                Some([x, y]) => {
                    h.write_u8(1);
                    h.write(&x.to_bits().to_le_bytes());
                    h.write(&y.to_bits().to_le_bytes());
                }
                None => h.write_u8(0),
            }
        }
    }
}

/// Generate the 4-axis cut of `picks` on the planes `p` and `hs` state.
pub(super) fn generate(
    op: &WireContourOp,
    four: &WireFourAxis,
    cond: &WireCondition,
    p: &WireParams,
    hs: &ResolvedHeights,
    picks: &RuledPicks,
) -> Result<Toolpath, GenError> {
    if op.taper_deg != 0.0 {
        return Err(invalid(format!(
            "a 4-axis cut and a {:.3} degree taper are both stated - the two profiles are the lean; set the taper angle to 0",
            op.taper_deg
        )));
    }
    let top = hs.top.unwrap_or(p.plane_z);
    for (which, plane, word, at) in [("upper", "top", top, picks.upper.z), ("lower", "bottom", p.plane_z, picks.lower.z)] {
        if (word - at).abs() > FLAT {
            return Err(invalid(format!(
                "the {which} profile lies at Z{at:.3} and the workpiece's {plane} plane at Z{word:.3} - a 4-axis cut reads the {which} profile on the {plane} plane; set the plane to the profile's height"
            )));
        }
    }
    let pt = |q: Option<[f64; 2]>| q.map(|[x, y]| Point2::new(x, y));
    let pairing = match four.pairing {
        WirePairingRule::ArcLength => WirePairing::ArcLength,
        WirePairingRule::SyncPoints => {
            WirePairing::SyncPoints(four.sync.iter().map(|s| WireSync { upper: pt(s.upper), lower: pt(s.lower) }).collect())
        }
    };
    let ruled =
        WireRuled {
        top_z: top,
        pairing,
        tolerance: four.tolerance,
        widening: four.widening,
        max_deg: cond.max_taper_deg,
        machine: cond.machine.clone(),
    };
    let (upper, lower) = (
        WireProfile { loops: &picks.upper.loops, chains: &picks.upper.chains },
        WireProfile { loops: &picks.lower.loops, chains: &picks.lower.chains },
    );
    let mut path = wire_contour_ruled(upper, lower, p, &ruled)?;
    if four.pairing == WirePairingRule::ArcLength && !four.sync.is_empty() {
        path.notes.push(format!(
            "wire EDM: 4-axis - {} sync pair(s) are stored and not read: the pairing is by arc length",
            four.sync.len()
        ));
    }
    Ok(path)
}

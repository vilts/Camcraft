//! The round half of the Probe pick seed (#171): a picked cylindrical
//! wall (a welded patch) or a closed round rim seeds a Bore or Boss
//! centre - centre and radius from the circle fit Circular's pick reads
//! ([`super::circle_of_region`]), at the rim's height. Bore or Boss is
//! the wall's own winding: an outward normal that points AT the axis is
//! a hole's wall (the material is outside it), one pointing away is a
//! boss's. Provenance: touch-probe practice (a bore is touched outward
//! from its centre, a boss inward from outside); facet-winding normals
//! from standard mesh geometry. Our own law; no vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::geometry::feature::{self, Chain};
use crate::probing::{ProbeDir, ProbeMode};

use super::probe_pick::{PICK_TOL, PickCtx, ProbeSeed};

/// A wall counts as vertical when its outward normal's Z is within the
/// probe's axis tolerance of level.
fn vertical(n: Point3) -> bool {
    n.z.abs() <= super::probe_pick::PROBE_AXIS_TOL_DEG.to_radians().sin()
}

/// The circle a level ring of world points stands on: centre, radius
/// and the ring's height. Refused by name when the ring tilts or the
/// fit finds no circle.
fn ring_circle(pts: &[Point3], what: &str) -> Result<(Point2, f64, f64), String> {
    let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p.z), h.max(p.z)));
    if !(hi.is_finite() && lo.is_finite()) || hi - lo > PICK_TOL {
        return Err(format!("the picked {what} is not level - a bore or boss centre is found on a vertical round wall"));
    }
    let not_round = || format!("the picked {what} is not a circle - a bore or boss centre needs a round wall");
    let ring = Loop::new(pts.iter().map(|p| Point2::new(p.x, p.y)).collect()).map_err(|_| not_round())?;
    let region = Region::new(vec![ring]).map_err(|_| not_round())?;
    let (c, r) = super::circle_of_region(&region, crate::units::Unit::Millimeters).map_err(|_| not_round())?;
    Ok((c, r, hi))
}

/// Bore or Boss from the area-weighted side the walls' outward normals
/// face, about the centre. Mixed or level-only walls refuse.
fn side_of(cx: &PickCtx, faces: &[usize], c: Point2, what: &str) -> Result<ProbeMode, String> {
    let mut lean = 0.0f64;
    let mut walls = 0usize;
    for &fi in faces {
        let (Some(n), Some(m), Some(f)) = (cx.outward(fi), cx.centroid(fi), cx.features.faces.get(fi)) else {
            continue;
        };
        if !vertical(n) {
            continue;
        }
        walls += 1;
        lean += f.area * (n.x * (m.x - c.x) + n.y * (m.y - c.y)).signum();
    }
    if walls == 0 {
        return Err(format!("the picked {what} has no vertical wall - a bore or boss centre is touched on one"));
    }
    if lean < 0.0 {
        Ok(ProbeMode::Bore)
    } else if lean > 0.0 {
        Ok(ProbeMode::Boss)
    } else {
        Err(format!("the picked {what} faces neither into a hole nor out of a boss"))
    }
}

fn seed(mode: ProbeMode, c: Point2, r: f64, z: f64) -> ProbeSeed {
    ProbeSeed { mode, dir: ProbeDir::ZNeg, dir_y: ProbeDir::YPos, target: [c.x, c.y, z], radius: Some(r) }
}

/// A closed rim chain: its circle, and the wall under it decides the side.
pub(super) fn rim_seed(cx: &PickCtx, chain: &Chain) -> Result<ProbeSeed, String> {
    let pts: Vec<Point3> = feature::chain_points(&cx.features, chain).iter().map(|p| cx.world(*p)).collect();
    let (c, r, z) = ring_circle(&pts, "rim")?;
    let below: Vec<usize> = cx
        .faces_of_edges(chain.edges.iter().copied())
        .into_iter()
        .filter(|&fi| cx.centroid(fi).is_some_and(|m| m.z < z - PICK_TOL))
        .collect();
    Ok(seed(side_of(cx, &below, c, "rim")?, c, r, z))
}

/// A welded round wall: every strip vertical, the top closed ring's
/// circle at the rim, the strips' winding deciding the side.
pub(super) fn wall_seed(cx: &PickCtx, id: u64) -> Result<ProbeSeed, String> {
    let gone = "the picked wall is no longer in the model - re-pick it";
    let patch = feature::patches(&cx.features).into_iter().find(|p| p.id == id).ok_or(gone)?;
    let rings = feature::patch_rings(&cx.features, id).ok_or(gone)?;
    let faces: Vec<usize> = cx
        .features
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| patch.faces.contains(&f.id))
        .map(|(i, _)| i)
        .collect();
    if faces.iter().any(|&fi| cx.outward(fi).is_none_or(|n| !vertical(n))) {
        return Err("the picked wall is not vertical (a cone, fillet or tilted wall) - a bore or boss centre is found on a vertical round wall".into());
    }
    let mut top: Option<(Point2, f64, f64)> = None;
    for ring in rings.iter().filter(|r| r.closed) {
        let pts: Vec<Point3> = ring.verts.iter().filter_map(|&v| cx.point(v)).collect();
        if pts.len() != ring.verts.len() {
            return Err("the picked wall's rim is not finite".into());
        }
        let fit = ring_circle(&pts, "wall's rim")?;
        if top.is_none_or(|t| fit.2 > t.2) {
            top = Some(fit);
        }
    }
    let (c, r, z) = top.ok_or("the picked wall has no closed rim - a bore or boss centre needs a full round wall")?;
    Ok(seed(side_of(cx, &faces, c, "wall")?, c, r, z))
}

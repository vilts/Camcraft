//! Wire EDM no-core burnout (#540, part of #290): a closed pocket's
//! whole interior burnt away by the wire, so no core is left to drop.
//!
//! The pattern, stated: offset rings of the pocket, the first at the
//! stated wall clearance (the wire centre's least distance to the
//! walls), each next one [`RING_STEP`] x the kerf offset further in,
//! until the offset is empty. The rings are cut innermost first, so
//! every ring closes around kerf already burnt - no closed cut ever
//! encloses unburnt material. The wire is threaded once at the stated
//! start hole, leads straight to the innermost ring, steps out ring by
//! ring along the shortest link, and leads back to the start hole.
//!
//! Laws, each a refusal by name: a clearance under the kerf offset
//! (the kerf would burn the wall); a missing start hole, or one outside
//! the clearance; a pattern whose offset splits into several islands
//! (one thread cannot reach them all as rings); the kerf certificate
//! (no move nearer the walls than the clearance); and the no-core law -
//! the kerf swept along the whole path is taken out of the pocket and
//! a ring of the wall around it, and any piece left that is not joined
//! to the wall is a core that drops (region connectivity).
//!
//! Provenance: the "no-core" / burnout practice of general wire EDM
//! literature as the umbrella #290 cites it (the pocket's interior is
//! eroded entirely instead of freeing a slug); the rings are the
//! textbook polygon offset (clipper2 via `geo2d`), the no-core law is
//! the disc sweep and polygon booleans of `geo2d`. Our own code; no
//! vendor post, control manual or CAM product behaviour consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, nearest, walk_from};
use super::{laws, ring, WireParams, WireSide, COVER_TOL, PATH_TOL};

/// The ring step as a fraction of the kerf offset: under 1, so the
/// innermost ring's kerf covers what it encloses and neighbouring
/// rings' kerfs overlap by more than half their width.
pub const RING_STEP: f64 = 0.9;
/// The least area (mm²) of a piece left standing that the no-core law
/// reads as a core: a numerical sliver of the booleans is not one.
pub const CORE_AREA_TOL: f64 = 1e-3;
/// The most rings one burnout lays (a pocket ~ 2 x this x the step
/// across): a bound on the work, refused by name past it.
const MAX_RINGS: usize = 4096;
/// How wide (mm) a ring of wall the no-core law adds around the pocket:
/// the material every piece left must reach to stay held.
const WALL_RING: f64 = 1.0;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire burnout: {why}"))
}

/// The burnout door: the clearance against the kerf, before any
/// geometry is read. Shared with the command door.
///
/// # Errors
/// A non-finite clearance, or one under the kerf offset.
pub fn clearance_door(clearance: f64, kerf_offset: f64) -> Result<(), String> {
    if !clearance.is_finite() {
        return Err("wire burnout: the wall clearance is not a finite number".into());
    }
    if clearance < kerf_offset - PATH_TOL {
        return Err(format!(
            "wire burnout: the wall clearance {clearance:.4} mm is below the kerf offset {kerf_offset:.4} mm (wire radius + spark gap) - the kerf would burn the wall; state a clearance of at least the kerf offset"
        ));
    }
    Ok(())
}

/// Burn the one closed pocket in `loops` out with the wire, the rings
/// standing at least `clearance` from its walls.
///
/// # Errors
/// Every refusal the module head lists, by name; none is a panic.
pub fn wire_burnout(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams, clearance: f64) -> Result<Toolpath, GenError> {
    p.door()?;
    let k = p.kerf_offset();
    clearance_door(clearance, k).map_err(GenError::Invalid)?;
    if p.side != WireSide::Die {
        return Err(invalid(format!(
            "a burnout removes a pocket's interior - it is a die-side cut, and this operation states the {} side",
            p.side.word()
        )));
    }
    if p.tabs.is_some() {
        return Err(invalid(
            "tabs are stated on a burnout - a burnout leaves no core for a tab to hold (#539, #540)".into(),
        ));
    }
    let (given, profile) = ring::profile(loops, chains)?;
    let rings = rings(&profile, clearance, k)?;
    let Some(outer) = rings.first() else {
        return Err(invalid(format!(
            "the pocket is too small for the wire - no point inside it stands the wall clearance ({clearance:.4} mm) from its walls"
        )));
    };
    // The outermost ring at the kerf offset IS the finished wall: the
    // corner minimum holds for it as for a contour.
    if clearance < k + COVER_TOL {
        laws::cover(&given, outer, p)?;
    }
    let Some(start) = p.start else {
        return Err(invalid(
            "no start hole is stated - the burnout is threaded at a start hole inside the pocket; the wire cannot be threaded through solid material".into(),
        ));
    };
    if !Region::new(vec![Loop::new(outer.clone())?])?.contains(start) {
        return Err(GenError::invalid_at(
            format!(
                "wire burnout: the start hole at X{:.3} Y{:.3} stands outside the burnout's reach - it must lie inside the pocket, at least the wall clearance ({clearance:.4} mm) from every wall",
                start.x, start.y
            ),
            Point3::new(start.x, start.y, p.plane_z),
        ));
    }
    let z = p.plane_z;
    let mut b = PathBuilder::new(Point3::new(start.x, start.y, z));
    let mut at = start;
    for r in rings.iter().rev() {
        let (seg, foot) = nearest(r, at);
        for q in walk_from(r, seg, foot) {
            if dist(q, at) > 1e-9 {
                b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), None);
                at = q;
            }
        }
    }
    b.push(MoveKind::Feed, Point3::new(start.x, start.y, z), None);
    let mut path = b.finish();
    if path.moves.len() < 4 {
        return Err(invalid("the burnout lays down no cut".into()));
    }
    // The certificate: no move nearer the walls than the clearance.
    laws::kerf(&path, &given, true, clearance, z).map_err(|e| match e {
        GenError::At { msg, at } => GenError::At { msg: msg.replace("wire contour", "wire burnout").replace("the kerf offset", "the wall clearance"), at },
        e => e,
    })?;
    no_core(&profile, &path, k, z)?;
    path.notes.extend(notes(p, clearance, rings.len(), start));
    Ok(path)
}

/// The rings, outermost first: the pocket shrunk by `clearance`, then by
/// each further step, counter-clockwise, until the offset is empty.
fn rings(profile: &Region, clearance: f64, k: f64) -> Result<Vec<Vec<Point2>>, GenError> {
    let step = RING_STEP * k;
    let mut out = Vec::new();
    for j in 0..=MAX_RINGS {
        let d = clearance + step * j as f64;
        let off = profile.offset_sharp(-d)?;
        let one = match off.loops() {
            [] => return Ok(out),
            [one] => one,
            many => {
                return Err(invalid(format!(
                    "the pocket narrows to a neck - at {d:.4} mm from its walls the ring pattern splits into {} islands, and one thread of the wire cannot burn them all out as rings; widen the neck or cut each part with its own operation",
                    many.len()
                )));
            }
        };
        let mut pts = one.points().to_vec();
        if pts.len() < 3 {
            return Ok(out);
        }
        if !one.is_ccw() {
            pts.reverse();
        }
        out.push(pts);
    }
    Err(invalid(format!("the pocket needs more than {MAX_RINGS} rings - it is too large for one burnout at this wire")))
}

/// The no-core law: the kerf swept along the path out of the pocket
/// (plus a ring of wall); every piece left must be the wall's.
fn no_core(profile: &Region, path: &Toolpath, k: f64, z: f64) -> Result<(), GenError> {
    let mut pts: Vec<Point2> = path.moves.first().map(|m| vec![Point2::new(m.from.x, m.from.y)]).unwrap_or_default();
    pts.extend(path.moves.iter().map(|m| Point2::new(m.to.x, m.to.y)));
    let kerf = Region::sweep(&[pts], k)?;
    let left = profile.offset_sharp(WALL_RING)?.difference(&kerf)?;
    // The wall's piece is the one with the largest outer loop (it holds
    // the added ring); any other outer loop is a piece cut free.
    let mut outers: Vec<&Loop> = left.loops().iter().filter(|l| l.area() > 0.0).collect();
    outers.sort_by(|a, b| b.area().total_cmp(&a.area()));
    let cores: Vec<&&Loop> = outers.iter().skip(1).filter(|l| l.area() > CORE_AREA_TOL).collect();
    let Some(core) = cores.first() else {
        return Ok(());
    };
    let pts = core.points();
    let n = pts.len().max(1) as f64;
    let c = pts.iter().fold(Point2::new(0.0, 0.0), |a, q| Point2::new(a.x + q.x / n, a.y + q.y / n));
    Err(GenError::invalid_at(
        format!(
            "wire burnout: the pattern leaves a core of {:.4} mm² near X{:.3} Y{:.3} ({} piece(s) in all) that the kerf does not reach - it would drop free; refused",
            core.area(),
            c.x,
            c.y,
            cores.len()
        ),
        Point3::new(c.x, c.y, z),
    ))
}

fn notes(p: &WireParams, clearance: f64, rings: usize, start: Point2) -> Vec<String> {
    let cond = if p.condition.is_empty() { String::new() } else { format!(", condition {}", p.condition) };
    vec![
        format!(
            "wire EDM burnout: no core - {rings} offset ring(s) of the pocket cut innermost first, the outermost {clearance:.4} mm from the walls, {:.4} mm apart (kerf offset {:.4} mm: wire radius {:.4} + spark gap {:.4}{cond})",
            RING_STEP * p.kerf_offset(),
            p.kerf_offset(),
            p.wire_diameter * 0.5,
            p.gap
        ),
        format!(
            "wire EDM burnout: threaded once at X{:.3} Y{:.3}; straight lead to the innermost ring, straight links outward ring to ring, straight lead back to the threading point; no slug drops",
            start.x, start.y
        ),
        "wire EDM: a vertical cut - the moves state no pose (XY only) and no feed (the control's generator sets the rate)".into(),
    ]
}

#[cfg(test)]
#[path = "burnout_tests.rs"]
mod tests;

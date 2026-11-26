//! The wire's exit pick (#3332, part of #1448): where a vertical,
//! single-pass wire cut leaves, stated by the user. The pick is a wish;
//! it is held or refused by name, and it never moves.
//!
//! - An OPEN run ends at the end nearest the exit pick. The wire rides
//!   the same line on the same side; only the walk turns round when the
//!   pick names the other end. A pick the same distance from the two
//!   ends names no end and is refused.
//! - A CLOSED profile is cut all the way round from the threading point,
//!   as without a pick. The wire then rides its own kerf on, the same way
//!   round, to the path's point nearest the exit pick, and leaves by a
//!   straight lead-out to the pick. The pick must stand on the waste side,
//!   the kerf offset or more from the profile (inside the wire path of a
//!   die, outside it for a punch), as the start hole must. The lead-out
//!   then cannot cross the profile, and the kerf law certifies it.
//! - Tabs are refused with an exit pick: each stretch between tabs and
//!   the tab cut-off start from the threading point with the wire
//!   threaded, so the wire cannot leave before them.
//!
//! The cut without the pick is generated first, by the module's own
//! generator, so every law of that cut holds before the exit is read.
//!
//! Provenance: general wire EDM practice as the umbrella #290 cites it
//! (a slot that the wire already burnt is free to travel in; the waste
//! side takes the leads) over plain 2D polyline geometry. Our own code;
//! no vendor post, control manual or CAM product behaviour was consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{self, dist};
use super::{cut_pass, invalid, laws, WireParams, WireSide, MIN_LEAD};

/// How near (mm) the pick's distances to the two ends of an open run may
/// be and still name no end: 1 um.
pub const END_TIE: f64 = 1e-3;

fn at(why: String, q: Point2, z: f64) -> GenError {
    GenError::invalid_at(format!("wire contour: {why}"), Point3::new(q.x, q.y, z))
}

/// Cut the profile or the open run as [`super::wire_contour`] does, and
/// leave it at `exit` (module head).
///
/// # Errors
/// Every refusal of the cut without the pick, then the exit's own, by
/// name; none is a panic.
pub fn wire_contour_exit(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams, exit: Point2) -> Result<Toolpath, GenError> {
    if !(exit.x.is_finite() && exit.y.is_finite()) {
        return Err(invalid("the exit point's position is not a finite point".into()));
    }
    if chains.is_empty() && p.tabs.is_some() {
        return Err(invalid(
            "an exit point is stated with tabs - each stretch between tabs and the tab cut-off start from the threading point with the wire threaded, so the wire cannot leave at an exit point; clear the exit point or remove the tabs".into(),
        ));
    }
    let base = cut_pass(loops, chains, p, false)?;
    if chains.is_empty() {
        closed(base, loops, p, exit)
    } else {
        open(base, chains, p, exit)
    }
}

/// The path's points: where it starts, then the end of every move.
fn points(path: &Toolpath) -> Vec<Point2> {
    let mut v: Vec<Point2> = path.moves.first().map(|m| Point2::new(m.from.x, m.from.y)).into_iter().collect();
    v.extend(path.moves.iter().map(|m| Point2::new(m.to.x, m.to.y)));
    v
}

/// The feed moves through `pts` on the programmed plane, with `notes`.
fn feeds(pts: &[Point2], z: f64, notes: Vec<String>) -> Result<Toolpath, GenError> {
    let Some((first, rest)) = pts.split_first() else {
        return Err(invalid("the wire path lays down no cut".into()));
    };
    let mut b = PathBuilder::new(Point3::new(first.x, first.y, z));
    let mut prev = *first;
    for &q in rest {
        if dist(prev, q) > 1e-9 {
            b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), None);
            prev = q;
        }
    }
    let mut path = b.finish();
    if path.moves.is_empty() {
        return Err(invalid("the wire path lays down no cut".into()));
    }
    path.notes = notes;
    Ok(path)
}

/// Replace the note that starts with `lead` by `said` (or add it).
fn resay(notes: &mut Vec<String>, lead: &str, said: String) {
    match notes.iter_mut().find(|n| n.starts_with(lead)) {
        Some(n) => *n = said,
        None => notes.push(said),
    }
}

/// An open run: the cut ends at the run's end nearest `exit`.
fn open(base: Toolpath, chains: &[Vec<Point2>], p: &WireParams, exit: Point2) -> Result<Toolpath, GenError> {
    let run = chains.first().map_or(&[][..], Vec::as_slice);
    let (Some(&a), Some(&b)) = (run.first(), run.last()) else {
        return Err(invalid("the open run has no ends for an exit point to name".into()));
    };
    let (da, db) = (dist(exit, a), dist(exit, b));
    if !(da.is_finite() && db.is_finite()) || (da - db).abs() <= END_TIE {
        return Err(at(
            format!(
                "the exit point at X{:.3} Y{:.3} stands the same distance from both ends of the open run (X{:.3} Y{:.3} and X{:.3} Y{:.3}) - it names no end; move it nearer the end where the cut must leave, or clear it",
                exit.x, exit.y, a.x, a.y, b.x, b.y
            ),
            exit,
            p.plane_z,
        ));
    }
    let mut pts = points(&base);
    let mut notes = base.notes;
    let turned = da < db;
    if turned {
        pts.reverse();
    }
    let (Some(&t0), Some(&t1)) = (pts.first(), pts.last()) else {
        return Err(invalid("the open run lays down no cut".into()));
    };
    resay(
        &mut notes,
        "wire EDM: threaded outside the stock",
        format!(
            "wire EDM: threaded outside the stock at X{:.3} Y{:.3} and run out at X{:.3} Y{:.3}, each {:.3} mm clear of the stock's edge past the kerf; no start hole, no return",
            t0.x, t0.y, t1.x, t1.y, p.clearance
        ),
    );
    notes.push(format!(
        "wire EDM: the exit point at X{:.3} Y{:.3} names the run's {} end - the cut is walked {} the run's point order and ends there; the kerf stays {} of that point order",
        exit.x,
        exit.y,
        if turned { "first" } else { "last" },
        if turned { "against" } else { "along" },
        p.side.word()
    ));
    let path = feeds(&pts, p.plane_z, notes)?;
    // The same line the other way round: the certificate again, on the
    // moves as they are written.
    laws::kerf(&path, run, false, p.kerf_offset(), p.plane_z)?;
    Ok(path)
}

/// A closed profile: the whole lap, then on in the kerf to the path's
/// point nearest `exit`, and a straight lead-out to `exit`.
fn closed(base: Toolpath, loops: &[Loop], p: &WireParams, exit: Point2) -> Result<Toolpath, GenError> {
    let (given, profile) = ring::profile(loops, &[])?;
    let k = p.kerf_offset();
    let path_ring = ring::wire_ring(&profile, p.side, k)?;
    let Some(start) = p.start else {
        return Err(invalid("no start hole is stated".into()));
    };
    let (eseg, efoot) = ring::nearest(&path_ring, exit);
    let lead = dist(exit, efoot);
    if !(lead.is_finite() && lead > MIN_LEAD) {
        return Err(at(
            format!(
                "the exit point at X{:.3} Y{:.3} stands on the wire path itself - move it off the profile by more than the kerf offset ({k:.4} mm), or clear it",
                exit.x, exit.y
            ),
            exit,
            p.plane_z,
        ));
    }
    exit_side(&profile, &path_ring, exit, p)?;
    // The lap as it was cut, without its return to the threading point.
    let mut pts = points(&base);
    if pts.len() < 4 {
        return Err(invalid("the wire path lays down no cut".into()));
    }
    pts.pop();
    // On in the kerf, the lap's own way round, from the seam to the foot
    // of the exit.
    let (seg, foot) = ring::nearest(&path_ring, start);
    let n = path_ring.len();
    let ahead = eseg == seg && dist(path_ring[seg], efoot) >= dist(path_ring[seg], foot);
    if !ahead {
        let steps = (eseg + n - seg - 1) % n + 1;
        pts.extend((1..=steps).map(|j| path_ring[(seg + j) % n]));
    }
    pts.push(efoot);
    pts.push(exit);
    let mut notes = base.notes;
    resay(
        &mut notes,
        "wire EDM: threaded at",
        format!(
            "wire EDM: threaded at X{:.3} Y{:.3}; straight lead-in of {:.3} mm to the path's nearest point, the profile cut counter-clockwise all the way round, then on in its own kerf to the point nearest the exit point and a straight lead-out of {lead:.3} mm to the exit point at X{:.3} Y{:.3} - the wire does not return to the threading point",
            start.x,
            start.y,
            dist(start, foot),
            exit.x,
            exit.y
        ),
    );
    let path = feeds(&pts, p.plane_z, notes)?;
    // The certificate: no move, the lead-out included, brings the kerf
    // into the product.
    laws::kerf(&path, &given, true, k, p.plane_z)?;
    Ok(path)
}

/// The exit point must stand on the waste side, at least the kerf offset
/// from the profile - the start hole's law, said of the exit.
fn exit_side(profile: &Region, path_ring: &[Point2], exit: Point2, p: &WireParams) -> Result<(), GenError> {
    let k = p.kerf_offset();
    let inside_path = Region::new(vec![Loop::new(path_ring.to_vec())?])?.contains(exit);
    let inside_profile = profile.contains(exit);
    let why = match (p.side, inside_path, inside_profile) {
        (WireSide::Die, true, _) | (WireSide::Punch, false, _) => return Ok(()),
        (WireSide::Die, false, true) | (WireSide::Punch, true, false) => format!(
            "the exit point at X{:.3} Y{:.3} stands closer to the profile than the kerf offset ({k:.4} mm) - the lead-out would burn the {} there; move it away from the profile, or clear it",
            exit.x,
            exit.y,
            p.side.word()
        ),
        (WireSide::Die, false, false) => format!(
            "the exit point at X{:.3} Y{:.3} lies outside the profile - a die's wire leaves inside it, in the slug; the lead-out would burn through the die",
            exit.x, exit.y
        ),
        (WireSide::Punch, true, true) => format!(
            "the exit point at X{:.3} Y{:.3} lies inside the profile - a punch's wire leaves outside it; the lead-out would burn through the punch",
            exit.x, exit.y
        ),
        (side, _, _) => format!("the side is {} of travel - a closed profile states die or punch", side.word()),
    };
    Err(at(why, exit, p.plane_z))
}

#[cfg(test)]
#[path = "exit_tests.rs"]
mod tests;

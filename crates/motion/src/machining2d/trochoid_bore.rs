//! The trochoid lane's bore (issues #220, #1509): the helix that takes
//! the tool down to a level at the chain's start, and - for a loop band
//! wider than the tool - the spiral that opens that bore out to the
//! band. Helical entry is general machining practice (handbook ramping);
//! the opening is the two-centre semicircle spiral (successive half
//! turns about two centres half a pitch apart: each ends on the bore's
//! wall axis one half pitch further out), so it posts as G2/G3 arcs and
//! consecutive turns nest without crossing. Its pitch follows the lane's
//! forward step ([`PITCH_PER_STEP`] of it), so the measured-engagement
//! ladder that shortens the step tightens the spiral with it, and the
//! spiral is read by the same measurement as the loops.
//!
//! Why it exists: a helix of radius `b` clears the ring `b +- r` only.
//! With `b > r` a post of radius `b - r` is left standing at the bore's
//! centre, which no front arc ever reaches. The 2D Adaptive trochoidal
//! method always asks for the opened bore; the stand-alone slot asks for
//! it when `b > r` (#1589) and keeps the plain bore it always posted
//! otherwise. Ours, no vendor material.

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::face::GenError;
use super::trochoid_loops::{Layout, MAX_LOOPS};
use super::trochoid_slot::TrochoidParams;

/// An opened bore's helix radius, as a share of the tool radius: under
/// one radius the turns overlap at the centre and no post is left.
const HELIX_PER_R: f64 = 0.5;

/// The opening spiral's pitch, as a share of the lane's forward step:
/// a turn of the spiral is a tighter inside curve than a front arc, so
/// the same radial bite wraps more of the cutter (read on the 34 x 16
/// pin of `adaptive_troch`: at a full step the level read past 90
/// degrees and the ladder cut the step to 0.55 of it; at half, 86).
const PITCH_PER_STEP: f64 = 0.5;

/// The helix radius of the bore: the band's own half-width, or - opened
/// and wider than the tool can bore clean - [`HELIX_PER_R`] of the tool
/// radius.
fn helix_radius(lay: &Layout, p: &TrochoidParams, open: bool) -> f64 {
    let h = HELIX_PER_R * 0.5 * p.tool_diameter;
    if open && lay.b > h { h } else { lay.b }
}

/// The point of the bore's wall axis `v` from its centre (`-b` is the
/// layout's entry, `+b` the point opposite).
fn axis(lay: &Layout, v: f64) -> Point2 {
    let k = v / lay.b;
    Point2::new(lay.c0.x + (lay.opposite.x - lay.c0.x) * k, lay.c0.y + (lay.opposite.y - lay.c0.y) * k)
}

/// Where the bore's helix starts (and where the tool is positioned
/// above the level before it).
pub(super) fn start(lay: &Layout, p: &TrochoidParams, open: bool) -> Point2 {
    let h = helix_radius(lay, p, open);
    if h < lay.b {
        // On the path's lattice: every level's helix then rides the very
        // chords of the one above it (a start a rounding off would lay
        // near-coincident chords, which the crossing law reads as
        // crossings).
        let q = axis(lay, -h);
        Point2::new(lattice(q.x), lattice(q.y))
    } else {
        lay.entry
    }
}

/// The bore of one level at `z` from `from`: helix down, one floor
/// circle, then - opened - the spiral out to the band and the back half
/// of the band's circle. Ends on the layout's entry.
pub(super) fn bore(b: &mut PathBuilder, lay: &Layout, p: &TrochoidParams, z: f64, from: f64, open: bool) -> Result<(), GenError> {
    let p3 = |q: Point2, zz: f64| Point3::new(lattice(q.x), lattice(q.y), lattice(zz));
    let ccw = lay.ccw;
    let h = helix_radius(lay, p, open);
    let narrowed = h < lay.b;
    // Past the turn cap the bore is refused by name, never cut as one
    // turn standing in for the many the drop needs (#774).
    let Some(turns) = crate::links::helix_turns(from - z, h, 0.0) else {
        let (what, fix) = if narrowed {
            ("half the tool radius", "use a smaller stepdown")
        } else {
            ("half the loop band", "use a smaller stepdown or a wider loop band")
        };
        return Err(GenError::Invalid(format!(
            "trochoidal slot: the bore helix would need more than {} turns to descend {:.3} mm on a {:.3} mm helix radius ({what}) - {fix}",
            crate::links::MAX_HELIX_TURNS,
            from - z,
            h
        )));
    };
    let (near, far) = if narrowed { (axis(lay, -h), axis(lay, h)) } else { (lay.entry, lay.opposite) };
    let turns = turns.ceil().max(1.0);
    let halves = (2.0 * turns) as usize;
    for k in 1..=halves {
        let zz = from - (from - z) * k as f64 / halves as f64;
        let to = if k % 2 == 1 { far } else { near };
        b.push_arc(MoveKind::Feed, p3(to, zz), Some(p.plunge_feed), lay.c0, ccw);
    }
    b.push_arc(MoveKind::Feed, p3(far, z), Some(p.feed), lay.c0, ccw);
    b.push_arc(MoveKind::Feed, p3(near, z), Some(p.feed), lay.c0, ccw);
    if !narrowed {
        return Ok(());
    }
    // The opening. One radial step `d` off the floor circle (a tangent
    // departure would osculate the circle it leaves, which the crossing
    // law reads as a crossing), then half turn k from the axis point at
    // radius `h + k d` on one side to `h + (k + 1) d` on the other,
    // about the point midway between them; an odd count ends on the
    // point opposite the entry, and the back half of the band's own
    // circle closes on the entry (its front half is the last half
    // turn's, within `d`, where the loops cut next). A pitch too long
    // for the load is read by the lane's measurement, which then
    // shortens the step.
    let pitch = (PITCH_PER_STEP * lay.spacing).max(1e-6);
    let n = (((lay.b - h) / (0.5 * pitch)).ceil() as usize).clamp(2, MAX_LOOPS);
    let n = n + n % 2;
    let d = (lay.b - h) / n as f64;
    let mut v = -(h + d);
    b.push(MoveKind::Feed, p3(axis(lay, v), z), Some(p.feed));
    for k in 1..n {
        let next = if k + 1 == n { lay.b } else { h + (k + 1) as f64 * d } * -v.signum();
        b.push_arc(MoveKind::Feed, p3(axis(lay, next), z), Some(p.feed), axis(lay, 0.5 * (v + next)), ccw);
        v = next;
    }
    b.push_arc(MoveKind::Feed, p3(lay.entry, z), Some(p.feed), lay.c0, ccw);
    Ok(())
}

//! Stage C's pose law alone, read at one station's BALL CENTRE (split
//! out of `stamp.rs` at the AGENTS 400-line seam): the contact read,
//! the swarf law's ruled pose, the lean law - and, for the swarf
//! kinds only, the POSED fallback where neither speaks safely.
//!
//! The fallback is the "upright in the wall" finding. On the capped
//! blade the waterline rings under the equator turn off the tube's
//! end columns onto the caps, and there the ring's chords stand up to
//! three hundredths past the tangent distance: inside the plan's own
//! tolerance, outside the stamp's micron band. The stamp read air,
//! stated upright (the modal law's fallback for a program that has
//! leaned), and a 60 mm upright flute at the end column under the
//! equator stands 0.8 mm inside the tube - the verifier's conviction
//! at every link retract that inherited the pose. So a swarf stamp
//! reads contact to the PLAN's tolerance, and at a row the ruling
//! cannot read (the end columns' withheld claim, a cap, a rim edge)
//! it does not take the lean law's word blind: an overhang's normal
//! leans the spindle UNDER the horizon, so where the lean law's pose
//! dips, or stands its flank in the part, the station carries the
//! RULING instead - the region's own straight direction, level,
//! signed toward the air side of the contact where the normal says
//! which side that is (a cap face, a rim edge) and by the continuity
//! rule ([`ruled_sign`]) where it does not (the tube's end columns,
//! where the ruling lies in the flank). Either pose is stated only
//! with its flank proved in air ([`flank_clear`]); a row where no
//! pose clears refuses the path by name - never upright in the wall.
//!
//! Provenance: the lean law is `tilt`'s (ISO 10791-10 tradition); the
//! flank proof is the link law's own sweep. Our own design notes; no
//! vendor material consulted.

use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::retip::UP;
use super::stamp::Station;
use super::stamp_link::{flank_clear, ruled_sign, turn_deg, LINK_TURN_DEG, SIN_MIN};
use super::tilt::{command_axis, AxisControl};

/// The facet list, its AABB shells, and the whole shell's box (the
/// flank proof walks a pose until it leaves it).
pub(super) struct Mesh<'a> {
    pub(super) facets: &'a [Triangle],
    pub(super) boxes: &'a [[f64; 6]],
    pub(super) bounds: [f64; 6],
}

impl<'a> Mesh<'a> {
    pub(super) fn new(facets: &'a [Triangle], boxes: &'a [[f64; 6]]) -> Self {
        let mut bounds = [f64::INFINITY; 6];
        bounds[3..].fill(f64::NEG_INFINITY);
        for b in boxes {
            for i in 0..3 {
                bounds[i] = bounds[i].min(b[i]);
                bounds[i + 3] = bounds[i + 3].max(b[i + 3]);
            }
        }
        Self { facets, boxes, bounds }
    }
}

/// The swarf read of a stamp: the region's unit XY ruling, the
/// per-facet verdict of [`super::ruling::analyze`], and the plan's
/// tolerance - the band a swarf station is read as contact to.
#[derive(Clone, Copy)]
pub(super) struct Ruled<'a> {
    pub(super) dir: Point3,
    pub(super) ruled: &'a [bool],
    pub(super) tol: f64,
}

/// The law's read of one station: the pose (None in air and on a
/// level floor), the cone axis the shank escape tilts around (the
/// contact normal where there is one, vertical otherwise), and
/// whether the law refused to state a direction at all (vertical it
/// stays, and no escape invents an angle the law would not compute).
pub(super) type Read = (Option<[f64; 3]>, Point3, bool);

/// Stage C's pose law - the angle the surface commands before the
/// shank has an opinion, read at the station's BALL CENTRE; the swarf
/// kinds' fallback rides on it (module header).
#[allow(clippy::too_many_arguments)] // the law's flat read of one station
pub(super) fn pose_law(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Station],
    c: &AxisControl,
    ruling: Option<Ruled>,
    held: [f64; 3],
    last_lean: Option<[f64; 3]>,
) -> Result<Read, GenError> {
    let up = Point3::new(UP[0], UP[1], UP[2]);
    let p = s.to;
    let mut best: Option<(f64, Point3, usize)> = None;
    for (fi, (f, bx)) in mesh.facets.iter().zip(mesh.boxes.iter()).enumerate() {
        // Box distance: if the nearest gap already exceeds r + band,
        // no point of the box is in contact with the ball.
        let gx = (bx[0] - p.x).max(0.0).max(p.x - bx[3]);
        let gy = (bx[1] - p.y).max(0.0).max(p.y - bx[4]);
        let gz = (bx[2] - p.z).max(0.0).max(p.z - bx[5]);
        if (gx * gx + gy * gy + gz * gz).sqrt() > r + band {
            continue;
        }
        let q = closest_pt_tri(p, &[f.a, f.b, f.c]);
        let d2 = {
            let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
            dx * dx + dy * dy + dz * dz
        };
        if best.is_none_or(|(bd, _, _)| d2 < bd) {
            best = Some((d2, q, fi));
        }
    }
    // In air the swarf hold does not speak: a posed feed in air is read
    // by the verifier's posed contact law as a cut standing off the part.
    let Some((d2, q, fi)) = best else {
        return Ok((None, up, false));
    };
    let d = d2.sqrt();
    // Contact (or a bite - verify is the law that judges a bite, the
    // normal here is still the honest direction out of the surface).
    // A centre ON the surface has no direction out of it at all.
    if d > r + band || d <= 1e-9 {
        return Ok((None, up, false));
    }
    let n = Point3::new((p.x - q.x) / d, (p.y - q.y) / d, (p.z - q.z) / d);
    // A level floor: the contact stands straight under the centre,
    // there is no flank to lean off, and the honest pose for a floor
    // is the one it always had (upright).
    if 1.0 - n.z <= 1e-12 {
        return Ok((hold(s, mesh, r, ruling, held, None), up, false));
    }
    // The swarf law (M5.1 D): on a facet the ruling analysis marked
    // ruled, the tool lies WITH the blade - the pass direction
    // projected into the surface, tangent to the flank and normal to
    // nothing, which is what keeps the ball touching along a ruling.
    // A facet whose plane says almost nothing of the direction
    // (seen nearly head-on) is no flank at all, and the lean law of
    // stage C speaks for it instead.
    // A contact whose normal leans along the ruling (a rim or cap
    // EDGE under the ball, not the flank) holds no ruling at all:
    // the projection would turn with the edge as the ball rolls over
    // it, and the lean law speaks for an edge as it does for a cap.
    if let Some(rl) = ruling.filter(|rl| rl.ruled.get(fi).copied().unwrap_or(false)) {
        let dn = rl.dir.dot(n);
        let d3 = Point3::new(rl.dir.x - n.x * dn, rl.dir.y - n.y * dn, rl.dir.z - n.z * dn);
        let l = tl(d3);
        if l > SIN_MIN && dn.abs() <= SIN_MIN {
            let a = ruled_sign(Point3::new(d3.x / l, d3.y / l, d3.z / l), held, last_lean);
            let a = if a[2] < -1e-9 { level_ruling(s, mesh, r, rl, fi, held, last_lean).unwrap_or(a) } else { a };
            return Ok((Some(a), n, false));
        }
    }
    // The feed direction: this station's travel; a vertical entry has
    // none of its own, so the next station's states where the cut
    // goes. M8.3: a dive DEAD DOWN onto a flank has travel, but none
    // ALONG a cut that has not started - its own vector would lean the
    // lead inside the plunge plane. The cut the dive lands into is the
    // next station's, which is what the drive-curve family stamps its
    // first station with (curve.rs). A dive that lands on nothing (the
    // path ends here) keeps its own vector.
    let mut t = p - s.from;
    if tl(t) <= 1e-9 || (t.x * t.x + t.y * t.y <= 1e-18 && 1.0 - n.z * n.z > 1e-18) {
        t = rest
            .iter()
            .map(|m| m.to - p)
            .find(|&tt| tl(tt) > 1e-9)
            .unwrap_or(t);
    }
    // Nowhere to lead (the path ends here): hold the normal.
    let lean = if tl(t) <= 1e-9 { Some(n) } else { command_axis(n, t, c) };
    let Some(a) = lean else {
        // A control that states no direction (a NaN the sane door
        // somehow missed): hold vertical, say so - never invent an
        // angle the law refused to compute, and never let the escape
        // speak for a law that stayed silent.
        tracing::warn!("multiaxis: the axis law stated no direction at {p:?}; the move holds vertical");
        return Ok((None, up, true));
    };
    let a = [a.x, a.y, a.z];
    match ruling {
        None => Ok((Some(a), n, false)),
        Some(rl) => swarf_fallback(s, mesh, r, rl, n, a, held, last_lean)
            .map(|b| (hold(s, mesh, r, ruling, held, Some(b)).or(Some(b)), n, false)),
    }
}

/// The rim dip's cure (#17): a ruled contact read off a facet's EDGE
/// (the ball at a rim column, its contact normal leaning along the
/// ruling by up to the 10-degree band) projects the ruling into a
/// plane the facet does not lie in, and the pose dips under level by
/// as much. The facet's own plane holds the ruling exactly; where that
/// reading is level or above and its flank is proved in air
/// ([`flank_clear`]), it is the pose. None leaves the contact's
/// projection standing (still inside the band the laws allow).
#[allow(clippy::too_many_arguments)] // one station's flat read
fn level_ruling(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    rl: Ruled,
    fi: usize,
    held: [f64; 3],
    last_lean: Option<[f64; 3]>,
) -> Option<[f64; 3]> {
    let f = mesh.facets.get(fi)?;
    let fn_ = (f.b - f.a).cross(f.c - f.a);
    let ln = tl(fn_);
    if !(ln.is_finite() && ln > 1e-12) {
        return None;
    }
    let m = Point3::new(fn_.x / ln, fn_.y / ln, fn_.z / ln);
    let dm = rl.dir.dot(m);
    let d3 = Point3::new(rl.dir.x - m.x * dm, rl.dir.y - m.y * dm, rl.dir.z - m.z * dm);
    let l = tl(d3);
    if !(l.is_finite() && l > SIN_MIN) {
        return None;
    }
    let a = ruled_sign(Point3::new(d3.x / l, d3.y / l, d3.z / l), held, last_lean);
    (a[2] >= -1e-9 && flank_clear(s.to, a, r, rl.tol, mesh.facets, mesh.boxes, &mesh.bounds)).then_some(a)
}

/// The swarf hold (#17): wherever the ball is in contact but the ruling
/// does not speak (a level floor, a rim or cap edge under the ball -
/// every contact station the lean law answers), a pose that would turn past
/// the link threshold off the pose held coming in keeps the held pose
/// instead, when that pose is a lean (not upright), not under level,
/// and its flank is proved in air at this station ([`flank_clear`]).
/// The ball cuts the same surface under any pose about its centre, so
/// the contact is unchanged; only the flank could differ, and it is
/// proved. What it saves is the rim station's turn at a raster's ends
/// and the cap rings' turns off the lying pose. Never in air: a posed
/// feed off the part is read by the verifier's posed contact law as a
/// cut standing off its surface (the app blade's 14.8 mm Envelope rest
/// when the hold rode the exit rise; that rise now runs as a rapid
/// under the held pose instead, `super::stamp_rise`, #48). None where the held
/// pose does not qualify, or the law's own pose is within the
/// threshold: the law's pose stands.
fn hold(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    ruling: Option<Ruled>,
    held: [f64; 3],
    law: Option<[f64; 3]>,
) -> Option<[f64; 3]> {
    let rl = ruling?;
    if held[2] >= 1.0 - 1e-12 || held[2] < -1e-6 {
        return None;
    }
    if turn_deg(held, law.unwrap_or(UP)) <= LINK_TURN_DEG {
        return None;
    }
    flank_clear(s.to, held, r, rl.tol, mesh.facets, mesh.boxes, &mesh.bounds).then_some(held)
}

/// The swarf kinds' second opinion on a lean-law pose (module
/// header): kept where it stays above the horizon with its flank in
/// air; the ruling where it would dip under (an overhang's normal) or
/// stand in the part - toward the air side of the contact where the
/// normal names one, else the continuous end first and the other
/// after it; refused by name where no pose clears.
#[allow(clippy::too_many_arguments)] // one station's flat read
fn swarf_fallback(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    rl: Ruled,
    n: Point3,
    a: [f64; 3],
    held: [f64; 3],
    last_lean: Option<[f64; 3]>,
) -> Result<[f64; 3], GenError> {
    let clear = |b: [f64; 3]| flank_clear(s.to, b, r, rl.tol, mesh.facets, mesh.boxes, &mesh.bounds);
    if a[2] >= 0.0 && clear(a) {
        return Ok(a);
    }
    let d = rl.dir;
    let dn = d.dot(n);
    let (first, second) = if dn > SIN_MIN {
        ([d.x, d.y, d.z], None)
    } else if dn < -SIN_MIN {
        ([-d.x, -d.y, -d.z], None)
    } else {
        let e = ruled_sign(d, held, last_lean);
        (e, Some([-e[0], -e[1], -e[2]]))
    };
    if let Some(b) = std::iter::once(first).chain(second).find(|&b| clear(b)) {
        return Ok(b);
    }
    Err(GenError::Invalid(format!(
        "swarf pose at move {} (ball centre {:.2}, {:.2}, {:.2}): the lean law's pose \
         [{:.3} {:.3} {:.3}] {}, and the ruling [{:.3} {:.3} {:.3}] stands in the part - \
         no pose clears this row; refused: a swarf tool never stands upright in the wall",
        s.mv,
        s.to.x,
        s.to.y,
        s.to.z,
        a[0],
        a[1],
        a[2],
        if a[2] < 0.0 { "points under the horizon" } else { "stands in the part" },
        first[0],
        first[1],
        first[2]
    )))
}

/// The length of a difference vector, spelled once for the law.
fn tl(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

//! Helix entries of a flat Circular's rings kept apart (#3109).
//!
//! Provenance: our own plane geometry - two circles meet when the
//! distance of their centers lies between the difference and the sum
//! of their radii (Euclid, elementary) - and the crossing law of
//! `verify::crossing`. No vendor material consulted.
//!
//! A flat Circular runs every ring down the same level ladder, so the
//! helix of each ring at a level shares its depth band with the helix
//! of every other ring at that level. The crossing law refuses two
//! descents that cross in XY in a shared band.
//!
//! The kernel turns the start of each later ring to a vertex of its own
//! polygon where the predicted helix orbit stays clear of the orbits
//! already flown ([`ring_phase`]). A helix makes a full turn at least,
//! so a clear orbit circle is a clear helix. Then the kernel reads the
//! descents that the ring really flew with the crossing law itself and
//! refuses the ring by name if one crosses an earlier ring's descent
//! ([`Descents::add`]) - the prediction picks the start, the law proves
//! it.

use crate::cl::{ArcPlane, Move, MoveKind, Toolpath, lattice};
use crate::entry::disc_clears;
use crate::geo2d::Point2;
use crate::ring_zig::MIN_ARC_CHORD;
use crate::verify::crossed_pairs;

use super::levels::Cert;

/// An orbit this near the helix radius is a helix orbit.
const TOUCH: f64 = 1e-6;

/// The clear gap a predicted orbit keeps from every orbit flown, so a
/// lattice snap or a chord of the crossing law cannot close it.
const GAP: f64 = 0.01;

/// The chord tolerances the reading runs the crossing law at: the
/// common verify tolerance and a finer one, so a pair that one chord
/// size misses near a touch is still read.
const READ_TOLS: [f64; 2] = [0.01, 0.001];

/// The most start vertices tried for one ring.
const MAX_TRIES: usize = 64;

/// Do two orbit circles meet (cross or touch) within `tol`?
fn circles_meet(c0: Point2, r0: f64, c1: Point2, r1: f64, tol: f64) -> bool {
    let d = (c0.x - c1.x).hypot(c0.y - c1.y);
    d <= r0 + r1 + tol && d >= (r0 - r1).abs() - tol
}

/// The descents among `moves`: each feed whose Z changes (a helix arc,
/// a ramp chord) - the only moves of one ring that can cross a move of
/// another ring at a shared depth band.
pub(super) fn descents(moves: &[Move]) -> Vec<Move> {
    moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z != m.to.z).cloned().collect()
}

/// A descent's XY box (an arc's whole orbit) and its depth band.
struct Bounds {
    lo: Point2,
    hi: Point2,
    z: (f64, f64),
}

impl Bounds {
    fn of(m: &Move) -> Self {
        let z = (m.from.z.min(m.to.z), m.from.z.max(m.to.z));
        match m.arc {
            Some(a) => {
                let r = (m.from.x - a.center.x).hypot(m.from.y - a.center.y);
                let (c, r) = (a.center, r + GAP);
                Bounds { lo: Point2::new(c.x - r, c.y - r), hi: Point2::new(c.x + r, c.y + r), z }
            }
            None => Bounds {
                lo: Point2::new(m.from.x.min(m.to.x) - GAP, m.from.y.min(m.to.y) - GAP),
                hi: Point2::new(m.from.x.max(m.to.x) + GAP, m.from.y.max(m.to.y) + GAP),
                z,
            },
        }
    }

    /// Boxes that overlap in XY and bands that share more than an end
    /// (the crossing law's band test). A non-finite box always meets.
    fn meets(&self, o: &Bounds) -> bool {
        let finite = [self.lo.x, self.lo.y, self.hi.x, self.hi.y, o.lo.x, o.lo.y, o.hi.x, o.hi.y]
            .iter()
            .all(|v| v.is_finite());
        !finite
            || (self.z.1.min(o.z.1) - self.z.0.max(o.z.0) > 0.0
                && self.lo.x <= o.hi.x
                && o.lo.x <= self.hi.x
                && self.lo.y <= o.hi.y
                && o.lo.y <= self.hi.y)
    }
}

/// The descents flown so far, ring by ring.
#[derive(Default)]
pub(super) struct Descents {
    moves: Vec<Move>,
}

impl Descents {
    /// File the descents of one ring. `Err` names the first pair in
    /// which a descent of this ring crosses a descent of an earlier ring
    /// at a shared depth - the program verification would refuse.
    pub(super) fn add(&mut self, ring: Vec<Move>) -> Result<(), String> {
        // Only an earlier descent whose band and box meet one of this
        // ring's can cross it; the law reads those and the ring.
        let boxes: Vec<Bounds> = ring.iter().map(Bounds::of).collect();
        let mut moves: Vec<Move> =
            self.moves.iter().filter(|m| boxes.iter().any(|b| b.meets(&Bounds::of(m)))).cloned().collect();
        let old = moves.len();
        moves.extend(ring.iter().cloned());
        let path = Toolpath::new(moves);
        let hit = READ_TOLS
            .iter()
            .flat_map(|&tol| crossed_pairs(&path, tol))
            .find(|&[a, b]| a < old && b >= old);
        let Some([a, b]) = hit else {
            self.moves.extend(ring);
            return Ok(());
        };
        let at = |i: usize| {
            path.moves
                .get(i)
                .map_or(Point2::new(f64::NAN, f64::NAN), |m| m.arc.map_or(Point2::new(m.to.x, m.to.y), |a| a.center))
        };
        let (n, o) = (at(b), at(a));
        Err(format!(
            "helix entry: the descent at X{:.3} Y{:.3} crosses the descent of an earlier ring at X{:.3} Y{:.3} \
             at a shared depth - use a smaller helix radius, a wider stepover, or a plunge or ramp entry",
            n.x, n.y, o.x, o.y
        ))
    }

    /// The helix orbits flown so far, each once: the XY arcs of the
    /// descents no wider than the helix radius.
    fn orbits(&self, helix_r: f64) -> Vec<(Point2, f64)> {
        let mut out: Vec<(Point2, f64)> = Vec::new();
        for m in &self.moves {
            let Some(a) = m.arc.filter(|a| a.plane == ArcPlane::Xy) else {
                continue;
            };
            let r = (m.from.x - a.center.x).hypot(m.from.y - a.center.y);
            if r <= helix_r + TOUCH && !out.iter().any(|&(c, q)| c == a.center && q == r) {
                out.push((a.center, r));
            }
        }
        out
    }
}

/// An internally tangent orbit in a Circular ring's disk. Test the actual
/// near and far points of the two posted half-turns after lattice rounding.
pub(super) fn tangent_center(axis: Point2, radius: f64, q: Point2, r: f64) -> Option<Point2> {
    let dx = q.x - axis.x;
    let dy = q.y - axis.y;
    let d = dx.hypot(dy);
    if d <= 0.0 || r <= 0.0 || r > radius {
        return None;
    }
    let c = Point2::new(lattice(q.x - r * dx / d), lattice(q.y - r * dy / d));
    let far = Point2::new(lattice(2.0 * c.x - q.x), lattice(2.0 * c.y - q.y));
    let near = Point2::new(lattice(q.x), lattice(q.y));
    // The first and last half-turn meet q. Intermediate half-turns meet
    // the lattice-snapped near point. Either chord can round shut at post.
    if ![q, near].iter().all(|p| (p.x - far.x).hypot(p.y - far.y) >= MIN_ARC_CHORD) {
        return None;
    }
    let orbit = [q, near, far].iter().map(|p| (p.x - c.x).hypot(p.y - c.y)).fold(0.0, f64::max);
    ((c.x - axis.x).hypot(c.y - axis.y) + orbit <= radius + 1e-10).then_some(c)
}

/// The center the descent places a helix of radius `r` at for a pass
/// that starts at `q`: the first of the eight compass offsets whose
/// disc the room holds (`descent::plan_helix`'s first try). `None`
/// when no offset fits - the descent then looks elsewhere.
fn predicted_center(cert: &Cert, q: Point2, r: f64) -> Option<Point2> {
    if let Cert::RingDisk { center, radius, .. } = cert {
        return tangent_center(*center, *radius, q, r);
    }
    let room = cert.room();
    (0..8).find_map(|k| {
        let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
        let c = Point2::new(lattice(q.x + r * cs), lattice(q.y + r * sn));
        disc_clears(room, c, r + 1e-3).then_some(c)
    })
}

/// The start vertex (an index into a ring polygon of `n` chords) whose
/// predicted helix orbit keeps [`GAP`] from every orbit flown. Tried in
/// halving order (0, n/2, n/4, 3n/4, ...), so the first ring and a ring
/// with no neighbour keep vertex 0 - the start every earlier Circular
/// had. `vertex(i)` is the polygon's vertex `i`. When none is clear,
/// vertex 0: the reading after the ring then refuses it by name.
pub(super) fn ring_phase(
    n: usize,
    vertex: impl Fn(usize) -> Point2,
    room: &Cert,
    helix_r: f64,
    flown: &Descents,
) -> usize {
    let orbits = flown.orbits(helix_r);
    if orbits.is_empty() || n == 0 {
        return 0;
    }
    let tries = n.min(MAX_TRIES);
    let stride = n / tries;
    halving_order(tries)
        .into_iter()
        .map(|i| i * stride)
        .find(|&i| {
            predicted_center(room, vertex(i), helix_r)
                .is_some_and(|c| orbits.iter().all(|&(o, r)| !circles_meet(c, helix_r, o, r, GAP)))
        })
        .unwrap_or(0)
}

/// `0..n` (n a power of two) in halving order: 0, n/2, n/4, 3n/4, n/8,
/// 3n/8, ... - each next start as far from the ones tried as can be.
fn halving_order(n: usize) -> Vec<usize> {
    let mut out = vec![0];
    let mut step = n;
    while step > 1 {
        let half = step / 2;
        let mut i = half;
        while i < n {
            out.push(i);
            i += step;
        }
        step = half;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::ArcMove;
    use crate::geometry::Point3;

    #[test]
    fn halving_order_visits_each_index_once() {
        assert_eq!(halving_order(8), vec![0, 4, 2, 6, 1, 3, 5, 7]);
        assert_eq!(halving_order(1), vec![0]);
        let mut all = halving_order(64);
        all.sort_unstable();
        assert_eq!(all, (0..64).collect::<Vec<_>>());
    }

    /// Two equal orbits two radii apart touch; a gap clears them; one
    /// orbit inside another, apart, does not meet it.
    #[test]
    fn orbits_meet_by_their_circles() {
        let o = Point2::new(0.0, 0.0);
        assert!(circles_meet(o, 1.0, Point2::new(2.0, 0.0), 1.0, TOUCH));
        assert!(!circles_meet(o, 1.0, Point2::new(2.0 + GAP, 0.0), 1.0, TOUCH));
        assert!(!circles_meet(o, 3.0, Point2::new(0.5, 0.0), 1.0, TOUCH));
    }

    /// One full helix turn about `c` from `z0` to `z1`, two half turns.
    fn helix(c: Point2, z0: f64, z1: f64) -> Vec<Move> {
        let arc = |from: Point3, to: Point3| Move {
            arc: Some(ArcMove { center: c, ccw: true, plane: ArcPlane::Xy }),
            ..Move::new(MoveKind::Feed, from, to, Some(100.0))
        };
        let zm = 0.5 * (z0 + z1);
        vec![
            arc(Point3::new(c.x - 1.0, c.y, z0), Point3::new(c.x + 1.0, c.y, zm)),
            arc(Point3::new(c.x + 1.0, c.y, zm), Point3::new(c.x - 1.0, c.y, z1)),
        ]
    }

    /// The reading refuses a ring whose helix crosses an earlier ring's
    /// helix in a shared band, and files one in the next band down; a
    /// planar move is no descent.
    #[test]
    fn the_reading_refuses_a_crossing_in_a_shared_band_only() {
        let mut flown = Descents::default();
        assert!(flown.add(descents(&helix(Point2::new(0.0, 0.0), 2.0, 1.0))).is_ok());
        assert!(flown.add(descents(&helix(Point2::new(0.0, 0.0), 1.0, 0.0))).is_ok(), "next band");
        // Off the chord lattice, so the two orbits cross between vertices.
        let why = flown.add(descents(&helix(Point2::new(0.93, 0.07), 2.0, 1.0))).err().unwrap_or_default();
        assert!(why.starts_with("helix entry:"), "{why}");
        let flat = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 1.0), Point3::new(5.0, 0.0, 1.0), Some(1.0));
        assert!(descents(&[flat]).is_empty());
        assert_eq!(flown.orbits(1.0).len(), 1, "one orbit filed (the refused ring is not)");
    }
}

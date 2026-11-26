//! Per-ruling widening of a 4-axis wire cut (#1170, on #537's laws).
//! The uniform widening holds the whole wire path off both profiles by
//! what the worst ruling needs. Here each ruling end takes its own:
//!
//! - The wire's rulings are laid over the parallels at the kerf offset,
//!   as `ruled` lays them. Each end has an offset vector: from the
//!   profile's nearest point to the end (at a mitred corner the corner's
//!   diagonal; where several edges are equally near, the direction that
//!   moves off all of them at once).
//! - Each end has a scale, 1 at the start: the end stands at its
//!   parallel's point plus `(scale - 1)` of the vector. Ends that share
//!   one point (a fan's apex) share one scale, so the apex stays a point.
//! - The ruled kerf law reads every ruling's own need / clearance. An
//!   end whose reading is short grows by that ratio; the law is read
//!   again until every reading holds. Scales only grow, so the wire
//!   never comes nearer than a pass that already held.
//! - A long span between two rulings gets a transition ruling one
//!   [`TRANSITION`] in from each end, on the same patch: the widening of
//!   a twisted corner then ends there, and the straight stretch beyond
//!   stands at the kerf offset. Transitions that end on the widened
//!   chord (a constant widening) are dropped again.
//!
//! The certificate is the law itself: the path leaves only when the
//! ruled kerf law holds at every height on the wire that is emitted.
//! Refused by name: a ruling that needs more than the tolerance, and a
//! widened path that crosses or turns back on itself (neighbouring
//! widenings too far apart to level).
//!
//! Provenance: the kerf readings are `ruled_laws` (do Carmo s. 3-5; Liu,
//! CAD 27(12), 1995 - the tangent plane of a non-developable ruled
//! surface turns along a ruling, by an amount that differs from ruling
//! to ruling); the scaling is plain fixed-point iteration on our own
//! readings. Our own code; no vendor post, control manual or CAM
//! product behaviour was consulted.

use super::laws::UncutRest;
use super::pairing::Ring;
use super::ring::{self, dist, pt_seg};
use super::ruled::{emit, invalid, lead_in, too_far, CutReport, WireRuled, SETTLED};
use super::ruled_laws::{clearance, crossing, fold_of, folds, foot, lean_law, lerp, tangle, Ruling, MAX_RULINGS};
use super::{laws, preimage, start_side, WireParams, MIN_LEAD};
use crate::cl::Toolpath;
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// The passes before the widening is called unsettled.
const PASSES: usize = 24;
/// Feet of one point within this (mm) of its nearest are equally near:
/// five steps of the offsetter's lattice.
const FEET_TIE: f64 = 5e-6;
/// How far (mm) in from each end of a long span a transition ruling is
/// laid; a span under three of them gets none.
const TRANSITION: f64 = 1.0;
/// Two ends nearer than this (mm) are one point.
const SAME: f64 = 1e-9;
/// A transition within this (mm) of its span's widened chord is idle.
const IDLE: f64 = 1e-6;
/// A ruling widened by no more than this (mm) stands at the kerf offset,
/// as the note counts it.
const AT_OFFSET: f64 = 1e-4;

/// What `ruled` resolved before the widening: the numbers, the start
/// hole, the machine's maximum lean, the stated surface and its anchors.
pub(super) struct Stated<'a> {
    pub(super) p: &'a WireParams,
    pub(super) r: &'a WireRuled,
    pub(super) start: Point2,
    pub(super) max: f64,
    pub(super) product: &'a [Ruling],
    pub(super) anchors: &'a [(Point2, Point2, usize)],
}

/// The direction and length that moves `q` off the profile `given` the
/// way a wider offset would: `q` less its nearest point, or - several
/// edges equally near - the mean of their normals, long enough that `q`
/// leaves every one of them by its own distance.
fn away(q: Point2, given: &[Point2]) -> (f64, f64) {
    let n = given.len();
    let feet: Vec<(f64, Point2)> = (0..n)
        .map(|i| {
            let f = foot(q, given[i], given[(i + 1) % n]).1;
            (dist(q, f), f)
        })
        .collect();
    let near = feet.iter().map(|f| f.0).fold(f64::INFINITY, f64::min);
    if !(near.is_finite() && near > 0.0) {
        return (0.0, 0.0);
    }
    let normals: Vec<(f64, f64)> =
        feet.iter().filter(|(d, _)| *d <= near + FEET_TIE).map(|(d, f)| ((q.x - f.x) / d, (q.y - f.y) / d)).collect();
    let (mx, my) = normals.iter().fold((0.0, 0.0), |m, v| (m.0 + v.0, m.1 + v.1));
    let len = mx.hypot(my);
    let (mx, my) = (mx / len, my / len);
    let low = normals.iter().map(|v| v.0 * mx + v.1 * my).fold(f64::INFINITY, f64::min);
    if low.is_finite() && low > 0.2 {
        return (mx * near / low, my * near / low);
    }
    // Normals too far apart to average: the first nearest foot's.
    feet.iter().find(|f| f.0 <= near).map_or((0.0, 0.0), |(_, f)| (q.x - f.x, q.y - f.y))
}

/// One plane's ends of the rulings: where each stands on the parallel
/// at the kerf offset, its group (ends sharing one point), and each
/// group's offset vector and scale.
struct Ends {
    base: Vec<Point2>,
    group: Vec<usize>,
    off: Vec<(f64, f64)>,
    scale: Vec<f64>,
}

impl Ends {
    fn new(base: Vec<Point2>, given: &[Point2], healed: Option<(&[Point2], f64, super::WireSide)>) -> Self {
        let mut group = Vec::with_capacity(base.len());
        let mut off: Vec<(f64, f64)> = Vec::new();
        for (i, q) in base.iter().enumerate() {
            if i == 0 || off.is_empty() || dist(base[i - 1], *q) > SAME {
                off.push(healed.map_or_else(|| away(*q, given), |(path, k, side)| preimage::surviving_away(*q, path, k, side)));
            }
            group.push(off.len().saturating_sub(1));
        }
        // The ring closes: a fan may straddle the seam.
        let last = off.len().saturating_sub(1);
        if last > 0 && base.first().zip(base.last()).is_some_and(|(a, b)| dist(*a, *b) <= SAME) {
            group.iter_mut().filter(|g| **g == last).for_each(|g| *g = 0);
            off.pop();
        }
        Ends { base, group, scale: vec![1.0; off.len()], off }
    }

    /// Ruling `i`'s end as widened.
    fn at(&self, i: usize) -> Point2 {
        let g = self.group[i];
        let (s, o) = (self.scale[g] - 1.0, self.off[g]);
        Point2::new(self.base[i].x + s * o.0, self.base[i].y + s * o.1)
    }

    fn scale_of(&self, i: usize) -> f64 {
        self.scale[self.group[i]]
    }

    /// Give rulings `i` and `j` the wider of their two scales; whether
    /// anything changed.
    fn level(&mut self, i: usize, j: usize) -> bool {
        let (a, b) = (self.group[i], self.group[j]);
        let wide = self.scale[a].max(self.scale[b]);
        let changed = self.scale[a] < wide || self.scale[b] < wide;
        self.scale[a] = wide;
        self.scale[b] = wide;
        changed
    }

    fn widest(&self) -> f64 {
        self.scale.iter().copied().fold(1.0, f64::max)
    }
}

/// The rulings with a transition one [`TRANSITION`] in from each end of
/// every long span; `Some(t)` marks one, laid at fraction `t` of its span.
fn transitions(base: &[Ruling]) -> Vec<(Ruling, Option<f64>)> {
    let n = base.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (base[i], base[(i + 1) % n]);
        out.push((a, None));
        let len = dist(a.0, b.0).max(dist(a.1, b.1));
        if len.is_finite() && len > 3.0 * TRANSITION {
            for t in [TRANSITION / len, 1.0 - TRANSITION / len] {
                out.push(((lerp(a.0, b.0, t), lerp(a.1, b.1, t)), Some(t)));
            }
        }
    }
    out
}

/// Which rulings stay: a span's transitions go when each ends on the
/// widened chords between the span's own rulings. The caller reads the
/// kerf law again on what remains.
fn kept(laid: &[(Ruling, Option<f64>)], wire: &[Ruling]) -> Vec<bool> {
    let n = laid.len();
    let mut keep = vec![true; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && laid[j].1.is_some() {
            j += 1;
        }
        let (a, b) = (wire[i], wire[j % n]);
        let idle = |m: usize| pt_seg(wire[m].0, a.0, b.0) <= IDLE && pt_seg(wire[m].1, a.1, b.1) <= IDLE;
        if (i + 1..j).all(idle) {
            keep[i + 1..j].fill(false);
        }
        i = j;
    }
    keep
}

/// Whether the widened step `w0 -> w1` runs against the step `b0 -> b1`
/// it was laid as.
fn reversed(b0: Point2, b1: Point2, w0: Point2, w1: Point2) -> bool {
    (b1.x - b0.x) * (w1.x - w0.x) + (b1.y - b0.y) * (w1.y - w0.y) < 0.0
}

fn tangled(q: Point2, z: f64) -> GenError {
    GenError::invalid_at(
        format!(
            "wire contour: the per-ruling widening makes the wire path cross or turn back on itself near X{:.3} Y{:.3} on Z{z:.3} - neighbouring rulings need widenings too far apart there; state the uniform widening, or restate the pairing",
            q.x, q.y
        ),
        Point3::new(q.x, q.y, z),
    )
}

/// The closed ring of `pts`, coincident neighbours dropped.
fn simple(pts: impl Iterator<Item = Point2>) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::new();
    for q in pts {
        if out.last().is_none_or(|l| dist(*l, q) > SAME) {
            out.push(q);
        }
    }
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| dist(*a, *b) <= SAME) {
        out.pop();
    }
    out
}

/// Cut the stated surface with every ruling widened by its own need.
/// `planes` is the `[lower, upper]` profile as given and its region.
///
/// # Errors
/// Every refusal the module head and `ruled` list, by name.
pub(super) fn per_ruling(c: &Stated<'_>, planes: [(&[Point2], &Region); 2]) -> Result<Toolpath, GenError> {
    let (p, r, start) = (c.p, c.r, c.start);
    let (z0, h, k) = (p.plane_z, r.top_z - p.plane_z, p.kerf_offset());
    let (rule, hint) = (r.pairing.word(), r.pairing.hint());
    let [(given_l, region_l), (given_u, region_u)] = planes;
    let scaled = |z: f64, s: f64| WireParams { wire_diameter: p.wire_diameter * s, gap: p.gap * s, plane_z: z, ..p.clone() };
    let ring_u = ring::wire_ring(region_u, p.side, k)?;
    let ring_l = ring::wire_ring(region_l, p.side, k)?;
    let rest_u = laws::cover_healed(given_u, &ring_u, &scaled(r.top_z, 1.0), r.heal_tolerance)?;
    let rest_l = laws::cover_healed(given_l, &ring_l, &scaled(z0, 1.0), r.heal_tolerance)?;
    let mut uncut = UncutRest::worse(rest_l, rest_u);
    let (Some(wu), Some(wl)) = (Ring::new(&ring_u), Ring::new(&ring_l)) else {
        return Err(invalid("the wire path has collapsed - the profile is too small for the wire".into()));
    };
    let sewn = preimage::sew(&wu, &wl, [given_l, given_u], c.anchors, start)?;
    let laid = transitions(&sewn.wire);
    let n = laid.len();
    if n > MAX_RULINGS {
        return Err(invalid(format!("the pairing lays {n} rulings, more than {MAX_RULINGS} - simplify the profiles")));
    }
    let base: Vec<Ruling> = laid.iter().map(|l| l.0).collect();
    folds(&base, z0, h, &rule, hint)?;
    let mut lower = Ends::new(base.iter().map(|b| b.0).collect(), given_l, sewn.healed[0].then_some((&ring_l, k, p.side)));
    let mut upper = Ends::new(base.iter().map(|b| b.1).collect(), given_u, sewn.healed[1].then_some((&ring_u, k, p.side)));
    for _ in 0..PASSES {
        let wire: Vec<Ruling> = (0..n).map(|i| (lower.at(i), upper.at(i))).collect();
        // Unequal widenings must not turn a step back: level the two.
        let (mut levelled, mut back) = (false, None);
        for i in 0..n {
            let j = (i + 1) % n;
            let turned = fold_of(wire[i], wire[j]).is_some()
                || reversed(base[i].0, base[j].0, wire[i].0, wire[j].0)
                || reversed(base[i].1, base[j].1, wire[i].1, wire[j].1);
            if turned {
                let changed = [lower.level(i, j), upper.level(i, j)].contains(&true);
                levelled |= changed;
                back = back.or((!changed).then_some(wire[i].0));
            }
        }
        if levelled {
            continue;
        }
        folds(&wire, z0, h, &rule, hint)?;
        if let Some(q) = back {
            return Err(tangled(q, z0));
        }
        lean_law(&wire, z0, h, c.max, &rule, &r.machine)?;
        let read = clearance(c.product, &wire, start, k, z0, h);
        let (mut grown, mut worst) = (false, None::<(f64, Point3)>);
        for (e, (ends, z)) in [(&mut lower, z0), (&mut upper, r.top_z)].into_iter().enumerate() {
            let mut need = vec![1.0f64; ends.scale.len()];
            for (i, per) in read.per.iter().enumerate() {
                let g = ends.group[i];
                need[g] = need[g].max(per[e]);
                let standing = k * (ends.scale[g] * need[g] - 1.0);
                let over = !need[g].is_finite() || standing > r.tolerance + 1e-9;
                if over && worst.is_none_or(|(w, _)| standing > w) {
                    let q = ends.at(i);
                    worst = Some((standing, Point3::new(q.x, q.y, z)));
                }
            }
            for (scale, need) in ends.scale.iter_mut().zip(need) {
                if need.is_finite() && need > 1.0 + SETTLED {
                    *scale *= need * (1.0 + SETTLED * 0.5);
                    grown = true;
                }
            }
        }
        if let Some((standing, at)) = worst {
            return Err(too_far(r, at, standing));
        }
        // The law as a whole, not only its per-ruling shares.
        if grown || read.ratio.is_nan() || read.ratio > 1.0 + SETTLED {
            continue;
        }
        // Settled. Drop the transitions a constant widening left idle,
        // if the law still holds on what remains.
        let mut keep = kept(&laid, &wire);
        let slim: Vec<Ruling> = wire.iter().zip(&keep).filter(|(_, k)| **k).map(|(w, _)| *w).collect();
        let (mut wire, mut read) = (wire, read);
        if slim.len() < n {
            let again = clearance(c.product, &slim, start, k, z0, h);
            if again.ratio <= 1.0 + SETTLED {
                (wire, read) = (slim, again);
            } else {
                keep = vec![true; n];
            }
        }
        let steepest = lean_law(&wire, z0, h, c.max, &rule, &r.machine)?;
        crossing(&wire, z0, h)?;
        let Some(first) = wire.first().copied() else {
            return Err(invalid("the wire path lays down no cut".into()));
        };
        let rings = [simple(wire.iter().map(|w| w.0)), simple(wire.iter().map(|w| w.1))];
        let on = [(given_l, region_l, z0, lower.widest()), (given_u, region_u, r.top_z, upper.widest())];
        for (pts, (given, _, z, wide)) in rings.iter().zip(on) {
            if let Some(q) = tangle(pts) {
                return Err(tangled(q, z));
            }
            let after = laws::cover_healed(given, pts, &scaled(z, wide), r.heal_tolerance)?;
            uncut = UncutRest::worse(uncut, after);
        }
        let lead = dist(start, first.0);
        if !(lead.is_finite() && lead > MIN_LEAD) {
            return Err(GenError::invalid_at(
                format!(
                    "wire contour: the start hole at X{:.3} Y{:.3} stands on the wire path itself - move it off the lower profile by more than the kerf offset ({:.4} mm)",
                    start.x,
                    start.y,
                    k * lower.scale_of(0)
                ),
                Point3::new(start.x, start.y, z0),
            ));
        }
        for (pts, (_, region, z, wide)) in rings.iter().zip(on).rev() {
            start_side(region, pts, start, &scaled(z, wide))?;
        }
        if let Some(lead) = read.lead {
            return Err(lead_in(lead));
        }
        let (mut most, mut spot, mut plain) = (0.0f64, first.0, 0usize);
        for i in (0..n).filter(|i| keep[*i]) {
            let more = k * (lower.scale_of(i).max(upper.scale_of(i)) - 1.0);
            if more > most {
                (most, spot) = (more, lower.at(i));
            }
            plain += usize::from(more <= AT_OFFSET);
        }
        let held = format!(
            "wire EDM: 4-axis - each ruling is held off both profiles by the kerf offset {k:.4} mm and what its own twist and lean need: at most {most:.4} mm more, near X{:.3} Y{:.3} (tolerance {:.4}), which stands on the profile planes; {plain} of {} rulings stand within {AT_OFFSET:.4} mm of the kerf offset - a straight leaning wire cannot touch a twisted ruled surface along its whole height, and the kerf is kept out of it at every height",
            spot.x,
            spot.y,
            r.tolerance,
            wire.len()
        );
        return emit(&wire, p, r, CutReport { start, lead, steepest, held, given_l, given_u, uncut });
    }
    Err(invalid(format!("the wire's offset from the paired surface ({rule}) did not settle - refused")))
}

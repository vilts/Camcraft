//! The point physics of the crease pair (M2.5 B): does a ball center
//! REST in a corner, does a locus segment survive the resting test,
//! and how resting segments weld into runs. The laws - concavity,
//! the locus, the band, the chain - are stated in [`super::pencil`];
//! this file is their arithmetic. Provenance as there: published
//! ball-end corner kinematics, our own construction.

use crate::geometry::{Point3, Triangle};

use super::params::Finish3DParams;
use super::grid::SupSet;
use super::support::{dist2_pt_tri};

/// The model's float skin: welded vertices agree to this. The weld
/// scale every chain in this module family keys on - lifted from the
/// crease pair's two private copies (M2.5 D3) so the drape's boundary
/// chain and the loci's locus chain weld the SAME mesh the same way.
pub(crate) fn vkey_scale(facets: &[Triangle]) -> f64 {
    let mut m = 1.0f64;
    for f in facets {
        for q in [f.a, f.b, f.c] {
            m = m.max(q.x.abs()).max(q.y.abs()).max(q.z.abs());
        }
    }
    (m * 1e-9).max(1e-9)
}

/// The weld key: a vertex's lattice cell at the float-skin scale.
pub(crate) fn vkey(p: Point3, s: f64) -> (i64, i64, i64) {
    (
        (p.x / s).round() as i64,
        (p.y / s).round() as i64,
        (p.z / s).round() as i64,
    )
}

/// One concave-edge candidate: the kept locus endpoints (ball
/// centers), their weld keys, and the two unit face normals.
pub(crate) struct Seg {
    pub(crate) c0: Point3,
    pub(crate) c1: Point3,
    pub(crate) k0: (i64, i64, i64),
    pub(crate) k1: (i64, i64, i64),
}

pub(super) fn sub3(p: Point3, q: Point3) -> Point3 {
    Point3::new(p.x - q.x, p.y - q.y, p.z - q.z)
}

pub(super) fn add3(p: Point3, q: Point3) -> Point3 {
    Point3::new(p.x + q.x, p.y + q.y, p.z + q.z)
}

pub(super) fn scale3(p: Point3, k: f64) -> Point3 {
    Point3::new(p.x * k, p.y * k, p.z * k)
}

pub(crate) fn cmp_pt(a: Point3, b: Point3) -> std::cmp::Ordering {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
}

/// The third vertex of a face, given the two forming an edge (by
/// coordinate - the welded ids and the floats agree). Lifted from
/// pencil's private copy (M2.5 E B): the deburr walk reads the same
/// opposite vertices to ask the mirrored question.
pub(super) fn third(tri: Triangle, e0: Point3, e1: Point3) -> Point3 {
    for &q in &[tri.a, tri.b, tri.c] {
        if !near(q, e0) && !near(q, e1) {
            return q;
        }
    }
    tri.c
}

pub(super) fn near(p: Point3, q: Point3) -> bool {
    let s = (p.x.abs() + p.y.abs() + p.z.abs() + 1.0) * 1e-12;
    (p.x - q.x).abs() <= s && (p.y - q.y).abs() <= s && (p.z - q.z).abs() <= s
}

/// Is this center a READING of the corner, inside the programmed
/// window? Clear of every facet, resting on the solid at exactly r,
/// and both contact feet standing on real facets.
pub(super) fn rests(c: Point3, na: Point3, nb: Point3, p: &Finish3DParams, r: f64, sups: &SupSet) -> bool {
    if !c.is_finite() {
        return false;
    }
    // Tip keeps the window: the locus never cuts below the floor the
    // op programmed, and never centers above the clearance plane.
    if c.z - r < p.bottom_z - p.tolerance || c.z > p.clearance_z {
        return false;
    }
    let eps = 1e-6 * r.max(1.0);
    let mut touch = f64::INFINITY;
    for s in sups {
        if c.x < s.xmin || c.x > s.xmax || c.y < s.ymin || c.y > s.ymax {
            continue;
        }
        let d2 = dist2_pt_tri(c, &s.tri);
        if d2 < (r - eps) * (r - eps) {
            return false; // a bite: the ball cuts here, which is not resting
        }
        touch = touch.min(d2);
    }
    if (touch.sqrt() - r).abs() > eps {
        return false; // floating: not tangent to anything
    }
    // The feet of the two radii to the faces must stand on facets:
    // tangency to a plane that carries no surface is not a corner.
    let skin = (1e-4 * r).max(1e-7 * (r + c.x.abs() + c.y.abs() + c.z.abs()).max(1.0));
    on_solid(sub3(c, scale3(na, r)), skin, sups) && on_solid(sub3(c, scale3(nb, r)), skin, sups)
}

fn on_solid(q: Point3, skin: f64, sups: &SupSet) -> bool {
    for s in sups {
        if q.x < s.xmin || q.x > s.xmax || q.y < s.ymin || q.y > s.ymax {
            continue;
        }
        if dist2_pt_tri(q, &s.tri) <= skin * skin {
            return true;
        }
    }
    false
}

/// The longest stretch of resting centers along the locus segment;
/// None when nothing but air answers. The segment's OWN endpoints
/// are kept untouched when they rest - they are the welded
/// coincidences the chain walks on, and clipping must not disturb a
/// joint, so a clipped end only appears mid-edge.
pub(super) fn shrink(
    p0: Point3,
    p1: Point3,
    na: Point3,
    nb: Point3,
    p: &Finish3DParams,
    r: f64,
    sups: &SupSet,
) -> Option<(Point3, Point3)> {
    let d = sub3(p1, p0);
    let span = d.dot(d).sqrt();
    // A zero-length or NaN span has no direction to march; +inf
    // keeps its old right to fail the finite gate downstream.
    if span.partial_cmp(&0.0).is_none_or(|o| o.is_le()) {
        return None;
    }
    let n = ((span / (0.25 * r)).ceil() as usize).clamp(4, 32);
    let at = |i: usize| add3(p0, scale3(d, i as f64 / n as f64));
    let mut flags = [false; 33];
    for (i, f) in flags.iter_mut().enumerate().take(n + 1) {
        *f = rests(at(i), na, nb, p, r, sups);
    }
    let mut best: Option<(usize, usize)> = None;
    let mut open = false;
    let mut a = 0;
    for (i, &on) in flags.iter().take(n + 1).enumerate() {
        if on && !open {
            open = true;
            a = i;
        }
        if open && (!on || i == n) {
            let b = if on { i } else { i - 1 };
            if b > a && best.is_none_or(|(x, y)| b - a > y - x) {
                best = Some((a, b));
            }
            open = false;
        }
    }
    let (a, b) = best?;
    Some((if a == 0 { p0 } else { at(a) }, if b == n { p1 } else { at(b) }))
}

/// Weld segments into polylines and closed loops. Deterministic:
/// segments start in build order, candidates are sorted by index,
/// and continuation picks the smoothest leg - closing a loop always
/// wins its tie. A run's last point sits back on its first exactly
/// when the corner closes on itself.
pub(crate) fn chain(segs: &[Seg]) -> Vec<Vec<Point3>> {
    let mut at: std::collections::HashMap<(i64, i64, i64), Vec<(usize, bool)>> =
        std::collections::HashMap::new();
    for (i, sg) in segs.iter().enumerate() {
        at.entry(sg.k0).or_default().push((i, false));
        at.entry(sg.k1).or_default().push((i, true));
    }
    let mut used = vec![false; segs.len()];
    let mut runs: Vec<Vec<Point3>> = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut pts = vec![segs[start].c0, segs[start].c1];
        let mut keys = vec![segs[start].k0, segs[start].k1];
        let mut closed = false;
        // Grow the tail first, then the head: the order decides only
        // which candidate wins a smoothness tie, never what is built.
        for end in [1usize, 0usize] {
            if closed {
                break;
            }
            loop {
                let (k, other) = if end == 1 {
                    (keys[keys.len() - 1], keys[0])
                } else {
                    (keys[0], keys[keys.len() - 1])
                };
                let mut cand: Vec<(usize, bool)> = at.get(&k).cloned().unwrap_or_default();
                cand.sort_unstable();
                let last = if end == 1 { pts[pts.len() - 1] } else { pts[0] };
                let pin = if end == 1 {
                    sub3(pts[pts.len() - 1], pts[pts.len() - 2])
                } else {
                    sub3(pts[0], pts[1])
                };
                let mut pick: Option<(usize, bool, bool)> = None;
                let mut best_score = f64::NEG_INFINITY;
                for &(i, flipped) in &cand {
                    if used[i] {
                        continue;
                    }
                    // (i, false) says c0 stands at this key: near =
                    // c0, so far = c1; flipped is the mirror.
                    let (far, fk) = if flipped {
                        (segs[i].c0, segs[i].k0)
                    } else {
                        (segs[i].c1, segs[i].k1)
                    };
                    if fk == other {
                        pick = Some((i, flipped, true));
                        break; // the loop closes here; smoothness is moot
                    }
                    let pout = sub3(far, last);
                    let (pl, ql) = (pin.dot(pin), pout.dot(pout));
                    let score = if pl > 0.0 && ql > 0.0 {
                        pin.dot(pout) / (pl * ql).sqrt()
                    } else {
                        0.0
                    };
                    if score > best_score {
                        best_score = score;
                        pick = Some((i, flipped, false));
                    }
                }
                let Some((i, flipped, closing)) = pick else { break };
                used[i] = true;
                let (far, fk) = if flipped {
                    (segs[i].c0, segs[i].k0)
                } else {
                    (segs[i].c1, segs[i].k1)
                };
                if end == 1 {
                    pts.push(far);
                    keys.push(fk);
                } else {
                    pts.insert(0, far);
                    keys.insert(0, fk);
                }
                if closing {
                    closed = true;
                    break;
                }
            }
        }
        runs.push(pts);
    }
    runs
}

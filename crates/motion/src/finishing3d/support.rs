//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

use super::edge::{flag_folds, fold_best};
use super::grid::SupSet;
use super::params::{SPLIT_MZ, TOUCH_EPS};
/// A facet prepared for the raster's support scan. `m` is the unit
/// normal pointed into the sky (the ball rests on the plane from
/// above whichever way the winding went); `shallow` marks the facets
/// the raster may touch; the padded box rejects far facets per
/// sample without touching their vertices.
pub(super) struct Sup {
    pub(super) tri: [Point3; 3],
    pub(super) m: Point3,
    pub(super) shallow: bool,
    /// Edge `i` (`tri[i] → tri[i+1]`) is a convex fold onto another
    /// shallow facet - the neighbour's sky normal, kept on one of the
    /// two (#36, `edge.rs`).
    pub(super) folds: [Option<Point3>; 3],
    pub(super) xmin: f64,
    pub(super) ymin: f64,
    pub(super) xmax: f64,
    pub(super) ymax: f64,
}

pub(super) fn build_sups(facets: &[Triangle], r: f64) -> SupSet {
    let mut sups: Vec<Sup> = facets
        .iter()
        .map(|f| {
            let mut m = f.normal();
            if m.z < 0.0 {
                m = Point3::new(-m.x, -m.y, -m.z);
            }
            let tri = [f.a, f.b, f.c];
            Sup {
                tri,
                shallow: m.is_finite() && m.z >= SPLIT_MZ,
                folds: [None; 3],
                xmin: tri[0].x.min(tri[1].x.min(tri[2].x)) - r,
                xmax: tri[0].x.max(tri[1].x.max(tri[2].x)) + r,
                ymin: tri[0].y.min(tri[1].y.min(tri[2].y)) - r,
                ymax: tri[0].y.max(tri[1].y.max(tri[2].y)) + r,
                m,
            }
        })
        .collect();
    flag_folds(&mut sups);
    // The grid is built from the finished entries (folds flagged), #2619.
    SupSet::new(sups)
}

/// Point-to-triangle squared distance by the classic Voronoi-region
/// decomposition (published closed form - dot products and one
/// barycentric solve, no iteration).
pub(super) fn dist2_pt_tri(p: Point3, t: &[Point3; 3]) -> f64 {
    let q = closest_pt_tri(p, t);
    let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
    (dx * dx + dy * dy + dz * dz).max(0.0)
}

/// The closest point of the triangle to `p` - the contact point of a
/// ball centered at `p` the instant it touches. The Voronoi-region
/// decomposition is published closed form (dot products and one
/// barycentric solve, no iteration); `dist2_pt_tri` above IS this
/// function's distance, so the two can never disagree. Made
/// crate-public for M5.1 stage C: the 5-axis stamp asks "what facet
/// point does this ball-center rest on" - the same question the
/// raster's clearance law asks, from the other side.
pub(crate) fn closest_pt_tri(p: Point3, t: &[Point3; 3]) -> Point3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (abx, aby, abz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (acx, acy, acz) = (c.x - a.x, c.y - a.y, c.z - a.z);
    let (apx, apy, apz) = (p.x - a.x, p.y - a.y, p.z - a.z);
    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    let (bpx, bpy, bpz) = (p.x - b.x, p.y - b.y, p.z - b.z);
    let d3 = abx * bpx + aby * bpy + abz * bpz;
    let d4 = acx * bpx + acy * bpy + acz * bpz;
    let (cpx, cpy, cpz) = (p.x - c.x, p.y - c.y, p.z - c.z);
    let d5 = abx * cpx + aby * cpy + abz * cpz;
    let d6 = acx * cpx + acy * cpy + acz * cpz;
    let vc = d1 * d4 - d3 * d2;
    let vb = d5 * d2 - d1 * d6;
    let va = d3 * d6 - d5 * d4;
    // One region per arm; each computes the closest point, and the
    // arms are exhaustive by the published case proof.
    if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else if d3 >= 0.0 && d4 <= d3 {
        b
    } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3).max(1e-300);
        Point3::new(a.x + abx * v, a.y + aby * v, a.z + abz * v)
    } else if d6 >= 0.0 && d5 <= d6 {
        c
    } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6).max(1e-300);
        Point3::new(a.x + acx * w, a.y + acy * w, a.z + acz * w)
    } else if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)).max(1e-300);
        Point3::new(b.x + (c.x - b.x) * w, b.y + (c.y - b.y) * w, b.z + (c.z - b.z) * w)
    } else {
        let denom = (va + vb + vc).max(1e-300);
        let (v, w) = (vb / denom, vc / denom);
        Point3::new(
            a.x + abx * v + acx * w,
            a.y + aby * v + acy * w,
            a.z + abz * v + acz * w,
        )
    }
}

/// The center is clear when every facet keeps at least `r` of ball
/// around it - the same no-gouge law the ring enforces through the
/// forbidden region, asked of one point at a time.
///
/// The currency is EXACT (`r - TOUCH_EPS`), and that is load-bearing:
/// the distance is the triangle's, so a foot past a facet's edge
/// reads as distance to the shared edge - at least `r` for a ball
/// that truly rests on the crease. Any slack here would instead
/// forgive a SUNK center, whose ball passes inside the far facet
/// while the foot-clamped test still reads ≥ r: on a convex fold
/// that is a bite off the roof, measured (the corrugated belt's
/// envelope once ran a tol-threaded currency here: 0.0317 mm of
/// ridge bitten, under the tolerance and therefore silent). The
/// lattice-sag question is kept out of this test entirely: the only
/// slack in the kernel is `tangent_z_folded`'s, which answers the
/// GRADIENT TABLE's "where would a straddling ball rest" - a slope
/// reading that runs no cut - while the march's ground stays the
/// strict tangency, whose crease silences the minimum-step law is
/// specced against.
pub(super) fn ball_clear(c: Point3, sups: &SupSet, r: f64) -> bool {
    for s in sups.near(c.x, c.y) {
        if c.x < s.xmin || c.x > s.xmax || c.y < s.ymin || c.y > s.ymax {
            continue; // outside the padded footprint: the ball cannot reach it
        }
        if dist2_pt_tri(c, &s.tri) < (r - TOUCH_EPS) * (r - TOUCH_EPS) {
            return false;
        }
    }
    true
}

/// May the ball be lowered along the vertical column at XY `(x, y)`
/// from the center plane `top_cz` down to center height `cz` without
/// cutting on the way? `ball_clear` proves the STOP, not the DESCENT:
/// ground that hides beneath a canopy - the cone's base disc under
/// its own flank - keeps a tangent-clear center and is still
/// unreachable, because the ball crosses the canopy mid-column. The
/// exact certifier is the segment-triangle distance (the sweep of a
/// ball along a segment IS that segment inflated by `r` - the same
/// question the envelope scan asks, asked before posting rather than
/// after); the padded footprint filters facets exactly as
/// `ball_clear`'s does, since a column plus a radius only meets
/// triangles whose padded footprint it enters.
///
/// `limit` is the currency of the answer: an analytic tangent center
/// measures exactly `r` and belongs to `r - TOUCH_EPS`; a start point
/// read off a tessellated ring boundary sits a chord-sag inside the
/// true locus and belongs to `r - tol`, the same debt `leg_clears`
/// states.
pub(super) fn column_clear(x: f64, y: f64, cz: f64, top_cz: f64, limit: f64, sups: &SupSet) -> bool {
    use crate::stock_sim::dist_seg_tri;
    let (hi, lo) = (top_cz.max(cz), top_cz.min(cz));
    let (a, b) = (Point3::new(x, y, hi), Point3::new(x, y, lo));
    for s in sups.near(x, y) {
        if x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue; // outside the padded footprint: the swept ball cannot reach it
        }
        let t = Triangle::new(s.tri[0], s.tri[1], s.tri[2]);
        if dist_seg_tri(a, b, &t) < limit {
            return false;
        }
    }
    true
}

/// `support_at`'s answer plus the ground's slope: the gradient of the
/// support FUNCTION along the unit ray `u` at the point `q`. Where the
/// chosen tangent rests on one facet plane, that gradient is
/// -(m_xy . u)/m_z of the facet the ball sits on - the same currency
/// `shallow_footprint`'s `mz` spends globally, asked at one point.
/// A chord across the support could miss where a tessellated ring
/// crest answers None; the facet plane does not miss.
pub(super) fn support_grad(q: Point2, u: Point2, r: f64, floor_tip: f64, sups: &SupSet) -> Option<(f64, f64)> {
    let (x, y) = (q.x, q.y);
    let mut best: Option<(f64, f64)> = None;
    for s in sups.near(x, y) {
        if !s.shallow || x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        let Some(cz) = tangent_z_folded(s, x, y, r) else { continue };
        if cz - r < floor_tip - TOUCH_EPS {
            continue;
        }
        if best.is_some_and(|b| b.0 >= cz) {
            continue;
        }
        if ball_clear(Point3::new(x, y, cz), sups, r) {
            best = Some((cz, -(s.m.x * u.x + s.m.y * u.y) / s.m.z));
        }
    }
    // The convex folds' edge contacts (#36), judged by the same law.
    if let Some(e) = fold_best(sups, x, y, r, floor_tip, (u.x, u.y), best.map(|b| b.0)) {
        best = Some(e);
    }
    best
}

/// Plunge-support's law spoken with `support_grad`'s answer: only
/// where the column itself descends clean.
pub(super) fn plunge_support_grad(
    q: Point2,
    u: Point2,
    r: f64,
    floor_tip: f64,
    top_cz: f64,
    sups: &SupSet,
) -> Option<(f64, f64)> {
    let (cc, cut) = plunge_support(q.x, q.y, r, floor_tip, top_cz, sups);
    if cut || cc.is_none() {
        return None;
    }
    support_grad(q, u, r, floor_tip, sups)
}

/// The march ground for families that enter by vertical plunge: the
/// support's answer, but only where the descent is possible. Returns
/// the center height and whether the column law TRIMMED an answer
/// `support_at` had given - a trimmed ground is loud (a pass note),
/// never a silently vanished floor; the rest scan answers for what
/// stays under the canopy.
pub(super) fn plunge_support(
    x: f64,
    y: f64,
    r: f64,
    floor_tip: f64,
    top_cz: f64,
    sups: &SupSet,
) -> (Option<f64>, bool) {
    match support_at(x, y, r, floor_tip, sups) {
        None => (None, false),
        Some(cz) => {
            if column_clear(x, y, cz, top_cz, r - TOUCH_EPS, sups) {
                (Some(cz), false)
            } else {
                (None, true)
            }
        }
    }
}

/// The tangent-center height of a ball resting on one shallow facet
/// above center XY `(x, y)`: the contact `q = (x, y) − r·m_xy` must
/// fall inside the facet's XY projection (a shallow facet's
/// projection is never degenerate), and the center sits at
/// `q_z + r·m_z`. None when this facet owns no tangent here.
///
/// The facet law is the strict one: a foot a hair past a facet edge
/// answers nothing here. A convex fold's straddle annulus is answered
/// by the edge contact instead (`edge::fold_best`, #36), never by
/// slack in this test.
pub(super) fn tangent_z(s: &Sup, x: f64, y: f64, r: f64) -> Option<f64> {
    tangent_z_at(s, x, y, r, false)
}

/// The same question for the GRADIENT lookup only (`support_grad`):
/// the contact may ride onto the facet's neighbour across a fold the
/// ball genuinely straddles. This slack is safe HERE because a
/// gradient answer proposes nothing the march rides: the ground the
/// curve actually walks is still `support_at`'s exact question, and
/// the column safety is `plunge_support`'s. Used it for the whole
/// kernel once, on the theory that candidates only propose and
/// `ball_clear` decides; true, and the belt still lost its staircase
/// - the edge-resting centers a straddling ball legitimately has
///   then ANSWER the march where the module's promise says break, and
///   a law's promise outranks a looser answer. Narrowed to the table.
pub(super) fn tangent_z_folded(s: &Sup, x: f64, y: f64, r: f64) -> Option<f64> {
    tangent_z_at(s, x, y, r, true)
}

fn tangent_z_at(s: &Sup, x: f64, y: f64, r: f64, fold: bool) -> Option<f64> {
    if !s.shallow {
        return None;
    }
    let (a, b, c) = (s.tri[0], s.tri[1], s.tri[2]);
    let qx = x - r * s.m.x;
    let qy = y - r * s.m.y;
    // Same-sign barycentrics against the XY projection.
    let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let w0 = ((b.x - a.x) * (qy - a.y) - (b.y - a.y) * (qx - a.x)) / det;
    let w1 = ((c.x - a.x) * (a.y - qy) - (c.y - a.y) * (a.x - qx)) / det;
    let w2 = 1.0 - w0 - w1;
    // The folded ask (`fold`, the gradient table's) may ride the
    // contact outside this facet, onto its neighbour, but only as
    // far as a real neighbour could reach. A pure barycentric slack
    // is the wrong currency: the legit overshoot is set by the FOLD
    // a ball can straddle, not by the triangle's size, and a big
    // triangle's 15 percent is millimetres of empty air (the plane's
    // plane-extension would then answer a rung standing off the
    // model's edge, cutting nothing and chaining links the angle law
    // must refuse). Two caps say it together:
    //  - an absolute one, a twentieth of the ball: past any facet
    //    edge, tangency survives while the plane's float over the
    //    fold (overshoot times the normal's turn, at most a radian
    //    of tilt-change) stays under the pattern's teeth; 0.05*r is
    //    the same budget the tessellation currency already spends,
    //    and it is what lets a polar mesh's radial-knife band rest
    //    - a quad facet's diagonal-split triangles carry normals a
    //    half-wedge off the ray, so the true contact sits just
    //    OUTSIDE both translated footprints, a gap of
    //    r*sin(tilt)*sin(dtheta/2) that no facet-size law speaks.
    //  - a relative one, a fiftieth of the height: on coarse facets
    //    the fold straddled is the mesh's own sag, and that debt was
    //    already paid by the old barycentric hair (0.02); keeping it
    //    as a floor under the absolute cap means every tangency the
    //    module answered before it still answers.
    // The barycentric test below still runs at 0.15 as a shape
    // guard: a sliver's absolute cap must not buy an overshoot of
    // whole facets. Safety is ball_clear's, never the slack's.
    if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
        // inside: the exact tangency, no slack spent.
    } else if !fold {
        if w0.min(w1).min(w2) < -1e-9 {
            return None;
        }
    } else {
        let wmin = w0.min(w1).min(w2);
        if wmin < -0.15 {
            return None;
        }
        let area2 = det.abs();
        let (e0, e1, e2) = (
            (b.x - a.x).hypot(b.y - a.y),
            (c.x - a.x).hypot(c.y - a.y),
            (c.x - b.x).hypot(c.y - b.y),
        );
        let h = |edge: f64| area2 / edge.max(1e-300);
        let cap = |hi: f64| (0.05 * r).max(0.02 * hi);
        let over = |w: f64, hi: f64| w >= 0.0 || -w * hi <= cap(hi);
        if !(over(w0, h(e0)) && over(w1, h(e1)) && over(w2, h(e2))) {
            return None;
        }
    }
    // The plane through the triangle, solved for z (m_z >= SPLIT_MZ).
    let qz = a.z - (s.m.x * (qx - a.x) + s.m.y * (qy - a.y)) / s.m.z;
    (qz.is_finite() && (qz + r * s.m.z).is_finite()).then_some(qz + r * s.m.z)
}

/// The raster's whole law in one question: where does the ball sit
/// above `(x, y)`? The HIGHEST tangent center that keeps the ball
/// clear of every facet - the topmost surface a descending tool
/// meets first - and `None` over air, over unreached overhangs, and
/// over anything whose tangent sits below the programmed floor: the
/// floor outranks the surface, nothing cuts below `bottom_z`. The
/// clearance is exact (`r - TOUCH_EPS` through `ball_clear`), and the
/// candidates are the strict facet tangency plus, since #36, the
/// drop-cutter edge contact on CONVEX folds between shallow facets
/// (`edge.rs`): the `r·|Δm|` annulus a fold leaves is ridden on the
/// edge, not broken at. A concave fold, a mesh rim, or a shallow-steep
/// edge still answers nothing there. The folded candidate belongs to
/// `tangent_z_folded`, the gradient table's question, which runs no
/// cut. Slack in the clearance itself would forgive a real bite;
/// slack in a candidate would only admit a question.
pub(super) fn support_at(x: f64, y: f64, r: f64, floor_tip: f64, sups: &SupSet) -> Option<f64> {
    let mut best: Option<f64> = None;
    for s in sups.near(x, y) {
        if !s.shallow || x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        let Some(cz) = tangent_z(s, x, y, r) else { continue };
        if cz - r < floor_tip - TOUCH_EPS {
            continue;
        }
        if best.is_some_and(|b| b >= cz) {
            continue; // below the current best: clearing it cannot raise the answer
        }
        if ball_clear(Point3::new(x, y, cz), sups, r) {
            best = Some(cz);
        }
    }
    // The convex folds' edge contacts (#36): where the strict tangency
    // leaves its `r·|Δm|` annulus the ball rests on the shared edge.
    fold_best(sups, x, y, r, floor_tip, (1.0, 0.0), best).map(|e| e.0).or(best)
}

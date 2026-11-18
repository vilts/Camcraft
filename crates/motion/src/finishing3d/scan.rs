//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, Seg};

/// The forbidden-center polygon one facet contributes at center plane
/// `cz` - the support scan of `facet ⊕ ball ∩ plane`, exact per the
/// module's second law. Returns the empty vector (no contribution)
/// when the facet's height band misses the plane, or if every
/// direction somehow came up candidate-free (the band test makes that
/// impossible; the fallback keeps the law total without an expect).
fn push_support_dir(
    v: &[Point3; 3],
    cz: f64,
    r: f64,
    r2: f64,
    ux: f64,
    uy: f64,
    pts: &mut Vec<Point2>,
) -> bool {
    // The argmax candidates for this direction: in-band vertices,
    // plus each non-horizontal edge's stationary point when it
    // lands inside the segment. Six at most, no allocation.
    let mut cand = [v[0]; 6];
    let mut nc = 0usize;
    for &q in v {
        if (cz - q.z).abs() <= r {
            cand[nc] = q;
            nc += 1;
        }
    }
    for e in 0..3 {
        let (q0, q1) = (v[e], v[(e + 1) % 3]);
        let ez = q1.z - q0.z;
        if ez == 0.0 {
            continue; // horizontal edge: its endpoints own the max
        }
        // g'(t) = 0 along the edge means w* = -a*r*sign(ez) /
        // sqrt(ez^2 + a^2) with a = E_xy.u; |w*| <= r by
        // inspection, so t* is inside the height band whenever
        // it is inside the segment.
        let a = (q1.x - q0.x) * ux + (q1.y - q0.y) * uy;
        let wst = -a * r * ez.signum() / (ez * ez + a * a).sqrt();
        let t = (cz - q0.z - wst) / ez;
        if t > 0.0 && t < 1.0 && nc < cand.len() {
            let qz = q0.z + ez * t;
            if (cz - qz).abs() <= r {
                cand[nc] = Point3::new(q0.x + (q1.x - q0.x) * t, q0.y + (q1.y - q0.y) * t, qz);
                nc += 1;
            }
        }
    }
    if nc == 0 {
        return false;
    }
    let mut best: Option<(f64, Point2)> = None;
    for q in &cand[..nc] {
        let rho = (r2 - (cz - q.z) * (cz - q.z)).max(0.0).sqrt();
        // M4.2b: the scan point lands on the grid the geo2d bridge
        // rounds onto the moment it is born, so every argument it
        // is used in stays independent of platform libm rounding.
        let p = Point2::new(lattice(q.x + rho * ux), lattice(q.y + rho * uy));
        let s = lattice(p.x * ux + p.y * uy);
        if best.is_none_or(|(b, _)| s > b) {
            best = Some((s, p));
        }
    }
    match best {
        Some((_, p)) => {
            pts.push(p);
            true
        }
        None => false,
    }
}

/// Sample extra tangency points along shallow Z-sloped edges where the XY
/// ellipse chord between adjacent angles of `ds` would exceed `MAX_CHORD`.
fn push_shallow_edge_supports(
    v: &[Point3; 3],
    cz: f64,
    r: f64,
    dtheta_ds: f64,
    dtheta_max: f64,
    pts: &mut Vec<Point2>,
) {
    const MAX_CHORD: f64 = 0.006;
    for e in 0..3 {
        let (q0, q1) = (v[e], v[(e + 1) % 3]);
        let ez = q1.z - q0.z;
        let (dx, dy) = (q1.x - q0.x, q1.y - q0.y);
        let l_xy = dx.hypot(dy);
        if ez == 0.0 || l_xy <= 1e-9 || ez.abs() >= l_xy {
            continue;
        }
        let z_lo = q0.z.min(q1.z).max(cz - r);
        let z_hi = q0.z.max(q1.z).min(cz + r);
        if z_lo >= z_hi {
            continue;
        }
        let theta_lo = ((cz - z_hi) / r).clamp(-1.0, 1.0).asin();
        let theta_hi = ((cz - z_lo) / r).clamp(-1.0, 1.0).asin();
        let span = theta_hi - theta_lo;
        if span <= 1e-9 {
            continue;
        }
        // Maximum XY radius of curvature of the edge-swept ball ellipse on
        // [theta_lo, theta_hi]: R_curv(theta) = r * (L^2 / ez^2) * cos^3(theta).
        let theta_min = if theta_lo <= 0.0 && 0.0 <= theta_hi {
            0.0
        } else {
            theta_lo.abs().min(theta_hi.abs())
        };
        let cos_min = theta_min.cos();
        let len2 = l_xy * l_xy + ez * ez;
        let r_curv_max = r * (len2 / (ez * ez)) * cos_min * cos_min * cos_min;
        let sag_est = r_curv_max * (1.0 - (0.5 * dtheta_ds).cos());
        if sag_est <= MAX_CHORD {
            continue;
        }
        let steps = (span / dtheta_max).ceil().max(1.0) as usize;
        let (ex, ey) = (dx / l_xy, dy / l_xy);
        let (px, py) = (-ey, ex);
        let mut emit_theta = |theta: f64| {
            if theta.abs() >= std::f64::consts::FRAC_PI_2 - 1e-6 {
                return;
            }
            let u_e = (-ez / l_xy) * theta.tan();
            if u_e.abs() >= 1.0 - 1e-9 {
                return;
            }
            let (sin_t, cos_t) = theta.sin_cos();
            let t = (cz - q0.z - r * sin_t) / ez;
            if t <= 0.0 || t >= 1.0 {
                return;
            }
            let qx = q0.x + dx * t;
            let qy = q0.y + dy * t;
            let rho = r * cos_t;
            let u_p = (1.0 - u_e * u_e).sqrt();
            let ux1 = ex * u_e + px * u_p;
            let uy1 = ey * u_e + py * u_p;
            let ux2 = ex * u_e - px * u_p;
            let uy2 = ey * u_e - py * u_p;
            pts.push(Point2::new(lattice(qx + rho * ux1), lattice(qy + rho * uy1)));
            pts.push(Point2::new(lattice(qx + rho * ux2), lattice(qy + rho * uy2)));
        };
        for k in 0..=steps {
            emit_theta(theta_lo + span * (k as f64 / steps as f64));
        }
        // Explicitly emit the z = cz slice crossing (theta = 0) so flat wall
        // edges keep both tying crossings regardless of ds tie-breaking.
        if theta_lo < 0.0 && 0.0 < theta_hi {
            emit_theta(0.0);
        }
    }
}

pub(super) fn facet_polygon(f: &Triangle, r: f64, cz: f64, ds: &[(f64, f64)]) -> Vec<Point2> {
    let v = [f.a, f.b, f.c];
    let zmin = v[0].z.min(v[1].z).min(v[2].z);
    let zmax = v[0].z.max(v[1].z).max(v[2].z);
    if zmax < cz - r || zmin > cz + r {
        return Vec::new();
    }
    let r2 = r * r;
    let mut pts = Vec::with_capacity(ds.len());
    for &(ux, uy) in ds {
        if !push_support_dir(&v, cz, r, r2, ux, uy, &mut pts) {
            return Vec::new();
        }
    }
    let dtheta_ds = if ds.is_empty() { 0.05 } else { std::f64::consts::TAU / ds.len() as f64 };
    let dtheta_max = dtheta_ds.min(2.0 * (2.0 * 0.004 / r).sqrt());
    push_shallow_edge_supports(&v, cz, r, dtheta_ds, dtheta_max, &mut pts);
    convex_hull(&pts)
}

/// Andrew monotone-chain hull, CCW, collinear points dropped. The
/// input came from a support scan of a convex set, so this only ever
/// trims float noise - but sorting first makes the output independent
/// of the scan's starting direction either way.
pub(super) fn convex_hull(pts: &[Point2]) -> Vec<Point2> {
    // M4.2b: the argument is had in the bridge's integers. The scan
    // points arrive on the grid; collinearity is an exact i128
    // cross product, so a tangent where three scan points sit a
    // tenth of a micron apart keeps or drops its middle vertex on
    // the geometry, never on a float rounding. Coordinates stay
    // well under i64's reach (a 1000 mm part is 1e9 grid units;
    // an edge product 4e18 is i128's small change), and a saturating
    // cast keeps even a nonsense input finite.
    let grid = |v: f64| (v * 1e6).round() as i64;
    let mut p: Vec<(i64, i64)> = pts.iter().map(|q| (grid(q.x), grid(q.y))).collect();
    p.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    p.dedup_by(|a, b| a == b);
    if p.len() < 3 {
        return Vec::new();
    }
    let cross = |o: &(i64, i64), a: &(i64, i64), b: &(i64, i64)| -> i128 {
        (a.0 - o.0) as i128 * (b.1 - o.1) as i128 - (a.1 - o.1) as i128 * (b.0 - o.0) as i128
    };
    let mut lower: Vec<(i64, i64)> = Vec::with_capacity(p.len());
    for q in &p {
        while lower.len() >= 2 && cross(&lower[lower.len() - 2], &lower[lower.len() - 1], q) <= 0
        {
            lower.pop();
        }
        lower.push(*q);
    }
    let mut upper: Vec<(i64, i64)> = Vec::with_capacity(p.len());
    for q in p.iter().rev() {
        while upper.len() >= 2 && cross(&upper[upper.len() - 2], &upper[upper.len() - 1], q) <= 0
        {
            upper.pop();
        }
        upper.push(*q);
    }
    lower.pop(); // each chain repeats the far endpoint of the other
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return Vec::new(); // collinear after all: no area, no loop
    }
    lower
        .into_iter()
        .map(|(x, y)| Point2::new(x as f64 / 1e6, y as f64 / 1e6))
        .collect()
}

/// The forbidden-center region at one center plane: every facet's
/// convex contribution fused in one Clipper pass. The law here is
/// `Region::union_all`, NOT `Region::new`: facet contributions of one
/// solid are all material, while `new` reads nested loops as holes by
/// shell parity - a law for sketched geometry. A box's bottom-facet
/// footprint genuinely sits inside its walls' offset square at low
/// levels; under parity it became a "hole", the ring walker drew a
/// ring around it, and the ball rode inside corner material (the
/// M2.2a gouge that minted this entry). Overlapping or interpenetrating
/// solids need no special case either way; duplicates now double-wind
/// as material instead of cancelling.
pub(super) fn level_region(
    facets: &[Triangle],
    r: f64,
    cz: f64,
    ds: &[(f64, f64)],
) -> Result<Region, GenError> {
    let mut loops: Vec<Loop> = Vec::new();
    for f in facets {
        if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
            return Err(GenError::Invalid("model has a non-finite facet".into()));
        }
        let hull = facet_polygon(f, r, cz, ds);
        // A band-skimming facet can collapse to a segment or a point
        // (a disk tangent from the side); it forbids no area, so it
        // is no loop. That is a geometry fact, not a failure.
        if let Ok(l) = Loop::new(hull) && l.area().abs() > 1e-12 {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// Walk a fitted ring as tip-Z `z` feed moves (the private twin of
/// machining2d's `push_segs` - same Seg enum, same chaining law).
pub(super) fn push_walk(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64) {
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed), *center, *ccw);
            }
        }
    }
}

//! Solid-truth material intervals for hidden-rest reporting (M2.2d).
//!
//! Provenance: our own design; the inside/outside test is the classic
//! ray-parity membership rule, standard in any computational-geometry
//! text (e.g. the point-in-mesh surveys in Held & Lyckega, "Approximate
//! cell decomposition of structures"). Nothing derived from vendor
//! binaries or output forensics.
//!
//! Why this module exists. The envelope kernel (`stock_sim`) rides a
//! HEIGHT FIELD: per dexel column it knows the topmost surface, which
//! is exactly what a Z-following ball sees - and exactly what hides a
//! ceiling's shadow. Material sheltered under an overhang never
//! appears in a height-field rest measure: the column's top is the
//! ceiling, so the floor below it is invisible (RECOMMENDED.md names
//! this as M2.2b's open debt). The cure is the SOLID truth: cast the
//! column as a vertical ray and pair its mesh crossings. A closed
//! shell crosses the ray an even number of times and each even-index
//! hit opens a material interval that the next, odd-index hit closes -
//! however deep, however shadowed. Rest is then what material remains
//! outside the swept ball's reach, ceilings included.
//!
//! This module commits to three laws:
//! - (a) CLAIMS: every facet whose closed XY triangle contains the
//!   column makes a claim - INTERIOR (point deep inside) or BOUNDARY
//!   (point on an edge or vertex, within tolerance). At one crossing z
//!   the claims settle into material counts by plane: each interior
//!   claim is its own sheet, and bare boundary claims of one plane
//!   merge to ONE crossing (they are pieces of a single sheet meeting
//!   at an edge). When several non-coplanar bare boundary groups meet
//!   at the same z (a revolved meridian seam, cone apex or V-root
//!   crease, #2880), their XY angular sectors around the column decide
//!   parity: 1 crossing when any open angular sector has odd group
//!   multiplicity (one sheet crossed on its edge), 0 when every sector
//!   has even multiplicity (a grazing fold). Per-triangle half-open
//!   ownership cannot do this - a shared diagonal would then belong to
//!   one owning triangle, but two coincident sheets stacked flush would
//!   collapse to one, lying about parity. Claims see it through: an
//!   ordinary column is even on any closed shell however triangulated,
//!   and a knife-edge double surface (the column ON two flush sheets at
//!   their own shared edge) lands odd - reported, never guessed.
//! - (b) TOTAL: no input - NaN facets, zero-area facets, open shells -
//!   panics or hangs. An odd crossing count is reported as
//!   `Crossings::Open`, an honest unknown; callers may measure it but
//!   may not call it material.
//! - (c) ORIENTATION-FREE: parity needs a CLOSED shell, not a wound
//!   one. Flipped facets change nothing here - normals group by
//!   LINE, not direction, boundary sectors are CCW independent of
//!   winding, and inside/outside never reads winding.

use crate::geometry::{Point3, Triangle};

#[cfg(test)]
#[path = "solid_rest_tests.rs"]
mod tests;

/// Where a vertical ray at `(x, y)` found the shell, z-ascending.
#[derive(Clone, Debug, PartialEq)]
pub enum Crossings {
    /// Even crossings: each pair bounds a material interval
    /// (closed-shell answer).
    Paired(Vec<f64>),
    /// Odd crossings: the column ends inside - open shell, or a
    /// knife-edge boundary column despite law (a)'s claims. `z` is the
    /// crossing list; the last entry is unpaired.
    Open { z: Vec<f64> },
}

impl Crossings {
    /// The material intervals if every crossing is bracketed.
    #[must_use]
    pub fn intervals(&self) -> Option<Vec<(f64, f64)>> {
        match self {
            Crossings::Paired(z) => Some(z.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()),
            Crossings::Open { .. } => None,
        }
    }
}

/// One facet's claim on a column: crossing z, facet plane normal, and
/// for a BOUNDARY claim the CCW XY angular sector `[start, span]`
/// covered around the column (`[NaN, NaN]` for an INTERIOR claim,
/// keeping `Claim` at 40 bytes).
pub(crate) struct Claim {
    pub(crate) z: f64,
    pub(crate) n: Point3,
    pub(crate) sector: [f32; 2],
}

impl Claim {
    fn is_interior(&self) -> bool {
        self.sector[0].is_nan()
    }
}

/// All crossing z of the vertical line through `(x, y)` with `facets`.
#[must_use]
pub fn column_crossings(facets: &[Triangle], x: f64, y: f64) -> Crossings {
    let mut claims: Vec<Claim> = Vec::with_capacity(16);
    for t in facets {
        if let Some(c) = plane_claim(&t.a, &t.b, &t.c, x, y) {
            claims.push(c);
        }
    }
    crossings_from_claims(&claims)
}

/// The parity engine, law (a) in one pass: claims sorted, clustered
/// into physical levels within `EPS_Z`, settled to crossings per
/// plane group (every interior claim is a sheet; bare boundary groups
/// settle by their XY angular coverage around the column - 1 for a
/// coplanar merge or manifold ridge/apex, 0 for a grazing fold), then
/// odd/even into `Crossings`. Split out so the envelope kernel can
/// feed it pre-binned claims without touching facets again.
pub(crate) fn crossings_from_claims(claims: &[Claim]) -> Crossings {
    const EPS_Z: f64 = 1e-9;
    const EPS_PAR: f64 = 1e-9; // sin of the angle where normals are "same line"
    if claims.is_empty() {
        return Crossings::Paired(Vec::new());
    }
    let mut sorted: Vec<&Claim> = claims.iter().collect();
    sorted.sort_by(|p, q| p.z.total_cmp(&q.z));
    let mut z: Vec<f64> = Vec::with_capacity(claims.len());
    let mut i = 0;
    while i < sorted.len() {
        // One physical level: every claim within EPS_Z of the cluster's first z.
        let z0 = sorted[i].z;
        let mut sum = 0.0;
        let mut count = 0usize;
        // Plane groups at this level: representative normal, interior
        // count, and bare boundary sectors (unallocated for interior hits).
        let mut groups: Vec<(Point3, usize, Vec<[f32; 2]>)> = Vec::new();
        while i < sorted.len() && (sorted[i].z - z0).abs() <= EPS_Z {
            let c = sorted[i];
            let is_int = c.is_interior();
            match groups
                .iter()
                .position(|(r, _, _)| parallel_line(*r, c.n, EPS_PAR))
            {
                Some(p) => {
                    if is_int {
                        groups[p].1 += 1;
                    } else {
                        groups[p].2.push(c.sector);
                    }
                }
                None => {
                    let secs = if is_int { Vec::new() } else { vec![c.sector] };
                    groups.push((c.n, usize::from(is_int), secs));
                }
            }
            sum += c.z;
            count += 1;
            i += 1;
        }
        let zm = sum / count as f64;
        let mut bare: Vec<Vec<[f32; 2]>> = Vec::new();
        for (_, inter, secs) in groups {
            if inter > 0 {
                for _ in 0..inter {
                    z.push(zm);
                }
            } else if !secs.is_empty() {
                bare.push(secs);
            }
        }
        for _ in 0..boundary_crossings(&bare) {
            z.push(zm);
        }
    }
    if z.len() % 2 == 1 {
        Crossings::Open { z }
    } else {
        Crossings::Paired(z)
    }
}

/// True when both unit normals lie on the same LINE (parallel or
/// antiparallel) - coplanar sheets in any winding (law c).
fn parallel_line(a: Point3, b: Point3, eps: f64) -> bool {
    let c = a.cross(b);
    c.dot(c) <= eps * eps
}

/// Whether `angle` (in `[0, TAU)`) lies strictly inside the CCW sector
/// `[start, start + span]`.
fn sector_contains(sec: [f32; 2], angle: f64) -> bool {
    let start = f64::from(sec[0]);
    let span = f64::from(sec[1]);
    let delta = (angle - start).rem_euclid(std::f64::consts::TAU);
    delta > 0.0 && delta < span
}

/// Crossings contributed by non-coplanar bare-boundary plane groups at
/// one Z level (#2880): 1 if any open angular sector around the column
/// has odd group multiplicity (a single manifold sheet crossed on a
/// meridian edge, apex, or rim), 0 if every sector has even
/// multiplicity (a grazing V-root or overhang fold). One coplanar group
/// always counts as 1 (preserving law (a)'s knife-edge `Open` verdict
/// on flush stacked shells).
fn boundary_crossings(bare: &[Vec<[f32; 2]>]) -> usize {
    if bare.is_empty() {
        return 0;
    }
    if bare.len() == 1 {
        return 1;
    }
    const TAU: f64 = std::f64::consts::TAU;
    const MIN_WEDGE: f64 = 1e-4;
    let mut cuts: Vec<f64> = Vec::with_capacity(bare.len() * 2);
    for grp in bare {
        for &[s, sp] in grp {
            if !(s.is_finite() && sp.is_finite()) {
                return 1; // non-finite sector: never silence
            }
            let a = f64::from(s).rem_euclid(TAU);
            let b = (a + f64::from(sp)).rem_euclid(TAU);
            cuts.push(a);
            cuts.push(b);
        }
    }
    cuts.sort_unstable_by(f64::total_cmp);
    let n = cuts.len();
    for idx in 0..n {
        let a = cuts[idx];
        let b = if idx + 1 < n { cuts[idx + 1] } else { cuts[0] + TAU };
        let w = b - a;
        if w < MIN_WEDGE {
            continue;
        }
        let mid = (a + 0.5 * w).rem_euclid(TAU);
        let mult = bare
            .iter()
            .filter(|grp| grp.iter().any(|&sec| sector_contains(sec, mid)))
            .count();
        if mult % 2 == 1 {
            return 1;
        }
    }
    0
}

fn norm_angle(a: f64) -> f64 {
    a.rem_euclid(std::f64::consts::TAU)
}

fn sector_between(v0: (f64, f64), v1: (f64, f64), ccw: bool) -> [f32; 2] {
    let (p, q) = if ccw { (v0, v1) } else { (v1, v0) };
    let a0 = norm_angle(p.1.atan2(p.0));
    let a1 = norm_angle(q.1.atan2(q.0));
    let span = (a1 - a0).rem_euclid(std::f64::consts::TAU).min(std::f64::consts::PI);
    [a0 as f32, span as f32]
}

fn half_plane(e: (f64, f64), ccw: bool) -> [f32; 2] {
    let (ex, ey) = if ccw { e } else { (-e.0, -e.1) };
    [norm_angle(ey.atan2(ex)) as f32, std::f32::consts::PI]
}

/// The CCW XY angular sector `[start, span]` covered by triangle `abc`
/// around a boundary column with barycentric coordinates `(1-u-v, u, v)`
/// and signed twice-area `d`. Independent of winding (law c).
fn boundary_sector(a: &Point3, b: &Point3, c: &Point3, d: f64, u: f64, v: f64, eps: f64) -> [f32; 2] {
    let w = 1.0 - u - v;
    let ccw = d > 0.0;
    let ab = (b.x - a.x, b.y - a.y);
    let bc = (c.x - b.x, c.y - b.y);
    let ca = (a.x - c.x, a.y - c.y);
    match (w <= eps, u <= eps, v <= eps) {
        (false, true, true) => sector_between(ab, (-ca.0, -ca.1), ccw),
        (true, false, true) => sector_between(bc, (-ab.0, -ab.1), ccw),
        (true, true, false) => sector_between(ca, (-bc.0, -bc.1), ccw),
        (false, false, true) => half_plane(ab, ccw),
        (true, false, false) => half_plane(bc, ccw),
        (false, true, false) => half_plane(ca, ccw),
        _ => [0.0, std::f32::consts::PI],
    }
}

/// The claim of `abc` on the column at `(x, y)`, or None if the
/// column misses the facet's XY triangle or the projection is
/// degenerate (vertical wall seen edge-on, zero area) - walls are
/// handled by the columns their faces bound, never by a ray down
/// their plane. Claims carry the strictness that law (a) needs.
pub(crate) fn plane_claim(
    a: &Point3,
    b: &Point3,
    c: &Point3,
    x: f64,
    y: f64,
) -> Option<Claim> {
    let d = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    // Degeneracy skin scales with the projection's own footprint.
    let span = (b.x - a.x)
        .abs()
        .max((c.x - a.x).abs())
        .max((b.y - a.y).abs())
        .max((c.y - a.y).abs());
    if !d.is_finite() || !span.is_finite() || d.abs() <= 1e-12 * span.max(1.0) * span.max(1.0) {
        return None;
    }
    let px = x - a.x;
    let py = y - a.y;
    if !(px.is_finite() && py.is_finite()) {
        return None;
    }
    let u = (px * (c.y - a.y) - py * (c.x - a.x)) / d;
    let v = ((b.x - a.x) * py - (b.y - a.y) * px) / d;
    const EPS_B: f64 = 1e-12;
    // Closed containment, then the strictness that splits interior
    // from boundary claims. The barycentric tolerance is dimension-
    // free, so it is a fraction of the facet, not of the model.
    if u < -EPS_B || v < -EPS_B || u + v > 1.0 + EPS_B {
        return None;
    }
    let interior = u > EPS_B && v > EPS_B && u + v < 1.0 - EPS_B;
    // A level facet's crossing IS its height: the barycentric sum
    // rounds by an ulp where the three z are equal, which a strict
    // read at the plane itself (`above(z)` at a stock top, the field's
    // crossing read - #780) must never see. A tilted facet keeps the
    // sum.
    let z = if a.z == b.z && b.z == c.z { a.z } else { (1.0 - u - v) * a.z + u * b.z + v * c.z };
    let n = (*b - *a).cross(*c - *a);
    let l2 = n.dot(n);
    if !z.is_finite() || l2 <= 0.0 || !l2.is_finite() {
        return None;
    }
    let inv = 1.0 / l2.sqrt();
    let sector = if interior {
        [f32::NAN, f32::NAN]
    } else {
        boundary_sector(a, b, c, d, u, v, EPS_B)
    };
    Some(Claim {
        z,
        n: Point3::new(n.x * inv, n.y * inv, n.z * inv),
        sector,
    })
}

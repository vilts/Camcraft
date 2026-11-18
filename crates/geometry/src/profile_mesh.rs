//! Closed outward triangle meshing and 2D validation for spun half-profiles
//! and extruded closed profiles (#744).
//! Provenance: standard ear-clipping polygon triangulation and solid of revolution.

use std::f64::consts::TAU;

use crate::dxf_profile::{ClosedProfile, ExtrudeAxis, HalfProfile, ProfileError, ProfileVertex};
use crate::geometry::{Point3, Triangle};
use crate::geo2d::COORD_LIMIT;

/// Azimuthal subdivisions around the spindle axis (matches holder revolution).
pub const REVOLVE_RING: usize = 32;
const DEDUP_TOL: f64 = 1e-6;
const CROSS_TOL: f64 = 1e-8;
const AREA_MIN: f64 = 1e-6;

fn checked_values(values: &[f64], what: &str) -> Result<(), ProfileError> {
    if values.iter().any(|v| !v.is_finite() || v.abs() >= COORD_LIMIT) {
        return Err(ProfileError(format!("profile {what} is non-finite or outside the permitted coordinate range")));
    }
    Ok(())
}

// Validate the actual collision envelope as well as the visual facets. A finite
// mesh alone is insufficient when its conservative chord allowance overflows.
fn check_mesh(tris: &[Triangle], error: f64) -> Result<(), ProfileError> {
    checked_values(&[error], "collision bound")?;
    for t in tris {
        for p in [t.a, t.b, t.c] {
            checked_values(&[p.x, p.y, p.z], "mesh coordinate")?;
            checked_values(&[
                p.x - error, p.x + error, p.y - error, p.y + error,
                p.z - error, p.z + error,
            ], "collision bound")?;
        }
    }
    Ok(())
}

fn arc_steps(theta: f64) -> usize {
    ((theta.abs() / (10.0_f64.to_radians())).ceil() as usize).clamp(4, 72)
}

/// Upper bound on the distance from a curved profile to its flattened edges.
/// Each circular subarc differs from its chord by at most its sagitta.
#[must_use]
pub fn profile_chord_error(points: &[ProfileVertex], closed: bool) -> f64 {
    let count = if closed { points.len() } else { points.len().saturating_sub(1) };
    (0..count).filter_map(|i| {
        let a = points.get(i)?;
        let b = points.get((i + 1) % points.len())?;
        let chord = (b.u - a.u).hypot(b.v - a.v);
        if a.bulge == 0.0 { return Some(0.0); }
        // flatten_edges omits arcs below its numeric thresholds. Bound
        // those by the whole edge sagitta instead of a sampled subarc.
        if a.bulge.abs() <= 1e-9 || chord <= DEDUP_TOL {
            return Some(chord * a.bulge.abs() * 0.5 + DEDUP_TOL);
        }
        let theta = 4.0 * a.bulge.atan();
        let radius = chord * (1.0 + a.bulge * a.bulge) / (4.0 * a.bulge.abs());
        Some(radius * (1.0 - (theta / (2.0 * arc_steps(theta) as f64)).cos()) + DEDUP_TOL)
    }).fold(0.0, f64::max)
}

/// Upper bound for both section flattening and azimuthal revolution chords.
/// The original section radius cannot exceed its endpoint radius plus the
/// whole edge sagitta. Rotation preserves the section approximation error.
#[must_use]
pub fn spun_chord_error(profile: &HalfProfile) -> f64 {
    let section_error = profile_chord_error(&profile.points, false);
    let radius = profile.points.windows(2).map(|edge| {
        let a = edge[0];
        let b = edge[1];
        a.v.max(b.v) + (b.u - a.u).hypot(b.v - a.v) * a.bulge.abs() * 0.5
    }).fold(0.0, f64::max);
    // spun_loop snaps radii within 0.1 um to the axis. Include that
    // normalization and flattening deduplication in the bound.
    section_error + radius * (1.0 - (TAU / (2.0 * REVOLVE_RING as f64)).cos()) + 1e-4 + DEDUP_TOL
}

/// Flatten consecutive `(p0 -> p1, bulge)` edge into intermediate points `[p0, ...]`.
pub(crate) fn flatten_edges(points: &[ProfileVertex], closed: bool) -> Result<Vec<[f64; 2]>, ProfileError> {
    if points.len() < 2 {
        return Err(ProfileError("profile requires at least two vertices".into()));
    }
    let count = if closed { points.len() } else { points.len() - 1 };
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(points.len() * 2);
    for i in 0..count {
        let a = points.get(i).ok_or_else(|| ProfileError("vertex missing".into()))?;
        let b = points
            .get((i + 1) % points.len())
            .ok_or_else(|| ProfileError("vertex missing".into()))?;
        if !(a.u.is_finite() && a.v.is_finite() && a.bulge.is_finite())
            || !(b.u.is_finite() && b.v.is_finite() && b.bulge.is_finite())
        {
            return Err(ProfileError("profile contains non-finite coordinate".into()));
        }
        checked_values(&[a.u, a.v, b.u, b.v], "coordinate")?;
        push_unique(&mut out, [a.u, a.v]);
        if a.bulge.abs() > 1e-9 {
            let theta = 4.0 * a.bulge.atan();
            let dx = b.u - a.u;
            let dy = b.v - a.v;
            let chord = dx.hypot(dy);
            if chord > DEDUP_TOL {
                let squared_bulge = a.bulge * a.bulge;
                if !squared_bulge.is_finite() {
                    return Err(ProfileError("profile arc arithmetic overflow".into()));
                }
                let half = chord * 0.5;
                let h = half * (1.0 - squared_bulge) / (2.0 * a.bulge);
                let nx = -dy / chord;
                let ny = dx / chord;
                let cx = 0.5 * (a.u + b.u) + nx * h;
                let cy = 0.5 * (a.v + b.v) + ny * h;
                let radius = (a.u - cx).hypot(a.v - cy);
                checked_values(&[h, cx, cy, radius], "arc arithmetic")?;
                let a0 = (a.v - cy).atan2(a.u - cx);
                let steps = arc_steps(theta);
                for s in 1..steps {
                    let ang = a0 + theta * (s as f64 / steps as f64);
                    let p = [cx + radius * ang.cos(), cy + radius * ang.sin()];
                    checked_values(&p, "arc coordinate")?;
                    push_unique(&mut out, p);
                }
            }
        }
    }
    if !closed
        && let Some(last) = points.last()
    {
        push_unique(&mut out, [last.u, last.v]);
    }
    if closed && out.len() >= 2
        && let (Some(first), Some(last)) = (out.first(), out.last())
        && (first[0] - last[0]).hypot(first[1] - last[1]) < DEDUP_TOL
    {
        out.pop();
    }
    Ok(out)
}

fn push_unique(pts: &mut Vec<[f64; 2]>, p: [f64; 2]) {
    if let Some(prev) = pts.last()
        && (prev[0] - p[0]).hypot(prev[1] - p[1]) < DEDUP_TOL
    {
        return;
    }
    pts.push(p);
}

#[must_use]
pub(crate) fn signed_area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        if let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) {
            sum += a[0] * b[1] - b[0] * a[1];
        }
    }
    sum * 0.5
}

fn cross2(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    let min_x = a[0].min(b[0]) - CROSS_TOL;
    let max_x = a[0].max(b[0]) + CROSS_TOL;
    let min_y = a[1].min(b[1]) - CROSS_TOL;
    let max_y = a[1].max(b[1]) + CROSS_TOL;
    p[0] >= min_x && p[0] <= max_x && p[1] >= min_y && p[1] <= max_y
}

fn segs_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let c1 = cross2(a, b, c);
    let c2 = cross2(a, b, d);
    let c3 = cross2(c, d, a);
    let c4 = cross2(c, d, b);
    if ((c1 > CROSS_TOL && c2 < -CROSS_TOL) || (c1 < -CROSS_TOL && c2 > CROSS_TOL))
        && ((c3 > CROSS_TOL && c4 < -CROSS_TOL) || (c3 < -CROSS_TOL && c4 > CROSS_TOL))
    {
        return true;
    }
    (c1.abs() <= CROSS_TOL && on_segment(a, b, c))
        || (c2.abs() <= CROSS_TOL && on_segment(a, b, d))
        || (c3.abs() <= CROSS_TOL && on_segment(c, d, a))
        || (c4.abs() <= CROSS_TOL && on_segment(c, d, b))
}

/// Refuse self-crossing polygon or open chain boundaries by edge index.
pub(crate) fn check_no_self_crossings(pts: &[[f64; 2]], closed: bool, kind: &str) -> Result<(), ProfileError> {
    let n = pts.len();
    let edge_count = if closed { n } else { n.saturating_sub(1) };
    if edge_count < 2 {
        return Ok(());
    }
    for i in 0..edge_count {
        let i_next = (i + 1) % n;
        let (Some(&a), Some(&b)) = (pts.get(i), pts.get(i_next)) else { continue };
        for j in (i + 2)..edge_count {
            if closed && i == 0 && j + 1 == edge_count {
                continue;
            }
            let j_next = (j + 1) % n;
            let (Some(&c), Some(&d)) = (pts.get(j), pts.get(j_next)) else { continue };
            if segs_intersect(a, b, c, d) {
                return Err(ProfileError(format!(
                    "{kind} self-crosses between edges #{i} and #{j}"
                )));
            }
        }
    }
    Ok(())
}

/// Normalize a 2D closed profile to strictly positive CCW area with no crossings.
pub(crate) fn validate_and_orient_closed(points: &[ProfileVertex]) -> Result<Vec<[f64; 2]>, ProfileError> {
    let mut poly = flatten_edges(points, true)?;
    if poly.len() < 3 {
        return Err(ProfileError("closed profile needs at least three non-collinear vertices".into()));
    }
    check_no_self_crossings(&poly, true, "extruded profile")?;
    let area = signed_area(&poly);
    if area.abs() < AREA_MIN {
        return Err(ProfileError("extruded profile encloses zero area".into()));
    }
    if area < 0.0 {
        poly.reverse();
    }
    Ok(poly)
}

/// Validate a spun half-profile and return a CCW non-crossing closed loop in `(z, r)`.
pub(crate) fn spun_loop(profile: &HalfProfile) -> Result<Vec<[f64; 2]>, ProfileError> {
    let mut poly = flatten_edges(&profile.points, false)?;
    if poly.len() < 2 {
        return Err(ProfileError("spun half-profile needs at least two vertices".into()));
    }
    for p in &mut poly {
        if p[1] < -1e-4 {
            return Err(ProfileError(format!(
                "spun half-profile crosses the centreline at r={:.4} mm",
                p[1]
            )));
        }
        if p[1] < 1e-4 {
            p[1] = 0.0;
        }
    }
    let closed_end = poly
        .first()
        .zip(poly.last())
        .is_some_and(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-3);
    if closed_end {
        poly.pop();
    } else {
        let on_axis = poly
            .first()
            .zip(poly.last())
            .is_some_and(|(a, b)| a[1] == 0.0 && b[1] == 0.0);
        if !on_axis {
            return Err(ProfileError(
                "spun half-profile must start and end on the centreline (or form a closed half-section loop)"
                    .into(),
            ));
        }
    }
    if poly.len() < 3 {
        return Err(ProfileError("spun half-profile encloses zero cross-section area".into()));
    }
    check_no_self_crossings(&poly, true, "spun half-profile")?;
    let area = signed_area(&poly);
    if area.abs() < AREA_MIN {
        return Err(ProfileError("spun half-profile encloses zero area".into()));
    }
    if area < 0.0 {
        poly.reverse();
    }
    Ok(poly)
}

/// Build a closed outward triangle mesh by revolving `profile` around the Z spindle axis.
pub fn spun_mesh(profile: &HalfProfile) -> Result<Vec<Triangle>, ProfileError> {
    let loop_zr = spun_loop(profile)?;
    let n = loop_zr.len();
    let mut tris = Vec::with_capacity(n * REVOLVE_RING * 2);
    let ring: Vec<(f64, f64)> = (0..REVOLVE_RING)
        .map(|k| {
            let ang = TAU * (k as f64) / (REVOLVE_RING as f64);
            (ang.cos(), ang.sin())
        })
        .collect();
    for i in 0..n {
        let (Some(&[z0, r0]), Some(&[z1, r1])) = (loop_zr.get(i), loop_zr.get((i + 1) % n)) else {
            continue;
        };
        if r0 == 0.0 && r1 == 0.0 {
            continue;
        }
        for k in 0..REVOLVE_RING {
            let (Some(&(c0, s0)), Some(&(c1, s1))) = (ring.get(k), ring.get((k + 1) % REVOLVE_RING)) else {
                continue;
            };
            let p00 = Point3::new(r0 * c0, r0 * s0, z0);
            let p01 = Point3::new(r0 * c1, r0 * s1, z0);
            let p10 = Point3::new(r1 * c0, r1 * s0, z1);
            let p11 = Point3::new(r1 * c1, r1 * s1, z1);
            if r0 == 0.0 {
                tris.push(Triangle::new(p00, p10, p11));
            } else if r1 == 0.0 {
                tris.push(Triangle::new(p00, p10, p01));
            } else {
                tris.push(Triangle::new(p00, p10, p11));
                tris.push(Triangle::new(p00, p11, p01));
            }
        }
    }
    if tris.is_empty() {
        return Err(ProfileError("spun mesh is empty".into()));
    }
    check_mesh(&tris, spun_chord_error(profile))?;
    Ok(tris)
}

/// Build a closed outward triangle mesh by extruding `profile` along `axis` by `depth_mm` (`> 0`).
pub fn extruded_mesh(
    profile: &ClosedProfile,
    axis: ExtrudeAxis,
    depth_mm: f64,
) -> Result<Vec<Triangle>, ProfileError> {
    if !depth_mm.is_finite() || depth_mm <= 0.0 {
        return Err(ProfileError(format!(
            "extrusion depth must be positive and finite (got {depth_mm})"
        )));
    }
    checked_values(&[depth_mm], "extrusion depth")?;
    let poly = validate_and_orient_closed(&profile.points)?;
    let cap_tris = ear_clip(&poly)?;
    let n = poly.len();
    let lift = |uv: [f64; 2], w: f64| match axis {
        ExtrudeAxis::Z => Point3::new(uv[0], uv[1], w),
        ExtrudeAxis::Y => Point3::new(uv[0], w, uv[1]),
        ExtrudeAxis::X => Point3::new(w, uv[0], uv[1]),
    };
    let right_handed = !matches!(axis, ExtrudeAxis::Y);
    let mut tris = Vec::with_capacity(cap_tris.len() * 2 + n * 2);
    for [a, b, c] in cap_tris {
        let (Some(&pa), Some(&pb), Some(&pc)) = (poly.get(a), poly.get(b), poly.get(c)) else {
            continue;
        };
        let (a0, b0, c0) = (lift(pa, 0.0), lift(pb, 0.0), lift(pc, 0.0));
        let (ad, bd, cd) = (lift(pa, depth_mm), lift(pb, depth_mm), lift(pc, depth_mm));
        if right_handed {
            tris.push(Triangle::new(a0, c0, b0));
            tris.push(Triangle::new(ad, bd, cd));
        } else {
            tris.push(Triangle::new(a0, b0, c0));
            tris.push(Triangle::new(ad, cd, bd));
        }
    }
    for i in 0..n {
        let (Some(&p0), Some(&p1)) = (poly.get(i), poly.get((i + 1) % n)) else {
            continue;
        };
        let (a0, b0) = (lift(p0, 0.0), lift(p1, 0.0));
        let (ad, bd) = (lift(p0, depth_mm), lift(p1, depth_mm));
        if right_handed {
            tris.push(Triangle::new(a0, b0, bd));
            tris.push(Triangle::new(a0, bd, ad));
        } else {
            tris.push(Triangle::new(a0, bd, b0));
            tris.push(Triangle::new(a0, ad, bd));
        }
    }
    check_mesh(&tris, profile_chord_error(&profile.points, true))?;
    Ok(tris)
}

fn ear_clip(poly: &[[f64; 2]]) -> Result<Vec<[usize; 3]>, ProfileError> {
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut out = Vec::with_capacity(poly.len().saturating_sub(2));
    while idx.len() > 3 {
        let m = idx.len();
        let mut found = None;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (Some(&a), Some(&b), Some(&c)) = (poly.get(ia), poly.get(ib), poly.get(ic)) else {
                continue;
            };
            if cross2(a, b, c) <= 1e-12 {
                continue;
            }
            let empty = idx.iter().all(|&j| {
                if j == ia || j == ib || j == ic {
                    return true;
                }
                let Some(&p) = poly.get(j) else { return true };
                !point_in_tri(p, a, b, c)
            });
            if empty {
                found = Some((k, [ia, ib, ic]));
                break;
            }
        }
        let Some((k, tri)) = found else {
            return Err(ProfileError("could not triangulate profile polygon".into()));
        };
        out.push(tri);
        idx.remove(k);
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    Ok(out)
}

fn point_in_tri(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    cross2(a, b, p) >= -1e-12 && cross2(b, c, p) >= -1e-12 && cross2(c, a, p) >= -1e-12
}

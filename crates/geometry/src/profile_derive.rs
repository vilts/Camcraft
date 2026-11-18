//! Derive candidate extruded or spun fixture silhouettes from closed 3D
//! facet meshes (#744) while leaving the original solid as the source.
//! Provenance: Andrew's monotone chain 2D convex hull + axial radial binning.

use crate::dxf_profile::{ClosedProfile, ExtrudeAxis, HalfProfile, ProfileError, ProfileVertex};
use crate::geometry::{Point3, Triangle};
use crate::profile_mesh::{signed_area, spun_loop, validate_and_orient_closed};

const SPAN_MIN: f64 = 1e-3;

/// Derive a convex extruded bounding/silhouette profile + depth from solid facets.
pub fn derive_extruded_from_facets(
    facets: &[Triangle],
    axis: ExtrudeAxis,
) -> Result<(ClosedProfile, f64), ProfileError> {
    if facets.is_empty() {
        return Err(ProfileError("cannot derive extruded profile from empty facets".into()));
    }
    let project = |p: Point3| -> ([f64; 2], f64) {
        match axis {
            ExtrudeAxis::Z => ([p.x, p.y], p.z),
            ExtrudeAxis::Y => ([p.x, p.z], p.y),
            ExtrudeAxis::X => ([p.y, p.z], p.x),
        }
    };
    let (mut w_lo, mut w_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut pts = Vec::with_capacity(facets.len() * 3);
    for t in facets {
        for v in [t.a, t.b, t.c] {
            let (uv, w) = project(v);
            w_lo = w_lo.min(w);
            w_hi = w_hi.max(w);
            pts.push(uv);
        }
    }
    let hull = convex_hull_2d(pts)?;
    let depth = (w_hi - w_lo).max(SPAN_MIN);
    let points = hull
        .into_iter()
        .map(|[u, v]| ProfileVertex { u, v, bulge: 0.0 })
        .collect();
    let prof = ClosedProfile { points };
    let _ = validate_and_orient_closed(&prof.points)?;
    Ok((prof, depth))
}

/// Derive a spun axial radial envelope half-profile from solid facets around Z.
pub fn derive_spun_from_facets(facets: &[Triangle]) -> Result<HalfProfile, ProfileError> {
    if facets.is_empty() {
        return Err(ProfileError("cannot derive spun profile from empty facets".into()));
    }
    let (mut z_lo, mut z_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut r_max = 0.0_f64;
    for t in facets {
        for v in [t.a, t.b, t.c] {
            z_lo = z_lo.min(v.z);
            z_hi = z_hi.max(v.z);
            r_max = r_max.max(v.x.hypot(v.y));
        }
    }
    if !(z_hi > z_lo && r_max > SPAN_MIN) {
        return Err(ProfileError("solid has degenerate axial or radial span".into()));
    }
    let bins = 8usize;
    let dz = (z_hi - z_lo) / (bins as f64);
    let mut points = vec![ProfileVertex { u: z_lo, v: 0.0, bulge: 0.0 }];
    for b in 0..bins {
        let b_lo = z_lo + (b as f64) * dz - 1e-6;
        let b_hi = z_lo + ((b + 1) as f64) * dz + 1e-6;
        let mut r = 0.0_f64;
        for t in facets {
            for v in [t.a, t.b, t.c] {
                if v.z >= b_lo && v.z <= b_hi {
                    r = r.max(v.x.hypot(v.y));
                }
            }
        }
        let z0 = z_lo + (b as f64) * dz;
        let z1 = z_lo + ((b + 1) as f64) * dz;
        if r > SPAN_MIN {
            points.push(ProfileVertex { u: z0, v: r, bulge: 0.0 });
            points.push(ProfileVertex { u: z1, v: r, bulge: 0.0 });
        }
    }
    points.push(ProfileVertex { u: z_hi, v: 0.0, bulge: 0.0 });
    let half = HalfProfile { points };
    let _ = spun_loop(&half)?;
    Ok(half)
}

fn convex_hull_2d(mut pts: Vec<[f64; 2]>) -> Result<Vec<[f64; 2]>, ProfileError> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6);
    if pts.len() < 3 {
        return Err(ProfileError("projected facets have fewer than three points".into()));
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    if signed_area(&lower).abs() < 1e-6 {
        return Err(ProfileError("projected hull has zero area".into()));
    }
    Ok(lower)
}

//! Arc geometry resolution (`I`/`J`/`K` center offsets and `R` radius arcs) in G17/G18/G19 planes.
//! Provenance: RS-274/NGC circular/helical interpolation rules (NISTIR 6556 §3.5.3) and `camcraft_cl::cl::ArcPlane` handedness.

use camcraft_cl::cl::ArcPlane;
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::geometry::Point3;

use crate::types::{ArcCenterMode, GcodeError};

/// Resolved arc geometry in millimeters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedArc {
    /// Center in `plane`'s word order: `(X, Y)`, `(X, Z)`, or `(Y, Z)`.
    pub center_words: Point2,
    /// Present for a full 360° `I/J/K` circle/helix so the caller splits it into two 180° pieces.
    pub split_mid: Option<Point3>,
}

/// Resolve and validate an arc from `from` to `to` in `plane`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_arc(
    line: usize,
    from: Point3,
    to: Point3,
    plane: ArcPlane,
    ccw: bool,
    center_mode: ArcCenterMode,
    ijk_mm: [Option<f64>; 3],
    r_mm: Option<f64>,
    tol_mm: f64,
) -> Result<ResolvedArc, GcodeError> {
    let has_ijk = ijk_mm.iter().any(Option::is_some);
    if r_mm.is_some() && has_ijk {
        return Err(GcodeError::InvalidArc {
            line,
            message: "cannot specify both R radius and I/J/K center words".to_string(),
        });
    }

    match plane {
        ArcPlane::Xy if ijk_mm[2].is_some() => {
            return Err(GcodeError::InvalidArc {
                line,
                message: "K offset is not valid in G17 (XY) arc plane".to_string(),
            });
        }
        ArcPlane::Xz if ijk_mm[1].is_some() => {
            return Err(GcodeError::InvalidArc {
                line,
                message: "J offset is not valid in G18 (XZ) arc plane".to_string(),
            });
        }
        ArcPlane::Yz if ijk_mm[0].is_some() => {
            return Err(GcodeError::InvalidArc {
                line,
                message: "I offset is not valid in G19 (YZ) arc plane".to_string(),
            });
        }
        _ => {}
    }

    if let Some(r) = r_mm {
        return resolve_r_arc(line, from, to, plane, ccw, r, tol_mm);
    }
    resolve_ijk_arc(line, from, to, plane, center_mode, ijk_mm, tol_mm)
}

fn resolve_r_arc(
    line: usize,
    from: Point3,
    to: Point3,
    plane: ArcPlane,
    ccw: bool,
    r: f64,
    tol_mm: f64,
) -> Result<ResolvedArc, GcodeError> {
    if !r.is_finite() || r.abs() <= 1e-6 {
        return Err(GcodeError::InvalidArc {
            line,
            message: "arc radius R must be finite and non-zero".to_string(),
        });
    }
    let (u0, v0, _) = plane.uvw(from);
    let (u1, v1, _) = plane.uvw(to);
    let du = u1 - u0;
    let dv = v1 - v0;
    let d = du.hypot(dv);
    if d <= 1e-6 {
        return Err(GcodeError::InvalidArc {
            line,
            message: "full-circle or zero-length arc with R word is not supported; use I/J/K offsets".to_string(),
        });
    }

    let r_abs = r.abs();
    let max_chord = 2.0 * r_abs + tol_mm;
    if d > max_chord {
        return Err(GcodeError::InvalidArc {
            line,
            message: format!(
                "arc endpoint distance {d:.4} mm exceeds diameter 2*|R| ({:.4} mm)",
                2.0 * r_abs
            ),
        });
    }

    let half_d = 0.5 * d;
    let h = (r_abs * r_abs - half_d * half_d).max(0.0).sqrt();
    let tu = du / d;
    let tv = dv / d;
    // Left normal (-tv, tu) gives minor arc for CCW (G3) and major arc for CW (G2).
    let sign = if (ccw && r > 0.0) || (!ccw && r < 0.0) {
        1.0
    } else {
        -1.0
    };
    let cu = 0.5 * (u0 + u1) + sign * h * (-tv);
    let cv = 0.5 * (v0 + v1) + sign * h * tu;
    let center_words = plane.words(plane.point(cu, cv, 0.0));
    Ok(ResolvedArc {
        center_words,
        split_mid: None,
    })
}

fn resolve_ijk_arc(
    line: usize,
    from: Point3,
    to: Point3,
    plane: ArcPlane,
    center_mode: ArcCenterMode,
    ijk_mm: [Option<f64>; 3],
    tol_mm: f64,
) -> Result<ResolvedArc, GcodeError> {
    let (a_opt, b_opt) = match plane {
        ArcPlane::Xy => (ijk_mm[0], ijk_mm[1]),
        ArcPlane::Xz => (ijk_mm[0], ijk_mm[2]),
        ArcPlane::Yz => (ijk_mm[1], ijk_mm[2]),
    };

    let center_words = match center_mode {
        ArcCenterMode::Incremental => {
            if a_opt.is_none() && b_opt.is_none() {
                return Err(GcodeError::InvalidArc {
                    line,
                    message: "arc requires I/J/K offsets or R radius".to_string(),
                });
            }
            let fw = plane.words(from);
            Point2::new(fw.x + a_opt.unwrap_or(0.0), fw.y + b_opt.unwrap_or(0.0))
        }
        ArcCenterMode::Absolute => {
            let (Some(a), Some(b)) = (a_opt, b_opt) else {
                return Err(GcodeError::InvalidArc {
                    line,
                    message: "G90.1 absolute arc requires both in-plane center words".to_string(),
                });
            };
            Point2::new(a, b)
        }
    };

    let (cu, cv) = plane.center_uv(center_words);
    let (u0, v0, w0) = plane.uvw(from);
    let (u1, v1, w1) = plane.uvw(to);
    let r0 = (u0 - cu).hypot(v0 - cv);
    let r1 = (u1 - cu).hypot(v1 - cv);
    if !r0.is_finite() || !r1.is_finite() || r0 <= 1e-6 || r1 <= 1e-6 {
        return Err(GcodeError::InvalidArc {
            line,
            message: "arc center coincides with endpoint (zero radius)".to_string(),
        });
    }
    if (r0 - r1).abs() > tol_mm {
        return Err(GcodeError::InvalidArc {
            line,
            message: format!(
                "arc radius mismatch: start radius {r0:.4} mm vs end radius {r1:.4} mm"
            ),
        });
    }

    let split_mid = if (u1 - u0).hypot(v1 - v0) <= 1e-6 {
        let mu = 2.0 * cu - u0;
        let mv = 2.0 * cv - v0;
        let mw = 0.5 * (w0 + w1);
        Some(plane.point(mu, mv, mw))
    } else {
        None
    };

    Ok(ResolvedArc {
        center_words,
        split_mid,
    })
}

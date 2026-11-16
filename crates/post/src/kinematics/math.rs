// Provenance: clean-room implementation from public 5-axis XYZAC trunnion geometry and Rodrigues rotation math.

//! Vector algebra, Rodrigues rotation, SLERP, configuration validation, and forward kinematics.

use super::types::{
    AcTrunnionConfig, KinematicsError, MachinePose, PartPose, RotaryAxisConfig, ValidatedTrunnion,
};

const PARALLEL_SIN_TOL: f64 = 1e-6;
const ZERO_VEC_TOL: f64 = 1e-9;

#[inline]
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub(crate) fn norm(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

#[inline]
pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub(crate) fn scale(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

#[inline]
pub(crate) fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub(crate) fn ensure_finite_scalar(value: f64, field: &'static str) -> Result<f64, KinematicsError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(KinematicsError::NonFiniteInput { field })
    }
}

pub(crate) fn ensure_finite_vec(
    v: [f64; 3],
    field: &'static str,
) -> Result<[f64; 3], KinematicsError> {
    if v[0].is_finite() && v[1].is_finite() && v[2].is_finite() {
        Ok(v)
    } else {
        Err(KinematicsError::NonFiniteInput { field })
    }
}

pub(crate) fn normalize_finite(
    v: [f64; 3],
    field: &'static str,
) -> Result<[f64; 3], KinematicsError> {
    ensure_finite_vec(v, field)?;
    let n = norm(v);
    if n < ZERO_VEC_TOL {
        return Err(KinematicsError::ZeroVector { field });
    }
    Ok([v[0] / n, v[1] / n, v[2] / n])
}

/// Rotates `v` around unit vector `axis` by `angle_rad` using Rodrigues' rotation formula.
#[inline]
pub(crate) fn rodrigues(axis: [f64; 3], angle_rad: f64, v: [f64; 3]) -> [f64; 3] {
    let (s, c) = angle_rad.sin_cos();
    let kv = dot(axis, v);
    let kxv = cross(axis, v);
    let one_minus_c = 1.0 - c;
    [
        v[0] * c + kxv[0] * s + axis[0] * (kv * one_minus_c),
        v[1] * c + kxv[1] * s + axis[1] * (kv * one_minus_c),
        v[2] * c + kxv[2] * s + axis[2] * (kv * one_minus_c),
    ]
}

/// Spherical linear interpolation between two unit vectors `a` and `b` at parameter `t` in `[0, 1]`.
pub(crate) fn slerp_unit(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    let cos_theta = dot(a, b).clamp(-1.0, 1.0);
    let sin_theta = norm(cross(a, b));
    if sin_theta < 1e-9 && cos_theta > 0.0 {
        let blended = lerp3(a, b, t);
        let n = norm(blended);
        return if n > ZERO_VEC_TOL {
            scale(blended, 1.0 / n)
        } else {
            a
        };
    }
    if sin_theta < 1e-9 && cos_theta < 0.0 {
        // Near 180 deg opposition: choose a deterministic orthogonal pivot vector.
        let seed = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let ortho = cross(a, seed);
        let on = norm(ortho);
        let pivot = if on > ZERO_VEC_TOL {
            scale(ortho, 1.0 / on)
        } else {
            [0.0, 1.0, 0.0]
        };
        return rodrigues(pivot, std::f64::consts::PI * t, a);
    }
    let theta = sin_theta.atan2(cos_theta);
    let w0 = ((1.0 - t) * theta).sin() / sin_theta;
    let w1 = (t * theta).sin() / sin_theta;
    let combined = add(scale(a, w0), scale(b, w1));
    let n = norm(combined);
    if n > ZERO_VEC_TOL {
        scale(combined, 1.0 / n)
    } else {
        a
    }
}

/// Validates axis geometry, travel windows, weights, and tolerances before solving poses.
pub fn validate_trunnion(cfg: &AcTrunnionConfig) -> Result<ValidatedTrunnion, KinematicsError> {
    let a_unit = validate_rotary_axis(&cfg.a, "rotary axis A")?;
    let c_unit = validate_rotary_axis(&cfg.c, "rotary axis C")?;
    let spindle_unit = normalize_finite(cfg.spindle_axis, "spindle axis").map_err(|err| {
        KinematicsError::InvalidConfig {
            message: format!("invalid spindle axis: {err}"),
        }
    })?;

    if norm(cross(a_unit, c_unit)) < PARALLEL_SIN_TOL {
        return Err(KinematicsError::InvalidConfig {
            message: format!(
                "axis {} and axis {} cannot be parallel",
                cfg.a.name, cfg.c.name
            ),
        });
    }
    if norm(cross(a_unit, spindle_unit)) < PARALLEL_SIN_TOL {
        return Err(KinematicsError::InvalidConfig {
            message: format!(
                "axis {} and spindle axis cannot be parallel",
                cfg.a.name
            ),
        });
    }

    let a_pivot = ensure_finite_vec(cfg.a_pivot, "a_pivot")?;
    let c_offset = ensure_finite_vec(cfg.c_offset, "c_offset")?;
    let wcs_offset = ensure_finite_vec(cfg.wcs_offset, "wcs_offset")?;
    let work_datum = ensure_finite_vec(cfg.work_datum, "work_datum")?;
    let pole_sin_tol = ensure_finite_scalar(cfg.pole_sin_tol, "pole_sin_tol")?;
    if pole_sin_tol <= 0.0 || pole_sin_tol >= 1.0 {
        return Err(KinematicsError::InvalidConfig {
            message: format!(
                "pole_sin_tol must lie strictly in (0, 1), got {}",
                cfg.pole_sin_tol
            ),
        });
    }
    let weight_a = ensure_finite_scalar(cfg.weight_a, "weight_a")?;
    let weight_c = ensure_finite_scalar(cfg.weight_c, "weight_c")?;
    if weight_a < 0.0 || weight_c < 0.0 || (weight_a == 0.0 && weight_c == 0.0) {
        return Err(KinematicsError::InvalidConfig {
            message: format!(
                "branch weights (weight_a={weight_a}, weight_c={weight_c}) must be non-negative with a positive sum"
            ),
        });
    }
    let initial_c_deg = ensure_finite_scalar(cfg.initial_c_deg, "initial_c_deg")?;

    Ok(ValidatedTrunnion {
        a: cfg.a.clone(),
        c: cfg.c.clone(),
        a_unit,
        c_unit,
        spindle_unit,
        a_pivot,
        c_offset,
        wcs_offset,
        work_datum,
        sign: cfg.sign,
        pole_sin_tol,
        weight_a,
        weight_c,
        initial_c_deg,
    })
}

fn validate_rotary_axis(
    axis: &RotaryAxisConfig,
    label: &'static str,
) -> Result<[f64; 3], KinematicsError> {
    if axis.name.trim().is_empty() {
        return Err(KinematicsError::InvalidConfig {
            message: format!("{label} name cannot be empty"),
        });
    }
    if !axis.axis[0].is_finite() || !axis.axis[1].is_finite() || !axis.axis[2].is_finite() {
        return Err(KinematicsError::InvalidConfig {
            message: format!("axis {} direction vector contains non-finite values", axis.name),
        });
    }
    let n = norm(axis.axis);
    if n < ZERO_VEC_TOL {
        return Err(KinematicsError::InvalidConfig {
            message: format!("axis {} direction vector cannot be zero", axis.name),
        });
    }
    if !axis.zero_offset_deg.is_finite() {
        return Err(KinematicsError::InvalidConfig {
            message: format!("axis {} zero_offset_deg must be finite", axis.name),
        });
    }
    if let Some((lo, hi)) = axis.travel {
        if !lo.is_finite() || !hi.is_finite() {
            return Err(KinematicsError::InvalidConfig {
                message: format!("axis {} travel bounds must be finite", axis.name),
            });
        }
        if lo > hi {
            return Err(KinematicsError::InvalidConfig {
                message: format!("axis {} travel bounds are inverted ({lo} > {hi})", axis.name),
            });
        }
    }
    Ok([axis.axis[0] / n, axis.axis[1] / n, axis.axis[2] / n])
}

/// Evaluates forward kinematics from a validated trunnion, returning machine pose and rotated tool axis.
pub(crate) fn forward_machine_validated(
    v: &ValidatedTrunnion,
    part: PartPose,
    a_deg: f64,
    c_deg: f64,
) -> Result<(MachinePose, [f64; 3]), KinematicsError> {
    let tip = ensure_finite_vec(part.tip, "part tip")?;
    let u = normalize_finite(part.axis, "part tool axis")?;
    let a_cmd = ensure_finite_scalar(a_deg, "a_deg")?;
    let c_cmd = ensure_finite_scalar(c_deg, "c_deg")?;

    let phi_a = v.sign.factor() * (a_cmd - v.a.zero_offset_deg).to_radians();
    let phi_c = v.sign.factor() * (c_cmd - v.c.zero_offset_deg).to_radians();

    let p_c_rot = rodrigues(v.c_unit, phi_c, add(tip, v.wcs_offset));
    let p_in_a = add(v.c_offset, p_c_rot);
    let p_mach = add(v.a_pivot, rodrigues(v.a_unit, phi_a, p_in_a));
    let xyz = sub(p_mach, v.work_datum);

    let axis_after_c = rodrigues(v.c_unit, phi_c, u);
    let axis_mach = normalize_finite(rodrigues(v.a_unit, phi_a, axis_after_c), "machine tool axis")?;

    Ok((
        MachinePose {
            xyz,
            a_deg: a_cmd,
            c_deg: c_cmd,
        },
        axis_mach,
    ))
}

/// Evaluates inverse/table-to-part forward kinematics from machine coordinates to part tip and axis.
pub(crate) fn forward_part_validated(
    v: &ValidatedTrunnion,
    machine: MachinePose,
) -> Result<PartPose, KinematicsError> {
    let xyz = ensure_finite_vec(machine.xyz, "machine xyz")?;
    let a_cmd = ensure_finite_scalar(machine.a_deg, "machine a_deg")?;
    let c_cmd = ensure_finite_scalar(machine.c_deg, "machine c_deg")?;

    let phi_a = v.sign.factor() * (a_cmd - v.a.zero_offset_deg).to_radians();
    let phi_c = v.sign.factor() * (c_cmd - v.c.zero_offset_deg).to_radians();

    let p_mach = add(xyz, v.work_datum);
    let p_in_a = rodrigues(v.a_unit, -phi_a, sub(p_mach, v.a_pivot));
    let tip = sub(
        rodrigues(v.c_unit, -phi_c, sub(p_in_a, v.c_offset)),
        v.wcs_offset,
    );

    let axis_in_a = rodrigues(v.a_unit, -phi_a, v.spindle_unit);
    let axis = normalize_finite(rodrigues(v.c_unit, -phi_c, axis_in_a), "part tool axis")?;

    Ok(PartPose { tip, axis })
}

/// Maps a part pose and `(A, C)` angles into machine coordinates and rotated tool axis in machine space.
pub fn forward_machine(
    cfg: &AcTrunnionConfig,
    part: PartPose,
    a_deg: f64,
    c_deg: f64,
) -> Result<(MachinePose, [f64; 3]), KinematicsError> {
    let v = validate_trunnion(cfg)?;
    forward_machine_validated(&v, part, a_deg, c_deg)
}

/// Maps a machine pose `(X, Y, Z, A, C)` back to part tip position and unit tool axis.
pub fn forward_part(
    cfg: &AcTrunnionConfig,
    machine: MachinePose,
) -> Result<PartPose, KinematicsError> {
    let v = validate_trunnion(cfg)?;
    forward_part_validated(&v, machine)
}

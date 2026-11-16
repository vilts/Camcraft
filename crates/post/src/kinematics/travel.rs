// Provenance: clean-room implementation from public 5-axis rotary travel window wrapping rules.

//! Travel-limit validation and `360`-degree turn wrapping helpers for rotary axes.

use std::f64::consts::PI;

use crate::cl::unwrap_near;

use super::types::{KinematicsError, RotaryAxisConfig};

const TRAVEL_EPS_DEG: f64 = 1e-9;

/// Selects the valid `base_deg + 360*k` turn inside `travel` closest to `target_deg`.
pub(crate) fn pick_turn_in_travel(
    base_deg: f64,
    travel: Option<(f64, f64)>,
    target_deg: f64,
    axis_name: &str,
    hint: &str,
) -> Result<f64, KinematicsError> {
    let base = wrap_deg_signed(base_deg);
    if travel.is_none() {
        return Ok(unwrap_near(base, target_deg));
    }
    let turns = valid_turns_in_travel(base, travel).map_err(|(lo, hi, nearest)| {
        KinematicsError::TravelLimitExceeded {
            axis_name: axis_name.to_string(),
            required_deg: nearest,
            min_deg: lo,
            max_deg: hi,
            hint: hint.to_string(),
        }
    })?;
    let mut best: Option<(f64, f64)> = None;
    for candidate in turns {
        let dist = (candidate - target_deg).abs();
        let replace = match best {
            None => true,
            Some((best_val, best_dist)) => {
                dist < best_dist - 1e-12
                    || ((dist - best_dist).abs() <= 1e-12 && candidate.abs() < best_val.abs())
            }
        };
        if replace {
            best = Some((candidate, dist));
        }
    }
    best.map(|(v, _)| v).ok_or_else(|| KinematicsError::InvalidConfig {
        message: format!("axis {axis_name} produced no valid travel turn"),
    })
}

/// Enumerates all `base_deg + 360*k` turns lying within `travel`.
pub(crate) fn valid_turns_in_travel(
    base_deg: f64,
    travel: Option<(f64, f64)>,
) -> Result<Vec<f64>, (f64, f64, f64)> {
    let base = wrap_deg_signed(base_deg);
    let Some((lo, hi)) = travel else {
        return Ok(vec![base]);
    };
    let k_min = ((lo - base - TRAVEL_EPS_DEG) / 360.0).ceil() as i32;
    let k_max = ((hi - base + TRAVEL_EPS_DEG) / 360.0).floor() as i32;
    let mut vals = Vec::new();
    for k in k_min..=k_max {
        let raw = base + 360.0 * f64::from(k);
        if raw >= lo - TRAVEL_EPS_DEG && raw <= hi + TRAVEL_EPS_DEG {
            vals.push(raw.clamp(lo, hi));
        }
    }
    if vals.is_empty() {
        Err((lo, hi, unwrap_near(base, 0.5 * (lo + hi))))
    } else {
        Ok(vals)
    }
}

/// Clamps `angle_deg` onto `travel` within floating-point tolerance, or returns `None`.
pub(crate) fn check_in_travel(angle_deg: f64, travel: Option<(f64, f64)>) -> Option<f64> {
    match travel {
        None => Some(angle_deg),
        Some((lo, hi)) => {
            if angle_deg >= lo - TRAVEL_EPS_DEG && angle_deg <= hi + TRAVEL_EPS_DEG {
                Some(angle_deg.clamp(lo, hi))
            } else {
                None
            }
        }
    }
}

/// Builds a named `C`-axis travel refusal error.
pub(crate) fn travel_err_c(
    axis: &RotaryAxisConfig,
    required_deg: f64,
    hint: &str,
) -> KinematicsError {
    let (min_deg, max_deg) = axis.travel.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
    KinematicsError::TravelLimitExceeded {
        axis_name: axis.name.clone(),
        required_deg,
        min_deg,
        max_deg,
        hint: hint.to_string(),
    }
}

/// Normalizes an angle in radians into `(-PI, PI]`.
#[inline]
pub(crate) fn wrap_rad_signed(angle: f64) -> f64 {
    let two_pi = 2.0 * PI;
    let mut r = angle.rem_euclid(two_pi);
    if r > PI {
        r -= two_pi;
    }
    if r == -0.0 {
        0.0
    } else {
        r
    }
}

/// Normalizes an angle in degrees into `(-180, 180]`.
#[inline]
pub(crate) fn wrap_deg_signed(deg: f64) -> f64 {
    let mut r = deg.rem_euclid(360.0);
    if r > 180.0 {
        r -= 360.0;
    }
    if r == -0.0 {
        0.0
    } else {
        r
    }
}

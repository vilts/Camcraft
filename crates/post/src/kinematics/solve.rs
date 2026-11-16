// Provenance: clean-room implementation from public 5-axis XYZAC trunnion geometry and Rodrigues rotation math.

//! Analytical Rodrigues AC inverse kinematics, pole holding, branch continuity, and C unwrapping.

use std::f64::consts::PI;

use crate::cl::unwrap_near;

use super::math::{
    cross, dot, forward_machine_validated, norm, normalize_finite, rodrigues, scale, sub,
    validate_trunnion,
};
use super::travel::{
    check_in_travel, pick_turn_in_travel, travel_err_c, valid_turns_in_travel, wrap_deg_signed,
    wrap_rad_signed,
};
use super::types::{
    AcCandidate, AcTrunnionConfig, KinematicsError, MachinePose, PartPose, SectionBoundary,
    ValidatedTrunnion,
};

/// Stateful modal XYZAC inverse-kinematics solver tracking branch continuity and C winding.
#[derive(Clone, Debug)]
pub struct AcSolver {
    v: ValidatedTrunnion,
    modal: Option<(f64, f64)>,
}

impl AcSolver {
    /// Creates a new solver after validating trunnion axis geometry, limits, and weights.
    pub fn new(config: &AcTrunnionConfig) -> Result<Self, KinematicsError> {
        let v = validate_trunnion(config)?;
        Ok(Self { v, modal: None })
    }

    /// Returns a reference to the validated trunnion configuration used by this solver.
    pub fn trunnion(&self) -> &ValidatedTrunnion {
        &self.v
    }

    /// Returns the most recently solved `(A, C)` command angles (deg), if any pose was solved.
    pub fn current_angles(&self) -> Option<(f64, f64)> {
        self.modal
    }

    /// Resets modal `(A, C)` history back to the initial unmoved state.
    pub fn reset(&mut self) {
        self.modal = None;
    }

    pub(crate) fn set_modal_angles(&mut self, a_deg: f64, c_deg: f64) {
        self.modal = Some((a_deg, c_deg));
    }

    /// Re-wraps the current modal `C` angle into its primary turn inside `C.travel` at a safe retract.
    pub fn rewrap_c_at_retract(&mut self) -> Result<Option<f64>, KinematicsError> {
        let Some((a_cmd, c_cmd)) = self.modal else {
            return Ok(None);
        };
        let target = if self.v.c.rollover { 180.0 } else { 0.0 };
        let c_base = wrap_deg_signed(c_cmd);
        let wrapped = pick_turn_in_travel(c_base, self.v.c.travel, target, &self.v.c.name, "")?;
        self.modal = Some((a_cmd, wrapped));
        Ok(Some(wrapped))
    }

    /// Solves one part pose into machine `(X, Y, Z, A, C)` coordinates, updating modal `(A, C)` state.
    pub fn solve_pose(
        &mut self,
        part: PartPose,
        boundary: SectionBoundary,
    ) -> Result<MachinePose, KinematicsError> {
        let _ = super::math::ensure_finite_vec(part.tip, "part tip")?;
        let u = normalize_finite(part.axis, "part tool axis")?;
        let (a_deg, c_deg) = self.select_angles_for_axis(u, boundary)?;
        let (mach, _) = forward_machine_validated(&self.v, part, a_deg, c_deg)?;
        self.modal = Some((mach.a_deg, mach.c_deg));
        Ok(mach)
    }

    fn select_angles_for_axis(
        &self,
        u: [f64; 3],
        boundary: SectionBoundary,
    ) -> Result<(f64, f64), KinematicsError> {
        let ref_c = self.modal.map_or(self.v.initial_c_deg, |(_, c)| c);
        let branches = analytical_branches_validated(&self.v, u, ref_c)?;

        let mut best: Option<((f64, f64), f64)> = None;
        let mut first_a_err: Option<KinematicsError> = None;
        let mut first_c_err: Option<KinematicsError> = None;

        let ref_a = self.modal.map_or(0.0, |(a, _)| a);
        let c_target_non_cont = match (boundary, self.modal) {
            (SectionBoundary::Continuous, None) => {
                if self.v.c.rollover {
                    180.0
                } else {
                    self.v.initial_c_deg
                }
            }
            (SectionBoundary::SafeRetract, _) => {
                if self.v.c.rollover {
                    180.0
                } else {
                    0.0
                }
            }
            (SectionBoundary::Continuous, Some(_)) => ref_c,
        };

        for cand in &branches {
            let a_choices = match valid_turns_in_travel(cand.a_deg, self.v.a.travel) {
                Ok(choices) => choices,
                Err((min_deg, max_deg, nearest)) => {
                    if first_a_err.is_none() {
                        first_a_err = Some(KinematicsError::TravelLimitExceeded {
                            axis_name: self.v.a.name.clone(),
                            required_deg: nearest,
                            min_deg,
                            max_deg,
                            hint: "tool tilt angle cannot fit within A travel limits on this branch"
                                .to_string(),
                        });
                    }
                    continue;
                }
            };

            let c_choice = if cand.at_pole {
                match (boundary, self.modal) {
                    (SectionBoundary::Continuous, Some((_, prev_c))) => {
                        match check_in_travel(prev_c, self.v.c.travel) {
                            Some(c_clamped) => Ok(c_clamped),
                            None => Err(travel_err_c(
                                &self.v.c,
                                prev_c,
                                "modal C angle sits outside configured C travel",
                            )),
                        }
                    }
                    (SectionBoundary::Continuous, None) => {
                        if check_in_travel(self.v.initial_c_deg, self.v.c.travel).is_some()
                            && !self.v.c.rollover
                        {
                            Ok(self.v.initial_c_deg)
                        } else {
                            pick_turn_in_travel(
                                self.v.initial_c_deg,
                                self.v.c.travel,
                                c_target_non_cont,
                                &self.v.c.name,
                                "initial C angle falls outside C travel",
                            )
                        }
                    }
                    (SectionBoundary::SafeRetract, _) => pick_turn_in_travel(
                        ref_c,
                        self.v.c.travel,
                        c_target_non_cont,
                        &self.v.c.name,
                        "re-wrapped C angle falls outside C travel",
                    ),
                }
            } else {
                match (boundary, self.modal) {
                    (SectionBoundary::Continuous, Some((_, prev_c))) => {
                        let unwrapped = unwrap_near(cand.c_deg, prev_c);
                        match check_in_travel(unwrapped, self.v.c.travel) {
                            Some(c_clamped) => Ok(c_clamped),
                            None => Err(travel_err_c(
                                &self.v.c,
                                unwrapped,
                                "C requires a checked unwind at clearance; no continuous C turn fits travel",
                            )),
                        }
                    }
                    (SectionBoundary::SafeRetract, _) | (SectionBoundary::Continuous, None) => {
                        pick_turn_in_travel(
                            cand.c_deg,
                            self.v.c.travel,
                            c_target_non_cont,
                            &self.v.c.name,
                            "no C turn lies within travel bounds",
                        )
                    }
                }
            };

            let c_cmd = match c_choice {
                Ok(val) => val,
                Err(err) => {
                    if first_c_err.is_none() {
                        first_c_err = Some(err);
                    }
                    continue;
                }
            };

            let c_cost_ref = match (boundary, self.modal) {
                (SectionBoundary::Continuous, Some((_, prev_c))) => prev_c,
                _ => c_target_non_cont,
            };

            for a_cmd in a_choices {
                let cost =
                    self.v.weight_a * (a_cmd - ref_a).abs() + self.v.weight_c * (c_cmd - c_cost_ref).abs();
                let replace = match best {
                    None => true,
                    Some((_, best_cost)) => cost < best_cost - 1e-12,
                };
                if replace {
                    best = Some(((a_cmd, c_cmd), cost));
                }
            }
        }

        if let Some((angles, _)) = best {
            return Ok(angles);
        }
        if let Some(c_err) = first_c_err {
            return Err(c_err);
        }
        if let Some(a_err) = first_a_err {
            return Err(a_err);
        }
        Err(KinematicsError::UnreachableOrientation {
            axis_name: self.v.a.name.clone(),
            reason: "no analytical branch satisfied machine rotary limits".to_string(),
        })
    }
}

/// Computes all analytical `(A, C)` solution branches before applying machine travel limits.
pub fn analytical_branches(
    cfg: &AcTrunnionConfig,
    part_axis: [f64; 3],
    modal_c_deg: f64,
) -> Result<Vec<AcCandidate>, KinematicsError> {
    let v = validate_trunnion(cfg)?;
    let u = normalize_finite(part_axis, "part tool axis")?;
    let c_fallback = super::math::ensure_finite_scalar(modal_c_deg, "modal_c_deg")?;
    analytical_branches_validated(&v, u, c_fallback)
}

pub(crate) fn analytical_branches_validated(
    v: &ValidatedTrunnion,
    u: [f64; 3],
    modal_c_deg: f64,
) -> Result<Vec<AcCandidate>, KinematicsError> {
    let k_a = v.a_unit;
    let k_c = v.c_unit;
    let s = v.spindle_unit;

    // Solve R(k_a, -phi_a) * s . k_c = k_c . u -> c1*cos(phi_a) + c2*sin(phi_a) = p - c0.
    let c0 = dot(k_c, k_a) * dot(k_a, s);
    let c1 = dot(k_c, s) - c0;
    let c2 = -dot(k_c, cross(k_a, s));
    let r_amp = norm(cross(k_c, k_a)) * norm(cross(s, k_a));

    let p = dot(k_c, u);
    let rhs = p - c0;
    if rhs.abs() > r_amp + 1e-9 {
        return Err(KinematicsError::UnreachableOrientation {
            axis_name: v.a.name.clone(),
            reason: format!(
                "projected axis component {rhs:.6} exceeds reachable envelope +/-{r_amp:.6}"
            ),
        });
    }

    let sin_to_c = norm(cross(k_c, u));
    let at_pole = sin_to_c <= v.pole_sin_tol;

    let sin_alpha = if c0.abs() <= 1e-12 && (r_amp - 1.0).abs() <= 1e-12 {
        // Exact non-canceling identity when A is orthogonal to both C and the spindle.
        sin_to_c
    } else {
        let diff = (r_amp - rhs.abs()).max(0.0);
        let sum = (r_amp + rhs.abs()).max(0.0);
        (diff * sum).sqrt()
    };

    let delta = c2.atan2(c1);
    let alpha = sin_alpha.atan2(rhs);

    let mut raw_phis = vec![wrap_rad_signed(delta + alpha)];
    let second_phi = wrap_rad_signed(delta - alpha);
    if (second_phi - raw_phis[0]).abs() > 1e-12
        && (second_phi - raw_phis[0]).abs() < (2.0 * PI - 1e-12)
    {
        raw_phis.push(second_phi);
    }

    let mut out = Vec::with_capacity(raw_phis.len());
    for phi_a in raw_phis {
        let c_deg = if at_pole {
            wrap_deg_signed(modal_c_deg)
        } else {
            let w = rodrigues(k_a, -phi_a, s);
            let u_perp = sub(u, scale(k_c, dot(k_c, u)));
            let w_perp = sub(w, scale(k_c, dot(k_c, w)));
            let sin_phi_c = dot(cross(k_c, u_perp), w_perp);
            let cos_phi_c = dot(u_perp, w_perp);
            let phi_c = sin_phi_c.atan2(cos_phi_c);
            wrap_deg_signed(v.c.zero_offset_deg + v.sign.factor() * phi_c.to_degrees())
        };
        let a_deg = wrap_deg_signed(v.a.zero_offset_deg + v.sign.factor() * phi_a.to_degrees());
        out.push(AcCandidate {
            a_deg,
            c_deg,
            at_pole,
        });
    }

    Ok(out)
}

//! Split from `finishing3d/params.rs` for the 400-line law; validation
//! and spacing laws only.
//!
//! Provenance: standard ball-end cusp geometry; no vendor material consulted.

use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::params::Finish3DParams;

/// The stepdown the scallop law prescribes: cusp height
/// `Δz²/(8 r cos²α) ≤ tol` for `α ≤ 45°` gives `Δz = 2√(r·tol)`.
/// This is the ladder's CEILING (M13.9): `ladder::levels` reads the
/// part's own slopes and tightens below it on ground shallower than
/// 45°, never above it. Non-finite or non-positive inputs collapse to
/// 0.0, which the caller's level law turns into a clean refusal.
#[must_use]
pub fn scallop_stepdown(r: f64, tol: f64) -> f64 {
    if !(r.is_finite() && tol.is_finite()) || r <= 0.0 || tol <= 0.0 {
        return 0.0;
    }
    2.0 * (r * tol).sqrt()
}

/// The raster stepover the same scallop law prescribes sideways: two
/// Z-following ball paths whose tangent centers are `s·sec α` apart in
/// the plane normal to the pass leave a cusp `≈ (s·sec α)²/(8r)` on a
/// facet tilted `α` from horizontal, so `s = √(8·r·tol)·cos α` holds
/// the cusp at `tol`. The caller multiplies by the model's smallest
/// shallow `m_z` - one law, the tilt's worst case. Non-positive or
/// non-finite inputs collapse to 0.0 like the stepdown's.
#[must_use]
pub fn scallop_stepover(r: f64, tol: f64) -> f64 {
    if !(r.is_finite() && tol.is_finite()) || r <= 0.0 || tol <= 0.0 {
        return 0.0;
    }
    (8.0 * r * tol).sqrt()
}

/// The scallop law's reach ceiling (M13.9, #37): the spacing at which
/// the cusp formula `h = r − √(r² − (s/2)²)` stops describing a cusp.
/// At `s = 2r` the two balls only touch and `h = r`; past it they never
/// meet and the strip between them is uncut ground, not a scallop. So
/// a stated `stepover` or `steep_stepover` at or past the ball
/// diameter is refused by name, never generated into a path whose
/// verdict would read a "cusp" a full radius tall. The tolerance has
/// the same ceiling through the auto law: `√(8·r·tol)` - the small-cusp
/// form of the formula above - reaches `2r` at `tol = r/2`, so a
/// tolerance at or past half the ball radius is refused too (the
/// exact formula never lets a cusp reach that tall short of `s = √3·r`;
/// such a "tolerance" is no finishing promise). Inputs below the
/// ceiling are untouched: no clamp, no rounding.
pub(super) fn scallop_ceiling(p: &Finish3DParams) -> Result<(), GenError> {
    let d = p.tool_diameter;
    for (name, s) in [("stepover", p.stepover), ("steep stepover", p.steep_stepover)] {
        if let Some(s) = s
            && s >= d
        {
            return Err(GenError::Invalid(format!(
                "3D finish {name} {s} mm reaches the scallop ceiling: passes a ball diameter ({d} mm) apart leave uncut ground, not a cusp"
            )));
        }
    }
    if p.tolerance >= d / 4.0 {
        return Err(GenError::Invalid(format!(
            "3D finish tolerance {} mm reaches the scallop ceiling: at half the ball radius ({} mm) the scallop stepover is a full ball diameter",
            p.tolerance,
            d / 4.0
        )));
    }
    Ok(())
}

/// The adaptive march's refinement floor, mm: `delta0/4096` by its
/// own right (the quarter-chord bisection has to stop SOMEWHERE),
/// raised - never lowered - by an explicit `min_step`. A NaN floor
/// loses to the auto floor by `f64::max`'s own law, so a nonsense
/// number can never freeze the march; an over-tall floor simply
/// turns refinement off, which is legal coarse ground.
#[must_use]
pub(super) fn refinement_floor(delta0: f64, min_step: Option<f64>) -> f64 {
    let auto = delta0 / 4096.0;
    match min_step {
        Some(s) => auto.max(s),
        None => auto,
    }
}

/// Unit directions of the convex scan. The count follows the
/// disk-chord law: the scan's worst miss is a disk sagitta at most
/// `r(1 − cos(π/N))`, held under `tol/2` (the other half of the
/// budget belongs to the stepdown scallop).
pub(super) fn dirs(r: f64, tol: f64) -> Vec<(f64, f64)> {
    let want = std::f64::consts::PI * (2.0 * r / tol).sqrt();
    let n = (want.ceil() as usize).clamp(128, 16384);
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (a.cos(), a.sin())
        })
        .collect()
}

/// The shared numeric gate every 3D finish reads before touching
/// geometry. Messages say "3D finish", not one strategy's name - the
/// check belongs to both.
pub(super) fn validate(p: &Finish3DParams, facets: &[Triangle]) -> Result<(), GenError> {
    if !(p.tool_diameter.is_finite()
        && p.tool_diameter > 0.0
        && p.tolerance.is_finite()
        && p.tolerance > 0.0
        && p.feed.is_finite()
        && p.feed > 0.0
        && p.plunge_feed.is_finite()
        && p.plunge_feed > 0.0
        && p.clearance_z.is_finite()
        && p.top_z.is_finite()
        && p.bottom_z.is_finite()
        && p.stepover.is_none_or(|s| s.is_finite() && s > 0.0)
        && p.steep_stepover.is_none_or(|s| s.is_finite() && s > 0.0)
        && p.pencil_reference.is_none_or(|d| d.is_finite() && d > 0.0)
        && p.retract_z.is_none_or(|rz| rz.is_finite())
        && p.min_step.is_none_or(|s| s.is_finite() && s > 0.0)
        // The ramp angle is refused OUTSIDE (0, 90) even while
        // `ramp` is off - an unreachable promise is not an inert
        // number, it is a lie waiting for the checkbox.
        && (0.0..90.0).contains(&p.ramp_angle))
    {
        return Err(GenError::Invalid(
            "3D finish needs finite positive tool, tolerance, feeds, heights and surface-finish numbers".into(),
        ));
    }
    scallop_ceiling(p)?;
    if facets.is_empty() {
        return Err(GenError::Empty("model has no facets".into()));
    }
    for f in facets {
        if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
            return Err(GenError::Invalid("model has a non-finite facet".into()));
        }
    }
    Ok(())
}

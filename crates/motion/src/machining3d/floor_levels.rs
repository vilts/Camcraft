//! Provenance: our own flat-floor discovery over the public triangle mesh and depth ladder (#2088).

use super::params::Pocket3DParams;
use crate::geometry::Triangle;
use crate::machining2d::{GenError, MAX_PASSES, cut_levels_under, ladder_stepdown};

/// Keep the regular ladder and include flat floors above its first lower
/// plane (or across `[bottom_z, top_z)` when `stock_to_leave > 0`, so
/// deeper floors also receive their allowance level). The existing
/// per-level material checks still decide where to cut.
pub(super) fn cut_levels(
    facets: &[Triangle],
    p: &Pocket3DParams,
    stepdown: f64,
) -> Result<Vec<f64>, GenError> {
    let mut zs = cut_levels_under(p.top_z, p.links.approach.stock_top, p.bottom_z, stepdown)?;
    // An existing Top Z stock pass can already add one to the regular budget.
    let limit = zs.len().max(MAX_PASSES as usize);
    let Some(first) = zs.iter().copied().find(|z| *z < p.top_z) else {
        return Ok(zs);
    };
    for t in facets {
        // Exact horizontal faces only: a sloped triangle is not a flat floor.
        if t.a.z == t.b.z && t.a.z == t.c.z && t.normal().z > 0.0 {
            let z = t.a.z + p.stock_to_leave;
            let in_band = if p.stock_to_leave > 0.0 { z >= p.bottom_z } else { z > first };
            // These finite bounds also exclude NaN and overflowing sums.
            if in_band && z < p.top_z {
                zs.push(z);
            }
        }
    }
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    if zs.len() > limit {
        return Err(GenError::Invalid(format!(
            "shallow floor levels need more than {limit} passes - simplify the model"
        )));
    }
    Ok(zs)
}

/// The stepdown of the coarse ladder: the approach's ladder mode
/// (#2012) applied to the operation's stepdown over its own span.
pub(super) fn coarse_step(p: &Pocket3DParams) -> f64 {
    ladder_stepdown(&p.links.approach, p.top_z, p.bottom_z, p.stepdown)
}

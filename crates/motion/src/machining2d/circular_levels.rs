//! Circular depth ladders. Provenance: the clean-room paraboloid in
//! `circular.rs` and the common maximum-axial-depth cut-level law.

use super::{CircularParams, GenError, PassStart};
use super::super::equal_steps::{cut_levels_for, ladder_stepdown};
use super::super::levels::{cut_levels, stock_over};

pub(super) fn stock_top(p: &CircularParams) -> f64 {
    let a = &p.links.approach;
    a.entry_top(p.top_z).max(a.stock_top.unwrap_or(p.top_z))
}

/// Forced orders make no assumption about the material a neighboring
/// ring removed. Take full ladders to the band's floor. Material over
/// Top Z gets its own ladder, also bounded by the maximum stepdown.
pub(super) fn ring_levels(p: &CircularParams, k: usize, rho: f64) -> Result<Vec<f64>, GenError> {
    let a = &p.links.approach;
    let r = p.tool_diameter * 0.5;
    let floor = if p.vault { p.bowl_z((rho + r).min(p.radius)) } else { p.bottom_z };
    let over = stock_over(p.top_z, a.stock_top, 1e-9 * p.top_z.abs().max(1.0));
    if p.pass_start != PassStart::Auto {
        let top = stock_top(p);
        let mut zs = if top > p.top_z {
            cut_levels(top, p.top_z, ladder_stepdown(a, top, p.top_z, p.stepdown))?
        } else { Vec::new() };
        if floor < p.top_z || (!p.vault && zs.is_empty()) {
            zs.extend(cut_levels(p.top_z, floor, ladder_stepdown(a, p.top_z, floor, p.stepdown))?);
        }
        return Ok(zs);
    }
    // The original Auto ladder: the outer neighbor's floor is itself
    // a level of each inner vault band. Drop air levels at the rim.
    let zs = if !p.vault || k == 0 {
        cut_levels_for(p.top_z, a, floor, p.stepdown)?
    } else {
        let prev = p.bowl_z((rho + p.stepover + r).min(p.radius));
        levels_from(prev, floor, ladder_stepdown(a, prev, floor, p.stepdown))?
    };
    Ok(if p.vault && !over { under_top(zs, p.top_z) } else { zs })
}

/// An inner vault ring's ladder: the plane `top` (the previous ring's
/// floor, where this ring's new strip opens) and then the shared
/// [`cut_levels`] below it - the ladder this ring has always cut. A span
/// of zero or less is the one plane at `bottom`, as `cut_levels` says.
pub(super) fn levels_from(top: f64, bottom: f64, stepdown: f64) -> Result<Vec<f64>, GenError> {
    let below = cut_levels(top, bottom, stepdown)?;
    if top - bottom <= 0.0 {
        return Ok(below);
    }
    Ok(std::iter::once(top).chain(below).collect())
}

/// The vault planes strictly under Top Z (a plane at Top Z removes
/// nothing, #2004); `1e-9` of the span absorbs the bowl's float noise.
pub(super) fn under_top(zs: Vec<f64>, top: f64) -> Vec<f64> {
    let tol = 1e-9 * top.abs().max(1.0);
    zs.into_iter().filter(|&z| z < top - tol).collect()
}

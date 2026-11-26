//! Law (d3), feed plunge depth (#3679): a feed descent judged against the
//! floor that the cuts before it left under the FULL tool disc - a dive
//! whose rim stands on stock that no level above cut convicts when its
//! cutter takes more of that stock in the one move than one stepdown
//! of the operation. Provenance: the shop practice that a plunge into
//! uncut stock takes at most one stepdown of material (machine-tool
//! handbooks), read on our own rest model; no vendor source was
//! consulted.
//!
//! The take is the move's own bite, never the whole column below the
//! floor: a tip that a rapid already stood inside uncut stock can pass
//! only the material between its start and its end - the stock above
//! the start is the rapid law's crime, named there and holding the
//! post there - so the bite's floor is the LESSER of the move's start
//! and the rest surface. A dive from air keeps the old reading whole
//! (`from.z` stands above the floor, so the min is the floor), and a
//! start no number can read falls back to the floor the same way
//! (`f64::min` drops the NaN, a floor the rest cannot read included).
//!
//! Advisory by the #3525 pattern: nothing here gouges and nothing
//! collides (the plunge IS the cut), the risk is the tool load, so the
//! finding names the dive and the readings that repair it, and the
//! collision laws alone decide if the post is held. The gate is
//! [`VerifyContext::plunge_stepdown`]: `None` - every kind that cuts no
//! levels, and every context that states no limit - keeps the law
//! silent and every stored verdict byte-identical, the same discipline
//! as `finish_tol` and `model`.
//!
//! The disc is [`super::rapids::footprint`]'s rings, judged as law (d)
//! judges a dive: each sample at the height the tool's own profile
//! stands at its radius ([`Cutter::lift_at`]), so a ball or a point
//! whose lifted rim clears the floor is air there. One finding per
//! move, at the worst convicting sample. A reading that cannot be
//! compared - a non-finite limit, a NaN bite - convicts: the
//! `partial_cmp` form of `!(depth <= limit + z_tol)` reads NaN as no
//! acquittal, never as a pass, and a floor the rest cannot read falls
//! back to the move's own start, an upper bound the bite cannot pass.
//! `Mode::Revolve`, `Mode::Radial` and `Mode::Rotary` stand the law
//! down with law (d)'s own words: the XY column is not those paths'
//! geometry.

use super::laws::xy;
use super::probe::Cutter;
use super::rapids::footprint;
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};

/// (d3) Feed plunge depth (#3679): every non-rapid axial descent whose
/// disc meets stock that the cuts before it did not clear below one
/// stepdown of the operation, named once at its worst sample. See the
/// module doc for the reading and the advisory law.
pub(super) fn plunge_depth(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    cutter: &Cutter,
    warns: &mut Vec<Warning>,
) {
    let Some(limit) = cx.plunge_stepdown else {
        return; // the unstated-limit law: no stepdown, no judgement
    };
    if matches!(cx.mode, Mode::Revolve | Mode::Radial | Mode::Rotary) {
        return; // the XY column is not those paths' geometry (law (d)'s words)
    }
    // #904: the footprint gives a coarse model's excess back.
    let d = cx.tool.radius - cx.tightened(cx.gouge_tol);
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue; // law (d) owns the rapids
        }
        let (a, b) = (xy(&m.from), xy(&m.to));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx * dx + dy * dy > cx.z_tol * cx.z_tol {
            continue; // a lateral or ramped feed: its engagement is the level law's
        }
        if m.to.z.partial_cmp(&(m.from.z - cx.z_tol)).is_none_or(|o| !o.is_lt()) {
            continue; // a lift, a flat pass, or a start no number can read
        }
        // One finding per move, at the worst CONVICTING sample: the
        // bite the move itself takes below the floor the cuts before
        // it left under the disc, at the tool's own height there - the
        // lesser of the move's start and that floor, so a tip a rapid
        // already stood inside stock answers for its own descent only
        // (the stock above the start is law (d)'s to name), and a
        // floor the rest cannot read falls back to the start, an
        // upper bound the bite cannot pass. The test itself is
        // `!(depth <= limit + z_tol)` in its fail-closed `partial_cmp`
        // form: a non-finite stepdown or a NaN bite is no acquittal -
        // the dive is named, never passed.
        let mut worst: Option<f64> = None;
        for (q, rho) in footprint(b, d) {
            if !rest.in_stock(q) {
                continue;
            }
            let depth = m.from.z.min(rest.surface_before(q, i)) - (m.to.z + cutter.lift_at(rho));
            if depth.partial_cmp(&(limit + cx.z_tol)).is_none_or(|o| o.is_gt()) {
                worst = Some(match worst {
                    // The deepest convicting sample is the named one;
                    // a NaN reading stays named until a finite depth
                    // that also convicts replaces it.
                    Some(w) if !w.is_nan() && depth <= w => w,
                    _ => depth,
                });
            }
        }
        if let Some(mm) = worst {
            warns.push(Warning::PlungeTooDeep { at: i, mm, limit_mm: limit });
        }
    }
}

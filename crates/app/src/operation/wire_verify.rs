//! The Wire Contour's verification inputs (#534, part of #290 D7): the
//! op's cached path, its condition's kerf offset, its stated side and
//! start hole, and the part's and stock's sections through the whole
//! height the vertical wire passes - handed to `verify::wire::judge`.
//! Provenance: our own glue over the sim crate's wire laws (whose
//! module head cites the public sources) and the slicer; read live at
//! the post, as the plasma clause reads its paths.

use flecs_ecs::prelude::*;

use crate::geo2d::Point2;
use crate::geometry::{slice, Triangle};
use crate::model;
use crate::verify::wire::{judge, CutSide, WireFinding, WireJob, WireSection, WireVerdict};

use super::types::{OpParams, WireSide};
use super::SrcRef;

/// The most heights the wire laws slice: both ends and the planes
/// between them, about every 2 mm.
const MAX_PLANES: usize = 9;
const PLANE_STEP: f64 = 2.0;

fn z_span(f: &[Triangle]) -> Option<(f64, f64)> {
    f.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).filter(|z| z.is_finite()).fold(None, |acc, z| match acc {
        None => Some((z, z)),
        Some((lo, hi)) => Some((lo.min(z), hi.max(z))),
    })
}

/// The heights to judge across `lo..hi`: just inside both ends (a
/// horizontal face is never sliced edge-on) and evenly between.
fn planes(lo: f64, hi: f64) -> Vec<f64> {
    let span = hi - lo;
    if !(span.is_finite() && span > 0.0) {
        return vec![lo];
    }
    let eps = (span * 0.01).min(1e-3);
    let n = ((span / PLANE_STEP).ceil() as usize + 1).clamp(2, MAX_PLANES);
    (0..n).map(|i| lo + eps + (span - 2.0 * eps) * i as f64 / (n - 1) as f64).collect()
}

/// Judge one Wire Contour against the world as it stands now: `None`
/// for any other kind; `Err` names why the laws cannot run (no path, no
/// condition, no part to judge against, a section that cannot be cut).
///
/// # Errors
/// Each refusal by name; the gate blocks the post on it.
pub fn wire_verdict(world: &World, e: &Entity) -> Option<Result<WireVerdict, String>> {
    let op = model::operation_of(world, e)?;
    let OpParams::WireContour(w) = &op.params else { return None };
    Some((|| {
        let setup = op.setup.as_deref();
        // A closed cut with no start hole never generates; said by the
        // law's own name, not as a missing path. An open run (#1109,
        // left / right) states none: it is threaded outside the stock.
        if w.start.is_none() && !w.side.is_open_side() {
            return Ok(WireVerdict { findings: vec![WireFinding::NoStart], ..WireVerdict::default() });
        }
        let cache = model::toolpath_of(world, e).ok_or("the wire laws have no toolpath to judge - generate it first")?;
        let cond = super::wire_op::condition_of(world, setup, w)?;
        if matches!(op.src, SrcRef::Stock) {
            return Err("the wire laws judge the kerf against the part, and this cut is taken from the stock - cut it from the part model".into());
        }
        let material = super::material::material_of(world, setup, &op.src).map_err(|e| e.to_string())?;
        let stock = model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
            .and_then(|s| model::world_facets(world, &s))
            .filter(|f| !f.is_empty());
        let hs = super::resolve_params(world, setup, Some(&op.src), &op.params)?;
        // The vertical wire cuts everything between its guides: the op's
        // planes, the part and the stock, whichever reaches farther.
        let mut lo = hs.bottom.unwrap_or(f64::INFINITY);
        let mut hi = hs.top.unwrap_or(f64::NEG_INFINITY);
        for (a, b) in [z_span(&material.facets), stock.as_deref().and_then(z_span)].into_iter().flatten() {
            lo = lo.min(a);
            hi = hi.max(b);
        }
        if !(lo.is_finite() && hi.is_finite()) {
            return Err("the wire laws find no height to judge - neither the planes nor the part resolve".into());
        }
        let cut = |f: &[Triangle], z: f64, what: &str| {
            slice::slice(f, z).map_err(|e| format!("the {what}'s section at Z{z:.3} cannot be cut: {e}"))
        };
        let mut sections = Vec::new();
        for z in planes(lo, hi) {
            let part = cut(&material.facets, z, "part")?;
            let stock = stock.as_deref().map(|f| cut(f, z, "stock")).transpose()?;
            sections.push(WireSection { z, part, stock });
        }
        let side = match w.side {
            WireSide::Die => CutSide::Die,
            WireSide::Punch => CutSide::Punch,
            WireSide::Left | WireSide::Right => CutSide::Open,
        };
        let start = w.start.map(|[x, y]| Point2::new(x, y));
        let cutoff_from = cutoff_from(w, &cache.path)?;
        Ok(judge(&WireJob {
            path: &cache.path,
            kerf_offset: cond.wire_diameter * 0.5 + cond.gap,
            // Each skim judged at its own condition's kerf (#538).
            skim_offsets: cond.skims.iter().map(|s| s.wire_diameter * 0.5 + s.gap).collect(),
            side,
            start,
            sections,
            cutoff_from,
            capture: w.capture,
            // #537: a 4-axis cut's poses are its rulings.
            taper: w.taper_deg != 0.0 || w.four_axis.is_some(),

            burnout: w.burnout.is_some(),
        }))
    })())
}

/// Where a tabbed cut's tab cut-off begins in its path (#539): `None`
/// for an untabbed cut.
///
/// # Errors
/// A tabbed op whose path does not hold the cut-off its tabs state
/// (generated before the tabs were set, or not a tabbed path).
pub fn cutoff_from(w: &super::types::WireContourOp, path: &crate::cl::Toolpath) -> Result<Option<usize>, String> {
    let Some(t) = &w.tabs else { return Ok(None) };
    let (Some(n), Some([x, y])) = (t.count, w.start) else {
        return Err("the tabs are not fully stated - the tab count and the start hole place the cut-off".into());
    };
    crate::wire_edm::cutoff_split(path, Point2::new(x, y), n as usize).map(Some).ok_or_else(|| {
        format!("the path does not hold the tab cut-off its {n} tab(s) state - regenerate it")
    })
}

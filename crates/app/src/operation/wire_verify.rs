//! The Wire Contour's verification inputs (#534, part of #290 D7): the
//! op's cached path, its condition's kerf offset, its stated side and
//! start hole, and the part's and stock's sections through the whole
//! height the vertical wire passes - handed to `verify::wire::judge`.
//! Provenance: our own glue over the sim crate's wire laws (whose
//! module head cites the public sources) and the slicer; read live at
//! the post, as the plasma clause reads its paths.

use flecs_ecs::prelude::*;

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

/// Check the full height swept by the wire against each stated guide.
fn check_guide_span(world: &World, setup: Option<&str>, lo: f64, hi: f64) -> Result<(), String> {
    let (_, machine) = crate::machine::of_setup(world, setup);
    let Some(edm) = machine.as_ref().and_then(|m| m.edm.as_ref()) else { return Ok(()) };
    let (_, origin) = crate::posting::wcs_context(world, setup);
    // Place the guides in the same frame as the work, as guide_planes does for
    // the post. Subtracting a decimal origin from the work can round a fit away.
    let world_z = |guide_z: f64| {
        let z = guide_z + origin[2];
        z.is_finite().then_some(z).ok_or("the wire guide height in the work frame is not finite")
    };
    if let Some(lower) = edm.lower_guide_z
        && lo < world_z(lower)?
    {
        let bottom = lo - origin[2];
        return Err(format!("work bottom Z{bottom:.3} is below the lower guide Z{lower:.3}, outside the guides' reach"));
    }
    if let Some(upper) = edm.upper_guide_z
        && hi > world_z(upper)?
    {
        let top = hi - origin[2];
        return Err(format!("work top Z{top:.3} is above the upper guide Z{upper:.3}, outside the guides' reach"));
    }
    Ok(())
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
        if w.picks.first(crate::path_picks::PickRole::Start).is_none() && !w.side.is_open_side() {
            return Ok(WireVerdict { findings: vec![WireFinding::NoStart], ..WireVerdict::default() });
        }
        let cache = model::toolpath_of(world, e).ok_or("the wire laws have no toolpath to judge - generate it first")?;
        let cond = super::wire_op::condition_of(world, setup, &op.tool, w)?;
        if matches!(op.src, SrcRef::Stock) {
            return Err("the wire laws judge the kerf against the part, and this cut is taken from the stock - cut it from the part model".into());
        }
        let hs = super::resolve_params(world, setup, Some(&op.src), &op.params)?;
        let selected = super::wire_sketch::selected(&op.src, &op.params);
        // #3215: a vertical cut of a model whose arcs were read back from
        // the facets is judged under the chord the fit was bounded by.
        let facet_arcs = !selected && w.four_axis.is_none() && w.taper_deg == 0.0 && cache.path.moves.iter().any(|m| m.arc.is_some());
        let sections = if selected {
            let name = model::name_of(world, e).ok_or("the selected wire operation has no name")?;
            let sketch = super::wire_sketch::resolve(world, &op, &name, &hs)?;
            let part = crate::geo2d::Region::new(vec![sketch.verify_ring(w)?]).map_err(|e| e.to_string())?;
            let stock = sketch.stock_region()?;
            sketch.check_path(&cache.path, cond.wire_diameter * 0.5 + cond.gap)?;
            let (lo, hi) = (sketch.stock.min.z, sketch.stock.max.z);
            check_guide_span(world, setup, lo, hi)?;
            // Both sections are constant through the complete analytic prism.
            [lo, hi].into_iter().map(|z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect()
        } else {
            model_sections(world, &op, &hs)?
        };
        let side = match w.side {
            WireSide::Die => CutSide::Die,
            WireSide::Punch => CutSide::Punch,
            WireSide::Left | WireSide::Right => CutSide::Open,
        };
        let start = w.picks.first(crate::path_picks::PickRole::Start).map(|p| p.xy);
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
            // #1170: the 4-axis cut's stated verify tolerance; #3210: a
            // constant taper's is the model's chord tolerance.
            verify_tol: match &w.four_axis {
                Some(f) => f.verify_tolerance,
                None if facet_arcs => arc_chord(world, setup, &op.src)?,
                None => taper_chord(world, &op.src, w),
            },
            taper_chord: w.four_axis.is_none() && (w.taper_deg != 0.0 || facet_arcs),
        }))
    })())
}

/// The verify tolerance of a constant-taper cut (#3210): the coarsest
/// chord its source models were meshed at - the importer's promise, or
/// the coarser chord a reduced-fidelity model records. The generator
/// lays the wire from one height's section, and the facets at the other
/// heights stand off the true wall by up to that chord. 0 for a cut
/// that states no taper.
fn taper_chord(world: &World, src: &SrcRef, w: &super::types::WireContourOp) -> f64 {
    if w.taper_deg == 0.0 {
        return 0.0;
    }
    let names: Vec<&str> = match src {
        SrcRef::Entity(name) => vec![name.as_str()],
        SrcRef::Selection(items) => items.iter().map(|g| g.entity.as_str()).collect(),
        SrcRef::Stock => Vec::new(),
    };
    let cap = crate::geometry::import::step::CHORD_CAP;
    names
        .into_iter()
        .filter_map(|n| model::find_by_name(world, n))
        .filter_map(|e| crate::model_fidelity::chord_of(world, &e))
        .fold(cap, f64::max)
}

/// The tolerance a model's read-back arcs are judged under (#3215): the
/// chord the generator bounded each arc's sagitta by (the importer's
/// promise plus the model's excess, read as generation reads it,
/// `judged_material`) plus the fit's vertex tolerance.
fn arc_chord(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<f64, String> {
    let m = super::material::judged_material(world, setup, src).map_err(|e| e.to_string())?;
    Ok(crate::geometry::import::step::CHORD_CAP + m.excess + crate::wire_edm::MODEL_ARC_FIT_TOL)
}

fn model_sections(world: &World, op: &crate::schema::Operation, hs: &super::ResolvedHeights) -> Result<Vec<WireSection>, String> {
    let setup = op.setup.as_deref();
    let material = super::material::material_of(world, setup, &op.src).map_err(|e| e.to_string())?;
    let stock = model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
        .and_then(|s| model::world_facets(world, &s))
        .filter(|f| !f.is_empty());
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
    check_guide_span(world, setup, lo, hi)?;
    let cut = |f: &[Triangle], z: f64, what: &str| {
        slice::slice(f, z).map_err(|e| format!("the {what}'s section at Z{z:.3} cannot be cut: {e}"))
    };
    let mut sections = Vec::new();
    for z in planes(lo, hi) {
        let part = cut(&material.facets, z, "part")?;
        let stock = stock.as_deref().map(|f| cut(f, z, "stock")).transpose()?;
        sections.push(WireSection { z, part, stock });
    }
    Ok(sections)
}

/// Where a tabbed cut's tab cut-off begins in its path (#539): `None`
/// for an untabbed cut.
///
/// # Errors
/// A tabbed op whose path does not hold the cut-off its tabs state
/// (generated before the tabs were set, or not a tabbed path).
pub fn cutoff_from(w: &super::types::WireContourOp, path: &crate::cl::Toolpath) -> Result<Option<usize>, String> {
    let Some(t) = &w.tabs else { return Ok(None) };
    let (Some(n), Some(pick)) = (t.count, w.picks.first(crate::path_picks::PickRole::Start)) else {
        return Err("the tabs are not fully stated - the tab count and the start hole place the cut-off".into());
    };
    crate::wire_edm::cutoff_split(path, pick.xy, n as usize).map(Some).ok_or_else(|| {
        format!("the path does not hold the tab cut-off its {n} tab(s) state - regenerate it")
    })
}

/// The leftover a stated sharp inside corner keeps (#3207), as the
/// generator said it on the op's cached path: a note on the verify
/// line, never a refusal. `None` when the op states no sharp corners
/// or its path keeps no rest.
#[must_use]
pub fn sharp_corner_said(world: &World, e: &Entity) -> Option<String> {
    let op = model::operation_of(world, e)?;
    let OpParams::WireContour(w) = &op.params else { return None };
    if !w.sharp_corners {
        return None;
    }
    let cache = model::toolpath_of(world, e)?;
    let note = cache.path.notes.iter().find(|n| n.starts_with(crate::wire_edm::SHARP_NOTE))?;
    Some(note.trim_start_matches("wire EDM: ").to_owned())
}

//! The world half of the area controls (M11.5): every read the law in
//! `boundary` needs - stock outline, part material standing at the cut
//! planes, the picked include / exclude / open boundaries - resolved on
//! the caller's thread into an owned [`AreaCtx`] (the M5.3 B job law:
//! nothing in the run phase names a `World`). `None` for the identity
//! controls: an untouched operation reads nothing new, so its path and
//! stamp are exactly what they were.
//!
//! Provenance: our own design notes; the pick resolution is the
//! selection walk every region already runs. No vendor material consulted.

use super::boundary::{AreaCtx, Family, Native, OpenGeom};
use super::boundary_picks::picks_geom;
use super::*;

/// Whether `material` is a finished PART rather than the setup's stock -
/// the depth-aware verify's own test, shared with the job collector.
pub(super) fn part_solid_of(world: &World, setup: Option<&str>, src: &SrcRef, material: &Material) -> bool {
    let stock_name = crate::setup::stock_entity_name(world, setup);
    !material.facets.is_empty()
        && match src {
            SrcRef::Stock => false,
            SrcRef::Entity(n) => *n != stock_name,
            SrcRef::Selection(items) => !items.is_empty() && items.iter().all(|g| g.entity != stock_name),
        }
}

/// [`area_ctx`] for a caller holding only the operation (the preview):
/// tool and material read here, the same reads generation makes.
pub(super) fn area_ctx_of(world: &World, op: &Operation) -> Option<Result<AreaCtx, GenError>> {
    if op.area.is_default() {
        return None;
    }
    let tool = model::find_by_name(world, &op.tool).and_then(|e| model::tool_of(world, &e));
    let Some(tool) = tool else {
        return Some(Err(GenError::Invalid(format!("no tool named '{}'", op.tool))));
    };
    let setup = op.setup.as_deref();
    let material = match material_of(world, setup, &op.src) {
        Ok(m) => m,
        Err(e) => return Some(Err(e)),
    };
    let part_solid = part_solid_of(world, setup, &op.src, &material);
    area_ctx(world, op, tool.diameter * 0.5, &material, part_solid)
}

/// Resolve the operation's area controls against the world, or `None`
/// for the identity. Refusals are named here, at the door of the region.
pub(super) fn area_ctx(
    world: &World,
    op: &Operation,
    tool_r: f64,
    material: &Material,
    part_solid: bool,
) -> Option<Result<AreaCtx, GenError>> {
    if op.area.is_default() {
        return None;
    }
    Some(build(world, op, tool_r, material, part_solid))
}

/// The words a 3+2 index with a non-finite letter refuses by - the
/// parameter door's own (`collect_job`), repeated so the preview and
/// the page say the same thing.
pub(super) const TILT_NO_DIRECTION: &str = "the 3+2 index commands no direction - its angles are not finite";

fn build(world: &World, op: &Operation, tool_r: f64, material: &Material, part_solid: bool) -> Result<AreaCtx, GenError> {
    let kind = op.params.kind();
    if let Some(why) = area_unsupported(kind) {
        return Err(GenError::Invalid(format!("{why}; its Boundary settings must stay at their defaults")));
    }
    // The 3+2 index (M11.5 owed): the law runs in the frame the planar
    // generator runs in - the stock outline, the part silhouette, the
    // material standing at the cut plane and every pick read through
    // the frame's door (`TiltedFrame::local_facets` / `to_local`), the
    // way `region_turn_of` and `run_job` already read the cutting
    // region. The controls then mean in the tilted plane exactly what
    // they mean in XY, and the result rides the same wrap back out.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => Some(t.frame().ok_or_else(|| GenError::Invalid(TILT_NO_DIRECTION.into()))?),
    };
    if frame.is_some() && matches!(op.src, SrcRef::Selection(_)) {
        // The wrap's own refusal (M5.1 B), named here so the preview
        // never resolves a world-XY pick against a tilted plane.
        return Err(GenError::Invalid(
            "a tilted plane cannot yet index a picked selection - pick a whole model instead".into(),
        ));
    }
    if !(tool_r.is_finite() && tool_r > 0.0) {
        return Err(GenError::Invalid("the tool has no usable diameter".into()));
    }
    let setup = op.setup.as_deref();
    let (family, native) = match &op.params {
        OpParams::Face { .. } => (Family::Face, Native::ToolInside),
        OpParams::Contour { .. } => (Family::Contour, Native::ToolOutside),
        // The finish's rings reach a radius past the silhouette (the
        // ball rides a wall with its edge on it): tool outside, natively.
        OpParams::Finish3D { .. } => (Family::Finish, Native::ToolOutside),
        // The outline rides (M13.10): the loops are the path, centre on.
        OpParams::Trace { .. } | OpParams::Engrave { .. } => (Family::Ride, Native::ToolOn),
        _ => (Family::Clearing, Native::ToolInside),
    };
    let oz = frame.map_or(0.0, |f| f.origin.z);
    let local: Option<Vec<Triangle>> = frame.as_ref().map(|f| f.local_facets(&material.facets));
    let facets: &[Triangle] = local.as_deref().unwrap_or(&material.facets);
    let stock_name = crate::setup::stock_entity_name(world, setup);
    let stock = model::find_by_name(world, &stock_name)
        .and_then(|e| model::world_facets(world, &e))
        .map(|f| match &frame {
            None => f,
            Some(fr) => fr.local_facets(&f),
        })
        .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
        .filter(|r| !r.is_empty());
    // Part material standing above the cut plane(s): the pocket island
    // law's ladder for the pocket family on picked part features, the
    // finished plane for Face and Contour on a part. ALL of it - the
    // plane's section plus the shadow of what hangs over the plane
    // (`slice::material_overhead`): a ledge or lip the plane does not cut
    // is still steel an extension must not flood and a face must not
    // sweep.
    let mut islands = false;
    let overhead_union = |facets: &[Triangle], planes: &[f64]| -> Result<Region, GenError> {
        let mut b = Region::empty();
        for &z in planes {
            b = b.union(&slice::material_overhead(facets, z).map_err(from_slice)?)?;
        }
        Ok(b)
    };
    let blocked = match &op.params {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, .. } => {
            let planes_for = |parts: &[Triangle], init_top: f64| -> Option<Vec<f64>> {
                let t = fold_height(world, setup, Some(&op.src), frame.as_ref(), *top_z)?;
                let b = fold_height(world, setup, Some(&op.src), frame.as_ref(), *bottom_z)?;
                let mt = super::area::stock_top(world, setup, parts, init_top, frame.as_ref());
                Some(super::area::ladder_planes(t - oz, mt, b - oz, *stepdown))
            };
            match &op.src {
                SrcRef::Selection(items) => match pocket_ladder(world, setup, items, &op.params)? {
                    Some((part, planes)) => {
                        islands = true;
                        Some(overhead_union(&part, &planes)?)
                    }
                    None => None,
                },
                // Whole-model / stock-source pockets clear stock AROUND
                // standing part islands (`islands` stays false so manual
                // offset into the part still exercises DepthLaw::Keep,
                // while extension only grows past the stock exterior).
                SrcRef::Entity(_) if part_solid && stock.is_some() => match planes_for(&material.facets, material.top) {
                    Some(planes) => Some(overhead_union(facets, &planes)?),
                    None => None,
                },
                SrcRef::Stock if stock.is_some() => {
                    let parts = crate::setup_align::parts_facets_in(world, setup);
                    if parts.is_empty() {
                        None
                    } else {
                        match planes_for(&parts, f64::NEG_INFINITY) {
                            Some(planes) => {
                                let local_parts = frame.as_ref().map(|fr| fr.local_facets(&parts));
                                Some(overhead_union(local_parts.as_deref().unwrap_or(&parts), &planes)?)
                            }
                            None => None,
                        }
                    }
                }
                _ => None,
            }
        }
        OpParams::Face { .. } | OpParams::Contour { .. } if part_solid => {
            // #2389: under a tilt the plane is read along the tool axis.
            let hs = resolve_params_folded(world, setup, Some(&op.src), &op.params, frame.as_ref())
                .map_err(GenError::Invalid)?;
            let z = hs
                .bottom
                .ok_or_else(|| GenError::Invalid("the cut plane does not resolve".into()))?;
            // The plane in the frame's own height (`z - origin.z`, the
            // L1 law `run_job` shifts every height by).
            Some(slice::material_overhead(facets, z - oz).map_err(from_slice)?)
        }
        _ => None,
    };
    let mut extra = Vec::with_capacity(op.area.extra.len());
    for b in &op.area.extra {
        let region = match &b.shape {
            BoundaryShape::Picks(picks) => match picks_geom(world, setup, picks, &op.params, frame.as_ref())? {
                OpenGeom::Ring(r) => r,
                // The backstop: the page and the Apply door
                // (`area_pick_issue`) catch this first.
                OpenGeom::Chain(_) | OpenGeom::Chains(_) => {
                    return Err(GenError::Invalid(
                        "an include / exclude boundary must close - its picks spell an open chain; use it as the open boundary instead".into(),
                    ));
                }
            },
            BoundaryShape::Stock => stock.clone().ok_or_else(|| {
                GenError::Invalid("a stock-outline boundary needs stock - this setup has none".into())
            })?,
            BoundaryShape::Silhouette => {
                if !part_solid {
                    return Err(GenError::Invalid(
                        "a part-silhouette boundary needs a part source - pick the part or choose its model as the source".into(),
                    ));
                }
                silhouette(facets).ok_or_else(|| GenError::Empty("the part casts no silhouette".into()))?
            }
        };
        extra.push((b.mode, region));
    }
    let open = match &op.area.open {
        None => None,
        Some(o) => Some((picks_geom(world, setup, &o.picks, &op.params, frame.as_ref())?, o.side)),
    };
    Ok(AreaCtx {
        controls: op.area.clone(),
        family,
        native,
        tool_r,
        stock,
        blocked,
        islands,
        extra,
        open,
        lane: if frame.is_none() { super::open_band::lane(world, op)? } else { None },
        unit: crate::units::of(world),
    })
}

/// The digest an operation's cached path answers to: [`current_stamp`],
/// plus - only when the area controls are not the identity - the
/// controls and every world input they read (picked entities' placement
/// and geometry, the stock's box). An untouched operation stamps the
/// exact bytes it always did.
#[must_use]
pub fn op_stamp(world: &World, op: &Operation, op_name: &str) -> Option<GenStamp> {
    let base = current_stamp(world, &op.tool, &op.src, &op.params, op_name, op.setup.as_deref())?;
    if op.area.is_default() {
        return Some(base);
    }
    let a = &op.area;
    let mut h = std::hash::DefaultHasher::new();
    let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
    h.write_u64(base.0);
    h.write(b"camcraft-area-v1");
    h.write_u8(match a.containment {
        None => 0,
        Some(Containment::Inside) => 1,
        Some(Containment::On) => 2,
        Some(Containment::Outside) => 3,
    });
    w(&mut h, a.offset);
    h.write_u8(u8::from(a.corner == CornerStyle::Sharp));
    w(&mut h, a.extension);
    let picks = |h: &mut std::hash::DefaultHasher, items: &[GeoSel]| -> Option<()> {
        h.write_usize(items.len());
        for g in items {
            let (tag, id) = match g.feature {
                FeatureSel::Face(id) => (0u8, id),
                FeatureSel::Chain(id) => (1, id),
                FeatureSel::Patch(id) => (2, id),
                FeatureSel::Vertex(id) => (3, id),
            };
            h.write_u8(tag);
            h.write_u64(id);
            h.write_usize(g.entity.len());
            h.write(g.entity.as_bytes());
            let e = model::find_by_name(world, &g.entity)?;
            let loc = model::transform_of(world, &e).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]] {
                h.write(&v.to_bits().to_le_bytes());
            }
            h.write_u64(model::geometry_signature(world, &e)?);
        }
        Some(())
    };
    h.write_usize(a.extra.len());
    for b in &a.extra {
        h.write_u8(u8::from(b.mode == BoundaryMode::Exclude));
        match &b.shape {
            BoundaryShape::Picks(p) => {
                h.write_u8(0);
                picks(&mut h, p)?;
            }
            BoundaryShape::Stock => h.write_u8(1),
            BoundaryShape::Silhouette => h.write_u8(2),
        }
    }
    match &a.open {
        None => h.write_u8(0),
        Some(o) => {
            h.write_u8(if o.side == ChainSide::Inside { 1 } else { 2 });
            picks(&mut h, &o.picks)?;
        }
    }
    // The stock outline closes open boundaries and bounds Face's run-out.
    if let Some(b) = model::stock_world_aabb_for(world, op.setup.as_deref()) {
        for v in [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z] {
            w(&mut h, v);
        }
    }
    Some(GenStamp(h.finish()))
}

/// The area controls' half of `stale_reason`: a boundary pick whose
/// entity is gone. `None` when every referenced entity still stands.
pub(super) fn area_stale_reason(world: &World, op: &Operation) -> Option<String> {
    op.area
        .picks()
        .find(|g| model::find_by_name(world, &g.entity).is_none())
        .map(|g| format!("a Boundary pick names '{}', which was deleted", g.entity))
}

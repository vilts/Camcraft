//! The plain Slot read off a geometry selection (#1504): two opposite
//! straight edges, or one face (or closed edge ring), are the room the
//! slot may cut in. The pick seeds the slot that fills the room into the
//! op's endpoints and width (in the pick's own command, so one undo
//! takes both back); generation and the preview refuse by name a slot
//! that does not fit the room - the confine law of
//! `machining2d::SlotBound`, shared with the trochoidal slot (#1503).
//! With nothing picked the typed endpoints rule, as before.
//!
//! Provenance: our own law over our own selection model; the plane
//! geometry is `machining2d::slot_room`'s. No vendor material consulted.

use super::*;
use crate::machining2d::{SlotBound, SlotFit};

/// The room a Slot's picks allow, as the world stands now, in the
/// frame of the setup `setup` (#2276: a part that setup holds turned
/// reads turned, through `setup_align::pose_in`): two open edges give
/// the strip between them, one face or closed ring its region. Any other selection is refused by name - a slot is never
/// guessed from picks that do not bound one.
///
/// # Errors
/// A vanished pick, a point or wall pick, or a selection that is not
/// two opposite edges or one face.
pub fn slot_bound_of(world: &World, setup: Option<&str>, items: &[GeoSel]) -> Result<SlotBound, GenError> {
    // #1868: picks on a DXF profile are one slot outline or one centerline.
    if let Some(bound) = super::slot_sketch::bound_of(world, items) {
        return bound;
    }
    let mut edges: Vec<Vec<Point2>> = Vec::new();
    let mut rings: Vec<Loop> = Vec::new();
    let mut areas = 0usize;
    for g in items {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!("the selection names '{}', which is gone", g.entity)));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!("'{}' has no geometry to select from", g.entity)));
        };
        let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
        let xy = |p: &[f64; 3]| {
            let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
            Point2::new(w.x, w.y)
        };
        let label = crate::pick::geo_label(world, g);
        let gone = || GenError::Invalid(format!("{label} is no longer in the model: re-pick it"));
        match &g.feature {
            FeatureSel::Face(fid) => {
                let f = features.faces.iter().find(|f| f.id == *fid).ok_or_else(gone)?;
                for l in f.loops.iter().filter(|l| !matches!(l.kind, LoopKind::Open)) {
                    let pts: Vec<Point2> = l.verts.iter().filter_map(|&vi| features.verts.get(vi as usize)).map(xy).collect();
                    rings.push(Loop::new(pts).map_err(|_| {
                        GenError::Invalid(format!("{label} bounds no area seen from above - pick the slot's floor face or two opposite edges"))
                    })?);
                }
                areas += 1;
            }
            FeatureSel::Chain(cid) => {
                let c = crate::model::resolve_chain(&features, *cid).ok_or_else(gone)?;
                let pts: Vec<Point2> = feature::chain_points(&features, &c).iter().map(xy).collect();
                if c.closed {
                    rings.push(Loop::new(pts).map_err(|_| {
                        GenError::Invalid(format!("{label} bounds no area seen from above - pick the slot's floor face or two opposite edges"))
                    })?);
                    areas += 1;
                } else {
                    edges.push(pts);
                }
            }
            FeatureSel::Patch(_)
            | FeatureSel::Vertex(_)
            | FeatureSel::SketchChain(_)
            | FeatureSel::SketchLayer(_) => {
                return Err(GenError::Invalid(format!(
                    "{label} is not an edge or a face - a Slot is picked by two opposite straight edges or one floor face"
                )));
            }
        }
    }
    match (edges.as_slice(), areas) {
        ([e1, e2], 0) => SlotBound::from_edges(e1, e2),
        ([], 1) => SlotBound::from_face(Region::new(rings)?),
        _ => Err(GenError::Invalid(format!(
            "a Slot is picked by two opposite straight edges or one floor face - this selection has {} open edge{} and {} face{} or ring{}",
            edges.len(),
            if edges.len() == 1 { "" } else { "s" },
            areas,
            if areas == 1 { "" } else { "s" },
            if areas == 1 { "" } else { "s" },
        ))),
    }
}

/// The confine law at the doors that read the world (the preview): a
/// Slot with picks must lie inside the room they bound. `Ok` for every
/// other kind, and for a Slot without picks (the typed endpoints rule).
///
/// # Errors
/// The picks bound no slot room, or the slot does not fit it.
pub(super) fn confine_slot(world: &World, setup: Option<&str>, src: Option<&SrcRef>, params: &OpParams) -> Result<(), GenError> {
    match (params, src) {
        (OpParams::Slot { a, b, width, .. }, Some(SrcRef::Selection(items))) if !items.is_empty() => {
            slot_bound_of(world, setup, items)?.judge(*a, *b, *width)
        }
        // #1868: the Trochoidal Slot on sketch picks obeys the same law.
        (OpParams::TrochoidalSlot { a, b, width, .. }, Some(src @ SrcRef::Selection(items)))
            if super::slot_sketch::is_sketch_slot(world, src, params) =>
        {
            slot_bound_of(world, setup, items)?.judge(*a, *b, *width)
        }
        _ => Ok(()),
    }
}

/// The Slot `params` with its endpoints and width set to the slot that
/// fills the room `items` bound - what a pick seeds (`SetOperationGeometry`,
/// `CreateOperation`). `None` for another kind, or picks that bound no
/// slot room yet (one edge of two picked so far): the typed numbers
/// stand, and generation names why.
#[must_use]
pub fn slot_from_picks(world: &World, setup: Option<&str>, params: &OpParams, items: &[GeoSel]) -> Option<OpParams> {
    // #1868: sketch picks seed the Trochoidal Slot too; its model picks
    // bound a region and seed nothing, as before.
    let sketch = super::slot_sketch::is_sketch_slot(world, &SrcRef::Selection(items.to_vec()), params);
    if !(matches!(params, OpParams::Slot { .. }) || sketch) || items.is_empty() {
        return None;
    }
    let seed = slot_bound_of(world, setup, items).and_then(|bound| match bound.centerline() {
        // A centerline gives the two ends; the typed width stands.
        Some((a, b)) => Ok((a, b, None)),
        None => bound.fit().map(|SlotFit { a, b, width }| (a, b, Some(width))),
    });
    let (fa, fb, fw) = match seed {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!("slot picks seed nothing: {e}");
            return None;
        }
    };
    let mut seeded = params.clone();
    if let OpParams::Slot { a, b, width, .. } | OpParams::TrochoidalSlot { a, b, width, .. } = &mut seeded {
        (*a, *b, *width) = (fa, fb, fw.unwrap_or(*width));
    }
    Some(seeded)
}

/// #3475: a NEW plain Slot's width from its tool - the seed's width, or
/// the tool diameter when that is wider, so that a default Slot always
/// holds the tool it is made with. Called where a Slot is made from a
/// seed with a known tool (the ribbon's quick-add, MCP `add_operation`
/// before its params patch, a change of kind); a width that the caller
/// or the picks state is never touched. A tool of the seed's width or
/// less leaves the seed exactly as it was.
pub fn seed_slot_for_tool(params: &mut OpParams, tool_diameter: f64) {
    if let OpParams::Slot { width, .. } = params
        && tool_diameter.is_finite()
        && tool_diameter > *width
    {
        *width = tool_diameter;
    }
}

/// #3475: does the slot fit law speak for this Slot's width? Not when its
/// picks seed no slot (`slot_from_picks` is `None`): the width is then
/// only the seed's fallback, and generation names the picks instead.
#[must_use]
pub fn slot_width_is_stated(world: &World, setup: Option<&str>, src: &SrcRef, params: &OpParams) -> bool {
    match src {
        SrcRef::Selection(items) => slot_from_picks(world, setup, params, items).is_some(),
        _ => true,
    }
}

/// What the op page echoes for a Slot with picks: the room and the
/// slot that fills it when the typed slot fits, else the refusal
/// generation would make. `None` without picks or for another kind.
#[must_use]
pub fn slot_fit_of(world: &World, op: &Operation) -> Option<Result<String, String>> {
    let (OpParams::Slot { a, b, width, .. }, SrcRef::Selection(items)) = (&op.params, &op.src) else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    let bound = match slot_bound_of(world, op.setup.as_deref(), items) {
        Ok(bd) => bd,
        Err(e) => return Some(Err(e.to_string())),
    };
    if let Err(e) = bound.judge(*a, *b, *width) {
        return Some(Err(e.to_string()));
    }
    let (l, w) = bound.room();
    if bound.centerline().is_some() {
        return Some(Ok(format!("From selection: centerline {l:.3} mm long - the slot follows it at the typed width")));
    }
    Some(Ok(format!("From selection: room {l:.3} x {w:.3} mm - the slot stays inside it")))
}

//! The keep law (#1603): the pocket family (2D Pocket, 2D Adaptive, 2D
//! Offset) clears material AROUND a part it must keep, and answers for
//! that part on the operation itself - whole-model source or stock
//! source alike - so the verdict never rides on the program sim alone.
//!
//! Provenance: our own design (`docs/decisions/1595-engrave-mark-law.md`,
//! the #1603 section) over the sim crate's solid law
//! (`verify::solid_hits`). No vendor material consulted.
//!
//! ## The law
//!
//! - **Whole-model source** (`DepthLaw::Keep`): the part's sections are
//!   islands (#12), so with the default area controls every move stands
//!   outside the part. An area offset or extension moves the boundary
//!   INTO those islands, and nothing on the operation judged that. The
//!   solid law, nothing excused, now runs against the source's own mesh.
//!   Only where #12's reading applies (`GenJob::around_part`): with no
//!   stock set up the model is the volume to remove, and nothing is kept.
//! - **Stock source** (or a pick off the setup's stock): the source is
//!   the material to remove, but the parts standing inside it are not
//!   (#1554). The same solid law runs against the setup's parts
//!   ([`parts_in_stock`]). A document with no part judges nothing.
//!   #1607 widened this from the pocket family to every 2D kind that
//!   answers for the part on a part source ([`is_stock_kept_kind`]):
//!   the typed kinds (Slot, Trochoidal slot, Circular), Contour and
//!   Face take the same solid law; Engrave and Trace take their own
//!   mark law (`mark_law`) against the parts instead, so a mark on the
//!   part's surface reads the same whether the line came off the part
//!   or off the stock. #1631 added the hole kinds (Drill without
//!   cross holes, Bore, Thread), which take their own containment law
//!   against the parts - the round wall of a hole the part models where
//!   the op declares one is excused, nothing else - and the 2D Chamfer,
//!   which takes the solid law: a stock edge is not a part edge.
//!   #1635 added the cross drill (a Drill with cross holes,
//!   [`is_cross_drill`]): its chain answers to its own radial part law
//!   (`cross_part`) against the setup's parts, as it does against its
//!   source on a part source - each declared cross hole's round wall,
//!   about its own axis, excused; never the stock it was read off.
//!
//! - **Part source, neighbouring parts** (#1621): a part-source
//!   operation's own law reads its source's mesh only, so a cut on part
//!   B that enters part A beside it had no finding on the operation -
//!   and with the stock turned (the sim refuses to judge) nothing judged
//!   A at all. Every part-source operation that carries a part law
//!   (`depth_law_of` not `None`) now also answers to the plain solid law
//!   against the setup's OTHER parts ([`neighbour_parts`]): no mark
//!   budget there (a mark is on the part it marks), no hole excused (the
//!   holes are the source's), nothing kept around. Its own source keeps
//!   its own law. #1641: the cross drill too (its `DepthLaw` reads
//!   `None`, so it was skipped): its chain answers to the radial law
//!   (`cross_part::neighbours_clear`) against the other parts, no
//!   cross hole's wall excused there.
//!
//! The cutter's own tip and stack are swept against the mesh, so a legal
//! pocket - its rings tangent to the part - stays clean, and a cut that
//! enters the part is a blocking `Gouge` on its move.

use crate::geometry::Triangle;

use super::{OpParams, SrcRef};

/// Whether `params` is a pocket-family kind: it clears an area and
/// keeps the part.
#[must_use]
pub fn is_keep_kind(params: &OpParams) -> bool {
    matches!(params, OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. })
}

/// #1655: a pocket-family op that clears the stock AROUND the part (a
/// whole-model source, #12, or the stock source with parts, #1554) says
/// by name each band of that area narrower than the tool: the generator
/// gives such a band no pass, and refuses only when nothing is left to
/// cut. `cleared_around` is that reading; every other job is unchanged.
/// The note never blocks the cut: a region the band check cannot read
/// keeps its path and says that the check did not run.
pub(super) fn narrow_noted(
    mut path: crate::cl::Toolpath,
    params: &OpParams,
    region: &crate::geo2d::Region,
    tool_diameter: f64,
    cleared_around: bool,
    unit: crate::units::Unit,
) -> crate::cl::Toolpath {
    let leave = match params {
        OpParams::Pocket { stock_to_leave, .. }
        | OpParams::Adaptive2D { stock_to_leave, .. }
        | OpParams::Offset2D { stock_to_leave, .. } => *stock_to_leave,
        _ => return path,
    };
    if !cleared_around {
        return path;
    }
    let label = params.kind().info().label;
    match crate::machining2d::unreached_note(region, tool_diameter, leave, unit) {
        Ok(Some(n)) => path.notes.push(format!("{label}: {n}")),
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(kind = label, "the band check around the part did not run: {e}");
            path.notes.push(format!(
                "{label}: the check for bands of the stock narrower than the tool did not run ({e}) - look at the stock around the part"
            ));
        }
    }
    path
}

/// Whether `params` answers for the parts standing in its stock when
/// its source is the stock (#1603, #1607, #1631): every 2D kind that
/// carries a part law on a part source - the pocket family (`Keep`),
/// Slot, Trochoidal slot and Circular (`authored_law`), Contour and Face
/// (`Section`), Engrave and Trace (`Mark`), the hole kinds (`Contain`,
/// [`is_hole_kind`]) - and the 2D Chamfer (#1631: on a part source its
/// break is the part's own edge; off the stock, the solid law) - and
/// the cross drill (#1635, [`is_cross_drill`]: `cross_part`).
#[must_use]
pub fn is_stock_kept_kind(params: &OpParams) -> bool {
    is_keep_kind(params)
        || super::mark_law::is_mark_kind(params)
        || is_hole_kind(params)
        || is_cross_drill(params)
        || matches!(
            params,
            OpParams::Slot { .. }
                | OpParams::TrochoidalSlot { .. }
                | OpParams::Circular { .. }
                | OpParams::Contour { .. }
                | OpParams::Face { .. }
                | OpParams::Chamfer2D { .. }
        )
}

/// Whether `params` is a hole kind that answers to the containment law
/// (`DepthLaw::Contain`) on a part source: a Drill with no cross holes
/// (a cross chain answers to `cross_part` against its source), a Bore,
/// a Thread. Off the stock (#1631) the same law reads the setup's
/// parts: the drill, bar or mill may enter a part only through a hole
/// the part models there - its declared hole's round wall excused, to
/// its diameter and no deeper than its floor - and anywhere else it is
/// a gouge.
#[must_use]
pub fn is_hole_kind(params: &OpParams) -> bool {
    match params {
        OpParams::Drill { cross, .. } => cross.is_empty(),
        OpParams::Bore { .. } | OpParams::Thread { .. } => true,
        _ => false,
    }
}

/// Whether `params` is a cross drill: a Drill with cross holes. Its
/// chain runs along each hole's own axis and answers to `cross_part`
/// (the cross holes' walls excused about their axes), never to the
/// vertical-wall containment law; off the stock (#1635) that law reads
/// the setup's parts ([`parts_in_stock`]) instead of the stock.
#[must_use]
pub fn is_cross_drill(params: &OpParams) -> bool {
    matches!(params, OpParams::Drill { cross, .. } if !cross.is_empty())
}

/// The parts an operation whose source is NOT the part (the stock, or
/// picks off it) must still keep: the setup's parts in its frame - what
/// the program sim judges against (`setup_align::parts_facets_in`:
/// fixtures and a stock-only model left out). Empty for a kind outside
/// [`is_stock_kept_kind`], and for a part-solid source (whose own mesh
/// its `DepthLaw` reads).
#[must_use]
pub(super) fn parts_in_stock(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
    part_solid: bool,
) -> Vec<Triangle> {
    if part_solid || !is_stock_kept_kind(params) {
        return Vec::new();
    }
    crate::setup_align::parts_facets_in(world, setup)
}

/// The parts a part-source operation must keep beside its own (#1621):
/// every part the setup holds (`setup_align::parts_facets_in`'s set -
/// fixtures and a stock-only model left out) but the entities its
/// source names, in the setup's frame. Empty for a source that is not
/// the part (whose parts [`parts_in_stock`] reads), for a kind that
/// carries no part law on a part source, and for a one-part document.
/// A cross drill (#1641, [`is_cross_drill`]) carries one - its radial
/// `cross_part` - and judges these parts in it, nothing excused.
#[must_use]
pub(super) fn neighbour_parts(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
    src: &SrcRef,
    part_solid: bool,
) -> Vec<Triangle> {
    if !part_solid {
        return Vec::new();
    }
    kept_models(world, params, src, true)
        .into_iter()
        .filter_map(|(_, e)| crate::setup_align::facets_in(world, &e, setup))
        .flatten()
        .collect()
}

/// Whether a part-source operation keeps neighbouring parts. Generation
/// and the live posting check share this rule, including kinds whose
/// own source is judged outside `depth_law_of`.
pub(super) fn is_part_kept_kind(params: &OpParams, src: &SrcRef) -> bool {
    super::generate::depth_law_of(params, src, true) != super::generate::DepthLaw::None
        || is_cross_drill(params)
        || matches!(params, OpParams::Pocket3D { .. } | OpParams::MultiAxisRough { .. })
}

/// The models whose meshes an operation's part laws read beyond its own
/// source: on a part source ([`neighbour_parts`]) the setup's other
/// parts, on a stock source ([`parts_in_stock`]) every part. Empty for a
/// kind that carries no such law.
fn kept_models(
    world: &flecs_ecs::prelude::World,
    params: &OpParams,
    src: &SrcRef,
    part_solid: bool,
) -> Vec<(String, flecs_ecs::prelude::Entity)> {
    let own: Vec<&str> = match src {
        _ if !part_solid => {
            if !is_stock_kept_kind(params) {
                return Vec::new();
            }
            Vec::new()
        }
        // #1641: a cross drill carries its radial part law
        // (`cross_part`) though its `DepthLaw` reads `None`. #1749: so
        // does a 3D Pocket (and its stamped twin), whose cutter now passes its model's
        // silhouette edge - its footprint keeps out of these parts, and
        // the solid law below holds them as the second net.
        _ if !is_part_kept_kind(params, src) => return Vec::new(),
        SrcRef::Stock => return Vec::new(),
        SrcRef::Entity(n) => vec![n.as_str()],
        SrcRef::Selection(items) => items.iter().map(|g| g.entity.as_str()).collect(),
    };
    let skip = crate::stock_source::stock_only_model(world);
    crate::model::model_entities(world)
        .into_iter()
        .filter(|(name, e)| !own.contains(&name.as_str()) && crate::stock_source::judged(world, e, skip))
        .collect()
}

/// The parts an operation's part laws read beyond its own source, one
/// mesh per model, by name, in the setup's frame (#1628): the set
/// [`parts_in_stock`] (a stock source) or [`neighbour_parts`] (a part
/// source) reads, as it stands NOW - the post's live re-judge
/// (`kept_rejudge`) names the part a cut enters.
#[must_use]
pub(super) fn kept_parts_named(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
    src: &SrcRef,
    part_solid: bool,
) -> Vec<(String, Vec<Triangle>)> {
    kept_models(world, params, src, part_solid)
        .into_iter()
        .filter_map(|(name, e)| crate::setup_align::facets_in(world, &e, setup).map(|f| (name, f)))
        .collect()
}

/// #1637: a stock-source 2D Chamfer whose solid-law check hits a part
/// breaks an edge belonging to the part rather than stock-only material.
/// Rewrite `Warning::Gouge` into `Warning::StockChamferGouge` so every
/// channel names the cause and the part-source workaround.
pub(super) fn note_stock_chamfer_gouges(
    params: &OpParams,
    part_solid: bool,
    warns: &mut [crate::verify::Warning],
) {
    if part_solid || !matches!(params, OpParams::Chamfer2D { .. }) {
        return;
    }
    for w in warns {
        if let crate::verify::Warning::Gouge { at } = *w {
            *w = crate::verify::Warning::StockChamferGouge { at };
        }
    }
}

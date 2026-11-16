//! Heights measured from the operation's own selection (M11.3 A): the Z
//! span of the geometry an operation machines, read through the same
//! feature walk generation runs. Provenance: our own design notes and
//! the public CAM convention of a height "from the selection"; no
//! vendor material consulted.
//!
//! The rule, per source - the span is (lowest Z, highest Z) in WORLD
//! coordinates, through each entity's placement:
//!
//! - `SrcRef::Selection` - the union over every pick of:
//!   - a picked face: its boundary loops' vertices (a planar face's Z
//!     extent is its boundary's; a flat face's span is its own Z, twice);
//!   - a welded wall (patch): every strip face of the weld, same rule -
//!     so a dome's crown counts even where no rim runs over it;
//!   - an edge chain (closed loop or open run): its points.
//! - `SrcRef::Entity` - that entity's world bounding box.
//! - `SrcRef::Stock` - the setup's stock box. The stock IS the selection
//!   of a stock-sourced operation; resolving (not refusing) means a
//!   later source change can never strand a height that was legal when
//!   it was written - it re-resolves against the new source, the op goes
//!   Stale, and the panel echoes the new Z.
//!
//! Top / Clearance / Retract take the highest Z, Bottom the lowest (the
//! reference names its end: [`HeightReference::TopOfSelection`] /
//! [`HeightReference::BottomOfSelection`]); the offset adds as for every
//! other reference. A selection that yields no finite Z refuses by name
//! - never a silent 0.

use super::*;

/// The empty-pick-list refusal, quoted by the command door and by
/// generation alike.
pub const SELECTION_EMPTY: &str =
    "the selection is empty - pick faces or edges in the Geometry section";
/// Asked without the operation's source in hand (a context that only
/// knows the setup): never guessed.
pub const SELECTION_UNKNOWN: &str = "no operation selection in this context";

/// The (lowest, highest) world Z of an operation's own source geometry,
/// by the rule in this module's header. Err is the reason, phrased for a
/// diagnostic the user can act on.
pub fn selection_z_range(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<(f64, f64), &'static str> {
    let span = match src {
        SrcRef::Stock => model::stock_world_aabb_for(world, setup)
            .map(|b| (b.min.z, b.max.z))
            .ok_or("no stock yet - set it up in Setup > Stock")?,
        SrcRef::Entity(name) => {
            let e = model::find_by_name(world, name)
                .ok_or("the operation's source entity is gone")?;
            // #326: the source as its setup holds it.
            crate::setup_align::entity_aabb_in(world, &e, setup)
                .map(|b| (b.min.z, b.max.z))
                .ok_or("the operation's source entity carries no geometry")?
        }
        SrcRef::Selection(items) => picks_z_range(world, setup, items)?,
    };
    let (lo, hi) = span;
    if lo.is_finite() && hi.is_finite() && lo <= hi {
        Ok((lo, hi))
    } else {
        Err("the selection has no finite height")
    }
}

/// The picks' half of [`selection_z_range`].
fn picks_z_range(world: &World, setup: Option<&str>, items: &[GeoSel]) -> Result<(f64, f64), &'static str> {
    if items.is_empty() {
        return Err(SELECTION_EMPTY);
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in items {
        let entity = model::find_by_name(world, &g.entity)
            .ok_or("a picked entity is gone - re-pick the selection")?;
        let features = model::features_of(world, &entity)
            .ok_or("a picked entity carries no geometry - re-pick the selection")?;
        let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default(); // #326
        let mut take = |vi: u32| {
            if let Some(p) = features.verts.get(vi as usize) {
                let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                lo = lo.min(w.z);
                hi = hi.max(w.z);
            }
        };
        let gone = "a picked feature is no longer in the model - re-pick it";
        match g.feature {
            FeatureSel::Face(fid) => {
                let f = features.faces.iter().find(|f| f.id == fid).ok_or(gone)?;
                f.loops.iter().flat_map(|l| l.verts.iter()).for_each(|&v| take(v));
            }
            FeatureSel::Patch(pid) => {
                let p = feature::patches(&features)
                    .into_iter()
                    .find(|p| p.id == pid)
                    .ok_or(gone)?;
                for f in features.faces.iter().filter(|f| p.faces.contains(&f.id)) {
                    f.loops.iter().flat_map(|l| l.verts.iter()).for_each(|&v| take(v));
                }
            }
            FeatureSel::Vertex(vid) => {
                take(feature::resolve_vertex(&features, vid).ok_or(gone)?);
            }
            FeatureSel::Chain(cid) => {
                let c = feature::resolve_chain(&features, cid).ok_or(gone)?;
                for p in feature::chain_points(&features, &c) {
                    let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                    lo = lo.min(w.z);
                    hi = hi.max(w.z);
                }
            }
        }
    }
    Ok((lo, hi))
}

/// The base Z of a selection reference: the span's end it names. `src`
/// None (a caller holding no operation) is unresolved, never a guess.
pub(super) fn selection_base(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    r: HeightReference,
) -> Option<f64> {
    let (lo, hi) = selection_z_range(world, setup, src?).ok()?;
    match r {
        HeightReference::BottomOfSelection => Some(lo),
        _ => Some(hi),
    }
}

/// The command door's question (M11.3 A): every height of `params` that
/// reads the selection must resolve against `src`, else the refusal
/// names the height's source and why - at creation, at a parameter
/// edit and at a source change, before any generator sees it.
pub fn selection_heights_sane(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Option<String> {
    let mut uses = false;
    params.for_each_height(|h| uses |= h.reference.is_selection());
    if !uses {
        return None;
    }
    selection_z_range(world, setup, src)
        .err()
        .map(|why| format!("a height is measured From: Selection, which does not resolve: {why}"))
}

/// Ordering sanity on resolved values: an operation whose top is not
/// above its bottom (or whose finished cut is not below its first stage)
/// is refused at command creation. A height that cannot resolve is not
/// here's business: `resolve_params` refuses it at generation, by name.
/// (Moved beside its sibling door from `heights` at M11.3 A, unchanged
/// but for the `src` the Selection references read.)
#[must_use]
pub fn depth_sane(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> bool {
    let Ok(r) = resolve_params(world, setup, src, params) else {
        return true; // unresolved is refused elsewhere, never a false lie here
    };
    match params {
        OpParams::Face { .. } => match (r.face_top, r.bottom) {
            (Some(t), Some(c)) => t >= c,
            _ => true,
        },
        // #1409: a Facing whose top meets its facing plane is a part
        // already at length - legal to create, and its generate says
        // "nothing to face" by name, rather than this door calling a
        // level window an upward cut.
        OpParams::Facing { .. } => match (r.top, r.bottom) {
            (Some(t), Some(b)) => t >= b,
            _ => true,
        },
        _ => match (r.top, r.bottom) {
            (Some(t), Some(b)) => t > b,
            _ => true,
        },
    }
}

/// The microlift door (#774): a trochoidal slot's lifted return must
/// not rise above the clearance plane, read on the resolved heights
/// ([`OpParams::microlift_refusal`] holds the rule). An unresolved
/// height is refused elsewhere, by name.
#[must_use]
pub fn microlift_sane(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> Option<String> {
    let r = resolve_params(world, setup, src, params).ok()?;
    params.microlift_refusal(r.clearance, r.top?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{History, ImportModel};
    use crate::geometry::cube;

    fn face(clearance_z: Height) -> OpParams {
        OpParams::Face { holes: Default::default(),
            tilt: None,
            blend_turns: false,
            stepover: 8.0,
            clearance_z,
            cut_z: Height::fixed(-0.5),
            top_z: None,
            stepdown: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        }
    }

    fn digest(world: &World, p: &OpParams, src: &SrcRef) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(world, None, Some(src), &mut h);
        h.finish()
    }

    /// The digest law of the appended tags: params that never read the
    /// selection digest identically whatever the source (no byte moves
    /// for any pre-M11.3 op), and a Selection height digests the Z it
    /// resolves to - so two sources with different spans differ.
    #[test]
    fn the_stamp_reads_the_selection_only_when_a_height_does() {
        let world = crate::world::new_world();
        let mut h = History::default();
        let lo_mesh = cube(Point3::ZERO, Point3::new(1.0, 1.0, 2.0));
        let hi_mesh = cube(Point3::ZERO, Point3::new(1.0, 1.0, 7.0));
        h.execute(&world, ImportModel::from_mesh("lo", lo_mesh));
        h.execute(&world, ImportModel::from_mesh("hi", hi_mesh));
        let lo = SrcRef::Entity("lo".into());
        let hi = SrcRef::Entity("hi".into());
        let fixed = face(Height::fixed(25.0));
        assert_eq!(digest(&world, &fixed, &lo), digest(&world, &fixed, &hi));
        let sel = face(Height::of(HeightReference::TopOfSelection, 25.0));
        assert_ne!(digest(&world, &sel, &lo), digest(&world, &sel, &hi));
        assert_ne!(digest(&world, &sel, &lo), digest(&world, &fixed, &lo), "the tag is its own");
        let bot = face(Height::of(HeightReference::BottomOfSelection, 25.0));
        assert_ne!(digest(&world, &sel, &lo), digest(&world, &bot, &lo), "the two ends differ");
    }
}

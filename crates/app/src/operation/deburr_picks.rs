//! The Deburr operation's picked edge chains (#1888), read from the
//! world: the source's picks are the edges to ride, `exclude_edges` the
//! edges to leave out. Provenance: our own resolution over the feature
//! layer's chain ids, the walk `selection::drive_chains` does for the
//! drive-curve contour, with the deburr's own refusals.
//!
//! One list answers both: the source's chains first (one per chain
//! pick), then the excluded ones. [`picked`] says where the list
//! splits. An `Entity` or `Stock` source, or a selection with no edge
//! chain in it, has no picked chains: the ride finds the model's
//! exposed edges itself. A face, patch or vertex pick names the model
//! the ride reads, as it did before the chains (#2284's pin).

use super::*;

/// How many leading chains of [`chains_of`]'s list are the source's.
pub(super) fn picked(src: &SrcRef) -> usize {
    match src {
        SrcRef::Selection(items) => items
            .iter()
            .filter(|g| matches!(g.feature, FeatureSel::Chain(_)))
            .count(),
        SrcRef::Stock | SrcRef::Entity(_) => 0,
    }
}

/// The selection law and the models read by excluded chains are cache
/// inputs. A missing model cannot validate an existing toolpath.
pub(super) fn stamp_inputs(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    exclude: &[GeoSel],
    h: &mut impl std::hash::Hasher,
) -> Option<()> {
    // Before #1888, chain picks named whole models. Keep their cached
    // automatic rides stale until the selected edges are regenerated.
    if picked(src) != 0 {
        h.write(b"deburr-selected-edges-v1");
    }
    for g in exclude {
        h.write(b"deburr-exclusion-model-v1");
        let entity = model::find_by_name(world, &g.entity)?;
        let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
        for v in [
            loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3],
        ] {
            h.write(&v.to_bits().to_le_bytes());
        }
        h.write_u64(model::geometry_signature(world, &entity)?);
    }
    Some(())
}

/// The picked chains, then the excluded chains, in the setup's frame.
pub(super) fn chains_of(
    world: &World,
    op: &Operation,
    exclude: &[GeoSel],
) -> Result<Vec<Vec<Point3>>, GenError> {
    let mut out = Vec::new();
    if let SrcRef::Selection(items) = &op.src {
        for g in items
            .iter()
            .filter(|g| matches!(g.feature, FeatureSel::Chain(_)))
        {
            out.push(chain(world, op.setup.as_deref(), g, "to break")?);
        }
    }
    for g in exclude {
        out.push(chain(world, op.setup.as_deref(), g, "to exclude")?);
    }
    Ok(out)
}

/// One pick as a polyline where it stands; anything but a live edge
/// chain is refused by its label.
fn chain(
    world: &World,
    setup: Option<&str>,
    g: &GeoSel,
    role: &str,
) -> Result<Vec<Point3>, GenError> {
    let Some(entity) = model::find_by_name(world, &g.entity) else {
        return Err(GenError::Invalid(format!(
            "the Deburr edges {role} name '{}', which is gone",
            g.entity
        )));
    };
    let Some(features) = model::features_of(world, &entity) else {
        return Err(GenError::Invalid(format!(
            "'{}' has no geometry to select from",
            g.entity
        )));
    };
    let label = crate::pick::geo_label(world, g);
    let FeatureSel::Chain(cid) = g.feature else {
        return Err(GenError::Invalid(format!(
            "{label} is not an edge chain - Deburr takes edge chains as the edges {role}"
        )));
    };
    let Some(c) = crate::model::resolve_chain(&features, cid) else {
        return Err(GenError::Invalid(format!(
            "{label} is no longer in the model: re-pick it"
        )));
    };
    let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
    let pts: Vec<Point3> = feature::chain_points(&features, &c)
        .iter()
        .map(|p| transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot))
        .collect();
    if pts.len() < 2 {
        return Err(GenError::Invalid(format!(
            "{label} holds a single point - re-pick a real edge"
        )));
    }
    Ok(pts)
}

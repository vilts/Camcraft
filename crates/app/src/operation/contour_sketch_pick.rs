//! #1864: Enumerate and resolve DXF sketch chains from `SrcRef` selections and compute
//! sketch Z bounds and UI per-chain descriptors.
//!
//! Provenance: clean-room enumeration of `geometry::sketch_chains` entities mapped through
//! `sketch::world_pose`; original CamCraft code, no vendor material consulted.

use std::collections::HashSet;

use flecs_ecs::prelude::World;

use crate::geo2d::Point2;
use crate::geometry::sketch_chains::{ChainId, LoopRole, SketchChainRef};
use crate::geometry::{Point3, transform_point};
use crate::machining2d::{GenError, Side};
use crate::schema::Location;

use super::contour_sketch::ResolvedSketchChain;
use super::{ChainOverride, ChainSide, FeatureSel, GeoSel, SrcRef};

/// Read-only UI descriptor for one cuttable chain in a contour operation's DXF source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SketchChainInfo {
    pub entity: String,
    pub chain: ChainId,
    pub label: String,
    pub layer: String,
    pub closed: bool,
    /// Natural `Side::Auto` side (`Some(Outside | Inside)`) for unambiguous closed chains,
    /// `None` for open chains or ambiguous closed chains.
    pub auto_side: Option<Side>,
}

pub(super) fn transform_chain_pts(pose: Location, pts: &[Point2]) -> Result<Vec<Point2>, GenError> {
    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let w = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
        if !(w.x.is_finite() && w.y.is_finite()) {
            return Err(GenError::Invalid(
                "the DXF profile transforms to non-finite coordinates".into(),
            ));
        }
        out.push(Point2::new(w.x, w.y));
    }
    Ok(out)
}

/// `(min_z, max_z)` across every selected DXF sketch entity in world space (uncached, for pins).
#[must_use]
pub fn sketch_z_range_fresh(
    world: &World,
    src: &SrcRef,
) -> Option<Result<(f64, f64), &'static str>> {
    let profiles = crate::sketch_model::source_profiles(world, src);
    if profiles.is_empty() {
        return None;
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for name in profiles {
        let Some((entity, sketch)) = crate::sketch::find(world, &name) else {
            return Some(Err("the DXF profile is gone"));
        };
        let Some(pose) = crate::sketch::world_pose(world, &entity) else {
            return Some(Err("the DXF profile has no stock to lie on - set up stock first"));
        };
        for chain in sketch.data.chains().all() {
            for p in chain.sample(0.1) {
                let w = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
                lo = lo.min(w.z);
                hi = hi.max(w.z);
            }
        }
    }
    if lo.is_finite() && hi.is_finite() && lo <= hi {
        Some(Ok((lo, hi)))
    } else {
        Some(Err("the DXF profile has no finite height"))
    }
}

fn side_word(s: ChainSide) -> &'static str {
    match s {
        ChainSide::Inside => "Inside",
        ChainSide::Outside => "Outside",
    }
}

pub(super) fn visit_src_chains(
    world: &World,
    src: &SrcRef,
    mut f: impl FnMut(&str, Option<Location>, SketchChainRef<'_>, String) -> Result<(), GenError>,
) -> Result<(), GenError> {
    let mut seen: HashSet<(String, ChainId)> = HashSet::new();
    let mut emit = |entity_name: &str,
                    pose: Option<Location>,
                    chain: SketchChainRef<'_>|
     -> Result<(), GenError> {
        let chain_id = chain.id();
        if !seen.insert((entity_name.to_owned(), chain_id)) {
            return Ok(());
        }
        let label = crate::pick::geo_label(
            world,
            &GeoSel {
                entity: entity_name.to_owned(),
                feature: FeatureSel::SketchChain(chain_id),
            },
        );
        f(entity_name, pose, chain, label)
    };

    match src {
        SrcRef::Entity(name) => {
            let Some((entity, sketch)) = crate::sketch::find(world, name) else {
                return Err(GenError::Invalid(format!("no DXF profile named '{name}'")));
            };
            let pose = crate::sketch::world_pose(world, &entity);
            for chain in sketch.data.chains().all() {
                emit(name, pose, chain)?;
            }
        }
        SrcRef::Selection(items) => {
            for g in items {
                let Some((entity, sketch)) = crate::sketch::find(world, &g.entity) else {
                    return Err(GenError::Invalid(format!(
                        "the selection names '{}', which is not a DXF profile",
                        g.entity
                    )));
                };
                let pose = crate::sketch::world_pose(world, &entity);
                let label = crate::pick::geo_label(world, g);
                match &g.feature {
                    FeatureSel::SketchChain(id) => {
                        let Some(chain) = sketch.data.chains().find(*id) else {
                            return Err(GenError::Invalid(format!(
                                "{label} is no longer in DXF profile '{}' - re-pick it",
                                g.entity
                            )));
                        };
                        emit(&g.entity, pose, chain)?;
                    }
                    FeatureSel::SketchLayer(layer) => {
                        if !sketch.data.layers().iter().any(|l| l == layer) {
                            return Err(GenError::Invalid(format!(
                                "layer '{layer}' is no longer in DXF profile '{}' - re-pick it",
                                g.entity
                            )));
                        }
                        for chain in sketch.data.chains().by_layer(layer) {
                            emit(&g.entity, pose, chain)?;
                        }
                    }
                    _ => {
                        return Err(GenError::Invalid(format!(
                            "{label}: '{}' is a DXF profile - pick a sketch chain or layer",
                            g.entity
                        )));
                    }
                }
            }
        }
        SrcRef::Stock => {
            return Err(GenError::Invalid("stock source is not a DXF profile".into()));
        }
    }
    Ok(())
}

/// UI metadata list for every cuttable DXF sketch chain named by `src` (uncached, for pins).
/// Returns an empty list if `src` names no placed DXF profiles or cannot be resolved cleanly.
#[must_use]
pub fn sketch_chain_infos_fresh(world: &World, src: &SrcRef) -> Vec<SketchChainInfo> {
    if crate::sketch_model::source_profiles(world, src).is_empty() {
        return Vec::new();
    }
    let mut infos = Vec::new();
    let _ = visit_src_chains(world, src, |entity_name, _, chain, label| {
        let auto_side = chain.nesting().and_then(|res| {
            res.as_ref().ok().map(|n| match n.role {
                LoopRole::Outer => Side::Outside,
                LoopRole::Hole => Side::Inside,
            })
        });
        infos.push(SketchChainInfo {
            entity: entity_name.to_owned(),
            chain: chain.id(),
            label,
            layer: chain.layer().to_owned(),
            closed: chain.is_closed(),
            auto_side,
        });
        Ok(())
    });
    infos
}

pub(super) fn collect_chains(
    world: &World,
    src: &SrcRef,
    tolerance: f64,
    op_side: Side,
    op_climb: bool,
    overrides: &[ChainOverride],
) -> Result<Vec<ResolvedSketchChain>, GenError> {
    let mut out: Vec<ResolvedSketchChain> = Vec::new();
    visit_src_chains(world, src, |entity_name, pose_opt, chain, label| {
        let Some(pose) = pose_opt else {
            return Err(GenError::Invalid(format!(
                "DXF profile '{entity_name}' has no stock to lie on - set up stock first"
            )));
        };
        let chain_id = chain.id();
        let closed = chain.is_closed();
        let pts = transform_chain_pts(pose, &chain.sample(tolerance))?;
        if (closed && pts.len() < 3) || (!closed && pts.len() < 2) {
            return Err(GenError::Invalid(format!(
                "{label} has too few points to machine"
            )));
        }
        let ov = overrides
            .iter()
            .find(|o| o.entity == entity_name && o.chain == chain_id);
        let eff_climb = op_climb ^ ov.is_some_and(|o| o.reverse);
        let mut parent = None;
        let mut eff_side = None;
        let mut natural_auto = false;
        let mut open_side_err = None;
        let mut auto_role_err = None;
        if !closed {
            if let Some(s) = ov.and_then(|o| o.side) {
                open_side_err = Some(format!(
                    "{label} is an open sketch chain - per-chain {} side compensation is not allowed; clear its side override to cut along the centerline",
                    side_word(s)
                ));
            } else if op_side != Side::Auto {
                let word = if op_side == Side::Inside { "Inside" } else { "Outside" };
                open_side_err = Some(format!(
                    "{label} is an open sketch chain - {word} side compensation is not allowed on open chains; choose Side 'Auto' or pick closed chains"
                ));
            }
        } else if let Some(s) = ov.and_then(|o| o.side) {
            eff_side = Some(match s {
                ChainSide::Inside => Side::Inside,
                ChainSide::Outside => Side::Outside,
            });
        } else if op_side != Side::Auto {
            eff_side = Some(op_side);
        } else {
            natural_auto = true;
            if let Some(nest) = chain.nesting() {
                match nest {
                    Ok(n) => {
                        parent = n.parent;
                        eff_side = Some(match n.role {
                            LoopRole::Outer => Side::Outside,
                            LoopRole::Hole => Side::Inside,
                        });
                    }
                    Err(e) => {
                        auto_role_err = Some(format!(
                            "{label}: cannot determine inside vs outside automatically ({e}) - set a per-chain side override"
                        ));
                    }
                }
            }
        }
        out.push(ResolvedSketchChain {
            chain_id,
            entity: entity_name.to_owned(),
            label,
            closed,
            pts,
            parent,
            eff_side,
            eff_climb,
            natural_auto,
            open_side_err,
            auto_role_err,
        });
        Ok(())
    })?;
    if out.is_empty() {
        return Err(GenError::Empty(
            "the DXF profile selection contains no cuttable closed or open chains".into(),
        ));
    }
    Ok(out)
}

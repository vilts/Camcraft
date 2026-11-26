//! Shared per-entity user-friendly feature name table (#2403), cached as an
//! ECS component keyed by `Revision` + live `Weak<[Triangle]>` so every page,
//! status line, refusal message, and MCP view reads one stable name without
//! per-frame recomputation or leaked hexadecimal ids.
//!
//! Provenance: our own Revision-keyed ECS cache wrapping
//! `geometry::feature::build_feature_names`; no vendor material consulted.

use std::cell::Cell;
use std::sync::Arc;

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::Triangle;
use crate::geometry::feature::{FeatureNames, NameInputs, VertexId};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::{Facets, FeatureNamesMemo};

use super::feature_graph_shared;

thread_local! {
    /// How many times a `FeatureNames` table was constructed on this thread
    /// (pins verify once-per-geometry-change caching, zero per frame).
    static NAME_BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// Number of `FeatureNames` table builds on this thread.
#[must_use]
pub fn name_builds() -> u64 {
    NAME_BUILDS.with(Cell::get)
}

/// Cached user-friendly name table for `entity` in its local frame.
/// Returns `None` when `entity` has no mounted `Facets`.
#[must_use]
pub fn feature_names_of(world: &World, entity: &Entity) -> Option<Arc<FeatureNames>> {
    let view = entity.entity_view(world);
    let tris: Arc<[Triangle]> = view.try_get::<&Facets>(|f| f.tris.clone())?;
    let rev = model::rev_of(world, entity);
    let key = Arc::downgrade(&tris);
    if let Some(hit) = view
        .try_get::<&FeatureNamesMemo>(|m| (m.rev == rev && m.tris.ptr_eq(&key)).then(|| m.table.clone()))
        .flatten()
    {
        return Some(hit);
    }
    NAME_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let g = feature_graph_shared(&tris);
    let patches = model::patches(&g.features);
    let chains = g.chains();
    let runs = g.runs();
    let pick_verts: Vec<(u32, VertexId)> = g.points.iter().map(|p| (p.vert, p.id)).collect();
    let table = Arc::new(crate::geometry::feature::build_feature_names(&NameInputs {
        features: &g.features,
        tris: &tris,
        patches: &patches,
        chains,
        runs,
        pick_edges: &g.edges,
        pick_verts: &pick_verts,
    }));
    view.set(FeatureNamesMemo {
        rev,
        tris: key,
        table: table.clone(),
    });
    Some(table)
}

/// Lookup one item's friendly name (without `" of '<entity>'"`) if resolved.
#[must_use]
pub fn feature_name(world: &World, entity: &str, sel: &FeatureSel) -> Option<String> {
    if let FeatureSel::SketchLayer(layer) = sel {
        return crate::sketch::find(world, entity).and_then(|(_, s)| {
            s.data.layers().iter().any(|l| l == layer).then(|| {
                let n = s.data.chains().by_layer(layer).len();
                format!("Layer '{layer}' ({n} chain{})", if n == 1 { "" } else { "s" })
            })
        });
    }
    if let FeatureSel::SketchChain(id) = sel {
        return crate::sketch::find(world, entity).and_then(|(_, s)| {
            let chains = s.data.chains();
            let c = chains.find(*id)?;
            let idx = chains.index_of(*id).unwrap_or(1);
            let kind = if c.is_closed() { "Closed loop" } else { "Open chain" };
            Some(format!("{kind} {idx} ({})", c.layer()))
        });
    }
    let e = model::find_by_name(world, entity)?;
    let table = feature_names_of(world, &e)?;
    let kind = sel.name_kind()?;
    table.get(kind, sel.id()).map(str::to_owned)
}

/// One item's friendly name on `entity`, falling back to its plain kind noun
/// (`"Face"`, `"Curved face"`, `"Edge"`, `"Vertex"`, `"Sketch chain"`, `"Layer '...'""`) when stale or missing.
#[must_use]
pub fn feature_name_or_kind(world: &World, entity: &str, sel: &FeatureSel) -> String {
    feature_name(world, entity, sel).unwrap_or_else(|| match sel {
        FeatureSel::SketchChain(_) => "Sketch chain".to_owned(),
        FeatureSel::SketchLayer(layer) => format!("Layer '{layer}'"),
        _ => sel
            .name_kind()
            .map_or("Feature", FeatureNames::fallback)
            .to_owned(),
    })
}

/// Format using an already-fetched entity table (used when iterating many features
/// of the same entity, e.g. `list_features`).
#[must_use]
pub fn label_in_table(table: Option<&FeatureNames>, entity: &str, sel: &FeatureSel) -> String {
    let base = match sel.name_kind() {
        Some(kind) => table
            .and_then(|t| t.get(kind, sel.id()))
            .unwrap_or_else(|| FeatureNames::fallback(kind))
            .to_owned(),
        None => match sel {
            FeatureSel::SketchLayer(layer) => format!("Layer '{layer}'"),
            _ => "Sketch chain".to_owned(),
        },
    };
    if entity.is_empty() {
        base
    } else {
        format!("{base} of '{entity}'")
    }
}

/// Shared user-friendly display label for `g` (e.g. `"Face 12 of 'fig8'"` or
/// `"Bore 1 of 'part'"`, falling back to `"Face of 'part'"` if stale/deleted).
#[must_use]
pub fn geo_label(world: &World, g: &GeoSel) -> String {
    let base = feature_name_or_kind(world, &g.entity, &g.feature);
    if g.entity.is_empty() {
        base
    } else {
        format!("{base} of '{}'", g.entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, ImportModel, SetStock};
    use crate::geometry::{Point3, cube, cylinder};
    use crate::schema::Location;
    use crate::stock::StockShape;
    use crate::world::new_world;

    #[test]
    fn feature_names_cache_once_per_revision_for_model_and_stock() {
        let world = new_world();
        ImportModel::from_mesh("box", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))).apply(&world);
        ImportModel::from_mesh("cyl", cylinder(20.0, 10.0, 16)).apply(&world);
        SetStock::new(&world, StockShape::Box { dx: 20.0, dy: 20.0, dz: 10.0 }, Location::default())
            .expect("stock")
            .apply(&world);

        let box_e = model::find_by_name(&world, "box").expect("box");
        let cyl_e = model::find_by_name(&world, "cyl").expect("cyl");
        let stock_e = model::find_by_name(&world, crate::schema::STOCK_NAME).expect("stock");

        let b0 = name_builds();
        let t_box = feature_names_of(&world, &box_e).expect("box names");
        let b1 = name_builds();
        assert_eq!(b1 - b0, 1, "first read builds once");
        let t_box_again = feature_names_of(&world, &box_e).expect("cached box names");
        assert_eq!(name_builds(), b1, "repeated reads hit cache without rebuilding");
        assert_eq!(t_box, t_box_again);

        let cyl_f = model::features_of(&world, &cyl_e).expect("cyl features");
        let cyl_patch = crate::geometry::feature::patches(&cyl_f).into_iter().next().expect("patch");
        let cyl_label = geo_label(&world, &GeoSel { entity: "cyl".into(), feature: FeatureSel::Patch(cyl_patch.id) });
        assert_eq!(cyl_label, "Cylinder 1 of 'cyl'");

        let stock_f = model::features_of(&world, &stock_e).expect("stock features");
        let stock_label = geo_label(&world, &GeoSel { entity: "Stock".into(), feature: FeatureSel::Face(stock_f.faces[0].id) });
        assert!(stock_label.starts_with("Face ") && stock_label.ends_with(" of 'Stock'"), "{stock_label}");

        // Landing updated geometry invalidates memo and rebuilds once.
        let builds_before_update = name_builds();
        model::set_facets(&world, &box_e, cube(Point3::ZERO, Point3::new(30.0, 10.0, 5.0)).facets);
        let _t_box_updated = feature_names_of(&world, &box_e).expect("updated box names");
        assert_eq!(name_builds() - builds_before_update, 1, "mesh change rebuilds once");
        let _t_box_updated2 = feature_names_of(&world, &box_e).expect("cached updated names");
        assert_eq!(name_builds() - builds_before_update, 1, "subsequent read hits updated cache");
    }
}

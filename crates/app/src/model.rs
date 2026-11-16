//! Entity helpers over the schema components. The only place outside
//! commands/systems that touches flecs gets/sets, so the UI layer can stay
//! inside its layering rules (ui -> app -> flecs).
//!
//! `Entity` in flecs_ecs is a bare id; every verb goes through an
//! `EntityView` borrowed from the world, which is why each helper takes
//! `world` explicitly.

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{Aabb, IDENTITY_Q, Point3, Triangle, normalize_quat, transform_point};
use crate::schema::{
    Facets, Location, Name, Operation, OperationMeta, Selected, StockDef, ToolDef, Toolpath, Wcs,
};
use crate::stock::StockShape;

mod geometry;

pub use geometry::{features_of, features_of_local, geometry_signature};
use crate::tools::Tool;

/// Create a named model entity at the origin with no geometry yet.
#[must_use]
pub fn spawn(world: &World, name: &str) -> Entity {
    *world
        .entity()
        .set(Name(name.to_string()))
        .set(Location::default())
}

/// Land geometry on an entity - the ONLY writer of `Facets` (M5.3 A).
/// Every landing moves the revision counter in the same step: the
/// geometry signature memo keys by `(rev)`, so forgetting to move it
/// is not a thing a call site can do. Commands that restore a past
/// revision pin it right back via [`set_rev`], paired with the exact
/// bytes that revision carried.
pub fn set_facets(world: &World, entity: &Entity, tris: std::sync::Arc<[Triangle]>) {
    let view = entity.entity_view(world);
    view.set(Facets { tris });
    let next = rev_of(world, entity).saturating_add(1);
    view.set(crate::schema::Revision(next));
}

/// Set the translation half of the placement, keeping any existing rotation.
pub fn set_location(world: &World, entity: &Entity, p: Point3) {
    let rot = entity
        .entity_view(world)
        .try_get::<&Location>(|l| l.rot)
        .unwrap_or(IDENTITY_Q);
    entity.entity_view(world).set(Location { pos: p, rot });
}

/// Set the full rigid placement. Normalizes the quaternion at this single
/// write chokepoint: a zero or non-finite `rot` (a hand-edited file, a bad
/// parameter) becomes identity with a logged diagnostic, never NaN geometry
/// (Prime Directive).
pub fn set_transform(world: &World, entity: &Entity, mut loc: Location) {
    match normalize_quat(loc.rot) {
        Some(q) => loc.rot = q,
        None => {
            tracing::warn!(
                "model: rotation {:?} is not a usable quaternion; using identity",
                loc.rot
            );
            loc.rot = IDENTITY_Q;
        }
    }
    entity.entity_view(world).set(loc);
}

/// Delete an entity; Rust-owned components free via flecs's drop hooks.
pub fn remove(world: &World, entity: &Entity) {
    entity.entity_view(world).destruct();
}

/// The shared read: an `Arc` bump, never a copy - a 10M-triangle model
/// read per frame costs a refcount (M5.3 E). Deref it to `&[Triangle]`
/// at any kernel seam.
#[must_use]
pub fn facets_of(world: &World, entity: &Entity) -> Option<std::sync::Arc<[Triangle]>> {
    entity.entity_view(world).try_get::<&Facets>(|f| f.tris.clone())
}

/// Translation only - the common read for position-oriented code. Use
/// [`transform_of`] when orientation matters (render, pick, serialization).
#[must_use]
pub fn location_of(world: &World, entity: &Entity) -> Option<Point3> {
    entity.entity_view(world).try_get::<&Location>(|l| l.pos)
}

#[must_use]
pub fn transform_of(world: &World, entity: &Entity) -> Option<Location> {
    entity.entity_view(world).try_get::<&Location>(|l| *l)
}

/// Resolve an entity by its `Name` component (commands reference entities by
/// name so they stay invertible across undo/redo entity re-creation).
#[must_use]
pub fn find_by_name(world: &World, name: &str) -> Option<Entity> {
    let mut found = None;
    world.each_entity::<&Name>(|e, n| {
        if n.0 == name {
            found = Some(*e);
        }
    });
    found
}

/// Optional presentation data; old documents and test worlds use defaults.
#[must_use]
pub fn operation_meta_of(world: &World, entity: &Entity) -> OperationMeta {
    entity.entity_view(world).try_get::<&OperationMeta>(Clone::clone).unwrap_or_default()
}

/// The name shown to people, separate from the stable entity key.
#[must_use]
pub fn operation_label(world: &World, key: &str, entity: &Entity) -> String {
    operation_meta_of(world, entity).label.unwrap_or_else(|| key.to_owned())
}

/// All named entities, sorted by name - the serializer's iteration order.
#[must_use]
pub fn named_entities(world: &World) -> Vec<(String, Entity)> {
    let mut out: Vec<(String, Entity)> = Vec::new();
    world.each_entity::<&Name>(|e, n| {
        out.push((n.0.clone(), *e));
    });
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The named entities that are models - the user's parts. Stock and WCS
/// entities are setup furniture and live in the Setup pane; tools and
/// operations are CAM data; so the tree, the GPU part list and "fit to
/// models" must skip all of them.
#[must_use]
pub fn model_entities(world: &World) -> Vec<(String, Entity)> {
    named_entities(world)
        .into_iter()
        .filter(|(_, e)| is_model(world, e))
        .collect()
}

/// Is this named entity one of the user's parts - not stock, not a WCS
/// slot, not a tool, not an operation. The kind test behind [`model_entities`]
/// and the re-import routing in the UI (a name that lands on setup
/// furniture is a conflict, never an overwrite).
#[must_use]
pub fn is_model(world: &World, entity: &Entity) -> bool {
    !is_stock(world, entity)
        && !is_wcs(world, entity)
        && !is_tool(world, entity)
        && !is_operation(world, entity)
        && !is_setup(world, entity)
        && !crate::program_entry::is_entry(world, entity) // #155
}

/// The model's geometry revision counter (M3.1): `0` for an entity that
/// never had the component (a spawned-empty model, a pre-v20 project file).
#[must_use]
pub fn rev_of(world: &World, entity: &Entity) -> u64 {
    entity
        .entity_view(world)
        .try_get::<&crate::schema::Revision>(|r| r.0)
        .unwrap_or(0)
}

/// Pin a geometry revision counter (commands only, per AGENTS.md: the UI
/// and systems never write fields ad hoc). A pin REPLACES the landing
/// counter, so it must land together with the exact facets that value
/// carried when it was current - undo restores both sides, the loader
/// replays what the file remembered. A bare pin with foreign geometry
/// would hand the signature memo a reused key; the pairing is the law.
pub fn set_rev(world: &World, entity: &Entity, rev: u64) {
    entity.entity_view(world).set(crate::schema::Revision(rev));
}

#[must_use]
pub fn is_stock(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&StockDef>(|_| ()).is_some()
}

#[must_use]
pub fn is_wcs(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).has(Wcs)
}

/// The stock parameters of this entity, if it is the stock solid.
#[must_use]
pub fn stock_def_of(world: &World, entity: &Entity) -> Option<StockShape> {
    entity.entity_view(world).try_get::<&StockDef>(|s| s.shape.clone())
}

#[must_use]
pub fn is_tool(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&ToolDef>(|_| ()).is_some()
}

#[must_use]
pub fn is_operation(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&Operation>(|_| ()).is_some()
}

/// A setup entity (M3.2 stage A): the third kind of CAM data - setup
/// furniture, never a model, never a browser geometry row.
#[must_use]
pub fn is_setup(world: &World, entity: &Entity) -> bool {
    entity
        .entity_view(world)
        .try_get::<&crate::schema::SetupDef>(|_| ())
        .is_some()
}

/// Is this entity a FIXTURE (M3.2 stage B)? The tag rides a model - a
/// fixture has no kind of its own, so this is a marker test, not a
/// `is_*` kind: `is_setup` and friends decide what an entity IS, this
/// says what a model additionally means to the collision checker.
#[must_use]
pub fn is_fixture(world: &World, entity: &Entity) -> bool {
    entity
        .entity_view(world)
        .try_get::<&crate::schema::FixtureDef>(|_| ())
        .is_some()
}

/// The tool definition, if this entity is a tool.
#[must_use]
pub fn tool_of(world: &World, entity: &Entity) -> Option<Tool> {
    entity.entity_view(world).try_get::<&ToolDef>(|t| t.tool.clone())
}

/// The tool's own controller number (#221), if one was set. `None` both
/// for an unnumbered tool and for a non-tool; `tool_numbers` decides what
/// an unnumbered tool posts as.
#[must_use]
pub fn tool_number_of(world: &World, entity: &Entity) -> Option<u32> {
    entity
        .entity_view(world)
        .try_get::<&ToolDef>(|t| t.number)
        .flatten()
}

/// The operation definition, if this entity is an operation.
#[must_use]
pub fn operation_of(world: &World, entity: &Entity) -> Option<Operation> {
    entity.entity_view(world).try_get::<&Operation>(|o| o.clone())
}

/// The entity's name, if it wears one. The cheap inverse of
/// `find_by_name`, for code holding an entity that needs to speak of
/// it the way commands do.
#[must_use]
pub fn name_of(world: &World, entity: &Entity) -> Option<String> {
    entity.entity_view(world).try_get::<&Name>(|n| n.0.clone())
}

/// The cached path and its warnings, if this operation has been generated.
#[must_use]
pub fn toolpath_of(world: &World, entity: &Entity) -> Option<Toolpath> {
    entity.entity_view(world).try_get::<&Toolpath>(|t| t.clone())
}

/// Is there a cached path here at all - [`toolpath_of`]'s cheap cousin
/// for enable-gates that must not clone every path per frame.
#[must_use]
pub fn has_toolpath(world: &World, entity: &Entity) -> bool {
    entity
        .entity_view(world)
        .try_get::<&Toolpath>(|_| ())
        .is_some()
}

/// Facets with the entity's rigid transform applied - the frame the
/// slicers, regions and generators work in.
#[must_use]
pub fn world_facets(world: &World, entity: &Entity) -> Option<Vec<Triangle>> {
    let tris = facets_of(world, entity)?;
    let loc = transform_of(world, entity).unwrap_or_default();
    Some(
        tris.iter()
            .map(|t| Triangle::new(
                transform_point(t.a, loc.pos, loc.rot),
                transform_point(t.b, loc.pos, loc.rot),
                transform_point(t.c, loc.pos, loc.rot),
            ))
            .collect(),
    )
}

/// World-space bounds of one entity's geometry, `None` if it has none.
#[must_use]
pub fn entity_world_aabb(world: &World, entity: &Entity) -> Option<Aabb> {
    let tris = world_facets(world, entity)?;
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut any = false;
    for t in tris {
        for p in [t.a, t.b, t.c] {
            for axis in 0..3 {
                let v = [p.x, p.y, p.z][axis];
                if v.is_finite() {
                    min[axis] = min[axis].min(v);
                    max[axis] = max[axis].max(v);
                    any = true;
                }
            }
        }
    }
    any.then(|| Aabb {
        min: Point3::new(min[0], min[1], min[2]),
        max: Point3::new(max[0], max[1], max[2]),
    })
}

/// The models' largest horizontal distance from the vertical line through
/// `c` (world XY) - for a turned part standing on its axis there, its
/// revolved max radius (#230's snug cylinder fit). Stock and WCS are
/// excluded as in [`solid_world_aabb`]; `None` with no model geometry.
#[must_use]
pub fn solid_world_radius_about(world: &World, c: [f64; 2]) -> Option<f64> {
    let mut r_max: Option<f64> = None;
    for (_, entity) in model_entities(world) {
        let Some(tris) = facets_of(world, &entity) else {
            continue;
        };
        let loc = transform_of(world, &entity).unwrap_or_default();
        for t in tris.iter() {
            for p in [t.a, t.b, t.c] {
                let p = transform_point(p, loc.pos, loc.rot);
                let r = (p.x - c[0]).hypot(p.y - c[1]);
                if r.is_finite() {
                    r_max = Some(r_max.map_or(r, |m| m.max(r)));
                }
            }
        }
    }
    r_max
}

/// World-space bounding box of all model geometry. Stock and WCS are excluded
/// on purpose: fitting stock to a set that contains the stock would grow
/// forever on repeated Fit.
#[must_use]
pub fn solid_world_aabb(world: &World) -> Option<Aabb> {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut any = false;
    for (_, entity) in model_entities(world) {
        let Some(tris) = facets_of(world, &entity) else {
            continue;
        };
        let loc = transform_of(world, &entity).unwrap_or_default();
        for t in tris.iter() {
            for p in [t.a, t.b, t.c] {
                let p = transform_point(p, loc.pos, loc.rot);
                for axis in 0..3 {
                    let v = [p.x, p.y, p.z][axis];
                    if v.is_finite() {
                        min[axis] = min[axis].min(v);
                        max[axis] = max[axis].max(v);
                        any = true;
                    }
                }
            }
        }
    }
    any.then(|| Aabb {
        min: Point3::new(min[0], min[1], min[2]),
        max: Point3::new(max[0], max[1], max[2]),
    })
}

/// World-space bounding box of the Stock solid, if any. The WCS origin
/// anchor grid places on this box when stock exists, else on the model
/// bounds - the "pick a point on the model or the stock box" rule.
/// The default setup's stock; a caller standing in another setup (an
/// operation's height resolution, the active Setup page) asks for it
/// explicitly through [`stock_world_aabb_for`] (M3.2 stage A).
#[must_use]
pub fn stock_world_aabb(world: &World) -> Option<Aabb> {
    stock_world_aabb_for(world, None)
}

/// The same box for one declared setup context ([`crate::setup`]'s
/// implicit-setup law makes `None` byte-identical to the function above).
#[must_use]
pub fn stock_world_aabb_for(world: &World, setup: Option<&str>) -> Option<Aabb> {
    let name = crate::setup::stock_entity_name(world, setup);
    let entity = find_by_name(world, &name)?;
    if !is_stock(world, &entity) {
        return None;
    }
    let tris = facets_of(world, &entity)?;
    let loc = transform_of(world, &entity).unwrap_or_default();
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut any = false;
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            for axis in 0..3 {
                let v = [p.x, p.y, p.z][axis];
                if v.is_finite() {
                    min[axis] = min[axis].min(v);
                    max[axis] = max[axis].max(v);
                    any = true;
                }
            }
        }
    }
    any.then(|| Aabb {
        min: Point3::new(min[0], min[1], min[2]),
        max: Point3::new(max[0], max[1], max[2]),
    })
}

/// One of the 27 box-anchor points: `index` on each axis picks the low
/// corner (0), the center (1), or the high corner (2) of `b`. Used by the
/// Setup pane's origin-placement grid. Centers are exact averages of
/// finite bounds, so a symmetric box anchors its faces at x=y=0.
#[must_use]
pub fn anchor_point(b: &Aabb, index: [usize; 3]) -> [f64; 3] {
    let pick = |lo: f64, hi: f64, i: usize| match i {
        0 => lo,
        2 => hi,
        _ => (lo + hi) * 0.5,
    };
    [
        pick(b.min.x, b.max.x, index[0]),
        pick(b.min.y, b.max.y, index[1]),
        pick(b.min.z, b.max.z, index[2]),
    ]
}

/// Names of all currently-selected entities.
#[must_use]
pub fn selected_names(world: &World) -> Vec<String> {
    named_entities(world)
        .into_iter()
        .filter(|(_, e)| e.entity_view(world).has(Selected))
        .map(|(name, _)| name)
        .collect()
}

pub fn is_selected(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).has(Selected)
}

/// Destruct every named entity (new/open document). Rust-owned components
/// free through flecs's drop hooks.
pub fn clear_named(world: &World) {
    for (_, entity) in named_entities(world) {
        remove(world, &entity);
    }
}

#[cfg(test)]
mod tests {
    use super::anchor_point;
    use crate::geometry::{Aabb, Point3};

    #[test]
    fn anchor_points_are_corners_centers_and_face_mid() {
        let b = Aabb {
            min: Point3::new(-30.0, -20.0, 0.0),
            max: Point3::new(30.0, 20.0, 10.0),
        };
        assert_eq!(anchor_point(&b, [0, 0, 0]), [-30.0, -20.0, 0.0]);
        assert_eq!(anchor_point(&b, [2, 2, 2]), [30.0, 20.0, 10.0]);
        assert_eq!(anchor_point(&b, [1, 1, 2]), [0.0, 0.0, 10.0]); // top face center
        assert_eq!(anchor_point(&b, [2, 1, 0]), [30.0, 0.0, 0.0]); // +X bottom edge mid
        assert_eq!(anchor_point(&b, [0, 2, 1]), [-30.0, 20.0, 5.0]);
        // Grid indices come from UI positions; anything out of range lands on
        // the center rather than misbehaving.
        assert_eq!(anchor_point(&b, [5, 5, 5]), [0.0, 0.0, 5.0]);
    }
}

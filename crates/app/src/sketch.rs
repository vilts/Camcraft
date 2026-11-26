//! DXF profiles in the document (#1862): the `Sketch` entities, which
//! setup each belongs to, and where each lies on its setup's stock.
//!
//! Provenance: our own design, from the maintainer's words on issue 1862
//! ("A DXF should be possible to place on a stock and used as references
//! for cutting - not be treated like a model"). The transform algebra is
//! standard rigid-body math. No vendor material consulted.
//!
//! ## The laws here
//!
//! - **A profile is not a model.** It has no facets, `model::is_model` is
//!   false for it, and the part readers (`model::model_entities`) never
//!   list it.
//! - **A profile belongs to one setup** and lies on the top face of that
//!   setup's stock. Its `Location` is relative to that face, at the stock
//!   origin ([`face_frame`]): Z is 0 and the turn is about the face
//!   normal only ([`on_face`], [`face_placement`]). The stock is setup
//!   furniture, so the profile rides the stock when the stock moves, and
//!   a setup's part alignment does not move it.
//! - **No stock, no place.** A profile whose setup has no stock has no
//!   world placement ([`world_pose`] is `None`); nothing guesses one.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{IDENTITY_Q, Point3};
use crate::model;
use crate::schema::{Location, Sketch};
use crate::setup_align::compose;
use crate::stock::StockShape;

/// The largest X or Y a profile can be moved to on its face, mm.
pub const MAX_OFFSET: f64 = 1.0e6;

#[must_use]
pub fn is_sketch(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&Sketch>(|_| ()).is_some()
}

#[must_use]
pub fn sketch_of(world: &World, entity: &Entity) -> Option<Sketch> {
    entity.entity_view(world).try_get::<&Sketch>(Clone::clone)
}

/// The profile with this name, when the name wears one.
#[must_use]
pub fn find(world: &World, name: &str) -> Option<(Entity, Sketch)> {
    let entity = model::find_by_name(world, name)?;
    sketch_of(world, &entity).map(|sketch| (entity, sketch))
}

/// Every profile of the document, name-sorted.
#[must_use]
pub fn sketches(world: &World) -> Vec<(String, Entity)> {
    model::named_entities(world).into_iter().filter(|(_, e)| is_sketch(world, e)).collect()
}

/// The profiles placed in a declared setup context, name-sorted. `None`
/// and the first setup mean the same thing (the implicit-setup law).
#[must_use]
pub fn placed_in(world: &World, declared: Option<&str>) -> Vec<String> {
    let target = crate::setup::effective(world, declared);
    sketches(world)
        .into_iter()
        .filter(|(_, e)| {
            sketch_of(world, e).is_some_and(|s| crate::setup::effective(world, s.setup.as_deref()) == target)
        })
        .map(|(name, _)| name)
        .collect()
}

/// The top of a stock in its own frame: the box height, the cylinder
/// height, or the highest finite facet Z of a stock copied from a model.
fn stock_top(world: &World, stock: &Entity, shape: &StockShape) -> Option<f64> {
    match shape {
        StockShape::Box { dz, .. } => dz.is_finite().then_some(*dz),
        StockShape::Cylinder { height, .. } => height.is_finite().then_some(*height),
        StockShape::FromModel { .. } => model::facets_of(world, stock)?
            .iter()
            .flat_map(|t| [t.a.z, t.b.z, t.c.z])
            .filter(|z| z.is_finite())
            .reduce(f64::max),
    }
}

/// #2576: the refusal for a stock with no flat face on top.
pub const NO_TOP_FACE: &str = "the stock has no flat top face - a cylinder that lies on its side or a tilted stock takes no profile; \
     stand the stock upright or turn it square to the axes";

/// The frame profiles of a setup are placed in: the top face of the
/// setup's stock, at the stock origin, turned with the stock. `None`
/// when the setup has no stock, or when the stock has no flat top face
/// ([`stock_face`] names that).
#[must_use]
pub fn face_frame(world: &World, declared: Option<&str>) -> Option<Location> {
    stock_face(world, declared).ok().flatten()
}

/// [`face_frame`] with its refusal: `Ok(None)` when the setup has no
/// stock, `Err(NO_TOP_FACE)` when its stock has no flat face on top
/// (#2576). An upright stock (any turn about Z) keeps its own top face.
/// A stock laid square (a quarter turn) is read as the same solid
/// unturned, by the simulation's law (`stock_follow::sim_frame`), so the
/// profile lies on the face that is on top in the setup.
///
/// # Errors
/// [`NO_TOP_FACE`] for a laid cylinder or a stock tilted off the axes.
pub fn stock_face(world: &World, declared: Option<&str>) -> Result<Option<Location>, &'static str> {
    let Some(stock) = crate::setup::stock_entity(world, declared) else {
        return Ok(None);
    };
    let Some(shape) = model::stock_def_of(world, &stock) else {
        return Ok(None);
    };
    let at = model::transform_of(world, &stock).unwrap_or_default();
    let up = crate::geometry::quat_rotate_point(at.rot, Point3::new(0.0, 0.0, 1.0));
    let (shape, at) = if matches!(shape, StockShape::FromModel { .. }) || up.z >= 1.0 - 1e-9 {
        (shape, at)
    } else {
        // Since #2758 the simulation keeps a laid cylinder's turn; its ends
        // stand vertical, so it still has no flat face on top.
        let (shape, at) = crate::stock_follow::sim_frame(&shape, at).map_err(|_| NO_TOP_FACE)?;
        if crate::geometry::quat_rotate_point(at.rot, Point3::new(0.0, 0.0, 1.0)).z < 1.0 - 1e-9 {
            return Err(NO_TOP_FACE);
        }
        (shape, at)
    };
    Ok(stock_top(world, &stock, &shape).map(|top| compose(at, Location { pos: Point3::new(0.0, 0.0, top), rot: IDENTITY_Q })))
}

/// Where a profile lies in its setup's frame: its face placement on the
/// stock's top face. `None` for an entity that is no profile, and for a
/// profile whose setup has no stock.
#[must_use]
pub fn world_pose(world: &World, entity: &Entity) -> Option<Location> {
    let sketch = sketch_of(world, entity)?;
    let face = face_frame(world, sketch.setup.as_deref())?;
    Some(compose(face, model::transform_of(world, entity).unwrap_or_default()))
}

/// A face placement: `x`, `y` on the face in mm and a turn of
/// `angle_deg` about the face normal, counterclockwise seen from above.
#[must_use]
pub fn on_face(x: f64, y: f64, angle_deg: f64) -> Location {
    let half = angle_deg.to_radians() / 2.0;
    Location { pos: Point3::new(x, y, 0.0), rot: [0.0, 0.0, half.sin(), half.cos()] }
}

/// A `Location` read as a face placement (`x`, `y`, angle in degrees,
/// -180 to 180); `None` when it leaves the face (a Z offset or a tilt),
/// is not finite or is out of range. The project loader's test.
#[must_use]
pub fn face_placement(at: Location) -> Option<(f64, f64, f64)> {
    let [qx, qy, qz, qw] = at.rot;
    let finite = at.pos.is_finite() && at.rot.iter().all(|v| v.is_finite());
    let unit = ((qx * qx + qy * qy + qz * qz + qw * qw).sqrt() - 1.0).abs() <= 1e-9;
    let flat = at.pos.z.abs() <= 1e-9 && qx.abs() <= 1e-9 && qy.abs() <= 1e-9;
    let near = at.pos.x.abs() <= MAX_OFFSET && at.pos.y.abs() <= MAX_OFFSET;
    // The angle is read back to a billionth of a degree, so a turn of 90
    // reads 90 and a move that keeps the angle writes the same rotation.
    let angle = ((2.0 * qz.atan2(qw)).to_degrees() * 1e9).round() / 1e9;
    (finite && unit && flat && near).then_some((at.pos.x, at.pos.y, angle))
}

/// A free entity name for a profile: `stem`, else `stem (2)`, `stem (3)`
/// and so on. `None` when 10,000 candidates are all taken.
#[must_use]
pub fn free_name(world: &World, stem: &str) -> Option<String> {
    std::iter::once(stem.to_owned())
        .chain((2..10_000).map(|n| format!("{stem} ({n})")))
        .find(|name| model::find_by_name(world, name).is_none())
}

//! Part alignment per setup (#326): how the part sits on the machine for
//! one setup - which face is up, which edge runs along X - as a rigid
//! transform from the document frame into that setup's frame.
//!
//! Provenance: our own design; re-clamping a workpiece face-down or
//! turned on the table between operations is plain shop practice, and
//! the transform algebra is standard rigid-body math (unit quaternions,
//! R = Rz * Ry * Rx). No vendor implementation inspected.
//!
//! ## The laws here
//!
//! - **Two frames, kept apart.** The setup frame is where the part sits
//!   on the table; the work offset (`G54`..`G59`, [`crate::setup::wcs_slot`])
//!   is a plain translation placed in that frame. This module never
//!   touches the offset, and a rotated offset stays refused at the post.
//! - **Only parts move.** The stock, the work offsets and the fixtures
//!   of a setup are setup furniture: they are placed IN the setup frame
//!   already, so they read their own `Location` unchanged. A part (a
//!   model that is not a fixture) in an aligned setup reads
//!   `alignment * Location`.
//! - **Identity is absence.** `None` is "as modelled": every read below
//!   then calls the very function it called before #326, so an untouched
//!   setup is byte-identical (paths, stamps, posts and saved files).
//! - **Refuse what is not verified.** Generation in an aligned setup is
//!   wired for the 2D milling kinds over a whole model or the stock;
//!   every other kind, a pick, area controls, a tilt and a non-mill
//!   machine are refused by name ([`generation_problem`]) - never cut in
//!   the unaligned frame.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{Aabb, Point3, Triangle, quat_conjugate, quat_mul, quat_rotate_point, transform_point};
use crate::model;
use crate::schema::{Location, SetupDef};

/// The alignment STORED on a named setup (`None` = as modelled, or the
/// name wears no setup).
#[must_use]
pub fn stored(world: &World, setup: &str) -> Option<Location> {
    model::find_by_name(world, setup)
        .and_then(|e| e.entity_view(world).try_get::<&SetupDef>(|d| d.part_alignment))
        .flatten()
}

/// The alignment of a declared setup context: the effective setup's
/// stored transform. A document without setups has none.
#[must_use]
pub fn alignment(world: &World, declared: Option<&str>) -> Option<Location> {
    crate::setup::effective(world, declared).and_then(|s| stored(world, &s))
}

/// Does any setup of the document hold the part aligned?
#[must_use]
pub fn any_aligned(world: &World) -> bool {
    crate::setup::names(world).iter().any(|s| stored(world, s).is_some())
}

/// A part: a model that is not workholding. The only entities an
/// alignment moves.
#[must_use]
pub fn is_part(world: &World, entity: &Entity) -> bool {
    model::is_model(world, entity) && !model::is_fixture(world, entity)
}

/// `a` applied after `l`: the pose of something placed at `l`, seen
/// through the rigid transform `a`.
#[must_use]
pub fn compose(a: Location, l: Location) -> Location {
    Location { pos: transform_point(l.pos, a.pos, a.rot), rot: quat_mul(a.rot, l.rot) }
}

/// The inverse rigid transform (unit rotation assumed - every stored
/// alignment passed `check_location`).
#[must_use]
pub fn inverse(a: Location) -> Location {
    let rot = quat_conjugate(a.rot);
    let p = quat_rotate_point(rot, a.pos);
    Location { pos: Point3::new(-p.x, -p.y, -p.z), rot }
}

/// An entity's pose in a setup's frame: a part reads the setup's
/// alignment composed onto its document `Location`; everything else -
/// and every part of an unaligned setup - reads `transform_of` as is.
#[must_use]
pub fn pose_in(world: &World, entity: &Entity, declared: Option<&str>) -> Option<Location> {
    let loc = model::transform_of(world, entity)?;
    match alignment(world, declared) {
        Some(a) if is_part(world, entity) => Some(compose(a, loc)),
        _ => Some(loc),
    }
}

/// The pose the viewport draws and picks in: the ACTIVE setup's frame.
#[must_use]
pub fn view_pose(world: &World, entity: &Entity) -> Option<Location> {
    pose_in(world, entity, crate::setup::active(world).as_deref())
}

/// An entity's facets in a setup's frame ([`model::world_facets`] when
/// nothing is aligned).
#[must_use]
pub fn facets_in(world: &World, entity: &Entity, declared: Option<&str>) -> Option<Vec<Triangle>> {
    let Some(a) = alignment(world, declared).filter(|_| is_part(world, entity)) else {
        return model::world_facets(world, entity);
    };
    let tris = model::facets_of(world, entity)?;
    let loc = compose(a, model::transform_of(world, entity).unwrap_or_default());
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

fn bounds(points: impl Iterator<Item = Point3>) -> Option<Aabb> {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut any = false;
    for p in points {
        for (axis, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            if v.is_finite() {
                min[axis] = min[axis].min(v);
                max[axis] = max[axis].max(v);
                any = true;
            }
        }
    }
    any.then(|| Aabb {
        min: Point3::new(min[0], min[1], min[2]),
        max: Point3::new(max[0], max[1], max[2]),
    })
}

/// One entity's bounds in a setup's frame.
#[must_use]
pub fn entity_aabb_in(world: &World, entity: &Entity, declared: Option<&str>) -> Option<Aabb> {
    if alignment(world, declared).is_none() {
        return model::entity_world_aabb(world, entity);
    }
    let tris = facets_in(world, entity, declared)?;
    bounds(tris.iter().flat_map(|t| [t.a, t.b, t.c]))
}

/// [`model::solid_world_aabb`] in a setup's frame: the same entity set
/// (every model), the parts aligned.
#[must_use]
pub fn solid_aabb_in(world: &World, declared: Option<&str>) -> Option<Aabb> {
    if alignment(world, declared).is_none() {
        return model::solid_world_aabb(world);
    }
    let tris: Vec<Triangle> = model::model_entities(world)
        .into_iter()
        .filter_map(|(_, e)| facets_in(world, &e, declared))
        .flatten()
        .collect();
    bounds(tris.iter().flat_map(|t| [t.a, t.b, t.c]))
}

/// [`model::solid_world_radius_about`] in a setup's frame.
#[must_use]
pub fn solid_radius_about_in(world: &World, declared: Option<&str>, c: [f64; 2]) -> Option<f64> {
    if alignment(world, declared).is_none() {
        return model::solid_world_radius_about(world, c);
    }
    model::model_entities(world)
        .into_iter()
        .filter_map(|(_, e)| facets_in(world, &e, declared))
        .flatten()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| (p.x - c[0]).hypot(p.y - c[1]))
        .filter(|r| r.is_finite())
        .reduce(f64::max)
}

/// The parts (fixtures left out) as the machine must leave them, in a
/// setup's frame - the program sim's gouge target. A model that is only
/// the stock's source is stock, not part (#1094, `stock_source`).
#[must_use]
pub fn parts_facets_in(world: &World, declared: Option<&str>) -> Vec<Triangle> {
    let skip = crate::stock_source::stock_only_model(world);
    model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| crate::stock_source::judged(world, e, skip))
        .filter_map(|(_, e)| facets_in(world, &e, declared))
        .flatten()
        .collect()
}

/// The quick choices the Setups page offers. Absolute: each one REPLACES
/// the setup's alignment (they do not stack), turning the part about the
/// centre of its bounds so it stays where it was on the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignPreset {
    AsModelled,
    FlipX,
    FlipY,
    TurnZ90,
    TurnZ180,
    TurnZ270,
}

impl AlignPreset {
    pub const ALL: [AlignPreset; 6] = [
        AlignPreset::AsModelled,
        AlignPreset::FlipX,
        AlignPreset::FlipY,
        AlignPreset::TurnZ90,
        AlignPreset::TurnZ180,
        AlignPreset::TurnZ270,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AlignPreset::AsModelled => "As modelled",
            AlignPreset::FlipX => "Flip over X",
            AlignPreset::FlipY => "Flip over Y",
            AlignPreset::TurnZ90 => "Turn 90° about Z",
            AlignPreset::TurnZ180 => "Turn 180° about Z",
            AlignPreset::TurnZ270 => "Turn 270° about Z",
        }
    }

    /// The preset as (about X, about Y, about Z) degrees.
    #[must_use]
    pub fn angles(self) -> [f64; 3] {
        match self {
            AlignPreset::AsModelled => [0.0, 0.0, 0.0],
            AlignPreset::FlipX => [180.0, 0.0, 0.0],
            AlignPreset::FlipY => [0.0, 180.0, 0.0],
            AlignPreset::TurnZ90 => [0.0, 0.0, 90.0],
            AlignPreset::TurnZ180 => [0.0, 0.0, 180.0],
            AlignPreset::TurnZ270 => [0.0, 0.0, 270.0],
        }
    }
}

/// A rotation about one axis (0 = X, 1 = Y, 2 = Z) by `deg` degrees,
/// exact for the quarter turns (a preset flip moves vertices by exact
/// sign changes, not by a 1e-16 cosine). `None` for a non-finite angle.
#[must_use]
pub fn axis_quat(axis: usize, deg: f64) -> Option<[f64; 4]> {
    if !deg.is_finite() || axis > 2 {
        return None;
    }
    let turn = deg.rem_euclid(360.0);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let (s, c) = if turn == 0.0 {
        (0.0, 1.0)
    } else if turn == 90.0 {
        (h, h)
    } else if turn == 180.0 {
        (1.0, 0.0)
    } else if turn == 270.0 {
        (h, -h)
    } else {
        (0.5 * turn.to_radians()).sin_cos()
    };
    let mut q = [0.0, 0.0, 0.0, c];
    q[axis] = s;
    Some(q)
}

/// The rotation for (about X, about Y, about Z) degrees, applied X first,
/// then Y, then Z (R = Rz * Ry * Rx, `quat_from_euler`'s order).
#[must_use]
pub fn rotation_from_angles(deg: [f64; 3]) -> Option<[f64; 4]> {
    let [x, y, z] = [axis_quat(0, deg[0])?, axis_quat(1, deg[1])?, axis_quat(2, deg[2])?];
    Some(quat_mul(z, quat_mul(y, x)))
}

/// Is this rotation the identity (within a micro-radian)?
#[must_use]
pub fn is_identity_rot(q: [f64; 4]) -> bool {
    q[3].abs() >= 1.0 - 1e-12
}

/// The preset a stored rotation is, if any (a quaternion and its negation
/// are one rotation).
#[must_use]
pub fn preset_of(rot: [f64; 4]) -> Option<AlignPreset> {
    AlignPreset::ALL.into_iter().find(|p| {
        // q^-1 * rot is the identity when they are the same turn.
        rotation_from_angles(p.angles()).is_some_and(|q| is_identity_rot(quat_mul(quat_conjugate(q), rot)))
    })
}

/// A stored alignment as (about X, about Y, about Z) degrees, rounded to
/// a millionth so a quarter turn reads 90, not 89.99999999.
#[must_use]
pub fn angles_of(a: Option<Location>) -> [f64; 3] {
    let Some(a) = a else { return [0.0; 3] };
    let (r, p, y) = crate::geometry::quat_to_euler(a.rot);
    [r, p, y].map(|v| {
        let d = (v.to_degrees() * 1e6).round() / 1e6;
        if d == 0.0 { 0.0 } else { d } // no "-0"
    })
}

/// The plain words for a setup's alignment - a preset's name, or the
/// three angles - shared by the Setups page and the setup sheet.
#[must_use]
pub fn describe(a: Option<Location>) -> String {
    let Some(loc) = a else { return AlignPreset::AsModelled.label().to_owned() };
    if let Some(p) = preset_of(loc.rot) {
        return p.label().to_owned();
    }
    let [x, y, z] = angles_of(a);
    format!("turned X {x}°, Y {y}°, Z {z}°")
}

/// The parts' bounds centre in the DOCUMENT frame - the pivot every
/// preset turns about (typed angles pivot on the WCS origin by default,
/// #606, [`crate::setup_turn`]). `None` with no part geometry.
#[must_use]
pub fn part_centre(world: &World) -> Option<Point3> {
    let tris: Vec<Triangle> = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(_, e)| model::world_facets(world, &e))
        .flatten()
        .collect();
    let b = bounds(tris.iter().flat_map(|t| [t.a, t.b, t.c]))?;
    Some(Point3::new(
        0.5 * (b.min.x + b.max.x),
        0.5 * (b.min.y + b.max.y),
        0.5 * (b.min.z + b.max.z),
    ))
}

/// The alignment that turns the parts by `rot` about their bounds centre
/// `c`: p' = R (p - c) + c. The identity rotation is `None` (as modelled).
#[must_use]
pub fn about_centre(rot: [f64; 4], c: Point3) -> Option<Location> {
    if is_identity_rot(rot) {
        return None;
    }
    let rc = quat_rotate_point(rot, c);
    Some(Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot })
}

/// The op kinds whose every world read is wired through this module's
/// setup-frame helpers, and pinned by tests in an aligned setup.
fn verified_kind(params: &crate::operation::OpParams) -> bool {
    use crate::operation::OpParams;
    matches!(
        params,
        OpParams::Face { .. } | OpParams::Pocket { .. } | OpParams::Contour { .. } | OpParams::Adaptive2D { .. }
    )
}

/// Why an operation may not be generated in its setup's aligned frame
/// yet - `None` when the setup is unaligned (nothing changes) or the op
/// is one this slice verified. Read at the door of every generation
/// (`collect_job`), so an unverified op is refused by name and never cut
/// in the unaligned frame.
#[must_use]
pub fn generation_problem(world: &World, op: &crate::schema::Operation) -> Option<String> {
    use crate::operation::SrcRef;
    let setup = crate::setup::effective(world, op.setup.as_deref())?;
    stored(world, &setup)?;
    let head = format!("setup '{setup}' holds the part turned (part alignment)");
    if crate::machine::kind_of_setup(world, Some(&setup)) != crate::machine::Kind::Mill {
        return Some(format!("{head} - an aligned part is not supported yet on a lathe or mill-turn machine"));
    }
    if !verified_kind(&op.params) {
        return Some(format!(
            "{head} - {} is not supported yet in an aligned setup (Face, 2D Contour, 2D Pocket and \
             2D Adaptive are); set the setup's part alignment back to As modelled, or machine it in another setup",
            op.params.kind().info().label
        ));
    }
    if op.params.tilt().is_some() {
        return Some(format!("{head} - a tilted plane is not supported yet in an aligned setup"));
    }
    if matches!(op.src, SrcRef::Selection(_)) {
        return Some(format!("{head} - picked faces and edges are not supported yet in an aligned setup; pick a whole model or the stock"));
    }
    if !op.area.is_default() {
        return Some(format!("{head} - area controls are not supported yet in an aligned setup"));
    }
    None
}

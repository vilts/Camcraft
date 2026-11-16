//! The pivot of a typed part-alignment turn (#606): the Setups page's
//! "Part alignment" angles set the part's orientation (X, then Y, then Z,
//! from as modelled) and turn it about a pivot - the setup's WCS origin
//! by default, or the centre of the parts' bounds.
//!
//! - **About the WCS origin**, the part keeps its relation to the origin:
//!   the point of the part that sits on the origin NOW stays on it, and
//!   the rest of the part turns around it. The origin is the setup's own
//!   WCS (a translation in the setup frame), or the frame's zero when no
//!   WCS is placed - the same reading the post makes.
//! - **About the part centre**, the centre of the parts' bounds stays
//!   where it is (the presets' pivot: a flip keeps its footprint).
//! - **By, not to (#738).** [`turned_by`] turns the part from where it
//!   sits NOW (composed onto the held alignment) - the Setups page's
//!   preset and "Turn by" rows, so they never wipe a picker's move.
//!   [`turned`] keeps the absolute "to these angles from as modelled"
//!   reading (`AlignPartToSetup::turn`, the presets' `angles`).
//!
//! Provenance: our own design (#606, the maintainer's "it shifts the
//! model relative to the origin"); a turn about a fixed point is
//! standard rigid-body algebra (p' = R (p - q) + o). No vendor material
//! consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, quat_rotate_point, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;

/// What a typed turn keeps fixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TurnPivot {
    /// The part's point on the setup's WCS origin stays on it (#606).
    #[default]
    WcsOrigin,
    /// The centre of the parts' bounds stays put (the presets' pivot).
    PartCentre,
}

impl TurnPivot {
    pub const ALL: [TurnPivot; 2] = [TurnPivot::WcsOrigin, TurnPivot::PartCentre];

    /// The plain words, for the Setups page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            TurnPivot::WcsOrigin => "WCS origin",
            TurnPivot::PartCentre => "part centre",
        }
    }
}

/// The setup's WCS origin in its setup frame: the WCS entity's position,
/// or zero when none is placed (what the post measures from).
#[must_use]
pub fn wcs_origin(world: &World, setup: &str) -> Point3 {
    crate::setup::wcs_entity(world, Some(setup))
        .and_then(|e| model::transform_of(world, &e))
        .map_or(Point3::ZERO, |l| l.pos)
}

/// The centre of the parts' bounds as the alignment `held` (document ->
/// setup frame) holds them (#738): what a move from the part's CURRENT
/// pose turns about. The identity reads [`setup_align::part_centre`]
/// itself. `None` with no part geometry.
#[must_use]
pub fn held_centre(world: &World, held: Location) -> Option<Point3> {
    if held == Location::default() {
        return setup_align::part_centre(world);
    }
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    let mut any = false;
    for (_, e) in model::model_entities(world) {
        if model::is_fixture(world, &e) {
            continue;
        }
        for t in model::world_facets(world, &e).unwrap_or_default() {
            for p in [t.a, t.b, t.c] {
                let q = transform_point(p, held.pos, held.rot);
                for (i, v) in [q.x, q.y, q.z].into_iter().enumerate() {
                    if v.is_finite() {
                        lo[i] = lo[i].min(v);
                        hi[i] = hi[i].max(v);
                        any = true;
                    }
                }
            }
        }
    }
    any.then(|| Point3::new(0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), 0.5 * (lo[2] + hi[2])))
}

/// A turn by `rot` from where the part sits NOW (#738): the step about
/// `pivot` - the setup's WCS origin, or the parts' bounds centre as held -
/// composed onto the setup's held alignment. The Setups page's "Start from
/// a preset" and "Turn by" rows queue this, so they never discard what a
/// face, three-point or axis pick did. An identity turn changes nothing
/// (the command then says the setup already holds the part that way).
pub fn turned_by(world: &World, setup: &str, rot: [f64; 4], pivot: TurnPivot) -> Result<Option<Location>, String> {
    let held = setup_align::stored(world, setup).unwrap_or_default();
    if setup_align::is_identity_rot(rot) {
        return Ok(setup_align::stored(world, setup));
    }
    let c = held_centre(world, held).ok_or("there is no part to align - import a model first")?;
    let o = match pivot {
        TurnPivot::WcsOrigin => wcs_origin(world, setup),
        TurnPivot::PartCentre => c,
    };
    let ro = quat_rotate_point(rot, o);
    let step = Location { pos: Point3::new(o.x - ro.x, o.y - ro.y, o.z - ro.z), rot };
    let out = setup_align::compose(step, held);
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the turn is not finite".into());
    }
    Ok(Some(out))
}

/// The alignment that holds the parts turned by `rot` (from as modelled)
/// about `pivot`. All-zero angles are "as modelled" (`None`) whatever the
/// pivot - the words a stored turn reads back as never hide a shift.
pub fn turned(world: &World, setup: &str, rot: [f64; 4], pivot: TurnPivot) -> Result<Option<Location>, String> {
    if setup_align::is_identity_rot(rot) {
        return Ok(None);
    }
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    match pivot {
        TurnPivot::PartCentre => Ok(setup_align::about_centre(rot, c)),
        TurnPivot::WcsOrigin => {
            let o = wcs_origin(world, setup);
            // The document point the current alignment holds on the origin.
            let q = match setup_align::stored(world, setup) {
                Some(a) => {
                    let inv = setup_align::inverse(a);
                    transform_point(o, inv.pos, inv.rot)
                }
                None => o,
            };
            let rq = quat_rotate_point(rot, q);
            let pos = Point3::new(o.x - rq.x, o.y - rq.y, o.z - rq.z);
            if ![pos.x, pos.y, pos.z].iter().all(|v| v.is_finite()) {
                return Err("the turn about the WCS origin is not finite".into());
            }
            Ok(Some(Location { pos, rot }))
        }
    }
}

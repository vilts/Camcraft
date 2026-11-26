//! MoveSketch (#1862): move and turn a DXF profile on the top face of
//! its setup's stock.
//!
//! Provenance: our own command; the pattern is `MoveModel`'s ("pages own
//! their verbs": a profile is placed with this command, and `MoveModel`
//! refuses one by name). No vendor material consulted.
//!
//! The placement is given as X, Y and an angle about the face normal, so
//! a profile cannot leave its face: there is no Z and no tilt to state.
//! Construction snapshots the previous placement for the inverse; `apply`
//! and `invert` are pure writes.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::Location;
use crate::sketch;

pub struct MoveSketch {
    name: String,
    prev: Location,
    next: Location,
}

impl MoveSketch {
    /// Place the profile `name` at (`x`, `y`) mm on its stock face, turned
    /// `angle_deg` counterclockwise seen from above.
    pub fn new(world: &World, name: &str, x: f64, y: f64, angle_deg: f64) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
        if !sketch::is_sketch(world, &entity) {
            return Err(CommandError::Invalid(format!("'{name}' is not a DXF profile")));
        }
        if !(x.is_finite() && y.is_finite() && angle_deg.is_finite()) {
            return Err(CommandError::Invalid(format!("'{name}' placement must be finite")));
        }
        if x.abs() > sketch::MAX_OFFSET || y.abs() > sketch::MAX_OFFSET {
            return Err(CommandError::Invalid(format!(
                "'{name}' placement is more than {} mm from the stock origin",
                sketch::MAX_OFFSET
            )));
        }
        // One turn is every turn: keep the angle in (-180, 180].
        let angle = 180.0 - (180.0 - angle_deg).rem_euclid(360.0);
        Ok(MoveSketch {
            name: name.to_owned(),
            prev: model::transform_of(world, &entity).unwrap_or_default(),
            next: sketch::on_face(x, y, angle),
        })
    }

    fn write(&self, world: &World, at: Location) {
        if let Some((entity, _)) = sketch::find(world, &self.name) {
            model::set_transform(world, &entity, at);
        }
    }
}

impl Command for MoveSketch {
    fn apply(&self, world: &World) {
        self.write(world, self.next);
    }

    fn invert(&self, world: &World) {
        self.write(world, self.prev);
    }
}

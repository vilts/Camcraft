//! SetWcsAtCircleCentre (#739): anchor the active setup's WCS on the
//! centre of a picked circular edge (a hole rim, a boss edge). The circle
//! comes from the shared pick primitive (`pick::circle_centre`); the
//! placement is exactly a `SetWcsOrigin` at that centre, so undo restores
//! the previous origin the same way every other WCS edit does. A pick that
//! is not a circle is refused here, at command creation, by name.
//!
//! Provenance: our own command over this repo's pick and WCS commands;
//! published G-code work-offset semantics (RS-274 / ISO 6983) only.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, SetWcsOrigin};
use crate::geometry::Point3;
use crate::operation::GeoSel;
use crate::pick::{Circle, circle_centre};
use crate::schema::Location;

pub struct SetWcsAtCircleCentre {
    inner: SetWcsOrigin,
    circle: Circle,
}

impl SetWcsAtCircleCentre {
    /// Resolve `pick` to its circle and place the WCS origin at the
    /// centre with orientation `rot` (a quaternion `[x, y, z, w]`).
    pub fn new(world: &World, pick: &GeoSel, rot: [f64; 4]) -> Result<Self, CommandError> {
        let circle = circle_centre(world, pick).map_err(|why| CommandError::Invalid(why.to_string()))?;
        let [x, y, z] = circle.centre;
        let inner = SetWcsOrigin::new(world, Location { pos: Point3::new(x, y, z), rot })?;
        Ok(SetWcsAtCircleCentre { inner, circle })
    }

    /// The circle the origin is placed on.
    #[must_use]
    pub fn circle(&self) -> &Circle {
        &self.circle
    }

    /// The WCS entity this command places.
    #[must_use]
    pub fn entity_name(&self) -> &str {
        self.inner.entity_name()
    }
}

impl Command for SetWcsAtCircleCentre {
    fn apply(&self, world: &World) {
        self.inner.apply(world);
    }

    fn invert(&self, world: &World) {
        self.inner.invert(world);
    }
}

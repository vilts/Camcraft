//! SetWcsOrigin: place the active setup's one work coordinate system
//! (#342) - its origin and orientation.
//!
//! A setup has exactly one WCS; which `G54`..`G59` word the post writes
//! for it is the setup's label (`SetSetupWorkOffset`), never chosen here.
//! The `Wcs` tag distinguishes the entity from a model that happens to
//! share the name. Provenance: published G-code standards (RS-274 / ISO
//! 6983 work-offset semantics) and our own design notes only.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::normalize_quat;
use crate::model;
use crate::schema::{Location, Wcs};

pub struct SetWcsOrigin {
    /// The entity this command places - the active setup's WCS at
    /// construction time (`G54` in a document without setups,
    /// `Setup2/WCS` when the active setup owns its own). The G-code word
    /// is never stored here: it is the setup's label at post time.
    entity_name: String,
    location: Location,
    /// Where the WCS sat before; `None` means the entity did not exist
    /// yet, so the inverse removes it.
    prev: Option<Location>,
}

impl SetWcsOrigin {
    /// Validates against the world now: a finite rigid transform and no
    /// name collision with a model (AGENTS.md: validate early, never fail
    /// deep).
    pub fn new(world: &World, location: Location) -> Result<Self, CommandError> {
        let mut location = location;
        // #327: every refusal names the setup's WCS by its label
        // ("WCS (G55)"), so a script or the pane reports which offset
        // was refused.
        let active = crate::setup::active(world);
        let label = crate::setup::wcs_label(world, active.as_deref());
        if !location.rot.iter().all(|v| v.is_finite()) {
            return Err(CommandError::Invalid(format!(
                "{label}: rotation must be finite (check the angle inputs)"
            )));
        }
        location.rot = normalize_quat(location.rot).ok_or_else(|| {
            CommandError::Invalid(format!("{label}: rotation is degenerate (zero norm)"))
        })?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid(format!("{label}: origin must be finite")));
        }
        let entity_name = crate::setup::wcs_entity_name(world, active.as_deref());
        let prev = match model::find_by_name(world, &entity_name) {
            Some(entity) => {
                if !model::is_wcs(world, &entity) {
                    return Err(CommandError::NameConflict(entity_name));
                }
                model::transform_of(world, &entity)
            }
            None => None,
        };
        Ok(SetWcsOrigin {
            entity_name,
            location,
            prev,
        })
    }

    /// The entity this command places.
    #[must_use]
    pub fn entity_name(&self) -> &str {
        &self.entity_name
    }

    /// Place a NAMED setup's WCS (#771): the same validation as
    /// [`SetWcsOrigin::new`], but the setup is named rather than the
    /// active one - the Setups pages place each setup's origin from
    /// its own rows. `None` keeps the active-setup behaviour.
    pub fn for_setup(
        world: &World,
        setup: Option<&str>,
        location: Location,
    ) -> Result<Self, CommandError> {
        let mut location = location;
        let label = crate::setup::wcs_label(world, setup);
        if !location.rot.iter().all(|v| v.is_finite()) {
            return Err(CommandError::Invalid(format!(
                "{label}: rotation must be finite (check the angle inputs)"
            )));
        }
        location.rot = normalize_quat(location.rot).ok_or_else(|| {
            CommandError::Invalid(format!("{label}: rotation is degenerate (zero norm)"))
        })?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid(format!("{label}: origin must be finite")));
        }
        let entity_name = crate::setup::wcs_entity_name(world, setup);
        let prev = match model::find_by_name(world, &entity_name) {
            Some(entity) => {
                if !model::is_wcs(world, &entity) {
                    return Err(CommandError::NameConflict(entity_name));
                }
                model::transform_of(world, &entity)
            }
            None => None,
        };
        Ok(SetWcsOrigin {
            entity_name,
            location,
            prev,
        })
    }
}

/// Which box point names the origin (#771): the min corner or the
/// centre - the two readings a shop takes off a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxPoint {
    Corner,
    Centre,
}

impl BoxPoint {
    /// The short label the UI shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            BoxPoint::Corner => "corner",
            BoxPoint::Centre => "centre",
        }
    }
}

/// Where a setup's WCS origin is placed from (#771): a picked vertex,
/// or a box reading - the parts' bounds or the setup's stock.
#[derive(Clone, Debug, PartialEq)]
pub enum OriginAt {
    /// A picked vertex, world-placed.
    Vertex(crate::operation::GeoSel),
    /// The parts' bounds in this setup's frame.
    ModelBox(BoxPoint),
    /// The setup's stock bounds.
    StockBox(BoxPoint),
}

/// Resolve `at` to the origin point for `setup`, keeping the WCS's
/// current rotation (the frame comes from the plane side, #771; the
/// point only moves the origin). Refused by name, never a guessed
/// point.
pub fn place_setup_origin(world: &World, setup: &str, at: &OriginAt) -> Result<Location, CommandError> {
    let label = crate::setup::wcs_label(world, Some(setup));
    let wcs = crate::setup::wcs_entity_name(world, Some(setup));
    let rot = model::find_by_name(world, &wcs)
        .and_then(|e| model::transform_of(world, &e))
        .map(|l| l.rot)
        .unwrap_or_else(|| Location::default().rot);
    let pos = match at {
        OriginAt::Vertex(g) => crate::pick::align::vertex_with_pose(world, g, |world, entity| {
            crate::setup_align::pose_in(world, entity, Some(setup))
        }).map_err(CommandError::Invalid),
        OriginAt::ModelBox(point) => box_point(
            crate::setup_align::solid_aabb_in(world, Some(setup)),
            *point,
            "there is no model to read a box from - import one first",
        ),
        OriginAt::StockBox(point) => box_point(
            model::stock_world_aabb_for(world, Some(setup)),
            *point,
            &format!("setup '{setup}' has no stock placed - place one on the Stock page"),
        ),
    }?;
    let pos = crate::geometry::Point3::new(pos[0], pos[1], pos[2]);
    if !pos.is_finite() {
        return Err(CommandError::Invalid(format!("{label}: origin must be finite")));
    }
    Ok(Location { pos, rot })
}

fn box_point(aabb: Option<crate::geometry::Aabb>, point: BoxPoint, empty: &str) -> Result<[f64; 3], CommandError> {
    let b = aabb.ok_or_else(|| CommandError::Invalid(empty.to_string()))?;
    Ok(match point {
        BoxPoint::Corner => [b.min.x, b.min.y, b.min.z],
        BoxPoint::Centre => [
            (b.min.x + b.max.x) / 2.0,
            (b.min.y + b.max.y) / 2.0,
            (b.min.z + b.max.z) / 2.0,
        ],
    })
}

impl Command for SetWcsOrigin {
    fn apply(&self, world: &World) {
        // Pure world write from stored fields: idempotent under redo.
        let entity = match model::find_by_name(world, &self.entity_name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.entity_name),
        };
        entity.entity_view(world).add(Wcs); // tags add, not set (zero-sized)
        model::set_transform(world, &entity, self.location);
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.entity_name) else {
            return; // already undone (idempotent undo path)
        };
        match self.prev {
            Some(location) => model::set_transform(world, &entity, location),
            None => model::remove(world, &entity), // we created it
        }
    }
}

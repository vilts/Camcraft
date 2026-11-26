//! `SetSheetJob` (#3070): the guided sheet job as ONE undoable step over
//! the existing commands - `SetStock` (a box the sheet's size), then
//! `SetWcsOrigin` (Z zero at the sheet top or the spoilboard, XY zero on
//! the stated corner), then the setup's `sheet_job` record that the post
//! gate holds the program to ([`crate::sheet_job::program_problems`]).
//! Everything is validated and snapshotted at construction; `apply` runs
//! the steps in order and `invert` undoes them in reverse, so one undo
//! puts back the stock, the origin and the record as they were.
//! [`SetSheetJob::clear`] drops only the record.
//!
//! Provenance: our own design (the `BatchCommand` and
//! `SetSetupWorkholdingRpm` shapes); no vendor implementation inspected.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, SetStock, SetWcsOrigin};
use crate::model;
use crate::schema::SetupDef;
use crate::sheet_job::{self, SheetJob};
use crate::stock::StockShape;

/// Set up the active setup as a sheet job, or clear its record.
pub struct SetSheetJob {
    setup: String,
    steps: Vec<Box<dyn Command>>,
    prev: Option<SheetJob>,
    next: Option<SheetJob>,
}

impl std::fmt::Debug for SetSheetJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetSheetJob").field("setup", &self.setup).field("next", &self.next).finish_non_exhaustive()
    }
}

fn active_setup(world: &World) -> Result<(String, Option<SheetJob>), CommandError> {
    let setup = crate::setup::active(world).ok_or_else(|| {
        CommandError::Needs("a sheet job belongs to a setup - add a setup first".into())
    })?;
    let entity = model::find_by_name(world, &setup).ok_or_else(|| CommandError::NotFound(setup.clone()))?;
    let prev = entity
        .entity_view(world)
        .try_get::<&SetupDef>(|d| d.sheet_job)
        .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
    Ok((setup, prev))
}

impl SetSheetJob {
    /// The checklist on the active setup: the stock becomes a box the
    /// sheet's size, kept where the setup's stock sat (its XY centre and
    /// its bottom face; the origin when it has none), and the WCS moves
    /// to the stated zero. Refuses a job [`sheet_job::invalid`] names.
    pub fn new(world: &World, job: SheetJob) -> Result<Self, CommandError> {
        if let Some(why) = sheet_job::invalid(&job) {
            return Err(CommandError::Invalid(why));
        }
        let (setup, prev) = active_setup(world)?;
        let (cx, cy, z0) = model::stock_world_aabb_for(world, Some(&setup)).map_or((0.0, 0.0, 0.0), |b| {
            ((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0, b.min.z)
        });
        let (stock_at, origin) = sheet_job::placement(&job, cx, cy, z0);
        let shape = StockShape::Box { dx: job.length, dy: job.width, dz: job.thickness };
        let stock = SetStock::new(world, shape, stock_at)?;
        let wcs = SetWcsOrigin::for_setup(world, Some(&setup), origin)?;
        Ok(Self { setup, steps: vec![Box::new(stock), Box::new(wcs)], prev, next: Some(job) })
    }

    /// Drop the active setup's sheet-job record; the stock and the WCS
    /// stay as they are. Refuses when it states none.
    pub fn clear(world: &World) -> Result<Self, CommandError> {
        let (setup, prev) = active_setup(world)?;
        if prev.is_none() {
            return Err(CommandError::Invalid(format!("setup '{setup}' states no sheet job")));
        }
        Ok(Self { setup, steps: Vec::new(), prev, next: None })
    }

    /// The setup this command writes.
    #[must_use]
    pub fn setup(&self) -> &str {
        &self.setup
    }
}

fn write(world: &World, setup: &str, value: Option<SheetJob>) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
        def.sheet_job = value;
        view.set(def);
    }
}

impl Command for SetSheetJob {
    fn apply(&self, world: &World) {
        for step in &self.steps {
            step.apply(world);
        }
        write(world, &self.setup, self.next);
    }

    fn invert(&self, world: &World) {
        write(world, &self.setup, self.prev);
        for step in self.steps.iter().rev() {
            step.invert(world);
        }
    }
}

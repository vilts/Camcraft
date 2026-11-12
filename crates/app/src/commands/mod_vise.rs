//! The mod-vise pair command (#820): place a fixed and an adjustable jaw
//! block on a fixture plate's grid, fitted to the setup's stock, in ONE
//! undo unit. Each block lands through the fixture library's insert path
//! (`InsertFixture`, marked with its block parameters and its place on
//! the plate), seated by the same rules as `PlaceOnPlate` (the hole, the
//! turn, standing on the plate top, nothing hanging off). So both blocks
//! are ordinary meshed fixtures to the collision laws and riders of the
//! plate: moving the plate carries them, deleting it lets go of them.
//!
//! Provenance: our own composition of the insert and plate placement
//! commands over `modvise`'s geometry and fit.

use flecs_ecs::prelude::World;

use crate::commands::fixture_plate::{plate_of, seat};
use crate::commands::{Command, CommandError, InsertFixture};
use crate::fixture;
use crate::fixture_library::FixtureEntry;
use crate::geometry::{IDENTITY_Q, Point3};
use crate::model;
use crate::modvise::{self, JawParams, PairFit};
use crate::plate::{MountAt, PlateMount, RiderMove};
use crate::schema::Location;

/// Place a mod-vise pair on a plate.
pub struct PlaceModVisePair {
    fixed: InsertFixture,
    adjustable: InsertFixture,
    fit: PairFit,
}

/// One block of the pair, seated at `at` turned `yaw_deg`, as an insert.
fn block(
    world: &World,
    plate: &str,
    setup: Option<&str>,
    params: JawParams,
    at: &str,
    yaw_deg: f64,
) -> Result<InsertFixture, CommandError> {
    let def = plate_of(world, plate)?;
    let (plate_at, plate_mesh) = model::find_by_name(world, plate)
        .map(|e| (model::transform_of(world, &e).unwrap_or_default(), model::facets_of(world, &e).map(|f| f.to_vec()).unwrap_or_default()))
        .ok_or_else(|| CommandError::NotFound(plate.to_owned()))?;
    let facets = params.mesh().map_err(CommandError::Invalid)?;
    let m = PlateMount { plate: plate.to_owned(), at: MountAt::Hole(at.to_owned()), yaw_deg };
    let pose = seat(&def, &plate_mesh, plate_at, &params.base_name(), &facets, &m)?;
    let entry = FixtureEntry { name: params.base_name(), note: None, datum: Location { pos: Point3::ZERO, rot: IDENTITY_Q }, facets };
    Ok(InsertFixture::new(world, &entry, setup, pose)?.with_mod_jaw(params).with_mount(m))
}

impl PlaceModVisePair {
    /// Refused by name: an unknown plate or one with no grid, nonsense
    /// block figures, no stock in the plate's setup, and every refusal of
    /// the fit (`modvise::fit_pair`) or the seat (a block hanging off).
    pub fn new(world: &World, plate: &str, fixed: &JawParams, adjustable: &JawParams, at: &str, yaw_deg: f64) -> Result<Self, CommandError> {
        let def = plate_of(world, plate)?;
        let entity = model::find_by_name(world, plate).ok_or_else(|| CommandError::NotFound(plate.to_owned()))?;
        let plate_at = model::transform_of(world, &entity).unwrap_or_default();
        let setup = fixture::def_of(world, &entity).and_then(|d| d.setup);
        let stock = model::stock_world_aabb_for(world, setup.as_deref())
            .ok_or_else(|| CommandError::Needs(crate::vise_fit::no_stock_message(setup.as_deref(), "the mod vise to")))?;
        let fit = modvise::fit_pair(&def, plate_at, fixed, adjustable, at, yaw_deg, &stock)
            .map_err(|e| CommandError::Invalid(format!("mod vise on '{plate}': {e}")))?;
        let fixed_cmd = block(world, plate, setup.as_deref(), fixed.with_slide(0.0), &fit.fixed_at, fit.yaw_deg)?;
        let adj = adjustable.with_slide(fit.slide);
        // The names differ by side ("Mod vise fixed ..." / "... adjustable
        // ..."), and the fit refuses two blocks of one side.
        let adjustable_cmd = block(world, plate, setup.as_deref(), adj, &fit.adjustable_at, fit.yaw_deg + 180.0)?;
        Ok(Self { fixed: fixed_cmd, adjustable: adjustable_cmd, fit })
    }

    /// The model names the blocks land as: (fixed, adjustable).
    #[must_use]
    pub fn names(&self) -> (&str, &str) {
        (self.fixed.name(), self.adjustable.name())
    }

    /// The fit: the holes, the turn, the slide, the opening.
    #[must_use]
    pub fn fit(&self) -> &PairFit {
        &self.fit
    }

    /// Where the blocks land (world): (fixed, adjustable).
    #[must_use]
    pub fn locations(&self) -> (Location, Location) {
        (self.fixed.location(), self.adjustable.location())
    }
}

impl Command for PlaceModVisePair {
    fn apply(&self, world: &World) {
        self.fixed.apply(world);
        self.adjustable.apply(world);
    }

    fn invert(&self, world: &World) {
        self.adjustable.invert(world);
        self.fixed.invert(world);
    }
}

/// A plate edit's re-seated riders (`moves`), judged where they are mod
/// vise jaw blocks (#820 review): the edit keeps each block on its named
/// hole with its slide, but the stock stays put. Refused by name when it
/// puts a block's clamping face into the stock (or deeper in); a face it
/// lifts off the stock is a warning (re-fit it: `RefitModVise`, #832).
pub(crate) fn jaws_after_reseat(world: &World, moves: &[RiderMove]) -> Result<Vec<String>, CommandError> {
    let mut warnings = Vec::new();
    for m in moves {
        let Some(def) = model::find_by_name(world, &m.name).and_then(|e| fixture::def_of(world, &e)) else { continue };
        let Some(jaw) = def.mod_jaw.as_ref() else { continue };
        let Some(stock) = model::stock_world_aabb_for(world, def.setup.as_deref()) else { continue };
        let before = modvise::reach::face_gap(jaw, m.before, &stock);
        let after = modvise::reach::face_gap(jaw, m.after, &stock);
        warnings.extend(judge_face(&m.name, jaw, before, after, "the plate edit", "re-place the pair after changing the grid")?);
    }
    Ok(warnings)
}

/// One jaw block's clamping face moved from gap `before` to gap `after`
/// against the stock (`modvise::reach::face_gap`) by `cause`. Refused by
/// name when it ends in the stock deeper than it started; a face lifted
/// off the stock is a warning. A face that was not in front of the stock
/// before (`None`) was clear (#832, the #833 review's probe: a stock
/// moved beside the jaws, then a plate edit that brings a jaw 16 mm into
/// it, must not pass on a warning); one not in front after reaches into
/// nothing. `hint` says what to do instead.
pub(crate) fn judge_face(name: &str, jaw: &JawParams, before: Option<f64>, after: Option<f64>, cause: &str, hint: &str) -> Result<Option<String>, CommandError> {
    let deep = crate::fixture_clash::PENETRATION_MM;
    let Some(after) = after else { return Ok(None) };
    let side = jaw.side.word();
    if after < -deep && before.is_none_or(|b| after < b - deep) {
        return Err(CommandError::Invalid(format!(
            "{cause} would put mod vise '{name}''s {side} jaw {:.3} mm into the stock - {hint}",
            -after
        )));
    }
    if let Some(before) = before
        && after > deep
        && after > before + deep
    {
        return Ok(Some(format!("{cause} leaves mod vise '{name}''s {side} jaw {after:.3} mm off the stock - re-fit the pair to clamp it")));
    }
    Ok(None)
}

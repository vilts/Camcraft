//! Editing a placed mod-vise pair (#832), each ONE undo unit:
//!
//! - [`SetModJaw`]: change one jaw block's figures (or its slide). The
//!   mesh is regenerated through `UpdateModel`, as `SetVise` regenerates
//!   a vise, and the marker swapped with it. The block stays on its hole
//!   and turn; it is refused by name when it would no longer fit there
//!   (a bolt off the grid, the block hanging off the plate), when the
//!   edit changes its side, or when its clamping face would reach into
//!   the stock (a face lifted off it is a warning).
//! - [`RefitModVise`]: re-fit an adjustable block to the stock as it
//!   stands now, by the very fit the pair was placed with
//!   (`modvise::fit_pair`, the fixed block where it stands): it re-picks
//!   the hole nearest the stock that clears it and slides to touch it.
//!   Every refusal of the fit is named, and nothing moves.
//!
//! Provenance: our own composition of `UpdateModel`, the plate seat and
//! the marker swap over `modvise`'s geometry and fit.

use flecs_ecs::prelude::World;

use crate::commands::fixture_plate::{plate_of, seat};
use crate::commands::mod_vise::judge_face;
use crate::commands::{Command, CommandError, UpdateModel};
use crate::fixture;
use crate::geometry::{Mesh, Triangle};
use crate::model;
use crate::modvise::{self, JawParams, JawSide, PairFit};
use crate::plate::mount::{self, MountAt, PlateMount, RiderMove};
use crate::schema::{FixtureDef, Location};

/// Jaw block `name`'s marker, its parameters and its pose, or why it is
/// not one.
fn jaw_named(world: &World, name: &str) -> Result<(FixtureDef, JawParams, Location), CommandError> {
    let e = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
    let def = fixture::def_of(world, &e).ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
    let jaw = def.mod_jaw.clone().ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a mod vise jaw block")))?;
    Ok((def, jaw, model::transform_of(world, &e).unwrap_or_default()))
}

/// Plate `plate`'s pose and mesh.
fn plate_at(world: &World, plate: &str) -> Result<(Location, Vec<Triangle>), CommandError> {
    let e = model::find_by_name(world, plate).ok_or_else(|| CommandError::NotFound(plate.to_owned()))?;
    let mesh = model::facets_of(world, &e).map(|f| f.to_vec()).unwrap_or_default();
    Ok((model::transform_of(world, &e).unwrap_or_default(), mesh))
}

/// The body every edit shares: the block's new mesh, its marker swap and
/// the poses it moves (its own, and anything standing on it).
struct Edit {
    name: String,
    update: UpdateModel,
    prev: FixtureDef,
    next: FixtureDef,
    moves: Vec<RiderMove>,
}

impl Edit {
    fn new(world: &World, name: &str, facets: Vec<Triangle>, prev: FixtureDef, next: FixtureDef, before: Location, after: Location) -> Result<Self, CommandError> {
        let mut moves = mount::carry(world, name, mount::delta(before, after), &[]);
        moves.push(RiderMove { name: name.to_owned(), before, after });
        let update = UpdateModel::new(world, name, Mesh::from_facets(facets))?;
        Ok(Self { name: name.to_owned(), update, prev, next, moves })
    }

    fn write(&self, world: &World, forward: bool) {
        if forward {
            self.update.apply(world);
        } else {
            self.update.invert(world);
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(if forward { self.next.clone() } else { self.prev.clone() });
        }
        mount::write(world, &self.moves, forward);
    }
}

/// Change a placed jaw block's figures.
pub struct SetModJaw {
    edit: Edit,
    warning: Option<String>,
}

impl SetModJaw {
    /// Set jaw block `name` to `params`. Refused by name: not a jaw
    /// block, a changed side, figures that break the block law, a bolt or
    /// pin off the grid where it stands, a block hanging off its plate,
    /// a clamping face pushed into the stock.
    pub fn new(world: &World, name: &str, params: JawParams) -> Result<Self, CommandError> {
        let (prev, old, before) = jaw_named(world, name)?;
        if params.side != old.side {
            return Err(CommandError::Invalid(format!(
                "'{name}' is the {} of its pair - an edit keeps its side (place a new pair to change it)",
                old.what()
            )));
        }
        let facets = params.mesh().map_err(|e| CommandError::Invalid(format!("'{name}': {e}")))?;
        let after = match &prev.plate.mount {
            Some(m) => {
                let def = plate_of(world, &m.plate)?;
                if let MountAt::Hole(h) = &m.at {
                    modvise::fit::footprint_at(&def, &params, h, m.yaw_deg).map_err(|e| CommandError::Invalid(format!("'{name}' at {h}: {e}")))?;
                }
                let (at, plate_mesh) = plate_at(world, &m.plate)?;
                seat(&def, &plate_mesh, at, name, &facets, m)?
            }
            None => before,
        };
        let mut warnings = Vec::new();
        if let Some(stock) = model::stock_world_aabb_for(world, prev.setup.as_deref()) {
            let gap_before = modvise::reach::face_gap(&old, before, &stock);
            let gap_after = modvise::reach::face_gap(&params, after, &stock);
            warnings.extend(judge_face(name, &params, gap_before, gap_after, "the edit", "re-fit the pair, or edit it back")?);
        }
        warnings.extend(crate::fixture_clash::after_fixture_shape(world, name, &facets, after));
        let next = FixtureDef { mod_jaw: Some(params), ..prev.clone() };
        let edit = Edit::new(world, name, facets, prev, next, before, after)?;
        Ok(Self { edit, warning: crate::fixture_clash::join(warnings) })
    }

    /// The parameters the edit lands.
    #[must_use]
    pub fn params(&self) -> Option<&JawParams> {
        self.edit.next.mod_jaw.as_ref()
    }

    /// The by-name warnings of the edit (a jaw lifted off the stock, the
    /// new steel sitting in a part or the stock); `None` when clear.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

impl Command for SetModJaw {
    fn apply(&self, world: &World) {
        self.edit.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.edit.write(world, false);
    }
}

/// Re-fit a pair's adjustable block to the stock.
pub struct RefitModVise {
    edit: Edit,
    fit: PairFit,
    warning: Option<String>,
}

impl RefitModVise {
    /// Re-fit adjustable block `adjustable` against fixed block `fixed`
    /// (both on one plate, in one setup) to the setup's stock as it
    /// stands. Refused by name: either is not a jaw block of its side or
    /// not on the plate by a hole, they stand on different plates or
    /// setups, there is no stock, and every refusal of the fit.
    pub fn new(world: &World, fixed: &str, adjustable: &str) -> Result<Self, CommandError> {
        let (fdef, fj, _) = jaw_named(world, fixed)?;
        let (prev, aj, before) = jaw_named(world, adjustable)?;
        if fj.side != JawSide::Fixed {
            return Err(CommandError::Invalid(format!("'{fixed}' is the {}, not the fixed jaw block of a pair", fj.what())));
        }
        if aj.side != JawSide::Adjustable {
            return Err(CommandError::Invalid(format!("'{adjustable}' is the {} - only the adjustable jaw block is re-fitted", aj.what())));
        }
        let not_on = |n: &str| CommandError::Invalid(format!("'{n}' is not bolted to a plate's hole - re-place the pair"));
        let fm = fdef.plate.mount.clone().ok_or_else(|| not_on(fixed))?;
        let am = prev.plate.mount.clone().ok_or_else(|| not_on(adjustable))?;
        let MountAt::Hole(at) = &fm.at else { return Err(not_on(fixed)) };
        if fm.plate != am.plate {
            return Err(CommandError::Invalid(format!("'{fixed}' stands on '{}', '{adjustable}' on '{}' - not one pair", fm.plate, am.plate)));
        }
        if fdef.setup != prev.setup {
            return Err(CommandError::Invalid(format!("'{fixed}' and '{adjustable}' are in different setups - not one pair")));
        }
        let def = plate_of(world, &fm.plate)?;
        let (pat, plate_mesh) = plate_at(world, &fm.plate)?;
        let stock = model::stock_world_aabb_for(world, prev.setup.as_deref())
            .ok_or_else(|| CommandError::Invalid(format!("'{adjustable}': its setup has no stock to re-fit the mod vise to")))?;
        let fit = modvise::fit_pair(&def, pat, &fj, &aj.with_slide(0.0), at, fm.yaw_deg, &stock)
            .map_err(|e| CommandError::Invalid(format!("mod vise '{adjustable}' not re-fitted: {e}")))?;
        let params = aj.with_slide(fit.slide);
        let facets = params.mesh().map_err(|e| CommandError::Invalid(format!("'{adjustable}': {e}")))?;
        let m = PlateMount { plate: fm.plate.clone(), at: MountAt::Hole(fit.adjustable_at.clone()), yaw_deg: fit.yaw_deg + 180.0 };
        let after = seat(&def, &plate_mesh, pat, adjustable, &facets, &m)?;
        let warning = crate::fixture_clash::after_fixture_shape(world, adjustable, &facets, after);
        let mut next = FixtureDef { mod_jaw: Some(params), ..prev.clone() };
        next.plate.mount = Some(m);
        let edit = Edit::new(world, adjustable, facets, prev, next, before, after)?;
        Ok(Self { edit, fit, warning })
    }

    /// Re-fit adjustable block `adjustable` against the one fixed jaw
    /// block on its plate; refused by name when there is none, or more
    /// than one (then name it: [`RefitModVise::new`]).
    pub fn to_partner(world: &World, adjustable: &str) -> Result<Self, CommandError> {
        let fixed = partner_of(world, adjustable)?;
        Self::new(world, &fixed, adjustable)
    }

    /// The fit: the holes, the turn, the slide, the opening.
    #[must_use]
    pub fn fit(&self) -> &PairFit {
        &self.fit
    }

    /// Where the adjustable block lands (world).
    #[must_use]
    pub fn location(&self) -> Location {
        self.edit.moves.last().map_or_else(Location::default, |m| m.after)
    }

    /// A warning when the re-fitted steel sits in a part or the stock.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

/// The one fixed jaw block on adjustable block `adjustable`'s plate.
pub fn partner_of(world: &World, adjustable: &str) -> Result<String, CommandError> {
    let (def, _, _) = jaw_named(world, adjustable)?;
    let plate = def
        .plate
        .mount
        .map(|m| m.plate)
        .ok_or_else(|| CommandError::Invalid(format!("'{adjustable}' is not bolted to a plate - re-place the pair")))?;
    let fixed: Vec<String> = mount::riders(world, &plate)
        .into_iter()
        .filter(|n| {
            model::find_by_name(world, n)
                .and_then(|e| fixture::def_of(world, &e))
                .is_some_and(|d| d.setup == def.setup && d.mod_jaw.is_some_and(|j| j.side == JawSide::Fixed))
        })
        .collect();
    match fixed.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(CommandError::Invalid(format!("'{plate}' carries no fixed jaw block for '{adjustable}' to clamp against"))),
        many => Err(CommandError::Invalid(format!(
            "'{plate}' carries {} fixed jaw blocks ({}) - name the one '{adjustable}' clamps against",
            many.len(),
            many.join(", ")
        ))),
    }
}

impl Command for RefitModVise {
    fn apply(&self, world: &World) {
        self.edit.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.edit.write(world, false);
    }
}

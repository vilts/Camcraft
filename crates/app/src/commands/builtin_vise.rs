//! Built-in vise commands (#788): place a generated vise into a setup,
//! and change a placed vise's dimensions or jaw opening - each ONE undo
//! unit. Placing goes through the fixture library's insert path
//! (`InsertFixture`, marked with the vise's parameters), so a built-in
//! vise is an ordinary meshed fixture to the collision laws, the project
//! file and every fixture command. Editing regenerates the mesh through
//! `UpdateModel` (the model's revision moves, so every dependent cache
//! sees the new steel) and swaps the marker's parameters with it.
//!
//! Provenance: our own composition of the insert, update and marker
//! commands over `vise`'s generated geometry.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, InsertFixture, UpdateModel};
use crate::fixture;
use crate::fixture_library::FixtureEntry;
use crate::geometry::{IDENTITY_Q, Mesh, Point3};
use crate::model;
use crate::schema::{FixtureDef, Location};
use crate::vise::ViseParams;
use crate::vise_fit;

/// Where a new vise goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisePlacement {
    /// The vise frame's origin (the fixed jaw's face, centred, at the
    /// floor; a self-centering vise's centre between the jaws, #787) at
    /// this pose, the opening as given.
    At(Location),
    /// Against the setup's stock: fixed jaw on the stock's +y side,
    /// centred in x, floor under it, the opening the stock's y extent (a
    /// self-centering vise: centred on the stock in x and y).
    FitStock,
}

/// Place built-in vise `params` into setup context `setup` (`None` - the
/// implicit first setup).
pub struct InsertVise {
    inner: InsertFixture,
}

impl InsertVise {
    /// Refused when the vise breaks the vise law (named), the setup is
    /// gone, the pose is not a finite rigid frame, or - fitting - the
    /// setup has no stock or the stock is wider than the vise opens.
    pub fn new(
        world: &World,
        params: &ViseParams,
        setup: Option<&str>,
        placement: VisePlacement,
    ) -> Result<Self, CommandError> {
        let (params, pick) = match placement {
            VisePlacement::At(loc) => (params.clone(), loc),
            VisePlacement::FitStock => {
                vise_fit::fit(world, params, setup).map_err(|e| CommandError::Needs(e.message().to_owned()))?
            }
        };
        let facets = params.mesh().map_err(CommandError::Invalid)?;
        let entry = FixtureEntry {
            name: params.base_name(),
            note: None,
            datum: Location { pos: Point3::ZERO, rot: IDENTITY_Q },
            facets,
        };
        let inner = InsertFixture::new(world, &entry, setup, pick)?.with_vise(params);
        Ok(Self { inner })
    }

    /// The model name the vise lands as.
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// The pose of the vise frame.
    #[must_use]
    pub fn location(&self) -> Location {
        self.inner.location()
    }

    /// The parameters the vise is built from.
    #[must_use]
    pub fn params(&self) -> Option<&ViseParams> {
        self.inner.def().vise.as_ref()
    }
}

impl Command for InsertVise {
    fn apply(&self, world: &World) {
        self.inner.apply(world);
    }

    fn invert(&self, world: &World) {
        self.inner.invert(world);
    }
}

/// Change a placed built-in vise's parameters (its dimensions or jaw
/// opening): the mesh is regenerated and the marker updated together.
pub struct SetVise {
    name: String,
    update: UpdateModel,
    prev: FixtureDef,
    next: FixtureDef,
}

/// The vise marker of fixture `name`, or why not.
fn vise_def(world: &World, name: &str) -> Result<FixtureDef, CommandError> {
    let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
    let def = fixture::def_of(world, &entity)
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
    if def.vise.is_none() {
        return Err(CommandError::Invalid(format!("'{name}' is not a built-in vise (an imported fixture has a fixed shape)")));
    }
    Ok(def)
}

impl SetVise {
    /// Set vise `name` to `params`. Refused when `name` is not a built-in
    /// vise or `params` break the vise law (named).
    pub fn new(world: &World, name: &str, params: ViseParams) -> Result<Self, CommandError> {
        let prev = vise_def(world, name)?;
        let facets = params.mesh().map_err(CommandError::Invalid)?;
        let update = UpdateModel::new(world, name, Mesh::from_facets(facets))?;
        let next = FixtureDef { vise: Some(params), ..prev.clone() };
        Ok(Self { name: name.to_owned(), update, prev, next })
    }

    /// Set only vise `name`'s jaw opening.
    pub fn opening(world: &World, name: &str, opening: f64) -> Result<Self, CommandError> {
        let params = vise_def(world, name)?.vise.ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a built-in vise")))?;
        Self::new(world, name, params.with_opening(opening))
    }

    /// Open vise `name`'s jaws to its setup's stock, as the vise stands:
    /// the moving jaw against the stock's far side from the fixed jaw.
    pub fn opening_from_stock(world: &World, name: &str) -> Result<Self, CommandError> {
        let params = vise_def(world, name)?.vise.ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a built-in vise")))?;
        Self::from_stock(world, name, params)
    }

    /// Set vise `name` to `params` with the opening taken from its
    /// setup's stock (one undo unit for a resize plus a re-open).
    pub fn from_stock(world: &World, name: &str, params: ViseParams) -> Result<Self, CommandError> {
        let def = vise_def(world, name)?;
        let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
        let pose = model::transform_of(world, &entity).unwrap_or_default();
        let stock = model::stock_world_aabb_for(world, def.setup.as_deref())
            .ok_or_else(|| CommandError::Invalid(format!("'{name}': its setup has no stock to open the jaws to")))?;
        let opening = params.opening_to_stock(&pose, &stock).map_err(|e| CommandError::Invalid(format!("'{name}': {e}")))?;
        Self::new(world, name, params.with_opening(opening))
    }

    /// The parameters the edit lands.
    #[must_use]
    pub fn params(&self) -> Option<&ViseParams> {
        self.next.vise.as_ref()
    }
}

impl Command for SetVise {
    fn apply(&self, world: &World) {
        self.update.apply(world);
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        self.update.invert(world);
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(self.prev.clone());
        }
    }
}

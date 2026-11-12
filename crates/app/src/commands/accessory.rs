//! Built-in accessory commands (#789): place a generated workholding
//! accessory into a setup, and resize a placed one - each ONE undo unit.
//! Placing goes through the fixture library's insert path
//! (`InsertFixture`, marked with the accessory's parameters), so an
//! accessory is an ordinary meshed fixture to the collision laws, the
//! project file and every fixture command. Parallels fitted under the
//! stock inside a built-in vise also drop the vise (`MoveFixture`) in the
//! same unit. Resizing regenerates the mesh through `UpdateModel` (the
//! revision moves, so caches see the new steel) and swaps the marker.
//!
//! Provenance: our own composition of the insert, move, update and marker
//! commands over `accessory`'s generated geometry (the #788 vise pattern).

use flecs_ecs::prelude::World;

use crate::accessory::{self, Accessory};
use crate::commands::{Command, CommandError, InsertFixture, MoveFixture, UpdateModel};
use crate::fixture;
use crate::fixture_library::FixtureEntry;
use crate::geometry::{IDENTITY_Q, Mesh, Point3};
use crate::model;
use crate::schema::{FixtureDef, Location};

/// Where a new accessory goes.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessoryPlacement {
    /// The accessory frame's origin at this pose.
    At(Location),
    /// Parallels only: under the setup's stock inside built-in vise
    /// `vise`, so the stock's top stands `above` mm over the jaws - the
    /// parallels' height is set to fit and the vise drops to suit.
    UnderStockInVise { vise: String, above: f64 },
}

/// Place accessory `params` into setup context `setup` (`None` - the
/// implicit first setup).
pub struct InsertAccessory {
    inner: InsertFixture,
    /// The vise drop of a parallels fit, applied after the insert.
    lift: Option<MoveFixture>,
}

impl InsertAccessory {
    /// Refused when the accessory breaks its law (named), the setup is
    /// gone, the pose is not a finite rigid frame, or (fitting parallels)
    /// the vise is not a built-in vise, the setup has no stock, the
    /// stock already stands that high, or the pair does not fit the jaws.
    pub fn new(
        world: &World,
        params: &Accessory,
        setup: Option<&str>,
        placement: AccessoryPlacement,
    ) -> Result<Self, CommandError> {
        let (params, pick, lift) = match placement {
            AccessoryPlacement::At(loc) => (params.clone(), loc, None),
            AccessoryPlacement::UnderStockInVise { vise, above } => fit_under_stock(world, params, setup, &vise, above)?,
        };
        let facets = params.mesh().map_err(CommandError::Invalid)?;
        let entry = FixtureEntry {
            name: params.label().to_owned(),
            note: None,
            datum: Location { pos: Point3::ZERO, rot: IDENTITY_Q },
            facets,
        };
        let inner = InsertFixture::new(world, &entry, setup, pick)?.with_accessory(params);
        Ok(Self { inner, lift })
    }

    /// The model name the accessory lands as.
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// The pose of the accessory frame.
    #[must_use]
    pub fn location(&self) -> Location {
        self.inner.location()
    }

    /// The parameters it is built from (a fit's height included).
    #[must_use]
    pub fn params(&self) -> Option<&Accessory> {
        self.inner.def().accessory.as_ref()
    }
}

/// The parallels, their pose and the vise's drop for an under-stock fit.
fn fit_under_stock(
    world: &World,
    params: &Accessory,
    setup: Option<&str>,
    vise: &str,
    above: f64,
) -> Result<(Accessory, Location, Option<MoveFixture>), CommandError> {
    let Accessory::Parallels { thickness, length, .. } = *params else {
        return Err(CommandError::Invalid(format!(
            "only parallels fit under the stock in a vise, not a {}",
            params.label().to_lowercase()
        )));
    };
    let entity = model::find_by_name(world, vise).ok_or_else(|| CommandError::NotFound(vise.to_owned()))?;
    let vp = fixture::def_of(world, &entity)
        .and_then(|d| d.vise)
        .ok_or_else(|| CommandError::Invalid(format!("'{vise}' is not a built-in vise")))?;
    let pose = model::transform_of(world, &entity).unwrap_or_default();
    let stock = model::stock_world_aabb_for(world, setup)
        .ok_or_else(|| CommandError::Needs(crate::vise_fit::no_stock_message(setup, "the parallels under")))?;
    let fit = accessory::fit_parallels(&pose, vp.jaw_height, vp.opening, &stock, thickness, above)
        .map_err(|e| CommandError::Invalid(format!("parallels in '{vise}': {e}")))?;
    let fitted = Accessory::Parallels { height: fit.height, thickness, length, spacing: fit.spacing };
    let lift = MoveFixture::locating(world, vise, fit.vise)?;
    Ok((fitted, fit.parallels, Some(lift)))
}

impl Command for InsertAccessory {
    fn apply(&self, world: &World) {
        self.inner.apply(world);
        if let Some(lift) = &self.lift {
            lift.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(lift) = &self.lift {
            lift.invert(world);
        }
        self.inner.invert(world);
    }
}

/// Resize a placed built-in accessory: the mesh is regenerated and the
/// marker updated together.
pub struct SetAccessory {
    name: String,
    update: UpdateModel,
    prev: FixtureDef,
    next: FixtureDef,
}

impl SetAccessory {
    /// Set accessory `name` to `params`. Refused when `name` is not a
    /// built-in accessory, `params` is another kind, or breaks the law.
    pub fn new(world: &World, name: &str, params: Accessory) -> Result<Self, CommandError> {
        let prev = accessory_def(world, name)?;
        let was = prev.accessory.as_ref().map_or("", Accessory::kind);
        if was != params.kind() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a {}; it cannot become a {}",
                accessory::label_of(was).to_lowercase(),
                params.label().to_lowercase()
            )));
        }
        let facets = params.mesh().map_err(|e| CommandError::Invalid(format!("'{name}': {e}")))?;
        let update = UpdateModel::new(world, name, Mesh::from_facets(facets))?;
        let next = FixtureDef { accessory: Some(params), ..prev.clone() };
        Ok(Self { name: name.to_owned(), update, prev, next })
    }

    /// The parameters the edit lands.
    #[must_use]
    pub fn params(&self) -> Option<&Accessory> {
        self.next.accessory.as_ref()
    }
}

/// The accessory marker of fixture `name`, or why not.
pub fn accessory_def(world: &World, name: &str) -> Result<FixtureDef, CommandError> {
    let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
    let def = fixture::def_of(world, &entity)
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
    if def.accessory.is_none() {
        return Err(CommandError::Invalid(format!(
            "'{name}' is not a built-in accessory (an imported fixture has a fixed shape)"
        )));
    }
    Ok(def)
}

impl Command for SetAccessory {
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

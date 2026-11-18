//! Undoable commands for DXF/profile fixtures and STEP profile derivation (#744).
//! Provenance: clean-room composition of `FixtureProfile` with app command history.

use flecs_ecs::prelude::World;

use crate::commands::fixture::{check_location, require_writable};
use crate::commands::insert_fixture::free_name;
use crate::commands::{Command, CommandError, UpdateModel};
use crate::fixture::{self, MACHINE_TABLE};
use crate::fixture_profile::{ExtrudeAxis, FixtureProfile};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{FixtureDef, Location};
use crate::setup;

/// Insert a fixture generated from a spun half-profile or extruded 2D closed profile.
pub struct InsertProfileFixture {
    name: String,
    profile: FixtureProfile,
    facets: std::sync::Arc<[Triangle]>,
    location: Location,
    def: FixtureDef,
}

impl InsertProfileFixture {
    /// Validate `profile` and prepare an invertible insert command.
    pub fn new(
        world: &World,
        base_name: &str,
        profile: FixtureProfile,
        setup_name: Option<&str>,
        location: Location,
    ) -> Result<Self, CommandError> {
        let base = base_name.trim();
        if base.is_empty() {
            return Err(CommandError::Invalid("fixture name cannot be blank".into()));
        }
        if base == MACHINE_TABLE {
            return Err(CommandError::Invalid(
                "'Machine/Table' names the derived machine table".into(),
            ));
        }
        if let Some(s) = setup_name
            && !setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_string()));
        }
        let location = check_location(location, "fixture")?;
        let mesh = profile.mesh().map_err(CommandError::Invalid)?;
        Ok(Self {
            name: free_name(world, base),
            facets: mesh.facets,
            location,
            def: FixtureDef {
                setup: setup_name.map(str::to_owned),
                fixed: false,
                profile: Some(profile.clone()),
                ..FixtureDef::default()
            },
            profile,
        })
    }

    /// Import a turning spun fixture (chuck, jaw, collet, faceplate) from ASCII DXF bytes.
    pub fn from_dxf_spun(
        world: &World,
        base_name: &str,
        bytes: &[u8],
        setup_name: Option<&str>,
        location: Location,
    ) -> Result<Self, CommandError> {
        let profile = FixtureProfile::spun_from_dxf(bytes).map_err(CommandError::Invalid)?;
        Self::new(world, base_name, profile, setup_name, location)
    }

    /// Import a milling extruded fixture from ASCII DXF bytes along `axis`.
    pub fn from_dxf_extruded(
        world: &World,
        base_name: &str,
        bytes: &[u8],
        setup_name: Option<&str>,
        axis: ExtrudeAxis,
        depth_mm: Option<f64>,
        location: Location,
    ) -> Result<Self, CommandError> {
        let profile = FixtureProfile::extruded_from_dxf(bytes, axis, depth_mm)
            .map_err(CommandError::Invalid)?;
        Self::new(world, base_name, profile, setup_name, location)
    }

    /// The resolved model name this insert lands under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The validated profile held by this insert.
    #[must_use]
    pub fn profile(&self) -> &FixtureProfile {
        &self.profile
    }
}

impl Command for InsertProfileFixture {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.facets.clone());
        model::set_rev(world, &entity, 1);
        model::set_transform(world, &entity, self.location);
        entity.entity_view(world).set(self.def.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

/// Edit a profile fixture's geometry and regenerate its solid mesh in one undo unit.
pub struct SetFixtureProfile {
    name: String,
    update: UpdateModel,
    prev: FixtureDef,
    next: FixtureDef,
}

impl SetFixtureProfile {
    /// Replace `name`'s profile with `profile`, re-meshing directly into `Facets`.
    pub fn new(world: &World, name: &str, profile: FixtureProfile) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let prev = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        if crate::step_source::of(world, &entity).is_some() {
            return Err(CommandError::Invalid(format!(
                "'{name}' keeps its imported STEP solid; derive a profile for reference instead of replacing STEP geometry"
            )));
        }
        if prev.vise.is_some() || prev.mod_jaw.is_some() || prev.accessory.is_some() || prev.plate.plate.is_some() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a parametric vise/accessory/plate; edit its own parameters instead"
            )));
        }
        let mesh = profile.mesh().map_err(CommandError::Invalid)?;
        let update = UpdateModel::new(world, name, mesh)?;
        let next = FixtureDef { profile: Some(profile), ..prev.clone() };
        Ok(Self { name: name.to_owned(), update, prev, next })
    }
}

impl Command for SetFixtureProfile {
    fn apply(&self, world: &World) {
        self.update.apply(world);
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        self.update.invert(world);
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Optionally derive a spun or extruded reference profile from a STEP
/// fixture's mesh while keeping STEP source and STEP facets unchanged (#744).
/// A fixture without a STEP source is refused by name.
pub struct DeriveFixtureProfile {
    name: String,
    prev: FixtureDef,
    next: FixtureDef,
}

impl DeriveFixtureProfile {
    /// Derive a spun half-profile around `+Z` from fixture `name`'s mesh.
    pub fn spun(world: &World, name: &str) -> Result<Self, CommandError> {
        let (prev, facets) = target_fixture(world, name)?;
        let profile = FixtureProfile::derive_spun(&facets).map_err(CommandError::Invalid)?;
        let next = FixtureDef { profile: Some(profile), ..prev.clone() };
        Ok(Self { name: name.to_owned(), prev, next })
    }

    /// Derive an extruded profile along `axis` from fixture `name`'s mesh.
    pub fn extruded(world: &World, name: &str, axis: ExtrudeAxis) -> Result<Self, CommandError> {
        let (prev, facets) = target_fixture(world, name)?;
        let profile =
            FixtureProfile::derive_extruded(&facets, axis).map_err(CommandError::Invalid)?;
        let next = FixtureDef { profile: Some(profile), ..prev.clone() };
        Ok(Self { name: name.to_owned(), prev, next })
    }
}

fn target_fixture(world: &World, name: &str) -> Result<(FixtureDef, std::sync::Arc<[Triangle]>), CommandError> {
    let entity = require_writable(world, name)?;
    let def = fixture::def_of(world, &entity)
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
    // A derived profile is a reference only while a STEP source keeps the
    // facets. Without one, the profile is what collision, library save and
    // re-insertion read as the steel: a derived approximation there could
    // drop a generated fixture's chord bound or re-mesh an imported mesh
    // smaller than the real steel (PR #2101 review).
    if crate::step_source::of(world, &entity).is_none() {
        return Err(CommandError::Invalid(if def.profile.is_some() {
            format!(
                "'{name}' is generated from its own profile; edit that profile instead of deriving a reference profile"
            )
        } else {
            format!(
                "'{name}' has no STEP source; a reference profile is derived only from an imported STEP fixture"
            )
        }));
    }
    let facets = model::facets_of(world, &entity)
        .filter(|f| !f.is_empty())
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' has no mesh facets to derive from")))?;
    Ok((def, facets))
}

impl Command for DeriveFixtureProfile {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

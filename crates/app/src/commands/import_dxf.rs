//! ImportDxf (#1862): read a DXF drawing at construction, place it as a
//! profile on the top face of a setup's stock on apply, remove it on
//! invert.
//!
//! Provenance: our own command over this repo's DXF reader
//! (`camcraft_geometry::dxf`, from Autodesk's published DXF Reference)
//! and sketch chaining; the pattern is `ImportModel`'s. No vendor
//! material consulted.
//!
//! ## The laws this command keeps
//!
//! - **Everything is refused at construction.** A file that is not a
//!   DXF, is too large, does not parse, has nothing to place or cannot be
//!   chained, a setup that does not exist, and a setup without a stock
//!   never reach the history.
//! - **Units are never guessed.** A file that states no unit, with no
//!   unit from the caller, is the typed `CommandError::DxfUnitsMissing`.
//!   A unit from the caller wins over the file's.
//! - **A profile is placed, not loaded like a model.** It joins a setup
//!   (the active one by default) and starts on the top face of that
//!   setup's stock, at the stock origin.
//! - **`apply` and `invert` are pure writes**, keyed by name.

use std::path::Path;
use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{LEGACY_WCS_NAMES, Location, STOCK_NAME, Sketch};
use crate::sketch;
use crate::sketch_data::{SketchData, SketchError};
use crate::units::Unit;
use camcraft_geometry::dxf;

/// What the caller states for an import; the default states nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DxfOptions {
    /// The drawing's unit: wins over the file's `$INSUNITS`, and is
    /// required when the file states none.
    pub units: Option<Unit>,
    /// The chaining tolerance in mm; `None` is the chaining default.
    pub join_tolerance: Option<f64>,
    /// The profile's name; `None` is the file's stem, made unique.
    pub name: Option<String>,
}

pub struct ImportDxf {
    name: String,
    sketch: Sketch,
}

impl ImportDxf {
    /// Read and check `path` now, for `setup` (`None`: the active setup).
    pub fn new(world: &World, setup: Option<&str>, path: &Path, options: &DxfOptions) -> Result<Self, CommandError> {
        check_extension(path)?;
        let cap = u64::try_from(dxf::Limits::default().max_bytes).unwrap_or(u64::MAX);
        let read_err = |e: std::io::Error| CommandError::Dxf(format!("cannot read {}: {e}", path.display()));
        if std::fs::metadata(path).map_err(read_err)?.len() > cap {
            return Err(CommandError::Dxf(format!("{} is larger than the {cap} byte limit", path.display())));
        }
        let data = std::fs::read(path).map_err(read_err)?;
        Self::build(world, setup, path, &data, options)
    }

    /// [`ImportDxf::new`] over bytes already in hand, named by the file
    /// they came from (a browser's file picker hands over a name and its
    /// contents). The same laws run.
    pub fn from_bytes(
        world: &World,
        setup: Option<&str>,
        file_name: &str,
        data: &[u8],
        options: &DxfOptions,
    ) -> Result<Self, CommandError> {
        let path = Path::new(file_name);
        check_extension(path)?;
        Self::build(world, setup, path, data, options)
    }

    fn build(world: &World, setup: Option<&str>, path: &Path, data: &[u8], options: &DxfOptions) -> Result<Self, CommandError> {
        let setup = match setup {
            Some(name) if !crate::setup::exists(world, name) => return Err(CommandError::NotFound(name.to_owned())),
            Some(name) => Some(name.to_owned()),
            None => crate::setup::active(world),
        };
        match sketch::stock_face(world, setup.as_deref()) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(CommandError::Invalid(format!(
                    "{} has no stock - make a stock first, then place the DXF profile on it",
                    setup.as_deref().map_or_else(|| "the document".to_owned(), |s| format!("setup '{s}'"))
                )));
            }
            Err(why) => return Err(CommandError::Invalid(why.into())),
        }
        let doc = dxf::parse(data).map_err(|e| CommandError::Dxf(e.to_string()))?;
        let unit = options.units.map(|u| match u {
            Unit::Millimeters => dxf::Unit::Millimeters,
            Unit::Inches => dxf::Unit::Inches,
        });
        let data = SketchData::from_dxf(doc, unit, options.join_tolerance).map_err(|e| match e {
            SketchError::MissingUnits { width, height } => CommandError::DxfUnitsMissing { width, height },
            other => CommandError::Dxf(other.to_string()),
        })?;
        let name = profile_name(world, path, options.name.as_deref())?;
        Ok(ImportDxf { name, sketch: Sketch { setup, data: Arc::new(data) } })
    }

    /// The name the profile lands under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The setup the profile joins (`None` in a document without setups).
    #[must_use]
    pub fn setup(&self) -> Option<&str> {
        self.sketch.setup.as_deref()
    }

    /// What was read: units, layers, chains and the import summary.
    #[must_use]
    pub fn data(&self) -> &SketchData {
        &self.sketch.data
    }

    /// A profile read by another importer (#1880: `ImportSvg`), already checked, placed and
    /// removed as a DXF one is.
    pub(super) fn placed(name: String, sketch: Sketch) -> Self {
        ImportDxf { name, sketch }
    }
}

/// Only a `.dxf` file is a DXF drawing; any other extension is refused
/// by name before a byte is read.
fn check_extension(path: &Path) -> Result<(), CommandError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("dxf") => Ok(()),
        _ => Err(CommandError::Dxf(format!("{} is not a .dxf file", path.display()))),
    }
}

/// A stated name must be free and must not be a setup-furniture name; a
/// name taken from the file's stem is made unique (`part`, `part (2)`),
/// so one drawing can be placed more than once.
pub(super) fn profile_name(world: &World, path: &Path, stated: Option<&str>) -> Result<String, CommandError> {
    let reserved = |n: &str| n.eq_ignore_ascii_case(STOCK_NAME) || LEGACY_WCS_NAMES.contains(&n) || crate::setup::is_furniture_shape(n);
    if let Some(name) = stated {
        if name.trim().is_empty() || reserved(name) {
            return Err(CommandError::Invalid(format!("'{name}' is not a usable name for a DXF profile")));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_owned()));
        }
        return Ok(name.to_owned());
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).map(str::trim).filter(|s| !s.is_empty() && !reserved(s)).unwrap_or("profile");
    sketch::free_name(world, stem).ok_or_else(|| CommandError::NameConflict(stem.to_owned()))
}

impl Command for ImportDxf {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already there (an idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, Location::default());
        entity.entity_view(world).set(self.sketch.clone());
    }

    fn invert(&self, world: &World) {
        // Name-keyed, and only ever a profile: finding nothing, or
        // something else under the name, must no-op, not delete it.
        if let Some((entity, _)) = sketch::find(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

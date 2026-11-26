//! ImportSvg (#1880): read an SVG drawing at construction and place it as a profile on the top
//! face of a setup's stock, the same profile entity that `ImportDxf` places.
//!
//! Provenance: our own command over this repo's SVG reader (`camcraft_geometry::svg`, from
//! the W3C SVG 1.1 Recommendation) and `ImportDxf`. No vendor material consulted.
//!
//! ## The laws this command keeps
//!
//! - **Everything is refused at construction**, as for `ImportDxf`: a file that is not a
//!   `.svg`, is too large, does not parse, has nothing to place or cannot be chained, an
//!   unknown setup and a setup without a stock never reach the history.
//! - **The size is never guessed.** An SVG with no physical size, with no unit from the
//!   caller, is a `CommandError::Needs` that names the unit. A caller's unit for a file that
//!   states its size is refused, so two sizes never disagree silently.
//! - **`apply` and `invert` are `ImportDxf`'s**: the placed entity is the same.

use std::path::Path;
use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::commands::import_dxf::{profile_name, ImportDxf};
use crate::commands::{Command, CommandError, DxfOptions};
use crate::schema::Sketch;
use crate::sketch;
use crate::sketch_data::{SketchData, SketchError};
use crate::units::Unit;
use camcraft_geometry::{dxf, svg};

pub struct ImportSvg(ImportDxf);

impl ImportSvg {
    /// Read and check `path` now, for `setup` (`None`: the active setup).
    pub fn new(world: &World, setup: Option<&str>, path: &Path, options: &DxfOptions) -> Result<Self, CommandError> {
        check_extension(path)?;
        let cap = u64::try_from(dxf::Limits::default().max_bytes).unwrap_or(u64::MAX);
        let read_err = |e: std::io::Error| CommandError::Svg(format!("cannot read {}: {e}", path.display()));
        if std::fs::metadata(path).map_err(read_err)?.len() > cap {
            return Err(CommandError::Svg(format!("{} is larger than the {cap} byte limit", path.display())));
        }
        let data = std::fs::read(path).map_err(read_err)?;
        Self::build(world, setup, path, &data, options)
    }

    /// [`ImportSvg::new`] over bytes already in hand, named by the file they came from.
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
        if sketch::face_frame(world, setup.as_deref()).is_none() {
            return Err(CommandError::Invalid(format!(
                "{} has no stock - make a stock first, then place the SVG profile on it",
                setup.as_deref().map_or_else(|| "the document".to_owned(), |s| format!("setup '{s}'"))
            )));
        }
        let unit = options.units.map(|u| match u {
            Unit::Millimeters => dxf::Unit::Millimeters,
            Unit::Inches => dxf::Unit::Inches,
        });
        let doc = svg::parse(data, unit).map_err(|e| CommandError::Svg(e.to_string()))?;
        let data = SketchData::from_dxf(doc, unit, options.join_tolerance).map_err(|e| match e {
            SketchError::MissingUnits { width, height } => CommandError::Needs(format!(
                "the SVG file states no physical size (width and height in mm, cm, in, pt, pc or px) - state \"mm\" or \
                 \"in\" for one drawing unit; the drawing is {width} x {height} drawing units"
            )),
            SketchError::Empty { skipped } => CommandError::Svg(format!(
                "the SVG file has nothing to place: no path or shape was drawn ({skipped} skipped)"
            )),
            other => CommandError::Svg(other.to_string()),
        })?;
        let name = profile_name(world, path, options.name.as_deref())?;
        Ok(ImportSvg(ImportDxf::placed(name, Sketch { setup, data: Arc::new(data) })))
    }

    /// The name the profile lands under.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// The setup the profile joins (`None` in a document without setups).
    #[must_use]
    pub fn setup(&self) -> Option<&str> {
        self.0.setup()
    }

    /// What was read: units, layers, chains and the import summary.
    #[must_use]
    pub fn data(&self) -> &SketchData {
        self.0.data()
    }
}

/// Only a `.svg` file is an SVG drawing; any other extension is refused by name before a byte
/// is read.
fn check_extension(path: &Path) -> Result<(), CommandError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("svg") => Ok(()),
        _ => Err(CommandError::Svg(format!("{} is not a .svg file", path.display()))),
    }
}

impl Command for ImportSvg {
    fn apply(&self, world: &World) {
        self.0.apply(world);
    }

    fn invert(&self, world: &World) {
        self.0.invert(world);
    }
}

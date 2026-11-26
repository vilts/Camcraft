//! InsertOutlineText (#3072): make a typed string in a TrueType outline
//! font into closed shapes at construction, place them as a profile on
//! the top face of a setup's stock on apply, remove the profile on
//! invert. The shapes take a pocket, a V-carve or a cut-out.
//!
//! Provenance: our own command over this repo's outline reader
//! (`camcraft_geometry::outline_text`, from the public OpenType
//! specification); the pattern is `InsertText`'s. No vendor material
//! consulted.
//!
//! ## The laws this command keeps
//!
//! - **Everything is refused at construction.** A font file that cannot
//!   be read, is too large or has CFF outlines, an empty text, a
//!   character the font does not have, outlines that cross or touch, a
//!   height or spacing outside the limits, a placement that is not
//!   finite, a setup that does not exist, a setup without a stock and a
//!   name in use never reach the history.
//! - **The text is a profile of closed shapes.** It is the same `Sketch`
//!   entity that a DXF drawing gives, in millimetres, on the layer
//!   `text`, and it moves with `MoveSketch`. The font file is read once:
//!   the profile keeps the shapes, not the font.
//! - **`apply` and `invert` are pure writes**, keyed by name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{LEGACY_WCS_NAMES, Location, STOCK_NAME, Sketch};
use crate::sketch;
use camcraft_geometry::outline_text::{self, OutlineSpec, MAX_FONT_BYTES};

/// What the caller states for a text in an outline font.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineTextOptions {
    /// The TrueType font file (.ttf, or .otf with TrueType outlines).
    pub font: PathBuf,
    /// The string, on one line.
    pub text: String,
    /// The cap height, mm.
    pub height: f64,
    /// The extra gap between two characters, mm; 0 is the font's own gap.
    pub letter_spacing: f64,
    /// The left end of the baseline on the stock's top face, mm from the
    /// stock origin.
    pub x: f64,
    pub y: f64,
    /// The turn of the baseline about the face normal, degrees,
    /// counterclockwise seen from above.
    pub angle_deg: f64,
    /// The profile's name; `None` is `Text <string>`, made unique.
    pub name: Option<String>,
}

pub struct InsertOutlineText {
    name: String,
    at: Location,
    sketch: Sketch,
}

impl InsertOutlineText {
    /// Read the font, check `options` and make the shapes now, for
    /// `setup` (`None`: the active setup).
    pub fn new(world: &World, setup: Option<&str>, options: &OutlineTextOptions) -> Result<Self, CommandError> {
        let font = read_font(&options.font)?;
        Self::with_font(world, setup, &font, options)
    }

    /// [`InsertOutlineText::new`] over font bytes already in hand; the
    /// path in `options` only names the font in a refusal.
    pub fn with_font(
        world: &World,
        setup: Option<&str>,
        font: &[u8],
        options: &OutlineTextOptions,
    ) -> Result<Self, CommandError> {
        let setup = match setup {
            Some(name) if !crate::setup::exists(world, name) => return Err(CommandError::NotFound(name.to_owned())),
            Some(name) => Some(name.to_owned()),
            None => crate::setup::active(world),
        };
        if sketch::face_frame(world, setup.as_deref()).is_none() {
            return Err(CommandError::Invalid(format!(
                "{} has no stock - make a stock first, then place the text on it",
                setup.as_deref().map_or_else(|| "the document".to_owned(), |s| format!("setup '{s}'"))
            )));
        }
        let OutlineTextOptions { x, y, angle_deg, .. } = *options;
        if !(x.is_finite() && y.is_finite() && angle_deg.is_finite()) {
            return Err(CommandError::Invalid("the text placement must be finite".to_owned()));
        }
        if x.abs() > sketch::MAX_OFFSET || y.abs() > sketch::MAX_OFFSET {
            return Err(CommandError::Invalid(format!(
                "the text placement is more than {} mm from the stock origin",
                sketch::MAX_OFFSET
            )));
        }
        let spec = OutlineSpec { font, text: &options.text, height: options.height, letter_spacing: options.letter_spacing };
        let data = outline_text::sketch(&spec).map_err(|e| match e {
            outline_text::OutlineTextError::Font(_) | outline_text::OutlineTextError::FontTooLarge { .. } => {
                CommandError::Invalid(format!("{}: {e}", options.font.display()))
            }
            other => CommandError::Invalid(other.to_string()),
        })?;
        let name = text_name(world, &options.text, options.name.as_deref())?;
        // One turn is every turn: keep the angle in (-180, 180], as `MoveSketch` does.
        let angle = 180.0 - (180.0 - angle_deg).rem_euclid(360.0);
        Ok(InsertOutlineText { name, at: sketch::on_face(x, y, angle), sketch: Sketch { setup, data: Arc::new(data) } })
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
}

fn read_font(path: &Path) -> Result<Vec<u8>, CommandError> {
    let read_err = |e: std::io::Error| CommandError::Invalid(format!("cannot read the font {}: {e}", path.display()));
    let cap = u64::try_from(MAX_FONT_BYTES).unwrap_or(u64::MAX);
    if std::fs::metadata(path).map_err(read_err)?.len() > cap {
        return Err(CommandError::Invalid(format!("the font {} is larger than the {cap} byte limit", path.display())));
    }
    std::fs::read(path).map_err(read_err)
}

/// A stated name must be free and must not be a setup-furniture name. A
/// name made from the text is made unique (`Text A1`, `Text A1 (2)`).
fn text_name(world: &World, text: &str, stated: Option<&str>) -> Result<String, CommandError> {
    let reserved = |n: &str| n.eq_ignore_ascii_case(STOCK_NAME) || LEGACY_WCS_NAMES.contains(&n) || crate::setup::is_furniture_shape(n);
    if let Some(name) = stated {
        if name.trim().is_empty() || reserved(name) {
            return Err(CommandError::Invalid(format!("'{name}' is not a usable name for a text profile")));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_owned()));
        }
        return Ok(name.to_owned());
    }
    let stem = format!("Text {}", text.trim());
    sketch::free_name(world, &stem).ok_or(CommandError::NameConflict(stem))
}

impl Command for InsertOutlineText {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already there (an idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.at);
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

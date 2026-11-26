//! DXF profiles from the UI (#1863): the Stock page's "Add DXF profile…"
//! door and a viewport drop place a drawing on the active setup's stock
//! through one undoable `ImportDxf`; a drawing that states no unit opens
//! the units prompt and lands only on the user's answer; a placement edit
//! is one `MoveSketch`. Nothing here guesses a unit or writes the world
//! outside the history.
//!
//! Provenance: our own UI state over this repo's #1862 commands
//! (`ImportDxf`, `MoveSketch`); no vendor material consulted.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use camcraft_app::commands::{CommandError, DxfOptions, ImportDxf, MoveSketch};
use camcraft_app::sketch_data::SketchData;
use camcraft_app::units::Unit;

use super::{ErrorScope, State};
use crate::file_dialog::FileDialog;

/// Where a DXF drawing comes from: a file on disk, or the bytes a drop
/// or a browser picker handed over with the file's name.
#[derive(Clone, Debug)]
pub enum DxfSource {
    Path(PathBuf),
    Bytes { name: String, data: Arc<[u8]> },
}

impl DxfSource {
    /// The file name the user knows the drawing by.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            DxfSource::Path(p) => {
                p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
            }
            DxfSource::Bytes { name, .. } => name.clone(),
        }
    }
}

/// A drawing that states no unit, held until the user answers.
#[derive(Clone, Debug)]
pub struct DxfUnitsPrompt {
    pub source: DxfSource,
    /// The drawing's extents in its own (unknown) units.
    pub width: f64,
    pub height: f64,
}

/// The Stock page's placement drafts, one per profile name: X, Y in mm
/// and the angle in degrees. A profile without a draft shows the world.
#[derive(Clone, Debug, Default)]
pub struct SketchUi {
    pub drafts: BTreeMap<String, [f64; 3]>,
}

impl State {
    /// "Add DXF profile…": a native `.dxf` picker, then the import.
    pub fn add_dxf_profile_dialog(&mut self) {
        if self.refuse_in_browser(crate::browser_files::NEEDS_PICKER) {
            return;
        }
        let Some(path) = FileDialog::new().set_title("Add DXF profile").add_filter("DXF", &["dxf"]).pick_file()
        else {
            return;
        };
        self.import_dxf(DxfSource::Path(path), None);
    }

    /// Place `source` on the active setup's stock: one `ImportDxf`. A
    /// drawing with no unit and no `units` opens the units prompt.
    pub fn import_dxf(&mut self, source: DxfSource, units: Option<Unit>) {
        let setup = camcraft_app::setup::active(&self.world);
        let options = DxfOptions { units, ..DxfOptions::default() };
        let built = match &source {
            DxfSource::Path(p) => ImportDxf::new(&self.world, setup.as_deref(), p, &options),
            DxfSource::Bytes { name, data } => ImportDxf::from_bytes(&self.world, setup.as_deref(), name, data, &options),
        };
        match built {
            Ok(cmd) => {
                let line = summary_line(cmd.name(), cmd.setup(), cmd.data());
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                self.status = line;
            }
            Err(CommandError::DxfUnitsMissing { width, height }) => {
                self.status = format!("{}: the drawing states no units - choose Millimeters or Inches", source.label());
                self.dxf_units_prompt = Some(DxfUnitsPrompt { source, width, height });
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("DXF profile not placed: {err}")),
        }
    }

    /// The units prompt's answer: a unit places the held drawing, `None`
    /// (Cancel) drops it and places nothing.
    pub fn resolve_dxf_units(&mut self, units: Option<Unit>) {
        let Some(prompt) = self.dxf_units_prompt.take() else {
            return;
        };
        match units {
            Some(u) => self.import_dxf(prompt.source, Some(u)),
            None => self.status = format!("{} not placed: no units chosen", prompt.source.label()),
        }
    }

    /// Place profile `name` at (`x`, `y`) mm on its stock face, turned
    /// `angle_deg`: one `MoveSketch`.
    pub fn move_sketch(&mut self, name: &str, x: f64, y: f64, angle_deg: f64) {
        match MoveSketch::new(&self.world, name, x, y, angle_deg) {
            Ok(cmd) => {
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                self.sketch_ui.drafts.remove(name);
                self.status = format!("placed {name} at X {x}, Y {y}, {angle_deg} deg on the stock top");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("DXF profile not moved: {err}")),
        }
    }
}

/// The import summary for the status line: units, layers, chains, and
/// what the reader skipped, by entity type.
#[must_use]
pub fn summary_line(name: &str, setup: Option<&str>, data: &SketchData) -> String {
    let units = data.units();
    let unit_note = if units.overridden() { "as chosen" } else { "from the file" };
    let chains = data.chains();
    let skipped = data.skipped_total();
    let kinds: Vec<String> = data.summary().skipped.iter().map(|(k, n)| format!("{k} {n}")).collect();
    let skipped_text =
        if kinds.is_empty() { format!("{skipped} skipped") } else { format!("{skipped} skipped ({})", kinds.join(", ")) };
    let rejected = if chains.rejected.is_empty() { String::new() } else { format!(", {} not chained", chains.rejected.len()) };
    let layers = data.layers().len();
    format!(
        "placed DXF profile {name} on the stock of {}: {:?} {unit_note}, {layers} layer{}, {} closed and {} open chains{rejected}, {skipped_text}",
        setup.map_or_else(|| "the document".to_owned(), |s| format!("setup {s}")),
        units.applied,
        if layers == 1 { "" } else { "s" },
        chains.closed.len(),
        chains.open.len(),
    )
}

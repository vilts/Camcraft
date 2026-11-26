//! The length unit of a STEP file that states none (#1874). The import is
//! refused (`ImportError::StepUnitsMissing`); the window then asks for the
//! unit (`egui_app::step_units_prompt`) and runs the same import again in
//! the chosen unit. A cancel imports nothing. The question is view state:
//! no document change and no history entry until the import lands, and
//! that import is the one undoable command.
//! Provenance: our own wiring over our own `ImportModel::within_in`; no
//! HSMWorks material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::units::Unit;

use super::{State, file_label};

/// A STEP import that waits for its length unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepUnitsAsk {
    /// The file that states no length unit.
    pub path: PathBuf,
    /// The import-as-fixture binding of the refused import, kept for the
    /// import that runs again.
    pub as_fixture: Option<Option<String>>,
}

impl State {
    /// Ask for the length unit of `path` (the import was refused for it).
    pub(super) fn ask_step_units(&mut self, path: PathBuf, as_fixture: Option<Option<String>>) {
        self.status = format!("{} states no length unit - choose the unit it was drawn in", file_label(&path));
        self.step_units_ask = Some(StepUnitsAsk { path, as_fixture });
    }

    /// The answer to the question: import the file again in `unit`, or,
    /// with `None`, import nothing. Without a question it does nothing.
    pub(crate) fn resolve_step_units(&mut self, unit: Option<Unit>) {
        let Some(ask) = self.step_units_ask.take() else { return };
        let Some(unit) = unit else {
            self.status = format!("import of {} cancelled: the file states no length unit", file_label(&ask.path));
            return;
        };
        self.import_as_fixture = ask.as_fixture;
        self.step_units_next = Some((ask.path.clone(), unit));
        self.import_path(ask.path);
    }

    /// The unit chosen for the import of `path`, taken once. Every import
    /// start clears the choice: a unit chosen for one import never waits
    /// for a later import of the same file (#3166 review).
    pub(super) fn take_step_units(&mut self, path: &Path) -> Option<Unit> {
        self.step_units_next.take().and_then(|(p, unit)| (p == path).then_some(unit))
    }
}

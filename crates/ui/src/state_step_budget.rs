//! The STEP facet budget the next import runs under (#1054): an import
//! setting, typed on the model page and read by every STEP import the
//! window starts (Model button, recent list, drag and drop). Setting it is
//! not a document change - nothing in the world moves until an import
//! lands, and that import is the undoable command that records the budget
//! with the model. A typed value outside `step_budget`'s bounds is refused
//! by name and the last good budget stays.
//! Provenance: our own wiring over our own `app::geometry::import::
//! step_budget`; no vendor material.

use std::path::Path;

use camcraft_app::geometry::import::{self, step_budget};

use super::State;

/// The setting and its typed draft.
#[derive(Debug, Clone)]
pub struct StepBudgetUi {
    /// The budget the next STEP import runs under.
    pub value: u64,
    /// What the field shows; committed by [`State::set_step_budget`].
    pub draft: String,
    /// The last refusal, shown under the field until a good value lands.
    pub error: Option<String>,
    /// True once the user committed a budget this session (#1185). Until
    /// then a re-import of a STEP model runs under the budget recorded
    /// with that model, not the launch default.
    pub explicit: bool,
}

impl Default for StepBudgetUi {
    fn default() -> Self {
        StepBudgetUi { value: step_budget::DEFAULT, draft: step_budget::DEFAULT.to_string(), error: None, explicit: false }
    }
}

/// `text` as a budget: digits, with `_`, `,` and spaces read as digit
/// grouping. Anything else, or a number out of bounds, is the refusal.
fn parse(text: &str) -> Result<u64, String> {
    let digits: String = text.chars().filter(|c| !matches!(c, '_' | ',' | ' ')).collect();
    let n = digits
        .parse::<u64>()
        .map_err(|_| format!("{} must be a whole number of facets, got '{}'", step_budget::NAME, text.trim()))?;
    step_budget::check(n).map(|_| n)
}

impl State {
    /// Commit a typed budget. A good one becomes the setting (the field
    /// shows it plainly); a bad one is refused by name on the field and
    /// the status line, and the setting keeps its last good value.
    pub fn set_step_budget(&mut self, text: &str) -> Result<u64, String> {
        match parse(text) {
            Ok(n) => {
                self.step_budget = StepBudgetUi { value: n, draft: n.to_string(), error: None, explicit: true };
                self.status = format!("{}: {n} facets for the next STEP import", step_budget::NAME);
                Ok(n)
            }
            Err(err) => {
                self.step_budget.error = Some(err.clone());
                self.status = format!("refused: {err}");
                Err(err)
            }
        }
    }

    /// The budget an import of `path` asks for: the one the user set this
    /// session when they did (explicit wins, #1185); else the budget the
    /// model of that name was imported under; else `None` (the default;
    /// STL and OBJ are their own mesh and take no budget).
    pub(crate) fn step_budget_for(&self, path: &Path) -> Option<u64> {
        if !matches!(import::format_from_path(path), Ok(import::Format::Step)) {
            return None;
        }
        if self.step_budget.explicit {
            return (self.step_budget.value != step_budget::DEFAULT).then_some(self.step_budget.value);
        }
        camcraft_app::model_fidelity::reimport_budget(&self.world, path, None)
    }
}

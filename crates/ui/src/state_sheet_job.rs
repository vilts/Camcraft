//! A setup's sheet job from the Setups page (#3070): the checklist queues
//! `CommandKind::SetSheetJob`, and the drain lands it here as one undoable
//! `SetSheetJob`; a refusal is the command's own text on the setup's error
//! line. The command sets the ACTIVE setup's stock, so a checklist applied
//! on another setup's page is refused by name, never put on the wrong one.
//! Provenance: our own page law (issue #3070); no vendor material consulted.

use camcraft_app::commands::SetSheetJob;
use camcraft_app::setup;
use camcraft_app::sheet_job::SheetJob;

use super::{ErrorScope, State};

impl State {
    /// Set up (`Some`) or clear (`None`) `name`'s sheet job.
    pub(super) fn set_sheet_job(&mut self, name: &str, job: Option<SheetJob>) {
        if setup::active(&self.world).as_deref() != Some(name) {
            self.refuse(ErrorScope::Setup, format!("sheet job not changed: activate {name} first (the sheet job sets the active setup's stock)"));
            return;
        }
        let cmd = match job {
            Some(j) => SetSheetJob::new(&self.world, j),
            None => SetSheetJob::clear(&self.world),
        };
        match cmd {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = match job {
                    Some(j) => format!("{name}: sheet job set, through-cut allowance {:.2} mm", j.allowance),
                    None => format!("{name} states no sheet job"),
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("sheet job not changed: {err}")),
        }
    }
}

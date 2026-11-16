//! A setup's workholding rpm limit from the Setups page (#1031): the
//! row queues `CommandKind::SetSetupWorkholdingRpm`, and the drain lands
//! it here as one undoable `SetSetupWorkholdingRpm`; a refusal is the
//! command's own text on the setup's error line.
//! Provenance: our own page law (issue #1031); no vendor material consulted.

use camcraft_app::commands::SetSetupWorkholdingRpm;

use super::{ErrorScope, State};

impl State {
    /// State (`Some`) or clear (`None`) `setup`'s workholding maximum rpm.
    pub(super) fn set_workholding_rpm(&mut self, setup: &str, max_rpm: Option<f64>) {
        match SetSetupWorkholdingRpm::new(&self.world, setup, max_rpm) {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = match max_rpm {
                    Some(r) => format!("{setup}: workholding maximum {r:.0} rpm"),
                    None => format!("{setup} states no workholding maximum rpm"),
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("workholding limit not changed: {err}")),
        }
    }
}

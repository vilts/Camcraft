//! The post page's program number and comment (#258): each field's
//! commit rides the undoable `SetProgramInfo`, the other value kept; a
//! typed value the rules refuse is refused by name in the status line.
//! Provenance: our own page law (issue #258); no vendor material consulted.

use camcraft_app::posting;
use camcraft_app::schema::ProgramInfo;

use super::{ErrorScope, State};

impl State {
    /// Commit the typed program number (blank clears it).
    pub(super) fn set_program_number(&mut self, text: &str) {
        let number = match posting::parse_program_number(text) {
            Ok(n) => n,
            Err(err) => return self.refuse(ErrorScope::Setup, format!("program number not set: {err}")),
        };
        let next = ProgramInfo { number, ..posting::program_info(&self.world) };
        self.commit_program_info(next, "program number");
    }

    /// Commit the typed program comment (blank clears it).
    pub(super) fn set_program_comment(&mut self, text: String) {
        let next = ProgramInfo { comment: Some(text), ..posting::program_info(&self.world) };
        self.commit_program_info(next, "program comment");
    }

    fn commit_program_info(&mut self, next: ProgramInfo, what: &str) {
        match camcraft_app::commands::SetProgramInfo::new(&self.world, next) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("{what} set");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("{what} not set: {err}")),
        }
    }
}

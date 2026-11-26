//! The model page's vouch through the queue (#1339): build
//! `VouchModelFidelity`, run it as one labelled undo step, and say on the
//! status line who vouched - or the command's own refusal, by name.
//! Provenance: our own wiring over our own `app::commands::
//! VouchModelFidelity`; no vendor material.

use camcraft_app::commands::VouchModelFidelity;

use super::{ErrorScope, State};

impl State {
    pub(crate) fn vouch_model(&mut self, name: &str, by: &str) {
        match VouchModelFidelity::new(&self.world, name, by) {
            Ok(command) => {
                let copies = command.copies().len();
                let who = command.by().to_string();
                command.run(&mut self.history, &self.world);
                self.doc_version += 1;
                self.status = match copies {
                    0 => format!("{who} vouched for the mesh of {name}"),
                    n => format!("{who} vouched for the mesh of {name} and {n} stock copy(ies) of it"),
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("{name} not vouched for: {err}")),
        }
    }
}

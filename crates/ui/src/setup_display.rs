//! Which setups the viewport draws (#1494): the active setup always, any
//! other setup only while the user has turned its browser eye on. Display
//! only - like the other browser eyes it is view state (not undoable, not
//! saved) and never reaches posting, simulation or collision checks: a
//! setup's operations, fixtures and work offset keep cutting and being
//! judged whether or not they are drawn.
//! Provenance: our own view-state design (issue #1494); no vendor material.

use camcraft_app::{fixture, model, setup};

use crate::state::State;

impl State {
    /// Is `name` drawn: the active setup always, another only when its eye
    /// is on. A document without setups has nothing to hide.
    #[must_use]
    pub(crate) fn setup_displayed(&self, name: &str) -> bool {
        setup::active(&self.world).as_deref() == Some(name) || self.shown_setups.iter().any(|n| n == name)
    }

    /// Flip another setup's eye. The active setup is always drawn, so its
    /// eye changes nothing.
    pub(crate) fn toggle_setup_display(&mut self, name: &str) {
        if setup::active(&self.world).as_deref() != Some(name) {
            Self::toggle_visibility(&mut self.shown_setups, name);
        }
    }

    /// The setups not drawn right now (empty without setups).
    fn undisplayed_setups(&self) -> Vec<String> {
        setup::names(&self.world).into_iter().filter(|n| !self.setup_displayed(n)).collect()
    }

    /// Does this operation's path stay undrawn because its setup is?
    #[must_use]
    pub(crate) fn op_hidden_by_setup(&self, name: &str) -> bool {
        let off = self.undisplayed_setups();
        if off.is_empty() {
            return false;
        }
        let Some(entity) = model::find_by_name(&self.world, name) else {
            return false;
        };
        let Some(op) = model::operation_of(&self.world, &entity) else {
            return false;
        };
        setup::effective(&self.world, op.setup.as_deref()).is_some_and(|s| off.contains(&s))
    }

    /// Does this work-offset gizmo stay undrawn because its setup is?
    #[must_use]
    pub(crate) fn wcs_hidden_by_setup(&self, name: &str) -> bool {
        self.undisplayed_setups().iter().any(|s| setup::wcs_entity_name(&self.world, Some(s)) == name)
    }

    /// Does this fixture stay undrawn (and unpickable) because its setup is?
    #[must_use]
    pub(crate) fn fixture_hidden_by_setup(&self, name: &str) -> bool {
        self.undisplayed_setups().iter().any(|s| fixture::tagged(&self.world, Some(s)).iter().any(|(n, _)| n == name))
    }
}

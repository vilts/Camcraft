//! A setup's own post options and its tool-list export (#389). The post
//! page's option widgets (skip pauses and homes, Output, threading) queue
//! the same `CommandKind`s as before; they land on the ACTIVE setup's own
//! options (`SetSetupPostOptions`) when it owns them and on the document's
//! (`SetPostOptions`) when it follows - the `state::setup_mount` routing
//! law, worn by the options. With two or more setups (#1526, the #1515
//! law) an edit on a setup that follows the document becomes THAT
//! setup's own options instead: the document's are what every other
//! following setup posts with, so writing them from one setup would
//! silently change another setup's program. Every arm is one undoable
//! command. The tool
//! list export is a read of the document plus a file write: no command,
//! no history (the setup-sheet law).
//! The setup's stated nose-to-rotary figure (#821) lands here too: one
//! undoable `SetSetupNoseToAxis`.
//! Provenance: our own routing law (issues #389, #1526); no vendor material consulted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use camcraft_app::commands::{
    CommandError, PostSettingsEdit, SetPostOptions, SetPostScriptOptions, SetPostSettings,
    SetSetupNoseToAxis, SetSetupPostOptions, SetSetupPostScriptOptions,
};
use camcraft_app::post::options::OptionValue;
use camcraft_app::schema::{PostOptions, PostRef};
use camcraft_app::{setup, setup_post_options, tool_list_export};

use super::{ErrorScope, State};

impl State {
    /// The active setup when it owns its post options: the setup the
    /// option widgets edit. `None` - they edit the document's.
    #[must_use]
    pub fn post_options_owner(&self) -> Option<String> {
        let active = setup::active(&self.world)?;
        let owns = setup_post_options::own(&self.world, Some(&active)).is_some()
            || setup_post_options::script_own(&self.world, Some(&active)).is_some();
        owns.then_some(active)
    }

    /// The setup an option edit writes: the active setup when it owns its
    /// post options, or when the document has more than one setup (#1526 -
    /// the edit becomes its own, so no other setup's program changes).
    /// `None` - the document's options (no setups, or the only one).
    #[must_use]
    pub fn post_options_target(&self) -> Option<String> {
        self.post_options_owner().or_else(|| self.shared_mount_target())
    }

    /// Set the post options in force on the setup [`Self::post_options_target`]
    /// names, else the document's. `done` is the status on success.
    pub(super) fn route_post_options(&mut self, next: PostOptions, done: &str, refused: &str, scope: ErrorScope) {
        let owner = self.post_options_target();
        let executed: Result<(), CommandError> = match &owner {
            Some(s) => SetSetupPostOptions::new(&self.world, s, Some(next)).map(|c| self.history.execute(&self.world, c)),
            None => SetPostOptions::new(&self.world, next).map(|c| self.history.execute(&self.world, c)),
        };
        match executed {
            Ok(()) => {
                self.doc_version += 1;
                self.status = match owner {
                    Some(s) => format!("{done} ({s})"),
                    None => done.to_owned(),
                };
            }
            Err(err) => self.refuse(scope, format!("{refused}: {err}")),
        }
    }

    /// Route per-post script overrides to the chosen setup or the document (#258).
    pub(super) fn route_post_script_options(
        &mut self,
        setup: Option<&str>,
        post: &PostRef,
        overrides: BTreeMap<String, OptionValue>,
    ) {
        let owner = setup.map(str::to_owned).or_else(|| self.post_options_target());
        let executed: Result<(), CommandError> = match &owner {
            Some(s) => SetSetupPostScriptOptions::for_setup_post(&self.world, s, post, overrides)
                .map(|c| self.history.execute(&self.world, c)),
            None => SetPostScriptOptions::for_post(&self.world, post, overrides)
                .map(|c| self.history.execute(&self.world, c)),
        };
        match executed {
            Ok(()) => {
                self.doc_version += 1;
                if self.feedback.as_ref().is_some_and(|(s, _)| *s == ErrorScope::Setup) {
                    self.feedback = None;
                }
                self.status = match owner {
                    Some(s) => format!("post script options set ({s})"),
                    None => String::from("post script options set"),
                };
            }
            Err(err) => self.refuse(
                ErrorScope::Setup,
                format!("post script options not set: {err}"),
            ),
        }
    }

    /// Apply combined Post dialog edits as one undo unit, optionally posting afterwards (#258).
    pub(super) fn apply_post_dialog(
        &mut self,
        edit: PostSettingsEdit,
        post_path: Option<PathBuf>,
        per_op_dir: Option<PathBuf>,
        program_prefix: Option<String>,
    ) {
        match SetPostSettings::new(&self.world, edit) {
            Ok(cmd) => {
                if !cmd.is_empty() {
                    self.history.execute(&self.world, cmd);
                    self.doc_version += 1;
                    self.status = String::from("post settings applied");
                }
                if self.feedback.as_ref().is_some_and(|(s, _)| *s == ErrorScope::Setup) {
                    self.feedback = None;
                }
                if let Some(dir) = per_op_dir {
                    let program = program_prefix.unwrap_or_else(|| self.program_name());
                    self.post_program_folder_named(dir, &program);
                    if self.status.starts_with("post refused:") {
                        if let Some(d) = self.post_dialog.as_mut() {
                            d.error = Some(self.status.clone());
                        }
                    } else {
                        self.post_dialog = None;
                    }
                } else if let Some(path) = post_path {
                    self.post_program_path(path);
                    if let Some(refusal) = &self.post_refusal {
                        if let Some(d) = self.post_dialog.as_mut() {
                            d.error = Some(refusal.reason.clone());
                        }
                    } else {
                        self.post_dialog = None;
                    }
                } else {
                    self.post_dialog = None;
                }
            }
            Err(err) => {
                let msg = format!("post settings not applied: {err}");
                if let Some(d) = self.post_dialog.as_mut() {
                    d.error = Some(msg.clone());
                }
                self.refuse(ErrorScope::Setup, msg);
            }
        }
    }

    /// A setup takes its own post options (a copy of the ones it posts
    /// with now, so nothing it posts changes) or follows the document's.
    pub(super) fn own_post_options(&mut self, setup: &str, own: bool) {
        let edit = if own {
            PostSettingsEdit {
                setup: Some(setup.to_owned()),
                post_options: Some(setup_post_options::of(&self.world, Some(setup))),
                script_replace: Some(setup_post_options::script_of(&self.world, Some(setup))),
                ..Default::default()
            }
        } else {
            PostSettingsEdit {
                setup: Some(setup.to_owned()),
                follow_document: true,
                ..Default::default()
            }
        };
        match SetPostSettings::new(&self.world, edit) {
            Ok(c) => {
                if c.is_empty() {
                    let msg = if own {
                        format!("setup '{setup}' already has its own post options")
                    } else {
                        format!("setup '{setup}' already follows the document's post options")
                    };
                    self.refuse(ErrorScope::Setup, format!("post options not changed: {msg}"));
                    return;
                }
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = if own {
                    format!("{setup} has its own post options")
                } else {
                    format!("{setup} follows the document's post options")
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("post options not changed: {err}")),
        }
    }

    /// #821: `setup` states its own nose-to-rotary figure (mm), or clears it.
    pub(super) fn set_setup_nose(&mut self, setup: &str, value: Option<f64>) {
        match SetSetupNoseToAxis::new(&self.world, setup, value) {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = match value {
                    Some(v) => format!("{setup} states nose_to_axis {v:.3} mm"),
                    None => format!("{setup} judges its rotary from the machine's nose_to_axis less its stack"),
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("nose_to_axis not changed: {err}")),
        }
    }

    /// Ask where to write `setup`'s tool list, then write it.
    pub(super) fn tool_list_export_dialog(&mut self, setup: Option<String>) {
        let stem = setup.clone().or_else(|| setup::effective(&self.world, None)).unwrap_or_else(|| "setup".to_owned());
        let picked: Option<PathBuf> = crate::file_dialog::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .add_filter("Text", &["txt"])
            .set_file_name(format!("{stem}-tools.csv"))
            .save_file();
        if let Some(path) = picked {
            self.tool_list_export_path(setup.as_deref(), &path);
        }
    }

    /// Write `setup`'s tool list to `path`: CSV for `.csv`, text
    /// otherwise. A pure read of the document; the status line is the
    /// only effect.
    pub(super) fn tool_list_export_path(&mut self, setup: Option<&str>, path: &Path) {
        let text = tool_list_export::for_path(&self.world, setup, path);
        let count = tool_list_export::rows(&self.world, setup).len();
        match std::fs::write(path, text) {
            Ok(()) => self.status = format!("tool list ({count} tool(s)) written to {}", path.display()),
            Err(err) => self.status = format!("tool list write failed: {err}"),
        }
    }
}

//! Save, Save As and Post in the browser build (#1431): the same project
//! text and the same gated program the desktop writes, handed to the
//! browser as a download instead of a path; and the refusal, by name, of
//! every file command a page cannot run yet.
//! Provenance: our own law (issue #1431); no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::{posting, project};

use super::{CommandKind, PostView, State};
use crate::browser_files;

impl State {
    /// In the browser build, `action`'s refusal: the status line names
    /// why and the command does nothing else. `false` lets it run.
    pub(super) fn refused_in_browser(&mut self, action: &CommandKind) -> bool {
        match self.browser_refusal_of(action) {
            Some(why) => {
                diag_info!("{action:?} refused in the browser: {why}");
                self.status = why.to_owned();
                true
            }
            None => false,
        }
    }

    /// Why `action` cannot run here: `None` on the desktop, and in the
    /// browser for every command a page can run.
    #[must_use]
    pub fn browser_refusal_of(&self, action: &CommandKind) -> Option<&'static str> {
        if !self.browser_build {
            return None;
        }
        browser_files::refusal(action, posting::post_options(&self.world).per_op)
    }

    /// `why` in the browser build, `None` on the desktop: the reason a
    /// file button that runs no `CommandKind` is greyed with.
    #[must_use]
    pub fn in_browser(&self, why: &'static str) -> Option<&'static str> {
        self.browser_build.then_some(why)
    }

    /// The status-line refusal of a direct file call in the browser;
    /// `true` when it was refused.
    pub(crate) fn refuse_in_browser(&mut self, why: &'static str) -> bool {
        match self.in_browser(why) {
            Some(why) => {
                self.status = why.to_owned();
                true
            }
            None => false,
        }
    }

    /// The download name the document saves under: its project's name,
    /// else `untitled.camproj`.
    fn project_download_name(&self) -> String {
        let name = self.project_path.as_deref().and_then(Path::file_name).and_then(|n| n.to_str());
        browser_files::file_name(name.unwrap_or("untitled.camproj"), "camproj")
    }

    /// Save (`ask` = Save As: a name first) as a download. `true` when the
    /// project went out; the document is clean only then.
    pub(crate) fn download_project(&mut self, ask: bool) -> bool {
        let mut name = self.project_download_name();
        if ask {
            let Some(answer) = browser_files::ask_name(&name) else {
                self.status = String::from("save cancelled");
                return false;
            };
            name = browser_files::file_name(&answer, "camproj");
        }
        let text = match project::save_text(&self.world) {
            Ok(text) => text,
            Err(err) => {
                self.status = format!("save failed: {err}");
                return false;
            }
        };
        if let Err(err) = browser_files::offer(&name, text.as_bytes(), "application/json") {
            self.status = format!("save failed: {err}");
            return false;
        }
        // What the download holds matches the history position (#107).
        self.history.mark_clean();
        self.status = format!("saved {name} (downloaded)");
        self.project_path = Some(PathBuf::from(name));
        true
    }

    /// Post the one combined program as a download, or be refused by name
    /// - the same gate as the desktop post, which offers nothing on refusal.
    pub(crate) fn download_program(&mut self) {
        let name = browser_files::file_name(&crate::post_file::for_document(self).file_name, "nc");
        let posted = posting::post_setup_unwritten(&self.world)
            .and_then(|p| browser_files::offer(&name, p.program.as_bytes(), "text/plain").map(|()| p));
        match posted {
            Ok(posted) => {
                self.post_refusal = None;
                self.status = format!(
                    "posted {} op(s), {} lines in {} as {name} (downloaded)",
                    posted.ops,
                    posted.lines,
                    posting::unit_word(self.units())
                );
                if let Some(line) = &posted.not_judged {
                    self.status = format!("{} - WARNING: {line}", self.status);
                }
                self.post_view = Some(PostView {
                    path: PathBuf::from(name),
                    program: posted.program,
                    line_pos: posted.line_pos,
                    open: true,
                    line: 0,
                    blocks: posted.blocks,
                    trace: false,
                    not_judged: posted.not_judged,
                });
            }
            Err(reason) => {
                self.status = format!("post refused: {reason}");
                self.post_refusal = Some(crate::egui_app::post_refusal::PostRefusal {
                    path: PathBuf::from(name),
                    reason,
                    doc_version: self.doc_version,
                });
            }
        }
    }
}

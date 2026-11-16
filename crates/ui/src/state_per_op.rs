//! The post page's Output choice and the per-operation post (#111):
//! the switch rides `SetPostOptions` (one undo unit), and a per-op
//! post asks for a folder instead of a file. A program whose setups run
//! on different machines or posts writes one file per mount (#388).
//! Provenance: our own page law (issue #111); no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::posting;

use super::{ErrorScope, PostView, State};

impl State {
    /// The Output choice: `true` writes one file per operation.
    pub(super) fn set_per_op_output(&mut self, per_op: bool) {
        // #389: routed to the active setup's own options when it has them.
        let next = camcraft_app::schema::PostOptions { per_op, ..posting::post_options(&self.world) };
        let done = if per_op { "post writes one file per operation" } else { "post writes one combined program" };
        self.route_post_options(next, done, "post options not changed", ErrorScope::Operation);
    }

    /// The program name a per-op post stamps on its files: the
    /// project's file stem, or `program` for an unsaved document.
    #[must_use]
    pub fn program_name(&self) -> String {
        self.project_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|s| s.to_str())
            .unwrap_or("program")
            .to_owned()
    }

    /// Post one file per operation into `dir`, or be refused by name -
    /// writing nothing (posting's own law). The viewer shows the first
    /// file; the status line names how many were written and where.
    pub(super) fn post_program_folder(&mut self, dir: PathBuf) {
        let program = self.program_name();
        match posting::post_per_op(&self.world, &dir, &program) {
            Ok(files) => {
                let lines: usize = files.iter().map(|f| f.posted.lines).sum();
                self.status = format!(
                    "posted {} file(s), one per operation, {lines} lines in {} to {}",
                    files.len(),
                    posting::unit_word(self.units()),
                    dir.display()
                );
                if let Some(line) = files.first().and_then(|f| f.posted.not_judged.clone()) {
                    self.status = format!("{} - WARNING: {line}", self.status);
                }
                if let Some(first) = files.into_iter().next() {
                    let p = first.posted;
                    self.post_view = Some(PostView {
                        path: first.path,
                        program: p.program,
                        line_pos: p.line_pos,
                        open: true,
                        line: 0,
                        blocks: p.blocks,
                        trace: false,
                        not_judged: p.not_judged,
                    });
                }
            }
            Err(err) => self.status = format!("post refused: {err}"),
        }
    }

    /// A per-mount post's outcome (#388): the status line names every
    /// file and its setups; the viewer shows the first.
    pub(super) fn show_posted_mounts(&mut self, files: Vec<posting::PostedMount>) {
        let names: Vec<String> = files
            .iter()
            .map(|f| {
                let file = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("{file} ({})", f.mount.setups.join(", "))
            })
            .collect();
        self.status = format!("posted {} file(s) in {}, one per machine and post: {}", files.len(), posting::unit_word(self.units()), names.join("; "));
        if let Some(line) = files.first().and_then(|f| f.posted.not_judged.clone()) {
            self.status = format!("{} - WARNING: {line}", self.status);
        }
        if let Some(first) = files.into_iter().next() {
            let p = first.posted;
            self.post_view = Some(PostView {
                path: first.path,
                program: p.program,
                line_pos: p.line_pos,
                open: true,
                line: 0,
                blocks: p.blocks,
                trace: false,
                not_judged: p.not_judged,
            });
        }
    }

    /// The post's front door: a folder when the Output is one file per
    /// operation, the program file otherwise.
    pub(super) fn post_program_dialog(&mut self) {
        // #258: both pickers open where this document last posted, else
        // beside the saved project (`post_file::for_document`).
        if self.browser_build {
            // #1431: the browser downloads the one combined program (a
            // per-op post was refused before it got here).
            self.download_program();
            return;
        }
        let target = crate::post_file::for_document(self);
        if posting::post_options(&self.world).per_op {
            let mut picker = crate::file_dialog::FileDialog::new().set_title("Post one file per operation into");
            if let Some(dir) = &target.dir {
                picker = picker.set_directory(dir);
            }
            if let Some(dir) = picker.pick_folder() {
                self.post_program_folder(dir);
            }
        } else if let Some(path) = super::post_program_dialog(&target) {
            self.post_program_path(path);
        }
    }
}

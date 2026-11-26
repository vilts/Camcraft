//! The post page's Output choice and the per-operation post (#111):
//! the switch rides `SetPostOptions` (one undo unit), and a per-op
//! post asks for a folder instead of a file. A program whose setups run
//! on different machines or posts writes one file per mount (#388).
//! Provenance: our own page law (issue #111); no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::posting;

use super::{ErrorScope, PostView, State};

impl State {
    pub(super) fn create_agie_inspection(
        &mut self,
        name: String,
        tool: String,
        drawing: String,
        chain: camcraft_app::geometry::sketch_chains::ChainId,
        start: [f64; 2],
        setup: Option<String>,
    ) {
        if camcraft_app::setup::active(&self.world) != setup {
            self.refuse(ErrorScope::Operation, "The active setup changed. Select the inspection opening again.".into());
            return;
        }
        match camcraft_app::commands::CreateOperation::agie_inspection(&self.world, &name, &tool, &drawing, chain, start) {
            Ok(cmd) => {
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                self.pending.push_back(super::CommandKind::Select(Some(name.clone())));
                self.ui_target = super::UiTarget::Operation(name.clone());
                self.status = format!("{name} created for nominal inspection with no machining condition");
            }
            Err(why) => self.refuse(ErrorScope::Operation, format!("Inspection opening refused: {why}")),
        }
    }

    pub(super) fn import_agie_drawing(&mut self) {
        let Some(path) = crate::file_dialog::FileDialog::new().set_title("Import inspection drawing")
            .add_filter("DXF", &["dxf"]).pick_file() else { return; };
        let setup = camcraft_app::setup::active(&self.world);
        match camcraft_app::commands::ImportDxf::new(&self.world, setup.as_deref(), &path, &Default::default()) {
            Ok(cmd) => {
                let name = cmd.name().to_owned();
                self.history.execute(&self.world, cmd);
                self.doc_version += 1;
                self.status = format!("Imported DXF drawing {name}");
            }
            Err(why) => self.refuse(ErrorScope::Operation, format!("Drawing import refused: {why}")),
        }
    }

    pub(super) fn export_agie_inspection(&mut self, name: &str) {
        let Some(path) = crate::file_dialog::FileDialog::new()
            .set_title("New Agie inspection directory • physical stock XY unknown (length and width 0)")
            .set_file_name("INSPECT")
            .save_file()
        else {
            return;
        };
        self.export_agie_inspection_to(name, &path);
    }

    pub(crate) fn export_agie_inspection_to(&mut self, name: &str, path: &Path) {
        self.status = match camcraft_app::operation::export_agie_inspection(&self.world, name, path) {
            Ok(r) => format!(
                "{} {} • {} nominal spans • chain {} • {}",
                r.review, path.display(), r.nominal_blocks, r.chain, r.setup
            ),
            Err(why) => format!("Agie inspection export refused: {why}"),
        };
    }

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
        self.post_program_folder_named(dir, &program);
    }

    /// Post with the program prefix entered in the Post dialog.
    pub(super) fn post_program_folder_named(&mut self, dir: PathBuf, program: &str) {
        match posting::post_per_op(&self.world, &dir, program) {
            Ok(files) => {
                self.post_last_dir = Some(dir.clone());
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
        if self.browser_build {
            // #1431: the browser downloads the one combined program (a
            // per-op post was refused before it got here).
            self.download_program();
            return;
        }
        self.post_dialog = Some(crate::egui_app::post_dialog::PostDialogState::open(self));
    }
}

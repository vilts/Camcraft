//! Floating desktop "Post" dialog (#258): target folder & file name,
//! program number & comment, built-in post switches, grouped per-post
//! `declaredOptions`, and an Operations summary tab.
//!
//! Provenance: our own design over `SetPostSettings` and `layout::form`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use camcraft_app::commands::PostSettingsEdit;
use camcraft_app::post::options::{DeclaredOption, OptionValue};
use camcraft_app::schema::{PostOptions, PostRef, ProgramInfo};
use camcraft_app::{posting, setup_post_options};
use egui::{RichText, ScrollArea, Window};

use super::colorvision::text_inks;
use super::contrast;
use super::layout::{Action, ActionKind, BarInks, action_bar};
use super::post_dialog_tabs::{draw_operations_tab, draw_output_section, draw_program_section};
use super::post_script_options;
use super::theme;
use crate::state::{CommandKind, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostDialogTab {
    Settings,
    Operations,
}

#[derive(Debug, Clone)]
pub struct PostDialogState {
    pub tab: PostDialogTab,
    pub setup_target: Option<String>,
    pub post: PostRef,
    pub end_units_words: Option<(String, String)>,
    pub target_dir: String,
    pub file_name: String,
    pub program_number: String,
    pub program_comment: String,
    pub options: PostOptions,
    pub schema: Vec<DeclaredOption>,
    pub baseline: BTreeMap<String, OptionValue>,
    pub values: BTreeMap<String, OptionValue>,
    pub reset_script: bool,
    pub error: Option<String>,
}

impl PostDialogState {
    pub fn open(state: &State) -> Self {
        let setup_target = state.post_options_target();
        // Options can follow the document while the active setup owns its post.
        // Keep the write target separate from the post whose schema is edited.
        let post = posting::post_of(&state.world);
        let end_units_words = posting::end_units_words(&post).ok().flatten();
        let schema = posting::declared_options_for_post(&post).unwrap_or_default();
        let baseline =
            setup_post_options::script_for_post(&state.world, setup_target.as_deref(), &post);
        let values = post_script_options::resolved_values(&schema, &baseline);
        let info = posting::program_info(&state.world);
        let options = match setup_target.as_deref() {
            Some(s) => setup_post_options::of(&state.world, Some(s)),
            None => setup_post_options::document(&state.world),
        };
        let mut target = crate::post_file::dialog_target(state);
        if posting::agie::is_native(&post) && !posting::agie::valid_name(&target.file_name) {
            target.file_name = "WIREJOB".into();
        }
        Self {
            tab: PostDialogTab::Settings,
            setup_target,
            post,
            end_units_words,
            target_dir: target
                .dir
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            file_name: target.file_name,
            program_number: info.number.map_or_else(String::new, |n| n.to_string()),
            program_comment: info.comment.unwrap_or_default(),
            options,
            schema,
            baseline,
            values,
            reset_script: false,
            error: None,
        }
    }

    fn build_edit(&self) -> Result<PostSettingsEdit, String> {
        let number = if self.program_number.trim().is_empty() {
            None
        } else {
            let n: u32 = self.program_number.trim().parse().map_err(|_| {
                "Program number must be an integer between 1 and 9999".to_owned()
            })?;
            Some(n)
        };
        let trimmed_comment = self.program_comment.trim();
        let info = ProgramInfo {
            number,
            comment: (!trimmed_comment.is_empty()).then(|| trimmed_comment.to_owned()),
        };
        posting::check_program_info(&info)?;
        let script_overrides = if self.reset_script {
            BTreeMap::new()
        } else {
            post_script_options::overrides_from_values(&self.schema, &self.values, &self.baseline)
        };
        Ok(PostSettingsEdit {
            setup: self.setup_target.clone(),
            program_info: Some(info),
            post_options: Some(self.options),
            follow_document: false,
            script_post: Some(self.post.clone()),
            script_overrides: Some(script_overrides),
            script_replace: None,
        })
    }
}

/// Draw the desktop Post dialog when open (`State::post_dialog`).
pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(mut dlg) = state.post_dialog.take() else {
        return;
    };
    let mut open = true;
    let mut clicked_action: Option<usize> = None;
    let pal = theme::current();

    Window::new("Post")
        .id(egui::Id::new("post-dialog"))
        .open(&mut open)
        .default_size([560.0, 660.0])
        .show(ctx, |ui| {
            let inks = text_inks(ui, &state.world);
            let bar_inks = BarInks {
                accent: pal.accent,
                on_accent: contrast::on_fill(pal.accent),
                danger: inks.error,
            };
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dlg.tab, PostDialogTab::Settings, "Settings");
                ui.selectable_value(&mut dlg.tab, PostDialogTab::Operations, "Operations");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // #2292: show the shipped post script's name in the scope
                    // line, with its full path on hover. The header row must
                    // stay narrow so long install paths do not push the tab
                    // buttons off-screen (the path varies per checkout).
                    let post_label = posting::post_ref_label(&dlg.post);
                    let hover_path = posting::post_path(&dlg.post);
                    let scope = match &dlg.setup_target {
                        Some(s) => format!("Setup: {s} ({post_label})"),
                        None => post_label.to_string(),
                    };
                    let response = ui.label(RichText::new(scope).weak().small());
                    if let Some(path) = hover_path {
                        response.on_hover_text(path);
                    }
                });
            });
            ui.separator();

            if let Some(err) = &dlg.error {
                ui.colored_label(inks.error, err);
                ui.add_space(4.0);
            }

            let max_h = (ctx.content_rect().height() * 0.78).max(240.0);
            ScrollArea::vertical()
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // #3384: the floating scroll bar lies over the right edge of
                    // the content. Keep a gutter, so that a right-aligned
                    // checkbox stays clickable when the options overflow.
                    super::layout::keep_scroll_gutter(ui, 0.0);
                    match dlg.tab {
                        PostDialogTab::Settings => {
                            draw_output_section(ui, &mut dlg);
                            draw_program_section(ui, &mut dlg);
                            let changed = post_script_options::render_draft(
                                ui,
                                "post_dlg_script",
                                &dlg.schema,
                                &mut dlg.values,
                                !dlg.baseline.is_empty(),
                                true,
                                &mut dlg.reset_script,
                            );
                            if changed && !dlg.values.iter().all(|(k, v)| {
                                dlg.schema.iter().find(|d| &d.key == k).is_some_and(|d| &d.default == v)
                            }) {
                                dlg.reset_script = false;
                            }
                        }
                        PostDialogTab::Operations => draw_operations_tab(ui, state),
                    }
                });

            let bar = [
                Action {
                    label: "Post…",
                    kind: ActionKind::Primary,
                    enabled: true,
                    hint: Some("Apply settings and write the NC program"),
                    why_not: None,
                },
                Action {
                    label: "Apply",
                    kind: ActionKind::Secondary,
                    enabled: true,
                    hint: Some("Save post settings to this document as one undo step"),
                    why_not: None,
                },
                Action {
                    label: "Cancel",
                    kind: ActionKind::Secondary,
                    enabled: true,
                    hint: None,
                    why_not: None,
                },
            ];
            clicked_action = action_bar(ui, &bar, &bar_inks);
        });

    if !open || clicked_action == Some(2) {
        return;
    }

    if let Some(action @ (0 | 1)) = clicked_action {
        let edit = match dlg.build_edit() {
            Ok(e) => e,
            Err(e) => {
                dlg.error = Some(e);
                state.post_dialog = Some(dlg);
                return;
            }
        };
        dlg.error = None;
        if action == 1 {
            remember_target(state, &dlg);
            state.post_dialog = Some(dlg);
            state.pending.push_back(CommandKind::ApplyPostDialog {
                edit,
                post_path: None,
                per_op_dir: None,
                program_prefix: None,
            });
            ctx.request_repaint();
            return;
        }
        queue_post_after_apply(state, dlg, edit);
        if !state.pending.is_empty() {
            ctx.request_repaint();
        }
        return;
    }

    state.post_dialog = Some(dlg);
}

fn remember_target(state: &mut State, dlg: &PostDialogState) {
    state.post_last_dir = (!dlg.target_dir.trim().is_empty())
        .then(|| PathBuf::from(dlg.target_dir.trim()));
    state.post_last_name = (!dlg.file_name.trim().is_empty())
        .then(|| dlg.file_name.trim().to_owned());
}

fn queue_post_after_apply(
    state: &mut State,
    mut dlg: PostDialogState,
    edit: PostSettingsEdit,
) {
    let dir_trim = dlg.target_dir.trim();
    if posting::agie::is_native(&dlg.post) {
        if !posting::agie::valid_name(&dlg.file_name) {
            dlg.error = Some("Enter a job name of up to eight uppercase letters or digits, starting with a letter".into());
            state.post_dialog = Some(dlg);
            return;
        }
        if dlg.options.per_op {
            dlg.error = Some("Choose One program to create a native wire EDM job directory".into());
            state.post_dialog = Some(dlg);
            return;
        }
        let parent = if dir_trim.is_empty() {
            crate::file_dialog::FileDialog::new().set_title("Choose a parent folder for the native wire EDM job").pick_folder()
        } else {
            Some(PathBuf::from(dir_trim))
        };
        if let Some(parent) = parent {
            let path = parent.join(&dlg.file_name);
            dlg.target_dir = parent.display().to_string();
            state.pending.push_back(CommandKind::ApplyPostDialog {
                edit,
                post_path: Some(path),
                per_op_dir: None,
                program_prefix: None,
            });
        }
        state.post_dialog = Some(dlg);
        return;
    }
    if dlg.options.per_op {
        let picked = if !dir_trim.is_empty() {
            Some(PathBuf::from(dir_trim))
        } else {
            crate::file_dialog::FileDialog::new()
                .set_title("Post one file per operation into")
                .pick_folder()
        };
        if let Some(dir) = picked {
            dlg.target_dir = dir.display().to_string();
            state.post_last_dir = Some(dir.clone());
            let program_prefix = Some(dlg.file_name.trim().to_owned());
            state.post_last_name = program_prefix.clone();
            state.post_dialog = Some(dlg);
            state.pending.push_back(CommandKind::ApplyPostDialog {
                edit,
                post_path: None,
                per_op_dir: Some(dir),
                program_prefix,
            });
            return;
        }
        state.post_dialog = Some(dlg);
        return;
    }
    let ext = crate::post_file::post_extension(state);
    let name_trim = dlg.file_name.trim();
    let fname = if name_trim.is_empty() {
        format!("program.{ext}")
    } else {
        name_trim.to_owned()
    };
    let picked = if !dir_trim.is_empty() {
        Some(PathBuf::from(dir_trim).join(&fname))
    } else {
        let target = crate::post_file::PostTarget {
            dir: None,
            file_name: fname,
        };
        crate::state::save_program_dialog(&target, &ext)
    };
    if let Some(path) = picked {
        state.post_last_dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from);
        state.post_last_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_owned);
        state.post_dialog = Some(dlg);
        state.pending.push_back(CommandKind::ApplyPostDialog {
            edit,
            post_path: Some(path),
            per_op_dir: None,
            program_prefix: None,
        });
    } else {
        state.post_dialog = Some(dlg);
    }
}

//! Tab sections (`Settings` and `Operations`) for the desktop Post dialog (#258).
//!
//! Provenance: our own design split from `post_dialog` at the 400-line law.

use camcraft_app::schema::{BetweenSetups, EndUnits, ThreadingOutput};
use camcraft_app::{model, operation, posting, setup, setup_post_options};
use egui::{ComboBox, RichText, TextEdit, Ui, WidgetInfo, WidgetType};

use super::colorvision::text_inks;
use super::fields;
use super::layout::{check_row, group, hint, prop_row, prop_row_hint};
use super::post_dialog::PostDialogState;
use crate::state::State;

pub(super) fn draw_output_section(ui: &mut Ui, dlg: &mut PostDialogState) {
    group(ui, "Output target", None, |ui| {
        prop_row(ui, "Output folder", |ui| {
            if ui.button("Browse…").clicked()
                && let Some(dir) = crate::file_dialog::FileDialog::new()
                    .set_title("Choose NC output folder")
                    .pick_folder()
            {
                dlg.target_dir = dir.display().to_string();
            }
            fields::named(
                ui.add(
                    TextEdit::singleline(&mut dlg.target_dir)
                        .desired_width(210.0)
                        .hint_text("ask on Post…"),
                ),
                "Output folder",
            );
        });
        let name_label = if dlg.options.per_op {
            "Program prefix"
        } else {
            "Program file"
        };
        prop_row(ui, name_label, |ui| {
            fields::named(
                ui.add(TextEdit::singleline(&mut dlg.file_name).desired_width(210.0)),
                name_label,
            );
        });
    });
}

pub(super) fn draw_program_section(ui: &mut Ui, dlg: &mut PostDialogState) {
    group(ui, "Program", None, |ui| {
        prop_row_hint(ui, "Program number", "O0001 when blank (1 to 9999)", |ui| {
            fields::named(
                ui.add(
                    TextEdit::singleline(&mut dlg.program_number)
                        .desired_width(90.0)
                        .hint_text("1"),
                ),
                "Program number",
            );
        });
        prop_row_hint(
            ui,
            "Program comment",
            "Header comment or program name on Heidenhain/SINUMERIK",
            |ui| {
                fields::named(
                    ui.add(
                        TextEdit::singleline(&mut dlg.program_comment)
                            .desired_width(210.0)
                            .hint_text("machine name"),
                    ),
                    "Program comment",
                );
            },
        );
        prop_row(ui, "Output", |ui| {
            let text = if dlg.options.per_op {
                "One file per operation"
            } else {
                "One program"
            };
            let cb = ComboBox::from_id_salt("post_dlg_output")
                .selected_text(text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut dlg.options.per_op, false, "One program");
                    ui.selectable_value(
                        &mut dlg.options.per_op,
                        true,
                        "One file per operation",
                    );
                });
            fields::named(cb.response, "Output");
        });
        prop_row(ui, "Between setups", |ui| {
            let cb = ComboBox::from_id_salt("post_dlg_between")
                .selected_text(dlg.options.between_setups.label())
                .show_ui(ui, |ui| {
                    for b in BetweenSetups::ALL {
                        ui.selectable_value(&mut dlg.options.between_setups, b, b.label());
                    }
                });
            fields::named(cb.response, "Between setups");
        });
        if let Some((inch_word, mm_word)) = &dlg.end_units_words {
            prop_row(ui, "End units", |ui| {
                let inch_label = format!("Inch ({inch_word})");
                let mm_label = format!("mm ({mm_word})");
                let eu_label = |u: EndUnits| match u {
                    EndUnits::Leave => "Leave modal",
                    EndUnits::Inch => inch_label.as_str(),
                    EndUnits::Millimeters => mm_label.as_str(),
                };
                let cb = ComboBox::from_id_salt("post_dlg_end_units")
                    .selected_text(eu_label(dlg.options.end_units))
                    .show_ui(ui, |ui| {
                        for u in [EndUnits::Leave, EndUnits::Inch, EndUnits::Millimeters] {
                            ui.selectable_value(&mut dlg.options.end_units, u, eu_label(u));
                        }
                    });
                fields::named(cb.response, "End units");
            });
        } else if let Some(asked) = match dlg.options.end_units {
            EndUnits::Inch => Some("Inch"),
            EndUnits::Millimeters => Some("mm"),
            EndUnits::Leave => None,
        } {
            prop_row(ui, "End units", |ui| {
                ui.label(format!("{asked} (not supported by this post)"));
                if ui.button("Clear to Leave modal").clicked() {
                    dlg.options.end_units = EndUnits::Leave;
                }
            });
        }
        if let Some(pass) = super::post_options::pass_word(&dlg.post) {
            prop_row(ui, "Threading", |ui| {
                let passes = format!("{pass} passes");
                let th_label = |t: ThreadingOutput| match t {
                    ThreadingOutput::G32Passes => passes.as_str(),
                    ThreadingOutput::G76Cycle => "G76 cycle",
                };
                let cb = ComboBox::from_id_salt("post_dlg_threading")
                    .selected_text(th_label(dlg.options.threading))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut dlg.options.threading,
                            ThreadingOutput::G32Passes,
                            th_label(ThreadingOutput::G32Passes),
                        );
                        ui.selectable_value(
                            &mut dlg.options.threading,
                            ThreadingOutput::G76Cycle,
                            th_label(ThreadingOutput::G76Cycle),
                        );
                    });
                fields::named(cb.response, "Threading");
            });
        }

        labeled_check(ui, "Skip pauses and homes", &mut dlg.options.skip_stops);
        labeled_check(ui, "Smoothing", &mut dlg.options.smoothing);
        labeled_check(ui, "Explain G-code", &mut dlg.options.explain);
    });
}

fn labeled_check(ui: &mut Ui, label: &str, v: &mut bool) {
    let r = check_row(ui, label, v);
    r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *v, label));
}

pub(super) fn draw_operations_tab(ui: &mut Ui, state: &State) {
    let inks = text_inks(ui, &state.world);
    ui.label(
        RichText::new(format!(
            "Post: {}",
            posting::post_ref_label(&posting::post_of(&state.world))
        ))
        .strong(),
    );
    let problems = setup_post_options::program_problems(&state.world);
    for p in &problems {
        ui.colored_label(inks.error, p);
    }
    ui.separator();
    let ops = setup::operation_order(&state.world);
    if ops.is_empty() {
        hint(ui, "No operations in the program.");
        return;
    }
    egui::Grid::new("post_dlg_ops")
        .num_columns(4)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Setup").strong());
            ui.label(RichText::new("Operation").strong());
            ui.label(RichText::new("Kind").strong());
            ui.label(RichText::new("Status").strong());
            ui.end_row();
            for (name, e) in ops {
                let Some(op) = model::operation_of(&state.world, &e) else {
                    continue;
                };
                let setup_name = setup::effective(&state.world, op.setup.as_deref())
                    .unwrap_or_else(|| "Default".to_owned());
                let status = if op.suppressed {
                    "Suppressed"
                } else {
                    match operation::status_of(&state.world, &e) {
                        operation::OpStatus::Ok => "Ready",
                        operation::OpStatus::Stale => "Stale",
                        operation::OpStatus::Never => "Ungenerated",
                    }
                };
                ui.label(setup_name);
                ui.label(name);
                ui.label(op.params.kind().to_string());
                ui.label(status);
                ui.end_row();
            }
        });
}

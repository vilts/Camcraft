//! Grouped per-post `declaredOptions` property grid (#258), shared between
//! the Post page and the floating Post dialog.
//!
//! Provenance: our own design over `layout::form` and `post::options`.

use std::collections::BTreeMap;

use camcraft_app::post::options::{DeclaredOption, OptionKind, OptionValue};
use camcraft_app::{posting, setup_post_options};
use egui::{ComboBox, DragValue, TextEdit, Ui, WidgetInfo, WidgetType};

use super::fields;
use super::layout::{check_row, group, hint, prop_row_hint};
use crate::state::{CommandKind, State};

/// Populate a full key->value draft from declared defaults plus stored overrides.
pub(super) fn resolved_values(
    schema: &[DeclaredOption],
    stored: &BTreeMap<String, OptionValue>,
) -> BTreeMap<String, OptionValue> {
    schema
        .iter()
        .map(|opt| {
            let v = stored
                .get(&opt.key)
                .cloned()
                .unwrap_or_else(|| opt.default.clone());
            (opt.key.clone(), v)
        })
        .collect()
}

/// Extract minimal overrides: keep keys already in `baseline` or differing from schema defaults.
pub(super) fn overrides_from_values(
    schema: &[DeclaredOption],
    values: &BTreeMap<String, OptionValue>,
    baseline: &BTreeMap<String, OptionValue>,
) -> BTreeMap<String, OptionValue> {
    let mut out = BTreeMap::new();
    for opt in schema {
        if let Some(v) = values.get(&opt.key)
            && (baseline.contains_key(&opt.key) || *v != opt.default)
        {
            out.insert(opt.key.clone(), v.clone());
        }
    }
    out
}

fn draw_option_row(
    ui: &mut Ui,
    id_prefix: &str,
    opt: &DeclaredOption,
    val: &mut OptionValue,
    commit_text_on_change: bool,
) -> bool {
    let ctrl_w = ui.available_width().min(if commit_text_on_change { 140.0 } else { 72.0 }).max(48.0);
    match &opt.kind {
        OptionKind::Boolean => {
            let mut b = matches!(*val, OptionValue::Bool(true));
            let r = check_row(ui, &opt.title, &mut b);
            r.widget_info(|| {
                WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), b, &opt.title)
            });
            if !opt.description.is_empty() {
                hint(ui, &opt.description);
            }
            if r.changed() {
                *val = OptionValue::Bool(b);
                return true;
            }
            false
        }
        OptionKind::Integer { min, max } => {
            let mut i = match *val {
                OptionValue::Int(v) => v,
                _ => 0,
            };
            let lo = min.unwrap_or(i64::MIN / 2);
            let hi = max.unwrap_or(i64::MAX / 2);
            let changed = prop_row_hint(ui, &opt.title, &opt.description, |ui| {
                let r = fields::named(ui.add(DragValue::new(&mut i).range(lo..=hi)), &opt.title);
                r.changed()
            });
            if changed {
                *val = OptionValue::Int(i);
            }
            changed
        }
        OptionKind::Number { min, max } => {
            let mut f = match *val {
                OptionValue::Float(v) => v,
                OptionValue::Int(i) => i as f64,
                _ => 0.0,
            };
            let lo = min.unwrap_or(-1e9);
            let hi = max.unwrap_or(1e9);
            let changed = prop_row_hint(ui, &opt.title, &opt.description, |ui| {
                let r = fields::named(
                    ui.add(DragValue::new(&mut f).speed(0.1).range(lo..=hi)),
                    &opt.title,
                );
                r.changed()
            });
            if changed {
                *val = OptionValue::Float(f);
            }
            changed
        }
        OptionKind::String => {
            let shown = match val {
                OptionValue::Str(s) => s.clone(),
                _ => String::new(),
            };
            if commit_text_on_change {
                let mut draft = shown;
                let changed = prop_row_hint(ui, &opt.title, &opt.description, |ui| {
                    fields::named(
                        ui.add(TextEdit::singleline(&mut draft).desired_width(ctrl_w)),
                        &opt.title,
                    )
                    .changed()
                });
                if changed {
                    *val = OptionValue::Str(draft);
                }
                return changed;
            }
            let id = ui.id().with((id_prefix, &opt.key));
            let mut draft: String = ui
                .data_mut(|d| d.get_temp::<String>(id))
                .unwrap_or_else(|| shown.clone());
            let mut committed = false;
            prop_row_hint(ui, &opt.title, &opt.description, |ui| {
                let resp = fields::named(
                    ui.add(TextEdit::singleline(&mut draft).id(id).desired_width(ctrl_w)),
                    &opt.title,
                );
                if resp.has_focus() {
                    ui.data_mut(|d| d.insert_temp(id, draft.clone()));
                } else {
                    ui.data_mut(|d| d.remove::<String>(id));
                    if resp.lost_focus() && draft != shown {
                        *val = OptionValue::Str(draft);
                        committed = true;
                    }
                }
            });
            committed
        }
        OptionKind::Enum { choices } => {
            let cur_id = match val {
                OptionValue::Str(s) => s.clone(),
                _ => String::new(),
            };
            let selected_label = choices
                .iter()
                .find(|c| c.id == cur_id)
                .map_or(cur_id.as_str(), |c| c.title.as_str());
            let mut next_id = cur_id.clone();
            prop_row_hint(ui, &opt.title, &opt.description, |ui| {
                let cb = ComboBox::from_id_salt((id_prefix, &opt.key))
                    .width(ctrl_w)
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for c in choices {
                            ui.selectable_value(&mut next_id, c.id.clone(), c.title.as_str());
                        }
                    });
                fields::named(cb.response, &opt.title);
            });
            if next_id != cur_id {
                *val = OptionValue::Str(next_id);
                return true;
            }
            false
        }
    }
}

/// Render grouped `declaredOptions` rows into `values`. Returns true if any value changed or reset was clicked.
pub(super) fn render_draft(
    ui: &mut Ui,
    id_prefix: &str,
    schema: &[DeclaredOption],
    values: &mut BTreeMap<String, OptionValue>,
    has_overrides: bool,
    commit_text_on_change: bool,
    reset_clicked: &mut bool,
) -> bool {
    if schema.is_empty() {
        return false;
    }
    let mut groups: Vec<&str> = Vec::new();
    for opt in schema {
        let g = if opt.group.is_empty() {
            "Post properties"
        } else {
            opt.group.as_str()
        };
        if !groups.contains(&g) {
            groups.push(g);
        }
    }

    let mut changed = false;
    for g in groups {
        group(ui, g, None, |ui| {
            for opt in schema {
                let og = if opt.group.is_empty() {
                    "Post properties"
                } else {
                    opt.group.as_str()
                };
                if og != g {
                    continue;
                }
                let entry = values
                    .entry(opt.key.clone())
                    .or_insert_with(|| opt.default.clone());
                if draw_option_row(ui, id_prefix, opt, entry, commit_text_on_change) {
                    changed = true;
                }
            }
        });
    }

    let any_diff = schema
        .iter()
        .any(|opt| values.get(&opt.key) != Some(&opt.default));
    if ui
        .add_enabled(any_diff || has_overrides, egui::Button::new("Reset to defaults"))
        .clicked()
    {
        for opt in schema {
            values.insert(opt.key.clone(), opt.default.clone());
        }
        *reset_clicked = true;
        changed = true;
    }
    changed
}

/// Render the live Post page section for the post & target currently in view (#258).
pub(super) fn section(ui: &mut Ui, state: &mut State) {
    let target = state.post_options_target();
    let post = posting::post_of(&state.world);
    let schema = match posting::declared_options_for_post(&post) {
        Ok(s) if !s.is_empty() => s,
        _ => return,
    };
    let stored = setup_post_options::script_for_post(&state.world, target.as_deref(), &post);
    let mut values = resolved_values(&schema, &stored);
    let mut reset = false;
    let mut changed = false;
    ui.separator();
    ui.collapsing(egui::RichText::new("Post script options").strong(), |ui| {
        changed = render_draft(
            ui,
            "post_page",
            &schema,
            &mut values,
            !stored.is_empty(),
            false,
            &mut reset,
        );
    });
    if changed {
        let overrides = if reset {
            BTreeMap::new()
        } else {
            overrides_from_values(&schema, &values, &stored)
        };
        if overrides != stored {
            state.pending.push_back(CommandKind::SetPostScriptOptions {
                setup: target,
                post,
                overrides,
            });
        }
    }
}

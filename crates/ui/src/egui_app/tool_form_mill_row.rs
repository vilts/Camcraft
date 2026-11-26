//! The tool editor's form mill rows (#3369): the preset list (roundover,
//! cove, ogee, V-groove, bead, or a drawn outline), the preset's number
//! rows and the pilot length, and the seeding that keeps a draft sane when
//! the kind or the preset changes. Draft fields only; Apply / Add tool
//! carries them through `ChangeTool` / `AddTool`, so an edit is one undo
//! step. The diameter and the flute length follow the form.
//!
//! Provenance: the router bit forms and terms of the public router bit
//! catalogues, over `tools::form_mill`; our own widget. No vendor material
//! consulted.

use egui::Ui;

use camcraft_app::tools::{FormPreset, FormSpec, ToolKind};

use crate::state::{State, ToolUi};

use super::fields::{combo, num_plain_row, num_row};
use super::layout::{hint, prop_row, prop_row_hint};

/// The pilot bearing's length a new form mill starts with, mm.
const START_PILOT: f64 = 5.0;

/// Keep the form fields sane for the draft's kind: another kind states no
/// form; a form mill without one starts as a roundover, with no corner
/// radius and no plunge.
pub(super) fn seed(draft: &mut ToolUi) {
    if draft.tool_kind() != ToolKind::FormMill {
        draft.form = None;
        return;
    }
    if draft.form.is_none() {
        draft.form = FormSpec::from_preset(FormPreset::starter(0), START_PILOT).ok();
        draft.corner_r = 0.0;
        draft.tip_angle = 0.0;
        draft.center_cutting = false;
    }
    settle(draft);
}

/// Write the outline, the diameter and the flute length from the preset,
/// so the draft and the preview show the tool that Apply lands.
fn settle(draft: &mut ToolUi) {
    let mut t = draft.to_tool();
    t.settle_form();
    draft.diameter = t.diameter;
    draft.flute_len = t.flute_len;
    draft.form = t.form;
}

/// The preset list, its number rows and the pilot length, shown only for
/// a form mill.
pub(super) fn rows(ui: &mut Ui, state: &mut State) {
    if state.tool_ui.tool_kind() != ToolKind::FormMill {
        return;
    }
    let u = state.units();
    let Some(mut form) = state.tool_ui.form.clone() else { return };
    // Index 0 is a drawn outline; 1.. are the presets.
    let mut labels = vec!["Drawn outline"];
    labels.extend(FormPreset::NAMES);
    let before = form.preset.map_or(0, |p| p.index() + 1);
    let mut sel = before;
    prop_row_hint(ui, "Form preset", "the router bit form the outline is written from", |ui| {
        combo(ui, "tool_form_preset", &mut sel, &labels)
    });
    if sel != before {
        form.preset = sel.checked_sub(1).map(FormPreset::starter);
        if matches!(form.preset, Some(FormPreset::VGroove { .. })) {
            form.pilot_len = 0.0;
        }
    }
    if let Some(preset) = form.preset.as_mut() {
        for (label, v) in preset.numbers_mut() {
            if label.contains("(deg)") {
                prop_row(ui, label, |ui| num_plain_row(ui, &mut *v));
            } else {
                prop_row(ui, label, |ui| num_row(ui, u, &mut *v));
            }
        }
    } else {
        hint(ui, "a drawn outline: edit its points with the edit_tool MCP tool");
    }
    if !matches!(form.preset, Some(FormPreset::VGroove { .. })) {
        prop_row_hint(ui, "Pilot length", "the bearing's height at the tip; it guides and does not cut (0 = no pilot)", |ui| {
            num_row(ui, u, &mut form.pilot_len)
        });
    }
    hint(ui, "the diameter and flute length follow the form; no operation cuts with a form mill yet");
    state.tool_ui.form = Some(form);
    settle(&mut state.tool_ui);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form_draft(index: usize) -> ToolUi {
        let mut d = ToolUi { kind: 22, ..ToolUi::default() };
        seed(&mut d);
        if let Some(f) = d.form.as_mut() {
            f.preset = Some(FormPreset::starter(index));
            if index == 3 {
                f.pilot_len = 0.0;
            }
        }
        settle(&mut d);
        d
    }

    #[test]
    fn each_preset_seeds_a_sane_tool() {
        for (i, name) in FormPreset::NAMES.iter().enumerate() {
            let tool = form_draft(i).to_tool();
            assert_eq!(tool.kind, ToolKind::FormMill);
            assert_eq!(tool.form.as_ref().and_then(|f| f.preset).map(|p| p.name()), Some(*name));
            assert_eq!(tool.why_not_sane(), None, "{name}: {tool:?}");
        }
    }

    #[test]
    fn another_kind_clears_the_form() {
        let mut d = form_draft(0);
        d.kind = 0;
        seed(&mut d);
        let tool = d.to_tool();
        assert_eq!(tool.form, None);
        assert_eq!(tool.why_not_sane(), None);
    }
}

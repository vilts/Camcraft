//! The tool editor's taper tip rows (#3368): a chamfer mill's tip form
//! (sharp, flat or ball) and a flat tip's diameter, and the seeding that
//! keeps a draft sane when the kind or the form changes. Draft fields
//! only; Apply / Add tool carries them through `ChangeTool` / `AddTool`,
//! so the edit is one undo step.
//!
//! Provenance: the tapered end mill and chamfer mill forms of the public
//! machine-tool handbooks, over `tools::taper`; our own widget. No vendor
//! material consulted.

use egui::Ui;

use camcraft_app::tools::{TaperTip, ToolKind};

use crate::state::{State, ToolUi};

use super::fields::{combo, num_row};
use super::layout::{prop_row, prop_row_hint};

/// The form labels, in `TaperTip::ALL` order.
const FORMS: [&str; 3] = ["Sharp", "Flat", "Ball"];

/// Keep the tip fields sane for the draft's kind and form: another kind
/// states no tip; a flat tip needs a tip diameter under the diameter; a
/// ball tip needs a ball radius (the corner radius) and no tip diameter.
pub(super) fn seed(draft: &mut ToolUi) {
    if draft.tool_kind() != ToolKind::Taper {
        draft.tip_form = TaperTip::Sharp;
        draft.tip_diameter = 0.0;
        return;
    }
    match draft.tip_form {
        TaperTip::Sharp => draft.tip_diameter = 0.0,
        TaperTip::Flat => {
            if !(draft.tip_diameter > 0.0 && draft.tip_diameter < draft.diameter) {
                draft.tip_diameter = draft.diameter * 0.2;
            }
        }
        TaperTip::Ball => {
            draft.tip_diameter = 0.0;
            if !(draft.corner_r > 0.0 && draft.corner_r < draft.diameter * 0.5) {
                draft.corner_r = draft.diameter * 0.1;
            }
        }
    }
}

/// The tip form row, and a flat tip's diameter, shown only for a chamfer
/// mill. A ball tip's radius is the Corner radius row.
pub(super) fn tip_rows(ui: &mut Ui, state: &mut State) {
    if state.tool_ui.tool_kind() != ToolKind::Taper {
        return;
    }
    let u = state.units();
    let before = state.tool_ui.tip_form;
    let mut sel = TaperTip::ALL.iter().position(|f| *f == before).unwrap_or(0);
    prop_row_hint(ui, "Tip form", "sharp is the cone's point; a flat or ball tip is not cut by any operation yet", |ui| {
        combo(ui, "tool_tip_form", &mut sel, &FORMS)
    });
    state.tool_ui.tip_form = TaperTip::ALL.get(sel).copied().unwrap_or_default();
    if state.tool_ui.tip_form != before {
        seed(&mut state.tool_ui);
    }
    if state.tool_ui.tip_form == TaperTip::Flat {
        prop_row(ui, "Tip diameter", |ui| num_row(ui, u, &mut state.tool_ui.tip_diameter));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taper_draft(form: TaperTip) -> ToolUi {
        let mut d = ToolUi { kind: 10, ..ToolUi::default() };
        d.tip_angle = 90.0;
        d.tip_form = form;
        seed(&mut d);
        d
    }

    #[test]
    fn each_tip_form_seeds_a_sane_tool() {
        for form in TaperTip::ALL {
            let tool = taper_draft(form).to_tool();
            assert_eq!(tool.tip_form, form);
            assert_eq!(tool.why_not_sane(), None, "{form}: {tool:?}");
        }
    }

    #[test]
    fn another_kind_clears_the_tip() {
        let mut d = taper_draft(TaperTip::Flat);
        d.kind = 0;
        seed(&mut d);
        let tool = d.to_tool();
        assert_eq!((tool.tip_form, tool.tip_diameter), (TaperTip::Sharp, 0.0));
        assert_eq!(tool.why_not_sane(), None);
    }
}

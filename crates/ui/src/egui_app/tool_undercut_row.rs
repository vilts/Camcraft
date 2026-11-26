//! The tool editor's undercut cutter rows and seeding (#1444): the
//! dovetail's flank angle input and valid starting numbers when switching
//! to T-slot, keyseat, dovetail or lollipop cutters. Draft fields only;
//! Apply / Add tool carries them through `ChangeTool` / `AddTool`, so the
//! edit is undoable.
//!
//! Provenance: public machine-tool handbook cutter forms (Machinery's
//! Handbook) over `tools::undercut`; our own widget. No vendor material
//! consulted.

use egui::Ui;

use camcraft_app::tools::ToolKind;
use camcraft_app::undercut;

use crate::state::{State, ToolUi};

use super::fields::num_plain_row;
use super::layout::prop_row_hint;

/// Seed valid numbers for an undercut kind when the draft was just
/// switched onto it and its inherited numbers would fail `undercut::refusal`
/// (e.g. the `EM-0200-2FL` default has `diameter < shank_d` and
/// `tip_angle = 0.0`). Keeps existing numbers when already valid.
pub(super) fn seed(draft: &mut ToolUi) {
    let kind = draft.tool_kind();
    if !undercut::is_kind(kind) {
        return;
    }
    draft.corner_r = 0.0;
    if kind == ToolKind::Dovetail
        && (draft.tip_angle.is_nan() || !(draft.tip_angle > 0.0 && draft.tip_angle < 90.0))
    {
        draft.tip_angle = 60.0;
    }
    if undercut::refusal(&draft.to_tool()).is_some() {
        let (d, fl, sd, sl) = match kind {
            ToolKind::TSlot => (20.0, 6.0, 8.0, 30.0),
            ToolKind::Keyseat => (16.0, 3.0, 6.0, 25.0),
            ToolKind::Dovetail => (20.0, 5.0, 8.0, 30.0),
            ToolKind::Lollipop => (10.0, 5.0, 4.0, 30.0),
            _ => return,
        };
        draft.diameter = d;
        draft.flute_len = fl;
        draft.shank_d = sd;
        draft.shank_len = sl;
        if kind == ToolKind::Dovetail {
            draft.tip_angle = 60.0;
        }
    }
}

/// The dovetail cutter's flank angle input (`Tool::tip_angle`), shown
/// only when `ToolKind::Dovetail` is active.
pub(super) fn dovetail_row(ui: &mut Ui, state: &mut State) {
    if state.tool_ui.tool_kind() != ToolKind::Dovetail {
        return;
    }
    prop_row_hint(
        ui,
        "Flank angle (deg)",
        "angle between the dovetail flank and its bottom face (0 to 90 degrees)",
        |ui| num_plain_row(ui, &mut state.tool_ui.tip_angle),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::tools::catalog;

    #[test]
    fn switching_from_default_end_mill_to_any_undercut_kind_seeds_a_sane_tool() {
        for (index, kind) in [
            (18, ToolKind::TSlot),
            (19, ToolKind::Keyseat),
            (20, ToolKind::Dovetail),
            (21, ToolKind::Lollipop),
        ] {
            let mut draft = ToolUi { kind: index, ..ToolUi::default() };
            assert_eq!(draft.tool_kind(), kind);
            seed(&mut draft);
            let tool = draft.to_tool();
            assert_eq!(tool.why_not_sane(), None, "{kind}: {tool:?}");
            assert!(undercut::Undercut::of(&tool).is_some(), "{kind}");
            assert_eq!(tool.corner_r, 0.0, "{kind} keeps corner_r zero");
        }
    }

    #[test]
    fn switching_from_drill_to_dovetail_replaces_out_of_range_tip_angle() {
        let drill = catalog()
            .into_iter()
            .find(|(_, t)| t.kind == ToolKind::Drill)
            .map(|(_, t)| t)
            .expect("drill in catalog");
        let mut draft = ToolUi::from_tool("D", &drill, false);
        draft.kind = 20;
        seed(&mut draft);
        let tool = draft.to_tool();
        assert_eq!(tool.why_not_sane(), None, "{tool:?}");
        assert_eq!(tool.tip_angle, 60.0);
    }
}

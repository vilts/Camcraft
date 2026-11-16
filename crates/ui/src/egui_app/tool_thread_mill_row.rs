//! The tool editor's thread-mill rows (#894): a multi-form mill's one
//! pitch, a single-form mill's pitch range, and the form angle - shown
//! only for the two thread-mill kinds, with the draft seeded to a sane
//! tool when the kind is first picked. Draft fields only; Apply carries
//! them through `ChangeTool` like every other tool number, so the edit
//! is undoable.
//!
//! Provenance: ISO metric thread terms (pitch, 60-degree form) and
//! public handbook thread-mill descriptions; our own widget.

use egui::Ui;

use camcraft_app::thread_mill;
use camcraft_app::tools::ToolKind;

use crate::state::{State, ToolUi};

use super::fields::{field, num, num_plain};

/// Seed the numbers a thread-mill kind reads when the draft does not
/// state usable ones yet (the kind was just picked): a 60-degree form,
/// a 1 mm pitch that leaves a core, and a 0.5 mm finest pitch on the
/// single-form kind.
pub(super) fn seed(draft: &mut ToolUi) {
    let kind = draft.tool_kind();
    if !thread_mill::is_thread_mill(kind) {
        return;
    }
    if draft.tip_angle.is_nan() || !(draft.tip_angle > 0.0 && draft.tip_angle < 180.0) {
        draft.tip_angle = thread_mill::FORM_ANGLE;
    }
    let core = |p: f64| draft.diameter * 0.5 > thread_mill::EXTERNAL_DEPTH_PER_PITCH * p;
    if draft.pitch.is_nan() || draft.pitch <= 0.0 || !core(draft.pitch) {
        draft.pitch = if core(1.0) { 1.0 } else { (draft.diameter * 0.5).max(0.0) };
    }
    if kind == ToolKind::ThreadMillSingle {
        if draft.pitch_min.is_nan() || draft.pitch_min <= 0.0 || draft.pitch_min > draft.pitch {
            draft.pitch_min = draft.pitch * 0.5;
        }
    } else {
        draft.pitch_min = 0.0;
        if draft.flute_len.is_nan() || draft.flute_len < draft.pitch {
            draft.flute_len = draft.pitch * 8.0;
        }
    }
    draft.corner_r = 0.0;
}

/// The rows, shown for the two thread-mill kinds.
pub(super) fn thread_mill_rows(ui: &mut Ui, state: &mut State) {
    let kind = state.tool_ui.tool_kind();
    if !thread_mill::is_thread_mill(kind) {
        return;
    }
    let u = state.units();
    if kind == ToolKind::ThreadMillSingle {
        field(ui, "Finest pitch");
        num(ui, u, &mut state.tool_ui.pitch_min);
        field(ui, "Coarsest pitch");
        num(ui, u, &mut state.tool_ui.pitch);
        ui.label("one row of teeth: it cuts any pitch in this range, one turn per thread - the operation states the pitch");
    } else {
        field(ui, "Thread pitch");
        num(ui, u, &mut state.tool_ui.pitch);
        ui.label("rows of teeth ground to this one pitch - the flute length over the pitch is how many threads one revolution cuts");
    }
    field(ui, "Form angle (deg)");
    num_plain(ui, &mut state.tool_ui.tip_angle);
    ui.label("the included angle of the teeth - 60 for metric and unified threads; the diameter is across the tooth crests");
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::tools::catalog;

    #[test]
    fn picking_a_thread_mill_kind_seeds_a_sane_tool_and_round_trips() {
        let end_mill = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
        for (index, kind) in [(14, ToolKind::ThreadMill), (15, ToolKind::ThreadMillSingle)] {
            let mut draft = ToolUi::from_tool("TM", &end_mill, false);
            draft.kind = index;
            assert_eq!(draft.tool_kind(), kind);
            seed(&mut draft);
            let tool = draft.to_tool();
            assert!(tool.is_sane(), "{kind}: {tool:?}");
            assert_eq!(tool.tip_angle, 60.0);
            assert_eq!(tool.pitch_min > 0.0, kind == ToolKind::ThreadMillSingle);
            // The draft reads a stored thread mill back number for number.
            assert_eq!(ToolUi::from_tool("TM", &tool, true).to_tool(), tool);
        }
        // A stated thread mill is left as it is.
        let single = catalog().into_iter().find(|(n, _)| *n == "TMS-0600-P050-150").map(|(_, t)| t).expect("catalog");
        let mut draft = ToolUi::from_tool("S", &single, true);
        seed(&mut draft);
        assert_eq!(draft.to_tool(), single);
    }
}

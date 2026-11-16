//! The tool editor's turning cutting-data rows (#891): a lathe tool's
//! surface speed (m/min, ft/min in an inch document), feed per rev and
//! its own max-rpm clamp - the numbers a turning operation posts as
//! G96 / per-rev F. The draft fields only; Apply carries them through
//! `ChangeTool` like every other tool number, so they are undoable.
//!
//! Provenance: our own widget over `camcraft_tools::turning_data`
//! (machine-tool handbook turning relations); no vendor material.

use egui::{DragValue, Ui};

use camcraft_app::cutting_turn;
use camcraft_app::tools::TurningData;
use camcraft_app::units::{MM_PER_INCH, Unit};

use crate::state::State;

use super::fields::field;

/// m per foot, for the ft/min lens of an inch document.
const M_PER_FT: f64 = 0.3048;

/// The rows, shown for the lathe steels the turning table speaks for.
pub(super) fn turning_rows(ui: &mut Ui, state: &mut State) {
    if !cutting_turn::tabulated(state.tool_ui.tool_kind()) {
        return;
    }
    let u = state.units();
    field(ui, "Constant surface speed");
    let mut on = state.tool_ui.turning.is_some();
    ui.checkbox(&mut on, "state turning data (G96 + feed per rev)");
    if !on {
        state.tool_ui.turning = None;
        ui.label("unstated: the turning operations post this tool's rpm (G97)");
        return;
    }
    let t = state.tool_ui.turning.get_or_insert(TurningData { vc: 150.0, feed_per_rev: 0.15, max_rpm: None });
    let inch = u == Unit::Inches;
    let (vs, ls) = if inch { (1.0 / M_PER_FT, 1.0 / MM_PER_INCH) } else { (1.0, 1.0) };
    field(ui, "Surface speed");
    let mut vc = t.vc * vs;
    ui.add(DragValue::new(&mut vc).range(1.0..=6000.0).speed(1.0).max_decimals(0).suffix(if inch { " ft/min" } else { " m/min" }));
    t.vc = vc / vs;
    field(ui, "Feed per rev");
    let mut f = t.feed_per_rev * ls;
    ui.add(DragValue::new(&mut f).range(0.0001..=3.0).speed(0.001).max_decimals(4).suffix(if inch { " in/rev" } else { " mm/rev" }));
    t.feed_per_rev = f / ls;
    field(ui, "Max rpm");
    let mut capped = t.max_rpm.is_some();
    ui.horizontal(|ui| {
        ui.checkbox(&mut capped, "");
        if capped {
            let m = t.max_rpm.get_or_insert(3000.0);
            ui.add(DragValue::new(m).range(1.0..=100_000.0).speed(10.0).max_decimals(0).suffix(" rpm"));
        } else {
            t.max_rpm = None;
            ui.label("the machine's maximum alone");
        }
    });
    ui.label("G96 needs a clamp: the lower of this and the machine's spindle maximum - with neither stated the post refuses");
}

#[cfg(test)]
mod tests {
    use camcraft_app::tools::{TurningData, catalog};

    use crate::state::ToolUi;

    #[test]
    fn the_editor_draft_carries_the_turning_data_through_apply() {
        let insert = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t);
        let Some(mut t) = insert else { panic!("catalog insert") };
        assert_eq!(ToolUi::from_tool("TN", &t, true).to_tool().turning, None, "unstated stays unstated");
        let d = TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: Some(2500.0) };
        t.turning = Some(d);
        assert_eq!(ToolUi::from_tool("TN", &t, true).to_tool().turning, Some(d), "stated data survives an Apply");
    }
}

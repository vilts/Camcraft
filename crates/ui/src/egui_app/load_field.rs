//! The adaptive "Optimal load (% of D)" field (M11.4 B): edits the
//! ae/D ratio the params store as a percentage of the tool diameter and
//! quotes the derived contact angle `acos(1 - 2 ae/D)` as a hint. Where
//! it shows, the Stepover row does not: one radial-step field per
//! strategy (`PocketStrategy::steps_by_load`, #1512).
//! Provenance: straight-wall radial-engagement geometry of peripheral
//! milling (machining handbooks; `camcraft_app::load` carries the law).

use egui::{DragValue, Ui};

use camcraft_app::load::degrees_of_load;

/// The field's label - one spelling for every adaptive pane.
pub(super) const OPTIMAL_LOAD_LABEL: &str = "Optimal load (% of D)";

/// Draw the percent editor over `load` (a ratio). The draft is written
/// only when the user moved the number, so a value loaded from a file
/// (the exact 45-degree default, 14.6446...%) is never re-rounded by a
/// redraw - the EditOperationParams commit path sees no phantom edit.
/// No clamp: the panel never normalizes, the parameter door refuses an
/// out-of-range load by name (0 < ae/D <= 50%).
pub(super) fn optimal_load_field(ui: &mut Ui, load: &mut f64) {
    let mut pct = *load * 100.0;
    let changed = super::layout::prop_row(ui, OPTIMAL_LOAD_LABEL, |ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized([super::layout::CTRL_W, h], DragValue::new(&mut pct).speed(0.1).max_decimals(2).suffix(" %"))
            .changed()
    });
    if changed {
        *load = pct / 100.0;
    }
    super::layout::hint(
        ui,
        &format!(
            "radial width ae = {:.2}% of the tool diameter - about {:.1}° of cutter contact (acos(1 - 2 ae/D)); rings step by ae",
            *load * 100.0,
            degrees_of_load(*load)
        ),
    );
    super::layout::hint(ui, LOAD_SETS_STEP);
}

/// Said under every Optimal load field (#1512): the load is the step, so
/// the page shows no Stepover beside it.
pub(super) const LOAD_SETS_STEP: &str =
    "this load sets the step between passes - Stepover is not used here (it spaces Zigzag, Waterline and Auto)";

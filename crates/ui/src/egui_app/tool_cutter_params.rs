//! The drawn cutter's parameters (#93 slice 3): inside the Draw cutter
//! view, the numbers each milling kind's outline is drawn from - the
//! cutter's diameter, the kind's tip (a ball's radius, a bull's corner,
//! a drill's point or a chamfer mill's cone), the flute length, the
//! shank - plus a neck, which a tool states no number for and so lives
//! in the outline alone. They edit the editor's draft: a Parametric
//! outline is redrawn from them, a Custom one carried onto them, and the
//! side view beside follows every keystroke. Nothing here is a command;
//! Apply cutter lands numbers and outline together.
//! Provenance: our own pane over `app::cutter_fit`; the tip parameters
//! are the handbook's tool drawing. No vendor material consulted.

use egui::Ui;

use camcraft_app::cutter_fit::Neck;
use camcraft_app::tools::ToolKind;
use camcraft_app::units::{self, Unit};

use super::tool_profile_page::{BAD, len_cell};
use crate::state::State;

/// The parameters grid and the neck row, above the points table.
pub(super) fn params_block(ui: &mut Ui, state: &mut State, u: Unit) {
    egui::Grid::new("tool_cutter_params").num_columns(4).spacing([6.0, 2.0]).show(ui, |ui| {
        let t = &mut state.tool_ui;
        ui.label("cutter Ø");
        len_cell(ui, u, &mut t.diameter);
        match t.tool_kind() {
            // #263: a ball's corner IS its ball - shown, never edited.
            ToolKind::BallNose | ToolKind::Probe => {
                ui.label("ball R");
                ui.label(units::len(t.diameter * 0.5, u, 3));
            }
            ToolKind::BullNose => {
                ui.label("corner R");
                len_cell(ui, u, &mut t.corner_r);
            }
            ToolKind::Drill => angle(ui, "point angle °", &mut t.tip_angle),
            ToolKind::Taper => angle(ui, "cone angle °", &mut t.tip_angle),
            _ => {
                ui.label("");
                ui.label("");
            }
        }
        ui.end_row();
        ui.label("flutes");
        len_cell(ui, u, &mut t.flute_len);
        ui.label("");
        ui.label("");
        ui.end_row();
        ui.label("shank Ø");
        len_cell(ui, u, &mut t.shank_d);
        ui.label("shank length");
        len_cell(ui, u, &mut t.shank_len);
        ui.end_row();
    });
    neck_row(ui, state, u);
}

/// Neck on/off and its diameter and length: on draws the kind's tip, a
/// straight neck up from the flute top and the stated shank (Custom);
/// off is the numbers' own outline again (Parametric).
fn neck_row(ui: &mut Ui, state: &mut State, u: Unit) {
    let neck = state.cutter_neck();
    ui.horizontal(|ui| {
        let mut on = neck.is_some();
        let toggled = ui
            .checkbox(&mut on, "Neck")
            .on_hover_text("a relieved neck between the flutes and the shank; drawing it replaces a hand-drawn outline")
            .changed();
        if toggled {
            let t = &state.tool_ui;
            let start = Neck { d: 0.9 * t.diameter.min(t.shank_d), len: 0.5 * t.shank_len };
            state.set_cutter_neck(on.then_some(start));
        } else if let Some(mut n) = neck {
            ui.label("neck Ø");
            let a = len_changed(ui, u, &mut n.d);
            ui.label("neck length");
            let b = len_changed(ui, u, &mut n.len);
            if a || b {
                state.set_cutter_neck(Some(n));
            }
        }
    });
    if let Some(why) = &state.tool_cutter_ui.neck_refusal {
        ui.colored_label(BAD, why);
    }
}

/// A length cell that says whether it changed.
fn len_changed(ui: &mut Ui, u: Unit, v: &mut f64) -> bool {
    let before = *v;
    len_cell(ui, u, v);
    before.to_bits() != v.to_bits()
}

/// A degrees cell (no unit lens).
fn angle(ui: &mut Ui, label: &str, v: &mut f64) {
    ui.label(label);
    ui.add_sized([54.0, ui.spacing().interact_size.y], egui::DragValue::new(v).speed(0.5));
}

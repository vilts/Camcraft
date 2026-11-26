//! The DXF units prompt (#1863): a modal over the shell while a drawing
//! that states no unit waits on `state.dxf_units_prompt`. It shows the
//! drawing's size read each way, in millimeters and in inches, and asks
//! the user to choose. Millimeters and Inches answer through the command
//! queue; Cancel, Escape or a click on the backdrop places nothing. The
//! unit is never guessed.
//!
//! Provenance: our own layout over egui's `Modal`, after the unsaved
//! prompt (#107); no vendor material consulted.

use egui::{Id, Modal};

use camcraft_app::units::Unit;

use crate::state::{CommandKind, State};

/// Millimeters per inch.
const MM_PER_IN: f64 = 25.4;

/// A size number for the prompt: at most four decimals, no trailing zeros.
fn fmt(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The two readings of a `width` x `height` drawing: as millimeters and as
/// inches, each with its size in the other unit.
pub(crate) fn readings(width: f64, height: f64) -> [String; 2] {
    [
        format!(
            "Read in millimeters: {} x {} mm ({} x {} in)",
            fmt(width),
            fmt(height),
            fmt(width / MM_PER_IN),
            fmt(height / MM_PER_IN)
        ),
        format!(
            "Read in inches: {} x {} in ({} x {} mm)",
            fmt(width),
            fmt(height),
            fmt(width * MM_PER_IN),
            fmt(height * MM_PER_IN)
        ),
    ]
}

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(prompt) = state.dxf_units_prompt.as_ref() else {
        return;
    };
    let file = prompt.source.label();
    let [mm, inch] = readings(prompt.width, prompt.height);
    let mut answer: Option<Option<Unit>> = None;
    let response = Modal::new(Id::new("dxf-units-prompt")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading("DXF units");
        ui.label(format!("{file} does not state its units. Choose the unit of its coordinates."));
        ui.add_space(4.0);
        ui.label(mm);
        ui.label(inch);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Millimeters").clicked() {
                answer = Some(Some(Unit::Millimeters));
            }
            if ui.button("Inches").clicked() {
                answer = Some(Some(Unit::Inches));
            }
            if ui.button("Cancel").clicked() {
                answer = Some(None);
            }
        });
    });
    if answer.is_none() && response.should_close() {
        answer = Some(None);
    }
    if let Some(units) = answer {
        state.pending.push_back(CommandKind::ResolveDxfUnits(units));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_state_both_units_both_ways() {
        let [mm, inch] = readings(80.0, 20.0);
        assert_eq!(mm, "Read in millimeters: 80 x 20 mm (3.1496 x 0.7874 in)");
        assert_eq!(inch, "Read in inches: 80 x 20 in (2032 x 508 mm)");
    }
}

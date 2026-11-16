//! The one style of a "Pick" button (#1728): armed it carries the selection
//! fill and a stroke, idle it carries a faint tint of the same colour, so a
//! pick is easy to find and the armed one is easy to see.
//! Provenance: our own egui styling over the theme's selection colours; no
//! vendor material consulted.

use egui::{Button, Color32, Response, Stroke, Ui, Visuals};

/// Fill of a pick button: the selection fill armed, a faint tint idle.
pub(super) fn pick_fill(v: &Visuals, armed: bool) -> Color32 {
    let base = v.selection.bg_fill;
    if armed { base } else { base.gamma_multiply(0.3) }
}

/// A Pick button: `armed` shows the active state. The caller handles the click.
pub(super) fn pick_button(ui: &mut Ui, armed: bool, text: impl Into<egui::WidgetText>) -> Response {
    let button = pick_widget(ui, armed, text);
    ui.add(button)
}

/// The styled button itself, for a caller that sizes it (`add_sized`).
pub(super) fn pick_widget<'a>(ui: &Ui, armed: bool, text: impl Into<egui::WidgetText>) -> Button<'a> {
    let v = ui.visuals();
    let stroke = if armed { Stroke::new(1.5, v.selection.stroke.color) } else { Stroke::new(1.0, v.selection.bg_fill) };
    let fill = pick_fill(v, armed);
    Button::selectable(armed, text).fill(fill).stroke(stroke)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    #[test]
    fn armed_and_idle_fills_differ() {
        let v = Visuals::dark();
        assert_ne!(pick_fill(&v, true), pick_fill(&v, false));
        assert_ne!(pick_fill(&v, false), v.widgets.inactive.weak_bg_fill);
    }

    #[test]
    fn an_armed_pick_button_reads_as_toggled_on_and_an_idle_one_as_off() {
        let mut h = Harness::new_ui(|ui| {
            pick_button(ui, true, "Pick tip");
            pick_button(ui, false, "Pick axis");
        });
        h.run();
        let on = h.get_by_label("Pick tip").accesskit_node().toggled();
        let off = h.get_by_label("Pick axis").accesskit_node().toggled();
        assert_eq!(on, Some(egui::accesskit::Toggled::True));
        assert_eq!(off, Some(egui::accesskit::Toggled::False));
    }
}

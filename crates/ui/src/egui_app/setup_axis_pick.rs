//! The Setups page's part-axes rows (#557, full 3D since #769): set the
//! part's +Y and +X from a straight edge or two points picked in the
//! viewport, flip either, choose which one wins when both are picked,
//! and Apply. With both picked the two directions define the frame
//! outright - the primary kept exactly, the other made square to it,
//! +Z where the right-hand rule puts it; with one picked, the work
//! plane carries the turn (#557). The pick draft is view state
//! (`crate::axis_pick`); Apply queues `CommandKind::AlignPartFromAxes`,
//! the undoable `AlignPartToSetup`. The app says whether the picks
//! make an alignment and, if not, why - shown on the disabled Apply.
//! The work offset is not touched.
//!
//! Provenance: our own UI over plain fixture practice (indicate two
//! square edges); no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::pick::{self, InPlaneAxis};

use crate::axis_pick::{self, AxisBy};
use crate::state::{CommandKind, State};

/// The rows of one setup.
pub(super) fn rows(ui: &mut Ui, state: &mut State, setup: &str) {
    state.axis_rows_shown = true;
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    ui.label("    Part axes - pick an edge or two points for +Y and +X; +Z follows by the right-hand rule");
    for axis in [InPlaneAxis::Y, InPlaneAxis::X] {
        axis_row(ui, state, setup, axis);
    }
    let Some(d) = state.axis_pick.as_ref().filter(|d| d.setup == setup).cloned() else { return };
    let axes = d.wanted();
    if axes.y.is_none() && axes.x.is_none() {
        return;
    }
    // The command's own check: a refused pick, or one that gives the
    // alignment the setup already holds, greys Apply out with the why.
    // Just applied, the same picks are no change: say Flip re-applies
    // rather than "already holds" (#557 hand check).
    let check = AlignPartToSetup::check_align(&state.world, setup, &axes).map_err(|e| {
        if d.applied && e.to_string().contains("already holds") {
            "applied - tick Flip +Y / Flip +X and Apply again to align the part the other way".to_owned()
        } else {
            crate::pick_words::plain(&e.to_string())
        }
    });
    ui.horizontal(|ui| {
        ui.label("      Primary");
        let mut primary = d.axes.primary;
        for axis in [InPlaneAxis::Y, InPlaneAxis::X] {
            ui.radio_value(&mut primary, axis, axis.label())
                .on_hover_text("With both axes picked, this one is kept exactly; the other is made square to it.");
        }
        if primary != d.axes.primary {
            axis_pick::draft_for(state, setup).axes.primary = primary;
        }
    });
    // Short lines (#330): the buttons stay on screen in the narrow pane.
    ui.horizontal(|ui| {
        ui.label("     ");
        let tip = match &check {
            Ok(()) => "Align the part from where it sits now so the picks run along their axes (+Z by the right-hand rule; one undo step)."
                .to_owned(),
            Err(why) => why.clone(),
        };
        if ui
            .add_enabled(check.is_ok(), egui::Button::new("Apply axes"))
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            state.pending.push_back(CommandKind::AlignPartFromAxes { setup: setup.to_owned(), axes: axes.clone() });
        }
        if ui.button("Clear axes").on_hover_text("Drop the axis picks.").clicked() {
            state.axis_pick = None;
        }
    });
    let words = match &check {
        Err(why) => why.clone(),
        Ok(()) => match pick::in_plane_turn_deg(&state.world, setup, &axes) {
            // An in-plane pair is a plain turn about Z (#557); anything
            // else is the full 3D alignment (#769).
            Ok(deg) => format!("turns the part {} about Z", deg_words(deg)),
            Err(_) => "aligns the part: +Y and +X along their picks, +Z from the right-hand rule".to_owned(),
        },
    };
    if !words.is_empty() {
        ui.label(RichText::new(format!("      {words}")).weak());
    }
}

fn deg_words(deg: f64) -> String {
    let r = (deg * 1000.0).round() / 1000.0;
    format!("{}°", if r == 0.0 { 0.0 } else { r })
}

/// One axis: what it is picked by, the pick toggle, flip, and the picks.
fn axis_row(ui: &mut Ui, state: &mut State, setup: &str, axis: InPlaneAxis) {
    let draft = state.axis_pick.as_ref().filter(|d| d.setup == setup);
    let by = draft.map_or(AxisBy::Edge, |d| d.by(axis));
    let armed = draft.is_some_and(|d| d.picking == Some(axis));
    let slot = draft.and_then(|d| match axis {
        InPlaneAxis::Y => d.axes.y.clone(),
        InPlaneAxis::X => d.axes.x.clone(),
    });
    let picks = slot.as_ref().map(|s| s.picks.clone()).unwrap_or_default();
    let mut flip = slot.is_some_and(|s| s.flip);
    let name = axis.label();
    ui.horizontal(|ui| {
        ui.label(format!("      {name}"));
        for (b, text) in [(AxisBy::Edge, "Edge"), (AxisBy::Points, "Two points")] {
            if ui
                .selectable_label(by == b, format!("{name} by {text}"))
                .on_hover_text(format!("Set {name} from {} - picks at once.", b.noun()))
                .clicked()
            {
                // Choosing a mode arms its pick (#557 hand check).
                axis_pick::set_by(state, setup, axis, b);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("     ");
        let what = match by {
            AxisBy::Edge => "edge",
            AxisBy::Points => "points",
        };
        let text = if armed { format!("Stop picking {name}") } else { format!("Pick {name} {what}") };
        if super::pick_button::pick_button(ui, armed, text)
            .on_hover_text(format!("Click {} in the viewport; it runs along {name}.", by.noun()))
            .clicked()
        {
            if armed {
                axis_pick::disarm(state);
            } else {
                axis_pick::arm(state, setup, axis);
            }
        }
        if ui
            .checkbox(&mut flip, format!("Flip {name}"))
            .on_hover_text("Take the pick the other way (end to start).")
            .changed()
        {
            axis_pick::draft_for(state, setup).slot(axis).flip = flip;
        }
    });
    // Its own line, so it is never cut short: what and where, in the
    // document's units, never a feature id (#557 hand check).
    let text = crate::axis_pick::picks_text(&state.world, &picks, camcraft_app::units::of(&state.world));
    ui.label(RichText::new(format!("        {name}: {text}")).weak());
}

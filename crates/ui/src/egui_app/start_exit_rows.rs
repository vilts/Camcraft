//! The contour's user-set start and exit points on the "Lead in / Lead
//! out" tab (#1448, #1473 item 2): two optional groups, switched in their
//! header alongside a viewport Pick button and a Clear button. Off writes
//! `None` (the automatic seam, as every earlier file); toggling on seeds
//! the origin for typing, while viewport picking leaves `None` until a
//! part or stock click lands. Draws into the draft params; the page's
//! Apply / Generate commits them through `EditOperationParams`, so undo
//! puts the old point back. The generator owns the law: a point it
//! cannot hold is refused by name.
//!
//! Provenance: our own UI over the general machining practice of a
//! chosen entry and exit position on a closed profile; no vendor
//! material or text.

use egui::{Align, Layout, Margin, RichText, Ui};

use camcraft_app::geo2d::Point2;
use camcraft_app::links::Approach;
use camcraft_app::units::Unit;

use crate::start_exit_pick::{StartExitAct, StartExitRole, act_id, armed_id};

use super::fields::num;
use super::layout::{GROUP_GAP, consequence, hint, prop_row};
use super::pick_button::pick_button;

/// The two group titles, as the tab's structural tests find them.
pub(super) const START_GROUP: &str = "Start point";
pub(super) const EXIT_GROUP: &str = "Exit point";

struct PointRows {
    role: StartExitRole,
    title: &'static str,
    x: &'static str,
    y: &'static str,
    pick: &'static str,
    pick_help: &'static str,
    clear: &'static str,
    clear_help: &'static str,
    help: &'static str,
    says: &'static str,
}

const START: PointRows = PointRows {
    role: StartExitRole::Start,
    title: START_GROUP,
    x: "Start X",
    y: "Start Y",
    pick: "Pick start",
    pick_help: "click a face, edge or corner in the viewport to place the Contour start point",
    clear: "Clear start",
    clear_help: "reset the start point to automatic",
    help: "a point in model X and Y: the nearest closed contour starts its cut where this point meets its wall",
    says: "the lead-in goes in here; where it cannot fit, the operation is refused by name - the start never moves",
};

const EXIT: PointRows = PointRows {
    role: StartExitRole::Exit,
    title: EXIT_GROUP,
    x: "Exit X",
    y: "Exit Y",
    pick: "Pick exit",
    pick_help: "click a face, edge or corner in the viewport to place the Contour exit point",
    clear: "Clear exit",
    clear_help: "reset the exit point to automatic",
    help: "a point in model X and Y: the nearest closed contour is cut all the way round, then on to where this point meets its wall",
    says: "the lead-out leaves here; where it cannot fit, the operation is refused by name - the exit never moves",
};

/// Both groups, for the closed 2D contour's lead tab.
pub(super) fn start_exit_groups(ui: &mut Ui, u: Unit, a: &mut Approach) {
    point_group(ui, u, &START, &mut a.start_point);
    point_group(ui, u, &EXIT, &mut a.exit_point);
}

fn point_group(ui: &mut Ui, u: Unit, rows: &PointRows, p: &mut Option<Point2>) {
    let was_on = p.is_some();
    let mut on = was_on;
    let mut q = p.unwrap_or(Point2 { x: 0.0, y: 0.0 });
    let armed: Option<StartExitRole> = ui.ctx().data(|d| d.get_temp(armed_id())).flatten();
    let is_armed = armed == Some(rows.role);
    let pick_label = if is_armed { "Stop picking" } else { rows.pick };
    let mut picked = false;
    let mut cleared = false;

    let id = ui.make_persistent_id(("camcraft.group", rows.title));
    let collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let header = collapsing.show_header(ui, |ui| {
        ui.checkbox(&mut on, "").on_hover_text(format!("use {}", rows.title));
        ui.label(RichText::new(rows.title).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pick_button(ui, is_armed, pick_label).on_hover_text(rows.pick_help).clicked() {
                picked = true;
            }
        });
    });
    header.body(|ui| {
        if on {
            egui::Frame::NONE
                .inner_margin(Margin { left: 2, right: 0, top: 0, bottom: 2 })
                .show(ui, |ui| {
                    prop_row(ui, rows.x, |ui| num(ui, u, &mut q.x));
                    hint(ui, rows.help);
                    prop_row(ui, rows.y, |ui| num(ui, u, &mut q.y));
                    hint(ui, rows.help);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button(rows.clear).on_hover_text(rows.clear_help).clicked() {
                            cleared = true;
                        }
                    });
                    consequence(ui, rows.says);
                });
        }
    });
    ui.add_space(GROUP_GAP);

    if cleared {
        on = false;
    }
    *p = on.then_some(q);
    if picked {
        ui.ctx().data_mut(|d| d.insert_temp(act_id(), Some(StartExitAct::Toggle(rows.role))));
        ui.ctx().request_repaint();
    } else if was_on && !on {
        ui.ctx().data_mut(|d| d.insert_temp(act_id(), Some(StartExitAct::ClearIf(rows.role))));
        ui.ctx().request_repaint();
    } else if was_on != on {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
#[path = "start_exit_tests.rs"]
mod tests;

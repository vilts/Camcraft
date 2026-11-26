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

use egui::{Align, Label, Layout, Margin, RichText, Ui};

use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{OpParams, OpType, PickCapability};
use camcraft_app::path_picks::{PathPick, PathPicks, PickRole};
use camcraft_app::units::Unit;

use crate::start_exit_pick::{StartExitAct, StartExitRole, act_id, armed_id};

use super::fields::num;
use super::layout::{GROUP_GAP, check_row, consequence, group, hint, prop_row, small_button};
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

// #3325: the Slot's words. A slot pick names one of the two ends.
const SLOT_START: PointRows = PointRows {
    pick_help: "click a face, edge or corner in the viewport to place the Slot start point",
    help: "a point in model X and Y: the pass starts at the slot end nearest this point",
    says: "the cutter goes down at that end; a point far from the slot or between its ends is refused by name - the start never moves",
    ..START
};

const SLOT_EXIT: PointRows = PointRows {
    pick_help: "click a face, edge or corner in the viewport to place the Slot exit point",
    help: "a point in model X and Y: the pass stops at the slot end nearest this point",
    says: "the row count decides where a pass can stop; an exit that the start point makes impossible is refused by name - the exit never moves",
    ..EXIT
};

/// Both groups, for a kind whose pick is one of two ends (the Slot, #3325).
pub(super) fn end_groups(ui: &mut Ui, u: Unit, picks: &mut PathPicks) {
    pick_list_group(ui, u, &SLOT_START, &mut picks.start);
    pick_list_group(ui, u, &SLOT_EXIT, &mut picks.exit);
}

/// Both groups, for the closed 2D contour's lead tab.
pub(super) fn start_exit_groups(ui: &mut Ui, u: Unit, picks: &mut PathPicks) {
    pick_list_group(ui, u, &START, &mut picks.start);
    pick_list_group(ui, u, &EXIT, &mut picks.exit);
}

fn pick_list_group(ui: &mut Ui, u: Unit, rows: &PointRows, list: &mut Vec<PathPick>) {
    let mut pt = list.first().map(|p| p.xy);
    point_group(ui, u, rows, &mut pt);
    match pt {
        Some(xy) if list.is_empty() => list.push(PathPick::xy(xy)),
        Some(xy) => {
            if let Some(first) = list.first_mut() {
                first.xy = xy;
            }
        }
        None => list.clear(),
    }
}

/// The wire's threading point uses the same Start point group on Geometry.
pub(super) fn wire_start_group(ui: &mut Ui, u: Unit, picks: &mut PathPicks) {
    group(ui, START_GROUP, None, |ui| {
        let mut stated = !picks.start.is_empty();
        check_row(ui, "Start hole stated", &mut stated);
        match (stated, picks.start.first_mut()) {
            (true, Some(p)) => {
                prop_row(ui, "Start hole X", |ui| num(ui, u, &mut p.xy.x));
                prop_row(ui, "Start hole Y", |ui| num(ui, u, &mut p.xy.y));
            }
            (true, None) => picks.start.push(PathPick::xy(Point2::new(0.0, 0.0))),
            (false, _) => picks.start.clear(),
        }
    });
}

struct HoleRows {
    title: &'static str,
    stated: &'static str,
    x: &'static str,
    y: &'static str,
    says: &'static str,
}

const FIRST_HOLE: HoleRows = HoleRows {
    title: START_GROUP,
    stated: "First hole stated",
    x: "First hole X",
    y: "First hole Y",
    says: "the hole under this point is cut first; the other holes keep the list order from there. A point that is on no hole is refused by name",
};

const LAST_HOLE: HoleRows = HoleRows {
    title: EXIT_GROUP,
    stated: "Last hole stated",
    x: "Last hole X",
    y: "Last hole Y",
    says: "the hole under this point is cut last. A point that is on no hole is refused by name",
};

/// #3328: the first hole and the last hole of a Drill, a Bore or a Thread
/// (`PickCapability::HoleOrder`), typed in model X and Y. Switching a
/// group on seeds the first (or the last) hole of the list, so the seed
/// is a point that the generator takes. Other kinds draw nothing here.
pub(super) fn hole_order_groups(ui: &mut Ui, u: Unit, params: &mut OpParams) {
    if params.kind().info().pick() != PickCapability::HoleOrder {
        return;
    }
    let (first, last) = match params {
        OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => {
            (holes.first().map(|h| h.xy), holes.last().map(|h| h.xy))
        }
        _ => (None, None),
    };
    let Some(picks) = params.path_picks_mut() else {
        return;
    };
    hole_group(ui, u, &FIRST_HOLE, first, &mut picks.start);
    hole_group(ui, u, &LAST_HOLE, last, &mut picks.exit);
}

fn hole_group(ui: &mut Ui, u: Unit, rows: &HoleRows, seed: Option<Point2>, list: &mut Vec<PathPick>) {
    group(ui, rows.title, None, |ui| {
        let mut stated = !list.is_empty();
        check_row(ui, rows.stated, &mut stated);
        match (stated, list.first_mut()) {
            (true, Some(p)) => {
                prop_row(ui, rows.x, |ui| num(ui, u, &mut p.xy.x));
                prop_row(ui, rows.y, |ui| num(ui, u, &mut p.xy.y));
                consequence(ui, rows.says);
            }
            (true, None) => list.push(PathPick::xy(seed.unwrap_or(Point2::new(0.0, 0.0)))),
            (false, _) => list.clear(),
        }
    });
}

/// A saved wire exit stays visible and editable beside the threading point.
/// Apply commits the draft through the same undoable command as other wire fields.
pub(super) fn wire_exit_group(ui: &mut Ui, u: Unit, picks: &mut PathPicks) {
    group(ui, EXIT_GROUP, None, |ui| {
        let mut stated = !picks.exit.is_empty();
        check_row(ui, "Exit point stated", &mut stated);
        if stated {
            if picks.exit.is_empty() {
                picks.exit.push(PathPick::xy(Point2::new(0.0, 0.0)));
            }
            if let Some(p) = picks.exit.first_mut() {
                prop_row(ui, "Exit X", |ui| num(ui, u, &mut p.xy.x));
                prop_row(ui, "Exit Y", |ui| num(ui, u, &mut p.xy.y));
            }
            if small_button(ui, "Clear exit").clicked() {
                stated = false;
            }
            hint(ui, "an open run ends nearest this point; a closed profile completes its lap before leaving at this point");
        }
        if !stated {
            picks.exit.clear();
        }
    });
}

/// #3326: the Face's Start point and Exit point groups on Geometry, typed
/// in model X and Y. Apply commits the draft through the undoable command.
pub(super) fn raster_groups(ui: &mut Ui, u: Unit, picks: &mut PathPicks) {
    let start = ["Start point stated", "Start X", "Start Y", "Clear start"];
    let exit = ["Exit point stated", "Exit X", "Exit Y", "Clear exit"];
    typed_group(ui, u, START_GROUP, start, "the raster starts at the corner of the face nearest this point; the first row and the step direction follow from that corner", &mut picks.start);
    typed_group(ui, u, EXIT_GROUP, exit, "the last row ends on the side nearest this point; a side that the row count cannot reach from the start corner is refused by name", &mut picks.exit);
}

/// One typed pick group: a switch, the X and Y rows, a Clear button and
/// what the pick does. `labels` is [switch, X, Y, clear].
fn typed_group(ui: &mut Ui, u: Unit, title: &str, labels: [&str; 4], says: &str, list: &mut Vec<PathPick>) {
    group(ui, title, None, |ui| {
        let mut stated = !list.is_empty();
        check_row(ui, labels[0], &mut stated);
        if stated {
            if list.is_empty() {
                list.push(PathPick::xy(Point2::new(0.0, 0.0)));
            }
            if let Some(p) = list.first_mut() {
                prop_row(ui, labels[1], |ui| num(ui, u, &mut p.xy.x));
                prop_row(ui, labels[2], |ui| num(ui, u, &mut p.xy.y));
            }
            if small_button(ui, labels[3]).clicked() {
                stated = false;
            }
            hint(ui, says);
        }
        if !stated {
            list.clear();
        }
    });
}

/// #3324: the Start point / Exit point groups of a kind that does not
/// take that pick. Each says so by name, with the registry's reason
/// (`KindInfo::pick`), so no page hides that the planner chooses alone.
pub(super) fn not_taken_groups(ui: &mut Ui, kind: OpType) {
    let cap = kind.info().pick();
    for (role, title) in [(PickRole::Start, START_GROUP), (PickRole::Exit, EXIT_GROUP)] {
        if cap.takes(role) {
            continue;
        }
        let why = match cap {
            PickCapability::NotTaken(why) => why,
            _ => "this kind takes a start point only",
        };
        group(ui, title, None, |ui| {
            ui.add(Label::new(RichText::new(format!("{NOT_TAKEN}{why}")).weak()).wrap());
        });
    }
}

/// The lead words of a not-taken group's line, as the tests find it.
pub(super) const NOT_TAKEN: &str = "Not taken - ";

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

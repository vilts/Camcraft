//! The contour's user-set start and exit points on the "Lead in / Lead
//! out" tab (#1448; #1473 item 2, the typed half - the viewport pick is
//! still owed): two optional groups, switched in their header, each with
//! an X and a Y row in model coordinates. Off writes `None` (the
//! automatic seam, as every earlier file); on seeds the origin for the
//! user to edit. Draws into the draft params; the page's Apply commits
//! them through `EditOperationParams`, so undo puts the old point back.
//! The generator owns the law: a point it cannot hold is refused by name.
//!
//! Provenance: our own UI over the general machining practice of a
//! chosen entry and exit position on a closed profile; no vendor
//! material or text.

use egui::Ui;

use camcraft_app::geo2d::Point2;
use camcraft_app::links::Approach;
use camcraft_app::units::Unit;

use super::fields::num;
use super::layout::{consequence, group, hint, prop_row};

/// The two group titles, as the tab's structural tests find them.
pub(super) const START_GROUP: &str = "Start point";
pub(super) const EXIT_GROUP: &str = "Exit point";

struct PointRows {
    title: &'static str,
    x: &'static str,
    y: &'static str,
    help: &'static str,
    says: &'static str,
}

const START: PointRows = PointRows {
    title: START_GROUP,
    x: "Start X",
    y: "Start Y",
    help: "a point in model X and Y: the nearest closed contour starts its cut where this point meets its wall",
    says: "the lead-in goes in here; where it cannot fit, the operation is refused by name - the start never moves",
};

const EXIT: PointRows = PointRows {
    title: EXIT_GROUP,
    x: "Exit X",
    y: "Exit Y",
    help: "a point in model X and Y: the nearest closed contour is cut all the way round, then on to where this point meets its wall",
    says: "the lead-out leaves here; where it cannot fit, the operation is refused by name - the exit never moves",
};

/// Both groups, for the closed 2D contour's lead tab.
pub(super) fn start_exit_groups(ui: &mut Ui, u: Unit, a: &mut Approach) {
    point_group(ui, u, &START, &mut a.start_point);
    point_group(ui, u, &EXIT, &mut a.exit_point);
}

fn point_group(ui: &mut Ui, u: Unit, rows: &PointRows, p: &mut Option<Point2>) {
    let mut on = p.is_some();
    let mut q = p.unwrap_or(Point2 { x: 0.0, y: 0.0 });
    group(ui, rows.title, Some(&mut on), |ui| {
        prop_row(ui, rows.x, |ui| num(ui, u, &mut q.x));
        hint(ui, rows.help);
        prop_row(ui, rows.y, |ui| num(ui, u, &mut q.y));
        hint(ui, rows.help);
        consequence(ui, rows.says);
    });
    // The header switch is read before the body draws, so `q` holds the
    // seed (switched on) or this frame's edit (already on).
    *p = on.then_some(q);
}

#[cfg(test)]
#[path = "start_exit_tests.rs"]
mod tests;

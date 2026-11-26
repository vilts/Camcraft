//! The turning kinds' Start point / Exit point groups (#3331): a point of
//! the lathe (Z, X) plane, typed as model Z and model X. Off writes no
//! pick (the automatic start and exit, as every earlier file); on seeds
//! the origin for typing. Draws into the draft params; the page's Apply /
//! Generate commits them through `EditOperationParams`, so undo puts the
//! old point back. The generator owns the law: a point it cannot hold is
//! refused by name.
//!
//! Provenance: our own UI over the lathe cycle start point of standard
//! turning practice (machine-tool handbooks); no vendor material or text.

use egui::Ui;

use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{OpParams, PickCapability};
use camcraft_app::path_picks::PathPick;
use camcraft_app::units::Unit;

use super::fields::num;
use super::layout::{check_row, consequence, group, hint, prop_row};
use super::start_exit_rows::{EXIT_GROUP, START_GROUP};

struct Rows {
    title: &'static str,
    stated: &'static str,
    z: &'static str,
    x: &'static str,
    says: &'static str,
}

const START: Rows = Rows {
    title: START_GROUP,
    stated: "Start point stated",
    z: "Start Z",
    x: "Start X",
    says: "the first pass starts here; a point in the stock, on the chuck side or behind the part is refused by name - the start never moves",
};

const EXIT: Rows = Rows {
    title: EXIT_GROUP,
    stated: "Exit point stated",
    z: "Exit Z",
    x: "Exit X",
    says: "the last pass leaves here; a point in the stock, on the chuck side or behind the part is refused by name - the exit never moves",
};

const HELP: &str = "a point in the lathe (Z, X) plane, in model Z and X: it stands in air, outside the bar or past its free end";

/// Both groups, for a kind whose pick is a point of the lathe plane.
/// Draws nothing for every other kind.
pub(super) fn meridian_groups(ui: &mut Ui, u: Unit, params: &mut OpParams) {
    if params.kind().info().pick() != PickCapability::Meridian {
        return;
    }
    let Some(picks) = params.path_picks_mut() else { return };
    point_group(ui, u, &START, &mut picks.start);
    point_group(ui, u, &EXIT, &mut picks.exit);
}

fn point_group(ui: &mut Ui, u: Unit, rows: &Rows, list: &mut Vec<PathPick>) {
    group(ui, rows.title, None, |ui| {
        let mut stated = !list.is_empty();
        check_row(ui, rows.stated, &mut stated);
        match (stated, list.first_mut()) {
            (true, Some(p)) => {
                let mut z = p.z.unwrap_or(0.0);
                prop_row(ui, rows.z, |ui| num(ui, u, &mut z));
                p.z = Some(z);
                prop_row(ui, rows.x, |ui| num(ui, u, &mut p.xy.x));
                hint(ui, HELP);
                consequence(ui, rows.says);
            }
            (true, None) => list.push(PathPick { xy: Point2::new(0.0, 0.0), z: Some(0.0) }),
            (false, _) => list.clear(),
        }
    });
}

#[cfg(test)]
#[path = "turning_start_exit_tests.rs"]
mod tests;

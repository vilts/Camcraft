//! The Start point / Exit point groups of the two sinker kinds (#3333,
//! part of #1448), typed in model X and Y on the Geometry tab. A Sinker
//! Burn reads each point as an angle round its burn position: where its
//! orbit starts, and where it ends. Without an orbit the group says by
//! name that the point is not taken. A Sinker Side Burn reads each point
//! as an end of its side move. Draws into the draft params; the page's
//! Apply commits them through `EditOperationParams`, so undo puts the
//! old point back. The generator owns the law: a point that it cannot
//! hold is refused by name.
//!
//! Provenance: our own UI over our own sinker parameters (general
//! die-sink EDM practice); no vendor material or text.

use egui::{Label, RichText, Ui};

use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{OpParams, SinkerOrbitKind};
use camcraft_app::path_picks::PathPick;
use camcraft_app::units::Unit;

use super::fields::num;
use super::layout::{check_row, consequence, group, prop_row};
use super::start_exit_rows::{EXIT_GROUP, NOT_TAKEN, START_GROUP};

/// Why a Sinker Burn without an orbit takes no point, as its groups say it.
pub(super) const NO_ORBIT: &str = "a straight plunge starts and ends at the burn position; state an orbit on the Passes tab";

struct Rows {
    title: &'static str,
    stated: &'static str,
    x: &'static str,
    y: &'static str,
    says: &'static str,
}

const ORBIT_START: Rows = Rows {
    title: START_GROUP,
    stated: "Orbit start stated",
    x: "Orbit start X",
    y: "Orbit start Y",
    says: "the orbit starts toward this point from the burn position; a vector orbit burns the vector toward it first. A point on the burn position, or on no vector, is refused by name",
};

const ORBIT_EXIT: Rows = Rows {
    title: EXIT_GROUP,
    stated: "Orbit exit stated",
    x: "Orbit exit X",
    y: "Orbit exit Y",
    says: "the orbit ends toward this point, then returns to the burn position and retracts. A point that the orbit cannot end on is refused by name",
};

const SIDE_START: Rows = Rows {
    title: START_GROUP,
    stated: "Side move start stated",
    x: "Side move start X",
    y: "Side move start Y",
    says: "the electrode plunges at the end of the side move nearer this point: the burn position, or the wall end. A point at the middle of the move is refused by name",
};

const SIDE_EXIT: Rows = Rows {
    title: EXIT_GROUP,
    stated: "Side move exit stated",
    x: "Side move exit X",
    y: "Side move exit Y",
    says: "the electrode retracts at the end of the side move nearer this point: the burn position, or the wall end. A point at the middle of the move is refused by name",
};

/// Both groups of a Sinker Burn or a Sinker Side Burn. Switching a group
/// on seeds the automatic choice, so the seed is a point that the
/// generator takes. Other kinds draw nothing here.
pub(super) fn sinker_pick_groups(ui: &mut Ui, u: Unit, params: &mut OpParams) {
    match params {
        OpParams::SinkerBurn(p) => {
            let at = Point2::new(p.x, p.y);
            let Some(o) = p.orbit else {
                // A stale point stays visible, so that the user can switch it off.
                for (rows, list) in [(&ORBIT_START, &mut p.picks.start), (&ORBIT_EXIT, &mut p.picks.exit)] {
                    if list.is_empty() {
                        group(ui, rows.title, None, |ui| {
                            ui.add(Label::new(RichText::new(format!("{NOT_TAKEN}{NO_ORBIT}")).weak()).wrap());
                        });
                    } else {
                        point_group(ui, u, rows, at, list);
                    }
                }
                return;
            };
            // The automatic orbit starts at +X. A circle closes there; the
            // last vector of a vector orbit is the one before +X.
            let start = Point2::new(at.x + o.radius, at.y);
            let exit = match o.kind {
                SinkerOrbitKind::Vector if o.vectors > 1 => {
                    let a = std::f64::consts::TAU * f64::from(o.vectors - 1) / f64::from(o.vectors);
                    Point2::new(at.x + o.radius * a.cos(), at.y + o.radius * a.sin())
                }
                _ => start,
            };
            point_group(ui, u, &ORBIT_START, start, &mut p.picks.start);
            point_group(ui, u, &ORBIT_EXIT, exit, &mut p.picks.exit);
        }
        OpParams::SinkerSideBurn(s) => {
            // The automatic side burn plunges and retracts at the burn position.
            let at = Point2::new(s.burn.x, s.burn.y);
            point_group(ui, u, &SIDE_START, at, &mut s.burn.picks.start);
            point_group(ui, u, &SIDE_EXIT, at, &mut s.burn.picks.exit);
        }
        _ => {}
    }
}

fn point_group(ui: &mut Ui, u: Unit, rows: &Rows, seed: Point2, list: &mut Vec<PathPick>) {
    group(ui, rows.title, None, |ui| {
        let mut stated = !list.is_empty();
        check_row(ui, rows.stated, &mut stated);
        match (stated, list.first_mut()) {
            (true, Some(p)) => {
                prop_row(ui, rows.x, |ui| num(ui, u, &mut p.xy.x));
                prop_row(ui, rows.y, |ui| num(ui, u, &mut p.xy.y));
                consequence(ui, rows.says);
            }
            (true, None) => list.push(PathPick::xy(seed)),
            (false, _) => list.clear(),
        }
    });
}

#[cfg(test)]
#[path = "sinker_pick_rows_tests.rs"]
mod tests;

//! The Setups page's "Lay on a plane - pick three points" row (#555): pick
//! points 1, 2 and 3 in the viewport (model corners), choose G17 (XY),
//! G18 (ZX) or G19 (YZ), lay them down on it (the default, #606) or up,
//! optionally set point 1 onto the plane's
//! level, optionally take the 3-2-1 frame (point 1 the origin, 1 -> 2
//! along +X), and Apply. Distinct from the face row above it (#554): that
//! one takes a face, this one points. The draft is view state
//! (`crate::points_pick`); Apply queues `CommandKind::LayPartOnPoints`,
//! the undoable `AlignPartToSetup`. The app says whether the points make
//! a plane and, if not, why - shown on the disabled Apply. The work
//! offset is not touched.
//!
//! Provenance: our own UI over the 3-2-1 datum practice of fixture
//! design; no vendor material consulted.

use egui::{ComboBox, RichText, Ui};

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::pick::{self, PlaneLay, PointsLay};

use crate::pick_words;
use crate::state::{CommandKind, State};

const PLANES: [ArcPlane; 3] = [ArcPlane::Xy, ArcPlane::Xz, ArcPlane::Yz];

/// The row's options, kept between frames (view state).
#[derive(Clone, Copy)]
struct Draft {
    plane: ArcPlane,
    flip: bool,
    place: bool,
    frame: bool,
}

impl Default for Draft {
    fn default() -> Self {
        // #606: points down by default, like the face row - the plane
        // through them rests on the table. A view default only.
        Draft { plane: ArcPlane::Xy, flip: true, place: true, frame: false }
    }
}

/// One setup row's control.
pub(super) fn row(ui: &mut Ui, state: &mut State, setup: &str) {
    let id = ui.id().with(("setup_points_draft", setup));
    let mut d: Draft = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, Draft::default));
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664: the rows are on screen
    }
    let armed = state.points_pick.as_ref().is_some_and(|p| p.setup == setup);
    // Short lines like the face row (#330): its options stay on screen in
    // the narrow properties pane.
    ui.label("    Lay on a plane - pick three points");
    ui.horizontal(|ui| {
        ui.label("     ");
        let pick_text = if armed { "Stop picking points" } else { "Pick three points" };
        if super::pick_button::pick_button(ui, armed, pick_text)
            .on_hover_text("Click three corners of the model in the viewport; the plane through them is laid on the plane.")
            .clicked()
        {
            if armed {
                crate::points_pick::disarm(state);
            } else {
                crate::points_pick::arm(state, setup);
            }
        }
        ComboBox::from_id_salt(("setup_points_plane", setup))
            .selected_text(pick::plane_label(d.plane))
            .show_ui(ui, |ui| {
                for p in PLANES {
                    ui.selectable_value(&mut d.plane, p, pick::plane_label(p));
                }
            })
            .response
            .on_hover_text("The plane the three points are laid on.");
    });
    ui.horizontal(|ui| {
        ui.label("     ");
        ui.checkbox(&mut d.flip, "Points down").on_hover_text(
            "On: the points' outside faces down onto the plane (-Z for G17 rests them on the table). \
             Off: it looks out along +Z / +Y / +X.",
        );
        ui.checkbox(&mut d.place, "Point 1 to plane level")
            .on_hover_text("Set the points onto the plane (e.g. Z 0 for G17); off keeps the part where it is.");
    });
    ui.horizontal(|ui| {
        ui.label("     ");
        ui.checkbox(&mut d.frame, "3-2-1 frame").on_hover_text(
            "Also turn the part so point 1 -> 2 runs along +X (+Y for G19) and move point 1 to the origin.",
        );
    });
    if armed {
        points_lines(ui, state, setup, d);
    }
    ui.data_mut(|m| m.insert_temp(id, d));
}

/// The armed row: the three point prompts, Remove last, Clear, Apply.
fn points_lines(ui: &mut Ui, state: &mut State, setup: &str, d: Draft) {
    let picks = state.points_pick.as_ref().map(|p| p.picks.clone()).unwrap_or_default();
    // #738: the count, then each point where it is - never its id.
    ui.label(RichText::new(format!("      {}", pick_words::selected(&picks))).weak());
    let unit = camcraft_app::units::of(&state.world);
    for k in 0..3 {
        let text = match picks.get(k) {
            Some(g) => crate::axis_pick::point_words(&state.world, g, unit),
            None if k == picks.len() => "click a corner in the viewport".to_owned(),
            None => "-".to_owned(),
        };
        ui.label(RichText::new(format!("      Point {}: {text}", k + 1)).weak());
    }
    let pl = PointsLay { lay: PlaneLay { plane: d.plane, flip: d.flip, place: d.place }, frame: d.frame };
    // The command's own check: a refused pick, or one that gives the
    // alignment the setup already holds, greys Apply out with the why.
    let check = AlignPartToSetup::check_points(&state.world, setup, &picks, pl).map_err(|e| pick_words::plain(&e.to_string()));
    let side = pick::points_side(&state.world, &picks).ok();
    ui.horizontal(|ui| {
        ui.label("     ");
        if ui
            .add_enabled(!picks.is_empty(), egui::Button::new("Remove last"))
            .on_hover_text("Drop the last point picked.")
            .clicked()
        {
            crate::points_pick::remove_last(state);
        }
        if ui
            .add_enabled(!picks.is_empty(), egui::Button::new("Clear points"))
            .on_hover_text("Drop every point picked and start again from point 1.")
            .clicked()
        {
            crate::points_pick::clear(state);
        }
        let tip = match &check {
            Ok(()) => format!(
                "Turn the part from where it sits now so the three points' plane lies on {} (one undo step).",
                pick::plane_label(d.plane)
            ),
            Err(why) => why.clone(),
        };
        if ui
            .add_enabled(check.is_ok(), egui::Button::new("Apply points"))
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            state.pending.push_back(CommandKind::LayPartOnPoints { setup: setup.to_owned(), picks: picks.clone(), lay: pl });
        }
    });
    let words = match (&check, &side) {
        (Err(why), _) if picks.len() == 3 => why.clone(),
        (_, Some(side)) => side.clone(),
        _ => String::new(),
    };
    if !words.is_empty() {
        ui.label(RichText::new(format!("      {words}")).weak())
            .on_hover_text("The side away from the part's material is the points' outside: it faces down onto the plane (or up, with Points down off).");
    }
}

//! The Setups orientation section (#771): two dropdowns - "Plane
//! orientation" and "Orientation point" - each expanding its method's
//! controls below it, replacing the five scattered alignment rows. The
//! methods are the existing flows drawn verbatim (axes #769, points
//! #555, face #330, the preset/angles picker #326/#606); the
//! completing work is the cylinder angle (two curved walls name an
//! axis; it lands on a chosen world axis, turned by an angle about it)
//! and the origin point (a box reading - the parts' bounds or the
//! setup's stock - or a picked vertex; the WCS's rotation is kept,
//! the frame comes from the plane side).
//!
//! ONE section, mounted everywhere a setup's orientation is edited:
//! the Setups overview (every setup stacked) and each setup's own
//! Settings page - same dropdowns, same options, same look (#771, the
//! user's law). View state only; Apply queues the undoable commands
//! (`CommandKind::AlignPartCylinder`, `CommandKind::PlaceSetupOrigin`).
//!
//! Provenance: our own UI over plain fixture practice (the 3-2-1
//! setup: plane first, then the point); no vendor material consulted.

use egui::{ComboBox, RichText, Ui};

use camcraft_app::commands::{AlignPartToSetup, BoxPoint, OriginAt};
use camcraft_app::pick::TargetAxis;

use crate::state::{CommandKind, State};

/// How the part's frame is set.
#[derive(Clone, Copy, PartialEq)]
enum PlaneMethod {
    Axes,
    Points,
    Face,
    Manual,
    Cylinder,
}

impl PlaneMethod {
    const ALL: [PlaneMethod; 5] = [
        PlaneMethod::Axes,
        PlaneMethod::Points,
        PlaneMethod::Face,
        PlaneMethod::Manual,
        PlaneMethod::Cylinder,
    ];

    fn label(self) -> &'static str {
        match self {
            PlaneMethod::Axes => "Axes",
            PlaneMethod::Points => "Points",
            PlaneMethod::Face => "Face",
            PlaneMethod::Manual => "Manual",
            PlaneMethod::Cylinder => "Cylinder angle",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            PlaneMethod::Axes => "Two picks - an edge or two points for +Y and +X - define the frame in full 3D.",
            PlaneMethod::Points => "Three points lay the part on a plane through them.",
            PlaneMethod::Face => "A picked flat face lays on G17 / G18 / G19.",
            PlaneMethod::Manual => "The preset combo and typed angles about the parts' centre or the WCS origin.",
            PlaneMethod::Cylinder => "Two curved walls name a bore's or cylinder's axis; it lands on a chosen axis, turned by an angle about it.",
        }
    }
}

/// Where the setup's WCS origin sits.
#[derive(Clone, Copy, PartialEq)]
enum PointMethod {
    ModelBox,
    StockBox,
    ManualPick,
}

impl PointMethod {
    const ALL: [PointMethod; 3] = [PointMethod::ModelBox, PointMethod::StockBox, PointMethod::ManualPick];

    fn label(self) -> &'static str {
        match self {
            PointMethod::ModelBox => "From model box",
            PointMethod::StockBox => "From stock box",
            PointMethod::ManualPick => "Point selection",
        }
    }
}

/// The orientation section of one setup: two dropdowns, each expanding
/// its method's controls below. Drawn identically wherever a setup's
/// orientation is edited.
pub(super) fn section(ui: &mut Ui, state: &mut State, setup: &str) {
    plane_side(ui, state, setup);
    ui.add_space(4.0);
    point_side(ui, state, setup);
}

fn plane_side(ui: &mut Ui, state: &mut State, setup: &str) {
    // The id must be read once, outside the horizontal: a child Ui (the
    // horizontal's own) would name a different temp slot than the match
    // below reads, and the dropdown would never switch the content.
    let id = ui.id().with(("orient_plane", setup));
    let mut method: PlaneMethod = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, || PlaneMethod::Axes));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Plane orientation").strong());
        ComboBox::from_id_salt(id)
            .selected_text(method.label())
            .show_ui(ui, |ui| {
                ui.set_min_width(150.0);
                for m in PlaneMethod::ALL {
                    ui.selectable_value(&mut method, m, m.label()).on_hover_text(m.hint());
                }
            });
    });
    ui.data_mut(|m| m.insert_temp(id, method));
    match method {
        PlaneMethod::Axes => super::setup_axis_pick::rows(ui, state, setup),
        PlaneMethod::Points => super::setup_points_pick::row(ui, state, setup),
        PlaneMethod::Face => super::setup_plane_pick::row(ui, state, setup),
        PlaneMethod::Manual => super::setup_align_pick::picker(ui, state, setup),
        PlaneMethod::Cylinder => cylinder_panel(ui, state, setup),
    }
}

fn point_side(ui: &mut Ui, state: &mut State, setup: &str) {
    // Same law as plane_side: one id, read outside the horizontal.
    let id = ui.id().with(("orient_point", setup));
    let mut method: PointMethod = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, || PointMethod::ModelBox));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Orientation point").strong());
        ComboBox::from_id_salt(id)
            .selected_text(method.label())
            .show_ui(ui, |ui| {
                ui.set_min_width(150.0);
                for m in PointMethod::ALL {
                    ui.selectable_value(&mut method, m, m.label());
                }
            })
            .response
            .on_hover_text(
                "Where this setup's WCS origin sits. The frame comes from the plane side; \
                 the point only moves the origin, the rotation is kept.",
            );
    });
    ui.data_mut(|m| m.insert_temp(id, method));
    match method {
        PointMethod::ModelBox => box_panel(ui, state, setup, false),
        PointMethod::StockBox => box_panel(ui, state, setup, true),
        PointMethod::ManualPick => vertex_panel(ui, state, setup),
    }
}

/// The cylinder-angle method's controls (#771): pick one curved wall,
/// choose the axis it lands on and the angle about it, Apply.
fn cylinder_panel(ui: &mut Ui, state: &mut State, setup: &str) {
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    #[derive(Clone, Copy)]
    struct CylDraft {
        target: TargetAxis,
        angle: f64,
        flip: bool,
    }
    impl Default for CylDraft {
        fn default() -> Self {
            CylDraft { target: TargetAxis::Z, angle: 0.0, flip: false }
        }
    }
    let id = ui.id().with(("setup_cylinder", setup));
    let mut d: CylDraft = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, CylDraft::default));
    ui.label("    Pick the bore's or cylinder's curved wall - its facet normals name the axis");
    let mine = state.cylinder_pick.as_ref().is_some_and(|p| p.setup == setup);
    let armed = mine && crate::cylinder_pick::collecting(state);
    ui.horizontal(|ui| {
        let pick = if armed { "Pick wall (picking...)" } else { "Pick wall" };
        if super::pick_button::pick_button(ui, armed, pick).clicked() {
            if armed {
                crate::cylinder_pick::disarm(state);
            } else {
                crate::cylinder_pick::arm(state, setup);
            }
        }
        if mine && ui.button("Clear wall").clicked() {
            crate::cylinder_pick::disarm(state);
        }
    });
    if let Some(p) = state.cylinder_pick.as_ref().filter(|p| p.setup == setup)
        && let Some(g) = &p.pick
    {
        ui.label(format!("      wall: {} of '{}'", g.feature, g.entity));
    }
    ui.horizontal(|ui| {
        ui.label("      Lands on");
        ComboBox::from_id_salt(id.with("target"))
            .selected_text(d.target.label())
            .show_ui(ui, |ui| {
                for t in [TargetAxis::X, TargetAxis::Y, TargetAxis::Z] {
                    ui.selectable_value(&mut d.target, t, t.label());
                }
            });
        ui.add(egui::DragValue::new(&mut d.angle).speed(1.0).suffix("°"))
            .on_hover_text("The angle about the axis it landed on, applied after the landing.");
        ui.checkbox(&mut d.flip, "Flip axis").on_hover_text("Run the wall's axis the other way.");
        ui.data_mut(|m| m.insert_temp(id, d));
    });
    let pick = state
        .cylinder_pick
        .as_ref()
        .filter(|p| p.setup == setup)
        .and_then(|p| p.pick.clone());
    let check = pick
        .as_ref()
        .map(|g| AlignPartToSetup::check_cylinder(&state.world, setup, g, d.target, d.angle, d.flip))
        .unwrap_or_else(|| Err(camcraft_app::commands::CommandError::Invalid("pick the wall first".into())));
    let words = match &check {
        Ok(()) => format!("applies the wall's axis onto {}", d.target.label()),
        Err(why) => why.to_string(),
    };
    ui.label(RichText::new(format!("      {words}")).weak());
    ui.horizontal(|ui| {
        if ui
            .add_enabled(check.is_ok(), egui::Button::new("Apply cylinder"))
            .clicked()
            && let Some(g) = pick
        {
            state.pending.push_back(CommandKind::AlignPartCylinder {
                setup: setup.to_owned(),
                pick: g,
                target: d.target,
                angle_deg: d.angle,
                flip: d.flip,
            });
        }
    });
}

/// A box reading's controls (#771): corner or centre, Apply.
fn box_panel(ui: &mut Ui, state: &mut State, setup: &str, stock: bool) {
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    let id = ui.id().with((if stock { "orient_stock" } else { "orient_model" }, setup));
    let mut point: BoxPoint = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, || BoxPoint::Corner));
    ui.horizontal(|ui| {
        ComboBox::from_id_salt(id)
            .selected_text(point.label())
            .show_ui(ui, |ui| {
                ui.set_min_width(90.0);
                for p in [BoxPoint::Corner, BoxPoint::Centre] {
                    ui.selectable_value(&mut point, p, p.label());
                }
            })
            .response
            .on_hover_text("Which reading of the box the origin takes.");
        ui.data_mut(|m| m.insert_temp(id, point));
        let read = if stock {
            camcraft_app::model::stock_world_aabb_for(&state.world, Some(setup))
        } else {
            camcraft_app::setup_align::solid_aabb_in(&state.world, Some(setup))
        };
        let words = match read {
            Some(b) => format!(
                "box {:.1} .. {:.1} x {:.1} .. {:.1} x {:.1} .. {:.1}",
                b.min.x, b.max.x, b.min.y, b.max.y, b.min.z, b.max.z
            ),
            None => {
                if stock {
                    format!("setup '{setup}' has no stock placed - place one on the Stock page")
                } else {
                    "there is no model to read a box from - import one first".to_owned()
                }
            }
        };
        ui.label(RichText::new(words).weak());
    });
    let at = if stock { OriginAt::StockBox(point) } else { OriginAt::ModelBox(point) };
    if ui.button("Place origin").clicked() {
        state.pending.push_back(CommandKind::PlaceSetupOrigin {
            setup: setup.to_owned(),
            at,
        });
    }
}

/// The manual point selection's controls (#771): pick a vertex, Apply.
fn vertex_panel(ui: &mut Ui, state: &mut State, setup: &str) {
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    ui.label("    Pick a vertex in the viewport for the WCS origin");
    let mine = state.origin_pick.as_ref().is_some_and(|p| p.setup == setup);
    let armed = mine && crate::origin_pick::collecting(state);
    ui.horizontal(|ui| {
        let pick = if armed { "Pick point (picking...)" } else { "Pick point" };
        if super::pick_button::pick_button(ui, armed, pick).clicked() {
            if armed {
                crate::origin_pick::disarm(state);
            } else {
                crate::origin_pick::arm(state, setup);
            }
        }
        if mine && ui.button("Clear point").clicked() {
            crate::origin_pick::disarm(state);
        }
    });
    if let Some(p) = state.origin_pick.as_ref().filter(|p| p.setup == setup)
        && let Some(g) = &p.pick
    {
        ui.label(format!("      point: {} of '{}'", g.feature, g.entity));
        if ui.button("Place origin").clicked() {
            state.pending.push_back(CommandKind::PlaceSetupOrigin {
                setup: setup.to_owned(),
                at: OriginAt::Vertex(g.clone()),
            });
        }
    }
}

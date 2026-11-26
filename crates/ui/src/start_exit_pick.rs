//! The 2D Contour page's Start point / Exit point viewport pick (#1448,
//! #1473 item 2): while armed on the Lead in / Lead out tab, a click on a
//! part or stock face, edge or corner writes its model `(x, y)` into the
//! page draft (`OpUi::params`'s `picks`). Apply / Generate then rides `EditOperationParams`,
//! undoable like any parameter edit; leaving the tab, closing the editor,
//! clearing a point, or applying disarms the pick. Fixtures and tilted
//! contours are refused by name and change nothing.
//!
//! Provenance: our own viewport pick over general CAM entry/exit point
//! selection on a 2D profile; no vendor material consulted.

use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{self, OpParams};
use camcraft_app::pick::FeaturePick;
use camcraft_app::units;

use crate::interact::project_uv;
use crate::state::{State, UiTarget};

/// Which contour point a viewport pick sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartExitRole {
    Start,
    Exit,
}

impl StartExitRole {
    /// Human-readable noun used in banners, status lines and errors.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "start point",
            Self::Exit => "exit point",
        }
    }
}

/// UI action emitted by the Lead tab rows for `op_page` to apply to `State`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartExitAct {
    Toggle(StartExitRole),
    ClearIf(StartExitRole),
}

pub(crate) fn armed_id() -> egui::Id {
    egui::Id::new("camcraft.start_exit_pick.armed")
}

pub(crate) fn act_id() -> egui::Id {
    egui::Id::new("camcraft.start_exit_pick.act")
}

/// Arm viewport picking for `role` without inventing a coordinate.
pub fn arm(state: &mut State, role: StartExitRole) {
    state.op_ui.start_exit_pick = Some(role);
    state.status = format!("Picking {}: click on the part or stock in the viewport", role.label());
}

/// Disarm viewport picking.
pub fn disarm(state: &mut State) {
    state.op_ui.start_exit_pick = None;
}

/// Toggle `role` on/off, or switch from the other role.
pub fn toggle(state: &mut State, role: StartExitRole) {
    if state.op_ui.start_exit_pick == Some(role) {
        disarm(state);
        state.status = format!("Stopped picking {}", role.label());
    } else {
        arm(state, role);
    }
}

/// Whether viewport clicks currently route to Start/Exit point picking.
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.op_ui.start_exit_pick.is_some()
        && !state.op_ui.name.is_empty()
        && state.panels_open.properties
        && state.properties_shown
        && state.op_tab == crate::egui_app::op_tabs::SEC_LEADS
        && matches!(state.op_ui.params, OpParams::Contour { .. } | OpParams::Slot { .. })
        && state.pane_target() == UiTarget::Operation(state.op_ui.name.clone())
}

/// Consume one viewport hit into the draft's start or exit point; fixtures,
/// non-finite picks, and tilted contours are refused by name and write nothing.
pub fn take(state: &mut State, found: &FeaturePick) {
    let Some(role) = state.op_ui.start_exit_pick else {
        return;
    };
    if model::find_by_name(&state.world, &found.entity)
        .is_some_and(|e| model::is_fixture(&state.world, &e))
    {
        state.status = format!(
            "'{}' is a fixture - click on the part or stock for the {}",
            found.entity,
            role.label()
        );
        return;
    }
    let [x, y, z] = found.at;
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        state.status = format!("that pick on '{}' has no finite position", found.entity);
        return;
    }
    // #3325: the Slot takes the same pick as the 2D Contour.
    let kind = state.op_ui.params.kind().info().label;
    let (OpParams::Contour { picks, tilt, .. } | OpParams::Slot { picks, tilt, .. }) = &mut state.op_ui.params else {
        state.op_ui.start_exit_pick = None;
        return;
    };
    if tilt.is_some() {
        state.status = format!("{kind}: clear tilt before picking the {}", role.label());
        return;
    }
    let pick = camcraft_app::path_picks::PathPick::xy(Point2::new(x, y));
    let list = match role {
        StartExitRole::Start => &mut picks.start,
        StartExitRole::Exit => &mut picks.exit,
    };
    if let Some(first) = list.first_mut() {
        *first = pick;
    } else {
        list.push(pick);
    }
    let u = state.units();
    state.status = format!(
        "{kind} {}: ({} {}, {} {}) on '{}' - Apply or Generate to commit",
        role.label(),
        units::len(x, u, 3),
        u.as_str(),
        units::len(y, u, 3),
        u.as_str(),
        found.entity,
    );
}

/// Screen-space viewport uv segment `([u0, v0], [u1, v1])`.
pub(crate) type Seg = ([f32; 2], [f32; 2]);

/// Screen-space viewport uv markers for the drafted `(start, exit)` points
/// projected at the contour's resolved `top_z`, shown only while the Contour's
/// Lead in / Lead out tab is visible.
#[must_use]
pub(crate) fn marks(state: &State, aspect: f64) -> (Vec<Seg>, Vec<Seg>) {
    let mut start_segs = Vec::new();
    let mut exit_segs = Vec::new();
    if !state.panels_open.properties
        || !state.properties_shown
        || state.op_ui.name.is_empty()
        || state.op_tab != crate::egui_app::op_tabs::SEC_LEADS
        || state.pane_target() != UiTarget::Operation(state.op_ui.name.clone())
    {
        return (start_segs, exit_segs);
    }
    let (OpParams::Contour { picks, top_z, tilt: None, .. } | OpParams::Slot { picks, top_z, tilt: None, .. }) = &state.op_ui.params else {
        return (start_segs, exit_segs);
    };
    let Some(op) = model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
    else {
        return (start_segs, exit_segs);
    };
    // A display read (#3161): inside this scope a DXF sketch's Z span
    // reads the frame memo, so a still frame does not resample the sketch.
    let Some(z) = operation::display_heights(|| {
        operation::resolve_height(&state.world, op.setup.as_deref(), Some(&op.src), *top_z)
    }) else {
        return (start_segs, exit_segs);
    };
    let mvp = state.cam.mvp(aspect);
    if let Some(p) = picks.first(camcraft_app::path_picks::PickRole::Start).map(|k| k.xy).filter(|p| p.x.is_finite() && p.y.is_finite()) {
        crate::vertex_pick::vertex_marker(&mvp, [p.x, p.y, z], aspect, &mut start_segs);
    }
    if let Some(p) = picks.first(camcraft_app::path_picks::PickRole::Exit).map(|k| k.xy).filter(|p| p.x.is_finite() && p.y.is_finite()) {
        x_marker(&mvp, [p.x, p.y, z], aspect, &mut exit_segs);
    }
    (start_segs, exit_segs)
}

fn x_marker(
    mvp: &[[f32; 4]; 4],
    world: [f64; 3],
    aspect: f64,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    let Some(c) = project_uv(mvp, world) else {
        return;
    };
    let r = 0.008_f32;
    let rx = r / (aspect.max(1e-3) as f32);
    out.push(([c[0] - rx, c[1] - r], [c[0] + rx, c[1] + r]));
    out.push(([c[0] - rx, c[1] + r], [c[0] + rx, c[1] - r]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::operation::Tilt;
    use camcraft_app::pick::PickTarget;

    fn contour_state() -> State {
        let mut st = State::new();
        crate::egui_app::world::seed(&mut st);
        st.ui_target = UiTarget::Operation("Contour1".into());
        st.sync_ops_ui();
        st.op_tab = crate::egui_app::op_tabs::SEC_LEADS;
        st.properties_shown = true;
        st
    }

    fn wall_hit(entity: &str, at: [f64; 3]) -> FeaturePick {
        FeaturePick {
            entity: entity.to_owned(),
            target: PickTarget::Face { facet: 0, face: 0, sel: None },
            at,
            surface: None,
        }
    }

    #[test]
    fn marks_draw_for_picked_start_and_exit_and_hide_elsewhere() {
        let mut st = contour_state();
        if let OpParams::Contour { picks, .. } = &mut st.op_ui.params {
            picks.start = vec![camcraft_app::path_picks::PathPick::xy(Point2::new(30.0, 0.0))];
            picks.exit = vec![camcraft_app::path_picks::PathPick::xy(Point2::new(60.0, 30.0))];
        }
        let (start, exit) = marks(&st, 1.5);
        assert_eq!(start.len(), 4, "start point renders a 4-segment diamond");
        assert_eq!(exit.len(), 2, "exit point renders a 2-segment X-cross");

        st.op_tab = crate::egui_app::op_tabs::SEC_GEO;
        assert_eq!(marks(&st, 1.5), (Vec::new(), Vec::new()), "marks hide off the Lead tab");

        st.op_tab = crate::egui_app::op_tabs::SEC_LEADS;
        st.properties_shown = false;
        assert_eq!(marks(&st, 1.5), (Vec::new(), Vec::new()), "marks hide when properties are hidden");
    }

    #[test]
    fn tilted_contour_pick_is_refused_by_name() {
        let mut st = contour_state();
        if let OpParams::Contour { tilt, .. } = &mut st.op_ui.params {
            *tilt = Some(Tilt { basis_version: 1, a_deg: 15.0, b_deg: 0.0, origin: [0.0; 3] });
        }
        arm(&mut st, StartExitRole::Start);
        take(&mut st, &wall_hit("Block", [60.0, 18.0, 15.0]));
        assert!(st.status.contains("tilt"), "{}", st.status);
        if let OpParams::Contour { picks, .. } = &st.op_ui.params {
            assert!(picks.start.is_empty());
        }
    }

    #[test]
    fn a_slot_takes_the_viewport_pick_and_shows_its_marks() {
        // #3325: the Slot's Lead tab collects the same pick as the contour's.
        let mut st = contour_state();
        let slot = operation::KindInfo::ALL.iter().position(|k| k.label == "Slot").expect("the Slot");
        st.op_ui.params = crate::operations::retype(&st.op_ui.params, slot);
        assert!(matches!(st.op_ui.params, OpParams::Slot { .. }));
        arm(&mut st, StartExitRole::Exit);
        assert!(collecting(&st), "an armed Slot page collects the click");
        take(&mut st, &wall_hit("Block", [60.0, 18.0, 15.0]));
        assert!(st.status.starts_with("Slot exit point"), "{}", st.status);
        let picks = st.op_ui.params.path_picks();
        assert!(picks.start.is_empty());
        assert_eq!(picks.exit, vec![camcraft_app::path_picks::PathPick::xy(Point2::new(60.0, 18.0))]);
        assert_eq!(marks(&st, 1.5).1.len(), 2, "the exit point renders its X-cross");
    }

    #[test]
    fn non_finite_pick_is_refused_by_name() {
        let mut st = contour_state();
        arm(&mut st, StartExitRole::Exit);
        take(&mut st, &wall_hit("Block", [f64::NAN, 18.0, 15.0]));
        assert!(st.status.contains("finite"), "{}", st.status);
        if let OpParams::Contour { picks, .. } = &st.op_ui.params {
            assert!(picks.exit.is_empty());
        }
    }
}

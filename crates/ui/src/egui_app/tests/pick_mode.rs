//! #1514: operation picking is a mode. A viewport click changes the open
//! operation's geometry only while its Geometry tab shows (or a pick row is
//! armed); on every other tab the selection is locked - the click changes
//! nothing and does not fall through to a model select that would swap the
//! dialog - and the dialog says plainly "Picking on" / "Picking off". The
//! change itself stays an undoable command; the lock is ephemeral.
//! Provenance: our own pick-mode law test over our own shell; no vendor
//! material consulted.

use super::*;
use crate::interact::Mods;
use camcraft_app::operation::GeoSel;

const OFF: &str = "Picking off - selection locked";
const ON: &str = "Picking on - clicks edit the selection";

fn picks(h: &Harness<'static, ShellUi>) -> Vec<GeoSel> {
    let st = &h.state().state;
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        _ => Vec::new(),
    }
}

/// Contour1 open in Face mode, and a pixel over a face it does not hold.
fn open_with_other_face() -> (Harness<'static, ShellUi>, egui::Pos2, GeoSel) {
    let mut h = shell_harness();
    crate::actions::target(&mut h.state_mut().state, UiTarget::Operation("Contour1".into()));
    h.run();
    h.state_mut().state.geo_pick = GeoPick::Face;
    h.run();
    let held = picks(&h);
    let sh = h.state();
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    for q in [[60.0, 30.0, 15.0], [30.0, 0.0, 15.0], [0.0, 30.0, 15.0], [30.0, 60.0, 15.0], [30.0, 30.0, 30.0]] {
        let Some(uv) = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q) else { continue };
        let p = egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt);
        let ask = crate::feature_click::ask(&sh.state, Mods::default());
        if let Some((found, Some(feature))) = crate::feature_click::feature_at(&sh.state, [p.x, p.y], ask) {
            let g = GeoSel { entity: found.entity, feature };
            if !held.contains(&g) {
                return (h, p, g);
            }
        }
    }
    panic!("no visible face other than Contour1's picks: {held:?}");
}

fn painted_has(h: &Harness<'static, ShellUi>, want: &str) -> bool {
    h.query_all_by_label(want).next().is_some()
}

#[test]
fn a_viewport_click_on_another_tab_leaves_the_selection_and_the_dialog_alone() {
    let (mut h, p, _) = open_with_other_face();
    let before = picks(&h);
    h.state_mut().state.op_tab = 1; // Heights
    h.run_steps(2);
    assert!(interact::op_pick_locked(&h.state().state), "Heights locks the selection");
    click_viewport(&mut h, p);
    assert_eq!(picks(&h), before, "the locked click changed no geometry");
    let st = &h.state().state;
    assert_eq!(st.op_ui.name, "Contour1", "and did not swap the open dialog");
    assert!(st.pending.is_empty(), "no command queued, no Select");
    // Model mode: the same click must not fall through to a model select.
    h.state_mut().state.geo_pick = GeoPick::Model;
    h.run_steps(2);
    click_viewport(&mut h, p);
    assert_eq!(h.state().state.op_ui.name, "Contour1", "no model select closes the dialog");
    assert_eq!(picks(&h), before);
}

#[test]
fn the_same_click_on_the_geometry_tab_changes_the_picks_and_undo_restores_them() {
    let (mut h, p, face) = open_with_other_face();
    let before = picks(&h);
    assert_eq!(h.state().state.op_tab, 0, "the Geometry tab shows");
    assert!(!interact::op_pick_locked(&h.state().state));
    click_viewport(&mut h, p);
    assert_eq!(picks(&h), vec![face], "the click replaced the pick");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(2);
    assert_eq!(picks(&h), before, "undo restores the earlier selection");
}

#[test]
fn an_armed_boundary_row_unlocks_picking_off_the_geometry_tab() {
    let (mut h, _, _) = open_with_other_face();
    h.state_mut().state.op_tab = 1;
    h.run_steps(2);
    assert!(interact::op_pick_locked(&h.state().state));
    h.state_mut().state.boundary_pick = Some(crate::state::BoundaryPick::Open);
    assert!(!interact::op_pick_locked(&h.state().state), "an armed pick row is picking on");
}

#[test]
fn the_dialog_says_whether_picking_is_on_or_off() {
    let (mut h, _, _) = open_with_other_face();
    assert!(painted_has(&h, ON), "Geometry tab: picking on");
    assert!(!painted_has(&h, OFF));
    h.state_mut().state.op_tab = 1;
    h.run_steps(2);
    assert!(painted_has(&h, OFF), "Heights: picking off");
    assert!(!painted_has(&h, ON));
}

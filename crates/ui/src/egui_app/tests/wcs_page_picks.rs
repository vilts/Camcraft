//! #1926: "Set WCS from features" (the picks with roles Origin / X / Y / Z
//! that set the WCS origin and axes) is a group of the WCS page, next to
//! the work offset; the Setup page keeps only the part orientation. An
//! armed row on the WCS page takes viewport picks (the page is not the
//! Setup page, yet its drawn row marks the draft as shown, #664), and a
//! pick set from there places the WCS through the one undoable `SetWcs`.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use egui::accesskit::Role as ARole;
use egui_kittest::kittest::NodeT;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::GeoSel;
use camcraft_app::pick::Role;

use crate::egui_app::wcs_pick_row::GROUP;
use crate::state::SetupPage;

type H = Harness<'static, ShellUi>;

fn wcs_harness() -> H {
    let mut h = Harness::builder().with_size(Vec2::new(1800.0, 2600.0)).build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        let part = ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
        st.history.execute(&st.world, part);
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
    }
    h
}

fn open(h: &mut H, target: UiTarget) {
    crate::actions::target(&mut h.state_mut().state, target);
    h.run_steps(3);
}

/// Press "Pick Z axis" with the pointer. The pointer rests on the button
/// for one frame first, so egui's hit test knows the row before the press.
fn press_pick(h: &mut H) {
    h.get_by_label("Pick Z axis").hover();
    h.run();
    h.get_by_label("Pick Z axis").click();
}

fn top(h: &H, label: &str) -> f32 {
    h.get_by_label(label).rect().top()
}

fn count(h: &H, label: &str) -> usize {
    h.query_all_by_label(label).count()
}

fn pick(st: &State, edge: bool) -> GeoSel {
    let e = model::find_by_name(&st.world, "part").expect("part");
    let f = model::features_of(&st.world, &e).expect("features");
    if !edge {
        let id = f.faces.iter().find(|f| f.normal[2] > 0.99 && (f.offset - 10.0).abs() < 1e-9).map(|f| f.id).expect("the top face");
        return GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) };
    }
    let (a, b) = ([0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let id = camcraft_app::geometry::feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = camcraft_app::geometry::feature::chain_points(&f, c);
            p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
        })
        .map(|c| c.id)
        .expect("the front top edge");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) }
}

#[test]
fn the_wcs_page_shows_the_feature_pick_group_by_the_work_offset_and_the_setup_page_does_not() {
    let _g = snapshot_guard();
    let mut h = wcs_harness();
    open(&mut h, UiTarget::SetupNode("Setup1".into(), SetupPage::Settings));
    assert_eq!(count(&h, GROUP), 0, "the Setup page has no feature-pick group");
    assert_eq!(count(&h, "Pick Z axis"), 0, "the Setup page has no Pick for WCS");
    open(&mut h, UiTarget::Wcs);
    assert_eq!(count(&h, GROUP), 1, "the WCS page has the group");
    assert_eq!(count(&h, "Pick Z axis"), 1, "and its Pick for WCS button");
    let g = top(&h, GROUP);
    assert!(h.query_all_by_label("Anchor on").map(|n| n.rect().top()).fold(f32::MIN, f32::max) < g, "the picks follow the origin, turns and anchor groups");
    assert!(g < top(&h, crate::egui_app::wcs_page::OFFSET_GROUP), "the picks sit just above the work offset");
    assert!(top(&h, "Pick Z axis") > g && top(&h, "Pick Z axis") < top(&h, crate::egui_app::wcs_page::OFFSET_GROUP), "the button sits inside its group");
}

#[test]
fn an_armed_row_on_the_wcs_page_stays_armed_and_leaving_the_page_pauses_it() {
    let _g = snapshot_guard();
    let mut h = wcs_harness();
    open(&mut h, UiTarget::Wcs);
    press_pick(&mut h);
    h.run_steps(5);
    assert!(crate::wcs_pick::collecting(&h.state().state), "armed from the WCS page");
    h.run_steps(5);
    assert!(
        crate::wcs_pick::collecting(&h.state().state),
        "the per-frame sweep keeps a draft whose row the WCS page draws"
    );
    assert_eq!(h.state().state.wcs_pick.as_ref().expect("armed").role, Role::Z);
    open(&mut h, UiTarget::Stock);
    h.run_steps(3);
    assert!(!crate::wcs_pick::collecting(&h.state().state), "a page without the row pauses the draft");
}

#[test]
fn picks_from_the_wcs_page_place_the_wcs_undoably() {
    let _g = snapshot_guard();
    let mut h = wcs_harness();
    open(&mut h, UiTarget::Wcs);
    press_pick(&mut h);
    h.run_steps(3);
    let (z, x) = (pick(&h.state().state, false), pick(&h.state().state, true));
    crate::wcs_pick::take(&mut h.state_mut().state, z);
    h.state_mut().state.wcs_pick.as_mut().expect("armed").role = Role::X;
    crate::wcs_pick::take(&mut h.state_mut().state, x);
    h.run_steps(3);
    assert!(!h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply")).accesskit_node().is_disabled(), "Z and X make a frame");
    h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply")).click();
    h.run_steps(3);
    let st = &mut h.state_mut().state;
    let placed = |st: &State| camcraft_app::setup::wcs_entity(&st.world, Some("Setup1")).and_then(|e| model::transform_of(&st.world, &e));
    assert!(placed(st).is_some(), "the picks placed the WCS: {}", st.status);
    assert!(st.wcs_pick.is_none(), "a landed alignment ends the draft");
    assert!(st.history.undo(&st.world), "one undo step");
    assert!(placed(st).is_none(), "undo restores the unplaced WCS");
}

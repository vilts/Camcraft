//! #701 in the surface: the Setups page's "Set WCS from features" row
//! arms a role draft (each viewport pick is stamped with the chosen
//! role); picks land in the draft - never in the open operation - and
//! arming keeps the toolbar's pick mode, because the row takes faces,
//! edges, corners and walls alike. Apply is live-gated on the app's own
//! role check (the why rides the disabled button) and only answers on
//! the ACTIVE setup's row, because `SetWcs` places the active setup's
//! WCS. Apply queues the one
//! undoable `SetWcs` command with the fitted frame.
//! Provenance: our own page law; no vendor material consulted.

use egui::Vec2;
use egui::accesskit::Role as ARole;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::Role;

use crate::state::{CommandKind, GeoPick, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_part() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn features(state: &State) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(&state.world, "part").expect("part");
    model::features_of(&state.world, &e).expect("features")
}

/// The top face (local z = 10).
fn top_face(state: &State) -> GeoSel {
    let id = features(state)
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.99 && (f.offset - 10.0).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the top face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn edge(state: &State, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let f = features(state);
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
        })
        .map(|c| c.id)
        .expect("the model has that edge");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) }
}

fn corner(state: &State, at: [f64; 3]) -> GeoSel {
    let f = features(state);
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts[vi as usize] == at)
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the model has that corner");
    GeoSel { entity: "part".into(), feature: FeatureSel::Vertex(id) }
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::wcs_pick_row::row(ui, st, "Setup1");
}

/// The same frame drawn for the second setup's page (#701 review round 3:
/// the row stands on every setup's settings page, active or not).
fn frame_setup2(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::wcs_pick_row::row(ui, st, "Setup2");
}

fn wcs(state: &State) -> Option<([f64; 3], [f64; 3])> {
    wcs_of(state, "Setup1")
}

fn wcs_of(state: &State, setup: &str) -> Option<([f64; 3], [f64; 3])> {
    let e = camcraft_app::setup::wcs_entity(&state.world, Some(setup))?;
    let l = model::transform_of(&state.world, &e)?;
    let (r, p, y) = camcraft_app::geometry::quat_to_euler(l.rot);
    Some(([l.pos.x, l.pos.y, l.pos.z], [r, p, y]))
}

fn apply_disabled(h: &Harness<'static, State>) -> bool {
    h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply")).accesskit_node().is_disabled()
}

fn apply_button(h: &Harness<'static, State>) {
    h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply")).click();
}

#[test]
fn the_row_arms_a_draft_and_keeps_the_toolbars_pick_mode() {
    // The row takes faces, edges, corners and walls alike, so arming
    // must not force a pick mode - the user's own toolbar mode stands.
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the row queues nothing");
    h.state_mut().geo_pick = GeoPick::Vertex;
    h.run();
    h.get_by_label("Pick for WCS").click();
    h.run();
    let d = h.state().wcs_pick.clone().expect("the draft is armed");
    assert!(d.picking && d.setup == "Setup1", "{d:?}");
    assert_eq!(d.role, Role::Z, "the first pick defaults to Z");
    assert!(d.picks.is_empty());
    assert_eq!(h.state().geo_pick, GeoPick::Vertex, "arming keeps the toolbar's mode");
    h.get_by_label("Stop picking").click();
    h.run();
    assert!(h.state().wcs_pick.is_none(), "stop drops the draft");
}

#[test]
fn role_picks_land_in_the_draft_and_apply_queues_one_undoable_setwcs() {
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    h.get_by_label("Pick for WCS").click();
    h.run();
    let top = top_face(h.state());
    crate::wcs_pick::take(h.state_mut(), top.clone());
    h.state_mut().wcs_pick.as_mut().expect("armed").role = Role::X;
    let front = edge(h.state(), [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    crate::wcs_pick::take(h.state_mut(), front.clone());
    h.state_mut().wcs_pick.as_mut().expect("armed").role = Role::Origin;
    let c = corner(h.state(), [0.0, 0.0, 10.0]);
    crate::wcs_pick::take(h.state_mut(), c.clone());
    h.run();
    assert!(
        h.state().op_ui.geo.is_empty(),
        "role picks are draft state, not operation picks"
    );
    let d = h.state().wcs_pick.clone().expect("still drafting");
    assert_eq!(d.picks, vec![(top, Role::Z), (front, Role::X), (c, Role::Origin)], "each pick keeps its stamped role");
    assert!(!apply_disabled(&h), "a whole frame lights Apply up");
    apply_button(&h);
    h.run(); // the click queues SetWcs...
    h.run(); // ...and the next frame's drain lands it, as the shell does
    let (pos, angles) = wcs(h.state()).expect("the WCS is placed");
    assert!(pos.iter().zip([0.0, 0.0, 10.0]).all(|(a, b)| (a - b).abs() < 1e-9), "on the corner: {pos:?}");
    assert!(angles.iter().all(|a| a.abs() < 1e-9), "square to the block: {angles:?}");
    assert!(h.state().wcs_pick.is_none(), "a landed alignment ends the draft");
    run(h.state_mut(), CommandKind::Undo);
    h.run();
    assert!(wcs(h.state()).is_none(), "undo restores the unplaced WCS");
    run(h.state_mut(), CommandKind::Redo);
    h.run();
    assert!(wcs(h.state()).is_some(), "redo places it again");
}

#[test]
fn an_incomplete_frame_greys_apply_and_shows_the_apps_why() {
    // The row asks the app per frame, so the refusal words are the same
    // ones the command itself would give.
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    h.get_by_label("Pick for WCS").click();
    h.run();
    assert!(apply_disabled(&h), "nothing picked yet");
    let top = top_face(h.state());
    crate::wcs_pick::take(h.state_mut(), top);
    h.run();
    assert!(apply_disabled(&h), "one axis is no frame");
    let why = h.query_by_label_contains("two axes, plus an optional origin").is_some();
    assert!(why, "the app's own words ride the row");
    let front = edge(h.state(), [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    h.state_mut().wcs_pick.as_mut().expect("armed").role = Role::X;
    crate::wcs_pick::take(h.state_mut(), front);
    h.run();
    assert!(!apply_disabled(&h), "Z and X make a frame");
}

#[test]
fn the_row_on_an_inactive_setup_cannot_move_another_setups_work_zero() {
    // Review round 3: `SetWcs` places the ACTIVE setup's WCS, so the row
    // drawn for an inactive setup must never fire it. This mirrors the
    // reviewer's repro: Setup1 active, the row on Setup2's page, a whole
    // frame picked - and Apply stays grey until Setup2 is the active one.
    let mut state = with_part();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 300.0)).build_ui_state(frame_setup2, state);
    h.run();
    h.get_by_label("Pick for WCS").click();
    h.run();
    let d = h.state().wcs_pick.clone().expect("Setup2's row arms a Setup2 draft");
    assert_eq!(d.setup, "Setup2", "{d:?}");
    let top = top_face(h.state());
    crate::wcs_pick::take(h.state_mut(), top); // the default role is Z
    h.state_mut().wcs_pick.as_mut().expect("armed").role = Role::X;
    let front = edge(h.state(), [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    crate::wcs_pick::take(h.state_mut(), front);
    h.state_mut().wcs_pick.as_mut().expect("armed").role = Role::Origin;
    let origin = corner(h.state(), [20.0, 20.0, 10.0]);
    crate::wcs_pick::take(h.state_mut(), origin);
    h.run();
    assert!(apply_disabled(&h), "a whole frame on an inactive setup's row still greys Apply out");
    assert!(h.state().pending.is_empty(), "the greyed Apply queues nothing");
    let apply = h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply"));
    h.hover_at(apply.rect().center());
    h.run();
    assert!(
        h.query_by_label_contains("Make Setup2 the active setup").is_some(),
        "the greyed Apply says to make the row's own setup active"
    );
    // Make the row's setup active and the same frame places ITS own WCS,
    // while the setup that was active before never moves.
    run(h.state_mut(), CommandKind::ActivateSetup(Some("Setup2".into())));
    h.run();
    assert!(!apply_disabled(&h), "the active setup's row lights Apply up");
    apply_button(&h);
    h.run(); // the click queues SetWcs...
    h.run(); // ...and the next frame's drain lands it, as the shell does
    let (pos, _) = wcs_of(h.state(), "Setup2").expect("Setup2's WCS is placed");
    assert!(
        pos.iter().zip([20.0, 20.0, 10.0]).all(|(a, b)| (a - b).abs() < 1e-9),
        "on the picked corner: {pos:?}"
    );
    assert!(wcs_of(h.state(), "Setup1").is_none(), "the other setup's work zero did not move");
}

#[test]
fn re_picking_a_role_replaces_its_earlier_pick() {
    // One pick per role: the newest Z wins, the other roles stay put,
    // so a mis-click corrects itself without disarming.
    let mut state = with_part();
    crate::wcs_pick::arm(&mut state, "Setup1");
    let top = top_face(&state);
    crate::wcs_pick::take(&mut state, top.clone());
    let bottom = {
        let id = features(&state)
            .faces
            .iter()
            .find(|f| f.normal[2].abs() > 0.99 && f.offset.abs() < 1e-9) // z = 0: sign-canonical normal, zero offset
            .map(|f| f.id)
            .expect("the bottom face");
        GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
    };
    crate::wcs_pick::take(&mut state, bottom.clone());
    let e = edge(&state, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    state.wcs_pick.as_mut().expect("armed").role = Role::X;
    crate::wcs_pick::take(&mut state, e.clone());
    let d = state.wcs_pick.clone().expect("still drafting");
    assert_eq!(d.picks, vec![(bottom, Role::Z), (e, Role::X)], "the newest Z replaces the old one");
}

/// #2403 pin: every picked item (planar face, welded curved patch, edge chain,
/// corner vertex, and stock face) carries a user-friendly name with zero hex
/// ids across WCS, Geometry, Setup pages, status lines, and missing rows, and
/// shows the exact same name on every page while the model stays unchanged.
#[test]
fn picked_items_never_show_hex_ids_and_keep_one_name_across_pages() {
    use camcraft_app::commands::SetStock;
    use camcraft_app::geometry::cylinder;
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;

    fn frame_with_orient(ui: &mut egui::Ui, st: &mut State) {
        super::panes::shortcuts(ui, st);
        st.drain_actions();
        super::wcs_pick_row::row(ui, st, "Setup1");
        super::setup_orient_pick::section(ui, st, "Setup1");
    }

    let mut state = with_part();
    let boss = ImportModel::from_mesh("boss", cylinder(20.0, 10.0, 16));
    state.history.execute(&state.world, boss);
    let stock_cmd = SetStock::new(&state.world, StockShape::Box { dx: 30.0, dy: 30.0, dz: 15.0 }, Location::default())
        .expect("stock");
    state.history.execute(&state.world, stock_cmd);

    let boss_e = model::find_by_name(&state.world, "boss").expect("boss");
    let boss_f = model::features_of(&state.world, &boss_e).expect("boss features");
    let boss_patch = feature::patches(&boss_f).into_iter().next().expect("welded cylinder patch");
    let boss_rim = feature::chains(&boss_f).into_iter().next().expect("rim chain");
    let stock_name = camcraft_app::setup::stock_entity_name(&state.world, Some("Setup1"));
    let stock_e = model::find_by_name(&state.world, &stock_name).expect("stock");
    let stock_face_id = model::features_of(&state.world, &stock_e)
        .expect("stock features")
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .expect("stock top face")
        .id;

    let wall = GeoSel { entity: "boss".into(), feature: FeatureSel::Patch(boss_patch.id) };
    let rim = GeoSel { entity: "boss".into(), feature: FeatureSel::Chain(boss_rim.id) };
    let top = top_face(&state);
    let pt = corner(&state, [0.0, 0.0, 10.0]);
    let stock_top = GeoSel { entity: stock_name.clone(), feature: FeatureSel::Face(stock_face_id) };

    let mut h = Harness::builder().with_size(Vec2::new(1100.0, 500.0)).build_ui_state(frame_with_orient, state);
    h.run();

    let mut wcs_names = Vec::new();
    for pick in [top.clone(), wall.clone(), rim.clone(), pt.clone(), stock_top] {
        // 1. WCS page pick + status line.
        h.get_by_label("Pick for WCS").click();
        h.run();
        crate::wcs_pick::take(h.state_mut(), pick.clone());
        h.run();
        assert!(!h.state().status.contains("0x"), "status leaked hex id: {}", h.state().status);
        assert!(h.query_all_by_label_contains("0x").next().is_none(), "WCS row leaked hex id");
        let picked_line = h
            .query_by_label_contains("Picked: ")
            .and_then(|n| n.accesskit_node().value().or_else(|| n.accesskit_node().label()))
            .expect("Picked line");
        assert!(!picked_line.contains("0x"), "Picked line leaked hex id: {picked_line}");
        let wcs_item = picked_line
            .strip_prefix("      Picked: ")
            .unwrap_or(&picked_line)
            .split(" sets ")
            .next()
            .expect("item before sets")
            .to_owned();
        assert!(!wcs_item.contains("nothing"), "{picked_line}");
        wcs_names.push(wcs_item.clone());

        // 2. Geometry tab chip reads the exact same item name.
        h.state_mut().op_ui.geo = vec![pick.clone()];
        let chips = h.state().geo_chips();
        assert_eq!(chips.len(), 1);
        assert!(!chips[0].missing, "{:?}", chips[0]);
        assert!(!chips[0].label.contains("0x"), "Geometry chip leaked hex: {}", chips[0].label);
        assert!(
            chips[0].label.contains(&wcs_item),
            "Geometry chip ({}) must carry the same item name as WCS ({wcs_item})",
            chips[0].label
        );
        // Re-running frame keeps identical name while model is unchanged.
        h.run();
        let chips_again = h.state().geo_chips();
        assert_eq!(chips_again[0].label, chips[0].label);
        h.get_by_label("Stop picking").click();
        h.run();
    }
    let top_wcs_name = wcs_names[0].clone();
    let wall_wcs_name = wcs_names[1].clone();
    let pt_wcs_name = wcs_names[3].clone();

    // 3. Setup orient section, vertex_pick, and plane refusals show no hex and match WCS name.
    h.get_by(|n| n.role() == ARole::ComboBox && n.value().as_deref() == Some("Axes")).click();
    h.run();
    h.get_by_label("Cylinder angle").click();
    h.run();
    h.get_by(|n| n.role() == ARole::ComboBox && n.value().as_deref() == Some("From model box")).click();
    h.run();
    h.get_by_label("Point selection").click();
    h.run();
    h.state_mut().cylinder_pick = Some(crate::cylinder_pick::CylinderPick {
        setup: "Setup1".into(),
        pick: Some(wall.clone()),
        picking: false,
    });
    h.state_mut().origin_pick = Some(crate::origin_pick::OriginPick {
        setup: "Setup1".into(),
        pick: Some(pt.clone()),
        picking: false,
    });
    h.run();
    assert!(h.query_all_by_label_contains("0x").next().is_none(), "Setup orient leaked hex id");
    assert!(
        h.query_by_label_contains(&format!("wall: {wall_wcs_name}")).is_some(),
        "Setup orient wall label must match WCS name ({wall_wcs_name})"
    );
    assert!(
        h.query_by_label_contains(&format!("point: {pt_wcs_name}")).is_some(),
        "Setup orient point label must match WCS name ({pt_wcs_name})"
    );

    if let FeatureSel::Vertex(vid) = pt.feature {
        crate::vertex_pick::queue_origin_at_vertex(h.state_mut(), &pt.entity, vid);
        assert!(
            h.state().status.contains(&pt_wcs_name) && !h.state().status.contains("0x"),
            "vertex_pick status must share WCS name ({pt_wcs_name}): {}",
            h.state().status
        );
    }
    crate::vertex_pick::queue_origin_at_face(h.state_mut(), &top.entity, top.feature, false);
    assert!(
        h.state().status.contains(&top_wcs_name) && !h.state().status.contains("0x"),
        "face origin status must share WCS name ({top_wcs_name}): {}",
        h.state().status
    );

    crate::plane_pick::arm(h.state_mut(), "Setup1");
    crate::plane_pick::take(h.state_mut(), rim);
    assert!(!h.state().status.contains("0x"), "plane refusal leaked hex: {}", h.state().status);

    // 4. Missing item on deleted body still never shows a hex digest.
    h.state_mut().pending.push_back(CommandKind::DeleteByName("boss".into()));
    h.run();
    h.state_mut().op_ui.geo = vec![wall];
    let missing = h.state().geo_chips();
    assert_eq!(missing.len(), 1);
    assert!(missing[0].missing);
    assert!(!missing[0].label.contains("0x"), "missing chip hex: {}", missing[0].label);
    assert!(!missing[0].label.contains('…'), "missing chip truncated hex id: {}", missing[0].label);
}

//! #555 in the surface: the Setups page's "Lay on a plane - pick three
//! points" row arms the points draft in Vertex mode, points 1, 2, 3 land
//! in it in order (never in an operation), a face, an edge or a fourth
//! point is refused by name, Remove last and Clear walk the draft back,
//! Apply stays greyed with the reason until the points make a plane, and
//! Apply queues `LayPartOnPoints` - one undo unit. The work offset is not
//! touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature, quat_rotate_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::setup_align;

use crate::state::{CommandKind, GeoPick, State};

fn with_part() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
    state.history.execute(&state.world, part);
    state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    state.drain_actions();
    state
}

fn features(state: &State) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(&state.world, "part").expect("part");
    model::features_of(&state.world, &e).expect("features")
}

fn corner(state: &State, p: [f64; 3]) -> GeoSel {
    let f = features(state);
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts.get(vi as usize).is_some_and(|v| *v == p))
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("a corner");
    GeoSel { entity: "part".into(), feature: FeatureSel::Vertex(id) }
}

fn top_face(state: &State) -> GeoSel {
    let id = features(state).faces.iter().find(|f| f.normal[2] > 0.99).map(|f| f.id).expect("the top");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_points_pick::row(ui, st, "Setup1");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder().with_size(Vec2::new(1200.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    h
}

fn disabled(h: &Harness<'static, State>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

fn picks(h: &Harness<'static, State>) -> usize {
    h.state().points_pick.as_ref().map_or(0, |d| d.picks.len())
}

/// Local +Z of the part as Setup1 holds it.
fn held_up(state: &State) -> Option<[f64; 3]> {
    let a = setup_align::stored(&state.world, "Setup1")?;
    let v = quat_rotate_point(a.rot, Point3::new(0.0, 0.0, 1.0));
    Some([v.x, v.y, v.z])
}

#[test]
fn three_points_picked_in_order_lay_the_part_through_the_command_and_undo() {
    let mut h = harness();
    assert!(h.state().pending.is_empty(), "drawing the row queues nothing");
    assert!(h.query_by_label_contains("Lay on a plane - pick three points").is_some(), "the row says what it takes");
    h.get_by_label("Pick three points").click();
    h.run();
    assert!(h.state().points_pick.is_some(), "the draft is armed");
    assert_eq!(h.state().geo_pick, GeoPick::Vertex, "the points row picks vertices");
    assert!(h.query_by_label_contains("Point 1: click a corner").is_some(), "point 1 is prompted");
    assert!(disabled(&h, "Apply points"), "nothing picked: Apply waits");
    assert!(disabled(&h, "Remove last") && disabled(&h, "Clear points"));
    // A face or an edge sets no points: refused by name, nothing drafted.
    let face = top_face(h.state());
    crate::points_pick::take(h.state_mut(), face);
    assert!(h.state().status.contains("is a face"), "{}", h.state().status);
    assert_eq!(picks(&h), 0);
    for p in [[0.0, 0.0, 10.0], [20.0, 0.0, 10.0]] {
        let c = corner(h.state(), p);
        crate::points_pick::take(h.state_mut(), c);
    }
    h.run();
    assert!(h.query_by_label_contains("Point 3: click a corner").is_some(), "point 3 is prompted next");
    assert!(disabled(&h, "Apply points"), "two points make no plane");
    h.get_by_label("Remove last").click();
    h.run();
    assert_eq!(picks(&h), 1, "Remove last drops point 2");
    h.get_by_label("Clear points").click();
    h.run();
    assert_eq!(picks(&h), 0, "Clear drops them all");
    for p in [[0.0, 0.0, 10.0], [20.0, 0.0, 10.0], [20.0, 20.0, 10.0], [0.0, 20.0, 10.0]] {
        let c = corner(h.state(), p);
        crate::points_pick::take(h.state_mut(), c);
    }
    assert_eq!(picks(&h), 3, "a fourth point is refused");
    assert!(h.state().status.contains("Remove last or Clear"), "{}", h.state().status);
    assert!(h.state().op_ui.geo.is_empty(), "a draft pick is not an operation pick");
    h.run();
    // #606: points down by default - the top goes onto the table.
    assert!(!disabled(&h, "Apply points"), "points down, the top rests on the table");
    // Up and kept where it is: the top already faces +Z, nothing would
    // change - the reason is shown on the greyed Apply.
    h.get_by_label("Points down").click();
    h.get_by_label("Point 1 to plane level").click();
    h.run();
    assert!(disabled(&h, "Apply points"), "the top's outside already faces +Z");
    assert!(h.query_by_label_contains("already holds the part that way").is_some(), "the reason is shown");
    h.get_by_label("Point 1 to plane level").click();
    h.get_by_label("Points down").click();
    h.run();
    assert!(!disabled(&h, "Apply points"), "down again, the points rest on the table");
    h.get_by_label("Apply points").click();
    h.run();
    let up = held_up(h.state()).expect("Setup1 holds the part laid");
    assert!((up[2] + 1.0).abs() < 1e-9, "the top now faces the table: {up:?}");
    assert!(h.state().points_pick.is_none(), "a landed lay ends the pick");
    assert!(h.state().status.contains("through three points"), "{}", h.state().status);
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "undo restores as modelled");
    key(&mut h, Key::Y);
    assert!(held_up(h.state()).is_some(), "redo lays it again");
}

#[test]
fn coincident_points_grey_apply_out_and_one_draft_collects_at_a_time() {
    let mut h = harness();
    h.get_by_label("Pick three points").click();
    h.run();
    let a = corner(h.state(), [0.0, 0.0, 10.0]);
    let b = corner(h.state(), [20.0, 0.0, 10.0]);
    for g in [a.clone(), a, b] {
        crate::points_pick::take(h.state_mut(), g);
    }
    h.run();
    assert!(disabled(&h, "Apply points"));
    assert!(h.query_by_label_contains("coincide").is_some(), "the reason is shown");
    // Arming the face row stops the points pick, and back.
    crate::plane_pick::arm(h.state_mut(), "Setup1");
    assert!(h.state().points_pick.is_none());
    crate::points_pick::arm(h.state_mut(), "Setup1");
    assert!(h.state().plane_pick.is_none());
    crate::axis_pick::arm(h.state_mut(), "Setup1", camcraft_app::pick::InPlaneAxis::Y);
    assert!(h.state().points_pick.is_none());
}

/// #738: the row counts its points - "1 point selected", "2 points
/// selected" - and each point line says where it is; no label of the row
/// and no refusal carries a feature's internal id.
#[test]
fn the_pick_status_counts_the_points_and_never_shows_an_id() {
    let mut h = harness();
    h.get_by_label("Pick three points").click();
    h.run();
    let a = corner(h.state(), [0.0, 0.0, 10.0]);
    crate::points_pick::take(h.state_mut(), a);
    h.run();
    assert!(h.query_by_label_contains("1 point selected").is_some(), "one point, singular");
    let b = corner(h.state(), [20.0, 0.0, 10.0]);
    crate::points_pick::take(h.state_mut(), b);
    h.run();
    assert!(h.query_by_label_contains("2 points selected").is_some(), "two points, plural");
    assert!(h.query_by_label_contains("Point 2: on 'part' (20.000, 0.000, 10.000) mm").is_some(), "where point 2 is");
    let face = top_face(h.state());
    crate::points_pick::take(h.state_mut(), face);
    h.run();
    assert!(h.state().status.contains("is a face"), "{}", h.state().status);
    assert!(!h.state().status.contains("0x"), "no id on the status line: {}", h.state().status);
    assert!(h.query_all_by_label_contains("0x").next().is_none(), "no id on the row");
}

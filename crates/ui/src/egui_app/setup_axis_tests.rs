//! #557 in the surface, full 3D since #769: the Setups page's part-axes
//! rows arm the axis draft (Edge mode for an edge, Vertex mode for two
//! points), a pick lands in the draft (never in an operation), a face
//! is refused by name, Apply queues `AlignPartFromAxes` and the drain
//! lands it as one undo unit. The work offset is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature, quat_rotate_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::InPlaneAxis;
use camcraft_app::setup_align;

use crate::axis_pick::AxisBy;
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

/// The top front edge (local y = 0, z = 10), and its walk direction.
fn front_top_edge(state: &State) -> (GeoSel, [f64; 3]) {
    let f = features(state);
    let (id, p) = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            (p.len() == 2 && p.iter().all(|q| q[1] == 0.0 && q[2] == 10.0)).then_some((c.id, p))
        })
        .expect("a top edge");
    (GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) }, [p[1][0] - p[0][0], 0.0, 0.0])
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
    super::setup_axis_pick::rows(ui, st, "Setup1");
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

/// A local direction of the part as Setup1 holds it.
fn held_dir(state: &State, d: [f64; 3]) -> Option<[f64; 3]> {
    let a = setup_align::stored(&state.world, "Setup1")?;
    let v = quat_rotate_point(a.rot, Point3::new(d[0], d[1], d[2]));
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    Some([v.x / l, v.y / l, v.z / l])
}

fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder().with_size(Vec2::new(1000.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    h
}

fn apply_disabled(h: &Harness<'static, State>) -> bool {
    h.get_by_label("Apply axes").accesskit_node().is_disabled()
}

#[test]
fn an_edge_picked_for_y_turns_the_part_through_the_command_and_undoes() {
    let mut h = harness();
    assert!(h.state().pending.is_empty(), "drawing the rows queues nothing");
    assert!(h.query_by_label_contains("pick an edge or two points").is_some(), "the rows say which pick they take");
    h.get_by_label("Pick +Y edge").click();
    h.run();
    assert!(crate::axis_pick::collecting(h.state()), "the draft is armed");
    assert_eq!(h.state().geo_pick, GeoPick::Edge, "an axis by edge picks edges");
    // A face sets no direction: refused by name, nothing drafted.
    let face = top_face(h.state());
    crate::axis_pick::take(h.state_mut(), face);
    assert!(h.state().status.contains("a face sets no direction"), "{}", h.state().status);
    let (edge, d) = front_top_edge(h.state());
    crate::axis_pick::take(h.state_mut(), edge);
    h.run();
    assert!(h.state().op_ui.geo.is_empty(), "a draft pick is not an operation pick");
    assert!(h.query_by_label_contains("turns the part").is_some(), "the turn is shown before Apply");
    assert!(!apply_disabled(&h));
    h.get_by_label("Apply axes").click();
    h.run();
    let v = held_dir(h.state(), d).expect("Setup1 holds the part turned");
    assert!((v[1] - 1.0).abs() < 1e-9, "the edge runs along +Y: {v:?}");
    let kept = h.state().axis_pick.clone().expect("the picks stay after Apply, for Flip");
    assert!(kept.picking.is_none() && kept.applied, "a landed turn stops collecting, the picks kept");
    assert!(apply_disabled(&h), "the same picks again are no turn");
    assert!(h.query_by_label_contains("tick Flip").is_some(), "the greyed Apply says Flip re-applies");
    assert!(h.state().status.contains("aligned from its picks"), "{}", h.state().status);
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "undo restores as modelled");
    key(&mut h, Key::Y);
    assert!(held_dir(h.state(), d).is_some(), "redo turns it again");
}

#[test]
fn two_points_for_x_pick_vertices_and_flip_reverses_them() {
    let mut h = harness();
    h.get_by_label("+X by Two points").click();
    h.run();
    assert_eq!(h.state().geo_pick, GeoPick::Vertex, "two points pick vertices");
    let (edge, _) = front_top_edge(h.state());
    crate::axis_pick::take(h.state_mut(), edge);
    assert!(h.state().status.contains("is an edge"), "{}", h.state().status);
    // The front-left corner, then the back-left one: local +Y as +X.
    let a = corner(h.state(), [0.0, 0.0, 0.0]);
    let b = corner(h.state(), [0.0, 20.0, 0.0]);
    crate::axis_pick::take(h.state_mut(), a);
    crate::axis_pick::take(h.state_mut(), b);
    h.run();
    let d = h.state().axis_pick.clone().expect("draft");
    assert_eq!(d.by(InPlaneAxis::X), AxisBy::Points);
    assert_eq!(d.wanted().x.map(|p| p.picks.len()), Some(2), "two points kept");
    h.get_by_label("Flip +X").click();
    h.run();
    h.get_by_label("Apply axes").click();
    h.run();
    let v = held_dir(h.state(), [0.0, 1.0, 0.0]).expect("turned");
    assert!((v[0] + 1.0).abs() < 1e-9, "flipped, local +Y runs along -X: {v:?}");
}

#[test]
fn an_upright_edge_greys_apply_out_with_the_reason() {
    let mut h = harness();
    h.get_by_label("Pick +Y edge").click();
    h.run();
    let f = features(h.state());
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && p[0][0] == p[1][0] && p[0][1] == p[1][1]
        })
        .map(|c| c.id)
        .expect("an upright edge");
    crate::axis_pick::take(h.state_mut(), GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) });
    h.run();
    assert!(apply_disabled(&h), "an upright edge sets no in-plane direction");
    assert!(h.query_by_label_contains("along the work plane's normal").is_some(), "the reason is shown");
    // Arming the plane row stops the axis pick: one draft collects.
    crate::plane_pick::arm(h.state_mut(), "Setup1");
    assert!(!crate::axis_pick::collecting(h.state()));
}

/// #769: two perpendicular picks - one an upright edge that #557 alone
/// refused - define the full 3D frame. Apply is on, the words say the
/// alignment, the part lands with the picks on their axes and +Z where
/// the right-hand rule puts it (their cross), the work offset is never
/// touched, and one undo takes it back.
#[test]
fn two_picks_with_an_upright_edge_align_the_part_in_full_3d() {
    let mut h = harness();
    h.get_by_label("Pick +Y edge").click();
    h.run();
    // The upright front-left edge (x = 0, y = 0), its walked direction.
    let f = features(h.state());
    let (id, walked_z) = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            (p.len() == 2 && p[0][0] == 0.0 && p[0][1] == 0.0 && p[1][0] == 0.0 && p[1][1] == 0.0)
                .then_some((c.id, p[1][2] - p[0][2]))
        })
        .expect("the upright front edge");
    crate::axis_pick::take(h.state_mut(), GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) });
    h.run();
    // +X: the top front edge, its walked direction.
    let (edge, walked_x) = front_top_edge(h.state());
    h.get_by_label("Pick +X edge").click();
    h.run();
    crate::axis_pick::take(h.state_mut(), edge);
    h.run();
    assert!(!apply_disabled(&h), "the two picks define the frame - Apply is on");
    assert!(h.query_by_label_contains("aligns the part").is_some(), "the words say the full 3D alignment");
    h.get_by_label("Apply axes").click();
    h.run();
    assert!(h.state().status.contains("aligned from its picks"), "{}", h.state().status);
    // The picked directions land exactly; +Z lands on their cross.
    let y = held_dir(h.state(), [0.0, 0.0, walked_z]).expect("held");
    assert!((y[1] - 1.0).abs() < 1e-9, "the upright edge runs +Y: {y:?}");
    let x = held_dir(h.state(), walked_x).expect("held");
    assert!((x[0] - 1.0).abs() < 1e-9, "the top edge runs +X: {x:?}");
    // cross(walked_x, (0, 0, walked_z)) = (0, -dx*dz, 0).
    let c = [0.0, -walked_x[0] * walked_z, 0.0];
    let z = held_dir(h.state(), c).expect("held");
    assert!((z[2] - 1.0).abs() < 1e-9, "+Z lands on the picks' cross: {z:?}");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "undo restores as modelled");
}

#[test]
fn choosing_a_by_mode_arms_its_pick() {
    let mut h = harness();
    h.get_by_label("+Y by Two points").click();
    h.run();
    let d = h.state().axis_pick.clone().expect("draft");
    assert_eq!(d.picking, Some(InPlaneAxis::Y), "choosing Two points arms +Y");
    assert_eq!(h.state().geo_pick, GeoPick::Vertex);
    assert!(h.query_by_label("Stop picking +Y").is_some(), "the toggle shows armed");
    assert!(h.state().status.contains("pick two points for +Y"), "{}", h.state().status);
    // Re-choosing the current mode (Edge is the default) arms too.
    h.get_by_label("+X by Edge").click();
    h.run();
    assert_eq!(h.state().axis_pick.as_ref().and_then(|d| d.picking), Some(InPlaneAxis::X), "one axis collects");
    assert_eq!(h.state().geo_pick, GeoPick::Edge);
    assert!(h.query_by_label("Stop picking +X").is_some() && h.query_by_label("Pick +Y points").is_some());
}

#[test]
fn a_stray_feature_click_with_nothing_armed_says_what_to_do() {
    let mut h = harness();
    assert!(h.state().axis_rows_shown, "the rows say they are on screen");
    h.get_by_label("+X by Two points").click();
    h.run();
    h.get_by_label("Stop picking +X").click();
    h.run();
    assert!(!crate::axis_pick::collecting(h.state()));
    assert!(crate::axis_pick::hint_stray(h.state_mut()), "a stray click is swallowed");
    let status = h.state().status.clone();
    assert!(status.contains("click Pick +Y edge or Pick +X points first"), "{status}");
    assert!(!status.contains("open an operation"), "{status}");
    // Armed, the click feeds the axis and no hint fires.
    h.get_by_label("Pick +X points").click();
    h.run();
    assert!(!crate::axis_pick::hint_stray(h.state_mut()));
}

#[test]
fn the_picked_rows_name_points_and_edges_in_document_units() {
    let mut h = harness();
    h.get_by_label("+X by Two points").click();
    h.run();
    let a = corner(h.state(), [0.0, 0.0, 0.0]);
    crate::axis_pick::take(h.state_mut(), a);
    h.run();
    assert!(h.query_by_label_contains("+X: 1 of 2 points on 'part' (0.000, 0.000, 0.000) mm").is_some());
    let b = corner(h.state(), [0.0, 20.0, 0.0]);
    crate::axis_pick::take(h.state_mut(), b);
    h.get_by_label("+Y by Edge").click();
    h.run();
    let (edge, _) = front_top_edge(h.state());
    crate::axis_pick::take(h.state_mut(), edge);
    h.run();
    let label = |h: &Harness<'static, State>, head: &str| {
        h.query_all_by_label_contains(head).find_map(|n| n.accesskit_node().label().or_else(|| n.accesskit_node().value())).unwrap_or_default()
    };
    let pts = label(&h, "+X: 2 points");
    assert_eq!(pts, "        +X: 2 points on 'part' (0.000, 0.000, 0.000) -> (0.000, 20.000, 0.000) mm");
    let e = label(&h, "+Y: edge");
    assert!(e.contains("edge on 'part', 20.000 mm long, along ("), "{e}");
    for t in [&pts, &e] {
        assert!(!t.contains("0x") && !t.contains("vertex") && !t.contains("chain"), "no feature id: {t}");
    }
    h.state_mut().pending.push_back(CommandKind::SetUnits(camcraft_app::units::Unit::Inches));
    h.run();
    let pts = label(&h, "+X: 2 points");
    assert!(pts.ends_with("(0.00000, 0.00000, 0.00000) -> (0.00000, 0.78740, 0.00000) in"), "{pts}");
    let e = label(&h, "+Y: edge");
    assert!(e.contains("0.78740 in long"), "{e}");
}

#[test]
fn flip_after_apply_re_applies_as_its_own_undo_step() {
    let mut h = harness();
    h.get_by_label("+Y by Edge").click();
    h.run();
    let (edge, d) = front_top_edge(h.state());
    crate::axis_pick::take(h.state_mut(), edge);
    h.run();
    h.get_by_label("Apply axes").click();
    h.run();
    let along = |h: &Harness<'static, State>| held_dir(h.state(), d).map(|v| v[1]);
    assert!(along(&h).is_some_and(|y| (y - 1.0).abs() < 1e-9), "applied: the edge runs +Y");
    assert!(apply_disabled(&h));
    h.get_by_label("Flip +Y").click();
    h.run();
    assert!(!apply_disabled(&h), "Flip after Apply offers Apply again");
    assert!(h.query_by_label_contains("turns the part 180° about Z").is_some(), "the flip is a half turn");
    h.get_by_label("Apply axes").click();
    h.run();
    assert!(along(&h).is_some_and(|y| (y + 1.0).abs() < 1e-9), "flipped: the edge runs -Y");
    key(&mut h, Key::Z);
    assert!(along(&h).is_some_and(|y| (y - 1.0).abs() < 1e-9), "one undo takes back only the flip");
    key(&mut h, Key::Y);
    assert!(along(&h).is_some_and(|y| (y + 1.0).abs() < 1e-9), "redo flips it again");
    key(&mut h, Key::Z);
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "a second undo restores as modelled");
}

/// #709 hand check: after the second of two points, BOTH keep their
/// purple marker - the overlay draws every picked point (three arms each),
/// not only the first.
#[test]
fn both_picked_points_keep_their_markers() {
    let mut h = harness();
    h.get_by_label("+X by Two points").click();
    h.run();
    let a = corner(h.state(), [0.0, 0.0, 0.0]);
    let b = corner(h.state(), [0.0, 20.0, 0.0]);
    crate::axis_pick::take(h.state_mut(), a);
    h.run();
    let one = crate::interact::selection_feature_lines(h.state()).len();
    assert!(one > 0, "one point: its cross is drawn");
    crate::axis_pick::take(h.state_mut(), b);
    h.run();
    let two = crate::interact::selection_feature_lines(h.state()).len();
    assert_eq!(two, 2 * one, "two points: two crosses, the second as visible as the first");
}

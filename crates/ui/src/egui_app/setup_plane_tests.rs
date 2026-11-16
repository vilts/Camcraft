//! #330 in the surface: the Setups page's "Lay on a plane" row arms the
//! pick draft, a Face pick lands in the draft (never in an operation),
//! Apply queues `LayPartOnPlane` and the drain lands it as one undo unit;
//! picks that set no plane leave Apply disabled and push nothing. #554:
//! the row takes faces only - it arms Face mode, and an edge is refused
//! by name. The work offset is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature, quat_rotate_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::setup_align;

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

/// The front wall (local y = 0).
fn front_wall(state: &State) -> GeoSel {
    let id = features(state)
        .faces
        .iter()
        .find(|f| f.normal[1].abs() > 0.99 && f.offset.abs() < 1e-9)
        .map(|f| f.id)
        .expect("the front wall");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn top_edge(state: &State, y: f64) -> GeoSel {
    let f = features(state);
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && p.iter().all(|q| q[1] == y && q[2] == 10.0)
        })
        .map(|c| c.id)
        .expect("a top edge");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(id) }
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_plane_pick::row(ui, st, "Setup1");
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

fn wall_normal(state: &State) -> Option<[f64; 3]> {
    let a = setup_align::stored(&state.world, "Setup1")?;
    let v = quat_rotate_point(a.rot, Point3::new(0.0, -1.0, 0.0));
    Some([v.x, v.y, v.z])
}

#[test]
fn the_row_picks_a_face_and_lays_it_on_g17_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(700.0, 300.0))
        .build_ui_state(frame, with_part());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the row queues nothing");
    h.get_by_label("Pick a face").click();
    h.run();
    assert!(h.state().plane_pick.is_some(), "the draft is armed");
    assert_eq!(h.state().geo_pick, GeoPick::Face, "#554: faces only, no edge snapping");
    let wall = front_wall(h.state());
    crate::plane_pick::take(h.state_mut(), wall);
    h.run();
    assert!(h.state().op_ui.geo.is_empty(), "a draft pick is not an operation pick");
    h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Apply face")).click();
    h.run();
    let n = wall_normal(h.state()).expect("Setup1 holds the part laid");
    assert!(n[2] < -1.0 + 1e-9, "#606: the wall is laid face down on the table: {n:?}");
    assert!(h.state().plane_pick.is_none(), "a landed lay ends the pick");
    assert!(h.state().status.contains("laid on G17 (XY)"), "{}", h.state().status);
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "undo restores as modelled");
    key(&mut h, Key::Y);
    assert!(wall_normal(h.state()).is_some(), "redo lays it again");
}

/// #554: an edge that reaches the plane draft (the user switched to Edge
/// mode by hand) is refused by name and leaves the face in place; edges
/// sent to the command are refused by the app's own name too.
#[test]
fn an_edge_is_refused_by_name_and_the_face_stays() {
    let mut state = with_part();
    crate::plane_pick::arm(&mut state, "Setup1");
    let (front, back) = (top_edge(&state, 0.0), top_edge(&state, 20.0));
    let wall = front_wall(&state);
    crate::plane_pick::take(&mut state, wall.clone());
    crate::plane_pick::take(&mut state, front.clone());
    assert!(state.status.contains("an edge cannot set a plane"), "{}", state.status);
    let picks = state.plane_pick.as_ref().map(|p| p.picks.clone()).unwrap_or_default();
    assert_eq!(picks, vec![wall], "the edge is not drafted; the face stays");
    let lay = camcraft_app::pick::PlaneLay { plane: camcraft_app::cl::ArcPlane::Xy, flip: false, place: false };
    run(&mut state, CommandKind::LayPartOnPlane { setup: "Setup1".into(), picks: vec![front, back], lay });
    assert!(state.status.contains("an edge cannot set a plane"), "{}", state.status);
    assert_eq!(setup_align::stored(&state.world, "Setup1"), None, "a refusal writes nothing");
    assert!(state.plane_pick.is_some(), "a refusal keeps the draft to re-pick");
}

/// The row names the side taken as the pick's outside before Apply, and
/// a re-pick that gives the alignment the setup already holds greys Apply
/// out (the command's own check) instead of refusing after the click.
#[test]
fn the_side_is_shown_and_a_pick_that_changes_nothing_greys_apply_out() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1000.0, 300.0))
        .build_ui_state(frame, with_part());
    h.run();
    h.get_by_label("Pick a face").click();
    h.run();
    let wall = front_wall(h.state());
    crate::plane_pick::take(h.state_mut(), wall.clone());
    h.run();
    let side = |h: &Harness<'static, State>| {
        h.query_by_label_contains("outside: -Y in the model").is_some()
    };
    assert!(side(&h), "the wall's outside (-Y) is shown before Apply");
    let apply = h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Apply face"));
    assert!(!apply.accesskit_node().is_disabled(), "the wall lays the part differently");
    apply.click();
    h.run();
    assert!(wall_normal(h.state()).is_some(), "Setup1 holds the part laid");
    // The same pick again: nothing would change.
    h.get_by_label("Pick a face").click();
    h.run();
    crate::plane_pick::take(h.state_mut(), wall);
    h.run();
    let apply = h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Apply face"));
    assert!(apply.accesskit_node().is_disabled(), "the setup already holds the part that way");
    assert!(side(&h), "the side is still shown");
}

/// #606: "Face down" is on for a new pick - the picked face becomes the
/// part's bottom - and turning it off lays the face up (the old default),
/// both through the one undoable command.
#[test]
fn face_down_is_the_default_and_off_lays_the_face_up() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(700.0, 300.0))
        .build_ui_state(frame, with_part());
    h.run();
    let toggled = |h: &Harness<'static, State>| {
        h.get_by_label("Face down").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    };
    assert!(toggled(&h), "Face down is on for a new pick");
    h.get_by_label("Face down").click();
    h.run();
    assert!(!toggled(&h), "the click turns it off");
    h.get_by_label("Pick a face").click();
    h.run();
    let wall = front_wall(h.state());
    crate::plane_pick::take(h.state_mut(), wall);
    h.run();
    h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Apply face")).click();
    h.run();
    let n = wall_normal(h.state()).expect("Setup1 holds the part laid");
    assert!(n[2] > 1.0 - 1e-9, "off, the wall faces up: {n:?}");
    key(&mut h, Key::Z);
    assert_eq!(setup_align::stored(&h.state().world, "Setup1"), None, "undo restores as modelled");
}

/// #738: the row's pick status is a count and a kind - "nothing
/// selected", then "1 face selected" - and neither it, any label of the
/// row, nor a refusal on the status line carries a feature's internal id
/// ("face 0xccc7c...").
#[test]
fn the_pick_status_counts_the_face_and_never_shows_an_id() {
    let mut h = Harness::builder().with_size(Vec2::new(1200.0, 300.0)).build_ui_state(frame, with_part());
    h.run();
    h.get_by_label("Pick a face").click();
    h.run();
    assert!(h.query_by_label_contains("nothing picked yet").is_some(), "an empty draft says so");
    let wall = front_wall(h.state());
    crate::plane_pick::take(h.state_mut(), wall);
    h.run();
    assert!(h.query_by_label_contains("1 face selected").is_some(), "one face, counted");
    let edge = top_edge(h.state(), 0.0);
    crate::plane_pick::take(h.state_mut(), edge);
    h.run();
    assert!(h.state().status.contains("is an edge"), "the refusal names the kind: {}", h.state().status);
    assert!(!h.state().status.contains("0x"), "no id on the status line: {}", h.state().status);
    assert!(h.query_all_by_label_contains("0x").next().is_none(), "no id on the row");
}

//! #561 in the surface: the Fixtures page's "Locate part…" verb and the
//! Model page's "Locate in <fixture>…" arm the mate draft; a Face or
//! Vertex pick lands in the draft (never in an operation), part first,
//! then fixture; Apply queues `Locate` and the drain lands it as one
//! undo unit; picks that do not solve leave Apply disabled with the why.
//! #561 review: the mates read in plain words (a face by its outward
//! normal, a point by its coordinates), the drafted picks stay drawn,
//! a refused draft never offers Apply in its hint, the turn names its
//! mate and sense, and a fixture hit while a part face is wanted says
//! how to reach a part face hidden in the steel.
//! #2173: with a stock set, the draft holds the stock by default; the
//! part follows the located stock, and "Part faces" holds the part again.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::{ImportModel, MoveModel, TagFixture};
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::facets_centroid;
use camcraft_app::schema::Location;

use crate::state::State;

/// A 20 x 10 x 8 part well away from a 100 x 60 floor block marked as
/// the fixture "vise" (its top at Z 0).
fn with_vise() -> State {
    let mut state = State::new();
    let w = &state.world;
    state.history.execute(w, ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 10.0, 8.0))));
    let away = Location { pos: Point3::new(150.0, 80.0, 30.0), ..Location::default() };
    state.history.execute(w, MoveModel::new(w, "part", away).expect("placed"));
    state.history.execute(w, ImportModel::from_mesh("vise", cube(Point3::new(0.0, 0.0, -10.0), Point3::new(100.0, 60.0, 0.0))));
    state.history.execute(w, TagFixture::new(w, "vise", None).expect("marked"));
    state
}

/// The face of `name` whose facets centre on the LOCAL point `c`.
fn face(state: &State, name: &str, c: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(&state.world, name).expect("entity");
    let f = model::features_of(&state.world, &e).expect("features");
    let tris = model::facets_of(&state.world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .find(|f| {
            facets_centroid(&tris, f.facets.iter().map(|&i| i as usize))
                .is_some_and(|m| (0..3).all(|k| (m[k] - c[k]).abs() < 1e-6))
        })
        .map(|f| f.id)
        .expect("a face there");
    GeoSel { entity: name.into(), feature: FeatureSel::Face(id) }
}

fn loc(state: &State, name: &str) -> Location {
    let e = model::find_by_name(&state.world, name).expect("entity");
    model::transform_of(&state.world, &e).unwrap_or_default()
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
}

fn apply_enabled(h: &Harness<'static, State>) -> bool {
    !h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Apply locate")).accesskit_node().is_disabled()
}

fn fixtures_frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::pages::fixtures_section(ui, st, None);
}

fn model_frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::locate_form::model_row(ui, st, "part");
}

#[test]
fn the_fixtures_page_locates_the_part_by_a_face_mate_through_the_command() {
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 600.0)).build_ui_state(fixtures_frame, with_vise());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the page queues nothing");
    h.get_by_label("Locate part…").click();
    h.run();
    let d = h.state().locate_pick.clone().expect("the draft is armed");
    assert_eq!((d.fixture.as_str(), d.part.as_str()), ("vise", "part"), "the only part is the one located");
    assert!(!apply_enabled(&h), "no mate yet: nothing to apply");

    // A fixture pick first, and an edge, are said by name and drafted nowhere.
    let top = face(h.state(), "vise", [50.0, 30.0, 0.0]);
    crate::locate_pick::take(h.state_mut(), top.clone());
    assert!(h.state().status.contains("pick the part's face or point first"), "{}", h.state().status);
    let e = model::find_by_name(&h.state().world, "part").expect("part");
    let f = model::features_of(&h.state().world, &e).expect("features");
    let chain = feature::chains(&f).first().map(|c| c.id).expect("an edge");
    crate::locate_pick::take(h.state_mut(), GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) });
    assert!(h.state().status.contains("is not a mate"), "{}", h.state().status);

    let bottom = face(h.state(), "part", [10.0, 5.0, 0.0]);
    crate::locate_pick::take(h.state_mut(), bottom);
    crate::locate_pick::take(h.state_mut(), top);
    h.run();
    assert!(h.state().op_ui.geo.is_empty(), "a mate pick is not an operation pick");
    assert!(h.query_by_label_contains("Mate 1:").is_some(), "the mate is listed");
    assert!(apply_enabled(&h), "one face mate solves");
    h.get_by_label("Apply locate").click();
    h.run();
    let l = loc(h.state(), "part");
    assert!(l.pos.z.abs() < 1e-9, "the bottom sits on the floor at Z 0: {:?}", l.pos);
    assert!(h.state().status.contains("located part in vise"), "{}", h.state().status);
    // Hand check of #566: the mates stay, so the offset can be adjusted
    // after the part sits in the fixture - its bottom face is no longer
    // reachable to pick again.
    assert!(h.state().locate_pick.as_ref().is_some_and(|d| d.mates.len() == 1), "the mates stay after Apply");
    assert!(!apply_enabled(&h), "already there: Apply waits for an edit");
    assert!(h.query_by_label_contains("Located. Edit Offset or Turn").is_some(), "the located line says what to do");
    if let Some(d) = h.state_mut().locate_pick.as_mut() {
        d.offset.dz = 1.0;
    }
    h.run();
    assert!(apply_enabled(&h), "an Offset edit re-enables Apply");
    h.get_by_label("Apply locate").click();
    h.run();
    assert!((loc(h.state(), "part").pos.z - 1.0).abs() < 1e-9, "lifted 1 mm off the floor: {:?}", loc(h.state(), "part").pos);
    key(&mut h, Key::Z);
    assert!(loc(h.state(), "part").pos.z.abs() < 1e-9, "one undo step takes the offset back");
    key(&mut h, Key::Z);
    assert!((loc(h.state(), "part").pos.z - 30.0).abs() < 1e-9, "undo puts the part back");
    key(&mut h, Key::Y);
    assert!(loc(h.state(), "part").pos.z.abs() < 1e-9, "redo locates it again");
}

#[test]
fn the_model_page_brings_the_fixture_to_the_part() {
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 600.0)).build_ui_state(model_frame, with_vise());
    h.run();
    h.get_by_label("Locate in vise…").click();
    h.run();
    assert!(h.state().locate_pick.as_ref().is_some_and(|d| d.part == "part"), "armed for this part");
    let bottom = face(h.state(), "part", [10.0, 5.0, 0.0]);
    let top = face(h.state(), "vise", [50.0, 30.0, 0.0]);
    crate::locate_pick::take(h.state_mut(), bottom);
    crate::locate_pick::take(h.state_mut(), top);
    h.run();
    h.get_by_label("Move the fixture").click();
    h.run();
    let part_before = loc(h.state(), "part");
    h.get_by_label("Apply locate").click();
    h.run();
    assert_eq!(loc(h.state(), "part"), part_before, "the part keeps its place");
    assert!((loc(h.state(), "vise").pos.z - 30.0).abs() < 1e-9, "the floor top rises to the part's bottom at Z 30");
    assert!(h.state().status.contains("located vise on part"), "{}", h.state().status);
    key(&mut h, Key::Z);
    assert_eq!(loc(h.state(), "vise"), Location::default(), "undo puts the vise back");
}

fn vertex(state: &State, name: &str, v: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(&state.world, name).expect("entity");
    let f = model::features_of(&state.world, &e).expect("features");
    let vi = feature::corner_verts(&f).into_iter().find(|&i| f.verts[i as usize] == v).expect("a corner there");
    GeoSel { entity: name.into(), feature: FeatureSel::Vertex(feature::vertex_id(&f, vi).expect("vertex id")) }
}

#[test]
fn the_mates_read_in_plain_words_and_stay_drawn() {
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 600.0)).build_ui_state(fixtures_frame, with_vise());
    h.run();
    h.get_by_label("Locate part…").click();
    h.run();
    // No mate yet: the why stands on the page, and the hint offers no Apply.
    assert!(h.query_by_label_contains("Not yet: pick at least one mate").is_some(), "the zero-mate reason is visible");
    // A fixture hit while a part face is wanted: how to reach one hidden in the steel.
    let top = face(h.state(), "vise", [50.0, 30.0, 0.0]);
    crate::locate_pick::take(h.state_mut(), top.clone());
    assert!(h.state().status.contains("hide the fixture (eye) to pick part faces hidden in it"), "{}", h.state().status);
    { let f = face(h.state(), "part", [10.0, 5.0, 0.0]); crate::locate_pick::take(h.state_mut(), f); }
    crate::locate_pick::take(h.state_mut(), top);
    h.run();
    let mate = h.query_by_label_contains("Mate 1:").and_then(|n| n.accesskit_node().value()).expect("the mate is listed");
    assert!(
        mate.contains("Face ") && mate.contains(" of 'part' (-Z (bottom) face) on Face ") && mate.contains(" of 'vise' (+Z (top) face)"),
        "{mate}"
    );
    assert!(!mate.contains("0x"), "no raw ids: {mate}");
    assert!(
        h.query_by_label_contains("Turn about mate 1 normal (+ = counter-clockwise looking down the normal)").is_some(),
        "the turn names its mate and its sense"
    );
    assert!(!crate::interact::selection_feature_lines(h.state()).is_empty(), "the drafted faces stay drawn");
    // A point reads as its friendly name plus coordinates in the setup frame.
    let corner_sel = vertex(h.state(), "part", [0.0, 0.0, 0.0]);
    let corner_name = camcraft_app::pick::geo_label(&h.state().world, &corner_sel);
    let corner = crate::locate_pick::describe(h.state(), "vise", &corner_sel);
    assert_eq!(corner, format!("{corner_name} (150.00, 80.00, 30.00) mm"));
}

#[test]
fn a_refused_draft_says_why_instead_of_offering_apply() {
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 600.0)).build_ui_state(fixtures_frame, with_vise());
    h.run();
    // Locate once; the draft stays with its mate, and the part already
    // sits there: Apply waits, and the hint says what an edit would do.
    h.get_by_label("Locate part…").click();
    h.run();
    { let f = face(h.state(), "part", [10.0, 5.0, 0.0]); crate::locate_pick::take(h.state_mut(), f); }
    { let f = face(h.state(), "vise", [50.0, 30.0, 0.0]); crate::locate_pick::take(h.state_mut(), f); }
    h.run();
    h.get_by_label("Apply locate").click();
    h.run();
    assert!(h.state().locate_pick.is_some(), "the draft is still open");
    assert!(!apply_enabled(&h), "the part already sits there");
    let next = h.query_by_label_contains("Next:").and_then(|n| n.accesskit_node().value()).expect("the hint");
    assert!(!next.contains("Apply,") && next.contains("edit Offset or Turn"), "{next}");
    assert!(!h.state().status.contains("Apply,"), "the status hint agrees: {}", h.state().status);
    assert!(h.query_by_label_contains("Not yet:").is_none(), "a located draft is not a refusal");
}

thread_local! {
    static SHOW_FORM: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// The shell's law in miniature: reset, draw the page (or another one),
/// then sweep.
fn swept_frame(ui: &mut egui::Ui, st: &mut State) {
    st.locate_form_drawn = false;
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    if SHOW_FORM.with(std::cell::Cell::get) {
        super::pages::fixtures_section(ui, st, None);
    }
    crate::locate_pick::sweep(st);
}

/// Review of #566: a located draft keeps its mates for Offset edits, but
/// leaving the page drops it - it must never capture picks with no form
/// on screen (a face pick for an operation afterwards reaches the
/// operation, not "neither the part nor the fixture").
#[test]
fn leaving_the_page_drops_a_located_draft() {
    SHOW_FORM.with(|c| c.set(true));
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 600.0)).build_ui_state(swept_frame, with_vise());
    h.run();
    h.get_by_label("Locate part…").click();
    h.run();
    { let f = face(h.state(), "part", [10.0, 5.0, 0.0]); crate::locate_pick::take(h.state_mut(), f); }
    { let f = face(h.state(), "vise", [50.0, 30.0, 0.0]); crate::locate_pick::take(h.state_mut(), f); }
    h.run();
    h.get_by_label("Apply locate").click();
    h.run();
    assert!(h.state().locate_pick.is_some(), "on the page, the located draft stays for Offset edits");
    SHOW_FORM.with(|c| c.set(false));
    h.run();
    assert!(h.state().locate_pick.is_none(), "off the page, the draft is dropped");
    SHOW_FORM.with(|c| c.set(true));
}

/// #582: a Model-page move that sinks the part into the marked floor
/// lands (a warning, not a refusal) and says so by name on the status
/// line; a clear move says nothing more than "moved".
#[test]
fn a_model_page_move_into_the_fixture_warns_on_the_status_line() {
    use crate::state::CommandKind;
    let mut st = with_vise();
    let into = |z: f64| CommandKind::MoveModel { name: "part".into(), pos: [10.0, 10.0, z], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 };
    st.pending.push_back(into(-3.0));
    st.drain_actions();
    assert!((loc(&st, "part").pos.z + 3.0).abs() < 1e-9, "the move lands: a warning is not a refusal");
    assert!(st.status.starts_with("moved part - warning: 'part' would sit inside the fixture 'vise'"), "{}", st.status);
    st.pending.push_back(into(0.0));
    st.drain_actions();
    assert_eq!(st.status, "moved part", "resting on the floor is not inside it");
}

/// `with_vise` plus a box stock round the part: 3 mm under it, 1 mm on
/// every other side.
fn with_vise_and_stock() -> State {
    use camcraft_app::commands::SetStock;
    let mut state = with_vise();
    let w = &state.world;
    let b = model::find_by_name(w, "part").and_then(|e| model::entity_world_aabb(w, &e)).expect("part bounds");
    let (shape, pos) = camcraft_app::stock::fit_box_sides(&b, [1.0, 1.0, 1.0, 1.0, 3.0, 1.0]).expect("fits");
    let set = SetStock::new(&state.world, shape, Location { pos, ..Location::default() }).expect("stock");
    state.history.execute(&state.world, set);
    state
}

/// The stock face facing `n`, read where the locate reads it.
fn stock_face(state: &State, n: [f64; 3]) -> GeoSel {
    let name = camcraft_app::fixture_locate::stock_of(&state.world, "vise").expect("a stock");
    let e = model::find_by_name(&state.world, &name).expect("stock entity");
    let f = model::features_of(&state.world, &e).expect("stock features");
    f.faces
        .iter()
        .map(|f| GeoSel { entity: name.clone(), feature: FeatureSel::Face(f.id) })
        .find(|g| {
            matches!(camcraft_app::fixture_locate::read_pick(&state.world, "vise", g),
                Ok(camcraft_app::pick::MateDatum::Plane { normal, .. }) if (0..3).all(|k| (normal[k] - n[k]).abs() < 1e-9))
        })
        .expect("a stock face there")
}

/// #2173: with a stock set, the locate holds the stock by default - a
/// part face is refused by name, the stock's bottom goes on the floor,
/// the part follows inside it, and "Part faces" goes back to the part.
#[test]
fn the_fixtures_page_locates_the_stock_on_the_vise() {
    use camcraft_app::fixture_locate::LocateBody;
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 700.0)).build_ui_state(fixtures_frame, with_vise_and_stock());
    h.run();
    h.get_by_label("Locate part…").click();
    h.run();
    assert_eq!(h.state().locate_pick.as_ref().map(|d| d.body), Some(LocateBody::Stock), "a vise holds the stock");
    assert!(h.query_by_label_contains("Locate the stock of part in vise").is_some(), "the title names the stock");
    assert!(h.query_by_label("Stock faces").is_some() && h.query_by_label("Part faces").is_some(), "the body choice is drawn");
    let part_bottom = face(h.state(), "part", [10.0, 5.0, 0.0]);
    crate::locate_pick::take(h.state_mut(), part_bottom);
    assert!(h.state().status.contains("is neither the stock"), "{}", h.state().status);
    let top = face(h.state(), "vise", [50.0, 30.0, 0.0]);
    let bottom = stock_face(h.state(), [0.0, 0.0, -1.0]);
    crate::locate_pick::take(h.state_mut(), bottom);
    crate::locate_pick::take(h.state_mut(), top);
    h.run();
    assert!(apply_enabled(&h), "the stock floor mate solves");
    h.get_by_label("Apply locate").click();
    h.run();
    let stock = camcraft_app::fixture_locate::stock_of(&h.state().world, "vise").expect("a stock");
    let sb = model::find_by_name(&h.state().world, &stock)
        .and_then(|e| model::entity_world_aabb(&h.state().world, &e))
        .expect("stock bounds");
    assert!(sb.min.z.abs() < 1e-9, "the stock bottom sits on the floor: {:?}", sb.min);
    assert!((loc(h.state(), "part").pos.z - 3.0).abs() < 1e-9, "the part keeps 3 mm of stock under it");
    key(&mut h, Key::Z);
    assert!((loc(h.state(), "part").pos.z - 30.0).abs() < 1e-9, "one undo puts both back");
    h.get_by_label("Part faces").click();
    h.run();
    let d = h.state().locate_pick.clone().expect("still armed");
    assert_eq!(d.body, LocateBody::Part, "the part is held again");
    assert!(d.mates.is_empty(), "the stock mates are dropped with the body change");
}

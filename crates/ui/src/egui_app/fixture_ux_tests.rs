//! #339 slice 4 in the shell, structurally: the browser's fixture menu
//! rows (Use / Stop using as fixture on a model row, Import fixture… on
//! a setup's Fixtures node) queue the Fixtures page's own commands and
//! undo through the history; each Fixtures child row says how the gate
//! judges it (`mesh` / `box only: <reason>`); and a selected op whose
//! warnings strike a fixture raises the viewport tint flag and names
//! the fixture in its banner. No pixels: the snapshot lane is release.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, ImportModel, MoveFixture, SetStock, TagFixture,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::state::{CommandKind, SetupPage, State, TreeMenuTarget};

use super::browser_tree::{Leaf, browser_rows};
use super::fixture_menu::menu_rows;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed, outward box.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

fn import(state: &mut State, name: &str, tris: Vec<Triangle>) {
    state.history.execute(&state.world, ImportModel::from_mesh(name, Mesh::from_facets(tris)));
    state.doc_version += 1;
}

fn mark(state: &mut State, name: &str) {
    state.history.execute(&state.world, TagFixture::new(&state.world, name, None).expect("mark"));
    state.doc_version += 1;
}

fn is_fixture(state: &State, name: &str) -> bool {
    model::find_by_name(&state.world, name).is_some_and(|e| fixture::def_of(&state.world, &e).is_some())
}

fn labels(rows: &[(String, Option<CommandKind>)]) -> Vec<&str> {
    rows.iter().map(|(l, _)| l.as_str()).collect()
}

#[test]
fn a_model_row_menu_marks_and_unmarks_through_undoable_commands() {
    let mut state = State::new();
    import(&mut state, "Vise", cube(p(0.0, 0.0, 0.0), p(5.0, 5.0, 5.0)));
    let target = TreeMenuTarget::Model { name: "Vise".into(), row: 0 };
    let rows = menu_rows(&state, &target);
    assert_eq!(labels(&rows), ["Fixture", "Use as fixture", "Delete"]);
    assert!(rows[0].1.is_none(), "the header is no verb");
    let Some((_, Some(cmd))) = rows.into_iter().nth(1) else { panic!("a verb") };
    assert!(matches!(&cmd, CommandKind::TagFixture { name, setup: None } if name == "Vise"), "{cmd:?}");
    state.pending.push_back(cmd);
    state.drain_actions();
    assert!(is_fixture(&state, "Vise"));

    let rows = menu_rows(&state, &target);
    assert_eq!(labels(&rows), ["Fixture", "Stop using as fixture", "Delete"]);
    let Some((_, Some(cmd))) = rows.into_iter().nth(1) else { panic!("a verb") };
    assert!(matches!(&cmd, CommandKind::UntagFixture(n) if n == "Vise"), "{cmd:?}");
    state.pending.push_back(cmd);
    state.drain_actions();
    assert!(!is_fixture(&state, "Vise"));
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(is_fixture(&state, "Vise"), "undo re-marks");
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(!is_fixture(&state, "Vise"), "undo walks the mark back too");
    assert!(model::find_by_name(&state.world, "Vise").is_some(), "the solid stays");
}

#[test]
fn use_as_fixture_binds_to_the_active_setup_and_fixtures_offers_import() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    state.drain_actions();
    import(&mut state, "Vise", cube(p(0.0, 0.0, 0.0), p(5.0, 5.0, 5.0)));
    let rows = menu_rows(&state, &TreeMenuTarget::Model { name: "Vise".into(), row: 0 });
    assert_eq!(labels(&rows), ["Fixture", "Use as fixture (Setup1)", "Delete"]);
    assert!(matches!(&rows[1].1, Some(CommandKind::TagFixture { setup: Some(s), .. }) if s == "Setup1"));

    let rows = menu_rows(&state, &TreeMenuTarget::Fixtures("Setup1".into()));
    assert_eq!(
        labels(&rows),
        ["Fixtures", "Import fixture…", "Save fixture to library…", "Insert from library…", "Add built-in vise…", "Add accessory…", "Add fixture plate…"]
    );
    assert!(matches!(rows[1].1, Some(CommandKind::ImportFixtureDialog)));
    // The Fixtures node the menu anchors on stands in the tree.
    let (_, leaves) = browser_rows(&state);
    assert!(leaves.contains(&Leaf::Page("Setup1".into(), SetupPage::Fixtures)));
    // A row whose model is gone offers nothing (the menu closes).
    assert!(menu_rows(&state, &TreeMenuTarget::Model { name: "Gone".into(), row: 0 }).is_empty());
}

#[test]
fn each_fixture_row_says_mesh_or_box_only_and_why() {
    let mut state = State::new();
    import(&mut state, "Closed", cube(p(0.0, 0.0, 0.0), p(10.0, 5.0, 20.0)));
    let mut open = cube(p(20.0, 0.0, 0.0), p(30.0, 5.0, 20.0));
    open.truncate(open.len() - 1);
    import(&mut state, "Open", open);
    mark(&mut state, "Closed");
    mark(&mut state, "Open");
    let (rows, leaves) = browser_rows(&state);
    let suffix = |name: &str| {
        // The Fixtures child (depth 2 under the implicit setup), not the
        // Models row of the same name.
        rows.iter()
            .zip(&leaves)
            .find(|(r, l)| **l == Leaf::Model(name.into()) && r.depth == 2)
            .map(|(r, _)| r.suffix.clone())
    };
    assert_eq!(suffix("Closed").as_deref(), Some("mesh"));
    assert_eq!(suffix("Open").as_deref(), Some("box only: open mesh"));
    state.hidden_models.push("Open".into());
    let (rows, leaves) = browser_rows(&state);
    let hidden = rows.iter().zip(&leaves).find(|(r, l)| **l == Leaf::Model("Open".into()) && r.depth == 2);
    assert_eq!(hidden.map(|(r, _)| r.suffix.as_str()), Some("box only: open mesh (hidden)"));
}

/// A face pass with one jaw reaching 1 mm into its low-y edge.
fn struck_world() -> State {
    let mut state = State::new();
    let w = &state.world;
    state.history.execute(
        w,
        SetStock::new(w, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    state.history.execute(w, AddTool::new(w, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    state.history.execute(w, CreateOperation::new(w, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    state.history.execute(w, GenerateOperation::new(w, "Face1").expect("generate"));
    let op = model::find_by_name(w, "Face1").expect("Face1");
    let path = model::toolpath_of(w, &op).expect("path").path;
    let (x0, y0, x1) = path.moves.iter().flat_map(|m| [m.from, m.to]).fold(
        (f64::MAX, f64::MAX, f64::MIN),
        |(a, b, c), q| (a.min(q.x), b.min(q.y), c.max(q.x)),
    );
    import(&mut state, "JawLo", cube(p(x0 - 10.0, y0 - 25.0, -5.0), p(x1 + 10.0, y0 - 1.0, 21.0)));
    mark(&mut state, "JawLo");
    state.history.execute(&state.world, GenerateOperation::new(&state.world, "Face1").expect("regenerate"));
    state.doc_version += 1;
    state
}

#[test]
fn a_selected_op_tints_the_fixture_it_strikes_and_names_it() {
    let mut state = struck_world();
    assert!(state.fixture_highlight().is_empty(), "nothing selected, nothing tinted");
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    assert_eq!(state.fixture_highlight(), vec!["JawLo".to_owned()]);
    let tinted = super::world::collect(&state)
        .parts
        .iter()
        .filter(|g| !g.stock && !g.fill && !g.classes.is_empty())
        .filter(|g| g.classes.iter().all(|c| *c == crate::viewport::STRUCK_CLASS))
        .count();
    assert_eq!(tinted, 1, "exactly the struck jaw wears the tint");
    let (_, _, details) = super::op_page::status_text(&state, "Face1");
    assert!(details.iter().any(|d| d.starts_with("Strikes fixture: JawLo")), "{details:?}");
    // Display only: the tint queued nothing and changed no document.
    assert!(state.pending.is_empty());
}

#[test]
fn an_unresolvable_hit_is_said_in_the_banner_not_guessed() {
    let mut state = struck_world();
    // The jaw moves away without a regeneration: the cached warning
    // stands, but no fixture reproduces it now.
    let away = Location { pos: p(0.0, -500.0, 0.0), ..Location::default() };
    state.history.execute(&state.world, MoveFixture::new(&state.world, "JawLo", away).expect("move"));
    state.doc_version += 1;
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    assert!(state.fixture_highlight().is_empty(), "no tint on a guess");
    let (_, _, details) = super::op_page::status_text(&state, "Face1");
    assert!(details.iter().any(|d| d.starts_with("Fixture not resolved for move")), "{details:?}");
}

#[test]
fn the_fixtures_page_shows_the_refusal_text_without_logs() {
    use egui_kittest::kittest::Queryable;
    let mut state = State::new();
    let mut open = cube(p(0.0, 0.0, 0.0), p(10.0, 5.0, 20.0));
    open.truncate(open.len() - 1);
    import(&mut state, "OpenVise", open);
    mark(&mut state, "OpenVise");
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(700.0, 400.0))
        .build_ui_state(|ui, st: &mut State| super::pages::fixtures_section(ui, st, None), state);
    harness.run();
    let _ = harness.get_by_label_contains("Collision: judged by its bounding box: the fixture mesh is not closed");
}

//! #509: the Fixtures page fits a default-width properties pane - each
//! fixture row's name, position, Place / Unmark / Delete, the Fixed box
//! (its caption whole, wrapped not truncated) and the place fields stay
//! inside the pane, and the place fields say which axis they move.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::pages (fixtures_section), ui::state,
//! ui::egui_app::locate_form (#561).

use egui_kittest::kittest::NodeT;

use super::*;
use crate::state::SetupPage;

const FIXED: &str = "Fixed (does not turn with the part)";
const PLACE_FIELDS: [&str; 6] =
    ["Place X", "Place Y", "Place Z", "Tilt about X (roll)", "Tilt about Y (pitch)", "Turn about Z (yaw)"];
const TURN_BUTTONS: [&str; 6] = ["Turn X+", "Turn X-", "Turn Y+", "Turn Y-", "Turn Z+", "Turn Z-"];

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl")
}

/// A blank window with one setup and a cube marked as its fixture, the
/// Fixtures page open in the properties pane at the default dock split.
fn fixtures_page() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        st.pending.push_back(CommandKind::ImportFile(cube_path()));
        st.settle();
        st.pending.push_back(CommandKind::TagFixture { name: "cube".into(), setup: Some("Setup1".into()) });
        st.drain_actions();
        crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
    }
    h.run_steps(3);
    h
}

fn pane(h: &Harness<'static, ShellUi>) -> egui::Rect {
    let dock = &h.state().dock;
    let path = dock.find_tab(&Tab::Properties).expect("properties open");
    dock[path.node_path()].rect().expect("properties laid out")
}

/// Every node labelled `label` that the properties pane drew (the
/// browser and viewport sit left of it) lies wholly inside the pane.
fn assert_inside(h: &Harness<'static, ShellUi>, pane: egui::Rect, label: &str) {
    let nodes: Vec<egui::Rect> = h
        .query_all_by_label_contains(label)
        .map(|n| n.rect())
        .filter(|r| r.min.x >= pane.min.x - 1.0)
        .collect();
    assert!(!nodes.is_empty(), "the pane draws {label:?}");
    for r in nodes {
        assert!(
            r.min.x >= pane.min.x - 1.0 && r.max.x <= pane.max.x + 1.0,
            "{label:?} at {r:?} spills out of the pane {pane:?}"
        );
    }
}

#[test]
fn fixture_rows_fit_the_default_pane_and_label_the_place_fields() {
    let mut h = fixtures_page();
    let pane = pane(&h);
    assert!(h.query_by_label("Fixtures - Setup1").is_some(), "the page is open");
    // The place fields are one click away on every row, not hidden
    // behind clicking the name.
    h.get_by_label("Place…").click();
    h.run_steps(2);
    assert_eq!(h.state().state.fixture_ui.name, "cube", "Place… opens the draft");
    for label in ["cube", "yaw", "Place…", "Locate part…", "Unmark", "Delete", FIXED, "Apply"] {
        assert_inside(&h, pane, label);
    }
    for label in PLACE_FIELDS {
        assert!(h.query_by_label(label).is_some(), "the place row names {label:?}");
        assert_inside(&h, pane, label);
    }
    // The Fixed caption is whole: as wide as its unwrapped text, or
    // wrapped onto a second line - never cut short with an ellipsis.
    let r = h
        .query_all_by_label(FIXED)
        .map(|n| n.rect())
        .find(|r| r.min.x >= pane.min.x - 1.0)
        .expect("the Fixed caption");
    let (full, row_h) = h.ctx.fonts_mut(|f| {
        let g = f.layout_no_wrap(FIXED.into(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
        (g.size().x, g.size().y)
    });
    assert!(
        r.width() >= full * 0.9 || r.height() > row_h * 1.5,
        "the Fixed caption is truncated: {r:?} vs text width {full}"
    );
    // Opening the draft is view state: nothing reached the queue.
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
}

/// #561: the locate tools sit on the Fixtures page row and on the Model
/// page, inside the default pane, and arming one is view state only.
#[test]
fn the_locate_tools_sit_on_the_fixtures_page_and_the_model_page() {
    let mut h = fixtures_page();
    let rect = pane(&h);
    assert_inside(&h, rect, "Locate part…");
    h.get_by_label("Locate part…").click();
    h.run_steps(2);
    assert!(h.state().state.locate_pick.as_ref().is_some_and(|d| d.fixture == "cube"), "the row arms the mates");
    for label in ["Move the part", "Move the fixture", "Offset X", "Offset Y", "Offset Z", "Turn (deg)", "Apply locate", "Remove last"] {
        assert_inside(&h, rect, label);
    }
    assert!(h.get_by_label("Apply locate").accesskit_node().is_disabled(), "no mate yet");
    // #561 review: with no mate the reason stands on the page, not only
    // in the disabled button's tooltip, and the offset names its frame.
    assert!(h.query_by_label_contains("Not yet: ").is_some(), "the why is visible");
    assert!(h.query_by_label_contains("Offset (setup X / Y / Z)").is_some(), "the offset's frame is named");
    assert!(h.query_by_label_contains("Turn about setup +Z (+ = counter-clockwise").is_some(), "the turn's axis and sense");
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
    // The Model page of the fixture offers the same verb.
    crate::actions::target(&mut h.state_mut().state, UiTarget::Model("cube".into()));
    h.run_steps(3);
    assert!(h.query_by_label_contains("Locate in a fixture").is_some(), "the model page's locate row");
    let rect = pane(&h);
    assert_inside(&h, rect, "Stop locating");
}

/// #788: the page's "Add built-in vise…" and a placed vise's opening row
/// (Jaw opening, Set opening, Open to stock) sit inside the default pane.
#[test]
fn a_built_in_vise_row_fits_the_default_pane() {
    let mut h = fixtures_page();
    assert_inside(&h, pane(&h), "Add built-in vise…");
    {
        let st = &mut h.state_mut().state;
        let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
        st.pending.push_back(CommandKind::InsertBuiltinVise {
            setup: Some("Setup1".into()),
            params: six,
            fit_stock: false,
            pos: [0.0, 200.0, 0.0],
            yaw_deg: 0.0,
        });
        st.drain_actions();
    }
    h.run_steps(3);
    let rect = pane(&h);
    for label in ["Vise 6 in", "Built-in vise 6 in", "Jaw opening", "Set opening", "Open to stock"] {
        assert_inside(&h, rect, label);
    }
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
}

/// #819: the page's "Add fixture plate…" and a plate's row (its grid
/// line, the place picker on the other fixture) sit inside the default pane.
#[test]
fn a_fixture_plate_row_fits_the_default_pane() {
    let mut h = fixtures_page();
    assert_inside(&h, pane(&h), "Add fixture plate…");
    {
        let st = &mut h.state_mut().state;
        let p = camcraft_app::plate::preset("12 x 12 in").map(camcraft_app::plate::PlatePreset::params).expect("preset");
        st.pending.push_back(CommandKind::InsertFixturePlate {
            setup: Some("Setup1".into()),
            params: p,
            on_table: false,
            pos: [0.0, 400.0, 0.0],
            yaw_deg: 0.0,
        });
        st.drain_actions();
    }
    h.run_steps(3);
    let rect = pane(&h);
    for label in ["Plate 12 x 12 in", "Fixture plate: rows A-L, columns 1-12", "Place on plate…", "Detect plate grid"] {
        assert_inside(&h, rect, label);
    }
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
}

/// #789: the page's "Add accessory…" and a placed strap clamp's sizes
/// row (its figures and Apply sizes) sit inside the default pane.
#[test]
fn a_built_in_accessory_row_fits_the_default_pane() {
    let mut h = fixtures_page();
    assert_inside(&h, pane(&h), "Add accessory…");
    {
        let st = &mut h.state_mut().state;
        let clamp = camcraft_app::accessory::Accessory::default_of("strap_clamp").expect("clamp");
        let at = camcraft_app::schema::Location { pos: camcraft_app::geometry::Point3::new(0.0, 200.0, 0.0), ..Default::default() };
        st.pending.push_back(CommandKind::InsertAccessory {
            setup: Some("Setup1".into()),
            params: clamp,
            placement: camcraft_app::commands::AccessoryPlacement::At(at),
        });
        st.drain_actions();
    }
    h.run_steps(3);
    let rect = pane(&h);
    for label in ["Strap clamp", "Built-in strap clamp", "Stud above the strap", "Clamp height", "Apply sizes"] {
        assert_inside(&h, rect, label);
    }
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
}

/// #2174: the place draft offers the Model page's three angles and six
/// quarter-turn buttons; each is an enabled button inside the pane, and a
/// click queues one `TurnFixture` that lands an exact quarter turn.
#[test]
fn the_place_draft_turns_a_fixture_by_a_quarter_about_each_axis() {
    let mut h = fixtures_page();
    h.get_by_label("Place…").click();
    h.run_steps(2);
    let rect = pane(&h);
    assert_inside(&h, rect, "Quarter-turns about world axes");
    for label in TURN_BUTTONS {
        let node = h.get_by_label(label);
        assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Button, "{label} is a button");
        assert!(!node.accesskit_node().is_disabled(), "{label} is enabled");
        assert_inside(&h, rect, label);
    }
    h.get_by_label("Turn Z+").click();
    h.run_steps(1);
    assert!(
        matches!(h.state().state.pending.front(), Some(CommandKind::TurnFixture { name, axis: 2, degrees }) if name == "cube" && *degrees == 90.0),
        "{:?}",
        h.state().state.pending
    );
    h.state_mut().state.drain_actions();
    h.run_steps(2);
    let st = &h.state().state;
    let loc = camcraft_app::model::find_by_name(&st.world, "cube")
        .and_then(|e| camcraft_app::model::transform_of(&st.world, &e))
        .expect("the cube stands");
    let q = std::f64::consts::FRAC_1_SQRT_2;
    assert_eq!(loc.rot, [0.0, 0.0, q, q], "an exact quarter turn about Z");
    assert!((st.fixture_ui.yaw_deg - 90.0).abs() < 1e-9, "the draft re-reads the turn: {}", st.fixture_ui.yaw_deg);
    assert!(st.status.contains("turned fixture cube 90 degrees about Z"), "{}", st.status);
}

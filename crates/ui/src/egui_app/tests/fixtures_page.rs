//! #1440: the Fixtures page follows the WCS page's layout - titled
//! groups, the adds as equal buttons, one to a row - and keeps what it
//! showed before: the empty message, each fixture's row and verbs, the
//! machine table line, the conditional Mark button, and every input's
//! accessible name (#1119 / #1432). Provenance: our own page law.

use super::*;
use crate::state::SetupPage;

const FIXED: &str = "Fixed (does not turn with the part)";
const EMPTY: &str = "No workholding modeled: the collision checks see stock and part only.";
const FULL: &str = "Workholding in this setup's collision checks:";

fn blank() -> Harness<'static, ShellUi> {
    Harness::builder().with_size(Vec2::new(1280.0, 900.0)).build_ui_state(frame, ShellUi::blank())
}

/// A blank window with one setup, its Fixtures page not yet open.
fn blank_with_setup() -> Harness<'static, ShellUi> {
    let mut h = blank();
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    st.drain_actions();
    h
}

fn open(h: &mut Harness<'static, ShellUi>) {
    let st = &mut h.state_mut().state;
    crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
    h.run_steps(3);
}

/// One setup, a cube marked as its fixture, a built-in vise and a plate
/// with a dowel grid.
fn furnished() -> Harness<'static, ShellUi> {
    let mut h = blank_with_setup();
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::ImportFile(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl"),
        ));
        st.settle();
        st.pending.push_back(CommandKind::TagFixture { name: "cube".into(), setup: Some("Setup1".into()) });
        let mut p = camcraft_app::plate::preset("12 x 12 in").map(camcraft_app::plate::PlatePreset::params).expect("preset");
        // A dowel grid, so the place form can offer its dowel pair.
        p.dowels = Some(camcraft_app::plate::Dowels {
            pitch_x: 50.0,
            pitch_y: 50.0,
            offset_x: 25.0,
            offset_y: 25.0,
            diameter: 10.0,
        });
        st.pending.push_back(CommandKind::InsertFixturePlate {
            setup: Some("Setup1".into()),
            params: p,
            on_table: false,
            pos: [0.0; 3],
            yaw_deg: 0.0,
        });
        let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
        st.pending.push_back(CommandKind::InsertBuiltinVise {
            setup: Some("Setup1".into()),
            params: six,
            fit_stock: false,
            pos: [0.0, 0.0, 300.0],
            yaw_deg: 0.0,
        });
        st.drain_actions();
    }
    open(&mut h);
    h
}

fn rect(h: &Harness<'static, ShellUi>, l: &str) -> egui::Rect {
    h.get_by_label(l).rect()
}

const ADDS: [&str; 4] = ["Import fixture…", "Add built-in vise…", "Add accessory…", "Add fixture plate…"];

/// The four adds are equal, one to a row in order, on one left edge,
/// all under the Add workholding title.
fn assert_adds(h: &Harness<'static, ShellUi>, title_top: f32) {
    let r: Vec<egui::Rect> = ADDS.iter().map(|l| rect(h, l)).collect();
    assert!(r.iter().all(|b| b.top() > title_top), "adds under their title: {r:?}");
    assert!(r.windows(2).all(|w| w[1].top() >= w[0].bottom()), "one row each, in order: {r:?}");
    assert!(r.iter().all(|b| b.left() == r[0].left()), "one left edge: {r:?}");
    assert!(r.iter().all(|b| (b.width() - r[0].width()).abs() < 3.0), "equal buttons: {r:?}");
}

#[test]
fn the_fixtures_page_is_laid_out_in_groups_like_the_wcs_page() {
    let h = furnished();
    let (hold, add) = (rect(&h, "Workholding"), rect(&h, "Add workholding"));
    assert!(hold.top() < add.top(), "groups stack: {hold:?} {add:?}");
    // The group's quiet line, then the rows, all above the Add group.
    let line = rect(&h, FULL);
    assert!(line.top() > hold.top() && line.top() < add.top());
    assert!(h.query_by_label(EMPTY).is_none(), "the empty line is not shown with fixtures");
    // Each fixture's row and verbs stay in the first group.
    for l in ["cube", "Place…", "Locate part…", "Unmark", "Delete", "Place on plate…"] {
        let n: Vec<egui::Rect> = h.query_all_by_label(l).map(|n| n.rect()).filter(|r| r.min.x > 300.0).collect();
        assert!(!n.is_empty(), "the page draws {l:?}");
        assert!(n.iter().all(|r| r.top() > hold.top() && r.bottom() < add.top()), "{l:?} in Workholding: {n:?}");
    }
    assert!(h.query_all_by_label(FIXED).count() >= 1, "the Fixed box stays");
    assert!(h.query_by_label_contains("Built-in vise 6 in").is_some(), "the vise row stays");
    assert_adds(&h, add.top());
}

/// A machine that states its travel gives the page its table line, in the
/// first group, after the fixtures' rows (it is derived, never an entity).
#[test]
fn the_fixtures_page_keeps_the_machine_table_line() {
    let mut h = furnished();
    let mch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tormach-770mx.mch");
    h.state_mut().state.pending.push_back(CommandKind::LoadMachineFile(mch));
    h.run_steps(3);
    let (hold, add) = (rect(&h, "Workholding"), rect(&h, "Add workholding"));
    let t = h.query_by_label_contains("Machine/Table  span").expect("the table line").rect();
    assert!(t.top() > hold.top() && t.bottom() < add.top(), "the table line sits in Workholding: {t:?}");
    let last_row = h.query_all_by_label("Place on plate…").map(|n| n.rect().bottom()).fold(0.0, f32::max);
    assert!(t.top() > last_row, "after the fixtures' rows");
    assert!(h.query_by_label_contains("(from the machine file)").is_some(), "says where it comes from");
}

/// With no fixtures the page still says so, in the first group, and still
/// offers every add, in the second.
#[test]
fn the_fixtures_page_keeps_its_empty_message_with_no_fixtures() {
    let mut h = blank_with_setup();
    open(&mut h);
    let (hold, add) = (rect(&h, "Workholding"), rect(&h, "Add workholding"));
    let msg = rect(&h, EMPTY);
    assert!(msg.top() > hold.top() && msg.bottom() < add.top(), "the message sits in Workholding");
    assert!(h.query_by_label(FULL).is_none(), "the full line is not shown when empty");
    assert!(h.query_by_label_contains("Mark ").is_none(), "no Mark button with nothing selected");
    assert_adds(&h, add.top());
    assert!(h.query_by_label("Fixtures - Setup1").is_some(), "the page heading stays");
}

/// Each add still opens its own dialog, for this setup, from its new
/// button. (Import fixture opens a native file dialog, so it is pinned by
/// its place in the layout only.)
#[test]
fn the_fixtures_page_adds_open_their_dialogs() {
    use crate::state::FixtureLibDialog as D;
    let mut h = blank_with_setup();
    open(&mut h);
    for label in ["Add built-in vise…", "Add accessory…", "Add fixture plate…"] {
        h.get_by_label(label).click();
        h.run_steps(2);
        let lib = h.state().state.fixture_lib.clone();
        let ok = match label {
            "Add built-in vise…" => matches!(lib, Some(D::Vise(_))),
            "Add accessory…" => matches!(lib, Some(D::Accessory(_))),
            _ => matches!(lib, Some(D::Plate(_))),
        };
        assert!(ok, "{label} opens its dialog, got {lib:?}");
        h.state_mut().state.fixture_lib = None;
    }
}

/// A model selected and not yet a fixture offers one Mark button, on its
/// own row below the adds; it tags the model into this setup.
#[test]
fn the_fixtures_page_offers_mark_for_exactly_one_selected_model() {
    let mut h = blank_with_setup();
    open(&mut h);
    {
        let st = &mut h.state_mut().state;
        st.history.execute(
            &st.world,
            camcraft_app::commands::ImportModel::from_mesh(
                "block",
                camcraft_app::geometry::cube(
                    camcraft_app::geometry::Point3::new(0.0, 0.0, 0.0),
                    camcraft_app::geometry::Point3::new(4.0, 4.0, 4.0),
                ),
            ),
        );
        st.pending.push_back(CommandKind::Select(Some("block".into())));
        st.drain_actions();
    }
    h.run_steps(2);
    let mark = rect(&h, "Mark block as fixture");
    assert!(mark.top() > rect(&h, "Add fixture plate…").bottom(), "Mark under the adds");
    assert_eq!(mark.left(), rect(&h, "Import fixture…").left(), "Mark starts at the left edge");
    assert!((mark.width() - rect(&h, "Import fixture…").width()).abs() < 3.0, "Mark is as wide as the adds");
    h.get_by_label("Mark block as fixture").click();
    h.run_steps(3);
    assert!(
        h.state().state.status.to_lowercase().contains("fixture") || h.query_by_label(FULL).is_some(),
        "the click tagged the model: {}",
        h.state().state.status
    );
    assert!(h.query_by_label(FULL).is_some(), "block now stands in Workholding");
}

/// #1119 / #1432: every input on the page keeps its accessible name once
/// the rows open their forms.
#[test]
fn the_fixtures_page_keeps_every_input_named() {
    let mut h = furnished();
    h.query_all_by_label("Place…").next().expect("a Place… verb").click();
    h.run_steps(2);
    h.query_all_by_label("Place on plate…").next().expect("a Place on plate… verb").click();
    h.run_steps(3);
    // The dowel inputs draw only for a dowel pair: turn it on so they are
    // checked too.
    h.get_by_label("Dowel pair").click();
    h.run_steps(3);
    assert!(h.query_all_by_label("Dowels").count() >= 2, "the dowel fields did not draw");
    let (bad, seen) = super::super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 6, "only {seen} inputs found: the forms did not open");
    assert!(bad.is_empty(), "unnamed inputs {bad:?}");
}

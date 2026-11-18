//! The Heights tab's From: Selection entry (M11.3 A), driven through the
//! real page code: every height combo offers "Selection", the row echoes
//! the Z the app's one resolver gives for the edited op's own source, and
//! picking it on a bottom row stores the selection's LOWEST end.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::operation::{Height, HeightReference as R, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::state::{CommandKind, State};

fn pocket(clearance_z: Height, top_z: Height, bottom_z: Height) -> OpParams {
    OpParams::Pocket { rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z,
        retract_z: None,
        top_z,
        bottom_z,
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

/// Stock z 0..20, a block z 5..9, a tool, and one pocket sourced from
/// the block - opened in the pane the way a tree click opens it.
fn opened(params: OpParams) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 5.0), Point3::new(2.0, 2.0, 9.0))),
    );
    let src = SrcRef::Entity("block".into());
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "P1", "EM-0400-2FL", src, params).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("P1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    state
}

fn heights_tab(state: State) -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(600.0, 900.0))
        .build_ui_state(
            |ui, st: &mut State| super::params_page::params_fields(ui, st, super::op_tabs::SEC_HEIGHTS),
            state,
        )
}

fn combos_showing(h: &Harness<'static, State>, text: &str) -> usize {
    h.query_all_by(|n| {
        n.role() == Role::ComboBox && n.value().as_deref() == Some(text)
    })
    .count()
}

#[test]
fn a_selection_height_echoes_the_z_of_the_ops_own_source() {
    let state = opened(pocket(
        Height::of(R::TopOfSelection, 15.0),
        Height::of(R::TopOfSelection, 0.0),
        Height::of(R::BottomOfSelection, 0.0),
    ));
    let mut harness = heights_tab(state);
    harness.run();
    assert_eq!(combos_showing(&harness, "Selection"), 3, "all three rows read From: Selection");
    for echo in ["(= Z 24.000)", "(= Z 9.000)", "(= Z 5.000)"] {
        assert!(harness.query_by_label(echo).is_some(), "the row echoes {echo}");
    }
    assert!(harness.state().pending.is_empty(), "drawing the tab queues nothing");
}

#[test]
fn the_combo_offers_selection_and_a_bottom_row_takes_the_low_end() {
    let state = opened(pocket(Height::fixed(25.0), Height::fixed(9.0), Height::fixed(5.0)));
    let mut harness = heights_tab(state);
    harness.run();
    assert_eq!(combos_showing(&harness, "Fixed value"), 3);
    // The bottom row is the last height combo on the tab.
    let last = harness
        .query_all_by(|n| {
            n.role() == Role::ComboBox && n.value().as_deref() == Some("Fixed value")
        })
        .last()
        .expect("the bottom row's combo");
    last.click();
    harness.run();
    harness.get_by_label("Selection").click();
    harness.run();
    match &harness.state().op_ui.params {
        OpParams::Pocket { bottom_z, top_z, .. } => {
            assert_eq!(bottom_z.reference, R::BottomOfSelection, "a floor reads the low end");
            assert_eq!(top_z.reference, R::Fixed, "the other rows are untouched");
        }
        other => panic!("pocket expected, got {other:?}"),
    }
    // The switch zeroes the offset (user hand test 2026-11-06): the
    // floor IS the selection's lowest Z (5), never the old fixed 5 on top.
    match &harness.state().op_ui.params {
        OpParams::Pocket { bottom_z, .. } => assert_eq!(bottom_z.offset, 0.0, "a selection floor starts at offset 0"),
        other => panic!("pocket expected, got {other:?}"),
    }
    assert!(harness.query_by_label("(= Z 5.000)").is_some(), "the floor sits on the block's bottom, Z 5");
}

fn with_retract(mut p: OpParams, r: Height) -> OpParams {
    if let OpParams::Pocket { retract_z, .. } = &mut p {
        *retract_z = Some(r);
    }
    p
}

/// M11.3 A owed item: the retract-vs-clearance note resolves through the
/// op's own source, so a Selection-referenced clearance under a fixed
/// retract speaks with the selection's resolved Z (block top 9 + 1).
#[test]
fn a_selection_clearance_under_the_retract_names_its_resolved_z() {
    let params = with_retract(
        pocket(Height::of(R::TopOfSelection, 1.0), Height::fixed(9.0), Height::fixed(5.0)),
        Height::fixed(12.0),
    );
    let mut harness = heights_tab(opened(params));
    harness.run();
    let note = "above clearance - the hop travels at 10.000 (min of the two)";
    assert!(harness.query_by_label(note).is_some(), "the note names the Selection Z");
}

/// The mirror: a Selection-referenced retract (9 + 6 = 15) above a fixed
/// clearance of 11 - the old `Height::z` read None here and said nothing.
/// Under a clearance of 20 the relation is healthy and the page is quiet.
#[test]
fn a_selection_retract_above_clearance_produces_the_note() {
    let params = with_retract(
        pocket(Height::fixed(11.0), Height::fixed(9.0), Height::fixed(5.0)),
        Height::of(R::TopOfSelection, 6.0),
    );
    let mut harness = heights_tab(opened(params));
    harness.run();
    let note = "above clearance - the hop travels at 11.000 (min of the two)";
    assert!(harness.query_by_label(note).is_some(), "the Selection retract speaks");
    let quiet = with_retract(
        pocket(Height::fixed(20.0), Height::fixed(9.0), Height::fixed(5.0)),
        Height::of(R::TopOfSelection, 6.0),
    );
    let mut harness = heights_tab(opened(quiet));
    harness.run();
    assert!(harness.query_by_label_contains("above clearance -").is_none(), "15 under 20 is healthy");
    assert!(harness.query_by_label_contains("unresolved").is_none(), "and it resolved");
}

/// Unresolved is said, never guessed: with no op source in hand a
/// Selection height cannot resolve, and the note names the reason.
#[test]
fn an_unresolved_relation_says_why_never_a_number() {
    let state = State::new();
    let note = crate::operations::retract_note(
        &state.world,
        None,
        None,
        &Height::of(R::TopOfSelection, 6.0),
        &Height::fixed(11.0),
    );
    assert_eq!(
        note.as_deref(),
        Some("retract vs clearance unresolved: no operation selection in this context")
    );
    let corner =
        crate::operations::corner_note(&state.world, None, None, &Height::of(R::TopOfSelection, 0.0), 1.0);
    assert_eq!(corner, "the corner is unresolved: no operation selection in this context");
}

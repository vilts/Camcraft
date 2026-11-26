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
    OpParams::Pocket { pass_start: Default::default(), rest: None,
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
        corner_relief: Default::default(),
        picks: Default::default(),
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
        None,
        &Height::of(R::TopOfSelection, 6.0),
        &Height::fixed(11.0),
    );
    assert_eq!(
        note.as_deref(),
        Some("retract vs clearance unresolved: no operation selection in this context")
    );
    let tip =
        crate::operations::corner_note(&state.world, None, None, None, &Height::of(R::TopOfSelection, 0.0), 1.0);
    assert_eq!(tip, "the cone tip is unresolved: no operation selection in this context");
}


/// #2416: the bar has Y -1..11 and Z -20..10. A -90 measures
/// heights along +Y. The nonzero origin Z catches a missing fold shift.
fn asymmetric_tilted_pocket() -> State {
    let mut params = pocket(
        Height::of(R::TopOfStock, 10.0),
        Height::of(R::TopOfStock, 0.0),
        Height::of(R::TopOfStock, -1.0),
    );
    if let OpParams::Pocket { tilt, .. } = &mut params {
        *tilt = Some(camcraft_app::operation::Tilt {
            a_deg: -90.0, b_deg: 0.0, origin: [2.0, 0.0, 3.0], basis_version: 1,
        });
    }
    let mut state = opened(params);
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 40.0, dy: 12.0, dz: 30.0 },
        pos: [0.0, 5.0, -20.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state
}

#[test]
fn tilted_height_planes_use_tool_axis_and_local_stock_corners() {
    let state = asymmetric_tilted_pocket();
    let frame = state.op_ui.params.tilt().expect("tilt").frame().expect("frame");
    let lines = crate::interact::height_plane_lines(&state);
    let clearance: Vec<_> = lines.iter().filter(|v| {
        (0..3).all(|i| (v[3 + i] - crate::interact::CLEARANCE_RGB[i]).abs() < 1e-6)
    }).collect();
    assert_eq!(clearance.len(), 8, "four clearance edges");
    // Tool diameter 4 gives a pad of 4. In local XY, stock bounds are
    // X -22..18 and Y -7..23. The rectangle reaches X -26..22, Y -11..27.
    let expected = [(-26.0, -11.0), (22.0, -11.0), (22.0, 27.0), (-26.0, 27.0)];
    for vertex in &clearance {
        let point = frame.to_local(Point3::new(f64::from(vertex[0]), f64::from(vertex[1]), f64::from(vertex[2])));
        assert!((point.z - 21.0).abs() < 1e-5, "clearance along tool axis: {point:?}");
        assert!(expected.iter().any(|&(x, y)| (point.x - x).abs() < 1e-5 && (point.y - y).abs() < 1e-5), "local stock corner: {point:?}");
    }
    for (x, y) in expected {
        let want = frame.to_world(Point3::new(x, y, 21.0));
        assert!(clearance.iter().any(|v| (f64::from(v[0]) - want.x).abs() < 1e-5
            && (f64::from(v[1]) - want.y).abs() < 1e-5
            && (f64::from(v[2]) - want.z).abs() < 1e-5), "world corner: {want:?}");
    }
}

#[test]
fn tilted_height_echoes_use_tool_axis_on_asymmetric_stock() {
    let mut harness = heights_tab(asymmetric_tilted_pocket());
    harness.run();
    for echo in ["(= 21.000 along tool axis)", "(= 11.000 along tool axis)", "(= 10.000 along tool axis)"] {
        assert!(harness.query_by_label(echo).is_some(), "the row echoes {echo}");
    }
    assert!(harness.query_by_label_contains("(= Z ").is_none(), "tilted values name their axis");
}

#[test]
fn invalid_draft_tilt_hides_height_planes_and_names_the_missing_direction() {
    let mut state = asymmetric_tilted_pocket();
    if let OpParams::Pocket { tilt: Some(tilt), .. } = &mut state.op_ui.params {
        tilt.a_deg = f64::NAN;
    }
    assert!(crate::interact::height_planes(&state).is_empty(), "invalid tilt has no planes");
    assert!(crate::interact::height_plane_lines(&state).is_empty(), "invalid tilt has no lines");
    let mut harness = heights_tab(state);
    harness.run();
    assert_eq!(harness.query_all_by_label("(unresolved: the 3+2 index commands no direction - its angles are not finite)").count(), 3, "each invalid tilt row names its cause");
    assert!(harness.query_by_label_contains("(= Z ").is_none(), "invalid tilt cannot use setup Z");
}


#[test]
fn tilted_height_region_level_uses_the_same_fold() {
    let state = asymmetric_tilted_pocket();
    let level = camcraft_app::operation::region_z(
        &state.world, None, Some(&state.op_ui.src()), &state.op_ui.params,
    ).expect("tilted region level");
    assert!((level - 14.0).abs() < 1e-9, "top 11 along tool axis plus origin Z 3: {level}");
    let mut invalid = state.op_ui.params.clone();
    if let OpParams::Pocket { tilt: Some(tilt), .. } = &mut invalid {
        tilt.a_deg = f64::NAN;
    }
    assert_eq!(camcraft_app::operation::region_z(&state.world, None, Some(&state.op_ui.src()), &invalid), None);
}

#[test]
fn tilted_height_echoes_follow_an_unapplied_tilt_edit() {
    let mut state = asymmetric_tilted_pocket();
    if let OpParams::Pocket { tilt: Some(tilt), .. } = &mut state.op_ui.params {
        tilt.a_deg = 0.0;
        tilt.b_deg = 180.0;
    }
    let mut harness = heights_tab(state);
    harness.run();
    // The underside is Z -20. From origin Z 3, it lies 23 along -Z.
    for echo in ["(= 33.000 along tool axis)", "(= 23.000 along tool axis)", "(= 22.000 along tool axis)"] {
        assert!(harness.query_by_label(echo).is_some(), "draft tilt echoes {echo}");
    }
}


#[test]
fn tilted_fixed_height_echoes_keep_the_generation_origin_shift() {
    let mut state = asymmetric_tilted_pocket();
    if let OpParams::Pocket { clearance_z, top_z, bottom_z, .. } = &mut state.op_ui.params {
        *clearance_z = Height::fixed(30.0);
        *top_z = Height::fixed(13.0);
        *bottom_z = Height::fixed(12.0);
    }
    let mut harness = heights_tab(state);
    harness.run();
    for echo in ["(= 27.000 along tool axis)", "(= 10.000 along tool axis)", "(= 9.000 along tool axis)"] {
        assert!(harness.query_by_label(echo).is_some(), "fixed height echoes {echo}");
    }
}


#[test]
fn tilted_height_labels_show_local_axis_distance() {
    let state = asymmetric_tilted_pocket();
    let labels: Vec<_> = crate::interact::overlay_labels(&state, 4.0 / 3.0)
        .into_iter().map(|(_, _, text)| text).collect();
    for expected in ["Clearance 21.00 along tool axis", "Top 11.00 along tool axis", "Bottom 10.00 along tool axis"] {
        assert!(labels.iter().any(|text| text.contains(expected)), "viewport label {expected}: {labels:?}");
    }
}

#[path = "heights_note_tests.rs"]
mod notes;

#[path = "retract_link_tests.rs"]
mod retract_links;

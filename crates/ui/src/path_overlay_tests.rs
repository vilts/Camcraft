//! #2201 counter pins: a still frame with a path shown, selected or
//! previewed builds no path lines and measures no chevron arm; a change
//! of the path, its warnings, the dim, the palette or the arm builds
//! exactly once. The cached lines equal a fresh build.
//! Provenance: our own frame-cost tests over our own path display.

use camcraft_app::commands::color_vision::color_vision_set;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{ColorVision, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::toolpath_lines;
use crate::state::{CommandKind, State};

/// More moves than the 8000-move Offset path of the issue, in layers.
const MOVES: usize = 8200;

/// A Face path, repeated layer by layer past [`MOVES`], on "Face1".
fn state_with_large_path() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
    state.history.execute(
        &state.world,
        CreateOperation::new(
            &state.world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face {
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
            },
        )
        .expect("operation"),
    );
    state.history.execute(&state.world, GenerateOperation::new(&state.world, "Face1").expect("generate"));
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    let tp = model::toolpath_of(&state.world, &e).expect("generated path");
    assert!(!tp.path.moves.is_empty(), "the face path has moves");
    let mut path = tp.path.clone();
    let layer = path.moves.clone();
    while path.moves.len() < MOVES {
        path.moves.extend_from_slice(&layer);
    }
    e.entity_view(&state.world).set(Toolpath::new(path, tp.warnings.clone()));
    state
}

fn select(state: &mut State, name: Option<&str>) {
    state.pending.push_back(CommandKind::Select(name.map(str::to_owned)));
    state.drain_actions();
}

fn builds(state: &State) -> (u64, u64) {
    (state.frame_cache.paths.line_builds(), state.frame_cache.paths.arm_builds())
}

/// The selected path's lines, built fresh with no cache.
fn fresh(state: &State) -> Vec<[f32; 6]> {
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    let tp = model::toolpath_of(&state.world, &e).expect("path");
    let arm = state.frame_cache.paths.arm(&state.world);
    let pal = crate::egui_app::colorvision::palette_of(&state.world);
    let mut out = Vec::new();
    crate::path_lines::cache_lines(&mut out, &tp, pal, 1.0, arm);
    out
}

#[test]
fn a_still_selected_path_builds_its_lines_once() {
    let mut state = state_with_large_path();
    select(&mut state, Some("Face1"));
    let first = toolpath_lines(&state);
    assert!(!first.is_empty(), "the selected path draws");
    let after_first = builds(&state);
    for _ in 0..10 {
        assert_eq!(toolpath_lines(&state), first, "a still frame draws the same lines");
    }
    assert_eq!(builds(&state), after_first, "ten still frames build no line and measure no arm");
    assert_eq!(first, fresh(&state), "the cached lines are the fresh build");
}

#[test]
fn a_still_shown_path_builds_nothing() {
    let mut state = state_with_large_path();
    select(&mut state, None);
    state.show_all_paths = true;
    let first = toolpath_lines(&state);
    assert!(!first.is_empty(), "show all draws the unselected path");
    let after_first = builds(&state);
    for _ in 0..10 {
        let _ = toolpath_lines(&state);
    }
    assert_eq!(builds(&state), after_first, "still frames under show all build nothing");
}

#[test]
fn each_changed_input_builds_exactly_once() {
    let mut state = state_with_large_path();
    select(&mut state, Some("Face1"));
    let _ = toolpath_lines(&state);
    let (lines0, arms0) = builds(&state);

    // A new path: one line build, then still again.
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    let tp = model::toolpath_of(&state.world, &e).expect("path");
    let mut path = tp.path.clone();
    path.moves.truncate(path.moves.len() / 2);
    e.entity_view(&state.world).set(Toolpath::new(path, tp.warnings.clone()));
    let _ = toolpath_lines(&state);
    let _ = toolpath_lines(&state);
    assert_eq!(builds(&state), (lines0 + 1, arms0), "a new path builds its lines once");
    assert_eq!(toolpath_lines(&state), fresh(&state), "and the new lines are the fresh build");

    // The selection moves away under show all: the dim changes.
    state.show_all_paths = true;
    select(&mut state, None);
    let _ = toolpath_lines(&state);
    let _ = toolpath_lines(&state);
    assert_eq!(builds(&state), (lines0 + 2, arms0), "a new dim builds once");

    // Another colour-vision palette.
    color_vision_set(&state.world, ColorVision::Tritan);
    let _ = toolpath_lines(&state);
    let _ = toolpath_lines(&state);
    assert_eq!(builds(&state), (lines0 + 3, arms0), "a new palette builds once");

    // A large model lands: the chevron arm is measured again, and the
    // lines that wear it are built again, once each.
    state.history.execute(&state.world, ImportModel::from_mesh("block", cube(Point3::ZERO, Point3::new(400.0, 400.0, 400.0))));
    let _ = toolpath_lines(&state);
    let _ = toolpath_lines(&state);
    assert_eq!(builds(&state), (lines0 + 4, arms0 + 1), "a new extent measures the arm once");
}

#[test]
fn a_hidden_or_deleted_path_frees_its_lines() {
    let mut state = state_with_large_path();
    select(&mut state, Some("Face1"));
    let _ = toolpath_lines(&state);
    assert_eq!(state.frame_cache.paths.entries(), 1, "the drawn path holds one entry");
    state.hidden_paths.push("Face1".to_owned());
    assert!(toolpath_lines(&state).is_empty(), "a hidden path does not draw");
    assert_eq!(state.frame_cache.paths.entries(), 0, "a hidden path frees its lines");
}

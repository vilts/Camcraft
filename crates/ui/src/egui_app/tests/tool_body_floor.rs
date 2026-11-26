//! Provenance: our own editor-envelope laws over synthetic spun holders (#2009).

use std::sync::Arc;

use camcraft_app::commands::AddTool;
use camcraft_app::model;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_body;
use camcraft_app::tools::ToolKind;

use super::{State, envelope};

const TOOL: &str = "floor-control";

fn editor(machine_default: bool) -> State {
    editor_with_kind(machine_default, ToolKind::EndMill)
}

fn editor_with_kind(machine_default: bool, kind: ToolKind) -> State {
    let mut state = State::new();
    let mut tool = state.tool_ui.to_tool();
    tool.kind = kind;
    tool.diameter = 8.0;
    tool.flute_len = 20.0;
    tool.shank_d = 8.0;
    tool.shank_len = 40.0;
    tool.holder_d = 60.0;
    tool.holder_len = 60.0;
    state.history.execute(
        &state.world,
        AddTool::new(&state.world, TOOL, tool).unwrap(),
    );
    let spun = SpunProfile::new(vec![
        SpunPoint::line(60.0, 0.0),
        SpunPoint::line(60.0, 12.0),
        SpunPoint::line(80.0, 12.0),
        SpunPoint::line(80.0, 0.0),
    ])
    .unwrap();
    let mut body = tool_body::build_spun(spun);
    body.machine_default = machine_default;
    let entity = model::find_by_name(&state.world, TOOL).unwrap();
    tool_body::set_body(&state.world, &entity, Some(&body));
    crate::actions::open_tool_shape_editor(&mut state, TOOL);
    state
}

fn reach(bands: &[(f64, f64, f64)]) -> (f64, f64) {
    bands
        .iter()
        .fold((0.0_f64, 0.0_f64), |(top, radius), &(z, len, r)| {
            (top.max(z + len), radius.max(r))
        })
}

fn assert_raw_draft(bands: &[(f64, f64, f64)], top: f64) {
    let actual = reach(bands);
    assert!((actual.0 - top).abs() < 1e-6, "draft top: {actual:?}");
    assert!((actual.1 - 12.0).abs() < 2e-6, "draft radius: {actual:?}");
}

#[test]
fn machine_default_editor_envelope_matches_checked_parameter_floor() {
    let mut state = editor(true);
    let entity = model::find_by_name(&state.world, TOOL).unwrap();
    let tool = model::tool_of(&state.world, &entity).unwrap();
    let (_, judged) = tool_body::judged(&state.world, &entity, tool).unwrap();
    let expected: Vec<_> = judged
        .unwrap()
        .bands()
        .iter()
        .map(|b| (b.above, b.len, b.r))
        .collect();
    assert_eq!(reach(&expected), (120.0, 30.0));
    assert_eq!(
        envelope(&mut state).unwrap(),
        expected,
        "the editor shows the checked envelope"
    );
}

#[test]
fn machine_default_editor_envelope_tracks_parameter_edits() {
    let mut state = editor(true);
    envelope(&mut state).unwrap();
    state.tool_ui.holder_d = 80.0;
    state.tool_ui.holder_len = 90.0;
    assert_eq!(reach(&envelope(&mut state).unwrap()), (150.0, 40.0));
}

#[test]
fn explicit_editor_body_still_replaces_parameter_cylinder() {
    let mut state = editor(false);
    assert_raw_draft(&envelope(&mut state).unwrap(), 80.0);
}

#[test]
fn moved_default_body_is_an_explicit_editor_draft() {
    let mut state = editor(true);
    state.sync_tool_body_ui();
    state.tool_body_ui.offset = 5.0;
    assert_raw_draft(&envelope(&mut state).unwrap(), 85.0);
}

#[test]
fn replaced_default_mesh_is_an_explicit_editor_draft() {
    let mut state = editor(true);
    state.sync_tool_body_ui();
    // A new import has its own facet allocation, even if its dimensions agree.
    state.tool_body_ui.facets = Arc::from(state.tool_body_ui.facets.to_vec());
    assert_raw_draft(&envelope(&mut state).unwrap(), 80.0);
}

#[test]
fn invalid_default_body_placement_has_no_editor_envelope() {
    let mut state = editor(true);
    state.sync_tool_body_ui();
    state.tool_body_ui.axis = [0.0; 3];
    assert!(
        envelope(&mut state).is_none(),
        "invalid placement cannot show a checked floor"
    );
}

#[test]
fn lathe_frame_default_body_has_no_spindle_editor_envelope() {
    let mut state = editor_with_kind(true, ToolKind::Bore);
    state.sync_tool_body_ui();
    state.set_body_draft_frame(true);
    assert!(
        state.tool_body_ui.lathe.is_some(),
        "the boring bar now has a lathe draft"
    );
    assert!(
        envelope(&mut state).is_none(),
        "a lathe draft has no spindle envelope"
    );
}

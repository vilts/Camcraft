//! #361: the tool editor's Body section on a lathe-held tool - the
//! imported holder is placed in the turning tool frame (radial and
//! spindle directions, tip point, insert zone), Apply body lands one
//! undoable `SetToolBody::lathe`, a placement that is no frame is
//! refused by name in the editor, picks only fill the draft, a boring
//! bar chooses its frame, and a STEP meshed too coarsely is refused at
//! the import. Provenance: our own page law; the test holder is our own
//! geometry (`testdata/models/lathe_holder_offset.step`).

use super::*;
use camcraft_app::lathe_holder::LathePlacement;
use camcraft_app::lathe_insert_zone::InsertZone;
use camcraft_app::tool_body::BodyFrame;
use camcraft_app::tool_body_lathe_view::FrameChoice;
use camcraft_app::tool_body_view::BodyPick;
use crate::state::BodyPickMode;

const INSERT: &str = "TN-1600-0R8";
const BAR: &str = "BR-1000";
const MILL: &str = "EM-0600-4FL";

/// How the fixture sits in the turning tool frame (see the app suite
/// `vendor_tool_body`): placed, it spans x 2..120, z -26..-1.
const PLACED: LathePlacement = LathePlacement { radial: [0.0, 1.0, 0.0], along: [1.0, 0.0, 0.0], tip: [5.0, 10.0, 3.0] };

fn holder_step() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/lathe_holder_offset.step")
}

/// The seeded harness with `tool` racked and the editor open on it.
fn editor_on(tool: &str) -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    rack(&mut harness.state_mut().state, &[tool]);
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, tool);
    harness.run();
    harness.run();
    harness
}

fn frame_of(state: &State, tool: &str) -> Option<BodyFrame> {
    model::find_by_name(&state.world, tool)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .map(|b| b.frame)
}

fn landed(state: &State, tool: &str) -> Option<(LathePlacement, Option<InsertZone>)> {
    match frame_of(state, tool)? {
        BodyFrame::Lathe { place, zone, .. } => Some((place, zone)),
        BodyFrame::Spindle { .. } => None,
    }
}

fn place(harness: &mut Harness<'static, ShellUi>, at: LathePlacement) {
    let st = &mut harness.state_mut().state;
    st.import_tool_body_from(&holder_step());
    st.tool_body_ui.lathe.as_mut().expect("a lathe draft").place = at;
    harness.run();
}

#[test]
fn a_turning_tool_places_its_holder_in_the_lathe_frame_as_one_undoable_command() {
    let mut harness = editor_on(INSERT);
    assert!(harness.query_by_label("Import body…").is_some(), "Import body… stands on a turning tool");
    assert!(harness.query_by_label("Body frame").is_none(), "a lathe steel has one frame, no switch");
    assert!(harness.query_by_label_contains("the holder is not judged").is_some(), "a bare turning tool says so");
    place(&mut harness, PLACED);
    assert!(frame_of(&harness.state().state, INSERT).is_none(), "importing alone never touches the world");
    for label in ["Pick tip", "Pick radial", "Pick spindle", "Insert zone", "Apply body"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands on a lathe draft");
    }
    assert!(harness.query_by_label("Pick axis").is_none(), "no spindle-axis placement on a lathe steel");
    assert!(
        harness.query_by_label_contains("placed: radial 2.0 to 120.0, spindle -26.0 to -1.0 mm from the tip").is_some(),
        "the note reads the body in the turning frame"
    );
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(landed(st, INSERT), Some((PLACED, None)), "status: {}", st.status);
    assert!(st.status.contains("lathe body set"), "{}", st.status);
    // The landed body re-mirrors in its own frame: applying it again
    // changes no judged number.
    let before = frame_of(st, INSERT);
    assert_eq!(st.tool_body_ui.lathe.as_ref().map(|l| l.place), Some(camcraft_app::tool_body_lathe_view::AS_PLACED));
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(frame_of(&harness.state().state, INSERT).is_none(), "one undo takes the body off - one step");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert_eq!(frame_of(&harness.state().state, INSERT), before, "redo puts it back");
    // Clear body is the same command, and undoes.
    harness.get_by_label("Clear body").click();
    harness.run();
    harness.run();
    assert!(frame_of(&harness.state().state, INSERT).is_none(), "cleared");
    assert!(harness.state().state.tool_body_ui.lathe.is_some(), "the draft stays a lathe draft");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert_eq!(frame_of(&harness.state().state, INSERT), before, "undo restores the cleared body");
}

#[test]
fn a_lathe_placement_that_is_no_frame_is_refused_by_name_in_the_editor() {
    let mut harness = editor_on(INSERT);
    place(&mut harness, LathePlacement { along: [1.0, 0.2, 0.0], ..PLACED });
    assert!(harness.query_by_label_contains("placement: the lathe body's radial and spindle directions are not square").is_some());
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert!(frame_of(&harness.state().state, INSERT).is_none(), "nothing lands");
    let st = &harness.state().state;
    assert!(st.status.contains("tool body not set") && st.status.contains("not square"), "{}", st.status);
    assert!(!st.tool_body_ui.facets.is_empty(), "the draft stays for correction");
}

#[test]
fn the_insert_zone_rides_the_same_command_and_an_oversized_one_is_refused_by_name() {
    let mut harness = editor_on(INSERT);
    place(&mut harness, PLACED);
    {
        let l = harness.state_mut().state.tool_body_ui.lathe.as_mut().expect("a lathe draft");
        l.zone_on = true;
        (l.zone_toward, l.zone_away) = ([0.0, 6.0, 4.0], [5.0, 6.0, 0.0]);
    }
    harness.run();
    assert!(harness.query_by_label("Insert radial").is_some(), "the zone's rows stand once it is on");
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let zone = InsertZone { lo: [-0.0, -6.0, -4.0], hi: [5.0, 6.0, 0.0] };
    assert_eq!(landed(&harness.state().state, INSERT), Some((PLACED, Some(zone))), "status: {}", harness.state().state.status);
    // Re-mirrored, the draft shows the landed zone.
    let l = harness.state().state.tool_body_ui.lathe.clone().expect("a lathe draft");
    assert!(l.zone_on && l.zone_toward == [0.0, 6.0, 4.0] && l.zone_away == [5.0, 6.0, 0.0], "{l:?}");

    // A zone no insert could fill never lands; the body keeps its own.
    harness.state_mut().state.tool_body_ui.lathe.as_mut().expect("a lathe draft").zone_away[0] = 30.0;
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.contains("tool body not set") && st.status.contains("the insert zone reaches 30.000 mm from the tip along x"), "{}", st.status);
    assert_eq!(landed(st, INSERT).and_then(|(_, z)| z), Some(zone));
}

#[test]
fn picks_fill_the_lathe_placement_and_never_the_world() {
    let mut harness = editor_on(INSERT);
    place(&mut harness, PLACED);
    let hit = BodyPick { facet: 0, hit: [4.0, 12.5, 12.0], vertex: [4.0, 12.0, 13.0], normal: Some([0.0, -1.0, 0.0]) };
    for (mode, want) in [
        (BodyPickMode::Tip, LathePlacement { tip: [4.0, 12.0, 13.0], ..PLACED }),
        (BodyPickMode::Radial, LathePlacement { tip: [4.0, 12.0, 13.0], radial: [0.0, -1.0, 0.0], ..PLACED }),
        (BodyPickMode::Along, LathePlacement { tip: [4.0, 12.0, 13.0], radial: [0.0, -1.0, 0.0], along: [0.0, -1.0, 0.0] }),
    ] {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.pick = mode;
        st.apply_body_pick(hit);
        assert_eq!(st.tool_body_ui.lathe.as_ref().map(|l| l.place), Some(want), "{mode:?}: {}", st.status);
        assert_eq!(st.tool_body_ui.pick, BodyPickMode::Off, "a pick is one click");
    }
    let st = &mut harness.state_mut().state;
    assert!(st.status.contains("spindle direction picked: -Y"), "{}", st.status);
    // A facet without area names no direction.
    st.tool_body_ui.pick = BodyPickMode::Radial;
    st.apply_body_pick(BodyPick { normal: None, ..hit });
    assert!(st.status.contains("direction not picked"), "{}", st.status);
    assert!(frame_of(st, INSERT).is_none(), "picks never reach the world");
    assert!(st.pending.is_empty(), "and queue nothing");
}

#[test]
fn a_boring_bar_chooses_its_frame_and_a_mill_tool_has_none_to_choose() {
    let mut harness = editor_on(BAR);
    assert_eq!(harness.state().state.editor_body_frames(), FrameChoice::Either);
    assert!(harness.query_by_label("Body frame").is_some(), "a boring bar serves both machines");
    assert!(harness.state().state.tool_body_ui.lathe.is_none(), "it starts along the spindle axis");
    harness.state_mut().state.set_body_draft_frame(true);
    place(&mut harness, PLACED);
    assert!(harness.query_by_label("Pick radial").is_some(), "the lathe placement stands once chosen");
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert_eq!(landed(&harness.state().state, BAR), Some((PLACED, None)), "status: {}", harness.state().state.status);
    assert!(harness.state().state.tool_body_ui.lathe.is_some(), "a landed lathe body re-mirrors as a lathe draft");

    let mut harness = editor_on(MILL);
    assert_eq!(harness.state().state.editor_body_frames(), FrameChoice::Spindle);
    assert!(harness.query_by_label("Body frame").is_none());
    harness.state_mut().state.set_body_draft_frame(true);
    assert!(harness.state().state.tool_body_ui.lathe.is_none(), "a spun tool is never placed in the lathe frame");
}

#[test]
fn a_step_meshed_too_coarsely_is_refused_at_the_import_by_name() {
    let mut harness = editor_on(INSERT);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    camcraft_app::geometry::import::step::with_facet_budget(2_000, || harness.state_mut().state.import_tool_body_from(&path));
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.contains("tool body not imported") && st.status.contains("too detailed to import at full fidelity"), "{}", st.status);
    assert!(st.tool_body_ui.facets.is_empty(), "a refused file leaves no draft");
}

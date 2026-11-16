//! #361 / #1040: the tool editor's modular piece-assembly section -
//! attaching pieces onto a landed spindle or lathe body (auto-mated or
//! naming target/piece CSYS), removing a piece via the Remove button in
//! one undoable step, and refusing missing/ambiguous CSYS in the status.
//! Provenance: our own page test over our own modular fixtures; no vendor
//! file consulted.

use super::*;
use camcraft_app::lathe_holder::LathePlacement;
use camcraft_app::tool_body;

const INSERT: &str = "TN-1600-0R8";
const MILL: &str = "EM-0600-4FL";

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

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

fn pieces_len(state: &State, tool: &str) -> usize {
    model::find_by_name(&state.world, tool)
        .and_then(|e| tool_body::body_of(&state.world, &e))
        .map_or(0, |b| b.pieces.len())
}

#[test]
fn editor_attaches_and_removes_modular_lathe_pieces_in_one_undo_step() {
    let mut harness = editor_on(INSERT);
    let head = fixture("modular_lathe_head.step");
    let clamp = fixture("modular_clamp_unit.step");
    {
        let st = &mut harness.state_mut().state;
        st.import_tool_body_from(&head);
        st.tool_body_ui.lathe.as_mut().expect("lathe draft").place = LathePlacement {
            radial: [1.0, 0.0, 0.0],
            along: [0.0, 0.0, 1.0],
            tip: [0.0, 0.0, 0.0],
        };
    }
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, INSERT), 1);
    assert!(harness.query_by_label("Assembly (1 piece(s))").is_some());
    assert!(harness.query_by_label("Attach piece…").is_some());

    // Attach clamping unit via unambiguous auto-mate (MCS <-> CSW).
    harness.state_mut().state.attach_tool_body_piece_from(&clamp);
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, INSERT), 2);
    assert!(harness.query_by_label("Assembly (2 piece(s))").is_some());
    assert!(harness.query_by_label_contains("0: modular_lathe_head.step").is_some());
    assert!(harness.query_by_label_contains("1: modular_clamp_unit.step").is_some());

    // Remove piece 1 via the UI Remove button, then undo to restore it.
    let mut remove_btns: Vec<_> = harness.get_all_by_label("Remove").collect();
    assert_eq!(remove_btns.len(), 2);
    remove_btns.swap_remove(1).click();
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, INSERT), 1);

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, INSERT), 2);
}

#[test]
fn editor_mates_spindle_pieces_with_explicit_csys_and_refuses_bad_names() {
    let mut harness = editor_on(MILL);
    let head = fixture("tool_csys_box_iso13399.step");
    let unit = fixture("modular_clamp_unit.step");
    harness.state_mut().state.import_tool_body_from(&head);
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, MILL), 1);

    // Explicit CSYS mate attaches piece 1 cleanly.
    {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.assembly.target_csys = "MCS".into();
        st.tool_body_ui.assembly.piece_csys = "CSW".into();
        st.attach_tool_body_piece_from(&unit);
    }
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, MILL), 2);

    // Unknown CSYS name refuses by name without changing the assembly.
    {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.assembly.target_csys = "NO_SUCH".into();
        st.tool_body_ui.assembly.piece_csys = "CSW".into();
        st.attach_tool_body_piece_from(&unit);
    }
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(pieces_len(st, MILL), 2);
    assert!(st.status.contains("piece not attached") && st.status.contains("NO_SUCH"), "{}", st.status);
}

#[test]
fn editor_selects_target_piece_when_three_pieces_repeat_csys_names() {
    let mut harness = editor_on(MILL);
    harness
        .state_mut()
        .state
        .import_tool_body_from(&fixture("tool_csys_box_iso13399.step"));
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let unit = fixture("modular_clamp_unit.step");
    {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.assembly.target_csys = "MCS".into();
        st.tool_body_ui.assembly.piece_csys = "CSW".into();
        st.attach_tool_body_piece_from(&unit);
    }
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, MILL), 2);
    // Both existing pieces have MCS. Searching the whole assembly must refuse.
    {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.assembly.target_csys = "MCS".into();
        st.tool_body_ui.assembly.piece_csys = "CSW".into();
        st.attach_tool_body_piece_from(&unit);
    }
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, MILL), 2);
    assert!(harness.state().state.status.contains("ambiguous"));
    // Select adapter 1 through the editor control, then attach the third piece.
    harness.get_by_label("Target piece").click();
    harness.run();
    harness.get_by_label("1: modular_clamp_unit.step").click();
    harness.run();
    assert_eq!(
        harness.state().state.tool_body_ui.assembly.target_piece,
        Some(1)
    );
    harness.state_mut().state.attach_tool_body_piece_from(&unit);
    harness.run();
    harness.run();
    assert_eq!(
        pieces_len(&harness.state().state, MILL),
        3,
        "{}",
        harness.state().state.status
    );
    assert!(harness.query_by_label("Assembly (3 piece(s))").is_some());
}

fn body(state: &State, tool: &str) -> camcraft_app::schema::ToolBody {
    let entity = model::find_by_name(&state.world, tool).expect("tool");
    tool_body::body_of(&state.world, &entity).expect("body")
}

fn assembly_editor(tool: &str) -> Harness<'static, ShellUi> {
    let mut harness = editor_on(tool);
    let head = if tool == MILL {
        "tool_csys_box_iso13399.step"
    } else {
        "modular_lathe_head.step"
    };
    harness
        .state_mut()
        .state
        .import_tool_body_from(&fixture(head));
    if tool == INSERT {
        harness
            .state_mut()
            .state
            .tool_body_ui
            .lathe
            .as_mut()
            .expect("lathe")
            .place = LathePlacement {
            radial: [1.0, 0.0, 0.0],
            along: [0.0, 0.0, 1.0],
            tip: [0.0; 3],
        };
    }
    harness.run();
    apply_body(&mut harness);
    {
        let st = &mut harness.state_mut().state;
        st.tool_body_ui.assembly.target_csys = "MCS".into();
        st.tool_body_ui.assembly.piece_csys = "CSW".into();
        st.attach_tool_body_piece_from(&fixture("modular_clamp_unit.step"));
    }
    harness.run();
    harness.run();
    assert_eq!(pieces_len(&harness.state().state, tool), 2);
    harness
}

fn apply_body(harness: &mut Harness<'static, ShellUi>) {
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
}

#[test]
fn editor_unchanged_apply_retains_modular_pieces_and_csys() {
    for tool in [MILL, INSERT] {
        let mut harness = assembly_editor(tool);
        let original = body(&harness.state().state, tool);
        assert!(original.pieces.iter().all(|p| !p.csys.is_empty()));
        apply_body(&mut harness);
        let applied = body(&harness.state().state, tool);
        assert_eq!(
            applied.pieces, original.pieces,
            "Apply body must retain modular pieces: {tool}"
        );
        assert_eq!(applied.facets, original.facets);
        // Separate pieces still support removal, and undo restores their records.
        let mut buttons: Vec<_> = harness.get_all_by_label("Remove").collect();
        buttons.swap_remove(1).click();
        harness.run();
        harness.run();
        assert_eq!(pieces_len(&harness.state().state, tool), 1);
        harness
            .state_mut()
            .state
            .pending
            .push_back(CommandKind::Undo);
        harness.run();
        harness.run();
        assert_eq!(body(&harness.state().state, tool).pieces, original.pieces);
        // A new import deliberately replaces, rather than reusing, the assembly.
        let replacement = if tool == MILL { "tool_csys_box_iso13399.step" } else { "modular_lathe_head.step" };
        harness.state_mut().state.import_tool_body_from(&fixture(replacement));
        harness.run();
        apply_body(&mut harness);
        assert_eq!(pieces_len(&harness.state().state, tool), 1);
    }
}

#[test]
fn editor_placement_apply_transforms_all_modular_pieces_and_csys() {
    use camcraft_app::tool_body_assembly::{transform_csys_lathe, transform_csys_spindle};
    for tool in [MILL, INSERT] {
        let mut harness = assembly_editor(tool);
        let original = body(&harness.state().state, tool);
        let mut expected = original.pieces.clone();
        if tool == MILL {
            let axis = [0.01, 0.0, 1.0];
            let offset = 5.0;
            harness.state_mut().state.tool_body_ui.axis = axis;
            harness.state_mut().state.tool_body_ui.offset = offset;
            for p in &mut expected {
                p.facets = tool_body::place(&p.facets, axis, offset)
                    .expect("placed piece")
                    .into();
                p.csys = transform_csys_spindle(&p.csys, axis, offset).expect("placed CSYS");
            }
        } else {
            let place = LathePlacement {
                radial: [0.0, 1.0, 0.0],
                along: [0.0, 0.0, 1.0],
                tip: [-3.0, 2.0, -5.0],
            };
            harness
                .state_mut()
                .state
                .tool_body_ui
                .lathe
                .as_mut()
                .expect("lathe")
                .place = place;
            for p in &mut expected {
                p.facets = camcraft_app::tool_body_lathe::place(&p.facets, &place)
                    .expect("placed piece")
                    .into();
                p.csys = transform_csys_lathe(&p.csys, &place).expect("placed CSYS");
            }
        }
        harness.run();
        apply_body(&mut harness);
        let applied = body(&harness.state().state, tool);
        assert_ne!(
            applied.facets,
            original.facets,
            "placement must be applied: {}",
            harness.state().state.status
        );
        assert_eq!(
            applied.pieces, expected,
            "all piece meshes and CSYS must move together: {tool}"
        );
        let combined: Vec<_> = applied
            .pieces
            .iter()
            .flat_map(|p| p.facets.iter().copied())
            .collect();
        assert_eq!(applied.facets.as_ref(), combined);
        harness
            .state_mut()
            .state
            .pending
            .push_back(CommandKind::Undo);
        harness.run();
        harness.run();
        assert_eq!(body(&harness.state().state, tool).pieces, original.pieces);
    }
}

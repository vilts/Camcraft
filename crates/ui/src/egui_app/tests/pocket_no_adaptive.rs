//! #1517: 2D Pocket's Strategy list drops Adaptive (moved to 2D Adaptive),
//! while 3D Pocket keeps Adaptive; migrated un-carriable fields show a
//! `needs review` badge/row in the browser and a `Needs review` + `Mark reviewed`
//! control on the operation page that clears via undoable command.
//!
//! Provenance: our own structural UI tests over the shipped `shell_layout`;
//! no pixel fixtures consulted.

use super::*;
use crate::operations::{strategy3d_names, strategy_names};
use crate::state::UiTarget;
use camcraft_app::operation::{KindInfo, OpType};
use egui::accesskit::Role;

fn kind(ty: OpType) -> usize {
    KindInfo::ALL.iter().position(|k| k.ty == ty).unwrap_or(usize::MAX)
}

#[test]
fn pocket_strategy_list_drops_adaptive_while_3d_keeps_it() {
    assert!(
        !strategy_names().contains(&"Adaptive"),
        "2D Pocket strategy list must drop Adaptive: {:?}",
        strategy_names()
    );
    assert!(
        strategy3d_names().contains(&"Adaptive"),
        "3D Pocket must keep Adaptive until #1591: {:?}",
        strategy3d_names()
    );

    let mut harness = shell_harness();
    let cur = harness.state().state.op_ui.params.clone();
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, kind(OpType::Pocket));
    let passes = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Passes").unwrap_or(0);
    harness.state_mut().state.op_tab = passes;
    harness.run();
    assert_eq!(
        harness
            .query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Adaptive"))
            .count(),
        0,
        "2D Pocket Strategy combo must never show Adaptive"
    );
}

#[test]
fn migrated_uncarriable_op_shows_needs_review_and_mark_reviewed_clears() {
    let v63 = r#"{
        "version": 63,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 24.0, "dy": 24.0, "dz": 20.0}}},
            {"name": "EM-0400-2FL", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 4.0, "shank_len": 35.0,
                      "holder_d": 32.0, "holder_len": 30.0,
                      "material_group": "general", "rpm": 12000.0, "feed": 800.0}},
            {"name": "P1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "EM-0400-2FL", "src": "stock", "params": {"pocket": {
                 "links": {"approach": {"predrilled": true}},
                 "stepover": 2.0,
                 "stock_to_leave": 0.0,
                 "clearance_z": 25.0,
                 "top_z": 20.0,
                 "bottom_z": 15.0,
                 "stepdown": 5.0,
                 "feed": 800.0,
                 "plunge_feed": 200.0,
                 "climb": true,
                 "strategy": "adaptive",
                 "entry": "helix",
                 "helix_radius": 1.5,
                 "optimal_load": 0.25
             }}}}
        ]
    }"#;
    let path = std::env::temp_dir().join(format!("camcraft_1517_ui_{}.camproj", std::process::id()));
    std::fs::write(&path, v63).expect("write v63 fixture");

    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::OpenProjectFile(path.clone()));
    harness.run_steps(2);
    let _ = std::fs::remove_file(&path);

    let (rows, leaves) = panes::browser_rows(&harness.state().state);
    let p1_ix = leaves
        .iter()
        .position(|l| matches!(l, panes::Leaf::Op(k) if k == "P1"))
        .expect("P1 row in browser");
    assert!(
        rows[p1_ix].suffix.contains("needs review"),
        "browser row suffix must advertise needs review: {:?}",
        rows[p1_ix].suffix
    );
    assert_eq!(rows[p1_ix].badge, Some(Icon::BadgeWarning));

    harness.state_mut().state.ui_target = UiTarget::Operation("P1".into());
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Mark reviewed")
            .is_some(),
        "operation page must show Mark reviewed action while needs_review is active"
    );

    harness.get_by_role_and_label(Role::Button, "Mark reviewed").click();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Mark reviewed")
            .is_none(),
        "Mark reviewed button clears after reviewing"
    );
    let (rows, leaves) = panes::browser_rows(&harness.state().state);
    let p1_ix = leaves
        .iter()
        .position(|l| matches!(l, panes::Leaf::Op(k) if k == "P1"))
        .expect("P1 row in browser");
    assert!(
        !rows[p1_ix].suffix.contains("needs review"),
        "browser row clears needs review: {:?}",
        rows[p1_ix].suffix
    );

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Mark reviewed")
            .is_some(),
        "Undo must restore needs review and the Mark reviewed action"
    );
}

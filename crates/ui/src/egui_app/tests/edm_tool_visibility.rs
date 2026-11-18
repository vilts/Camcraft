//! #2011: EDM tool visibility matches selectable machine kinds (Wire EDM has a
//! wire tool; Sinker Electrode stays hidden when Sinker EDM is not selectable).
//! Provenance: our own structural UI tests over the tool library and tool editor.
//! Gate map (#2011): covers ui::machine_modes, ui::egui_app::tool_library, ui::egui_app::tool_settings.

use camcraft_app::machine::{Kind as MachineKind, parse_mch};
use camcraft_app::model;
use camcraft_app::tools::ToolKind;
use egui::accesskit::Role;

use super::*;

const SINKER_MCH: &str = r#"{
  "general": { "model": "SYNTH SINKER", "vendor": "Camcraft", "kind": "sinker_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
      { "id": "x", "name": "X", "type": "linear", "parts": [ { "id": "y", "name": "Y", "type": "linear" } ] },
      { "id": "z", "name": "Z", "type": "linear", "parts": [ { "id": "head", "name": "ram", "type": "head" } ] }
    ] } ] } },
  "edm": { "conditions": [ { "id": "S1", "name": "synthetic", "gap": 0.05, "undersize": 0.1, "role": "rough" } ] }
}"#;

fn open_tool_library() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1200.0))
        .build_ui_state(frame, ShellUi::blank());
    h.run_steps(2);
    h.get_by_label("View").click();
    h.run_steps(2);
    h.get_by_label("Tool library…").click();
    h.run_steps(3);
    h
}

#[test]
fn sinker_electrode_is_hidden_when_sinker_edm_is_not_selectable_and_wire_is_offered() {
    let mut h = open_tool_library();
    let sinker_selectable = crate::machine_modes::MACHINE_MODES
        .iter()
        .any(|(k, _, _)| *k == MachineKind::SinkerEdm);
    assert_eq!(
        h.query_by_label_contains("EDM-E10X6  Ø").is_some(),
        sinker_selectable,
        "Sinker Electrode catalog entry must only be visible when Sinker EDM is selectable"
    );
    assert!(
        h.query_by_label_contains("EDM-W025  Ø").is_some(),
        "Wire EDM is selectable, so the wire catalog entry must be visible"
    );

    h.get_by_role_and_label(Role::ComboBox, "Tool type").click();
    h.run_steps(3);
    assert_eq!(
        h.query_by_label("sinker electrode").is_some(),
        sinker_selectable,
        "Catalog Tool type filter must hide sinker electrode unless Sinker EDM is selectable"
    );
    assert!(h.query_by_label("EDM wire").is_some(), "Catalog Tool type filter must list EDM wire");
    h.get_by_label("All types").click();
    h.run_steps(2);

    h.get_by_label("New tool…").click();
    h.run_steps(3);
    h.query_all_by_role(Role::ComboBox)
        .find(|n| n.value().as_deref() == Some("End mill"))
        .expect("Type combo")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.query_by_label("Sinker electrode").is_some(),
        sinker_selectable,
        "Sinker electrode type must only be offered when Sinker EDM is selectable"
    );
    assert!(
        h.query_by_label("EDM wire").is_some(),
        "EDM wire type must be offered when Wire EDM is selectable"
    );

    // Selecting "EDM wire" seeds wire defaults, hides spindle rpm / corner radius,
    // and adds a valid Wire tool to the document.
    h.get_by_label("EDM wire").click_accesskit();
    h.run_steps(3);
    assert_eq!(h.state().state.tool_ui.tool_kind(), ToolKind::Wire);
    assert!(h.query_by_label("Wire diameter").is_some(), "Wire diameter label shown");
    assert!(h.query_by_label("Spindle rpm").is_none(), "wire has no spindle rpm");
    assert!(h.query_by_label("Corner radius").is_none(), "wire has no corner radius");
    h.state_mut().state.tool_ui.name = "ShopWire025".into();
    h.run_steps(2);
    h.get_by_label("Add tool").click_accesskit();
    h.run_steps(3);
    let saved = model::find_by_name(&h.state().state.world, "ShopWire025")
        .and_then(|e| model::tool_of(&h.state().state.world, &e))
        .expect("wire tool added via UI");
    assert_eq!(saved.kind, ToolKind::Wire);
    assert_eq!((saved.diameter, saved.rpm, saved.corner_r), (0.25, 0.0, 0.0));
}

#[test]
fn sinker_electrode_is_offered_only_when_sinker_edm_is_selectable_or_mounted() {
    let without_sinker = [(MachineKind::Mill, "Milling", "VMC 3-axis")];
    assert!(!crate::machine_modes::tool_kind_offered_for(
        ToolKind::Electrode,
        &without_sinker,
        &[MachineKind::Mill],
    ));
    let with_sinker = [
        (MachineKind::Mill, "Milling", "VMC 3-axis"),
        (MachineKind::SinkerEdm, "Sinker EDM", "Sinker"),
    ];
    assert!(crate::machine_modes::tool_kind_offered_for(
        ToolKind::Electrode,
        &with_sinker,
        &[MachineKind::Mill],
    ));
    assert!(crate::machine_modes::tool_kind_offered_for(
        ToolKind::Electrode,
        &without_sinker,
        &[MachineKind::SinkerEdm],
    ));

    // Mounting a custom Sinker EDM in the UI makes EDM-E10X6 and "Sinker electrode" visible together.
    let mut h = open_tool_library();
    let m = parse_mch(SINKER_MCH).expect("sinker parses");
    h.state_mut()
        .state
        .pending
        .push_back(CommandKind::EditMachineApply { path: "sinker.mch".into(), machine: Box::new(m) });
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("EDM-E10X6  Ø").is_some(),
        "mounting a Sinker EDM must reveal EDM-E10X6 in the catalog"
    );
    h.get_by_label("New tool…").click();
    h.run_steps(3);
    h.query_all_by_role(Role::ComboBox)
        .find(|n| n.value().as_deref() == Some("End mill"))
        .expect("Type combo")
        .click();
    h.run_steps(3);
    assert!(
        h.query_by_label("Sinker electrode").is_some(),
        "mounting a Sinker EDM must offer Sinker electrode in New Tool"
    );
}

//! #1521 slice 1: a plasma torch's cut-data recipes, keyed by material and thickness.
//! Provenance: synthetic values testing the issue's stated data rules; no cut chart was copied.
//! Gate map (#1521): covers tools::plasma_cut, tools::cutting_rows, tools::tool_sanity.

use camcraft_tools::cutting::WorkMaterial;
use camcraft_tools::cutting_rows::{CuttingRow, plasma_row_for, refusal_for_kind, row_for, rows_with_unknown, unknown_of};
use camcraft_tools::plasma_cut::{PlasmaCutData, PuddleJump};
use camcraft_tools::tools::{ToolKind, torch_tool};

fn recipe() -> PlasmaCutData {
    PlasmaCutData {
        thickness: 6.0,
        kerf: 1.5,
        cut_height: Some(1.5),
        pierce_height: Some(3.8),
        pierce_delay: Some(0.5),
        transfer_height: Some(2.5),
        puddle_jump: Some(PuddleJump { percent: 50.0, dwell: 0.2 }),
        amperage: Some(45.0),
        arc_voltage: Some(128.0),
        max_pierce_thickness: Some(12.0),
        edge_start: false,
    }
}

fn row() -> CuttingRow {
    CuttingRow { plasma: Some(recipe()), ..CuttingRow::plain(Some(WorkMaterial::MildSteel), 0.0, 2400.0) }
}

fn with(edit: impl FnOnce(&mut PlasmaCutData)) -> CuttingRow {
    let mut p = recipe();
    edit(&mut p);
    CuttingRow { plasma: Some(p), ..row() }
}

fn why(rows: &[CuttingRow]) -> String {
    refusal_for_kind(rows, ToolKind::Torch).expect("refused")
}

#[test]
fn plasma_recipe_is_kept_on_a_torch_and_refused_on_every_other_kind() {
    assert_eq!(refusal_for_kind(&[row()], ToolKind::Torch), None);
    let mut torch = torch_tool(1.5);
    torch.cutting = vec![row()];
    assert!(torch.is_sane(), "{:?}", torch.why_not_sane());
    for kind in [ToolKind::EndMill, ToolKind::Electrode, ToolKind::Waterjet, ToolKind::Wire] {
        let said = refusal_for_kind(&[row()], kind).expect("refused");
        if kind != ToolKind::Wire {
            assert!(said.contains("requires a plasma torch"), "{kind:?}: {said}");
        }
    }
    let mut mill = camcraft_tools::tools::catalog().remove(0).1;
    mill.cutting = vec![row()];
    assert!(!mill.is_sane(), "the tool door refuses a plasma recipe on a mill");
}

#[test]
fn plasma_recipe_refuses_bad_numbers_by_name() {
    for bad in [-1.0, 0.0, f64::NAN, f64::INFINITY] {
        assert!(why(&[with(|p| p.thickness = bad)]).contains("thickness"), "{bad}");
        assert!(why(&[with(|p| p.kerf = bad)]).contains("kerf"), "{bad}");
        assert!(why(&[with(|p| p.amperage = Some(bad))]).contains("amperage"), "{bad}");
        assert!(why(&[with(|p| p.arc_voltage = Some(bad))]).contains("arc voltage"), "{bad}");
        assert!(why(&[with(|p| p.max_pierce_thickness = Some(bad))]).contains("pierce capacity"), "{bad}");
        assert!(why(&[with(|p| p.puddle_jump = Some(PuddleJump { percent: bad, dwell: 0.2 }))]).contains("puddle jump percent"), "{bad}");
    }
    for bad in [-0.1, f64::NAN, f64::INFINITY] {
        assert!(why(&[with(|p| p.cut_height = Some(bad))]).contains("cut height"), "{bad}");
        assert!(why(&[with(|p| p.pierce_height = Some(bad))]).contains("pierce height"), "{bad}");
        assert!(why(&[with(|p| p.transfer_height = Some(bad))]).contains("transfer height"), "{bad}");
        assert!(why(&[with(|p| p.pierce_delay = Some(bad))]).contains("pierce delay"), "{bad}");
        assert!(why(&[with(|p| p.puddle_jump = Some(PuddleJump { percent: 50.0, dwell: bad }))]).contains("puddle jump dwell"), "{bad}");
    }
    let zero = with(|p| {
        p.cut_height = Some(0.0);
        p.pierce_height = Some(0.0);
        p.transfer_height = Some(0.0);
        p.pierce_delay = Some(0.0);
        p.puddle_jump = Some(PuddleJump { percent: 50.0, dwell: 0.0 });
    });
    assert_eq!(refusal_for_kind(&[zero], ToolKind::Torch), None, "zero heights and times are legal, as on the operation");
}

#[test]
fn plasma_recipe_left_blank_means_the_controller_and_needs_no_default() {
    let blank = with(|p| {
        *p = PlasmaCutData {
            thickness: 6.0, kerf: 1.5, cut_height: None, pierce_height: None, pierce_delay: None, transfer_height: None,
            puddle_jump: None, amperage: None, arc_voltage: None, max_pierce_thickness: None, edge_start: false,
        };
    });
    assert_eq!(refusal_for_kind(&[blank], ToolKind::Torch), None);
    // A puddle jump is a percentage of the pierce height: without one it has no meaning.
    let said = why(&[with(|p| p.pierce_height = None)]);
    assert!(said.contains("puddle jump") && said.contains("pierce height"), "{said}");
}

#[test]
fn plasma_recipe_thicker_than_the_pierce_capacity_needs_an_edge_start() {
    let said = why(&[with(|p| p.thickness = 16.0)]);
    assert!(said.contains("Mild steel") && said.contains("16") && said.contains("12") && said.contains("edge start"), "{said}");
    let edge = with(|p| {
        p.thickness = 16.0;
        p.edge_start = true;
    });
    assert_eq!(refusal_for_kind(&[edge], ToolKind::Torch), None, "an edge-start recipe may exceed the pierce capacity");
    assert_eq!(refusal_for_kind(&[with(|p| p.thickness = 12.0)], ToolKind::Torch), None, "the capacity itself is pierceable");
}

#[test]
fn plasma_recipe_rows_state_material_zero_rpm_feed_and_a_unique_key() {
    assert!(why(&[CuttingRow { material: None, ..row() }]).contains("names its workpiece material"));
    assert!(why(&[CuttingRow { rpm: 100.0, ..row() }]).contains("rpm"));
    for feed in [-1.0, 0.0, f64::NAN, f64::INFINITY] {
        assert!(why(&[CuttingRow { feed, ..row() }]).contains("feed"), "{feed}");
    }
    let said = why(&[row(), row()]);
    assert!(said.contains("Mild steel") && said.contains("twice"), "{said}");
    let thick = with(|p| p.thickness = 10.0);
    let brass = CuttingRow { material: Some(WorkMaterial::Brass), ..row() };
    assert_eq!(refusal_for_kind(&[row(), thick, brass], ToolKind::Torch), None, "one material, two thicknesses");
}

#[test]
fn plasma_recipe_is_picked_by_material_and_exact_thickness_only() {
    let thick = with(|p| p.thickness = 10.0);
    let plain = CuttingRow::plain(Some(WorkMaterial::MildSteel), 0.0, 3000.0);
    let rows = [row(), thick, plain];
    assert_eq!(refusal_for_kind(&rows, ToolKind::Torch), None, "a recipe does not count as the material's plain row");
    assert_eq!(plasma_row_for(&rows, WorkMaterial::MildSteel, 10.0), Some(&thick));
    assert_eq!(plasma_row_for(&rows, WorkMaterial::MildSteel, 6.0), Some(&row()));
    assert!(plasma_row_for(&rows, WorkMaterial::MildSteel, 8.0).is_none(), "no nearest-thickness substitution");
    assert!(plasma_row_for(&rows, WorkMaterial::Brass, 6.0).is_none());
    assert!(plasma_row_for(&rows, WorkMaterial::MildSteel, f64::NAN).is_none());
    // Selection by material alone never lands on a recipe.
    assert_eq!(row_for(&rows, Some(WorkMaterial::MildSteel)), Some(&plain));
    assert!(row_for(&[row()], Some(WorkMaterial::MildSteel)).is_none());
    assert!(refusal_for_kind(&[plain, plain], ToolKind::Torch).unwrap().contains("twice"), "plain rows keep their own law");
}

#[test]
fn plasma_recipe_never_stands_in_for_the_unknown_row() {
    let mut torch = torch_tool(1.5);
    torch.cutting = vec![row()];
    assert!(!torch.cutting[0].is_unknown());
    assert!(!CuttingRow { material: None, ..row() }.is_unknown(), "a recipe that lost its material is not the Unknown row");
    let unknown = unknown_of(&torch).expect("a torch keeps its derived Unknown row");
    assert_eq!(unknown, CuttingRow::plain(None, torch.rpm, torch.feed));
    assert_eq!(rows_with_unknown(&torch).len(), 2);
}

#[test]
fn plasma_recipe_file_form_omits_blanks_and_refuses_unknown_keys() {
    let text = serde_json::to_value(recipe()).expect("writes");
    assert_eq!(text["puddle_jump"]["percent"], 50.0);
    assert_eq!(serde_json::from_value::<PlasmaCutData>(text.clone()).expect("reads"), recipe());
    let blank: PlasmaCutData = serde_json::from_str(r#"{"thickness":6.0,"kerf":1.5}"#).expect("blank values are optional");
    assert_eq!((blank.pierce_height, blank.puddle_jump, blank.edge_start), (None, None, false));
    assert_eq!(serde_json::to_string(&blank).expect("writes"), r#"{"thickness":6.0,"kerf":1.5}"#);
    let mut typo = text;
    typo["peirce_height"] = serde_json::json!(3.0);
    assert!(serde_json::from_value::<PlasmaCutData>(typo).is_err(), "a misspelt key is refused, never dropped");
}

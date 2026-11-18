//! #1881: waterjet nozzle and material/thickness recipe validation.
//! Provenance: synthetic values testing the issue's explicit data rules.
//! Gate map (#1881): covers tools::waterjet, tools::cutting_rows, tools::tools.

use camcraft_tools::cutting::WorkMaterial;
use camcraft_tools::cutting_rows::{CuttingRow, refusal_for_kind, row_for, waterjet_row_for};
use camcraft_tools::tools::ToolKind;
use camcraft_tools::waterjet::{PierceMethod, WaterjetCutData, waterjet_tool};

fn recipe() -> WaterjetCutData {
    WaterjetCutData { thickness: 4.0, kerf: 0.8, pierce_time: 0.2, abrasive_delay: 0.1,
        re_pierce_time: 0.3, pierce_method: PierceMethod::Stationary }
}

fn row() -> CuttingRow {
    CuttingRow { material: Some(WorkMaterial::MildSteel), rpm: 0.0, feed: 100.0, stepdown: None, stepover: None, waterjet: Some(recipe()) }
}

#[test]
fn waterjet_nozzle_requires_zero_rpm_and_positive_finite_kerf() {
    let mill = camcraft_tools::tools::catalog().remove(0).1;
    assert!(mill.rpm > 0.0);
    assert!(camcraft_tools::waterjet::refusal(&mill).is_none(), "waterjet rules leave other tool kinds alone");
    let tool = waterjet_tool(0.8);
    assert_eq!(tool.kind, ToolKind::Waterjet);
    assert_eq!(tool.rpm, 0.0);
    assert_eq!(tool.geom().radius, 0.4);
    assert!(tool.is_sane());
    assert!(tool.torch_refusal().is_none());
    for rpm in [-1.0, 1.0, f64::NAN, f64::INFINITY] {
        let mut bad = tool.clone();
        bad.rpm = rpm;
        assert!(bad.torch_refusal().unwrap().contains("rpm"));
        assert!(!bad.is_sane());
    }
    for kerf in [-1.0, 0.0, f64::NAN, f64::INFINITY] {
        let bad = waterjet_tool(kerf);
        assert!(bad.torch_refusal().unwrap().contains("kerf"));
        assert!(!bad.is_sane());
    }
    let mut bad = tool;
    bad.corner_r = 0.1;
    assert!(bad.torch_refusal().unwrap().contains("corner_r"));
    assert!(!bad.is_sane());
}

#[test]
fn waterjet_recipe_rejects_bad_dimensions_and_delays_by_name() {
    for key in ["thickness", "kerf", "pierce_time", "abrasive_delay", "re_pierce_time"] {
        for value in [-1.0, f64::NAN, f64::INFINITY] {
            let mut w = recipe();
            match key {
                "thickness" => w.thickness = value,
                "kerf" => w.kerf = value,
                "pierce_time" => w.pierce_time = value,
                "abrasive_delay" => w.abrasive_delay = value,
                _ => w.re_pierce_time = value,
            }
            assert!(w.refusal().unwrap().contains(key), "{key} = {value}");
            assert!(refusal_for_kind(&[CuttingRow { waterjet: Some(w), ..row() }], ToolKind::Waterjet).unwrap().contains(key));
        }
    }
    for w in [WaterjetCutData { thickness: 0.0, ..recipe() }, WaterjetCutData { kerf: 0.0, ..recipe() }] {
        assert!(w.refusal().is_some());
    }
    let zero_delays = WaterjetCutData { pierce_time: 0.0, abrasive_delay: 0.0, re_pierce_time: 0.0, ..recipe() };
    assert!(zero_delays.refusal().is_none());
}

#[test]
fn waterjet_rows_require_complete_recipe_zero_rpm_and_valid_feed() {
    assert!(refusal_for_kind(&[row()], ToolKind::Waterjet).is_none());
    assert!(refusal_for_kind(&[CuttingRow { waterjet: None, ..row() }], ToolKind::Waterjet).unwrap().contains("requires thickness"));
    for rpm in [-1.0, 1.0, f64::NAN, f64::INFINITY] {
        assert!(refusal_for_kind(&[CuttingRow { rpm, ..row() }], ToolKind::Waterjet).unwrap().contains("rpm"));
    }
    for feed in [-1.0, 0.0, f64::NAN, f64::INFINITY] {
        assert!(refusal_for_kind(&[CuttingRow { feed, ..row() }], ToolKind::Waterjet).unwrap().contains("feed"));
    }
    let mut tool = waterjet_tool(0.8);
    tool.cutting = vec![CuttingRow { feed: 0.0, ..row() }];
    assert!(tool.torch_refusal().unwrap().contains("feed"));
    assert!(!tool.is_sane());
    for kind in [ToolKind::EndMill, ToolKind::Torch, ToolKind::Electrode] {
        assert!(refusal_for_kind(&[row()], kind).unwrap().contains("requires a waterjet tool"));
    }
    let mut mill = camcraft_tools::tools::catalog().remove(0).1;
    assert!(mill.is_sane());
    mill.cutting = vec![CuttingRow { rpm: 1000.0, ..row() }];
    assert!(!mill.is_sane(), "the generic tool door refuses waterjet data on a mill");
}

#[test]
fn waterjet_recipe_selection_requires_material_and_exact_thickness() {
    let thin = row();
    let thick = CuttingRow { waterjet: Some(WaterjetCutData { thickness: 8.0, ..recipe() }), feed: 50.0, ..row() };
    let brass = CuttingRow { material: Some(WorkMaterial::Brass), ..row() };
    let rows = [thin, thick, brass];
    assert!(refusal_for_kind(&rows, ToolKind::Waterjet).is_none());
    assert!(refusal_for_kind(&[thin, thin], ToolKind::Waterjet).unwrap().contains("twice"));
    assert_eq!(waterjet_row_for(&rows, WorkMaterial::MildSteel, 8.0), Some(&thick));
    assert_eq!(waterjet_row_for(&rows, WorkMaterial::Brass, 4.0), Some(&brass));
    assert!(waterjet_row_for(&rows, WorkMaterial::Brass, 8.0).is_none());
    assert!(waterjet_row_for(&rows, WorkMaterial::MildSteel, 4.1).is_none());
    assert!(waterjet_row_for(&rows, WorkMaterial::MildSteel, f64::NAN).is_none());
    assert!(row_for(&rows, Some(WorkMaterial::MildSteel)).is_none());
    let conventional = CuttingRow { waterjet: None, rpm: 1000.0, ..row() };
    assert_eq!(row_for(&[thin, conventional], Some(WorkMaterial::MildSteel)), Some(&conventional));
    assert!(refusal_for_kind(&[conventional], ToolKind::EndMill).is_none());
    assert!(refusal_for_kind(&[CuttingRow { rpm: 0.0, ..conventional }], ToolKind::EndMill).is_some());
}

#[test]
fn waterjet_pierce_method_and_recipe_roundtrip_without_guessing() {
    for (word, method) in [("stationary", PierceMethod::Stationary), ("dynamic", PierceMethod::Dynamic), ("wiggle", PierceMethod::Wiggle)] {
        let w = WaterjetCutData { pierce_method: method, ..recipe() };
        let v = serde_json::to_value(w).unwrap();
        assert_eq!(v["pierce_method"], word);
        assert_eq!(serde_json::from_value::<WaterjetCutData>(v).unwrap(), w);
    }
    let mut bad = serde_json::to_value(recipe()).unwrap();
    bad["pierce_method"] = "unknown".into();
    assert!(serde_json::from_value::<WaterjetCutData>(bad).is_err());
    let mut bad = serde_json::to_value(recipe()).unwrap();
    bad["unknown"] = 1.into();
    assert!(serde_json::from_value::<WaterjetCutData>(bad).is_err());
}

#[test]
fn waterjet_catalog_kind_parses_as_a_non_spinning_nozzle() {
    let mut catalog: serde_json::Value = serde_json::from_str(include_str!("../../../assets/tools/catalog.json")).unwrap();
    let rows = catalog["tools"].as_array_mut().unwrap();
    rows.truncate(1);
    rows[0]["name"] = "waterjet test".into();
    rows[0]["definition"]["kind"] = "waterjet".into();
    rows[0]["definition"]["diameter"] = 0.8.into();
    rows[0]["definition"]["rpm"] = 0.0.into();
    let tools = camcraft_tools::catalog_asset::parse_catalog(&catalog.to_string()).expect("a waterjet nozzle catalog reads");
    assert_eq!(tools[0].1.kind.to_string(), "waterjet nozzle");
    assert_eq!(tools[0].1.rpm, 0.0);
    assert!(tools[0].1.is_sane());
}

//! #890 / #2015 / #1521 / #2681: the tests of the tool editor's cutting
//! rows, moved out of `tool_cutting_rows.rs` (#3896) so that the widget
//! file stays short. Provenance: our own unit tests; no HSMWorks material
//! involved.

use super::add_row;
use crate::state::ToolUi;
use camcraft_app::tools::WorkMaterial;

#[test]
fn rows_add_per_material_and_ride_the_draft_both_ways() {
    let mut d = ToolUi { rpm: 9000.0, feed: 1200.0, ..ToolUi::default() };
    assert!(add_row(&mut d));
    assert!(add_row(&mut d));
    assert_eq!(d.cutting[0].material, Some(WorkMaterial::ALL[0]), "#2015: Unknown is shown, never added");
    assert_ne!(d.cutting[1].material, d.cutting[0].material, "a new row takes an unused material");
    assert_eq!((d.cutting[0].rpm, d.cutting[0].feed), (9000.0, 1200.0));
    d.cutting[1].rpm = 3000.0;
    let back = ToolUi::from_tool("T", &d.to_tool(), true);
    assert_eq!(back.cutting, d.cutting);
    while add_row(&mut d) {}
    assert_eq!(d.cutting.len(), WorkMaterial::ALL.len(), "one row per material, no more");
}

#[test]
fn waterjet_rows_require_a_thickness_and_preserve_every_recipe_field() {
    use camcraft_app::waterjet_tools::{PierceMethod, waterjet_tool};
    let mut d = ToolUi::from_tool("Nozzle", &waterjet_tool(0.8), true);
    assert!(add_row(&mut d));
    assert!(d.to_tool().torch_refusal().is_some_and(|s| s.contains("thickness")));
    d.cutting[0].material = Some(WorkMaterial::MildSteel);
    d.cutting[0].feed = 700.0;
    let w = d.cutting[0].waterjet.as_mut().expect("waterjet row");
    w.thickness = 5.0;
    w.kerf = 0.9;
    w.pierce_time = 2.0;
    w.abrasive_delay = 0.3;
    w.re_pierce_time = 1.2;
    w.pierce_method = PierceMethod::Dynamic;
    assert!(d.to_tool().is_sane());
    let first = d.cutting[0];
    assert!(add_row(&mut d));
    d.cutting[1] = first;
    d.cutting[1].waterjet.as_mut().unwrap().thickness = 10.0;
    let saved = d.to_tool();
    assert!(saved.is_sane());
    assert_eq!(saved.rpm, 0.0);
    assert_eq!(saved.kind, camcraft_app::tools::ToolKind::Waterjet);
    assert_eq!(ToolUi::from_tool("Nozzle", &saved, true).to_tool(), saved);
    assert_eq!(d.cutting[0], first, "adding another thickness keeps the first recipe");
    for _ in 0..WorkMaterial::ALL.len() { assert!(add_row(&mut d)); }
    assert!(d.cutting.len() > WorkMaterial::ALL.len(), "waterjet rows are not capped at one per material");
}

/// #1521: a torch's thickness recipe is not a speed-and-feed row. The
/// table does not draw it, a material row may stand beside it, and it
/// rides the draft untouched.
#[test]
fn a_torch_recipe_rides_the_draft_and_is_not_drawn_as_a_material_row() {
    use camcraft_app::plasma_cut_tools::PlasmaCutData;
    use camcraft_app::tool_cutting_rows::CuttingRow;
    use egui_kittest::kittest::Queryable;
    let recipe = CuttingRow {
        plasma: Some(PlasmaCutData {
            thickness: 6.0, kerf: 1.5, cut_height: Some(1.5), pierce_height: Some(3.8), pierce_delay: Some(0.5),
            transfer_height: None, puddle_jump: None, amperage: None, arc_voltage: None,
            max_pierce_thickness: Some(12.0), edge_start: false,
        }),
        ..CuttingRow::plain(Some(WorkMaterial::ALL[0]), 0.0, 2400.0)
    };
    let mut torch = camcraft_app::tools::torch_tool(1.5);
    torch.cutting = vec![recipe];
    let mut d = ToolUi::from_tool("Torch", &torch, true);
    assert_eq!(d.to_tool(), torch, "the draft carries the recipe both ways");
    let count = |d: &ToolUi| {
        let mut d = d.clone();
        let mut h = egui_kittest::Harness::new_ui(move |ui| {
            super::cutting_rows(ui, &mut d, camcraft_app::units::Unit::Millimeters, true);
        });
        h.run();
        h.query_all_by_label("Remove").count()
    };
    assert_eq!(count(&d), 0, "a recipe is not drawn as a removable material row");
    assert!(add_row(&mut d));
    assert_eq!(d.cutting[1], CuttingRow::plain(Some(WorkMaterial::ALL[0]), d.rpm, d.feed), "the recipe's material is still free for a plain row");
    assert_eq!(d.cutting[0], recipe);
    assert_eq!(count(&d), 1, "only the plain material row is drawn");
    assert!(d.to_tool().is_sane(), "{:?}", d.to_tool().why_not_sane());
}

/// #2681: thickness recipes do not occupy the plain material-row slots.
#[test]
fn torch_material_row_button_counts_plain_rows_only() {
    use camcraft_app::plasma_cut_tools::PlasmaCutData;
    use camcraft_app::tool_cutting_rows::CuttingRow;
    use egui_kittest::kittest::{NodeT, Queryable};

    for plain_rows in [false, true] {
        let mut torch = camcraft_app::tools::torch_tool(1.5);
        torch.cutting = WorkMaterial::ALL.into_iter().map(|material| CuttingRow {
            plasma: Some(PlasmaCutData {
                thickness: 6.0, kerf: 1.5, cut_height: Some(1.5), pierce_height: Some(3.8), pierce_delay: Some(0.5),
                transfer_height: None, puddle_jump: None, amperage: None, arc_voltage: None,
                max_pierce_thickness: Some(12.0), edge_start: false,
            }),
            ..CuttingRow::plain(Some(material), 0.0, 2400.0)
        }).collect();
        if plain_rows {
            torch.cutting.extend(WorkMaterial::ALL.into_iter().map(|material| {
                CuttingRow::plain(Some(material), 0.0, 2400.0)
            }));
        }
        assert!(torch.is_sane(), "{:?}", torch.why_not_sane());
        let mut d = ToolUi::from_tool("Torch", &torch, true);
        let mut h = egui_kittest::Harness::new_ui(move |ui| {
            super::cutting_rows(ui, &mut d, camcraft_app::units::Unit::Millimeters, true);
        });
        h.run();
        assert_eq!(
            h.get_by_label("Add material row").accesskit_node().is_disabled(),
            plain_rows,
            "only plain rows for every material disable Add material row"
        );
    }
}

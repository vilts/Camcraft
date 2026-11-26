//! Independent DXF profiles keep their resolved contour sides when selected together.
//! Gate map (M4.4): covers app::operation (contour_sketch, contour_sketch_pick).
//! Provenance: original commands and public ASCII DXF circle group codes.

use camcraft_app::commands::{
    AddTool, CreateOperation, DxfOptions, GenerateOperation, History, ImportDxf, SetStock,
};
use camcraft_app::machining2d::Lead;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef,
};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

#[test]
fn separate_nested_profiles_keep_both_outside_contours() {
    let dir =
        std::env::temp_dir().join(format!("camcraft-contour-profiles-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 120.0,
                dy: 120.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .unwrap(),
    );
    let tool_name = "EM-0400-2FL";
    let tool = catalog()
        .into_iter()
        .find(|(name, _)| *name == tool_name)
        .unwrap()
        .1;
    history.execute(&world, AddTool::new(&world, tool_name, tool).unwrap());
    for (name, radius) in [("outer", 20), ("inner", 8)] {
        let path = dir.join(format!("{name}.dxf"));
        std::fs::write(&path, format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\ncut\n10\n50\n20\n50\n40\n{radius}\n0\nENDSEC\n0\nEOF\n")).unwrap();
        history.execute(
            &world,
            ImportDxf::new(&world, None, &path, &DxfOptions::default()).unwrap(),
        );
    }
    let mut params = operation::seed_params(OpType::Contour.ordinal());
    if let OpParams::Contour {
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown,
        lead,
        tolerance,
        ..
    } = &mut params
    {
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *retract_z = Height::of(HeightReference::TopOfStock, 5.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
        *stepdown = 2.0;
        *lead = Lead::None;
        *tolerance = 0.005;
    }
    for (op_name, names, expected) in [
        ("InnerOnly", vec!["inner"], vec![10.0]),
        ("Both", vec!["outer", "inner"], vec![10.0, 22.0]),
    ] {
        let picks = names
            .into_iter()
            .map(|name| GeoSel {
                entity: name.into(),
                feature: FeatureSel::SketchLayer("cut".into()),
            })
            .collect();
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                op_name,
                tool_name,
                SrcRef::Selection(picks),
                params.clone(),
            )
            .unwrap(),
        );
        history.execute(&world, GenerateOperation::new(&world, op_name).unwrap());
        let entity = model::find_by_name(&world, op_name).unwrap();
        let cache = model::toolpath_of(&world, &entity).unwrap();
        let radii: Vec<_> = cache
            .path
            .moves
            .iter()
            .filter_map(|m| m.arc.map(|arc| arc.radius_sweep(m.from, m.to).0))
            .collect();
        assert!(!radii.is_empty(), "{op_name} must contain cutting arcs");
        for wanted in &expected {
            assert!(
                radii.iter().any(|r| (r - wanted).abs() < 0.02),
                "{op_name} needs outside radius {wanted}, got {radii:?}"
            );
        }
        assert!(
            radii
                .iter()
                .all(|r| expected.iter().any(|wanted| (r - wanted).abs() < 0.02)),
            "{op_name} must not reverse a profile's resolved side: {radii:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

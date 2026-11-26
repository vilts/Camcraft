//! Provenance: independent corner-relief reachability and boundary refusal tests.
//! Gate map (#2400): covers app::operation::corner_relief (reachability and boundary refusals),
//! app::commands (CreateOperation, ImportModel).

mod corner_relief_support;
mod pocket_support;

use camcraft_app::commands::{CreateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ExtraBoundary, FeatureSel, GeoSel, Height,
    HeightReference, OpParams, SrcRef,
};
use camcraft_app::recognize;
use corner_relief_support::{TOOL, base_pocket, patch_pocket, setup_world};
use pocket_support::{block_with_pocket, height_field, rounded_rect};
use serde_json::json;

#[test]
fn unreachable_corner_in_narrow_keyway_is_refused_by_name() {
    // 20 x 20 main pocket with a 4 mm wide rectangular keyway [10, 18] x [-2, 2].
    // The 6 mm cutter (radius 3) cannot fit inside the 4 mm keyway to reach corners (18, +/-2).
    let mesh = height_field(
        &[-25.0, -10.0, 10.0, 18.0, 25.0],
        &[-20.0, -10.0, -2.0, 2.0, 10.0, 20.0],
        |i, j| if (i == 1 && (1..=3).contains(&j)) || (i == 2 && j == 2) { 10.0 } else { 20.0 },
    );
    let (world, mut h) = setup_world(Some(mesh), 50.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    assert_eq!(rec.len(), 1);
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op");
    let params = patch_pocket(&base_op, json!({ "corner_relief": { "kind": "dogbone" } }))
        .expect("corner_relief param supported");
    h.execute(&world, CreateOperation::new(&world, "NarrowKeyway", TOOL, src, params).expect("create"));
    let op = model::operation_of(&world, &model::find_by_name(&world, "NarrowKeyway").unwrap()).unwrap();
    let err = camcraft_app::operation::generate(&world, &op, "NarrowKeyway")
        .expect_err("unreachable corner must be refused")
        .to_string();
    assert!(
        err.to_lowercase().contains("corner relief") && err.to_lowercase().contains("reach"),
        "refusal must name corner relief reachability: {err}"
    );
}

#[test]
fn relief_exiting_stock_or_thin_wall_or_exclude_is_refused_by_name() {
    // Case 1: thin 1 mm outer part wall (22 x 22 block with 20 x 20 pocket): a T-bone pokes 3 mm outside the part!
    let thin_mesh = block_with_pocket(11.0, 11.0, &rounded_rect(10.0, 10.0, 0.0, 1), 10.0, 20.0);
    let (w1, mut h1) = setup_world(Some(thin_mesh), 40.0, 40.0, 20.0);
    let rec1 = recognize::pockets_of(&w1, "part").expect("recognize");
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src1, base_op1) = recognize::operation_for(&rec1[0], &base).expect("op");
    let tbone = patch_pocket(
        &base_op1,
        json!({ "corner_relief": { "kind": "t_bone", "tbone_direction": "horizontal" } }),
    )
    .expect("t_bone supported");
    h1.execute(&w1, CreateOperation::new(&w1, "ThinWall", TOOL, src1, tbone).expect("create"));
    let op1 = model::operation_of(&w1, &model::find_by_name(&w1, "ThinWall").unwrap()).unwrap();
    let err1 = camcraft_app::operation::generate(&w1, &op1, "ThinWall")
        .expect_err("relief breaking out of thin part wall must refuse")
        .to_string();
    assert!(err1.to_lowercase().contains("corner relief"), "must name corner relief: {err1}");

    // Case 2: pocket 0.5 mm from stock edge (40 x 40 stock around a 39 x 39 pocket boundary via Include)
    let (w2, mut h2) = setup_world(None, 40.0, 40.0, 20.0);
    let plate = cube(Point3::new(-19.5, -19.5, 0.0), Point3::new(19.5, 19.5, 10.0));
    h2.execute(&w2, ImportModel::from_mesh("pocket_box", plate));
    let top_face = model::features_of(&w2, &model::find_by_name(&w2, "pocket_box").unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .expect("top face")
        .id;
    let src2 = SrcRef::Selection(vec![GeoSel { entity: "pocket_box".into(), feature: FeatureSel::Face(top_face) }]);
    let p2 = patch_pocket(
        &base_pocket(PocketStrategy::Waterline, Height::fixed(20.0), Height::fixed(10.0), 10.0),
        json!({ "corner_relief": { "kind": "dogbone" } }),
    )
    .expect("dogbone supported");
    h2.execute(&w2, CreateOperation::new(&w2, "OutStock", TOOL, src2.clone(), p2.clone()).expect("create"));
    let op2 = model::operation_of(&w2, &model::find_by_name(&w2, "OutStock").unwrap()).unwrap();
    let err2 = camcraft_app::operation::generate(&w2, &op2, "OutStock")
        .expect_err("relief exiting stock must refuse")
        .to_string();
    assert!(err2.to_lowercase().contains("stock"), "must name stock boundary: {err2}");

    // Case 3: Exclude boundary standing 0.2 mm past corner (10, 10) inside 60 x 40 stock
    let (w3, mut h3) = setup_world(None, 60.0, 40.0, 20.0);
    let pocket_slab = cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    let guard_slab = cube(Point3::new(10.2, 8.0, 0.0), Point3::new(18.0, 18.0, 5.0));
    h3.execute(&w3, ImportModel::from_mesh("area_box", pocket_slab));
    h3.execute(&w3, ImportModel::from_mesh("guard_box", guard_slab));
    let f_area = model::features_of(&w3, &model::find_by_name(&w3, "area_box").unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .unwrap()
        .id;
    let f_guard = model::features_of(&w3, &model::find_by_name(&w3, "guard_box").unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .unwrap()
        .id;
    h3.execute(&w3, CreateOperation::new(&w3, "ExclCorner", TOOL, SrcRef::Selection(vec![GeoSel { entity: "area_box".into(), feature: FeatureSel::Face(f_area) }]), p2).expect("create"));
    let e3 = model::find_by_name(&w3, "ExclCorner").unwrap();
    let area = AreaControls {
        extra: vec![ExtraBoundary {
            mode: BoundaryMode::Exclude,
            shape: BoundaryShape::Picks(vec![GeoSel { entity: "guard_box".into(), feature: FeatureSel::Face(f_guard) }]),
        }],
        ..Default::default()
    };
    h3.execute(&w3, camcraft_app::commands::EditOperationParams::with_area(&w3, "ExclCorner", model::operation_of(&w3, &e3).unwrap().params, area).expect("edit"));
    let op3 = model::operation_of(&w3, &model::find_by_name(&w3, "ExclCorner").unwrap()).unwrap();
    let err3 = camcraft_app::operation::generate(&w3, &op3, "ExclCorner")
        .expect_err("relief entering Exclude boundary must refuse")
        .to_string();
    assert!(
        err3.to_lowercase().contains("exclude") || err3.to_lowercase().contains("corner relief"),
        "must name Exclude obstruction: {err3}"
    );
}

#[test]
fn adaptive2d_spiral_and_offset2d_support_relief_while_trochoidal_and_do_not_touch_refuse() {
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(10.0, 10.0, 0.0, 1), 10.0, 20.0);
    let (world, _h) = setup_world(Some(mesh), 60.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op");

    // Retype Pocket -> Adaptive2D (Spiral) and apply Dogbone corner relief.
    let adp_base = camcraft_app::operation::retype_params(
        &base_op,
        camcraft_app::operation::OpType::Adaptive2D.ordinal(),
    );
    let mut adp_dog = adp_base.clone();
    if let OpParams::Adaptive2D { corner_relief, .. } = &mut adp_dog {
        corner_relief.kind = camcraft_app::machining2d::CornerReliefKind::Dogbone;
    }
    let adp_op = camcraft_app::schema::Operation {
        tool: TOOL.into(),
        src: src.clone(),
        params: adp_dog.clone(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let (tp, warns) = camcraft_app::operation::generate(&world, &adp_op, "AdpDog").expect("adaptive2d dogbone");
    assert!(!tp.moves.is_empty());
    // #3035: an Adaptive area that rides the Offset rings is one advisory
    // warning (it does not hold the post); verify is silent otherwise.
    let fell = camcraft_app::machining2d::rings_fallbacks(&tp.notes);
    assert_eq!(warns.len(), usize::from(fell > 0), "adaptive2d dogbone verify must be silent: {warns:?}");
    assert!(
        warns.iter().all(|w| {
            matches!(w, camcraft_app::verify::Warning::StrategyFallback { areas, .. } if *areas == fell)
                && !w.blocks_post()
        }),
        "adaptive2d dogbone: only the advisory fallback: {warns:?}\nnotes {:?}",
        tp.notes
    );

    // Trochoidal method with corner relief refuses by name.
    let mut adp_troch = adp_dog;
    if let OpParams::Adaptive2D { method, .. } = &mut adp_troch {
        *method = camcraft_app::machining2d::AdaptiveMethod::Trochoidal;
    }
    assert!(
        adp_troch.named_refusal().is_some(),
        "Adaptive2D Trochoidal + corner relief must be refused at validation"
    );
    let troch_op = camcraft_app::schema::Operation { params: adp_troch, ..adp_op.clone() };
    let troch_err = camcraft_app::operation::generate(&world, &troch_op, "AdpTroch")
        .expect_err("Adaptive2D Trochoidal + corner relief refuses generation")
        .to_string();
    assert!(troch_err.to_lowercase().contains("trochoidal"), "{troch_err}");

    // Retype Pocket -> Offset2D and apply Dogbone cleanly, then refuse when a DoNotTouch face collides with a corner.
    let off_base = camcraft_app::operation::retype_params(
        &base_op,
        camcraft_app::operation::OpType::Offset2D.ordinal(),
    );
    let mut off_dog = off_base;
    if let OpParams::Offset2D { corner_relief, .. } = &mut off_dog {
        corner_relief.kind = camcraft_app::machining2d::CornerReliefKind::Dogbone;
    }
    let off_op = camcraft_app::schema::Operation { params: off_dog, ..adp_op };
    let (off_tp, off_warns) = camcraft_app::operation::generate(&world, &off_op, "OffDog").expect("offset2d dogbone");
    assert!(!off_tp.moves.is_empty());
    assert!(off_warns.is_empty(), "offset2d dogbone verify must be silent: {off_warns:?}");

    // Separate clean DoNotTouch check on Offset2D: guard at (10.6, 10.6)..(18, 18) sits > 5 mm
    // from normal pocket pass (7, 7), but < 4.0 mm from dogbone spur end (7.879, 7.879) under 1 mm clearance.
    let (w4, mut h4) = setup_world(None, 60.0, 60.0, 20.0);
    h4.execute(&w4, ImportModel::from_mesh("area_box", cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 10.0))));
    h4.execute(&w4, ImportModel::from_mesh("guard_box", cube(Point3::new(10.6, 10.6, 10.0), Point3::new(18.0, 18.0, 20.0))));
    let f_area = model::features_of(&w4, &model::find_by_name(&w4, "area_box").unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .unwrap()
        .id;
    let f_guard = model::features_of(&w4, &model::find_by_name(&w4, "guard_box").unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.9)
        .unwrap()
        .id;
    let mut off_dnt_params = off_op.params.clone();
    if let OpParams::Offset2D { top_z, bottom_z, .. } = &mut off_dnt_params {
        *top_z = Height::fixed(20.0);
        *bottom_z = Height::fixed(10.0);
    }
    let mut off_dnt = camcraft_app::schema::Operation {
        tool: TOOL.into(),
        src: SrcRef::Selection(vec![GeoSel { entity: "area_box".into(), feature: FeatureSel::Face(f_area) }]),
        params: off_dnt_params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    off_dnt.area.do_not_touch.clearance = 1.0;
    off_dnt.area.do_not_touch.picks = vec![GeoSel { entity: "guard_box".into(), feature: FeatureSel::Face(f_guard) }];
    let dnt_err = camcraft_app::operation::generate(&w4, &off_dnt, "OffDnt")
        .expect_err("DoNotTouch clearance on guard face must refuse corner relief")
        .to_string();
    assert!(
        dnt_err.to_lowercase().contains("do not touch") || dnt_err.to_lowercase().contains("corner relief"),
        "must name Do not touch / corner relief: {dnt_err}"
    );
}

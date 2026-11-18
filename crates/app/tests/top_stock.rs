//! #104: explain a Top below remaining stock without changing the rapid/post laws.
//! #1559: a 2D ladder now carries that stock - its entry starts over the
//! material and feeds down, said by the `Carried stock:` note; a kind
//! that does not carry it keeps the #104 note and the rapid gate.
//! Gate map (#104, #1559): covers camcraft_app::operation::top_stock, camcraft_app::operation::generate.
//! Provenance: original synthetic fixtures and the shop rule never to rapid into uncut stock.

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, ReorderOperation, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{CARRIED_STOCK_NOTE, Height, OpParams, SrcRef, TOP_STOCK_WARNING};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use flecs_ecs::prelude::World;

fn scene(stock_top: f64, face: Option<f64>) -> World {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: stock_top,
            },
            Default::default(),
        )
        .expect("stock"),
    );
    let (_, tool) = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0600-4FL")
        .expect("catalog");
    history.execute(&world, AddTool::new(&world, "mill", tool).expect("tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    if let Some(cut) = face {
        let params = OpParams::Face {
            holes: Default::default(),
            blend_turns: false,
            tilt: None,
            stepover: 4.0,
            clearance_z: Height::fixed(20.0),
            cut_z: Height::fixed(cut),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        history.execute(
            &world,
            CreateOperation::new(&world, "Op1Face", "mill", SrcRef::Stock, params).expect("face"),
        );
        history.execute(
            &world,
            GenerateOperation::new(&world, "Op1Face").expect("face generation"),
        );
    }
    let params = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(12.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Op2Contour",
            "mill",
            SrcRef::Entity("part".into()),
            params,
        )
        .expect("contour"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Op2Contour").expect("contour generation"),
    );
    world
}

fn cache(world: &World) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, "Op2Contour").expect("op");
    model::toolpath_of(world, &e).expect("path")
}

#[test]
fn generation_carries_the_stock_over_top_and_names_it() {
    let world = scene(10.0, None);
    let tp = cache(&world);
    let note = tp.path.notes.iter().find(|n| n.starts_with(CARRIED_STOCK_NOTE));
    assert!(note.is_some(), "missing the cause: {:?}", tp.path.notes);
    let note = note.expect("note");
    for text in ["Z5.000", "Z10.000", "over its region", "engages 5.000 mm"] {
        assert!(note.contains(text), "{note}");
    }
    assert!(tp.path.notes.iter().all(|n| !n.starts_with(TOP_STOCK_WARNING)), "{:?}", tp.path.notes);
    assert!(
        !tp.warnings.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "{:?}",
        tp.warnings
    );
}

#[test]
fn a_slot_does_not_carry_the_stock_and_keeps_the_note_and_the_rapid_gate() {
    // The slot is not one of #1559's carrying ladders yet: its entry
    // still rapids to Top + the plunge height, the #104 note explains
    // it and the rapid law refuses the post.
    let world = scene(10.0, None);
    let mut history = History::default();
    let slot = OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(-10.0, -8.0),
        b: camcraft_app::geo2d::Point2::new(20.0, -8.0),
        width: 8.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: None,
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(2.0),
        stepdown: 3.0,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    };
    history.execute(&world, CreateOperation::new(&world, "Op3Slot", "mill", SrcRef::Stock, slot).expect("slot"));
    history.execute(&world, GenerateOperation::new(&world, "Op3Slot").expect("slot generation"));
    let e = model::find_by_name(&world, "Op3Slot").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("path");
    let note = tp.path.notes.iter().find(|n| n.starts_with(TOP_STOCK_WARNING));
    let note = note.unwrap_or_else(|| panic!("missing the cause: {:?}", tp.path.notes));
    for text in ["Z5.000", "Z10.000", "over its region", "raise Top or face first"] {
        assert!(note.contains(text), "{note}");
    }
    assert!(
        tp.warnings
            .iter()
            .any(|w| matches!(w, Warning::RapidInStock { .. }))
    );
    let out = std::env::temp_dir().join(format!("camcraft-top-stock-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    assert!(camcraft_app::posting::post_setup(&world, &out).is_err());
    assert!(!out.exists(), "the unsafe program must write nothing");
}

#[test]
fn facing_to_top_removes_the_height_warning_but_a_shallow_face_does_not() {
    let cleared = cache(&scene(10.0, Some(5.0)));
    assert!(
        cleared
            .path
            .notes
            .iter()
            .all(|n| !n.starts_with(TOP_STOCK_WARNING) && !n.starts_with(CARRIED_STOCK_NOTE)),
        "{:?}",
        cleared.path.notes
    );
    assert!(cleared.warnings.is_empty(), "{:?}", cleared.warnings);
    let partial = cache(&scene(10.0, Some(8.0)));
    assert!(
        partial
            .path
            .notes
            .iter()
            .any(|n| n.starts_with(CARRIED_STOCK_NOTE) && n.contains("Z8.000")),
        "{:?}",
        partial.path.notes
    );
}

#[test]
fn top_at_the_stock_surface_has_no_height_warning() {
    let tp = cache(&scene(5.0, None));
    assert!(
        tp.path
            .notes
            .iter()
            .all(|n| !n.starts_with(TOP_STOCK_WARNING) && !n.starts_with(CARRIED_STOCK_NOTE)),
        "{:?}",
        tp.path.notes
    );
}

#[test]
fn a_narrow_mesh_stock_feature_is_not_acquitted_by_an_unsampled_prior() {
    let world = scene(10.0, None);
    let mut history = History::default();
    let mut raw = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 5.0))
        .facets
        .to_vec();
    raw.extend(
        cube(
            Point3::new(1.001, 1.001, 5.0),
            Point3::new(1.006, 1.006, 10.0),
        )
        .facets
        .iter()
        .copied(),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("raw-stock", camcraft_app::geometry::Mesh::from_facets(raw)),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "raw-stock".into(),
            },
            Default::default(),
        )
        .expect("mesh stock"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh(
            "remote-patch",
            cube(Point3::new(12.0, 12.0, 0.0), Point3::new(16.0, 16.0, 5.0)),
        ),
    );
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(20.0),
        cut_z: Height::fixed(5.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Op1Elsewhere",
            "mill",
            SrcRef::Entity("remote-patch".into()),
            face,
        )
        .expect("remote face"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Op1Elsewhere").expect("generate remote face"),
    );
    history.execute(
        &world,
        ReorderOperation::new(&world, "Op1Elsewhere", "Op2Contour", false)
            .expect("prior runs first"),
    );
    let prior = camcraft_app::rest::prior_plan(&world, "Op2Contour", None, 3.0)
        .expect("plan")
        .expect("predecessor")
        .simulate()
        .expect("sampled grid");
    assert!(
        (0..prior.nx * prior.ny).all(|k| prior.top_at(k) <= 5.0 + 1e-6),
        "fixture must miss the narrow high feature, not credit its removal"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Op2Contour").expect("regenerate contour"),
    );
    assert!(
        cache(&world)
            .path
            .notes
            .iter()
            .any(|n| n.starts_with(TOP_STOCK_WARNING) && n.contains("Z10.000")),
        "the remote cut did not remove the boss: {:?}",
        cache(&world).path.notes
    );
}

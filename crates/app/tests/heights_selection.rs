//! M11.3 A acceptance: every height can be measured From: Selection - the
//! operation's OWN source geometry. What must hold: Top / Clearance /
//! Retract read the selection's highest Z and Bottom its lowest, per
//! source type (picked faces, picked edge chains, a whole entity, the
//! stock); the one resolver answers for generation, the panel and the
//! digest; a re-pick re-resolves and the op goes Stale; a selection that
//! yields no height is refused by name at the command door and at
//! generation - never a silent 0; the words round-trip through the file.
//! Provenance: our own design notes; the "height from the selection"
//! convention is public CAM practice, behavior only.
//!
//! Gate map (M11.3): covers app::commands (create, edit, geometry pick,
//! links), app::operation (heights, heights_sel, stamp), app::project,
//! app::schema.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationLinks, EditOperationParams, GenerateOperation, History,
    ImportModel, MoveModel, SetOperationGeometry, SetStock,
};
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference as R, OpParams, OpStatus, SrcRef,
};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn add_tool(world: &World, history: &mut History, name: &str) {
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(world, AddTool::new(world, name, tool).expect("tool"));
}

/// 60x60x20 box stock (z 0..20), a 2x2x4 block standing z 5..9, a tool.
fn fixture() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    history.execute(&world, stock.expect("stock"));
    history.execute(
        &world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 5.0), Point3::new(2.0, 2.0, 9.0))),
    );
    add_tool(&world, &mut history, TOOL);
    (world, history)
}

/// The horizontal planar faces of `name`, lowest plane first.
fn flat_faces(world: &World, name: &str) -> Vec<(f64, FeatureSel)> {
    let e = model::find_by_name(world, name).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let mut out: Vec<(f64, FeatureSel)> = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2].abs() > 0.999)
        .map(|fa| (fa.offset / fa.normal[2] + loc.pos.z, FeatureSel::Face(fa.id)))
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn pick(entity: &str, feature: FeatureSel) -> GeoSel {
    GeoSel { entity: entity.into(), feature }
}

fn top_of(src: &SrcRef, world: &World, off: f64) -> Option<f64> {
    operation::resolve_height(world, None, Some(src), Height::of(R::TopOfSelection, off))
}

fn bottom_of(src: &SrcRef, world: &World, off: f64) -> Option<f64> {
    operation::resolve_height(world, None, Some(src), Height::of(R::BottomOfSelection, off))
}

/// A pocket whose four heights all read the selection.
fn pocket_from_selection() -> OpParams {
    OpParams::Pocket {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(R::TopOfSelection, 15.0),
        retract_z: Some(Height::of(R::TopOfSelection, 2.0)),
        top_z: Height::of(R::TopOfSelection, 0.0),
        bottom_z: Height::of(R::BottomOfSelection, 0.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

/// A facing pass over the picked face, cutting AT the face (bottom end).
fn face_on_selection() -> OpParams {
    OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 2.0,
        clearance_z: Height::of(R::TopOfSelection, 15.0),
        cut_z: Height::of(R::BottomOfSelection, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    }
}

#[test]
fn the_reference_is_appended_and_names_its_end() {
    assert_eq!(R::KIND_LABELS[6], "Selection");
    assert_eq!(R::TopOfSelection.kind_index(), 6);
    assert_eq!(R::BottomOfSelection.kind_index(), 6);
    assert_eq!(R::selection_for(true), R::BottomOfSelection);
    assert_eq!(R::selection_for(false), R::TopOfSelection);
    assert_eq!(R::TopOfSelection.to_string(), "selection top");
    assert_eq!(R::BottomOfSelection.to_string(), "selection bottom");
    assert!(Height::of(R::TopOfSelection, 1.0).is_sane());
    assert!(!Height::of(R::TopOfSelection, f64::NAN).is_sane());
}

#[test]
fn each_source_type_resolves_its_own_span() {
    let (world, _h) = fixture();
    // Stock: the setup's stock box.
    assert_eq!(top_of(&SrcRef::Stock, &world, 0.0), Some(20.0));
    assert_eq!(bottom_of(&SrcRef::Stock, &world, 0.0), Some(0.0));
    // A whole entity: its world bounds.
    let ent = SrcRef::Entity("block".into());
    assert_eq!(top_of(&ent, &world, 0.0), Some(9.0));
    assert_eq!(bottom_of(&ent, &world, -1.0), Some(4.0), "the offset rides on top");
    // One picked flat face: its own Z at both ends.
    let faces = flat_faces(&world, "block");
    let (lo_z, lo) = faces[0];
    let (hi_z, hi) = faces[faces.len() - 1];
    assert!((hi_z - 9.0).abs() < 1e-9 && (lo_z - 5.0).abs() < 1e-9);
    let top_face = SrcRef::Selection(vec![pick("block", hi)]);
    assert_eq!(top_of(&top_face, &world, 0.0), Some(9.0));
    assert_eq!(bottom_of(&top_face, &world, 0.0), Some(9.0), "a flat face's span is its Z");
    // Several picks: the union.
    let both = SrcRef::Selection(vec![pick("block", hi), pick("block", lo)]);
    assert_eq!(top_of(&both, &world, 0.0), Some(9.0));
    assert_eq!(bottom_of(&both, &world, 0.0), Some(5.0));
    // An edge chain: its points' span.
    let e = model::find_by_name(&world, "block").expect("block");
    let f = model::features_of(&world, &e).expect("features");
    let chains = feature::chains(&f);
    let c = chains.first().expect("a cube has feature edges");
    let zs: Vec<f64> = feature::chain_points(&f, c).iter().map(|p| p[2]).collect();
    let (clo, chi) = zs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| (a.min(*z), b.max(*z)));
    let chain = SrcRef::Selection(vec![pick("block", FeatureSel::Chain(c.id))]);
    assert_eq!(top_of(&chain, &world, 0.0), Some(chi));
    assert_eq!(bottom_of(&chain, &world, 0.0), Some(clo));
    assert!((5.0..=9.0).contains(&clo) && (5.0..=9.0).contains(&chi));
    // A moved entity: the span follows the placement.
    let mut h = History::default();
    let up = Location { pos: Point3::new(0.0, 0.0, 3.0), rot: IDENTITY_Q };
    h.execute(&world, MoveModel::new(&world, "block", up).expect("move"));
    assert_eq!(top_of(&top_face, &world, 0.0), Some(12.0));
    // No op in hand: unresolved, never guessed.
    let none = Height::of(R::TopOfSelection, 0.0);
    assert_eq!(operation::resolve_height(&world, None, None, none), None);
    assert_eq!(none.z(&world), None);
    assert_eq!(
        operation::missing_source(&world, None, None, R::TopOfSelection),
        Some(operation::SELECTION_UNKNOWN)
    );
}

#[test]
fn every_height_slot_reads_through_the_one_resolver() {
    let (world, _h) = fixture();
    let src = SrcRef::Entity("block".into());
    let r = operation::resolve_params(&world, None, Some(&src), &pocket_from_selection())
        .expect("every height resolves");
    assert_eq!(r.clearance, 24.0, "clearance: highest Z + 15");
    assert_eq!(r.retract, Some(11.0), "retract: highest Z + 2");
    assert_eq!(r.top, Some(9.0), "top: highest Z");
    assert_eq!(r.bottom, Some(5.0), "bottom: lowest Z");
    assert!(operation::depth_sane(&world, None, Some(&src), &pocket_from_selection()));
    // Without the source the same params refuse by name.
    let err = operation::resolve_params(&world, None, None, &pocket_from_selection())
        .expect_err("no source in hand");
    assert!(err.contains("Clearance Z") && err.contains("selection top"), "{err}");
    // A one-face selection collapses top onto bottom: cut-downward refuses.
    let faces = flat_faces(&world, "block");
    let flat = SrcRef::Selection(vec![pick("block", faces[faces.len() - 1].1)]);
    assert!(!operation::depth_sane(&world, None, Some(&flat), &pocket_from_selection()));
}

#[test]
fn creation_and_edits_refuse_a_selection_that_yields_no_height() {
    let (world, mut history) = fixture();
    // A pick whose feature id is no longer in the model: the source door
    // lets ids resolve late, the heights door does not.
    let ghost = SrcRef::Selection(vec![pick("block", FeatureSel::Face(0xdead_beef))]);
    let err = CreateOperation::new(&world, "P0", TOOL, ghost.clone(), pocket_from_selection())
        .err()
        .expect("refused at the door")
        .to_string();
    assert!(err.contains("From: Selection") && err.contains("re-pick"), "{err}");
    // Created on a good entity source, then retargeted onto the ghost.
    let good = SrcRef::Entity("block".into());
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", TOOL, good, pocket_from_selection()).expect("create"),
    );
    let err = EditOperationLinks::new(&world, "P1", None, Some(ghost))
        .err()
        .expect("a source that cannot answer the heights")
        .to_string();
    assert!(err.contains("From: Selection"), "{err}");
    // Re-picking one flat face collapses top onto bottom: refused too.
    let faces = flat_faces(&world, "block");
    let err = SetOperationGeometry::new(&world, "P1", vec![pick("block", faces[faces.len() - 1].1)])
        .err()
        .expect("a pick that makes the op cut upward")
        .to_string();
    assert!(err.contains("cut downward"), "{err}");
    // Params that never read the selection pass the pick exactly as before.
    let fixed = OpParams::Pocket {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(9.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "P2", TOOL, SrcRef::Entity("block".into()), fixed).expect("create"),
    );
    assert!(SetOperationGeometry::new(&world, "P2", vec![pick("block", faces[0].1)]).is_ok());
}

#[test]
fn generation_refuses_a_vanished_selection_by_name() {
    let (world, mut history) = fixture();
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", TOOL, SrcRef::Entity("block".into()), pocket_from_selection())
            .expect("create"),
    );
    // The file-load road: an op whose stored pick no longer resolves
    // (commands never build one; a document opened over a changed model
    // does). Generation must refuse by the height's name, not cut at 0.
    let e = model::find_by_name(&world, "P1").expect("op");
    let mut op = model::operation_of(&world, &e).expect("operation");
    op.src = SrcRef::Selection(vec![pick("block", FeatureSel::Face(0xdead_beef))]);
    let err = operation::resolve_params(&world, None, Some(&op.src), &op.params)
        .expect_err("the picked face is gone");
    assert!(
        err.contains("Clearance Z (From: selection top, offset 15)") && err.contains("re-pick"),
        "{err}"
    );
    let Err(refusal) = operation::generate(&world, &op, "P1") else {
        panic!("a vanished selection must refuse generation");
    };
    let refusal = refusal.to_string();
    assert!(refusal.contains("selection") || refusal.contains("re-pick"), "{refusal}");
}

#[test]
fn an_edit_to_selection_rides_edit_params_and_undoes() {
    let (world, mut history) = fixture();
    let before = OpParams::Pocket {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(R::TopOfStock, 5.0),
        retract_z: None,
        top_z: Height::of(R::TopOfModel, 0.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", TOOL, SrcRef::Entity("block".into()), before.clone())
            .expect("create"),
    );
    let after = pocket_from_selection();
    history.execute(&world, EditOperationParams::new(&world, "P1", after.clone()).expect("edit"));
    let e = model::find_by_name(&world, "P1").expect("op");
    let read = |w: &World| model::operation_of(w, &e).expect("operation").params;
    assert_eq!(read(&world), after);
    assert!(history.undo(&world));
    assert_eq!(read(&world), before, "undo restores the previous references");
    assert!(history.redo(&world));
    assert_eq!(read(&world), after);
}

#[test]
fn a_selection_change_turns_the_op_stale() {
    let (world, mut history) = fixture();
    let faces = flat_faces(&world, "block");
    let (_, top) = faces[faces.len() - 1];
    let (_, bottom) = faces[0];
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "F1",
            TOOL,
            SrcRef::Selection(vec![pick("block", top)]),
            face_on_selection(),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("generate"));
    let e = model::find_by_name(&world, "F1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    // The facing pass cuts AT the picked face: Bottom of the selection.
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let lowest = tp.path.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((lowest - 9.0).abs() < 1e-9, "faces at the picked face's Z, got {lowest}");
    // A re-pick moves the resolved heights: Stale, like any input change.
    history.execute(
        &world,
        SetOperationGeometry::new(&world, "F1", vec![pick("block", bottom)]).expect("re-pick"),
    );
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale);
    assert!(history.undo(&world));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "undo puts the pick back");
    // Moving the picked entity moves the selection's Z: Stale again.
    let up = Location { pos: Point3::new(0.0, 0.0, 1.0), rot: IDENTITY_Q };
    history.execute(&world, MoveModel::new(&world, "block", up).expect("move"));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale);
}

#[test]
fn a_picked_face_of_the_users_part_sets_the_heights() {
    // The committed STEP part: pick its highest flat face, face it there.
    let world = new_world();
    let mut history = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the STEP imports");
    let name = imp.name.clone();
    history.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc);
    history.execute(&world, stock.expect("stock"));
    add_tool(&world, &mut history, "EM-1000-4FL");
    let faces = flat_faces(&world, &name);
    assert!(faces.len() >= 2, "the part carries several flat faces");
    let (top_z, top) = faces[faces.len() - 1];
    let (floor_z, floor) = faces[0];
    let src = SrcRef::Selection(vec![pick(&name, top)]);
    assert_eq!(top_of(&src, &world, 0.0).map(|z| (z - top_z).abs() < 1e-9), Some(true));
    assert_eq!(bottom_of(&src, &world, 0.0).map(|z| (z - top_z).abs() < 1e-9), Some(true));
    let span = SrcRef::Selection(vec![pick(&name, top), pick(&name, floor)]);
    assert_eq!(bottom_of(&span, &world, 0.0).map(|z| (z - floor_z).abs() < 1e-9), Some(true));
    history.execute(
        &world,
        CreateOperation::new(&world, "F1", "EM-1000-4FL", src, face_on_selection()).expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("generate"));
    let e = model::find_by_name(&world, "F1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let lowest = tp.path.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    let highest = tp.path.moves.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
    assert!((lowest - top_z).abs() < 1e-9, "cut at the picked face {top_z}, got {lowest}");
    assert!((highest - (top_z + 15.0)).abs() < 1e-9, "clearance = face + 15, got {highest}");
}

#[test]
fn the_selection_words_round_trip_through_the_file() {
    let (world, mut history) = fixture();
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", TOOL, SrcRef::Entity("block".into()), pocket_from_selection())
            .expect("create"),
    );
    let path = std::env::temp_dir().join(format!("camcraft_heights_sel_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(saved.contains("\"top_of_selection\""), "{saved}");
    assert!(saved.contains("\"bottom_of_selection\""), "{saved}");
    let world2 = new_world();
    project::open(&world2, &saved).expect("reopen at the same format version");
    let e = model::find_by_name(&world2, "P1").expect("op");
    assert_eq!(model::operation_of(&world2, &e).expect("operation").params, pocket_from_selection());
}

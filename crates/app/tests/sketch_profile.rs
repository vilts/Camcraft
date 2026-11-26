//! #1862: a DXF drawing is a profile placed on the stock of a setup - not
//! a model. Pins: the placement on the stock top, the move on the stock,
//! undo and redo, the typed missing-units error, the virtual model
//! reference with its two refusals (a 3D operation in the setup, and a
//! kind that needs a solid on the profile), the generation refusal, the
//! delete, and that deleting the entity frees its drawing.
//!
//! Provenance: our own tests over our own commands; the DXF text is
//! hand-built from Autodesk's public ASCII DXF group conventions.
//!
//! Gate map (M4.4): covers app::commands (ImportDxf, MoveSketch,
//! DeleteEntity, SelectEntity, DeleteSetup, MoveModel, CreateOperation,
//! ChangeOperationKind, SetOperationSetup, GenerateOperation),
//! app::sketch, app::sketch_model, app::schema (Sketch), app::model,
//! app::operation (heights), geometry::sketch_data.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, Command, CommandError, CreateOperation, CreateSetup, DeleteEntity, DeleteSetup,
    DxfOptions, GenerateOperation, History, ImportDxf, ImportModel, MoveModel, MoveSketch, SelectEntity, SetOperationSetup,
    SetStock,
};
use camcraft_app::geometry::{cube, transform_point, IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, HeightReference, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::units::Unit;
use camcraft_app::world::new_world;
use camcraft_app::{sketch, sketch_model};

const TOOL: &str = "EM-0400-2FL";

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-sketch-profile-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 100 x 50 rectangle and a circle (closed), one loose line (open) and
/// one TEXT (skipped). `insunits`: the header's `$INSUNITS`, or none.
fn dxf_text(insunits: Option<i32>) -> String {
    let header = insunits.map_or(String::new(), |code| format!("9\n$INSUNITS\n70\n{code}\n"));
    format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n50\n10\n0\n20\n50\n\
         0\nCIRCLE\n8\nholes\n10\n20\n20\n25\n40\n5\n\
         0\nLINE\n8\nmarks\n10\n60\n20\n10\n11\n80\n21\n10\n\
         0\nTEXT\n8\nnotes\n10\n0\n20\n0\n40\n3\n1\nhello\n\
         0\nENDSEC\n0\nEOF\n"
    )
}

fn write_dxf(dir: &Path, file: &str, insunits: Option<i32>) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, dxf_text(insunits)).expect("write dxf");
    path
}

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

fn refusal<T>(r: Result<T, CommandError>, what: &str) -> String {
    match r {
        Ok(_) => panic!("{what} should have been refused"),
        Err(e) => e.to_string(),
    }
}

/// A 200 x 120 x 6 sheet as the stock, its origin at (10, 20, 5), and a
/// flat end mill. No model.
fn sheet_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 200.0, dy: 120.0, dz: 6.0 };
    let at = Location { pos: Point3::new(10.0, 20.0, 5.0), rot: IDENTITY_Q };
    run(&world, &mut history, SetStock::new(&world, shape, at), "sheet stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    (world, history)
}

fn place(world: &World, history: &mut History, path: &Path) -> String {
    let cmd = ImportDxf::new(world, None, path, &DxfOptions::default()).expect("the drawing imports");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn pose(world: &World, name: &str) -> Location {
    let (entity, _) = sketch::find(world, name).expect("the profile is in the document");
    sketch::world_pose(world, &entity).expect("the profile has a place on its stock")
}

#[test]
fn a_dxf_profile_lands_on_the_stock_top_at_the_stock_origin_and_is_not_a_model() {
    let dir = tmp_dir("place");
    let (world, mut history) = sheet_world();
    let name = place(&world, &mut history, &write_dxf(&dir, "plate.dxf", Some(4)));
    assert_eq!(name, "plate", "named by the file's stem");

    let (entity, profile) = sketch::find(&world, &name).expect("a Sketch entity");
    assert!(!model::is_model(&world, &entity), "a profile is not a model");
    assert!(model::model_entities(&world).is_empty(), "the part readers never list it");
    assert!(model::facets_of(&world, &entity).is_none(), "no facets: no 3D tool reads it as a mesh");
    assert_eq!(profile.setup, None, "a document without setups: the implicit setup");

    // The placement: the top face of the stock (5 + 6), at the stock origin.
    let at = pose(&world, &name);
    assert_eq!(at.pos, Point3::new(10.0, 20.0, 11.0));
    assert_eq!(at.rot, IDENTITY_Q);

    // Layer, chain, unit and skip metadata.
    let data = &profile.data;
    assert_eq!(data.layers(), ["cut", "holes", "marks"]);
    assert_eq!((data.chains().closed.len(), data.chains().open.len(), data.chains().rejected.len()), (2, 1, 0));
    assert!(!data.units().overridden(), "the file's own millimetres");
    assert_eq!(data.summary().skipped.get("TEXT"), Some(&1));

    // The profile rides the stock: a moved stock carries it.
    let moved = Location { pos: Point3::new(-30.0, 0.0, 2.0), rot: IDENTITY_Q };
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 200.0, dy: 120.0, dz: 10.0 }, moved), "new stock");
    assert_eq!(pose(&world, &name).pos, Point3::new(-30.0, 0.0, 12.0), "the top of the new stock");

    // The same drawing placed again gets a name of its own.
    assert_eq!(place(&world, &mut history, &dir.join("plate.dxf")), "plate (2)");
    assert_eq!(sketch::placed_in(&world, None), ["plate", "plate (2)"]);
}

#[test]
fn a_profile_moves_and_turns_on_its_stock_face_with_undo_and_redo() {
    let dir = tmp_dir("move");
    let (world, mut history) = sheet_world();
    let name = place(&world, &mut history, &write_dxf(&dir, "plate.dxf", Some(4)));

    // 450 degrees is 90 degrees: one turn is every turn.
    run(&world, &mut history, MoveSketch::new(&world, &name, 30.0, -10.0, 450.0), "move");
    let at = pose(&world, &name);
    assert_eq!(at.pos, Point3::new(40.0, 10.0, 11.0), "moved on the face, still on the stock top");
    // The drawing's +X now runs along the face's +Y.
    let x_end = transform_point(Point3::new(100.0, 0.0, 0.0), at.pos, at.rot);
    near(x_end.x, 40.0);
    near(x_end.y, 110.0);
    near(x_end.z, 11.0);
    let (entity, _) = sketch::find(&world, &name).expect("profile");
    let (x, y, angle) = model::transform_of(&world, &entity).and_then(sketch::face_placement).expect("on the face");
    near(x, 30.0);
    near(y, -10.0);
    near(angle, 90.0);

    assert!(history.undo(&world), "undo the move");
    assert_eq!(pose(&world, &name), Location { pos: Point3::new(10.0, 20.0, 11.0), rot: IDENTITY_Q });
    assert!(history.undo(&world), "undo the import");
    assert!(sketch::find(&world, &name).is_none(), "undo removes the profile");
    assert!(history.redo(&world) && history.redo(&world), "redo both");
    assert_eq!(pose(&world, &name).pos, Point3::new(40.0, 10.0, 11.0), "redo puts the moved profile back");

    // Refusals, by name; nothing moves.
    assert!(refusal(MoveSketch::new(&world, &name, f64::NAN, 0.0, 0.0), "NaN").contains("must be finite"));
    assert!(refusal(MoveSketch::new(&world, &name, 2.0e6, 0.0, 0.0), "far").contains("from the stock origin"));
    assert!(refusal(MoveSketch::new(&world, TOOL, 0.0, 0.0, 0.0), "a tool").contains("is not a DXF profile"));
    assert!(matches!(MoveSketch::new(&world, "nothing", 0.0, 0.0, 0.0), Err(CommandError::NotFound(_))));
    // A profile is not placed with the model's verb.
    let why = refusal(MoveModel::new(&world, &name, Location::default()), "MoveModel on a profile");
    assert!(why.contains("is a DXF profile"), "{why}");
    assert_eq!(pose(&world, &name).pos, Point3::new(40.0, 10.0, 11.0));
}

#[test]
fn a_file_without_units_is_the_typed_missing_units_error_until_a_unit_is_stated() {
    let dir = tmp_dir("units");
    let (world, _history) = sheet_world();
    for (file, code) in [("none.dxf", None), ("zero.dxf", Some(0))] {
        let path = write_dxf(&dir, file, code);
        let refused = ImportDxf::new(&world, None, &path, &DxfOptions::default());
        match refused {
            Err(CommandError::DxfUnitsMissing { width, height }) => {
                near(width, 100.0);
                near(height, 50.0);
            }
            Err(other) => panic!("expected the typed missing-units error, got: {other}"),
            Ok(_) => panic!("a file with no units must not import by a guess"),
        }
        assert!(sketch::sketches(&world).is_empty(), "nothing landed");
        let inches = DxfOptions { units: Some(Unit::Inches), ..Default::default() };
        let cmd = ImportDxf::new(&world, None, &path, &inches).expect("a stated unit clears it");
        assert!(cmd.data().units().overridden());
        near(cmd.data().bounds().expect("bounds").1.x, 2540.0);
    }
}

#[test]
fn an_import_that_cannot_stand_is_refused_at_construction() {
    let dir = tmp_dir("refuse");
    let good = write_dxf(&dir, "plate.dxf", Some(4));
    let opts = DxfOptions::default();

    // No stock: the profile has nothing to lie on.
    let bare = new_world();
    let why = refusal(ImportDxf::new(&bare, None, &good, &opts), "no stock");
    assert!(why.contains("has no stock - make a stock first"), "{why}");

    let (world, mut history) = sheet_world();
    assert!(matches!(ImportDxf::new(&world, Some("Nowhere"), &good, &opts), Err(CommandError::NotFound(_))));
    let stl = dir.join("plate.stl");
    std::fs::write(&stl, dxf_text(Some(4))).expect("write");
    assert!(refusal(ImportDxf::new(&world, None, &stl, &opts), "not a dxf").contains("is not a .dxf file"));
    assert!(refusal(ImportDxf::new(&world, None, &dir.join("missing.dxf"), &opts), "missing").contains("cannot read"));
    let broken = dir.join("broken.dxf");
    std::fs::write(&broken, "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\ncut\n10\nnot-a-number\n").expect("write");
    assert!(matches!(ImportDxf::new(&world, None, &broken, &opts), Err(CommandError::Dxf(_))));
    let empty = dir.join("empty.dxf");
    std::fs::write(&empty, "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n").expect("write");
    assert!(refusal(ImportDxf::new(&world, None, &empty, &opts), "empty").contains("no line, arc or circle"));
    let wide = DxfOptions { join_tolerance: Some(50.0), ..Default::default() };
    assert!(refusal(ImportDxf::new(&world, None, &good, &wide), "tolerance").contains("join tolerance"));

    // A stated name must be free and must not be a furniture name.
    let named = |n: &str| DxfOptions { name: Some(n.to_owned()), ..Default::default() };
    assert!(matches!(ImportDxf::new(&world, None, &good, &named(TOOL)), Err(CommandError::NameConflict(_))));
    assert!(refusal(ImportDxf::new(&world, None, &good, &named("Stock")), "Stock").contains("not a usable name"));
    run(&world, &mut history, ImportDxf::new(&world, None, &good, &named("outline")), "named");
    assert!(sketch::find(&world, "outline").is_some());

    // The same laws over bytes in hand.
    let bytes = dxf_text(Some(1));
    let cmd = ImportDxf::from_bytes(&world, None, "picked.dxf", bytes.as_bytes(), &opts).expect("bytes import");
    assert_eq!(cmd.name(), "picked");
    assert!(refusal(ImportDxf::from_bytes(&world, None, "picked.txt", bytes.as_bytes(), &opts), "txt").contains("not a .dxf"));
}

#[test]
fn a_stock_and_profile_setup_accepts_a_2d_operation_and_refuses_what_needs_a_solid() {
    let dir = tmp_dir("ops");
    let (world, mut history) = sheet_world();
    let name = place(&world, &mut history, &write_dxf(&dir, "plate.dxf", Some(4)));
    let on_profile = || SrcRef::Entity(name.clone());
    let seed = |kind: OpType| operation::seed_params(kind.ordinal());

    // The virtual model reference: the setup has a profile and no model.
    let virt = sketch_model::of(&world, None).expect("a virtual model reference");
    assert_eq!(virt.profiles, std::slice::from_ref(&name));
    assert!(sketch_model::model_loaded(&world, None), "the operation pages have a model to name");
    assert!(!sketch_model::has_part(&world), "and it is not a solid");

    // A 2D operation on the sketch chains is accepted, and says what it is checked against.
    run(&world, &mut history, CreateOperation::new(&world, "Pkt", TOOL, on_profile(), seed(OpType::Pocket)), "2D Pocket on the profile");
    let op = model::find_by_name(&world, "Pkt").and_then(|e| model::operation_of(&world, &e)).expect("the operation");
    assert_eq!(op.src, on_profile());
    let what = sketch_model::checked_against(&world, &op).expect("an operation on a profile states its checks");
    assert!(what.contains("the stock 'Stock'") && what.contains("the chains of the DXF profile 'plate' (2 closed, 1 open)"), "{what}");
    assert!(what.contains("no solid model"), "{what}");

    // Refusal one: a 3D operation in that setup names the missing solid model.
    let why = refusal(CreateOperation::new(&world, "Finish", TOOL, SrcRef::Stock, seed(OpType::Finish3D)), "3D Finish, stock source");
    assert!(why.contains("a 3D Finish operation needs a solid model"), "{why}");
    assert!(why.contains("the DXF profile 'plate', a virtual model reference with no solid"), "{why}");
    let why = refusal(CreateOperation::new(&world, "Rough", TOOL, SrcRef::Stock, seed(OpType::Pocket3D)), "3D Pocket");
    assert!(why.contains("a 3D Pocket operation needs a solid model"), "{why}");

    // Refusal two: a kind that needs a solid, on the profile itself.
    for kind in [OpType::Finish3D, OpType::Drill, OpType::Contour5D] {
        let why = refusal(CreateOperation::new(&world, "No", TOOL, on_profile(), seed(kind)), "a solid kind on a profile");
        assert!(why.contains("needs a solid model - 'plate' is a DXF profile, a virtual model reference with no solid"), "{kind}: {why}");
    }
    // The same law at the retype door, and for picked features of a non-Contour profile op.
    let why = refusal(ChangeOperationKind::new(&world, "Pkt", OpType::Pocket3D.ordinal()), "retype to 3D");
    assert!(why.contains("needs a solid model"), "{why}");
    let pick = SrcRef::Selection(vec![GeoSel { entity: name.clone(), feature: FeatureSel::Chain(7) }]);
    let why = refusal(CreateOperation::new(&world, "Pick", TOOL, pick, seed(OpType::Pocket)), "picked chain");
    assert!(why.contains("picked chains of a profile are not supported yet"), "{why}");

    // A model height has no value on a virtual reference, and says why.
    assert_eq!(
        operation::missing_source(&world, None, None, HeightReference::TopOfModel),
        Some(sketch_model::NO_SOLID_HEIGHT)
    );

    // Non-Contour operations still never pass for want of a solid: refused by name, with what it checks.
    let why = refusal(GenerateOperation::new(&world, "Pkt"), "generate on a profile");
    assert!(why.contains("checked against the stock 'Stock' and the chains of the DXF profile 'plate'"), "{why}");
    assert!(why.contains("is not generated yet"), "{why}");
    assert!(model::find_by_name(&world, "Pkt").is_some_and(|e| !model::has_toolpath(&world, &e)), "no toolpath landed");

    // With a part in the document the reference is no longer virtual.
    let block = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 5.0));
    history.execute(&world, ImportModel::from_mesh("block", block));
    assert!(sketch_model::of(&world, None).is_none(), "a solid model stands for itself");
    assert!(sketch_model::source_problem(&world, None, &SrcRef::Stock, &seed(OpType::Finish3D)).is_none());
    assert!(operation::missing_source(&world, None, None, HeightReference::TopOfModel).is_none());
}

#[test]
fn a_profile_belongs_to_its_setup_and_deleting_it_is_undoable() {
    let dir = tmp_dir("setups");
    let world = new_world();
    let mut history = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    let sheet = || StockShape::Box { dx: 200.0, dy: 120.0, dz: 6.0 };
    run(&world, &mut history, CreateSetup::new(&world, Some("Sheet"), None), "Sheet");
    run(&world, &mut history, SetStock::new(&world, sheet(), Location::default()), "Sheet stock");
    run(&world, &mut history, CreateSetup::new(&world, Some("Other"), None), "Other");
    run(&world, &mut history, SetStock::new(&world, sheet(), Location::default()), "Other stock");

    // The command takes the setup as an argument.
    let path = write_dxf(&dir, "plate.dxf", Some(4));
    run(&world, &mut history, ImportDxf::new(&world, Some("Sheet"), &path, &DxfOptions::default()), "import into Sheet");
    assert_eq!(sketch::placed_in(&world, Some("Sheet")), ["plate"]);
    assert!(sketch::placed_in(&world, Some("Other")).is_empty());
    assert!(sketch_model::of(&world, Some("Sheet")).is_some() && sketch_model::of(&world, Some("Other")).is_none());

    // An operation cuts the profiles of its own setup only ("Other" is active).
    let contour = operation::seed_params(OpType::Contour.ordinal());
    let why = refusal(CreateOperation::new(&world, "Cut", TOOL, SrcRef::Entity("plate".into()), contour.clone()), "wrong setup");
    assert!(why.contains("on the stock of setup 'Sheet'"), "{why}");
    run(&world, &mut history, camcraft_app::commands::ActivateSetup::new(&world, Some("Sheet")), "activate Sheet");
    run(&world, &mut history, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Entity("plate".into()), contour), "Cut");
    let why = refusal(SetOperationSetup::new(&world, "Cut", Some("Other")), "move the operation away from its profile");
    assert!(why.contains("on the stock of setup 'Sheet'"), "{why}");

    // A setup is not deleted with a profile still on its stock.
    run(&world, &mut history, camcraft_app::commands::DeleteOperation::new(&world, "Cut"), "delete the operation");
    let why = refusal(DeleteSetup::new(&world, "Sheet"), "delete a setup with a profile");
    assert!(why.contains("the DXF profile 'plate' is placed in 'Sheet' - delete it first"), "{why}");

    // Selection is the same command as for every entity (it is not on the undo timeline).
    History::execute_ephemeral(&world, SelectEntity::new(&world, Some("plate")));
    assert_eq!(model::selected_names(&world), ["plate"]);
    History::execute_ephemeral(&world, SelectEntity::new(&world, None));
    assert!(model::selected_names(&world).is_empty());

    // Delete, undo, redo: the profile comes back with its drawing and its place.
    run(&world, &mut history, MoveSketch::new(&world, "plate", 5.0, 6.0, 30.0), "move");
    let (entity, before) = sketch::find(&world, "plate").expect("profile");
    let place = model::transform_of(&world, &entity);
    run(&world, &mut history, DeleteEntity::new(&world, "plate"), "delete");
    assert!(sketch::find(&world, "plate").is_none());
    assert!(history.undo(&world));
    let (entity, after) = sketch::find(&world, "plate").expect("undo restores the profile");
    assert_eq!(after, before);
    assert!(Arc::ptr_eq(&after.data, &before.data), "the drawing is shared, not copied");
    assert_eq!(model::transform_of(&world, &entity), place);
    assert!(history.redo(&world));
    assert!(sketch::find(&world, "plate").is_none());
    run(&world, &mut history, DeleteSetup::new(&world, "Sheet"), "the empty setup deletes");
}

#[test]
fn deleting_the_entity_frees_its_drawing() {
    let dir = tmp_dir("free");
    let (world, _history) = sheet_world();
    let cmd = ImportDxf::new(&world, None, &write_dxf(&dir, "plate.dxf", Some(4)), &DxfOptions::default()).expect("import");
    // Applied outside a history, so the entity is the drawing's only holder.
    History::execute_ephemeral(&world, cmd);
    let (entity, profile) = sketch::find(&world, "plate").expect("profile");
    let drawing = Arc::downgrade(&profile.data);
    drop(profile);
    assert!(drawing.upgrade().is_some(), "held by the entity");
    model::remove(&world, &entity);
    assert!(drawing.upgrade().is_none(), "deleting the entity frees the drawing");
}

#[test]
fn stock_source_word_is_reserved_for_explicit_profile_names_and_file_stems() {
    for spelling in ["stock", "STOCK", "sToCk"] {
        let (world, mut history) = sheet_world();
        let drawing = dxf_text(Some(4));
        let before = camcraft_app::project::save_text(&world).expect("save before refusal");
        let options = DxfOptions { name: Some(spelling.into()), ..Default::default() };
        let why = refusal(ImportDxf::from_bytes(&world, None, "plate.dxf", drawing.as_bytes(), &options), "reserved name");
        assert!(why.contains(spelling) && why.contains("not a usable name for a DXF profile"), "{why}");
        assert_eq!(camcraft_app::project::save_text(&world).expect("save after refusal"), before);

        // A reserved file stem uses a safe fallback, with the profile still addressable.
        let cmd = ImportDxf::from_bytes(&world, None, &format!("{spelling}.dxf"), drawing.as_bytes(), &DxfOptions::default())
            .expect("a reserved stem gets a safe name");
        let name = cmd.name().to_owned();
        assert_eq!(name, "profile");
        history.execute(&world, cmd);
        assert!(sketch::find(&world, &name).is_some());
        let pocket = operation::seed_params(OpType::Pocket.ordinal());
        run(&world, &mut history, CreateOperation::new(&world, "Pkt", TOOL, SrcRef::Entity(name), pocket), "profile operation");
        let why = refusal(GenerateOperation::new(&world, "Pkt"), "profile generation");
        assert!(why.contains("DXF profile 'profile'") && why.contains("is not generated yet"), "{why}");
    }
}

//! #1529: the Top/Bottom of Model heights stand on the PARTS, never on a
//! fixture. A built-in vise, a soft jaw, a dowel plate or a tagged mesh
//! fixture - in this setup or another - leaves both references at the
//! part's own bounds, so a default contour generates the same path with
//! or without workholding; the collision laws still see the fixture
//! (a pass into the vise is still struck at the gate).
//! Provenance: our own fixtures (a 50 x 50 x 6 block in a 55 x 55 x 11
//! box stock, the issue's reproduction); no vendor material consulted.
//!
//! Gate map (#1529): covers app::operation (heights), app::setup_align
//! (part_aabb_in), app::commands (InsertVise, InsertAccessory,
//! InsertPlate, TagFixture, CreateSetup, AlignPartToSetup), app::posting
//! (verification).

use flecs_ecs::prelude::World;

use camcraft_app::accessory::Accessory;
use camcraft_app::commands::{
    AccessoryPlacement, AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    InsertAccessory, InsertPlate, InsertVise, PlatePlacement, SetStock, TagFixture, VisePlacement,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::plate;
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::vise;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";
/// The part's Z span: the block stands Z 4..10 inside the Z 0..11 stock.
const PART_BOTTOM: f64 = 4.0;
const PART_TOP: f64 = 10.0;

fn at(x: f64, y: f64, z: f64) -> Location {
    Location { pos: Point3::new(x, y, z), rot: IDENTITY_Q }
}

fn contour(bottom_z: Height) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(20.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z,
        stepdown: 2.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The 50 x 50 x 6 block (Z 4..10), a 55 x 55 x 11 box stock, the tool
/// and a default-heights contour on the block ("Op"), not generated.
fn document(bottom_z: Height) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(2.5, 2.5, PART_BOTTOM), Point3::new(52.5, 52.5, PART_TOP))));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 55.0, dy: 55.0, dz: 11.0 }, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let op = CreateOperation::new(&world, "Op", TOOL, SrcRef::Entity("block".into()), contour(bottom_z));
    h.execute(&world, op.expect("op"));
    (world, h)
}

fn default_document() -> (World, History) {
    document(Height::of(HeightReference::BottomOfModel, 0.0))
}

/// (Top of Model, Bottom of Model) as `setup` resolves them.
fn model_refs(world: &World, setup: Option<&str>) -> (Option<f64>, Option<f64>) {
    let z = |r| operation::resolve_height(world, setup, None, Height::of(r, 0.0));
    (z(HeightReference::TopOfModel), z(HeightReference::BottomOfModel))
}

/// Generate "Op" and read its path's signature and lowest Z.
fn generated(world: &World, h: &mut History) -> (u64, f64) {
    h.execute(world, GenerateOperation::new(world, "Op").expect("generates"));
    let e = model::find_by_name(world, "Op").expect("op");
    let tp = model::toolpath_of(world, &e).expect("a path");
    let low = tp.path.moves.iter().flat_map(|m| [m.from.z, m.to.z]).fold(f64::INFINITY, f64::min);
    (tp.path_sig(), low)
}

/// The issue's reproduction: a 5 in vise whose jaw tops stand at Z 2,
/// under the part, its fixed jaw on the stock's +y face.
fn vise_below(world: &World, h: &mut History, setup: Option<&str>) -> String {
    let p = vise::size("5 in").expect("5 in").params(55.0);
    let cmd = InsertVise::new(world, &p, setup, VisePlacement::At(at(27.5, 55.0, 2.0 - p.jaw_height))).expect("vise");
    let name = cmd.name().to_owned();
    h.execute(world, cmd);
    name
}

#[test]
fn a_vise_under_the_part_leaves_bottom_of_model_at_the_part_and_the_path_unchanged() {
    let (bare, mut hb) = default_document();
    assert_eq!(model_refs(&bare, None), (Some(PART_TOP), Some(PART_BOTTOM)));
    let (sig, low) = generated(&bare, &mut hb);
    assert!((low - PART_BOTTOM).abs() < 1e-6, "the bare contour bottoms at the part: {low}");

    let (world, mut h) = default_document();
    vise_below(&world, &mut h, None);
    assert_eq!(model_refs(&world, None), (Some(PART_TOP), Some(PART_BOTTOM)), "the vise base is not the model bottom");
    assert_eq!(generated(&world, &mut h), (sig, low), "the vise changes nothing in the path");
}

#[test]
fn a_vise_fitted_to_the_stock_leaves_top_of_model_at_the_part_not_the_jaw_tops() {
    let (world, mut h) = default_document();
    let p = vise::size("5 in").expect("5 in").half_open();
    let cmd = InsertVise::new(&world, &p, None, VisePlacement::FitStock).expect("fits");
    let name = cmd.name().to_owned();
    h.execute(&world, cmd);
    let jaws = model::find_by_name(&world, &name).and_then(|e| model::entity_world_aabb(&world, &e)).expect("vise box");
    assert!(jaws.max.z > PART_TOP + 1.0, "the jaws stand over the part: {}", jaws.max.z);
    assert_eq!(model_refs(&world, None), (Some(PART_TOP), Some(PART_BOTTOM)), "the jaw tops are not the model top");
    let (_, low) = generated(&world, &mut h);
    assert!((low - PART_BOTTOM).abs() < 1e-6, "{low}");
}

#[test]
fn a_vise_declared_in_setup_two_does_not_move_setup_ones_heights() {
    let (bare, mut hb) = default_document();
    let want = generated(&bare, &mut hb);

    let (world, mut h) = default_document();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    vise_below(&world, &mut h, Some("Setup2"));
    for s in [None, Some("Setup1"), Some("Setup2")] {
        assert_eq!(model_refs(&world, s), (Some(PART_TOP), Some(PART_BOTTOM)), "setup {s:?}");
    }
    assert_eq!(generated(&world, &mut h), want, "Setup1's contour is the bare one");
}

#[test]
fn a_soft_jaw_a_plate_and_a_tagged_mesh_fixture_are_not_the_model() {
    let (world, mut h) = default_document();
    // A soft jaw standing 2 in tall from Z 0, over the part's top.
    let jaw = Accessory::default_of("soft_jaw").expect("soft jaw");
    h.execute(&world, InsertAccessory::new(&world, &jaw, None, AccessoryPlacement::At(at(27.5, 55.0, 0.0))).expect("soft jaw"));
    // A dowel plate under the stock (its frame's origin on its top face).
    let plate = plate::preset("300 x 300 mm").expect("preset").params();
    h.execute(&world, InsertPlate::new(&world, &plate, None, PlatePlacement::At(at(-122.5, -122.5, 0.0))).expect("plate"));
    // An imported clamp body, tagged a fixture: below and above the part.
    h.execute(&world, ImportModel::from_mesh("Riser", cube(Point3::new(-20.0, -20.0, -60.0), Point3::new(75.0, 75.0, -30.0))));
    h.execute(&world, TagFixture::new(&world, "Riser", None).expect("tag"));
    h.execute(&world, ImportModel::from_mesh("Clamp", cube(Point3::new(60.0, 0.0, 0.0), Point3::new(70.0, 10.0, 80.0))));
    h.execute(&world, TagFixture::new(&world, "Clamp", None).expect("tag"));
    assert_eq!(model_refs(&world, None), (Some(PART_TOP), Some(PART_BOTTOM)), "no fixture is the model");
    let (_, low) = generated(&world, &mut h);
    assert!((low - PART_BOTTOM).abs() < 1e-6, "{low}");
}

#[test]
fn the_collision_check_still_sees_the_vise() {
    // The contour taken 3 mm under the part runs to Z 1, past the jaw
    // tops at Z 2: the moving jaw stands against the -y wall's pass.
    // The no-vise control: the same pass with no fixture posts clean.
    let (bare, mut hb) = document(Height::of(HeightReference::BottomOfModel, -3.0));
    generated(&bare, &mut hb);
    let clean = posting::verification_gate(&bare);
    assert!(clean.is_empty(), "no vise, nothing to strike: {clean:?}");
    let (world, mut h) = document(Height::of(HeightReference::BottomOfModel, -3.0));
    vise_below(&world, &mut h, None);
    let (_, low) = generated(&world, &mut h);
    assert!((low - (PART_BOTTOM - 3.0)).abs() < 1e-6, "{low}");
    let problems = posting::verification_gate(&world);
    assert!(problems.iter().any(|p| p.starts_with("Op:") && p.contains("strikes a fixture")), "{problems:?}");
}

#[test]
fn the_collision_check_passes_the_bare_contour_with_the_vise_under_the_part() {
    // The no-vise control of the pin above: at Bottom of Model the
    // contour stops at Z 4, clear of the jaw tops at Z 2, so generate and
    // post agree - the verification gate holds nothing.
    let (world, mut h) = default_document();
    vise_below(&world, &mut h, None);
    let (_, low) = generated(&world, &mut h);
    assert!((low - PART_BOTTOM).abs() < 1e-6, "{low}");
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_tilted_setup_reads_the_part_in_its_own_frame_with_a_vise_present() {
    // Setup1 tilted 90 degrees about X stands the block on its side: the
    // 50 mm side is now the Z span (-18..32), not the world's 4..10. The
    // vise changes nothing in that frame.
    let (world, mut h) = default_document();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", [90.0, 0.0, 0.0]).expect("align"));
    let tilted = model_refs(&world, Some("Setup1"));
    let (Some(top), Some(bottom)) = tilted else { panic!("both references resolve: {tilted:?}") };
    assert!((top - bottom - 50.0).abs() < 1e-6, "the 50 mm side stands up: {bottom}..{top}");
    assert!((top - 32.0).abs() < 1e-6 && (bottom + 18.0).abs() < 1e-6, "Z -18..32: {bottom}..{top}");
    vise_below(&world, &mut h, Some("Setup1"));
    assert_eq!(model_refs(&world, Some("Setup1")), tilted, "the vise is not the tilted part");
}

#[test]
fn with_only_fixtures_left_the_model_heights_refuse_no_model_loaded() {
    // The block tagged a fixture plus a vise: no part is left, so both
    // references stay unresolved - never a fallback to the fixtures' box.
    let (world, mut h) = default_document();
    vise_below(&world, &mut h, None);
    h.execute(&world, TagFixture::new(&world, "block", None).expect("tag"));
    assert_eq!(model_refs(&world, None), (None, None));
    for r in [HeightReference::TopOfModel, HeightReference::BottomOfModel] {
        assert_eq!(operation::missing_source(&world, None, None, r), Some("no model loaded"), "{r:?}");
    }
    let err = match GenerateOperation::new(&world, "Op") {
        Ok(_) => panic!("generate must refuse with no part"),
        Err(e) => format!("{e}"),
    };
    assert!(err.contains("unresolved: no model loaded"), "{err}");
}

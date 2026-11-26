//! #3452: the issue's case through the document and the lathe post. A
//! Ø30 pin Z-70..0 with a 1 x 45° back chamfer, a Ø34 bar run on to
//! Z-84, a Part op at the bottom of the model (Z-70) with the 3 mm
//! GT-0300 blade on orientation 2 (the -Z corner). The kerf stands
//! wholly behind the part's end face, Z-73..-70: every posted feed is
//! at Z-73 (before the fix: Z-71.5, a kerf Z-71.5..-68.5 that took
//! 1.5 mm and the chamfer off the part), and the part keeps its 70 mm.
//! The +Z corner (orientation 1) posts the face itself, Z-70.
//!
//! Gate map (M4.4): covers app::operation (the Part arm and its kerf
//! verdict), app::posting (the blade-corner shift), app::commands.
//! Lane shape: seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, ChangeTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History,
    ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::setup_turn::TurnPivot;
use camcraft_app::tools::catalog;
use camcraft_app::turning::{PartKept, PartKerf};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const GT: &str = "GT-0300";
const W: f64 = 3.0;
/// The issue's pin: Ø30, Z-70..0, a 1 x 45° chamfer at the back face.
const PIN: [(f64, f64); 5] = [(0.0, -70.0), (14.0, -70.0), (15.0, -69.0), (15.0, 0.0), (0.0, 0.0)];
const END_FACE: f64 = -70.0;
/// The pin with its grip stub in the model: Z-84..0, the part Z-70..0.
const STUBBED: [(f64, f64); 4] = [(0.0, -84.0), (15.0, -84.0), (15.0, 0.0), (0.0, 0.0)];
/// The OP20 end-for-end flip preset (180 degrees about X; the angles
/// `setup_align_lathe.rs` pins).
const FLIP: [f64; 3] = [180.0, 0.0, 90.0];
/// The turn that stands a model drawn along document X up the spindle
/// (the angles `setup_align_lathe.rs` pins), and the same turned end for
/// end.
const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const TURN_FLIPPED: [f64; 3] = [-90.0, 0.0, 90.0];

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The lathe, the Ø34 bar Z-84..0, the GT-0300 blade on `corner`, the
/// model `meridian` as "pin", and - with `hold` - one setup that holds
/// the part as modelled, flipped end for end, spun, or turned. The Part
/// op "P1" at `plane` is created, not generated.
fn lathe_world(meridian: &[(f64, f64)], corner: Option<u8>, hold: Hold, plane: f64) -> (World, History) {
    lathe_world_kerf(meridian, corner, hold, plane, PartKerf::default())
}

/// [`lathe_world`] with the Part op's kerf setting (#3469).
fn lathe_world_kerf(meridian: &[(f64, f64)], corner: Option<u8>, hold: Hold, plane: f64, kerf: PartKerf) -> (World, History) {
    lathe_world_of_kerf(revolve(meridian, 32), corner, hold, plane, kerf)
}

/// How the one setup holds the part.
#[derive(Clone, Copy)]
enum Hold {
    /// No setup alignment: as modelled.
    AsModelled,
    /// The OP20 end-for-end flip preset.
    Flip,
    /// A turn about the spindle axis only.
    Spin,
    /// The given angles, about the WCS origin (a model drawn lying down
    /// stood up the spindle, as `setup_align_lathe.rs` does).
    Turn([f64; 3]),
}

/// A model drawn lying down along document X (the `unturn` of
/// `setup_align_lathe.rs`): (x, y, z) -> (y, z, x).
fn lying(m: Mesh) -> Mesh {
    let mv = |p: Point3| Point3::new(p.y, p.z, p.x);
    Mesh::from_facets(m.facets.iter().map(|t| Triangle::new(mv(t.a), mv(t.b), mv(t.c))).collect())
}

fn lathe_world_of(mesh: Mesh, corner: Option<u8>, hold: Hold, plane: f64) -> (World, History) {
    lathe_world_of_kerf(mesh, corner, hold, plane, PartKerf::default())
}

/// [`lathe_world_of`] with the Part op's kerf setting (#3469).
fn lathe_world_of_kerf(mesh: Mesh, corner: Option<u8>, hold: Hold, plane: f64, kerf: PartKerf) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let lathe = Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let mut blade = catalog().into_iter().find(|(n, _)| *n == GT).map(|(_, t)| t).expect("catalog blade");
    assert!((blade.width - W).abs() < 1e-12, "the fixture's blade is 3 mm wide");
    history.execute(&world, AddTool::new(&world, GT, blade.clone()).expect("blade"));
    blade.orientation = corner;
    history.execute(&world, ChangeTool::new(&world, GT, blade).expect("a sane corner"));
    history.execute(&world, ImportModel::from_mesh("pin", mesh));
    if !matches!(hold, Hold::AsModelled) {
        history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
        let cmd = match hold {
            Hold::Flip => AlignPartToSetup::angles(&world, "Setup1", FLIP),
            Hold::Spin => AlignPartToSetup::angles(&world, "Setup1", [0.0, 0.0, 90.0]),
            Hold::Turn(deg) => AlignPartToSetup::turn(&world, "Setup1", deg, TurnPivot::WcsOrigin),
            Hold::AsModelled => return (world, history),
        };
        history.execute(&world, cmd.expect("align"));
    }
    let at = Location { pos: Point3::new(0.0, 0.0, -84.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 34.0, height: 84.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let part = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        // The top only has to stand over the plane (the kernel reads it
        // as a number only).
        top_z: Height::fixed(if plane < 0.0 { 0.0 } else { plane + 5.0 }),
        bottom_z: Height::fixed(plane),
        stepdown: 2.0,
        feed: 120.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
        kerf,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "P1", GT, SrcRef::Entity("pin".into()), part).expect("create");
    history.execute(&world, cmd);
    (world, history)
}

fn pin_world(corner: Option<u8>) -> (World, History) {
    let (world, mut history) = lathe_world(&PIN, corner, Hold::AsModelled, END_FACE);
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    let e = model::find_by_name(&world, "P1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "the part-off generates");
    (world, history)
}

/// The generation refusal of "P1", or a panic if it generated.
fn refusal(world: &World) -> String {
    let e = model::find_by_name(world, "P1").expect("op");
    let op = model::operation_of(world, &e).expect("operation");
    operation::generate(world, &op, "P1").map(|_| ()).expect_err("the part-off is refused").to_string()
}

/// The posted feed moves' Z (the corner the blade was touched off on).
fn posted_feed_z(world: &World) -> Vec<f64> {
    posting::stream_records(world)
        .expect("posts")
        .into_iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) if m.kind != MoveKind::Rapid => Some(m.z),
            _ => None,
        })
        .collect()
}

#[test]
fn a_part_off_at_the_end_face_keeps_the_whole_part() {
    // Orientation 2: the posted Z is the -Z corner, the kerf runs W
    // up from it.
    let (world, _h) = pin_world(Some(2));
    let zs = posted_feed_z(&world);
    assert!(!zs.is_empty(), "the part-off posts feeds");
    for z in &zs {
        let (lo, hi) = (*z, *z + W);
        assert!(hi <= END_FACE + 1e-6, "the kerf Z{lo}..Z{hi} enters the part past its end face Z{END_FACE}");
        assert!((lo + 73.0).abs() < 1e-6, "the -Z corner posts at Z{lo}, wanted Z-73");
    }
    let kept = 0.0 - zs.iter().map(|z| z + W).fold(f64::NEG_INFINITY, f64::max);
    assert!((kept - 70.0).abs() < 1e-6, "the part comes off {kept} mm long, wanted the model's 70");
    let records = posting::stream_records(&world).expect("posts");
    let text = engine::run(LATHE_CPS, &posting::machine_value(&world), &cnc::serialize(&records))
        .expect("the lathe script runs")
        .program;
    assert!(text.contains("X0. Z-73. "), "the blade reaches the axis at Z-73: {text}");
    assert!(!text.contains("Z-71.5 "), "never the old centred kerf: {text}");
}

#[test]
fn the_plus_z_corner_posts_the_end_face_itself() {
    // Orientation 1 (+Z corner): the same kerf, written at its +Z
    // flank - on the face, never past it.
    let (world, _h) = pin_world(Some(1));
    let zs = posted_feed_z(&world);
    assert!(!zs.is_empty());
    assert!(zs.iter().all(|z| (z - END_FACE).abs() < 1e-6), "the +Z corner posts Z-70: {zs:?}");
}

/// Move the parting plane of "P1" (one undoable edit).
fn set_plane(world: &World, history: &mut History, plane: f64) {
    let e = model::find_by_name(world, "P1").expect("op");
    let mut params = model::operation_of(world, &e).expect("operation").params;
    if let OpParams::Part { top_z, bottom_z, .. } = &mut params {
        *top_z = Height::fixed(plane.max(0.0) + 100.0);
        *bottom_z = Height::fixed(plane);
    }
    history.execute(world, EditOperationParams::new(world, "P1", params).expect("edit"));
}

/// Generate "P1" and read its posted feed Z.
fn generated_feed_z(world: &World, history: &mut History) -> Vec<f64> {
    history.execute(world, GenerateOperation::new(world, "P1").expect("generate"));
    let e = model::find_by_name(world, "P1").expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok, "the part-off generates");
    posted_feed_z(world)
}

fn all_at_73(zs: &[f64], what: &str) {
    assert!(!zs.is_empty() && zs.iter().all(|z| (z + 73.0).abs() < 1e-6), "{what}: the -Z corner posts Z-73: {zs:?}");
}

#[test]
fn a_flipped_setup_refuses_the_part_off_by_name() {
    // The stubbed pin flipped end for end: the finished part (Z-84..-14
    // in the setup) is in the chuck, the stub at the free end. The chuck
    // side rule would put the kerf Z-17..-14 into the part, so the
    // part-off is refused before a move exists.
    let (world, _h) = lathe_world(&STUBBED, Some(2), Hold::Flip, -14.0);
    let why = refusal(&world);
    for want in ["cannot run here", "on both sides", "holds the part end for end", "'Chuck piece'"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
    // A part-only model in the flipped setup, parted at its +Z end: the
    // whole model stands on the chuck side - refused as before.
    let (world, _h) = lathe_world(&PIN, Some(2), Hold::Flip, 0.0);
    let why = refusal(&world);
    assert!(why.contains("stands wholly on the chuck side"), "{why}");
}

#[test]
fn a_setup_as_modelled_parts_the_stubbed_pin_behind_the_end_face() {
    // The same stubbed pin, held as modelled, plane at the part's end
    // face Z-70 inside the model: the kerf is Z-73..-70.
    let (world, mut history) = lathe_world(&STUBBED, Some(2), Hold::AsModelled, END_FACE);
    all_at_73(&generated_feed_z(&world, &mut history), "as modelled");
    // A turn about the spindle axis only is still as modelled.
    let (world, mut history) = lathe_world(&STUBBED, Some(2), Hold::Spin, END_FACE);
    all_at_73(&generated_feed_z(&world, &mut history), "spun about the spindle");
}

#[test]
fn a_part_modelled_lying_down_parts_only_at_its_end() {
    // Drawn along document X and stood up the spindle (TURN): the part
    // alone, parted at its end face, keeps the model - it generates.
    let (world, mut history) = lathe_world_of(lying(revolve(&PIN, 32)), Some(2), Hold::Turn(TURN), END_FACE);
    all_at_73(&generated_feed_z(&world, &mut history), "lying pin under TURN");
    // With its grip stub, the plane is inside the model: nothing says
    // which end of a lying model is free - under TURN and under TURN
    // turned end for end alike.
    for deg in [TURN, TURN_FLIPPED] {
        let (world, mut history) = lathe_world_of(lying(revolve(&STUBBED, 32)), Some(2), Hold::Turn(deg), END_FACE);
        // The plane 14 mm above the model's setup-frame bottom: the
        // stub/part seam under TURN, inside the model under both turns.
        let bottom = setup_align::part_aabb_in(&world, None).expect("the model in the setup").min.z;
        set_plane(&world, &mut history, bottom + 14.0);
        let why = refusal(&world);
        assert!(why.contains("modelled lying down"), "{deg:?} (model bottom Z{bottom}): {why}");
    }
}

// ---- #3469: the kept piece and the back stock ----

fn kerf_of(world: &World) -> PartKerf {
    let e = model::find_by_name(world, "P1").expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Part { kerf, .. } => kerf,
        other => panic!("a Part op expected, got {other:?}"),
    }
}

#[test]
fn a_flipped_setup_keeping_the_chuck_piece_cuts_the_stub_off_the_free_end() {
    // The stubbed pin flipped end for end: the part Z-84..-14 is in the
    // chuck, the stub Z-14..0 at the free end. Keeping the chuck piece
    // puts the kerf Z-14..-11, on the free-end side of the plane.
    let kerf = PartKerf { kept: PartKept::Chuck, back_stock: 0.0 };
    let (world, mut history) = lathe_world_kerf(&STUBBED, Some(2), Hold::Flip, -14.0, kerf);
    let e = model::find_by_name(&world, "P1").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    let made = operation::generate(&world, &op, "P1").map(|_| ());
    assert!(made.is_ok(), "the stub comes off: {made:?}");
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "the part-off generates");
    let zs = posted_feed_z(&world);
    assert!(!zs.is_empty(), "the part-off posts feeds");
    for z in &zs {
        assert!((z + 14.0).abs() < 1e-6, "the -Z corner posts the part's end face Z-14, got Z{z}: {zs:?}");
    }
}

#[test]
fn a_back_stock_posts_the_kerf_that_far_into_the_bar() {
    // The #3452 case with 0.5 mm back stock: the kerf is Z-73.5..-70.5,
    // so orientation 2 posts Z-73.5 and the part comes off 70.5 mm long.
    let kerf = PartKerf { kept: PartKept::CutOff, back_stock: 0.5 };
    let (world, mut history) = lathe_world_kerf(&PIN, Some(2), Hold::AsModelled, END_FACE, kerf);
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    let e = model::find_by_name(&world, "P1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "the part-off generates");
    let zs = posted_feed_z(&world);
    assert!(!zs.is_empty() && zs.iter().all(|z| (z + 73.5).abs() < 1e-6), "the -Z corner posts Z-73.5: {zs:?}");
}

#[test]
fn the_chuck_piece_with_nothing_in_the_chuck_is_refused_by_name() {
    // The #3452 pin stands wholly above its end face Z-70: keeping the
    // chuck piece parts nothing, and the refusal says which setting.
    let kerf = PartKerf { kept: PartKept::Chuck, back_stock: 0.0 };
    let (world, _h) = lathe_world_kerf(&PIN, Some(2), Hold::AsModelled, END_FACE, kerf);
    let why = refusal(&world);
    for want in ["nothing to part", "free-end side (higher Z)", "'Cut-off piece'"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
}

#[test]
fn the_kerf_setting_is_an_undoable_edit_and_rides_the_project_file() {
    let (world, mut history) = lathe_world(&PIN, Some(2), Hold::AsModelled, END_FACE);
    assert_eq!(kerf_of(&world), PartKerf::default(), "a new part-off keeps the cut-off piece, no back stock");
    // The default writes neither key: an unchanged op saves the same file.
    let p = std::env::temp_dir().join(format!("camcraft-3469-{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(!saved.contains("\"kept\"") && !saved.contains("\"back_stock\""), "the default writes no key");
    // A negative back stock is refused when the command is made.
    let e = model::find_by_name(&world, "P1").expect("op");
    let mut bad = model::operation_of(&world, &e).expect("operation").params;
    if let OpParams::Part { kerf, .. } = &mut bad {
        kerf.back_stock = -0.5;
    }
    assert!(EditOperationParams::new(&world, "P1", bad).is_err(), "a negative back stock is not a size");
    // The edit, its undo and redo.
    let want = PartKerf { kept: PartKept::Chuck, back_stock: 0.25 };
    let mut next = model::operation_of(&world, &e).expect("operation").params;
    if let OpParams::Part { kerf, .. } = &mut next {
        *kerf = want;
    }
    history.execute(&world, EditOperationParams::new(&world, "P1", next).expect("a sane edit"));
    assert_eq!(kerf_of(&world), want);
    assert!(history.undo(&world));
    assert_eq!(kerf_of(&world), PartKerf::default(), "Ctrl+Z restores the default");
    assert!(history.redo(&world));
    assert_eq!(kerf_of(&world), want, "Ctrl+Y reapplies it");
    // The stated setting rides the file and reads back whole.
    camcraft_app::project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(saved.contains("\"kept\": \"chuck\"") && saved.contains("\"back_stock\": 0.25"), "{saved}");
    let world2 = new_world();
    camcraft_app::project::open(&world2, &saved).expect("reopen");
    assert_eq!(kerf_of(&world2), want, "the file keeps the setting");
    let _ = std::fs::remove_file(&p);
}

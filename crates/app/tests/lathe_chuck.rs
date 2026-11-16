//! Issue #1442 at the app level: a lathe chuck generated as a built-in
//! accessory is workholding the lathe holder law reads. A generated 3-jaw
//! chuck gripping the part's r 12 end strikes an offset holder whose leg
//! reaches its jaws - blocking the post - and the same holder short of
//! them posts; a chuck fitted on the setup's round stock closes on its
//! diameter at its underside; every refusal is named; one undo each.
//!
//! Gate map (M4.4): covers camcraft_app::accessory::chuck,
//! commands::accessory (the OnStock placement).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (the #1037 suite's stepped part and
//! offset holder); no vendor material consulted.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::accessory::{Accessory, JawChuck};
use camcraft_app::commands::{
    AccessoryPlacement, CommandError, CreateOperation, GenerateOperation, History, ImportModel, InsertAccessory, LatheBodySpec,
    LoadMachine, SelectPost, SetStock, SetToolBody,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_app::world::new_world;
use camcraft_tools::lathe_holder::LathePlacement;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";

fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// The #1037 offset holder: its leg's foot `reach` below the tip.
fn offset_holder(reach: f64) -> LatheBodySpec {
    let mut facets = cuboid([2.0, -6.0, -1.0], [24.0, 6.0, 10.0]);
    facets.extend(cuboid([16.0, -6.0, -reach], [24.0, 6.0, -1.0]));
    let place = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
    LatheBodySpec { source: "offset.step".into(), facets, place, zone: None }
}

/// A small 3-jaw chuck gripping the r 12 end: its face at z -16 (the
/// part's underside), the jaws standing to z -10 out to r 30.
fn small_chuck() -> Accessory {
    Accessory::ThreeJawChuck(JawChuck {
        body_diameter: 80.0,
        body_length: 30.0,
        bore_diameter: 20.0,
        grip_diameter: 24.0,
        jaw_length: 18.0,
        jaw_width: 8.0,
        jaw_height: 6.0,
    })
}

/// The #1037 lathe document - the stepped part (r 12 z -16..0, r 6 to
/// 20), the insert on the rack - held in the generated chuck.
fn chucked() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, INSERT);
    let part = revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", part));
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), rot: IDENTITY_Q };
    let cmd = InsertAccessory::new(&world, &small_chuck(), None, AccessoryPlacement::At(at)).expect("a sane chuck");
    assert_eq!(cmd.name(), "3-jaw chuck");
    history.execute(&world, cmd);
    (world, history)
}

fn finish(world: &World, history: &mut History) {
    let params = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let op = CreateOperation::new(world, "F1", INSERT, SrcRef::Entity("step".into()), params).expect("create");
    history.execute(world, op);
    history.execute(world, GenerateOperation::new(world, "F1").expect("generate"));
}

fn holder_hits(world: &World) -> Vec<usize> {
    let c = cache(world, "F1").expect("cached");
    c.warnings.iter().filter_map(|w| match *w {
        Warning::FixtureHit { at, fixture_part: FixturePart::Holder } => Some(at),
        _ => None,
    }).collect()
}

fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("camcraft-chuck-1442-{tag}-{}.nc", std::process::id()));
    let r = posting::post_setup(world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    r
}

#[test]
fn a_generated_chuck_is_steel_the_holder_law_reads() {
    let (world, mut history) = chucked();
    let e = model::find_by_name(&world, "3-jaw chuck").expect("placed");
    let def = fixture::def_of(&world, &e).expect("a fixture");
    assert!(!def.fixed, "a chuck turns with the part");
    // The envelope as placed: body z -46..-16, jaws to z -10, r 40 drawn
    // with its flats on the circle.
    let b = model::entity_world_aabb(&world, &e).expect("box");
    assert!((b.min.z + 46.0).abs() < 1e-9 && (b.max.z + 10.0).abs() < 1e-9, "{b:?}");
    assert!(b.max.x >= 40.0 && b.max.x < 40.1, "{b:?}");
    // The leg's foot 15 below the tip: z -13 at the ride's foot, in the
    // jaws' z -16..-10 band at r 22.8..30.8, inside their r 30 reach.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(15.0))).expect("a sane holder"));
    finish(&world, &mut history);
    let hits = holder_hits(&world);
    assert!(!hits.is_empty(), "the holder in the chuck's jaws went unnamed");
    let e = post(&world, "jaws").expect_err("a holder in the chuck posted");
    assert!(e.contains("post blocked") && e.contains(&format!("holder at move {} strikes a fixture", hits[0])), "{e}");
}

#[test]
fn the_same_holder_above_the_jaws_posts() {
    let (world, mut history) = chucked();
    // The foot 10 below the tip: z -8 at the ride's foot, 2 over the jaws.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(10.0))).expect("a sane holder"));
    finish(&world, &mut history);
    assert!(holder_hits(&world).is_empty(), "{:?}", cache(&world, "F1").map(|c| c.warnings));
    post(&world, "clear").expect("the clear holder posts");
}

fn invalid(r: Result<InsertAccessory, CommandError>) -> String {
    r.err().map(|e| e.to_string()).unwrap_or_default()
}

#[test]
fn a_chuck_fits_on_round_stock_and_refuses_by_name() {
    let world = new_world();
    let mut history = History::default();
    let three = Accessory::default_of("three_jaw_chuck").expect("kind");
    // No stock yet: named.
    let why = invalid(InsertAccessory::new(&world, &three, None, AccessoryPlacement::OnStock));
    assert!(why.contains("no stock"), "{why}");
    // Square stock is not round about a vertical spindle.
    let shape = StockShape::Box { dx: 30.0, dy: 20.0, dz: 40.0 };
    history.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    let why = invalid(InsertAccessory::new(&world, &three, None, AccessoryPlacement::OnStock));
    assert!(why.contains("only round stock"), "{why}");
    // A 30 mm bar: the jaws close on 30 at its underside, on its axis.
    let shape = StockShape::Cylinder { diameter: 30.0, height: 40.0 };
    let at = Location { pos: Point3::new(5.0, -3.0, -20.0), ..Location::default() };
    history.execute(&world, SetStock::new(&world, shape, at).expect("bar"));
    let stock = model::stock_world_aabb_for(&world, None).expect("stock box");
    let cmd = InsertAccessory::new(&world, &three, None, AccessoryPlacement::OnStock).expect("fits");
    let Some(Accessory::ThreeJawChuck(c)) = cmd.params().cloned() else { panic!("a 3-jaw chuck") };
    assert!((c.grip_diameter - 30.0).abs() < 1e-6, "{c:?}");
    let pos = cmd.location().pos;
    let (cx, cy) = (0.5 * (stock.min.x + stock.max.x), 0.5 * (stock.min.y + stock.max.y));
    assert!((pos.x - cx).abs() < 1e-9 && (pos.y - cy).abs() < 1e-9 && (pos.z - stock.min.z).abs() < 1e-9, "{pos:?} vs {stock:?}");
    history.execute(&world, cmd);
    // A collet chuck takes the bar its nose length deep.
    let collet = Accessory::default_of("collet_chuck").expect("kind");
    let cmd = InsertAccessory::new(&world, &collet, None, AccessoryPlacement::OnStock).expect("fits");
    let Some(Accessory::ColletChuck(k)) = cmd.params().cloned() else { panic!("a collet chuck") };
    assert!((k.collet_bore - 30.0).abs() < 1e-6 && (cmd.location().pos.z - (stock.min.z + k.nose_length)).abs() < 1e-9);
    // A bar the collet nose cannot take, a bar the jaws cannot reach
    // past the body, and a kind that is no chuck: named.
    let small = Accessory::ColletChuck(camcraft_app::accessory::ColletChuck { nose_diameter: 25.0, ..k });
    let why = invalid(InsertAccessory::new(&world, &small, None, AccessoryPlacement::OnStock));
    assert!(why.contains("Collet chuck on the stock") && why.contains("collet bore"), "{why}");
    let Accessory::ThreeJawChuck(c) = three.clone() else { panic!("kind") };
    let tight = Accessory::ThreeJawChuck(JawChuck { body_diameter: 90.0, ..c });
    let why = invalid(InsertAccessory::new(&world, &tight, None, AccessoryPlacement::OnStock));
    assert!(why.contains("past the body"), "{why}");
    let block = Accessory::default_of("block").expect("kind");
    let why = invalid(InsertAccessory::new(&world, &block, None, AccessoryPlacement::OnStock));
    assert!(why.contains("only a lathe chuck"), "{why}");
    // One undo takes the chuck back.
    assert!(model::find_by_name(&world, "3-jaw chuck").is_some());
    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, "3-jaw chuck").is_none());
}

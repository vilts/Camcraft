//! #3083: on a mill-turn, the mill kinds answer to the first-traverse
//! laws as on a mill. A mill-turn posts a milling tool through the mill
//! post, which sends Z to the clearance height, then X/Y at that height:
//! after a tool change that X/Y traverse can cross any point of the
//! setup. A steady rest over a face's clearance plane holds the post by
//! name; the same steady under the plane posts the bare program; a 3+2
//! plane on the mill-turn's B reads the stock's top along its tilted
//! axis.
//!
//! Provenance: our own fixtures; the shop rule "never traverse through
//! uncut stock or steel" (machine-tool handbooks). No vendor material.
//!
//! Gate map (#3083): covers app::operation (first_traverse, generate,
//! stamp), app::posting, sim::verify (first_traverse).
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SetFixtureFixed, SetStock, TagFixture,
};
use camcraft_app::geometry::{cube, IDENTITY_Q, Point3};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

const CLEAR: f64 = 30.0;

/// A mill-turn: X, Y, Z, the spindle's C about Z and a B head that
/// holds a 3+2 lean.
fn mill_turn() -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), ..Default::default() },
            AxisLimits { name: "B".into(), rotary: true, axis: Some([0.0, 1.0, 0.0]), travel: Some((-120.0, 120.0)), ..Default::default() },
        ],
        ..Default::default()
    }
}

fn face(clear: f64, tilt: Option<Tilt>, cut: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt,
        blend_radius: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(clear),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// The mill-turn mounted, a `dx` x `dx` x `dz` block at `at`, a 4 mm end mill.
fn world(dx: f64, dz: f64, at: Point3) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("machine"));
    assert_eq!(camcraft_app::machine::kind_of_setup(&world, None), camcraft_app::machine::Kind::MillTurn);
    let loc = Location { pos: at, rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy: dx, dz }, loc).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", t).expect("tool"));
    (world, h)
}

fn generate(world: &World, h: &mut History, p: OpParams) -> (OpStatus, Toolpath, Vec<Warning>) {
    h.execute(world, CreateOperation::new(world, "Face1", "EM-0400-2FL", SrcRef::Stock, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Face1").expect("generate"));
    let e = model::find_by_name(world, "Face1").expect("op");
    let cached = model::toolpath_of(world, &e).expect("cached path");
    (camcraft_app::operation::status_of(world, &e), cached.path.clone(), cached.warnings.clone())
}

/// A steady rest `top` mm tall, standing still far off the stock and
/// off every move.
fn far_steady(world: &World, h: &mut History, top: f64) {
    let steady = model::spawn(world, "Steady1");
    model::set_facets(world, &steady, cube(Point3::new(150.0, 150.0, 0.0), Point3::new(160.0, 160.0, top)).facets.to_vec().into());
    model::set_rev(world, &steady, 1);
    h.execute(world, TagFixture::new(world, "Steady1", None).expect("tagged"));
    h.execute(world, SetFixtureFixed::new(world, "Steady1", true).expect("declared fixed"));
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft_first_traverse_mt_{tag}_{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let r = posting::post_setup(world, &out).map(|p| p.program);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    if r.is_err() {
        assert!(!wrote, "{tag}: a refused post writes nothing");
    }
    r
}

fn low(warns: &[Warning]) -> Option<(usize, f64, f64, bool)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, .. } => Some((at, height_mm, top_mm, fixture)),
        _ => None,
    })
}

#[test]
fn a_mill_turn_face_under_a_far_steady_holds_the_post_by_name() {
    let (world, mut h) = world(40.0, 10.0, Point3::new(60.0, 40.0, 0.0));
    far_steady(&world, &mut h, 40.0);
    let (status, path, warns) = generate(&world, &mut h, face(CLEAR, None, 9.0));
    assert_eq!(status, OpStatus::Ok);
    let m = path.moves[0];
    assert!((m.from.x - 150.0).abs() > 20.0 && m.from.x < 100.0, "the path stays far from the steady: {m:?}");
    assert_eq!(low(&warns), Some((0, CLEAR, 40.0, true)), "{warns:?}");
    let err = posting::post_setup_unwritten(&world).expect_err("the program must not post");
    for text in ["Face1", "clearance height Z30.000", "under the fixture top at Z40.000", "raise the clearance height above Z40.000"] {
        assert!(err.contains(text), "{text}: {err}");
    }
    assert!(post(&world, "far_steady").is_err());
}

#[test]
fn a_mill_turn_face_over_a_low_steady_posts_the_bare_program() {
    let (world, mut h) = world(40.0, 10.0, Point3::new(60.0, 40.0, 0.0));
    let (_, bare_path, bare_warns) = generate(&world, &mut h, face(CLEAR, None, 9.0));
    assert!(bare_warns.is_empty(), "{bare_warns:?}");
    let bare = post(&world, "bare").unwrap_or_else(|e| panic!("the bare program posts: {e}"));
    far_steady(&world, &mut h, 25.0);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    assert!(cached.warnings.is_empty(), "{:?}", cached.warnings);
    assert_eq!(cached.path.moves, bare_path.moves);
    let steadied = post(&world, "low_steady").unwrap_or_else(|e| panic!("the program posts over the low steady: {e}"));
    assert_eq!(steadied, bare, "a low steady changed the program");
}

/// A 60 x 60 x 20 block (x -30..30) indexed B 10 deg about its top
/// centre on the mill-turn's B: along the tilted axis its high corner
/// stands 30 sin 10 = 5.209 mm over the plane's zero. A clearance 5 mm
/// over that zero (Z 25) is under it and refused; Z 30 is clear. The
/// verdict gives the heights in the user's frame (#2991): the clearance
/// as set, and the corner over the origin Z 20.
#[test]
fn a_mill_turn_3_2_plane_reads_the_stock_top_along_its_tool_axis() {
    let tilt = Tilt { basis_version: 1, a_deg: 0.0, b_deg: 10.0, origin: [0.0, 0.0, 20.0] };
    let top = 30.0 * 10f64.to_radians().sin();
    for (clear, refused) in [(25.0, true), (30.0, false)] {
        let (world, mut h) = world(60.0, 20.0, Point3::new(0.0, 0.0, 0.0));
        let (status, path, warns) = generate(&world, &mut h, face(clear, Some(tilt), 19.5));
        assert_eq!(status, OpStatus::Ok);
        if refused {
            let (at, h_mm, top_mm, fixture) = low(&warns).unwrap_or_else(|| panic!("clearance Z{clear}: no verdict in {warns:?}; the path opens {:?}", path.moves.first()));
            assert_eq!((at, fixture), (0, false));
            assert!((h_mm - clear).abs() < 1e-6, "the clearance height the user set: {h_mm}");
            assert!((top_mm - (top + 20.0)).abs() < 1e-6, "the tilted block's high corner: {top_mm}, want {}", top + 20.0);
            assert!(posting::post_setup_unwritten(&world).is_err());
        } else {
            assert_eq!(low(&warns), None, "clearance Z{clear}: {warns:?}");
        }
    }
}

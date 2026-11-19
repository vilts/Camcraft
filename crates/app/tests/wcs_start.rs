//! #2382: every operation starts from its setup's WCS, never the world
//! origin. A part and its stock stand away from the world origin, the
//! setup's G54 sits on the stock's front-left top corner, and a 2D, a 3D
//! and a hole operation are generated. Each path opens over the WCS
//! origin (or its first hole) on the clearance plane; no move reaches
//! the world origin; and the posted X/Y words measure from G54.
//!
//! Gate map (#2382): covers app::operation (generate), app::posting,
//! cl::path (seat_approach), motion::machining2d, motion::finishing3d,
//! motion::drilling, post::post.
//!
//! Provenance: our own fixture (common::solid_part); the work-offset
//! reading of RS-274 / ISO 6983 (G54 program zero). No vendor material.

mod common;

use common::solid_part::{circle, plate, rect};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting};

/// The part's centre: far from the world origin in X and Y.
const C: (f64, f64) = (60.0, 40.0);
/// G54 on the stock's front-left top corner.
const G54: [f64; 3] = [40.0, 20.0, 10.5];
const CLEAR: f64 = 30.0;

/// A 40 x 40 plate, z 0..10, centred on `C`, with a Ø8 through hole.
fn holed_plate() -> Vec<Triangle> {
    let mut f = Vec::new();
    let hole = circle(C, 4.0, 48);
    plate(&rect(C.0 - 20.0, C.1 - 20.0, C.0 + 20.0, C.1 + 20.0), Some((&hole, 0.0)), 0.0, 10.0, &mut f);
    f
}

fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(holed_plate())));
    let at = Location { pos: Point3::new(C.0, C.1, 0.0), rot: IDENTITY_Q };
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.5 }, at).expect("stock");
    h.execute(&world, stock);
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    assert!((b.min.x - G54[0]).abs() < 1e-9 && (b.min.y - G54[1]).abs() < 1e-9 && (b.max.z - G54[2]).abs() < 1e-9, "{b:?}");
    let g54 = Location { pos: Point3::new(G54[0], G54[1], G54[2]), rot: IDENTITY_Q };
    h.execute(&world, SetWcsOrigin::new(&world, g54).expect("G54 on the stock corner"));
    for name in ["DR-0600", "EM-0400-2FL", "BN-0600"] {
        let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    }
    (world, h)
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(CLEAR),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn finish() -> OpParams {
    OpParams::Finish3D {
        pattern: FinishPattern::Parallel,
        stepover: Some(2.0),
        tolerance: 0.05,
        clearance_z: Height::fixed(CLEAR),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(0.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        fill_openings: None,
    }
}

fn drill() -> OpParams {
    let hole = Hole { xy: Point2::new(C.0, C.1), radius: 4.0, entry_z: 10.0, floor_z: 0.0, through: true, stepped: false };
    OpParams::Drill {
        clearance_z: Height::fixed(CLEAR),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 100.0,
        holes: vec![hole],
        cross: Vec::new(),
        break_through: None,
        break_through_from: Default::default(),
    }
}

fn run(world: &World, h: &mut History, name: &str, tool: &str, src: SrcRef, p: OpParams) -> Toolpath {
    h.execute(world, CreateOperation::new(world, name, tool, src, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name}");
    model::toolpath_of(world, &e).expect("cached path").path.clone()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// No move of the path starts or ends at the world origin's X/Y.
fn never_at_world_origin(name: &str, t: &Toolpath) {
    for (i, m) in t.moves.iter().enumerate() {
        for p in [m.from, m.to] {
            assert!(!(near(p.x, 0.0) && near(p.y, 0.0)), "{name}: move {i} {:?} reaches the world origin", m.kind);
        }
    }
}

/// The opening rapid runs on the clearance plane out of `start`.
fn opens_at(name: &str, t: &Toolpath, start: (f64, f64)) {
    let m = t.moves.first().unwrap_or_else(|| panic!("{name}: an empty path"));
    assert_eq!(m.kind, MoveKind::Rapid, "{name}: the path opens with a rapid");
    assert!(near(m.from.x, start.0) && near(m.from.y, start.1), "{name}: the approach starts at {:?}, want {start:?}", m.from);
    assert!(near(m.from.z, CLEAR), "{name}: the approach starts on the clearance plane, {:?}", m.from);
}

#[test]
fn a_2d_operation_starts_over_the_wcs_origin() {
    let (world, mut h) = world();
    let t = run(&world, &mut h, "Face1", "EM-0400-2FL", SrcRef::Stock, face());
    opens_at("Face1", &t, (G54[0], G54[1]));
    never_at_world_origin("Face1", &t);
}

#[test]
fn a_3d_operation_starts_over_the_wcs_origin() {
    let (world, mut h) = world();
    let t = run(&world, &mut h, "Finish1", "BN-0600", SrcRef::Entity("plate".into()), finish());
    opens_at("Finish1", &t, (G54[0], G54[1]));
    never_at_world_origin("Finish1", &t);
}

#[test]
fn a_hole_operation_starts_over_its_first_hole() {
    let (world, mut h) = world();
    let t = run(&world, &mut h, "Drill1", "DR-0600", SrcRef::Entity("plate".into()), drill());
    opens_at("Drill1", &t, C);
    never_at_world_origin("Drill1", &t);
}

/// Every X / Y word of a posted line, as numbers.
fn words(line: &str, letter: char) -> Vec<f64> {
    line.split_whitespace()
        .filter_map(|w| w.strip_prefix(letter))
        .filter_map(|v| v.parse::<f64>().ok())
        .collect()
}

#[test]
fn the_posted_program_measures_from_g54_and_never_visits_the_world_origin() {
    let (world, mut h) = world();
    let face = run(&world, &mut h, "Face1", "EM-0400-2FL", SrcRef::Stock, face());
    run(&world, &mut h, "Finish1", "BN-0600", SrcRef::Entity("plate".into()), finish());
    run(&world, &mut h, "Drill1", "DR-0600", SrcRef::Entity("plate".into()), drill());
    let out = std::env::temp_dir().join(format!("camcraft_wcs_start_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let program = posted.unwrap_or_else(|e| panic!("the program posts: {e}")).program;
    assert!(program.contains("G54"), "{program}");
    // The world origin is X-40 Y-20 in G54: no block goes there.
    for line in program.lines() {
        let (xs, ys) = (words(line, 'X'), words(line, 'Y'));
        assert!(xs.iter().all(|x| *x > -20.0) && ys.iter().all(|y| *y > -20.0), "a block measured from the world: {line}\n{program}");
    }
    // The face's first X/Y block is its opening rapid's end, less G54.
    let to = face.moves[0].to;
    let (x, y) = (to.x - G54[0], to.y - G54[1]);
    let first = program
        .lines()
        .find(|l| !words(l, 'X').is_empty() && !words(l, 'Y').is_empty())
        .unwrap_or_else(|| panic!("an X/Y block: {program}"));
    assert!(near(words(first, 'X')[0], x) && near(words(first, 'Y')[0], y), "first X/Y block {first}, want X{x} Y{y}");
}

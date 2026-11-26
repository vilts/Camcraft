//! #2382 / #2901: every operation measures from its setup's WCS, and its
//! path opens over its own entry point. A part and its stock stand away
//! from the world origin, the setup's G54 sits on the stock's front-left
//! top corner (or on its centre), and a 2D, a 3D and a hole operation
//! are generated. Each path opens with a rapid on the clearance plane
//! over its own entry (or its first hole) - never out of the WCS origin
//! or the frame zero, a point the machine never visits after a tool
//! change (#2901 reverses #2382's seat over the WCS origin). The path is
//! the same for the two WCS positions; no move reaches the world origin;
//! the posted X/Y words measure from G54; and a clamp that stands on the
//! old origin-to-entry line gets no struck-move verdict and no routed
//! link (only the first-traverse law reads it).
//!
//! Gate map (#2382, #2901): covers app::operation (generate), app::posting,
//! cl::path (seat_approach), motion::machining2d, motion::finishing3d,
//! motion::drilling, post::post.
//!
//! Provenance: our own fixture (common::solid_part); the work-offset
//! reading of RS-274 / ISO 6983 (G54 program zero). No vendor material.

mod common;

use common::solid_part::{circle, plate, rect};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin, TagFixture};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};

/// The part's centre: far from the world origin in X and Y.
const C: (f64, f64) = (60.0, 40.0);
/// G54 on the stock's front-left top corner.
const G54: [f64; 3] = [40.0, 20.0, 10.5];
/// G54 on the stock's top centre: the other usual touch-off.
const G54_CENTRE: [f64; 3] = [C.0, C.1, 10.5];
const CLEAR: f64 = 30.0;

/// A 40 x 40 plate, z 0..10, centred on `C`, with a Ø8 through hole.
fn holed_plate() -> Vec<Triangle> {
    let mut f = Vec::new();
    let hole = circle(C, 4.0, 48);
    plate(&rect(C.0 - 20.0, C.1 - 20.0, C.0 + 20.0, C.1 + 20.0), Some((&hole, 0.0)), 0.0, 10.0, &mut f);
    f
}

fn world() -> (World, History) {
    world_at(G54)
}

fn world_at(g54: [f64; 3]) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(holed_plate())));
    let at = Location { pos: Point3::new(C.0, C.1, 0.0), rot: IDENTITY_Q };
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.5 }, at).expect("stock");
    h.execute(&world, stock);
    let b = model::stock_world_aabb_for(&world, None).expect("stock box");
    assert!((b.min.x - G54[0]).abs() < 1e-9 && (b.min.y - G54[1]).abs() < 1e-9 && (b.max.z - G54[2]).abs() < 1e-9, "{b:?}");
    let g54 = Location { pos: Point3::new(g54[0], g54[1], g54[2]), rot: IDENTITY_Q };
    h.execute(&world, SetWcsOrigin::new(&world, g54).expect("G54 set"));
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
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(CLEAR),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn finish() -> OpParams {
    OpParams::Finish3D {
        pattern: FinishPattern::Parallel,
        direction_angle: 0.0,
        pass_start: camcraft_app::machining2d::PassStart::Auto,
        stepover: Some(2.0),
        tolerance: 0.05,
        clearance_z: Height::fixed(CLEAR),
        retract_z: None,
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
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
        waterlines: true,
        picks: Default::default(),
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
        picks: Default::default(),
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

/// #2901: the opening rapid starts on the clearance plane over its own
/// end - no X/Y travel - and it does not start at the WCS origin unless
/// the operation enters there.
fn opens_over_its_entry(name: &str, t: &Toolpath, wcs: [f64; 3]) {
    let m = t.moves.first().unwrap_or_else(|| panic!("{name}: an empty path"));
    assert_eq!(m.kind, MoveKind::Rapid, "{name}: the path opens with a rapid");
    assert!(m.arc.is_none(), "{name}: the approach is a straight rapid");
    assert!(near(m.from.x, m.to.x) && near(m.from.y, m.to.y), "{name}: the approach travels in X/Y: {:?} -> {:?}", m.from, m.to);
    assert!(near(m.from.z, CLEAR), "{name}: the approach starts on the clearance plane, {:?}", m.from);
    assert!(m.to.z <= CLEAR + 1e-6, "{name}: the approach does not climb over the clearance plane, {:?}", m.to);
    let entry_at_origin = near(m.to.x, wcs[0]) && near(m.to.y, wcs[1]);
    assert!(entry_at_origin || !(near(m.from.x, wcs[0]) && near(m.from.y, wcs[1])), "{name}: the approach starts at the WCS origin {:?}", m.from);
}

/// The two WCS positions give the same path relative to the part: the
/// WCS is only the reference of the posted words.
fn same_path(name: &str, corner: &Toolpath, centre: &Toolpath) {
    assert_eq!(corner.moves, centre.moves, "{name}: the WCS position changed the path");
}

fn both(name: &str, tool: &str, src: SrcRef, p: fn() -> OpParams) -> (Toolpath, Toolpath) {
    let (world, mut h) = world_at(G54);
    let corner = run(&world, &mut h, name, tool, src.clone(), p());
    let (world, mut h) = world_at(G54_CENTRE);
    let centre = run(&world, &mut h, name, tool, src, p());
    opens_over_its_entry(name, &corner, G54);
    opens_over_its_entry(name, &centre, G54_CENTRE);
    same_path(name, &corner, &centre);
    never_at_world_origin(name, &corner);
    (corner, centre)
}

#[test]
fn a_2d_operation_starts_over_its_entry_with_the_wcs_at_the_corner_or_the_centre() {
    both("Face1", "EM-0400-2FL", SrcRef::Stock, face);
}

#[test]
fn a_3d_operation_starts_over_its_entry_with_the_wcs_at_the_corner_or_the_centre() {
    both("Finish1", "BN-0600", SrcRef::Entity("plate".into()), finish);
}

#[test]
fn a_hole_operation_starts_over_its_first_hole() {
    let (corner, _) = both("Drill1", "DR-0600", SrcRef::Entity("plate".into()), drill);
    let m = corner.moves[0];
    assert!(near(m.from.x, C.0) && near(m.from.y, C.1), "Drill1: the approach starts at {:?}, want {C:?}", m.from);
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

/// G54 away from the stock, on the far side of a clamp: the old seat
/// drew a rapid from here to the entry, through the clamp.
const G54_OFF: [f64; 3] = [C.0 + 60.0, C.1, 10.5];

/// #2901: a clamp taller than the clearance plane stands on the line
/// from the WCS origin to the face's entry. The machine never travels
/// that line: the bare program has no `X0 Y0` block, and with the clamp
/// there is no fixture verdict on that line and no routed lift - the
/// path is the bare path. The clamp still stands over the clearance
/// plane, and the first traverse comes from an unknown X/Y, so the
/// first-traverse law (not a struck move) holds the post by name.
#[test]
fn a_clamp_on_the_old_origin_to_entry_line_is_judged_by_the_first_traverse_law_only() {
    let (world, mut h) = world_at(G54_OFF);
    let t = run(&world, &mut h, "Face1", "EM-0400-2FL", SrcRef::Stock, face());
    opens_over_its_entry("Face1", &t, G54_OFF);
    let out = std::env::temp_dir().join(format!("camcraft_wcs_start_clamp_{}.nc", std::process::id()));
    let bare = posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("the bare program posts: {e}")).program;
    let _ = std::fs::remove_file(&out);
    // `G28` / `G53` blocks go to the machine home, not the WCS origin.
    for line in bare.lines().filter(|l| !l.contains("G28") && !l.contains("G53")) {
        let (xs, ys) = (words(line, 'X'), words(line, 'Y'));
        assert!(!(xs.iter().any(|v| near(*v, 0.0)) && ys.iter().any(|v| near(*v, 0.0))), "a block goes to the WCS origin: {line}
{bare}");
    }

    // The clamp: on the old line, past the stock's +X edge (x 80) and
    // the face's overrun, standing over the clearance plane.
    let e = t.moves[0].to;
    let x = C.0 + 33.0;
    let k = (x - G54_OFF[0]) / (e.x - G54_OFF[0]);
    let y = G54_OFF[1] + (e.y - G54_OFF[1]) * k;
    assert!(k > 0.0 && k < 1.0, "the clamp stands between the origin and the entry: {e:?}");
    let clamp = model::spawn(&world, "Clamp1");
    model::set_facets(&world, &clamp, cube(Point3::new(x - 3.0, y - 3.0, 0.0), Point3::new(x + 3.0, y + 3.0, 40.0)).facets.to_vec().into());
    model::set_rev(&world, &clamp, 1);
    h.execute(&world, TagFixture::new(&world, "Clamp1", None).expect("tagged"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let op = model::find_by_name(&world, "Face1").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&world, &op), OpStatus::Ok);
    let cached = model::toolpath_of(&world, &op).expect("cached path");
    let first = Warning::FirstTraverseLow { at: 0, height_mm: CLEAR, top_mm: 40.0, fixture: true, tilted: false };
    assert_eq!(cached.warnings, vec![first], "no fixture verdict on a move the machine never makes");
    assert_eq!(cached.path.moves, t.moves, "the clamp routed a link");
    let err = posting::post_setup_unwritten(&world).expect_err("the clamp over the plane holds the post");
    assert!(err.contains("Face1") && err.contains("raise the clearance height above Z40.000"), "{err}");
}

/// #2901: a cache saved under the old law (stamp salt v3) holds the
/// invented rapid from the WCS origin, and the fixture checks judged
/// it. The salt moved (v3 -> v4): the cache reads Stale, the post
/// refuses it by name, and a regenerate opens over the entry.
///
/// The fixture `fixtures/opening_seat_v3_cache.camproj` is our own: the
/// corner-G54 Face1 of this file, saved under salt v3, with the cached
/// opening rapid's start put back on the G54 corner (40, 20) - what the
/// #2382 seat wrote.
#[test]
fn a_cache_saved_with_the_origin_seat_reads_stale_until_regenerated() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opening_seat_v3_cache.camproj");
    let world = camcraft_app::world::new_world();
    camcraft_app::project::load(&world, &path).expect("the recorded project loads");
    let op = model::find_by_name(&world, "Face1").expect("op");
    let first = model::toolpath_of(&world, &op).expect("the cache rides the file").path.moves[0];
    assert!(near(first.from.x, G54[0]) && near(first.from.y, G54[1]), "the fixture holds the old seat: {first:?}");
    assert_eq!(camcraft_app::operation::status_of(&world, &op), OpStatus::Stale, "an old-law cache reads fresh");
    let err = posting::post_setup_unwritten(&world).expect_err("an old-law cache must not post");
    assert!(err.contains("Face1") && err.contains("regenerate"), "{err}");

    let mut h = History::default();
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(camcraft_app::operation::status_of(&world, &op), OpStatus::Ok);
    let t = model::toolpath_of(&world, &op).expect("path").path.clone();
    opens_over_its_entry("Face1", &t, G54);
}

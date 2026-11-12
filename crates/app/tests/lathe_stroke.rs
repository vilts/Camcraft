//! The built-in Lathe and Mill-turn strokes (#493): travel bands are
//! read in PROGRAM coordinates through the work offset (`app::machine`
//! module doc; the convention #457 / PR #477 states for the mills), so
//! a slide of stroke L along the spindle is stated `-L..L`. The shipped
//! `-615..0` / `-620..0` Z bands were machine-frame numbers read as
//! program ones: every clearance in front of the face (program Z > 0)
//! was "out of stroke" and no turning program posted. X is measured
//! from the spindle centreline, which IS the program's X0, so its band
//! is already a program-frame band - but the built-in Lathe's floor
//! sits a little PAST centre (#630): a facing pass that faces clean to
//! centre puts the nose centre on X0 and the posted control point
//! (orientation 2) at -r, and the slide must reach it. Pinned here: a
//! face-and-turn job with its clearance in front of the face passes on
//! both built-ins, a facing pass's control point to centre fits the
//! Lathe's band, and a genuine over-travel still refuses by name.
//! Since #647 a lathe is judged on the POSTED path's meridian radius
//! (the kernels put the radius on Y; the post writes the control
//! point): a control point past the floor or an OD past the ceiling
//! refuses by name, one on the band edge passes.
//!
//! Provenance: general lathe practice (Z0 on the finished face, the
//! part in -Z, X from the centreline); every number is our own fixture.
//!
//! Gate map (M4.4): covers app::machine, post::machine, app::posting, app::commands.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, check_document};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TAU: f64 = std::f64::consts::TAU;
const TOOL: &str = "TN-1600-0R8";

/// A closed revolve of `meridian` (radius, z), 16 ways.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// A flanged bar with its finished face on Z0 (G54 unplaced = the
/// world origin), the part in -Z: flange r10 from z -40 to -20, neck
/// r7 to the face. Stock is the bar's envelope plus 1 mm to face off
/// in front of Z0.
const BAR: [(f64, f64); 6] = [(0.0, -40.0), (10.0, -40.0), (10.0, -20.0), (7.0, -20.0), (7.0, 0.0), (0.0, 0.0)];

fn generate(world: &World, history: &mut History, name: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, TOOL, SrcRef::Entity("bar".into()), params);
    history.execute(world, cmd.expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name} generates");
}

/// Face and rough-turn the bar on the built-in `machine_name`, every
/// clearance at `clearance` in front of the face.
fn job_on(machine_name: &str, clearance: f64) -> World {
    job_load(clearance, |w| LoadMachine::builtin(w, machine_name).expect("built-in machine"))
}

/// The face-and-turn job on the machine `load` mounts.
fn job_load(clearance: f64, load: impl FnOnce(&World) -> LoadMachine) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -40.0), ..Location::default() };
    let stock = SetStock::new(&world, StockShape::Cylinder { diameter: 20.0, height: 41.0 }, at);
    h.execute(&world, stock.expect("bar stock"));
    let cmd = load(&world);
    h.execute(&world, cmd);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR)));
    let facing = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(clearance),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut h, "Face1", facing);
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(clearance),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(-20.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut h, "Rough1", rough);
    world
}

fn mounted(world: &World, name: &str) -> machine::Machine {
    let (path, m) = machine::of(world);
    assert_eq!(path, posting::builtin_path(name), "the built-in is the document machine");
    m.expect("the built-in is mounted")
}

fn passes_in_front_of_the_face(name: &str, stroke: f64) {
    let world = job_on(name, 5.0);
    let m = mounted(&world, name);
    let z = m.axis(2).and_then(|a| a.travel).expect("Z stroke");
    assert_eq!(z, (-stroke, stroke), "{name}: the Z slide, full stroke each way");
    let c = check_document(&world, &m).expect("a fresh program checks");
    assert!(c.is_ok(), "{name}: clearance Z 5 in front of the face: {:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{name}: the gate is clean: {:?}", report.problems);
    let path = std::env::temp_dir().join(format!("camcraft_lathe_stroke_{stroke:.0}_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let posted = posted.unwrap_or_else(|e| panic!("{name} must post a face-and-turn job: {e}"));
    assert!(!posted.program.is_empty());
}

fn overtravel_refuses_by_name(name: &str, stroke: f64) {
    // A clearance more than the whole stroke in front of a face the
    // program also cuts at Z0 fits no placement of the work offset.
    let clearance = stroke + 20.0;
    let world = job_on(name, clearance);
    let m = mounted(&world, name);
    let c = check_document(&world, &m).expect("checks");
    let want = format!("in Z, the stroke is {:.3}..{:.3}", -stroke, stroke);
    // A lathe judges the POSTED path (#647): the orientation-2 control
    // point stands r = 0.8 below the rough pass's nose centre in Z. The
    // Mill-turn keeps the cached (nose-centre) reading.
    let reach = if name == "Lathe" { clearance - 0.8 } else { clearance };
    assert!(
        c.problems.iter().any(|p| p.starts_with("Rough1:") && p.contains(&format!("reaches {reach:.3}")) && p.contains(&want)),
        "{name}: {:?}",
        c.problems
    );
    assert!(!c.problems.iter().any(|p| p.contains(" in X")), "{name}: X stays inside 0..R: {:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.iter().any(|p| p.contains("the stroke is")), "{name}: {:?}", report.problems);
    let path = std::env::temp_dir().join(format!("camcraft_lathe_overtravel_{stroke:.0}_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "{name}: an over-travel never posts");
}

#[test]
fn a_turning_job_on_the_built_in_lathe_clears_the_face_and_posts() {
    passes_in_front_of_the_face("Lathe", 615.0);
}

#[test]
fn a_turning_job_on_the_built_in_millturn_clears_the_face_and_posts() {
    passes_in_front_of_the_face("Mill-turn", 620.0);
}

#[test]
fn a_clearance_past_the_lathe_carriage_still_refuses_by_name() {
    overtravel_refuses_by_name("Lathe", 615.0);
}

#[test]
fn a_clearance_past_the_millturn_quill_still_refuses_by_name() {
    overtravel_refuses_by_name("Mill-turn", 620.0);
}

/// The built-in Lathe's X floor (#630), in the checker's reading of a
/// lathe X (#647): the posted path's signed radius from the centreline.
/// -2.5 radius is -5 on diameter: room for the control point of a
/// nose up to r2.5 faced to centre (the catalog's largest is r0.8).
const LATHE_X_FLOOR: f64 = -2.5;

#[test]
fn lathe_x_stays_a_centreline_band() {
    // X is measured from the spindle centreline, the program's own X0.
    // The Lathe's floor reaches a little past centre so a facing pass's
    // control point can (#630); the Mill-turn's band is unchanged.
    for (name, band) in [("Lathe", (LATHE_X_FLOOR, 320.0)), ("Mill-turn", (0.0, 650.0))] {
        let world = job_on(name, 5.0);
        let x = mounted(&world, name).axis(0).and_then(|a| a.travel).expect("X stroke");
        assert_eq!(x, band, "{name}");
    }
}

#[test]
fn a_real_x_overtravel_past_centre_still_refuses_by_name() {
    // The work offset 10 mm toward +X puts the bar's centreline at
    // X-10 (Ø-20) in program terms - far past the small floor. No
    // placement turns that into a pass: the band leaves it out, the
    // check refuses the facing op by name, and nothing posts.
    let world = job_on("Lathe", 5.0);
    let m = mounted(&world, "Lathe");
    let (lo, _) = m.axis(0).and_then(|a| a.travel).expect("X stroke");
    assert!(-10.0 < lo, "X-10 (Ø-20) is outside the band's floor {lo}");
    let mut h = History::default();
    let at = Location { pos: Point3::new(10.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("G54 shifted"));
    let refused = match check_document(&world, &m) {
        Err(e) => e.starts_with("Face1:") || e.starts_with("Rough1:"),
        Ok(c) => c.problems.iter().any(|p| p.starts_with("Face1:") && p.contains(" in X")),
    };
    assert!(refused, "the centreline at X-10 is refused by name");
    let path = std::env::temp_dir().join(format!("camcraft_lathe_x_overtravel_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "an X over-travel never posts");
}

#[test]
fn a_facing_pass_to_centre_fits_the_built_in_lathe_x_band() {
    // Facing to centre: the nose centre ends on X0, the posted control
    // point of the orientation-2 TN-1600-0R8 at -r = -0.8 (X-1.6 on
    // diameter). Every posted point must sit inside the Lathe's band,
    // and the program checks clean and posts.
    let world = job_on("Lathe", 5.0);
    let m = mounted(&world, "Lathe");
    let (lo, hi) = m.axis(0).and_then(|a| a.travel).expect("X stroke");
    let e = model::find_by_name(&world, "Face1").expect("facing op");
    let op = model::operation_of(&world, &e).expect("operation");
    let tool = model::find_by_name(&world, TOOL).and_then(|t| model::tool_of(&world, &t)).expect("tool");
    let cached = model::toolpath_of(&world, &e).expect("cache").path;
    let (posted, q) = posting::nose::posted_path(&op.params, TOOL, &tool, &cached, [0.0; 3]).expect("posts");
    assert_eq!(q, Some(2), "the default turning orientation");
    // The path's meridian is one world axis (X or Y) about world Z; the
    // other stays 0, so the signed radius the post writes is their sum.
    let radius = |p: Point3| p.x + p.y;
    let min_x = posted.moves.iter().map(|mv| radius(mv.to)).fold(f64::INFINITY, f64::min);
    assert!((2.0 * min_x + 1.6).abs() < 1e-6, "the control point faces to X-1.6 on diameter, got {}", 2.0 * min_x);
    for mv in &posted.moves {
        let x = radius(mv.to);
        assert!(x >= lo && x <= hi, "X {x:.3} outside {lo}..{hi}");
    }
    let c = check_document(&world, &m).expect("checks");
    assert!(c.is_ok(), "a facing pass to centre checks clean: {:?}", c.problems);
}

// #647: the stroke check judges the POSTED radius. The turning kernels
// state their radius on the Y meridian, which world X never saw, and
// the post writes the control point, not the cached nose centre.

/// The built-in Lathe with its X band rewritten to `lo..hi`, mounted.
fn lathe_with_x(lo: f64, hi: f64, clearance: f64) -> World {
    let text = posting::MACHINES.iter().find(|(n, _)| *n == "Lathe").map(|(_, t)| *t).expect("built-in Lathe");
    let edited = text.replacen("\"min\": -2.5, \"max\": 320.0", &format!("\"min\": {lo:?}, \"max\": {hi:?}"), 1);
    assert_ne!(edited, text, "fixture: the X band was rewritten");
    let m = machine::parse_mch(&edited).expect("the edited lathe parses");
    job_load(clearance, |w| LoadMachine::new(w, "lathe-x.mch".into(), Some(m)).expect("edited lathe"))
}

fn x_problems(world: &World) -> Vec<String> {
    let (_, m) = machine::of(world);
    let c = check_document(world, &m.expect("a mounted machine")).expect("checks");
    c.problems.into_iter().filter(|p| p.contains(" in X")).collect()
}

/// The fixture's premise: `op`'s cached path rides the Y meridian, so
/// world X alone never sees its radius.
fn on_the_y_meridian(world: &World, op: &str) {
    let e = model::find_by_name(world, op).expect("op");
    let path = model::toolpath_of(world, &e).expect("cache").path;
    let pts = || path.moves.iter().flat_map(|m| [m.from, m.to]);
    assert!(pts().all(|p| p.x.abs() < 1e-9), "{op}: world X is 0 throughout");
    assert!(pts().any(|p| p.y.abs() > 1.0), "{op}: the radius rides Y");
}

#[test]
fn a_facing_control_point_below_the_x_floor_is_refused_by_name() {
    // Floor -0.5: the nose centre (X0) fits, the posted control point
    // (-0.8) does not - the posted point decides, and nothing posts.
    let world = lathe_with_x(-0.5, 320.0, 5.0);
    on_the_y_meridian(&world, "Face1");
    let p = x_problems(&world);
    assert!(
        p.iter().any(|l| l.starts_with("Face1:") && l.contains("reaches -0.800 in X, the stroke is -0.500..320.000")),
        "{p:?}"
    );
    let path = std::env::temp_dir().join(format!("camcraft_lathe_floor_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "an X over-travel past the floor never posts");
}

#[test]
fn a_facing_control_point_on_the_x_floor_passes() {
    let world = lathe_with_x(-0.8, 320.0, 5.0);
    let p = x_problems(&world);
    assert!(p.is_empty(), "the control point at the band edge fits: {p:?}");
}

#[test]
fn a_turning_pass_beyond_the_x_ceiling_is_refused_by_name() {
    // The bar is r10; an X ceiling of 9 is short of every OD pass.
    let world = lathe_with_x(-2.5, 9.0, 5.0);
    on_the_y_meridian(&world, "Rough1");
    let p = x_problems(&world);
    assert!(
        p.iter().any(|l| l.starts_with("Rough1:") && l.contains(" in X, the stroke is -2.500..9.000")),
        "{p:?}"
    );
}

//! The verification gate's stock-sim clause, made visible: a program
//! the material sim refuses to judge (a rotary cut, posed 3+2 moves,
//! no stock) posts on its generator-time verdicts WITH a named
//! "stock sim did not judge this program" line on the post result,
//! and blocks when no generator verdict covers it; a program the sim
//! judges posts exactly as before, with no line at all.
//! Provenance: our own fixtures (the operations suite's turned bar,
//! the tilt suite's parked pocket, the posting suite's contour); the
//! law is the Prime Directive's "correct, or visibly refused".
//!
//! Gate map (M4.4): covers app::posting, app::stock_sim, sim::stock_sim, app::script.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn stock_at(world: &World, history: &mut History, shape: StockShape, pos: Point3) {
    let loc = Location { pos, rot: IDENTITY_Q };
    history.execute(world, SetStock::new(world, shape, loc).expect("stock"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn silent(world: &World, name: &str) -> bool {
    model::find_by_name(world, name)
        .and_then(|e| model::toolpath_of(world, &e))
        .is_some_and(|c| c.warnings.is_empty())
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-notjudged-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// Revolve a meridian about world z (closed by its rho-0 fans) - the
/// operations suite's turned-bar construction.
fn revolve(n: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let pt = |rho: f64, z: f64, j: usize| {
        let th = std::f64::consts::TAU * j as f64 / n as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut f = Vec::new();
    for w in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let k = (j + 1) % n;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

fn lathe_mesh() -> Mesh {
    Mesh::from_facets(revolve(
        16,
        &[(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)],
    ))
}

/// The operations suite's rotary contour over the turned bar, with a
/// 6 mm ball and one fresh RC1 whose generator verdict is SILENT. No
/// stock entity: over the suite's box stock the generator's shank law
/// convicts the rotary ride (its cache then blocks the post on its own
/// WARN records, which is not what this suite tests). The sim refuses
/// on the rotary clause before it ever asks for a stock.
fn rotary_world() -> World {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    let params = OpParams::RotaryContour {
        tolerance: 0.1,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(14.0),
        bottom_z: Height::fixed(-14.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "RC1", "BN-0600", SrcRef::Entity("lathe".into()), params).expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "RC1").expect("generate"));
    // The letters need a machine that leans (the default VMC refuses
    // a tilted move by name - a different gate, not this one).
    mount(&world, &mut history, "tilthead.mch");
    // ...with G54 above the bar, so the program's Z reads below the
    // table plane (the tilt suite's parked-anchor reasoning, done with
    // the work offset; the -500..500 band of #457 holds either way).
    let g54 = Location { pos: Point3::new(0.0, 0.0, 40.0), rot: IDENTITY_Q };
    history.execute(&world, SetWcsOrigin::new(&world, g54).expect("place G54"));
    assert_eq!(status(&world, "RC1"), OpStatus::Ok, "fixture: fresh");
    assert!(
        silent(&world, "RC1"),
        "fixture: the generator verdict is clean: {:?}",
        model::find_by_name(&world, "RC1").and_then(|e| model::toolpath_of(&world, &e)).map(|c| c.warnings)
    );
    world
}

#[test]
fn a_rotary_program_posts_with_the_visible_not_judged_line() {
    let world = rotary_world();
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "covered by the generator verdict: {:?}", report.problems);
    let line = report.not_judged.clone().expect("the refusal is visible, never dropped");
    assert!(line.starts_with(posting::NOT_JUDGED), "{line}");
    assert!(line.contains("RC1") && line.contains("rotary program"), "named reason: {line}");
    assert_eq!(posting::verification_gate(&world), report.problems, "one gate, two readings");

    let path = tmp_nc("rotary");
    let posted = posting::post_setup(&world, &path).expect("the rotary program posts on its generator verdict");
    assert_eq!(posted.not_judged.as_deref(), Some(line.as_str()), "the post result carries it");
    assert!(path.exists());
    let text = std::fs::read_to_string(&path).expect("written");
    assert_eq!(text, posted.program, "the line rides the report, never the NC text");
    let _ = std::fs::remove_file(&path);
}

/// The tilt suite's parked 90-degree pocket, over a stock this time,
/// on the tilthead that owns the letters: every cutting move leans.
fn tilted_world() -> World {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, -20.0), Point3::new(40.0, 30.0, -10.0))),
    );
    stock_at(&world, &mut history, StockShape::Box { dx: 40.0, dy: 30.0, dz: 10.0 }, Point3::new(20.0, 15.0, -20.0));
    let params = OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { a_deg: 0.0, b_deg: 90.0, origin: [-25.0, 0.0, -15.0] }),
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(80.0),
        retract_z: Some(Height::fixed(70.0)),
        top_z: Height::fixed(50.0),
        bottom_z: Height::fixed(35.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    history.execute(
        &world,
        // The stock is the volume to remove (it coincides with the part
        // box): a whole-PART source would keep the part and refuse
        // (issue #12).
        CreateOperation::new(&world, "P", "EM-0600-4FL", SrcRef::Stock, params).expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "P").expect("generate"));
    mount(&world, &mut history, "tilthead.mch");
    world
}

fn mount(world: &World, history: &mut History, file: &str) {
    let asset = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines").join(file);
    history.execute(world, LoadMachine::from_file(world, &asset).expect("mount"));
}

#[test]
fn a_posed_program_is_refused_by_name_never_cut_upright() {
    let world = tilted_world();
    assert_eq!(status(&world, "P"), OpStatus::Ok, "fixture: fresh");
    let err = camcraft_app::stock_sim::simulate_document(&world)
        .expect_err("the column verdict must not judge a leaning tool as upright");
    assert!(err.contains("P: move") && err.contains("posed moves are not modelled by the column engine"), "{err}");
    let report = posting::verification_report(&world);
    let line = report.not_judged.expect("visible");
    assert!(line.starts_with(posting::NOT_JUDGED) && line.contains("posed moves"), "{line}");
    assert!(silent(&world, "P"), "fixture: the generator verdict is clean");
    let path = tmp_nc("posed");
    let posted = posting::post_setup(&world, &path).expect("posts on its generator verdict");
    assert!(posted.not_judged.is_some_and(|l| l.contains("posed moves")));
    let _ = std::fs::remove_file(&path);
}

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The operations suite's face fixture: 60x60x20 stock, a 4 mm end
/// mill, one Face1 - generated or not, as the test asks.
fn face_world(generate: bool) -> World {
    let world = new_world();
    let mut history = History::default();
    stock_at(&world, &mut history, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Point3::ZERO);
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params()).expect("create"),
    );
    if generate {
        history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    }
    world
}

#[test]
fn a_judged_program_posts_exactly_as_before_with_no_line() {
    let world = face_world(true);
    camcraft_app::stock_sim::simulate_document(&world).expect("fixture: the sim judges it");
    let report = posting::verification_report(&world);
    assert_eq!(report, posting::GateReport::default(), "judged and clean: nothing to say");
    let path = tmp_nc("judged");
    let posted = posting::post_setup(&world, &path).expect("posts");
    assert!(posted.not_judged.is_none());
    let text = std::fs::read_to_string(&path).expect("written");
    assert_eq!(text, posted.program, "the file is the program, untouched");
    assert!(!text.contains(posting::NOT_JUDGED));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn not_judged_with_no_generator_verdict_blocks() {
    // An op that never generated: the sim refuses (nothing to walk)
    // AND no generator verdict exists - two missing judges never add
    // up to a pass. (The post itself refuses earlier, by name; this is
    // the gate read standalone, the way the script API reads it.)
    let world = face_world(false);
    let problems = posting::verification_gate(&world);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with(posting::NOT_JUDGED) && problems[0].contains("nothing judged it"),
        "{}",
        problems[0]
    );
}

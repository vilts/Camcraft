//! #388: setups on different machines and posts. Every law judging an
//! operation reads its OWN setup's mount, so an inactive setup's op
//! generates against its own machine without being activated, and a
//! program whose setups mount different machines or posts posts as one
//! file per mount - each byte-identical to posting that setup alone,
//! each judged against its own machine.
//! Provenance: our own fixtures over our own commands; no vendor material.
//!
//! Gate map (#388): covers app::setup_mount (program_mounts),
//! app::posting (post_mounts, post_mounts_script, post_per_op,
//! machine_checks, verification_report, post_setup), app::machine, post::machine
//! (kind_of_setup, check_ops), app::operation (job, spindle_law),
//! app::commands (family gate per setup), app::script (cam.post files).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History,
    ImportModel, SetSetupMachine, SetSetupPost, SetStock, SetToolNumber,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, AxisLimits, Coolant, Kind, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, script};

const MILL_TOOL: &str = "EM-0400-2FL";
const LATHE_TOOL: &str = "TN-1600-0R8";
const TAU: f64 = std::f64::consts::TAU;

/// The turning suites' stage-A bar, its face 2 mm under the bar stock's
/// end (z 16) so the facing pass cuts stock, never the part.
const BAR: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 14.0), (0.0, 14.0)];

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

/// A lathe stating what its post reads (the lathe suites' bench lathe).
fn lathe() -> Machine {
    Machine {
        model: "Bench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

/// A mill whose Z stroke is [-5, 0] in program coordinates: the face at
/// 19.5 is above it, whatever the shipped VMC's own bands are (#477).
fn short_stroke_mill() -> Machine {
    let z = AxisLimits { name: "Z".into(), travel: Some((-5.0, 0.0)), ..Default::default() };
    Machine { model: "Short-stroke mill".into(), kind: Some("mill".into()), axes: vec![z], ..Default::default() }
}

/// A mill stating no strokes: nothing about where the face sits can
/// be refused on it.
fn bench_mill() -> Machine {
    Machine { model: "Bench mill".into(), kind: Some("mill".into()), ..Default::default() }
}

fn add_tool(world: &World, h: &mut History, name: &str) {
    let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("tool");
    h.execute(world, AddTool::new(world, name, tool).expect("add tool"));
}

fn generate(world: &World, h: &mut History, name: &str) {
    h.execute(world, GenerateOperation::new(world, name).expect("generates"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), camcraft_app::operation::OpStatus::Ok, "{name}");
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
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

fn facing() -> OpParams {
    facing_by(0.5)
}

fn facing_by(stepdown: f64) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

/// Setup1: a box faced on a mill (the bench mill, its own). Setup2: the
/// bar faced on a lathe through the Fanuc Lathe post, both its own.
/// Setup2 is active (the last created).
fn mill_and_lathe() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("box"));
    h.execute(&world, SetSetupMachine::new(&world, "Setup1", Some(("bench-mill.mch".into(), Some(bench_mill())))).expect("mill"));
    add_tool(&world, &mut h, MILL_TOOL);
    add_tool(&world, &mut h, LATHE_TOOL);
    h.execute(&world, CreateOperation::new(&world, "Face1", MILL_TOOL, SrcRef::Stock, face()).expect("face"));

    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", Some(("bench-lathe.mch".into(), Some(lathe())))).expect("lathe"));
    h.execute(&world, SetSetupPost::new(&world, "Setup2", Some(PostRef::Named("Fanuc Lathe".into()))).expect("lathe post"));
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let bar = SetStock::new(&world, StockShape::Cylinder { diameter: 12.0, height: 32.0 }, at);
    h.execute(&world, bar.expect("bar stock"));
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 16)));
    h.execute(&world, CreateOperation::new(&world, "Facing2", LATHE_TOOL, SrcRef::Entity("bar".into()), facing()).expect("facing"));
    (world, h)
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-388-{tag}-{}.{ext}", std::process::id()))
}

/// Post the program as one file per mount; (file name, text) each, the
/// files removed again.
fn post_mounts(world: &World, tag: &str) -> Result<Vec<(String, String)>, String> {
    let path = tmp(tag, "nc");
    let files = posting::post_mounts(world, &path)?;
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f.path).expect("written");
        assert_eq!(text, f.posted.program, "the file is the posted program");
        let _ = std::fs::remove_file(&f.path);
        out.push((f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), text));
    }
    Ok(out)
}

fn post_alone(world: &World, tag: &str) -> Result<String, String> {
    let path = tmp(tag, "nc");
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

fn suppress(world: &World, h: &mut History, op: &str, on: bool) {
    h.execute(world, EditOperationParams::with_suppressed(world, op, on).expect("suppress"));
}

#[test]
fn an_inactive_setups_op_generates_on_its_own_machine_without_activation() {
    let (world, mut h) = mill_and_lathe();
    assert_eq!(camcraft_app::setup::active(&world).as_deref(), Some("Setup2"));
    assert_eq!(machine::kind_of(&world), Kind::Lathe, "the ribbon reads the active setup's lathe");
    assert_eq!(machine::kind_of_setup(&world, Some("Setup1")), Kind::Mill);
    // The mill face of the inactive Setup1 generates where it stands -
    // once refused "activate 'Setup1' first" (#343).
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    assert_eq!(camcraft_app::setup::active(&world).as_deref(), Some("Setup2"), "nothing was activated");
    // And from the other side: the lathe op regenerates with Setup1 active.
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    h.execute(&world, EditOperationParams::new(&world, "Facing2", facing_by(0.4)).expect("edit"));
    generate(&world, &mut h, "Facing2");
    // Activation is no input of an op's stamp: both stay fresh across it.
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    for op in ["Face1", "Facing2"] {
        let e = model::find_by_name(&world, op).expect("op");
        assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Ok, "{op}");
    }
}

#[test]
fn a_mill_and_a_lathe_post_two_files_each_byte_identical_to_posting_it_alone() {
    let (world, mut h) = mill_and_lathe();
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    let files = post_mounts(&world, "mill-lathe").expect("two mounts post");
    assert_eq!(files.len(), 2, "one file per mount: {:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());
    assert!(files[0].0.ends_with("_01_Setup1.nc") && files[1].0.ends_with("_02_Setup2.nc"), "{files:?}");
    assert!(!files[0].1.contains("G18") && files[0].1.contains("G54"), "the mill file is the mill dialect: {}", files[0].1);
    assert!(files[1].1.contains("G18") && files[1].1.contains("G50 S12000"), "the lathe file is the lathe's own");
    // The single-file post still refuses a program over two mounts, by name.
    let why = post_alone(&world, "one-file").expect_err("one file holds one post");
    assert!(why.contains("2 mounts") && why.contains("Setup1") && why.contains("Setup2"), "{why}");

    // Each file IS that setup posted alone (the other left out, the
    // setup activated - how it was posted before #388).
    suppress(&world, &mut h, "Facing2", true);
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    assert_eq!(post_alone(&world, "alone-1").expect("Setup1 alone"), files[0].1, "byte-identical");
    suppress(&world, &mut h, "Facing2", false);
    suppress(&world, &mut h, "Face1", true);
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    assert_eq!(post_alone(&world, "alone-2").expect("Setup2 alone"), files[1].1, "byte-identical");
    // One mount left in the program: post_mounts writes the one file the
    // single-file post writes, at the path asked.
    let path = tmp("one-mount", "nc");
    let one = posting::post_mounts(&world, &path).expect("one mount");
    let _ = std::fs::remove_file(&path);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].path, path);
    assert_eq!(one[0].posted.program, files[1].1);
}

#[test]
fn a_travel_limit_is_judged_on_the_ops_own_machine_only() {
    // Two mills: Setup1 on a short-stroke mill (its Z stroke ends at 0 -
    // the face at 19.5 is above it), Setup2 on the strokeless bench mill.
    let world = new_world();
    let mut h = History::default();
    add_tool(&world, &mut h, MILL_TOOL);
    for (i, name) in ["Setup1", "Setup2"].into_iter().enumerate() {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(&world, stock.expect("box"));
        let op = format!("Face{}", i + 1);
        h.execute(&world, CreateOperation::new(&world, &op, MILL_TOOL, SrcRef::Stock, face()).expect("face"));
        let _ = name;
    }
    h.execute(&world, SetSetupMachine::new(&world, "Setup1", Some(("short.mch".into(), Some(short_stroke_mill())))).expect("short"));
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", Some(("bench-mill.mch".into(), Some(bench_mill())))).expect("bench"));
    for op in ["Face1", "Face2"] {
        generate(&world, &mut h, op);
    }
    let vmc = machine::of_setup(&world, Some("Setup1")).1.expect("short").model;
    for active in ["Setup2", "Setup1"] {
        h.execute(&world, ActivateSetup::new(&world, Some(active)).expect("activate"));
        let gate = posting::verification_gate(&world);
        let limits: Vec<&String> = gate.iter().filter(|p| p.contains("limit problem")).collect();
        assert_eq!(limits.len(), 1, "one machine is outrun ({active} active): {gate:?}");
        assert!(limits[0].starts_with(&format!("machine {vmc}:")) && limits[0].contains("Face1:"), "{gate:?}");
        assert!(!limits[0].contains("Face2"), "Face2 runs on the bench mill: {gate:?}");
        let refused = post_mounts(&world, "travel").expect_err("the gate holds every file");
        assert!(refused.contains("Face1:") && !refused.contains("Face2:"), "{refused}");
    }
    // Swap the steel: the verdict moves with it, to Face2 alone.
    h.execute(&world, SetSetupMachine::new(&world, "Setup1", Some(("bench-mill.mch".into(), Some(bench_mill())))).expect("bench"));
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", Some(("short.mch".into(), Some(short_stroke_mill())))).expect("short"));
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p.contains("Face2:")) && !gate.iter().any(|p| p.contains("Face1:")), "{gate:?}");
}

#[test]
fn per_operation_files_post_each_op_through_its_own_setups_post() {
    let (world, mut h) = mill_and_lathe();
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    let dir = tmp("per-op", "d");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let files = posting::post_per_op(&world, &dir, "job").expect("per-op posts");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(files.len(), 2);
    assert!(!files[0].posted.program.contains("G18"), "Face1 through the mill post");
    assert!(files[1].posted.program.contains("G18") && files[1].posted.program.contains("G50 S12000"), "Facing2 on its lathe");
}

#[test]
fn the_script_post_writes_one_file_per_mount() {
    let (world, mut h) = mill_and_lathe();
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    let expected = post_mounts(&world, "script-ref").expect("posts");
    let proj = tmp("script", "ccam");
    project::save(&world, &proj).expect("save");
    let nc = tmp("script-out", "nc");
    let q = |p: &std::path::Path| serde_json::to_string(&p.display().to_string()).expect("json");
    let out = script::run(&format!("cam.load({}); cam.generate(); cam.post({{ to: {} }})", q(&proj), q(&nc)));
    let _ = std::fs::remove_file(&proj);
    assert!(out.ok, "{:?}", out.errors);
    let files = out.result["files"].as_array().cloned().unwrap_or_default();
    assert_eq!(files.len(), 2, "{}", out.result);
    for (f, (name, text)) in files.iter().zip(&expected) {
        let path = std::path::PathBuf::from(f.as_str().expect("a path"));
        let tail = name.rsplit_once("-ref-").map(|(_, t)| t.split_once('_').map_or(t, |(_, t)| t)).unwrap_or(name);
        assert!(path.to_string_lossy().ends_with(&format!("_{tail}")), "{path:?} {name}");
        assert_eq!(&std::fs::read_to_string(&path).expect("written"), text, "the CLI's file is the app's");
        let _ = std::fs::remove_file(&path);
    }
    assert_eq!(out.result["mounts"][1]["post"], "Fanuc Lathe (built-in)", "{}", out.result);
}

/// #1224 row 7: `setup` posting through its own `post`, which does not
/// serve its own machine, is refused naming `op`, and nothing is posted.
fn unserved_post_is_refused(setup: &str, post: &str, op: &str) {
    let (world, mut h) = mill_and_lathe();
    h.execute(&world, SetSetupPost::new(&world, setup, Some(PostRef::Named(post.into()))).expect("own post"));
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    let tag = format!("unserved-{setup}");
    let why = post_mounts(&world, &tag).expect_err(&format!("{post} does not serve {setup}'s machine"));
    assert!(why.contains(op), "{post} on {setup}: the refusal names {op}: {why}");
    let p = tmp(&tag, "nc");
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    for f in [p.clone(), p.with_file_name(format!("{stem}_01_Setup1.nc")), p.with_file_name(format!("{stem}_02_Setup2.nc"))] {
        assert!(!f.exists(), "nothing is posted: {f:?}");
    }
}

/// A lathe post for the mill setup's face: the #1190 door refuses it.
#[test]
fn an_own_lathe_post_on_a_mill_setup_is_refused_by_name() {
    unserved_post_is_refused("Setup1", "Fanuc Lathe", "Face1");
}

/// A milling post for the lathe setup's turning op.
#[test]
fn an_own_milling_post_on_a_lathe_setup_is_refused_by_name() {
    unserved_post_is_refused("Setup2", "Haas", "Facing2");
}

/// #438: the tool-number clash is judged per machine. Two machines are
/// two tool tables - the same T number on each posts as two files; two
/// tools on one machine sharing a number still refuse, naming the mount.
#[test]
fn a_tool_number_shared_across_machines_posts_but_clashes_within_one() {
    let (world, mut h) = mill_and_lathe();
    // LATHE_TOOL posts as T2 by position; the mill tool takes 2 too.
    h.execute(&world, SetToolNumber::new(&world, MILL_TOOL, Some(2)).expect("number"));
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    let files = post_mounts(&world, "t-across").expect("T2 on two machines is two tables");
    assert_eq!(files.len(), 2);
    assert!(files[0].1.contains("T02 M6") && files[1].1.contains("T02"), "{files:?}");

    // A second mill tool on Setup1, posting by position, and the mill
    // tool on that same number (renumbered if it is not already):
    // one carousel, one T number.
    const OTHER: &str = "EM-0600-4FL";
    add_tool(&world, &mut h, OTHER);
    let n = camcraft_app::tool_numbers::number_of(&world, OTHER).expect("a tool");
    if camcraft_app::tool_numbers::number_of(&world, MILL_TOOL) != Some(n) {
        h.execute(&world, SetToolNumber::new(&world, MILL_TOOL, Some(n)).expect("number"));
    }
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    h.execute(&world, CreateOperation::new(&world, "Face1b", OTHER, SrcRef::Stock, face()).expect("face"));
    generate(&world, &mut h, "Face1b");
    let why = post_mounts(&world, "t-within").expect_err("one machine, one T number");
    assert!(
        why.contains(MILL_TOOL) && why.contains(OTHER) && why.contains(&format!("T{n}")) && why.contains("Setup1"),
        "{why}"
    );
    assert!(!why.contains(LATHE_TOOL), "the lathe's T2 is its own table: {why}");
}

//! #1232: a post writes only for a machine kind it serves. A setup's
//! own milling post on a lathe once posted a turning op as a milling
//! program (G17, T M6, G43, S M3, Y moves); now every posting path - one
//! file per mount, the single file, one file per operation, a caller's
//! own script - refuses by name (the setup, its ops, the post, the kind)
//! before a line is written, and writes nothing. A lathe on a lathe post
//! posts as before.
//! Provenance: our own fixtures over our own commands (the #388 mill +
//! lathe program of `setup_mounts_post`); no vendor material.
//!
//! Gate map (#1232): covers app::posting (post_serves, post_mounts,
//! post_mounts_script, post_setup, post_script, post_per_op,
//! post_per_op_script, the posting_serves door).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History,
    ImportModel, SetSetupMachine, SetSetupPost, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting};

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

/// The #388 program: Setup1 faces a box on a bench mill, Setup2 faces
/// the bar on a bench lathe through `lathe_post` - both their own mounts.
/// Both ops generated; Setup2 active.
fn mill_and_lathe(lathe_post: &str) -> (World, History) {
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
    generate(&world, &mut h, "Face1");
    generate(&world, &mut h, "Facing2");
    // The post is chosen after generation: the program is fresh and
    // clean, so a refusal can only be the post's.
    if lathe_post != "Fanuc Lathe" {
        h.execute(&world, SetSetupPost::new(&world, "Setup2", Some(PostRef::Named(lathe_post.into()))).expect("post"));
    }
    (world, h)
}

/// A scratch folder of its own, empty.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-1232-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

/// What `dir` holds after a post, then removed.
fn written(dir: &std::path::Path) -> Vec<String> {
    let names = std::fs::read_dir(dir)
        .map(|it| it.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(dir);
    names
}

fn haas() -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == "Haas").map(|(_, t)| *t).expect("Haas ships")
}

/// The refusal names the setup, its op, the post and the machine's kind.
fn names_it(why: &str, setup: &str, op: &str, post: &str, kind: &str) {
    for word in [setup, op, post, kind, "does not serve"] {
        assert!(why.contains(word), "{word:?} missing from: {why}");
    }
}

#[test]
fn an_own_milling_post_on_a_lathe_setup_is_refused_by_name() {
    let (world, _h) = mill_and_lathe("Haas");
    // One file per mount: refused before either file is written - the
    // program is one job, never half of it on disk.
    let dir = scratch("mounts");
    let why = posting::post_mounts(&world, &dir.join("job.nc")).expect_err("Haas does not serve a lathe");
    names_it(&why, "Setup2", "Facing2", "Haas", "turning");
    assert!(why.contains("Bench lathe") && !why.contains("Face1"), "Setup1 is served: {why}");
    assert_eq!(written(&dir), Vec::<String>::new(), "nothing is written, Setup1's file included");

    // One file per operation: the same refusal, nothing written.
    let dir = scratch("per-op");
    let why = posting::post_per_op(&world, &dir, "job").expect_err("per-op refuses too");
    names_it(&why, "Setup2", "Facing2", "Haas", "turning");
    assert_eq!(written(&dir), Vec::<String>::new());
}

#[test]
fn a_lathe_program_through_a_milling_post_is_refused_on_every_single_file_path() {
    let (world, mut h) = mill_and_lathe("Haas");
    // Setup2 alone: the single-file post through its own (Haas) post,
    // through Haas handed as a script, and the per-mount and per-op
    // script paths (one script for every mount).
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress"));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    let dir = scratch("single");
    let one = dir.join("job.nc");
    for why in [
        posting::post_setup(&world, &one).map(|_| ()),
        posting::post_script(&world, &one, haas()).map(|_| ()),
        posting::post_mounts_script(&world, &one, haas()).map(|_| ()),
        posting::post_per_op_script(&world, &dir, "job", haas()).map(|_| ()),
    ] {
        names_it(&why.expect_err("refused"), "Setup2", "Facing2", "Haas", "turning");
    }
    // A caller's own milling script (not a shipped one) is refused by
    // the dialect it declares, named as the given script.
    let own = haas().replace("Haas", "Own mill");
    let why = posting::post_script(&world, &one, &own).expect_err("an own milling script on a lathe");
    names_it(&why, "Setup2", "Facing2", "the given post script", "turning");
    assert_eq!(written(&dir), Vec::<String>::new());
}

#[test]
fn a_mill_setup_on_a_lathe_post_is_refused_and_the_lathe_on_its_lathe_post_still_posts() {
    let (world, mut h) = mill_and_lathe("Fanuc Lathe");
    // The served program posts: two files, the lathe's in its own dialect.
    let dir = scratch("served");
    let files = posting::post_mounts(&world, &dir.join("job.nc")).expect("served on both mounts");
    assert_eq!(files.len(), 2);
    assert!(files[1].posted.program.contains("G18") && files[1].posted.program.contains("G50 S12000"));
    assert_eq!(written(&dir).len(), 2);
    // Setup1 (a mill) through a lathe post: refused by name - here the
    // #1190 door names the milling op first, before any kind clause.
    h.execute(&world, SetSetupPost::new(&world, "Setup1", Some(PostRef::Named("Fanuc Lathe".into()))).expect("post"));
    let dir = scratch("mill-on-lathe-post");
    let why = posting::post_mounts(&world, &dir.join("job.nc")).expect_err("a lathe post on a mill");
    assert!(why.contains("Face1"), "{why}");
    assert_eq!(written(&dir), Vec::<String>::new());
}

#[test]
fn an_edm_mount_refuses_a_cutting_program_by_name() {
    // No EDM post ships and no cutting op runs on an EDM (#533): a
    // milling op whose setup is re-mounted on an EDM is refused by name
    // (the family gate reaches it before the kind door), nothing posted.
    // The plasma table on a milling post: `plasma_post`.
    for (kind, word) in [("wire_edm", "wire EDM"), ("sinker_edm", "sinker EDM")] {
        let (world, mut h) = mill_and_lathe("Fanuc Lathe");
        let m = Machine { model: format!("Bench {kind}"), kind: Some(kind.into()), ..Default::default() };
        h.execute(&world, SetSetupMachine::new(&world, "Setup1", Some((format!("{kind}.mch"), Some(m)))).expect("machine"));
        let dir = scratch(kind);
        let why = posting::post_mounts(&world, &dir.join("job.nc")).expect_err("a cutting program on an EDM");
        assert!(why.contains(word) && why.contains("Face1"), "{kind}: {why}");
        assert_eq!(written(&dir), Vec::<String>::new(), "{kind}");
    }
}

fn lathe_post() -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == "Fanuc Lathe").map(|(_, t)| *t).expect("Fanuc Lathe ships")
}

/// #1266: a user's own script states the kinds it serves (`serves`).
#[test]
fn a_users_own_script_is_judged_by_the_kinds_it_states() {
    let (world, mut h) = mill_and_lathe("Fanuc Lathe");
    // Setup2 (lathe) alone: a milling script that states `serves = ["mill"]`
    // and a plasma script (`serves = ["plasma"]`) are refused by name.
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress"));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    let dir = scratch("serves-lathe");
    let one = dir.join("job.nc");
    for kind in ["mill", "plasma"] {
        let own = format!("serves = [\"{kind}\"];\n{}", haas());
        let why = posting::post_script(&world, &one, &own).expect_err("states another kind");
        names_it(&why, "Setup2", "Facing2", "the given post script", "turning");
    }
    // A turning script that states nothing still posts on the lathe.
    let undeclared = format!("{}\n// the user's own copy\n", lathe_post());
    posting::post_script(&world, &one, &undeclared).expect("a turning dialect serves a lathe undeclared");
    // ... and one that states `lathe` posts too.
    let stated = format!("serves = \"lathe\";\n{}", lathe_post());
    posting::post_script(&world, &one, &stated).expect("serves lathe");
    let _ = written(&dir);

    // Setup1 (mill) alone: a plasma script is refused by name, a mill one posts.
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", false).expect("unsuppress"));
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Facing2", true).expect("suppress"));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    let dir = scratch("serves-mill");
    let one = dir.join("job.nc");
    let plasma = format!("serves = [\"plasma\"];\n{}", haas());
    let why = posting::post_script(&world, &one, &plasma).expect_err("a plasma script on a mill");
    names_it(&why, "Setup1", "Face1", "the given post script", "milling");
    let mill = format!("serves = [\"mill\"];\n{}", haas());
    posting::post_script(&world, &one, &mill).expect("serves mill");
    // A typo'd kind is a clean refusal, never read as stated.
    let typo = format!("serves = [\"plazma\"];\n{}", haas());
    let why = posting::post_script(&world, &one, &typo).expect_err("no such kind");
    assert!(why.contains("plazma"), "{why}");
    let _ = written(&dir);
}

/// The #388 program with Setup2 re-mounted on a mill-turn and Facing2
/// regenerated there, posting through `post` (`None`: the document's).
fn mill_and_mill_turn(post: Option<&str>) -> (World, History) {
    let (world, mut h) = mill_and_lathe("Fanuc Lathe");
    let m = Machine { model: "Bench mill-turn".into(), kind: Some("mill-turn".into()), ..lathe() };
    h.execute(&world, SetSetupMachine::new(&world, "Setup2", Some(("bench-mill-turn.mch".into(), Some(m)))).expect("mill-turn"));
    generate(&world, &mut h, "Facing2");
    // The remount may already have left Setup2 on that post.
    if let Ok(c) = SetSetupPost::new(&world, "Setup2", post.map(|p| PostRef::Named(p.into()))) {
        h.execute(&world, c);
    }
    (world, h)
}

#[test]
fn a_turning_section_on_a_mill_turn_through_a_milling_post_is_refused_by_name() {
    // Maintainer, 2026-11-12 ("Refuse it"): a facing op on a mill-turn
    // through Haas (its own post) or Fanuc (the document's, the shipped
    // Mill-turn's default) posted the issue's output - a turning insert
    // on the tool spindle at S3000, the facing pass as Y moves.
    for (post, name) in [(Some("Haas"), "Haas"), (None, "Fanuc")] {
        let (world, mut h) = mill_and_mill_turn(post);
        let refused = |why: &str| {
            for word in ["Setup2", "Facing2", name, "is not a turning post", "Bench mill-turn"] {
                assert!(why.contains(word), "{name}: {word:?} missing from: {why}");
            }
            assert!(!why.contains("Face1"), "{name}: the mill setup is served: {why}");
        };
        let dir = scratch(&format!("mt-mounts-{name}"));
        refused(&posting::post_mounts(&world, &dir.join("job.nc")).expect_err("per mount"));
        assert_eq!(written(&dir), Vec::<String>::new(), "{name}: nothing is written, Setup1's file included");
        let dir = scratch(&format!("mt-per-op-{name}"));
        refused(&posting::post_per_op(&world, &dir, "job").expect_err("per op"));
        assert_eq!(written(&dir), Vec::<String>::new(), "{name}");
        h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress"));
        h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
        let dir = scratch(&format!("mt-single-{name}"));
        refused(&posting::post_setup(&world, &dir.join("job.nc")).map(|_| ()).expect_err("single file"));
        assert_eq!(written(&dir), Vec::<String>::new(), "{name}");
    }
    // Through a turning post the same mill-turn section posts.
    let (world, _h) = mill_and_mill_turn(Some("Fanuc Lathe"));
    let dir = scratch("mt-served");
    let files = posting::post_mounts(&world, &dir.join("job.nc")).expect("a turning post serves it");
    assert!(files[1].posted.program.contains("G18"), "{}", files[1].posted.program);
    let _ = written(&dir);
}

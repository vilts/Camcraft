//! Provenance: #1453's tool-hand persistence contract and our own catalog fixture.
//! Gate map: covers app::project (tool JSON), tools::tools, app::posting, app::posting_family,
//! app::tool_library, app::commands (tool), app::operation (stamp_tool).

use camcraft_app::{model, posting, project, tools::{Tool, catalog}};
use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, GenerateOperation, History, ImportModel,
    ImportTools, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_library::{export_tool_library, parse_tool_library};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const POSTS: &[&str] = &[
    "Mazak", "Fanuc", "Haas", "Siemens 840D", "Heidenhain", "Tormach PathPilot", "Doosan DNM",
];

fn tap(hand: Option<&str>) -> Tool {
    let t = catalog().into_iter().find(|(n, _)| *n == "TAP-M6").expect("catalog tap").1;
    let mut v = project::tool_to_json(&t);
    if let Some(hand) = hand {
        v["thread_hand"] = serde_json::json!(hand);
    }
    project::tool_from_json(&v).expect("tap JSON")
}

fn tool(world: &World) -> Tool {
    model::find_by_name(world, "Tap").and_then(|e| model::tool_of(world, &e)).expect("Tap tool")
}

fn assert_left(t: &Tool) {
    assert_eq!(project::tool_to_json(t)["thread_hand"], "left");
}

fn puck() -> Mesh {
    // Same already-drilled puck as rigid_tapping.rs: no unrelated reach refusal.
    let meridian = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (3.1, 10.0), (3.1, 2.0), (0.0, 2.0)];
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
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

fn job(hand: Option<&str>, cycle: CycleChoice) -> (World, History) {
    job_on_machine(hand, cycle, false)
}

fn job_on_machine(hand: Option<&str>, cycle: CycleChoice, lathe: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    if lathe {
        let machine = camcraft_app::machine::Machine {
            model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default()
        };
        h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine)).expect("lathe"));
    } else {
        h.execute(&world, LoadMachine::builtin(&world, "VMC 3-axis").expect("machine"));
    }
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 40.0, height: 10.0 }, Location::default()).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("puck", puck()));
    h.execute(&world, AddTool::new(&world, "Tap", tap(hand)).expect("add tap"));
    let params = OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(25.0), retract_z: None, top_z: Height::fixed(12.0),
        bottom_z: None, cycle, peck: 0.0, dwell: 0.0, feed: 100.0, plunge_feed: 100.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0), radius: 3.05, entry_z: 10.0, floor_z: 2.0,
            through: false, stepped: false,
        }], cross: Vec::new(),
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Tap1", "Tap", SrcRef::Entity("puck".into()), params).expect("tap op"));
    h.execute(&world, GenerateOperation::new(&world, "Tap1").expect("generate"));
    (world, h)
}

fn output(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1453-{tag}-{}.nc", std::process::id()))
}

fn assert_refusal(world: &World, history: &mut History, tag: &str) {
    let stream_error = posting::stream_records(world).expect_err("left-hand tap cannot dump RH records");
    assert!(stream_error.contains("left-hand tap") && stream_error.contains("Tap1"), "{stream_error}");
    for post in POSTS {
        history.execute(world, SelectPost::new(world, PostRef::Named((*post).into())).expect("select post"));
        let path = output(tag);
        let _ = std::fs::remove_file(&path);
        let err = match posting::post_setup(world, &path) {
            Err(e) => e,
            Ok(p) => panic!("{post} emitted wrong-handed tapping: {}", p.program),
        };
        assert!(err.contains("left-hand tap") && err.contains("Tap1"), "{post}: {err}");
        assert!(!path.exists(), "{post}: refusal must write no NC file");
    }
}

#[test]
fn left_hand_tap_survives_tool_json_roundtrip() {
    let tap = catalog()
        .into_iter()
        .find(|(name, _)| *name == "TAP-M6")
        .expect("catalog tap")
        .1;
    let mut definition = project::tool_to_json(&tap);
    definition["thread_hand"] = serde_json::json!("left");
    let parsed = project::tool_from_json(&definition).expect("left-hand tap definition");
    assert_eq!(
        project::tool_to_json(&parsed)["thread_hand"],
        "left",
        "a declared left-hand tap must never silently become right-hand"
    );
}

#[test]
fn absent_hand_is_legacy_right_and_unknown_hands_are_refused() {
    let right = tap(Some("right"));
    assert_eq!(tap(None), right, "legacy tools default to right-hand");
    let mut definition = project::tool_to_json(&right);
    for bad in [serde_json::json!("backwards"), serde_json::json!(7), serde_json::Value::Null] {
        definition["thread_hand"] = bad.clone();
        let err = project::tool_from_json(&definition).expect_err("unknown hand must refuse");
        assert!(err.contains("thread hand") || err.contains("thread_hand"), "{bad}: {err}");
    }
}

#[test]
fn every_tapping_post_refuses_left_hand_for_rigid_floating_and_auto() {
    for (cycle, tag) in [(CycleChoice::Tap, "rigid"), (CycleChoice::FloatingTap, "floating"), (CycleChoice::Auto, "auto")] {
        let (world, mut history) = job(Some("left"), cycle);
        assert_refusal(&world, &mut history, tag);
    }
}

#[test]
fn right_hand_tap_still_posts_on_every_tapping_post() {
    let (world, mut history) = job(None, CycleChoice::Tap);
    assert!(posting::stream_records(&world).is_ok());
    for post in POSTS {
        history.execute(&world, SelectPost::new(&world, PostRef::Named((*post).into())).expect("select post"));
        let path = output("right");
        let posted = posting::post_setup(&world, &path).unwrap_or_else(|e| panic!("{post}: {e}"));
        assert!(!posted.program.is_empty(), "{post}");
        assert!(path.exists(), "{post} writes NC");
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn fanuc_lathe_posts_right_hand_and_refuses_left_hand_axial_taps() {
    for (hand, cycle) in [
        (None, CycleChoice::Tap),
        (Some("left"), CycleChoice::Tap),
        (Some("left"), CycleChoice::FloatingTap),
        (Some("left"), CycleChoice::Auto),
    ] {
        let (world, mut history) = job_on_machine(hand, cycle, true);
        history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
        let path = output("lathe");
        let _ = std::fs::remove_file(&path);
        let result = posting::post_setup(&world, &path);
        if hand.is_some() {
            let err = match result {
                Err(e) => e,
                Ok(p) => panic!("left-hand axial tap emitted NC: {}", p.program),
            };
            assert!(err.contains("left-hand tap") && err.contains("Tap1"), "{err}");
            assert!(!path.exists(), "left-hand axial tapping writes no NC");
        } else {
            let posted = result.expect("right-hand axial tap");
            assert!(posted.program.contains("G84"), "{}", posted.program);
            assert!(path.exists(), "right-hand axial tapping writes NC");
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[test]
fn hand_survives_project_and_library_and_old_readers_are_barred() {
    let (world, mut h) = job(Some("left"), CycleChoice::Tap);
    let saved = project::save_text(&world).expect("save");
    let value: serde_json::Value = serde_json::from_str(&saved).expect("project JSON");
    assert!(value["version"].as_u64().expect("version") >= 62, "old builds must refuse left-hand projects");
    let loaded = new_world();
    project::open(&loaded, &saved).expect("load");
    assert_left(&tool(&loaded));
    assert_refusal(&loaded, &mut h, "loaded");

    let library = export_tool_library(&world).expect("export library");
    let value: serde_json::Value = serde_json::from_str(&library).expect("library JSON");
    assert_eq!(value["version"], 2, "old library readers must refuse left-hand taps");
    let lib = parse_tool_library(&library).expect("read left library");
    let imported = new_world();
    let mut ih = History::default();
    ih.execute(&imported, ImportTools::new(&imported, lib).expect("import"));
    assert_left(&tool(&imported));

    let right = new_world();
    let mut rh = History::default();
    rh.execute(&right, AddTool::new(&right, "Tap", tap(None)).expect("right tap"));
    let legacy = export_tool_library(&right).expect("right library");
    let value: serde_json::Value = serde_json::from_str(&legacy).expect("library JSON");
    assert_eq!(value["version"], 1, "right-hand-only libraries keep their format");
    assert_eq!(parse_tool_library(&legacy).expect("legacy library").len(), 1);
}

#[test]
fn changing_cached_tap_hand_is_undoable_and_cannot_post_right_hand_cycles() {
    let (world, mut h) = job(None, CycleChoice::Tap);
    assert!(posting::stream_records(&world).is_ok());
    h.execute(&world, ChangeTool::new(&world, "Tap", tap(Some("left"))).expect("change hand"));
    assert_left(&tool(&world));
    assert!(posting::stream_records(&world).is_err(), "changed or stale cache must never post");
    h.execute(&world, GenerateOperation::new(&world, "Tap1").expect("regenerate left tap"));
    assert_refusal(&world, &mut h, "changed");
    // Undo selected posts, generation and hand edit until the original tool is restored.
    while tool(&world) != tap(None) {
        assert!(h.undo(&world), "hand edit remains undoable");
    }
    assert!(posting::stream_records(&world).is_ok(), "undo restores right-hand cache");
}

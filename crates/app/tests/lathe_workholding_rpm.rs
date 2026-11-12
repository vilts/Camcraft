//! A setup's workholding rpm limit (#1031): a lathe setup states its
//! chuck's or collet's rated maximum rpm, and constant surface speed is
//! clamped to min(machine, tool, workholding) in both lathe dialects
//! (Fanuc `G50 S`, Tormach `G96 D`), while a constant-rpm turning op over
//! the limit is refused by name at the program gate, its setup named -
//! never silently capped. Setting it is one undoable command, validated
//! at creation; the project-file key is additive and round-trips byte
//! for byte; the turning suggestion is held to it; a datum transfer keeps
//! it.
//!
//! Gate map (M4.4): covers app::setup_workholding, app::commands
//! (SetSetupWorkholdingRpm, CreateSetup's datum transfer), app::posting
//! (the CSS ceiling, the program gate), app::cutting_turn (the hold),
//! app::project (the additive key).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine, SetSetupWorkholdingRpm,
    SetStock, SetWorkMaterial,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::schema::Location;
use camcraft_app::tools::{Tool, ToolMaterial, TurningData, WorkMaterial, catalog};
use camcraft_app::world::new_world;
use camcraft_app::{cutting, posting, project, setup_workholding};

const TAU: f64 = std::f64::consts::TAU;
const FANUC: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TORMACH: &str = include_str!("../../../assets/posts/tormach-lathe.cps");
/// The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
const BAR: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

fn insert() -> Tool {
    let t = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t);
    Tool { tool_material: Some(ToolMaterial::Carbide), ..t.expect("catalog insert") }
}

fn css(max_rpm: Option<f64>) -> Tool {
    Tool { turning: Some(TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm }), ..insert() }
}

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

/// The bar in its own-size stock on a 12000 rpm lathe, declared as
/// Setup1 (stating `chuck` rpm), a carbide insert TN (as `tool`), mild
/// steel, a facing op F1 - generated.
fn faced(tool: Tool, chuck: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let stock = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 }, at);
    h.execute(&world, stock.expect("bar stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    h.execute(&world, AddTool::new(&world, "TN", tool).expect("insert"));
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 16)));
    h.execute(&world, SetWorkMaterial::new(&world, Some(WorkMaterial::MildSteel)).expect("material"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if chuck.is_some() {
        h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", chuck).expect("chuck limit"));
    }
    let params = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let op = CreateOperation::new(&world, "F1", "TN", SrcRef::Entity("bar".into()), params);
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "F1").expect("generate"));
    (world, h)
}

/// The document's stream through a lathe script, gate-free; the stream's
/// own WARN records (the verifier's notes) are taken out of the warnings.
fn post(cps: &str, world: &World) -> engine::PostOutput {
    let records = posting::stream_records(world).expect("every op generated");
    let mut out = engine::run(cps, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs");
    let warns: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    out.warnings.retain(|w| !warns.contains(w));
    out
}

/// `text` (a project file) with every `key` taken out, as a file
/// written before the key existed.
fn without_key(text: &str, key: &str) -> String {
    fn strip(v: &mut serde_json::Value, key: &str) {
        match v {
            serde_json::Value::Object(m) => {
                m.remove(key);
                m.values_mut().for_each(|x| strip(x, key));
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| strip(x, key)),
            _ => {}
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(text).expect("a project file is JSON");
    strip(&mut v, key);
    serde_json::to_string_pretty(&v).expect("JSON out")
}

fn chuck_lines(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|p| p.contains("workholding")).collect()
}

#[test]
fn a_2000_rpm_chuck_clamps_g96_under_a_higher_tool_and_machine_in_both_dialects() {
    for tool_clamp in [None, Some(3000.0)] {
        let (world, _h) = faced(css(tool_clamp), Some(2000.0));
        let f = post(FANUC, &world);
        assert!(f.warnings.is_empty(), "{:?}", f.warnings);
        assert!(f.program.contains("G50 S2000\nG96 S170 M03\n"), "the chuck's clamp before the G96: {}", f.program);
        assert!(!f.program.contains("S3000"), "never the tool's higher clamp: {}", f.program);
        let t = post(TORMACH, &world);
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert!(t.program.contains("G96 D2000 S170 M3\n"), "the chuck's rating as D: {}", t.program);
        assert!(chuck_lines(&world).is_empty(), "a clamped G96 is not refused: {:?}", chuck_lines(&world));
    }
    // A tool rated under the chuck keeps its own lower clamp.
    let (world, _h) = faced(Tool { rpm: 1000.0, ..css(Some(1500.0)) }, Some(2000.0));
    assert!(post(TORMACH, &world).program.contains("G96 D1500 S170 M3\n"));
}

#[test]
fn a_g97_op_over_the_chuck_is_refused_by_name_never_capped() {
    let (world, mut h) = faced(Tool { rpm: 2500.0, ..insert() }, Some(2000.0));
    let lines = chuck_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.contains("F1") && l.contains("2500 rpm"), "names the op and its rpm: {l}");
    assert!(l.contains("setup 'Setup1'") && l.contains("workholding maximum of 2000 rpm"), "names the setup: {l}");
    let out = std::env::temp_dir().join(format!("camcraft-1031-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, FANUC);
    let _ = std::fs::remove_file(&out);
    let why = posted.err().unwrap_or_default();
    assert!(why.contains("post blocked") && why.contains("workholding maximum of 2000 rpm"), "{why}");
    // Raising the stated rating (one undoable command) clears it; undo refuses again.
    h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", Some(2500.0)).expect("raise"));
    assert!(chuck_lines(&world).is_empty(), "at the limit is allowed: {:?}", chuck_lines(&world));
    assert!(h.undo(&world));
    assert_eq!(chuck_lines(&world).len(), 1);
    // No limit stated: the G97 posts as before.
    let (free, _h) = faced(Tool { rpm: 2500.0, ..insert() }, None);
    assert!(chuck_lines(&free).is_empty());
    assert!(post(FANUC, &free).program.contains("G97 S2500 M03\n"));
}

#[test]
fn a_3000_rpm_g97_turning_op_under_a_2000_rpm_chuck_is_still_refused_by_name() {
    // #1174 narrowed the limit to the work spindle; turning is that spindle.
    let (world, _h) = faced(Tool { rpm: 3000.0, ..insert() }, Some(2000.0));
    let lines = chuck_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("F1: turns the work at 3000 rpm (G97)"), "{}", lines[0]);
    assert!(lines[0].contains("setup 'Setup1''s workholding maximum of 2000 rpm"), "{}", lines[0]);
}

#[test]
fn the_limit_is_one_validated_undoable_command() {
    let (world, mut h) = faced(insert(), None);
    assert_eq!(setup_workholding::of(&world, Some("Setup1")), None);
    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY, 1.0e6] {
        assert!(SetSetupWorkholdingRpm::new(&world, "Setup1", Some(bad)).is_err(), "{bad} is refused");
    }
    assert!(SetSetupWorkholdingRpm::new(&world, "Setup1", None).is_err(), "clearing nothing is a no-op");
    assert!(SetSetupWorkholdingRpm::new(&world, "NoSuch", Some(2000.0)).is_err());
    assert!(SetSetupWorkholdingRpm::new(&world, "TN", Some(2000.0)).is_err(), "a tool is not a setup");
    h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", Some(2000.0)).expect("set"));
    assert!(SetSetupWorkholdingRpm::new(&world, "Setup1", Some(2000.0)).is_err(), "already stated");
    h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", None).expect("clear"));
    assert_eq!(setup_workholding::of(&world, Some("Setup1")), None);
    assert!(h.undo(&world));
    assert_eq!(setup_workholding::of(&world, Some("Setup1")), Some(2000.0));
    assert!(h.undo(&world));
    assert_eq!(setup_workholding::of(&world, Some("Setup1")), None);
    assert!(h.redo(&world));
    assert_eq!(setup_workholding::of(&world, Some("Setup1")), Some(2000.0));
    // A datum transfer is the same clamping system: it keeps the rating.
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    assert_eq!(setup_workholding::of(&world, Some("Setup2")), Some(2000.0));
}

#[test]
fn the_turning_suggestion_is_held_to_the_chuck() {
    let (world, _h) = faced(insert(), Some(2000.0));
    let s = cutting::suggest(&world, "F1").expect("a turning suggestion");
    assert_eq!(s.rpm, 2000.0, "4509 rpm at the bar, held to the chuck");
    assert!(s.notes.iter().any(|n| n.contains("workholding's maximum 2000")), "{:?}", s.notes);
    assert!((s.feed / s.rpm - 0.2).abs() < 1e-12, "the per-rev feed is kept: {}", s.feed);
}

#[test]
fn the_limit_is_additive_in_the_project_file_and_round_trips_byte_identical() {
    let (world, _h) = faced(insert(), Some(2000.0));
    let path = std::env::temp_dir().join(format!("camcraft-1031-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(text.matches("\"workholding_max_rpm\": 2000.0").count(), 1, "{text}");
    let back = new_world();
    project::open(&back, &text).expect("load");
    assert_eq!(setup_workholding::of(&back, Some("Setup1")), Some(2000.0));
    project::save(&back, &path).expect("resave");
    let again = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(again, text, "byte-identical round trip");
    // An older file states none; a value no command would take is corrupt.
    let older = without_key(&text, "workholding_max_rpm");
    assert!(!older.contains("workholding_max_rpm"), "the key came out: {older}");
    let old = new_world();
    project::open(&old, &older).expect("an older file loads");
    assert_eq!(setup_workholding::of(&old, Some("Setup1")), None);
    let bad = text.replace("\"workholding_max_rpm\": 2000.0", "\"workholding_max_rpm\": -5.0");
    let why = project::open(&new_world(), &bad).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(why.contains("Setup1") && why.contains("workholding"), "refused by name: {why}");
    // A document without the limit writes no key.
    let (plain, _h) = faced(insert(), None);
    project::save(&plain, &path).expect("save plain");
    let plain_text = std::fs::read_to_string(&path).expect("read plain");
    let _ = std::fs::remove_file(&path);
    assert!(!plain_text.contains("workholding_max_rpm"));
}

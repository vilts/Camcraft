//! The coolant review's follow-ups (#150): refusals name the operation,
//! a Flood op on a machine whose list states no flood code is refused,
//! a post that does not declare `coolantModes` refuses non-Flood asks,
//! machine names read through spelling variants, and the owed paths -
//! the Fanuc Lathe post, M88 on the shipped mill-turn, Mist then Flood.
//!
//! Gate map (M4.4): covers app::posting, post::post (engine, js),
//! post::machine (COOLANT_KINDS), assets/posts/*.cps,
//! assets/machines/millturn.mch.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, LoadMachine, SetStock,
};
use camcraft_app::cl::MoveKind;
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{CoolantMode, Height, OpParams, SrcRef};
use camcraft_app::post::cnc::{self, MoveRec, Record};
use camcraft_app::post::{self, engine};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use serde_json::json;

const MILLS: [&str; 7] = ["Fanuc", "Haas", "Siemens 840D", "Heidenhain", "Mazak", "grblHAL", "TinyG"];
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

fn face_params(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Two generated face ops on two tools, each with the coolant asked.
fn two_faces(first: CoolantMode, second: CoolantMode) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    for t in ["EM-0400-2FL", "EM-0600-4FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    for (name, tool, z, c) in [("Face1", "EM-0400-2FL", 19.5, first), ("Face2", "EM-0600-4FL", 19.0, second)] {
        let cmd = CreateOperation::new(&world, name, tool, SrcRef::Stock, face_params(z));
        history.execute(&world, cmd.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        if c != CoolantMode::default() {
            let cmd = EditOperationParams::with_coolant(&world, name, c);
            history.execute(&world, cmd.expect("coolant edit"));
        }
    }
    (world, history)
}

fn mount(world: &World, history: &mut History, coolants: Vec<Coolant>) {
    // States Z0 as its safe retract (#495/#503): the Siemens post refuses
    // a machine that leaves "up" unstated, which is not this suite's law.
    let m = Machine { model: "CoolBench".into(), coolants, safe_retract_z: Some(0.0), ..Default::default() };
    history.execute(world, LoadMachine::new(world, "cool.mch".into(), Some(m)).expect("sane"));
}

fn coolant(name: &str, code: &str) -> Coolant {
    Coolant { name: name.into(), m_code: code.into() }
}

fn script(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("a built-in")
}

fn post_with(world: &World, script: &str) -> post::PostOutput {
    let records = posting::stream_records(world).expect("a fresh world streams");
    post::post(script, &posting::machine_value(world), &cnc::serialize(&records)).expect("the script runs")
}

fn has_word(line: &str, code: &str) -> bool {
    line.split_whitespace().any(|w| w.trim_end_matches('*') == code)
}

/// Line indices carrying `code`, in program order.
fn lines_with(program: &str, code: &str) -> Vec<usize> {
    program.lines().enumerate().filter(|(_, l)| has_word(l, code)).map(|(i, _)| i).collect()
}

#[test]
fn a_coolant_refusal_names_the_operation() {
    // The default VMC states Flood and Mist only.
    let (world, _h) = two_faces(CoolantMode::Flood, CoolantMode::ThroughTool);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(
            out.warnings.iter().any(|w| w.contains("operation Face2") && w.contains("through-tool")),
            "{name}: the refusal names the op that asked: {:?}",
            out.warnings
        );
        assert!(!out.warnings.iter().any(|w| w.contains("Face1")), "{name}: the flood op is fine");
    }
}

#[test]
fn a_flood_op_on_a_machine_stating_no_flood_code_blocks_the_post_by_name() {
    let (world, mut history) = two_faces(CoolantMode::Flood, CoolantMode::Mist);
    mount(&world, &mut history, vec![coolant("Mist", "M7")]);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(
            out.warnings.iter().any(|w| w.contains("operation Face1") && w.contains("no flood coolant code")),
            "{name}: never posted as mist or dry: {:?}",
            out.warnings
        );
        assert!(!out.warnings.iter().any(|w| w.contains("Face2")), "{name}: the mist op posts its M7");
    }
    let path = std::env::temp_dir().join(format!("camcraft-noflood-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let err = posting::post_setup(&world, &path).expect_err("the gate holds the program");
    assert!(err.contains("Face1") && err.contains("flood"), "{err}");
    assert!(!path.exists(), "a refusal leaves no file");

    // A shop's own name is its flood (the M3.3 C law), and an Off op on
    // the mist-only machine is dry by choice - neither is refused.
    let (world, mut history) = two_faces(CoolantMode::Off, CoolantMode::Flood);
    mount(&world, &mut history, vec![coolant("Mist", "M7"), coolant("Brine", "M51")]);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert_eq!(lines_with(&out.program, "M51").len(), 1, "{name}: {}", out.program);
        assert!(lines_with(&out.program, "M7").is_empty(), "{name}: a flood op never posts mist");
    }
}

#[test]
fn machine_coolant_names_read_through_spelling_variants() {
    let (world, mut history) = two_faces(CoolantMode::ThroughTool, CoolantMode::Mist);
    mount(&world, &mut history, vec![coolant("flood", "M8"), coolant("Through spindle", "M88"), coolant(" MIST ", "M7")]);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert_eq!(lines_with(&out.program, "M88").len(), 1, "{name}: {}", out.program);
        assert_eq!(lines_with(&out.program, "M7").len(), 1, "{name}: {}", out.program);
    }
    assert_eq!(
        camcraft_app::machine::COOLANT_KINDS,
        ["Flood", "Mist", "Air", "Through-spindle", "Through-tool"],
        "the editor's pick list is the posts' vocabulary"
    );
}

#[test]
fn a_post_that_does_not_declare_coolant_modes_refuses_non_flood_ops_by_name() {
    let fanuc = script("Fanuc");
    let blind = fanuc
        .lines()
        .filter(|l| !l.starts_with("coolantModes"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(blind.len(), fanuc.len(), "the fixture strip took");
    let (world, _h) = two_faces(CoolantMode::Flood, CoolantMode::Off);
    let out = post_with(&world, &blind);
    assert!(
        out.warnings.iter().any(|w| w.contains("operation Face2") && w.contains("coolantModes")),
        "{:?}",
        out.warnings
    );
    assert!(post_with(&world, fanuc).warnings.is_empty(), "the declaring post posts it");
    let (flood, _h) = two_faces(CoolantMode::Flood, CoolantMode::Flood);
    assert!(post_with(&flood, &blind).warnings.is_empty(), "a flood-only program needs no flag");
}

#[test]
fn every_mill_post_switches_mist_then_flood_with_m9_between() {
    let (world, _h) = two_faces(CoolantMode::Mist, CoolantMode::Flood);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let (m7, m8, m9) = (lines_with(&out.program, "M7"), lines_with(&out.program, "M8"), lines_with(&out.program, "M9"));
        if name == "TinyG" {
            // M0 does not own coolant: shut it off before each manual tool
            // change as well as at the end of each cutting section.
            assert_eq!((m7.len(), m8.len(), m9.len()), (1, 1, 4), "{name}: {}", out.program);
            assert!(m9[0] < m7[0] && m7[0] < m9[1] && m9[1] < m9[2] && m9[2] < m8[0] && m8[0] < m9[3], "{name}: safety off, mist, section off, safety off, flood, section off: {}", out.program);
        } else {
            assert_eq!((m7.len(), m8.len(), m9.len()), (1, 1, 2), "{name}: {}", out.program);
            assert!(m7[0] < m9[0] && m9[0] < m8[0] && m8[0] < m9[1], "{name}: mist, off, flood, off: {}", out.program);
        }
    }
}

#[test]
fn m88_posts_on_the_shipped_mill_turn_machine() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/millturn.mch"),
    )
    .expect("the stock machine file ships");
    let m = camcraft_app::machine::parse_mch(&text).expect("the shipped mill-turn parses");
    let (world, mut history) = two_faces(CoolantMode::ThroughTool, CoolantMode::Flood);
    history.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(m)).expect("sane"));
    let out = post_with(&world, script("Fanuc"));
    // The face fixture's 12000 rpm tops the mill-turn's 10000 (its own
    // refusal); the coolant words are what this pins.
    assert!(!out.warnings.iter().any(|w| w.contains("coolant")), "{:?}", out.warnings);
    let (m88, m8) = (lines_with(&out.program, "M88"), lines_with(&out.program, "M8"));
    assert_eq!((m88.len(), m8.len()), (1, 1), "{}", out.program);
    assert!(m88[0] < m8[0], "op 1 through-spindle, op 2 flood: {}", out.program);
}

fn mv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> Record {
    Record::Move(MoveRec { kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// One lathe section: its params, tool, and a face pass.
fn lathe_section(op: &str, tool: u32, ask: Option<&str>) -> Vec<Record> {
    let mut v = vec![
        Record::Param { name: "OperationName".into(), value: json!(op) },
        Record::Param { name: "SpindleSpeed".into(), value: json!(1200.0) },
    ];
    if let Some(a) = ask {
        v.push(Record::Param { name: "Coolant".into(), value: json!(a) });
    }
    v.push(Record::Tool { number: tool, name: format!("T{tool}") });
    v.push(Record::Section { begin: true });
    v.push(mv(MoveKind::Rapid, 20.0, 2.0, None));
    v.push(mv(MoveKind::Feed, 0.0, 2.0, Some(150.0)));
    v.push(Record::Section { begin: false });
    v
}

#[test]
fn the_fanuc_lathe_post_switches_each_sections_coolant() {
    let m = Machine {
        model: "CoolLathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(4000.0),
        coolants: vec![coolant("Flood", "M8"), coolant("Mist", "M7")],
        ..Default::default()
    };
    let mch = camcraft_app::post::machine::mirror(&m);
    let mut records = lathe_section("Turn1", 1, Some("mist"));
    records.extend(lathe_section("Turn2", 2, None));
    records.extend(lathe_section("Turn3", 3, Some("through-tool")));
    let out = engine::run(LATHE_CPS, &mch, &cnc::serialize(&records)).expect("the lathe script runs");
    let (m7, m8, m9) = (lines_with(&out.program, "M7"), lines_with(&out.program, "M8"), lines_with(&out.program, "M9"));
    assert_eq!((m7.len(), m8.len(), m9.len()), (1, 1, 2), "{}", out.program);
    assert!(m7[0] < m9[0] && m9[0] < m8[0] && m8[0] < m9[1], "mist, off, flood, off: {}", out.program);
    assert_eq!(
        out.warnings.iter().filter(|w| w.contains("coolant")).collect::<Vec<_>>(),
        vec!["operation Turn3: the machine states no through-tool coolant code - pick another coolant for the operation or add it to the machine"],
        "{:?}",
        out.warnings
    );
}

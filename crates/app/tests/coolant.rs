//! The per-operation coolant choice (#112): the field, its command and
//! undo, its file key, and the M words every built-in post writes for it.
//!
//! Gate map (M4.4): covers app::commands, app::posting, app::project,
//! app::schema, post::post (CoolantMode, dump), assets/posts/*.cps.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{CoolantMode, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

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

/// Two generated face ops on two tools - so the program has a tool
/// change between them - each with the coolant asked.
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

fn coolant_of(world: &World, name: &str) -> CoolantMode {
    let e = model::find_by_name(world, name).expect("op");
    model::operation_of(world, &e).expect("operation").coolant
}

/// The document's stream through one built-in script (the default VMC:
/// Flood M8, Mist M7, no through-tool entry).
fn post_with(world: &World, script: &str) -> post::PostOutput {
    let records = posting::stream_records(world).expect("a fresh world streams");
    let text = cnc::serialize(&records);
    post::post(script, &posting::machine_value(world), &text).expect("the script runs")
}

/// Whether a block carries `code` as a word (Heidenhain ends its
/// blocks with `*`).
fn has_word(line: &str, code: &str) -> bool {
    line.split_whitespace().any(|w| w.trim_end_matches('*') == code)
}

const MILLS: [&str; 7] = ["Fanuc", "Haas", "Siemens 840D", "Heidenhain", "Mazak", "grblHAL", "TinyG"];

fn script(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("a built-in")
}

/// The first line at or after `from` matching `pred` - for order checks.
fn index_of(lines: &[&str], from: usize, pred: impl Fn(&str) -> bool) -> Option<usize> {
    lines.iter().skip(from).position(|l| pred(l)).map(|i| i + from)
}

#[test]
fn coolant_defaults_to_flood_and_writes_no_stream_word() {
    let (world, _h) = two_faces(CoolantMode::Flood, CoolantMode::Flood);
    assert_eq!(coolant_of(&world, "Face1"), CoolantMode::Flood);
    let records = posting::stream_records(&world).expect("streams");
    let text = cnc::serialize(&records);
    assert!(!text.contains("Coolant"), "flood is the stream's silent default - goldens stay byte-exact: {text}");
}

#[test]
fn coolant_edit_undoes_and_redoes_and_keeps_the_stamp() {
    let (world, mut history) = two_faces(CoolantMode::Flood, CoolantMode::Flood);
    let e = model::find_by_name(&world, "Face1").expect("op");
    let stamp = model::operation_of(&world, &e).expect("op").stamp;
    assert!(stamp.is_some(), "generated");
    let cmd = EditOperationParams::with_coolant(&world, "Face1", CoolantMode::Mist).expect("edit");
    history.execute(&world, cmd);
    assert_eq!(coolant_of(&world, "Face1"), CoolantMode::Mist);
    assert_eq!(
        model::operation_of(&world, &e).expect("op").stamp,
        stamp,
        "coolant shapes no geometry - the op stays generated"
    );
    history.undo(&world);
    assert_eq!(coolant_of(&world, "Face1"), CoolantMode::Flood, "undo restores the previous coolant");
    history.redo(&world);
    assert_eq!(coolant_of(&world, "Face1"), CoolantMode::Mist, "redo reapplies it");
    assert!(
        EditOperationParams::with_coolant(&world, "Nope", CoolantMode::Off).is_err(),
        "an unknown operation is refused by name"
    );
}

#[test]
fn coolant_round_trips_through_the_project_file() {
    let (world, _h) = two_faces(CoolantMode::Mist, CoolantMode::Flood);
    let dir = std::env::temp_dir();
    let path = dir.join(format!("camcraft-coolant-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert_eq!(text.matches("\"coolant\"").count(), 1, "only the non-default op writes the key: {text}");
    assert!(text.contains("\"coolant\": \"mist\""), "{text}");
    let back = new_world();
    camcraft_app::project::load(&back, &path).expect("load");
    assert_eq!(coolant_of(&back, "Face1"), CoolantMode::Mist);
    assert_eq!(coolant_of(&back, "Face2"), CoolantMode::Flood, "an absent key reads flood (old files)");

    // A file written before the field existed: strip the key, load flood.
    let old = text.replace(",\n        \"coolant\": \"mist\"", "").replace("\"coolant\": \"mist\",", "");
    assert!(!old.contains("\"coolant\""), "the fixture strip took");
    std::fs::write(&path, old).expect("rewrite");
    let legacy = new_world();
    camcraft_app::project::load(&legacy, &path).expect("an older file loads");
    let _ = std::fs::remove_file(&path);
    assert_eq!(coolant_of(&legacy, "Face1"), CoolantMode::Flood);
}

#[test]
fn every_mill_post_switches_flood_then_mist_with_m9_before_the_tool_change_and_the_end() {
    let (world, _h) = two_faces(CoolantMode::Flood, CoolantMode::Mist);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let on = |code: &'static str| move |l: &str| has_word(l, code);
        let m8 = index_of(&lines, 0, on("M8")).unwrap_or_else(|| panic!("{name}: flood on: {}", out.program));
        let off1 = index_of(&lines, m8, on("M9")).unwrap_or_else(|| panic!("{name}: off after op 1"));
        let m7 = index_of(&lines, off1, on("M7")).unwrap_or_else(|| panic!("{name}: mist on: {}", out.program));
        let off2 = index_of(&lines, m7, on("M9")).unwrap_or_else(|| panic!("{name}: off after op 2"));
        assert!(index_of(&lines, m7, on("M8")).is_none(), "{name}: the mist op writes no flood word");
        assert!(index_of(&lines, 0, on("M7")) == Some(m7), "{name}: the flood op writes no mist word");
        assert!(index_of(&lines, off2 + 1, on("M7")).is_none(), "{name}: nothing switches on after the last off");
        // The second tool's call sits between the first M9 and the mist.
        let tool_word = |w: &str| w.starts_with("TOOL") || (w.starts_with('T') && w[1..].starts_with(|c: char| c.is_ascii_digit() || c == '='));
        let second_call = lines[off1..m7].iter().any(|l| l.split_whitespace().any(tool_word));
        assert!(second_call, "{name}: the tool change lies between M9 and M7: {}", out.program);
        // TinyG explicitly shuts outputs off again before each manual tool stop.
        let expected = if name == "TinyG" { 3 } else { 2 };
        assert_eq!(lines[off1..].iter().filter(|l| on("M9")(l)).count(), expected, "{name}: section ends plus the manual-change safety off");
    }
}

#[test]
fn coolant_off_never_turns_fluid_on_for_its_section() {
    let (world, _h) = two_faces(CoolantMode::Off, CoolantMode::Flood);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let m8 = index_of(&lines, 0, |l| has_word(l, "M8")).expect("op 2 floods");
        let m9s: Vec<usize> = (0..lines.len()).filter(|&i| has_word(lines[i], "M9")).collect();
        if name == "TinyG" {
            assert_eq!(m9s.len(), 3, "two manual-change safety offs and one flooded-section end");
            assert!(m9s[0] < m8 && m9s[1] < m8 && m9s[2] > m8);
            assert!(!lines[..m8].iter().any(|l| has_word(l, "M7") || has_word(l, "M8")), "dry section never enables coolant");
        } else {
            assert_eq!(m9s.len(), 1, "{name}: only the flooded section switches off: {}", out.program);
            assert!(m9s[0] > m8, "{name}: the dry op wrote no off word before the flood");
        }
    }
}

#[test]
fn a_coolant_the_machine_states_no_code_for_blocks_the_post_by_name() {
    // The default VMC lists Flood and Mist only: through-tool has no code.
    let (world, _h) = two_faces(CoolantMode::ThroughTool, CoolantMode::Flood);
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(
            out.warnings.iter().any(|w| w.contains("through-tool")),
            "{name}: refused by name, never posted dry: {:?}",
            out.warnings
        );
    }
    let path = std::env::temp_dir().join(format!("camcraft-coolant-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let err = posting::post_setup(&world, &path).expect_err("the gate holds the program");
    assert!(err.contains("through-tool"), "{err}");
    assert!(!path.exists(), "a refusal leaves no file");
}

#[test]
fn a_machine_with_a_through_spindle_entry_posts_its_code() {
    let (world, mut history) = two_faces(CoolantMode::ThroughTool, CoolantMode::Flood);
    let mounted = camcraft_app::machine::Machine {
        model: "TscMill".into(),
        coolants: vec![
            camcraft_app::machine::Coolant { name: "Flood".into(), m_code: "M8".into() },
            camcraft_app::machine::Coolant { name: "Through-spindle".into(), m_code: "M88".into() },
        ],
        safe_retract_z: Some(0.0), // #503: the Siemens post asks the machine where "up" is
        ..Default::default()
    };
    let load = camcraft_app::commands::LoadMachine::new(&world, "tsc.mch".into(), Some(mounted));
    history.execute(&world, load.expect("sane"));
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let m88 = index_of(&lines, 0, |l| has_word(l, "M88")).unwrap_or_else(|| panic!("{name}: through-tool on: {}", out.program));
        let m8 = index_of(&lines, 0, |l| has_word(l, "M8")).expect("flood on");
        assert!(m88 < m8, "{name}: op 1 through-tool, op 2 flood: {}", out.program);
    }
}

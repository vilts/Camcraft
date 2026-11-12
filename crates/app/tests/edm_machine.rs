//! #530 (part of #290): the `wire_edm` and `sinker_edm` machine kinds
//! and the `.mch` `edm` section. Both kinds are declared-only; the edm
//! keys are optional (absent = not stated), scale with the file's unit
//! and refuse the file when garbled. No milling op enters a document
//! whose setup mounts an EDM, and - the post-time half of the family
//! law - an op made on a mill is refused by name at post once its setup
//! is remounted on an EDM. A project file round-trips the edm section.
//!
//! Gate map (#530): covers app::machine (validate, draft),
//! app::posting (post_serves, op_streams), app::posting_family (the
//! family gate at post), app::project (Machine round trip). The pure
//! `.mch` parser cases live in camcraft-post (`machine::edm::tests`).
//! Provenance: our own fixtures (`tests/edm/*.mch`, empty templates
//! with no vendor values; they join the shipped library with their
//! kind's first post, #533 / #542).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SetStock};
use camcraft_app::machine::validate::validate_text;
use camcraft_app::machine::{self, kind, parse_mch, Kind};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};

const WIRE: &str = include_str!("edm/wire-edm.mch");
const SINKER: &str = include_str!("edm/sinker-edm.mch");
const TOOL: &str = "EM-0400-2FL";

fn with_edm(base: &str, edm: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(base).expect("template json");
    v["edm"] = serde_json::from_str(edm).expect("edm json");
    v.to_string()
}

#[test]
fn the_templates_declare_their_kinds_and_state_nothing() {
    for (text, want, letters) in [(WIRE, Kind::WireEdm, ["X", "Y", "U", "V"].as_slice()), (SINKER, Kind::SinkerEdm, ["X", "Y", "Z"].as_slice())] {
        let m = parse_mch(text).expect("template parses");
        assert_eq!(kind(Some(&m)), want, "{}", m.model);
        let edm = m.edm.as_ref().expect("an edm section");
        assert_eq!(*edm, machine::Edm::default(), "{}: no vendor values shipped", m.model);
        assert!(m.is_sane(), "{}", m.model);
        let names: Vec<&str> = m.axes.iter().map(|a| a.name.as_str()).collect();
        for l in letters {
            assert!(names.contains(l), "{}: axis {l} in {names:?}", m.model);
        }
        assert_eq!(m.spindles, None, "{}: an EDM has no spindle", m.model);
        assert!(m.summary().starts_with(want.label()), "{}", m.summary());
    }
}

/// The Machines pane's validator lists an EDM's spindle and coolant by
/// name, as the reader refuses them (review #551 request 3; the parser's
/// cases live in camcraft-post `machine::edm::tests`).
#[test]
fn the_machines_pane_lists_an_edm_spindle_and_coolant_by_name() {
    for text in [WIRE, SINKER] {
        assert!(validate_text(text).is_empty(), "{:?}", validate_text(text));
        let bad = text.replace(
            r#""type": "head" }"#,
            r#""type": "head", "spindle": { "max_speed": 12000 }, "tool_station": { "coolants": [ { "name": "Flood", "on": "M8", "off": "M9" } ] } }"#,
        );
        assert_ne!(bad, text, "the template has a head");
        let keys: Vec<String> = validate_text(&bad).into_iter().map(|r| r.key).collect();
        assert!(keys.contains(&"spindle.max_speed".to_owned()) && keys.contains(&"coolants".to_owned()), "{keys:?}");
    }
    assert!(!posting::post_serves("Fanuc", Kind::WireEdm) && !posting::post_serves("Fanuc Lathe", Kind::SinkerEdm));
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

fn mill_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

fn mount(world: &World, h: &mut History, text: &str) {
    let m = parse_mch(text).expect("parses");
    h.execute(world, LoadMachine::new(world, "edm.mch".into(), Some(m)).expect("mount"));
}

#[test]
fn no_milling_op_enters_an_edm_setup() {
    for text in [WIRE, SINKER] {
        let (world, mut h) = mill_doc();
        mount(&world, &mut h, text);
        let Err(err) = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()) else {
            panic!("a face mill was admitted on an EDM");
        };
        assert!(err.to_string().contains("milling work"), "{err}");
    }
}

#[test]
fn a_milling_op_remounted_onto_an_edm_is_refused_at_post() {
    let (world, mut h) = mill_doc();
    h.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()).expect("face on a mill"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    assert!(posting::op_streams(&world).is_ok(), "the mill posts it");
    mount(&world, &mut h, WIRE);
    let err = posting::op_streams(&world).expect_err("an EDM cannot run a face mill");
    assert!(err.contains("Face1") && err.contains("wire EDM"), "{err}");
    h.undo(&world); // back on the mill
    assert!(posting::op_streams(&world).is_ok(), "undo puts the mill back");
}

#[test]
fn a_project_round_trips_the_edm_section() {
    let (world, mut h) = mill_doc();
    mount(
        &world,
        &mut h,
        &with_edm(WIRE, r#"{ "lower_guide_z": 0, "upper_guide_z": 120, "max_taper_deg": 10, "wire_diameters": [0.25] }"#),
    );
    let path = std::env::temp_dir().join(format!("camcraft-530-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let back = new_world();
    project::load(&back, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(machine::kind_of(&back), Kind::WireEdm);
    let (_, m) = machine::of(&back);
    let e = m.and_then(|m| m.edm).expect("edm survives the file");
    assert_eq!((e.upper_guide_z, e.max_taper_deg, e.wire_diameters), (Some(120.0), Some(10.0), vec![0.25]));
}

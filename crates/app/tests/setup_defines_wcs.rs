//! #329: the setup defines the work offset - a height measured from it
//! names no slot of its own. An older (v51) file whose heights named a
//! slot migrates onto its setup when the setup agreed (and posts exactly
//! as it did), and is refused by name when its operations disagreed or
//! nothing can carry the slot - never silently re-pointed.
//! Provenance: our own fixtures over RS-274 work-offset semantics.
//!
//! Gate map (#329): covers app::project (open, save, the v51 -> v52
//! work-offset rung), app::setup (wcs_slot), app::posting (post_setup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupWorkOffset, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::project::{self, ProjectError};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, setup};

const TOOL: &str = "EM-0400-2FL";

fn face(reference: HeightReference) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(reference, 25.0),
        cut_z: Height::of(reference, 19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// Where the v51 fixtures' slots sat: G54 at the origin, G55 / G56 off it.
const SLOTS_AT: [(&str, f64, f64); 3] = [("G54", 0.0, 0.0), ("G55", 10.0, -2.0), ("G56", -5.0, -4.0)];

/// A 60 x 60 x 20 box, a materialized Setup1 labelled `pick` whose one
/// WCS (#342) sits where that slot sat in [`SLOTS_AT`], and one generated
/// face per entry of `ops` (all measured from the work offset unless
/// `Fixed`).
fn document(pick: Option<usize>, ops: &[(&str, HeightReference)]) -> World {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let (_, x, z) = SLOTS_AT[pick.unwrap_or(0).min(SLOTS_AT.len() - 1)];
    let loc = Location { pos: Point3::new(x, 0.0, z), rot: IDENTITY_Q };
    h.execute(&world, SetWcsOrigin::new(&world, loc).expect("place the WCS"));
    if pick.is_some() {
        h.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", pick).expect("pick"));
    }
    for (name, r) in ops {
        let op = CreateOperation::new(&world, name, TOOL, SrcRef::Stock, face(*r));
        h.execute(&world, op.expect("create"));
        h.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    world
}

fn saved(world: &World, tag: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-329-{tag}-{}.ccam", std::process::id()));
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    text
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-329-{tag}-{}.nc", std::process::id()));
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

/// Every work-offset height of the named operations gets `slot` - the
/// v51 spelling (`{"work_offset": {"slot": n}}`).
fn name_slots(v: &mut Value, slot: u64) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Object(r)) = m.get_mut("reference")
                && let Some(w) = r.get_mut("work_offset")
            {
                *w = json!({ "slot": slot });
            }
            m.values_mut().for_each(|c| name_slots(c, slot));
        }
        Value::Array(a) => a.iter_mut().for_each(|c| name_slots(c, slot)),
        _ => {}
    }
}

/// A current save rewritten as the v51 file the old law wrote: version
/// 51, the setup's slot list, the one WCS named by its slot and the other
/// [`SLOTS_AT`] slots placed beside it, the setup's pick dropped
/// (`keep_pick` false) and each operation's work-offset heights naming
/// the slot `slots` gives it.
fn as_v51(text: &str, keep_pick: bool, slots: &[(&str, u64)]) -> String {
    let mut v: Value = serde_json::from_str(text).expect("json");
    v["version"] = json!(51);
    let entities = v["entities"].as_array_mut().expect("entities");
    let word = entities
        .iter()
        .find_map(|e| e["setup"]["work_offset"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "G54".to_owned());
    for e in entities.iter_mut() {
        if e["kind"] == json!("wcs") {
            e["name"] = json!(word);
        }
        if let Some(s) = e.get_mut("setup").and_then(Value::as_object_mut) {
            s.insert("wcs".into(), json!(["G54", "G55", "G56", "G57", "G58", "G59"]));
        }
    }
    for (slot, x, z) in SLOTS_AT.iter().filter(|(s, _, _)| *s != word) {
        entities.push(json!({"name": slot, "translation": [x, 0.0, z], "kind": "wcs"}));
    }
    for e in entities {
        if !keep_pick && let Some(s) = e.get_mut("setup").and_then(Value::as_object_mut) {
            s.remove("work_offset");
        }
        let name = e["name"].as_str().unwrap_or_default().to_owned();
        if let Some((_, slot)) = slots.iter().find(|(n, _)| *n == name) {
            name_slots(e, *slot);
        }
    }
    serde_json::to_string_pretty(&v).expect("text")
}

#[test]
fn a_v51_setup_whose_heights_agreed_takes_their_slot_and_posts_identically() {
    // What the old law posted: an unpicked setup, heights on G55 -> G55.
    let world = document(Some(1), &[("Face1", HeightReference::WorkOffset)]);
    let now = saved(&world, "agree");
    assert!(now.contains("\"work_offset\": {}"), "a v52 height names no slot: {now}");
    let expected = post(&world, "agree-now").expect("the G55 setup posts");
    assert!(expected.contains("G55"), "{expected}");

    let old = as_v51(&now, false, &[("Face1", 1)]);
    assert!(old.contains("\"slot\": 1") && !old.contains("\"work_offset\": \"G55\""), "{old}");
    let w2 = new_world();
    project::open(&w2, &old).expect("an agreeing v51 file opens");
    assert_eq!(setup::work_offset(&w2, "Setup1"), Some(1), "the slot moved onto the setup");
    assert_eq!(post(&w2, "agree-old").expect("posts without regenerating"), expected, "byte-identical post");
    // #342: the posted slot's entity is the setup's one WCS; the slots
    // nobody posted were dropped.
    assert_eq!(setup::wcs_entity_name(&w2, Some("Setup1")), "G55");
    let resaved = saved(&w2, "agree-resave");
    assert!(resaved.contains("\"wcs\": \"G55\"") && !resaved.contains("\"name\": \"G56\""), "{resaved}");

    // A G54-only setup migrates to the default: nothing written.
    let g54 = document(None, &[("Face1", HeightReference::WorkOffset)]);
    let now = saved(&g54, "g54");
    let w3 = new_world();
    project::open(&w3, &as_v51(&now, false, &[("Face1", 0)])).expect("G54 opens");
    assert_eq!(setup::work_offset(&w3, "Setup1"), None);
    assert_eq!(saved(&w3, "g54-resave"), now);
}

#[test]
fn a_v51_setup_whose_operations_disagreed_is_refused_by_name() {
    // Face1 on G55, Face2 on G56 - one setup, two zeros.
    let world = document(None, &[("Face1", HeightReference::WorkOffset), ("Face2", HeightReference::WorkOffset)]);
    let old = as_v51(&saved(&world, "split"), false, &[("Face1", 1), ("Face2", 2)]);
    let err = project::open(&new_world(), &old).expect_err("two offsets in one setup");
    assert!(matches!(err, ProjectError::WorkOffsetConflict(_)), "{err}");
    let msg = err.to_string();
    assert!(msg.contains("setup Setup1: Face1 (G55); Face2 (G56)"), "{msg}");
    assert!(msg.contains("re-point"), "{msg}");

    // An op measuring from nothing posted at G54 - it disagrees with a G55 one.
    let world = document(None, &[("Face1", HeightReference::WorkOffset), ("Face2", HeightReference::Fixed)]);
    let old = as_v51(&saved(&world, "mixed"), false, &[("Face1", 1)]);
    let msg = project::open(&new_world(), &old).expect_err("G55 and G54").to_string();
    assert!(msg.contains("Face1 (G55); Face2 (G54)"), "{msg}");

    // A picked setup whose heights named another slot (the old post
    // refusal) is refused at load, naming the pick.
    let world = document(Some(1), &[("Face1", HeightReference::WorkOffset)]);
    let old = as_v51(&saved(&world, "picked"), true, &[("Face1", 2)]);
    let msg = project::open(&new_world(), &old).expect_err("G56 heights, G55 pick").to_string();
    assert!(msg.contains("setup Setup1 (setup picks G55): Face1 (G55, G56)"), "{msg}");
}

#[test]
fn a_document_without_setups_cannot_carry_a_slot_and_says_so() {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face(HeightReference::WorkOffset));
    h.execute(&world, op.expect("create"));
    let now = saved(&world, "nosetup");
    assert_eq!(setup::wcs_slot(&world, None), 0, "no setup reads G54");
    let msg = project::open(&new_world(), &as_v51(&now, false, &[("Face1", 1)]))
        .expect_err("nothing to carry G55")
        .to_string();
    assert!(msg.contains("no setup to carry G55 (operations: Face1)"), "{msg}");
    project::open(&new_world(), &as_v51(&now, false, &[("Face1", 0)])).expect("G54 needs no carrier");
}

#[test]
fn a_v52_height_naming_a_slot_is_corrupt() {
    let world = document(None, &[("Face1", HeightReference::WorkOffset)]);
    let mut v: Value = serde_json::from_str(&saved(&world, "v52")).expect("json");
    name_slots(&mut v, 1);
    let err = project::open(&new_world(), &v.to_string()).expect_err("v52 names no slot");
    assert!(matches!(err, ProjectError::Corrupt(_)), "{err}");
    assert!(err.to_string().contains("Face1"), "{err}");
}

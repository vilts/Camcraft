//! #342: one WCS per setup - the browser's single "WCS" node is one
//! origin, and `G54`..`G59` is only the word the setup's post writes.
//! A v53 file migrates each setup onto the origin of the slot it posted
//! (posting byte-identical), drops the slots nobody posted with a load
//! note, keeps an orphaned operation's zero, and a v54 file still
//! listing slots is refused as corrupt; the posted slot keeps its #327
//! placement check through the rung. WCS edits undo inside their own
//! setup; a relabel never reaches another origin.
//! Provenance: our own fixtures over RS-274 work-offset semantics.
//!
//! Gate map (#342): covers app::project (open_noted, the v53 -> v54
//! one-WCS rung), app::setup (wcs_entity, wcs_entity_name, wcs_label),
//! app::commands (SetWcsOrigin, SetSetupWorkOffset, CreateSetup),
//! app::posting (post_setup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupWorkOffset, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project::{self, ProjectError};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, setup};

const TOOL: &str = "EM-0400-2FL";
const SLOTS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];

fn at(x: f64, y: f64, z: f64) -> Location {
    Location { pos: Point3::new(x, y, z), rot: IDENTITY_Q }
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// Stock, tool and - when `setups` - a materialized Setup1 labelled
/// `label` whose WCS sits at (10, 0, -2); then one generated face.
fn block(setups: bool, label: Option<usize>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    if setups {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    }
    if label.is_some() {
        h.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", label).expect("label"));
    }
    h.execute(&world, SetWcsOrigin::new(&world, at(10.0, 0.0, -2.0)).expect("place the WCS"));
    face_op(&world, &mut h, "Face1");
    (world, h)
}

fn face_op(world: &World, h: &mut History, name: &str) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face());
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn saved(world: &World, tag: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-342-{tag}-{}.ccam", std::process::id()));
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    text
}

fn post(world: &World, tag: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-342-{tag}-{}.nc", std::process::id()));
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out.expect("posts")
}

/// A current save rewritten as the v53 file the slot law wrote: version
/// 53, every setup listing its six slot entities (`G54`.. for the
/// materialized first setup, `<setup>/G5x` for the others), each WCS
/// entity renamed to the slot its setup posted, plus the `extra` placed
/// slots beside them.
fn as_v53(text: &str, extra: &[(&str, [f64; 3])]) -> String {
    let mut v: Value = serde_json::from_str(text).expect("json");
    v["version"] = json!(53);
    let entities = v["entities"].as_array_mut().expect("entities");
    let mut renames: Vec<(String, String)> = Vec::new();
    for e in entities.iter_mut() {
        let name = e["name"].as_str().unwrap_or_default().to_owned();
        let Some(s) = e.get_mut("setup").and_then(Value::as_object_mut) else {
            continue;
        };
        let word = s.get("work_offset").and_then(Value::as_str).unwrap_or("G54").to_owned();
        let own = s.get("wcs").and_then(Value::as_str).unwrap_or_default().to_owned();
        let prefix = if own == "G54" { String::new() } else { format!("{name}/") };
        s.insert("wcs".into(), json!(SLOTS.iter().map(|w| format!("{prefix}{w}")).collect::<Vec<_>>()));
        renames.push((own, format!("{prefix}{word}")));
    }
    for e in entities.iter_mut() {
        if e["kind"] == json!("wcs")
            && let Some((_, new)) = renames.iter().find(|(old, _)| e["name"] == json!(old))
        {
            e["name"] = json!(new);
        }
    }
    for (name, [x, y, z]) in extra {
        entities.push(json!({"name": name, "translation": [x, y, z], "kind": "wcs"}));
    }
    serde_json::to_string_pretty(&v).expect("text")
}

#[test]
fn a_v53_single_placed_slot_becomes_the_setups_wcs_and_posts_identically() {
    let (world, _) = block(true, Some(1));
    let expected = post(&world, "single-now");
    assert!(expected.contains("G55"), "{expected}");
    let old = as_v53(&saved(&world, "single"), &[]);
    assert!(old.contains("\"name\": \"G55\""), "{old}");
    let w = new_world();
    let notes = project::open_noted(&w, &old).expect("a v53 file opens");
    assert!(notes.is_empty(), "nothing dropped: {notes:?}");
    assert_eq!(setup::wcs_entity_name(&w, Some("Setup1")), "G55", "the posted slot's entity");
    assert_eq!(post(&w, "single-old"), expected, "byte-identical post");
    let resaved = saved(&w, "single-resave");
    assert!(resaved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)));
    assert!(resaved.contains("\"wcs\": \"G55\""), "one WCS name: {resaved}");
}

#[test]
fn several_placed_slots_keep_the_posted_one_and_drop_the_rest_with_a_note() {
    let (world, _) = block(true, Some(1));
    let expected = post(&world, "several-now");
    let old = as_v53(&saved(&world, "several"), &[("G54", [0.0, 0.0, 0.0]), ("G57", [1.0, 2.0, 3.0])]);
    let w = new_world();
    let notes = project::open_noted(&w, &old).expect("opens");
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("'G54'")) && notes.iter().any(|n| n.contains("'G57' (X1.000 Y2.000 Z3.000)")), "{notes:?}");
    assert!(notes.iter().all(|n| n.contains("no setup posted it")), "{notes:?}");
    assert!(model::find_by_name(&w, "G54").is_none() && model::find_by_name(&w, "G57").is_none());
    assert_eq!(post(&w, "several-old"), expected, "the posted origin is untouched");
}

#[test]
fn a_document_without_setups_keeps_g54_and_drops_the_other_slots() {
    let (world, _) = block(false, None);
    let expected = post(&world, "nosetup-now");
    let now = saved(&world, "nosetup");
    let old = as_v53(&now, &[("G55", [5.0, 5.0, 5.0])]);
    let w = new_world();
    let notes = project::open_noted(&w, &old).expect("opens");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("'G55'"), "{notes:?}");
    assert_eq!(post(&w, "nosetup-old"), expected);
    assert_eq!(saved(&w, "nosetup-resave"), now, "and it saves as the v54 document it was");
}

#[test]
fn two_setups_keep_their_own_posted_origins() {
    let (world, mut h) = block(true, None);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("Setup2 stock"));
    h.execute(&world, SetSetupWorkOffset::new(&world, "Setup2", Some(1)).expect("label G55"));
    h.execute(&world, SetWcsOrigin::new(&world, at(100.0, 0.0, 0.0)).expect("place Setup2's WCS"));
    face_op(&world, &mut h, "Face2");
    let expected = post(&world, "two-now");
    assert!(expected.contains("G54") && expected.contains("G55"), "{expected}");
    let old = as_v53(&saved(&world, "two"), &[("Setup2/G54", [5.0, 5.0, 5.0])]);
    assert!(old.contains("\"name\": \"Setup2/G55\""), "{old}");
    let w = new_world();
    let notes = project::open_noted(&w, &old).expect("opens");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("'Setup2/G54'"), "{notes:?}");
    assert_eq!(setup::wcs_entity_name(&w, Some("Setup1")), "G54");
    assert_eq!(setup::wcs_entity_name(&w, Some("Setup2")), "Setup2/G55");
    assert_eq!(post(&w, "two-old"), expected);
}

#[test]
fn an_operation_in_a_setup_the_file_lacks_keeps_its_zero() {
    let (world, _) = block(true, None);
    let mut v: Value = serde_json::from_str(&as_v53(&saved(&world, "gone"), &[("Gone/G54", [3.0, 0.0, 0.0])]))
        .expect("json");
    for e in v["entities"].as_array_mut().expect("entities") {
        if e["name"] == json!("Face1") {
            e["operation"]["setup"] = json!("Gone");
        }
    }
    let w = new_world();
    let notes = project::open_noted(&w, &v.to_string()).expect("opens");
    assert!(notes.is_empty(), "{notes:?}");
    let e = setup::wcs_entity(&w, Some("Gone")).expect("renamed onto the v54 name");
    assert_eq!(model::name_of(&w, &e).as_deref(), Some("Gone/WCS"));
    assert_eq!(model::transform_of(&w, &e).expect("loc").pos.x, 3.0, "the same zero");
}

#[test]
fn a_v54_setup_listing_slots_is_corrupt() {
    let (world, _) = block(true, None);
    let mut v: Value = serde_json::from_str(&saved(&world, "corrupt")).expect("json");
    for e in v["entities"].as_array_mut().expect("entities") {
        if let Some(s) = e.get_mut("setup").and_then(Value::as_object_mut) {
            s.insert("wcs".into(), json!(["G54", "G55"]));
        }
    }
    let err = project::open(&new_world(), &v.to_string()).expect_err("one WCS per setup");
    assert!(matches!(err, ProjectError::Corrupt(_)), "{err}");
    assert!(err.to_string().contains("Setup1"), "{err}");
}

#[test]
fn a_wcs_edit_undoes_inside_its_own_setup_and_a_relabel_reaches_no_other_origin() {
    let (world, mut h) = block(true, None);
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    h.execute(&world, SetWcsOrigin::new(&world, at(100.0, 0.0, 0.0)).expect("place"));
    h.execute(&world, SetWcsOrigin::new(&world, at(50.0, 0.0, 0.0)).expect("move"));
    let x = |w: &World, s: &str| {
        setup::wcs_entity(w, Some(s)).and_then(|e| model::transform_of(w, &e)).map(|l| l.pos.x)
    };
    assert_eq!(x(&world, "Setup2"), Some(50.0));
    assert!(h.undo(&world));
    assert_eq!(x(&world, "Setup2"), Some(100.0), "undo restores the setup's previous origin");
    assert_eq!(x(&world, "Setup1"), Some(10.0), "and leaves the other setup's alone");
    assert!(h.redo(&world));
    assert_eq!(x(&world, "Setup2"), Some(50.0));
    // Every word labels the same one origin.
    let wcs_count = |w: &World| model::named_entities(w).iter().filter(|(_, e)| model::is_wcs(w, e)).count();
    let before = wcs_count(&world);
    for (slot, word) in SLOTS.iter().enumerate().skip(1) {
        h.execute(&world, SetSetupWorkOffset::new(&world, "Setup2", Some(slot)).expect("relabel"));
        assert_eq!(x(&world, "Setup2"), Some(50.0));
        assert_eq!(setup::wcs_label(&world, Some("Setup2")), format!("WCS ({word})"));
    }
    assert_eq!(wcs_count(&world), before, "a relabel spawns no origin");
}

#[test]
fn a_v53_posted_slot_with_a_degenerate_rotation_is_refused_after_the_rung() {
    // #327 on the #342 migration path: the slot the v53 setup posted
    // becomes its one WCS, and the load's placement check still runs
    // on it - a zero-norm rotation is refused by name, never loaded as
    // the identity.
    let (world, _) = block(true, Some(1));
    let mut v: Value = serde_json::from_str(&as_v53(&saved(&world, "degen"), &[])).expect("json");
    let g55 = v["entities"]
        .as_array_mut()
        .expect("entities")
        .iter_mut()
        .find(|e| e["name"] == json!("G55"))
        .expect("the posted slot's row");
    g55["rotation"] = json!([0.0, 0.0, 0.0, 0.0]);
    let err = project::open(&new_world(), &v.to_string()).expect_err("a zero-norm WCS must not load");
    assert!(
        err.to_string().contains("WCS 'G55' has a non-finite or degenerate placement"),
        "named refusal, got: {err}"
    );
}

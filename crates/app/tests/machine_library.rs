//! The machine catalogue and validator (issue #287): every shipped
//! machine lists with its kind, axes, units and posts and validates
//! clean; each validation rule refuses a crafted bad file BY NAME; a
//! malformed file is a named refusal, never a panic.
//!
//! Fixture law (clean-room): the crafted files are our own values in
//! the `.mch` layout of `assets/machines/AGENTS.md`.
//! Provenance: our own tests, specced from the issue's validation list.
//!
//! Gate map (M4.4): covers app::machine, post::machine, app::machine, post::machine::library, app::machine, post::machine::validate, app::machine, post::machine::sheet, app::posting.

use std::path::PathBuf;

use camcraft_app::machine::library::{self, MachineEntry, MachineSource};
use camcraft_app::machine::validate::validate_text;
use camcraft_app::machine::Kind;
use camcraft_app::posting;

/// A good 3-axis mill; `patch` edits its JSON before it is written out.
fn mill(patch: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v = serde_json::json!({
      "general": { "model": "TEST MILL", "vendor": "Testbench", "kind": "mill" },
      "kinematics": { "default": {
        "units": { "length": "mm", "angle": "degrees" },
        "parts": [
          { "id": "base", "name": "table", "type": "table", "parts": [
            { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -200.0, "max": 0.0 }, "parts": [
              { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -100.0, "max": 100.0 }, "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -150.0, "max": 150.0 } }
              ] }
            ] },
            { "id": "head", "name": "head", "type": "head",
              "spindle": { "min_speed": 100.0, "max_speed": 8000.0, "power": 3.0 },
              "tool_station": { "max_tool_length": 120.0,
                "coolants": [ { "name": "Flood", "mCode": "M8" }, "Mist" ] } }
          ] }
        ] } },
      "controller": { "synced_configuration": { "parts": {
        "X": { "coordinate": 0, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 },
        "Y": { "coordinate": 1, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 },
        "Z": { "coordinate": 2, "max_normal_speed": 1500.0, "max_rapid_speed": 3000.0 }
      } } },
      "tooling": { "number_of_tools": 8 }
    });
    patch(&mut v);
    v.to_string()
}

fn at<'a>(v: &'a mut serde_json::Value, ptr: &str) -> &'a mut serde_json::Value {
    v.pointer_mut(ptr).unwrap_or_else(|| panic!("fixture has {ptr}"))
}

/// The refusals' keys, for asserting a rule refused by name.
fn keys(text: &str) -> Vec<String> {
    validate_text(text).into_iter().map(|r| r.key).collect()
}

fn one_refusal(text: &str, key: &str, rule_has: &str) {
    let r = validate_text(text);
    assert!(
        r.iter().any(|r| r.key == key && r.rule.contains(rule_has)),
        "expected a refusal `{key}: ..{rule_has}..`, got {r:?}"
    );
}

const X_TRAVEL: &str = "/kinematics/default/parts/0/parts/0/parts/0/parts/0/travel";
const SPINDLE: &str = "/kinematics/default/parts/0/parts/1/spindle";
const COOLANTS: &str = "/kinematics/default/parts/0/parts/1/tool_station/coolants";

#[test]
fn a_good_crafted_mill_validates_clean() {
    assert_eq!(validate_text(&mill(|_| {})), vec![]);
}

#[test]
fn every_shipped_machine_lists_and_validates_clean() {
    let list = library::builtins();
    assert_eq!(list.len(), posting::MACHINES.len());
    for e in &list {
        assert!(e.is_valid(), "{} refuses: {:?}", e.name, e.refusals);
        assert!(matches!(e.source, MachineSource::Builtin(_)));
        assert!(e.kind.is_some(), "{} has a kind", e.name);
        if e.kind == Some(Kind::Plasma) {
            // #671: the plasma post alone - a cutting post never serves a
            // plasma table.
            assert_eq!(e.posts, posting::PLASMA_POSTS, "{}: only the plasma post serves a plasma table", e.name);
        } else {
            assert!(!e.posts.is_empty(), "{} has a post", e.name);
        }
        assert!(!e.groups.is_empty(), "{} has a property sheet", e.name);
    }
    let lathe = list.iter().find(|e| e.name == "Lathe").expect("the library has a lathe");
    assert_eq!(lathe.kind, Some(Kind::Lathe));
    assert_eq!(lathe.axes, vec!["X", "Z"]);
    assert!(lathe.posts.contains(&"Fanuc Lathe") && !lathe.posts.contains(&"Fanuc"));
    let mt = list.iter().find(|e| e.name == "Mill-turn").expect("the library has a mill-turn");
    assert_eq!(mt.kind, Some(Kind::MillTurn));
    assert_eq!(mt.rotaries, vec!["C"]);
    let tm = list.iter().find(|e| e.name == "Tormach 770MX").expect("the library has the 770MX");
    assert_eq!(tm.units, "inch");
    // The sheet quotes the file's own numbers, not the mm restatement.
    let axes = tm.groups.iter().find(|g| g.title == "Axes").expect("an Axes group");
    let x = axes.rows.iter().find(|(l, _)| l == "X").expect("an X row");
    assert!(x.1.contains("-14 .. 14 in"), "{x:?}");
}

/// Every `.mch` in `assets/machines` is a library entry, except the
/// post-grammar default (`3axis-vmc.mch` is the static mirror form the
/// post reads, not a checker-grammar machine file).
#[test]
fn every_machine_file_in_assets_is_in_the_library() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines");
    let files = library::scan_dir(&dir);
    let named: Vec<String> = posting::MACHINES.iter().map(|(_, t)| t.to_string()).collect();
    for e in files {
        let MachineSource::File(p) = &e.source else { panic!("scan lists files") };
        if p.file_name().is_some_and(|n| n == "3axis-vmc.mch") {
            continue;
        }
        let text = std::fs::read_to_string(p).expect("read the asset");
        assert!(named.contains(&text), "{} is not registered in posting::MACHINES", p.display());
        assert!(e.is_valid(), "{} refuses: {:?}", p.display(), e.refusals);
    }
}

#[test]
fn backwards_travel_refuses_naming_the_axis() {
    let t = mill(|v| *at(v, X_TRAVEL) = serde_json::json!({ "min": 10.0, "max": 5.0 }));
    one_refusal(&t, "X travel", "min 10 must be below max 5");
    let t = mill(|v| *at(v, X_TRAVEL) = serde_json::json!({ "min": "a", "max": 5.0 }));
    one_refusal(&t, "X travel", "must both be numbers");
}

#[test]
fn non_positive_feeds_and_speeds_refuse_by_name() {
    let t = mill(|v| *at(v, "/controller/synced_configuration/parts/Y/max_normal_speed") = 0.0.into());
    one_refusal(&t, "Y feed (max_normal_speed)", "above 0");
    let t = mill(|v| *at(v, "/controller/synced_configuration/parts/Z/max_rapid_speed") = (-5.0).into());
    one_refusal(&t, "Z rapid (max_rapid_speed)", "above 0");
    let t = mill(|v| *at(v, &format!("{SPINDLE}/max_speed")) = "fast".into());
    one_refusal(&t, "spindle max_speed", "above 0");
    let t = mill(|v| *at(v, &format!("{SPINDLE}/min_speed")) = 9000.0.into());
    one_refusal(&t, "spindle", "must not exceed max_speed");
    let t = mill(|v| at(v, SPINDLE)["ranges"] = serde_json::json!([{ "min_speed": 500.0, "max_speed": 100.0 }]));
    one_refusal(&t, "spindle range 1", "below max_speed");
}

#[test]
fn malformed_codes_refuse_by_name() {
    let t = mill(|v| *at(v, COOLANTS) = serde_json::json!([{ "name": "Flood", "mCode": "G8" }]));
    one_refusal(&t, "coolant Flood", "not an M word");
    let t = mill(|v| *at(v, COOLANTS) = serde_json::json!([{ "mCode": "M8" }]));
    one_refusal(&t, "coolant 1", "needs a name");
    // A bare number code is the reader's other legal spelling.
    let t = mill(|v| *at(v, COOLANTS) = serde_json::json!([{ "name": "Flood", "mCode": 8 }]));
    assert_eq!(validate_text(&t), vec![]);
    let t = mill(|v| {
        at(v, "/kinematics/default/parts/0/parts/1/tool_station")["live_tool"] =
            serde_json::json!({ "c_axis_on": "M19", "spindle_on": "M3" });
    });
    one_refusal(&t, "live_tool", "states 2 of the 4 words");
    // A hand-written file with a bare M is flagged in the catalogue too
    // (the same validator), coolant and live-tool word alike.
    let t = mill(|v| *at(v, COOLANTS) = serde_json::json!([{ "name": "Flood", "mCode": "M" }]));
    one_refusal(&t, "coolant Flood", "no number after M");
    let t = mill(|v| {
        at(v, "/kinematics/default/parts/0/parts/1/tool_station")["live_tool"] =
            serde_json::json!({ "c_axis_on": "M", "c_axis_off": "M155", "spindle_on": "M133", "spindle_off": "M135" });
    });
    one_refusal(&t, "live_tool.c_axis_on", "no number after M");
}

#[test]
fn required_keys_per_kind_refuse_by_name() {
    // A mill with no Y.
    let t = mill(|v| {
        let z = at(v, "/kinematics/default/parts/0/parts/0");
        z["parts"] = serde_json::json!([{ "id": "x", "name": "X", "type": "linear" }]);
    });
    one_refusal(&t, "Y axis", "required for a mill machine");
    // A declared mill-turn with no rotary.
    let t = mill(|v| *at(v, "/general/kind") = "mill-turn".into());
    one_refusal(&t, "rotary axis", "required for a mill-turn");
    // A declared lathe needs X and Z only - the mill fixture still has them.
    let t = mill(|v| *at(v, "/general/kind") = "lathe".into());
    assert!(!keys(&t).iter().any(|k| k.ends_with("axis")), "{:?}", validate_text(&t));
    let t = mill(|v| *at(v, "/general/model") = "".into());
    one_refusal(&t, "general.model", "required");
    let t = mill(|v| v["kinematics"] = serde_json::json!({}));
    one_refusal(&t, "kinematics.default.parts", "required");
}

#[test]
fn other_stated_values_refuse_by_name() {
    let t = mill(|v| *at(v, "/general/kind") = "drill press".into());
    one_refusal(&t, "general.kind", "not mill, lathe, mill-turn or plasma");
    let t = mill(|v| *at(v, "/kinematics/default/units/length") = "cm".into());
    one_refusal(&t, "units.length", "not mm or inch");
    let t = mill(|v| *at(v, "/tooling/number_of_tools") = 0.into());
    one_refusal(&t, "tooling.number_of_tools", "above 0");
    let t = mill(|v| {
        at(v, "/kinematics/default/parts/0/parts/0/parts/0/parts/0")["name"] = "Y".into();
    });
    one_refusal(&t, "Y", "stated twice");
}

#[test]
fn malformed_files_refuse_by_name_and_never_panic() {
    for (text, key) in [
        ("", "file"),
        ("{ not json", "file"),
        ("[1, 2, 3]", "file"),
        ("\"a machine\"", "file"),
    ] {
        let r = validate_text(text);
        assert_eq!(r.len(), 1, "{text:?}: {r:?}");
        assert_eq!(r[0].key, key);
        let e = MachineEntry::from_text("bad", MachineSource::Builtin("bad"), text);
        assert!(!e.is_valid() && e.kind.is_none() && e.groups.is_empty());
    }
    // A rotary with a garbled direction - the reader refuses it; so
    // does the validator, naming the axis.
    let t = mill(|v| {
        at(v, "/kinematics/default/parts/0/parts")
            .as_array_mut()
            .expect("parts list")
            .push(serde_json::json!({ "id": "c", "name": "C", "type": "rotary", "axis": [0, "z", 1] }));
    });
    one_refusal(&t, "C axis", "three finite numbers");
}

#[test]
fn a_user_file_lists_by_its_model_and_an_unreadable_one_by_name() {
    let dir = std::env::temp_dir().join(format!("camcraft_machine_library_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("shop.mch"), mill(|_| {})).expect("write");
    std::fs::write(dir.join("broken.mch"), "{").expect("write");
    std::fs::write(dir.join("notes.txt"), "not a machine").expect("write");
    let list = library::scan_dir(&dir);
    assert_eq!(list.len(), 2, "only .mch files list");
    let broken = &list[0];
    assert_eq!(broken.name, "broken.mch");
    assert_eq!(broken.refusals[0].key, "file");
    let shop = &list[1];
    assert_eq!(shop.name, "TEST MILL");
    assert!(shop.is_valid(), "{:?}", shop.refusals);
    assert_eq!(shop.source, MachineSource::File(dir.join("shop.mch")));
    let gone = MachineEntry::from_file(&dir.join("gone.mch"));
    assert!(gone.refusals[0].rule.contains("cannot be read"));
    assert!(library::scan_dir(&dir.join("no-such-folder")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_catalogue_adds_the_documents_machine_file() {
    let world = camcraft_app::world::new_world();
    assert_eq!(library::enumerate_with(&world, None).len(), posting::MACHINES.len(), "a built-in mount adds nothing");
    let p = std::env::temp_dir().join(format!("camcraft_machine_library_doc_{}.mch", std::process::id()));
    std::fs::write(&p, mill(|_| {})).expect("write");
    let m = camcraft_app::machine::parse_mch(&mill(|_| {})).expect("parse");
    camcraft_app::machine::set(&world, &p.display().to_string(), Some(&m));
    let list = library::enumerate_with(&world, None);
    assert_eq!(list.len(), posting::MACHINES.len() + 1);
    assert_eq!(list.last().map(|e| e.source.clone()), Some(MachineSource::File(p.clone())));
    let _ = std::fs::remove_file(&p);
}

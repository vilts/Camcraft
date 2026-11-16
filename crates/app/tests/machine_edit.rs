//! The machine editor (issue #383): a `.mch` reads, edits, validates
//! and writes back with every unknown key kept; a refused file is never
//! written and each refusal lands on its field; the write is atomic; the
//! user machines folder resolves from `CAMCRAFT_MACHINES`, else the
//! platform data folder; Save / Save As keep shipped files read-only.
//!
//! Fixture law (clean-room): the crafted files are our own values in
//! the `.mch` layout of `assets/machines/AGENTS.md`.
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! Gate map (M4.4): covers app::machine, post::machine::draft, app::machine, post::machine::fields, app::machine, post::machine::store, app::machine, post::machine::library.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use camcraft_app::machine::draft::MachineDraft;
use camcraft_app::machine::fields::{self, FieldGroup, FieldKind};
use camcraft_app::machine::library::{self, MachineSource};
use camcraft_app::machine::store::{self, SaveError};
use camcraft_app::machine::Kind;
use serde_json::Value;

/// A fresh, empty folder for one test.
fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_machine_edit_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

/// A hand-written mill with keys the editor knows nothing about: a
/// top-level vendor block, a note on an axis part, a key in general.
const HAND_WRITTEN: &str = r#"{
  "general": { "model": "SHOP MILL", "kind": "mill", "shop_asset_tag": "B-17" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
      { "id": "x", "name": "X", "type": "linear", "travel": { "min": -150.0, "max": 150.0 }, "note": "gib adjusted 2026" },
      { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -100.0, "max": 100.0 } },
      { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -200.0, "max": 0.0 } },
      { "id": "head", "name": "head", "type": "head",
        "spindle": { "min_speed": 100.0, "max_speed": 8000.0 },
        "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
    ] } ]
  } },
  "x_vendor_block": { "probe": "OMP40", "warmup_minutes": 10 }
}"#;

fn field<'a>(groups: &'a [FieldGroup], label: &str) -> &'a fields::Field {
    groups
        .iter()
        .flat_map(|g| &g.fields)
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("no field {label}"))
}

fn edit(d: &mut MachineDraft, label: &str, text: &str) {
    let f = field(&d.groups(), label).clone();
    d.set(&f.pointer, f.kind, text).expect("the edit applies");
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).expect("read back")).expect("json")
}

#[test]
fn a_hand_written_file_round_trips_with_its_unknown_keys_kept() {
    let d = dir("roundtrip");
    let p = d.join("shop.mch");
    std::fs::write(&p, HAND_WRITTEN).expect("write");
    let mut m = MachineDraft::open(&MachineSource::File(p.clone())).expect("opens");
    assert!(!m.read_only && !m.dirty);
    edit(&mut m, "X travel max", "175");
    edit(&mut m, "Coolant Flood code", "M7");
    assert!(m.dirty);
    assert_eq!(m.save().expect("saves"), p);
    let v = read_json(&p);
    assert_eq!(v.pointer("/x_vendor_block/probe").and_then(Value::as_str), Some("OMP40"));
    assert_eq!(v.pointer("/x_vendor_block/warmup_minutes").and_then(Value::as_u64), Some(10));
    assert_eq!(v.pointer("/general/shop_asset_tag").and_then(Value::as_str), Some("B-17"));
    let x = v.pointer("/kinematics/default/parts/0/parts/0").expect("X part");
    assert_eq!(x.get("note").and_then(Value::as_str), Some("gib adjusted 2026"));
    assert_eq!(x.pointer("/travel/max").and_then(Value::as_f64), Some(175.0));
    assert_eq!(
        v.pointer("/kinematics/default/parts/0/parts/3/tool_station/coolants/0/mCode").and_then(Value::as_str),
        Some("M7")
    );
    // Everything else is exactly as it was read.
    let mut want: Value = serde_json::from_str(HAND_WRITTEN).expect("json");
    let _ = fields::set(&mut want, "/kinematics/default/parts/0/parts/0/travel/max", FieldKind::Number, "175");
    let _ = fields::set(&mut want, "/kinematics/default/parts/0/parts/3/tool_station/coolants/0/mCode", FieldKind::Text, "M7");
    assert_eq!(v, want);
    // And it lists in the user folder by its model, validating clean.
    let listed = library::scan_dir(&d);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "SHOP MILL");
    assert!(listed[0].is_valid(), "{:?}", listed[0].refusals);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_refused_file_is_not_written_and_each_refusal_sits_on_its_field() {
    let d = dir("refused");
    let p = d.join("shop.mch");
    std::fs::write(&p, HAND_WRITTEN).expect("write");
    let mut m = MachineDraft::open(&MachineSource::File(p.clone())).expect("opens");
    edit(&mut m, "X travel min", "400");
    edit(&mut m, "Spindle max speed", "0");
    edit(&mut m, "Coolant Flood code", "flood please");
    let refusals = m.refusals();
    let keys: Vec<&str> = refusals.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["X travel", "spindle max_speed", "spindle", "coolant Flood"], "{refusals:?}");
    let g = m.groups();
    let on = |label: &str| field(&g, label).refusals(&refusals).iter().map(|r| r.to_string()).collect::<Vec<_>>();
    assert_eq!(on("X travel min"), ["X travel: min 400 must be below max 150"]);
    assert_eq!(on("X travel max"), on("X travel min"), "a travel refusal is on both ends");
    assert!(on("Spindle max speed").iter().any(|r| r.starts_with("spindle max_speed: 0 must be a number above 0")));
    assert_eq!(on("Coolant Flood code"), [r#"coolant Flood: code "flood please" is not an M word like M8 (it must start with M)"#]);
    assert!(on("Y travel min").is_empty());
    assert!(fields::unplaced(&g, &refusals).is_empty(), "every refusal has a field");
    match m.save() {
        Err(SaveError::Refused(r)) => assert_eq!(r, refusals),
        other => panic!("a refused file saved: {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&p).expect("read"), HAND_WRITTEN, "the file on disk is untouched");
    assert!(m.dirty, "the edits are still in the editor");
    // Save As refuses the same way and writes nothing either.
    assert!(matches!(m.save_as(Some(&d), "other", false), Err(SaveError::Refused(_))));
    assert!(!d.join("other.mch").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_refusal_with_no_field_is_listed_above_the_grid() {
    let mut m = MachineDraft::blank(Kind::Mill);
    // Remove the Y axis part: the mill now lacks a required axis.
    if let Some(Value::Array(a)) = m.value.pointer_mut("/kinematics/default/parts/0/parts") {
        a.remove(1);
    }
    let r = m.refusals();
    let g = m.groups();
    let loose: Vec<String> = fields::unplaced(&g, &r).iter().map(|r| r.to_string()).collect();
    assert_eq!(loose, ["Y axis: required for a mill machine"]);
}

#[test]
fn live_tool_codes_are_all_four_or_none() {
    let mut m = MachineDraft::blank(Kind::MillTurn);
    assert!(m.refusals().is_empty(), "{:?}", m.refusals());
    edit(&mut m, "Live tool C on", "M154");
    let r = m.refusals();
    assert_eq!(r.len(), 1);
    let g = m.groups();
    for l in ["Live tool C on", "Live tool C off", "Live tool spindle on", "Live tool spindle off"] {
        assert_eq!(field(&g, l).refusals(&r).len(), 1, "{l} carries the live_tool refusal");
    }
    for (l, c) in [("Live tool C off", "M155"), ("Live tool spindle on", "M133"), ("Live tool spindle off", "M135")] {
        edit(&mut m, l, c);
    }
    assert!(m.refusals().is_empty(), "{:?}", m.refusals());
    // Clearing all four leaves no empty live_tool block behind.
    for l in ["Live tool C on", "Live tool C off", "Live tool spindle on", "Live tool spindle off"] {
        edit(&mut m, l, "");
    }
    assert!(m.refusals().is_empty(), "{:?}", m.refusals());
    assert!(!m.text().contains("live_tool"));
}

#[test]
fn every_blank_kind_starts_with_its_required_keys_and_validates() {
    for (k, axes) in [(Kind::Mill, "XYZ"), (Kind::Lathe, "XZ"), (Kind::MillTurn, "XYZC"), (Kind::Plasma, "XYZ")] {
        let m = MachineDraft::blank(k);
        assert!(m.refusals().is_empty(), "{k:?}: {:?}", m.refusals());
        assert!(m.path.is_none() && !m.read_only && m.dirty);
        let e = library::MachineEntry::from_text("t", MachineSource::Builtin("t"), &m.text());
        assert_eq!(e.kind, Some(k));
        assert_eq!(format!("{}{}", e.axes.concat(), e.rotaries.concat()), axes);
        let g = m.groups();
        for t in ["General", "Axes and travels", "Rotaries and limits", "Spindle range", "Feeds",
                  "Tool changer and stations", "Coolant codes", "Live-tool codes", "Units"] {
            assert!(g.iter().any(|x| x.title == t), "{k:?} lacks the {t} group");
        }
    }
}

#[test]
fn a_template_copy_saves_as_a_new_user_file_and_shipped_files_stay_read_only() {
    let d = dir("template");
    let src = MachineSource::Builtin("Lathe");
    let mut shipped = MachineDraft::open(&src).expect("opens");
    assert!(shipped.read_only);
    assert_eq!(shipped.save(), Err(SaveError::NeedsSaveAs), "Save on a shipped machine forces Save As");
    let mut m = MachineDraft::from_template(&src).expect("copies");
    assert_eq!(m.model(), "TURNING CENTER 2-AXIS copy");
    assert_eq!(m.save(), Err(SaveError::NeedsSaveAs), "a never-saved copy has no file yet");
    assert_eq!(m.suggested_name(), "turning-center-2-axis-copy");
    edit(&mut m, "Z travel min", "-500");
    let p = m.save_as(Some(&d), "my lathe", false).expect("saves");
    assert_eq!(p, d.join("my lathe.mch"));
    assert_eq!(m.path.as_deref(), Some(p.as_path()));
    assert!(!m.dirty);
    assert_eq!(read_json(&p).pointer("/kinematics/default/parts/0/parts/0/parts/0/travel/min").and_then(Value::as_f64), Some(-500.0));
    // A second machine under the same name asks before replacing.
    let mut other = MachineDraft::blank(Kind::Lathe);
    assert_eq!(other.save_as(Some(&d), "my lathe.mch", false), Err(SaveError::Exists(p.clone())));
    assert_eq!(other.save_as(Some(&d), "my lathe", true), Ok(p.clone()));
    assert_eq!(read_json(&p).pointer("/general/model").and_then(Value::as_str), Some("New lathe"));
    // Saving over its own file needs no confirmation.
    edit(&mut other, "X travel max", "300");
    assert!(matches!(other.save(), Err(SaveError::Refused(_))), "one end of a travel is not a stroke");
    edit(&mut other, "X travel min", "0");
    assert_eq!(other.save(), Ok(p.clone()));
    // Names that are paths, and no folder at all, are refused.
    for bad in ["", "../escape", "sub/dir", ".hidden"] {
        assert!(matches!(other.save_as(Some(&d), bad, true), Err(SaveError::BadName(_))), "{bad:?}");
    }
    assert_eq!(other.save_as(None, "x", true), Err(SaveError::NoFolder));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_write_is_atomic_and_leaves_no_temp_file() {
    let d = dir("atomic");
    let p = d.join("m.mch");
    store::write_atomic(&p, "first").expect("writes");
    store::write_atomic(&p, "second").expect("replaces");
    assert_eq!(std::fs::read_to_string(&p).expect("read"), "second");
    let names: Vec<String> = std::fs::read_dir(&d)
        .expect("list")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["m.mch"], "no temp file left behind");
    // A folder in the target's place: refused by the OS, nothing lost.
    let blocked = d.join("blocked.mch");
    std::fs::create_dir_all(blocked.join("inner")).expect("mkdir");
    assert!(matches!(store::write_atomic(&blocked, "x"), Err(SaveError::Io(_))));
    assert!(blocked.join("inner").is_dir());
    assert_eq!(std::fs::read_dir(&d).expect("list").count(), 2, "the temp file was removed");
    // A missing folder is created.
    let deep = d.join("new").join("folder").join("m.mch");
    store::write_atomic(&deep, "ok").expect("creates the folder");
    assert!(deep.is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_user_folder_resolves_from_the_override_else_the_platform_data_folder() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(*v))
    };
    let abs = if cfg!(windows) { "C:\\m" } else { "/m" };
    let home = if cfg!(windows) { "C:\\home\\u" } else { "/home/u" };
    let leaked: &'static [(&str, &str)] = Box::leak(Box::new([(store::ENV_VAR, abs), ("HOME", home)]));
    assert_eq!(store::user_dir_for("linux", env(leaked)), Some(PathBuf::from(abs)), "the override wins");
    let rel: &'static [(&str, &str)] = Box::leak(Box::new([(store::ENV_VAR, "rel/dir"), ("HOME", home)]));
    let linux = PathBuf::from(home).join(".local").join("share").join("camcraft").join("machines");
    assert_eq!(store::user_dir_for("linux", env(rel)), Some(linux.clone()), "a relative override is ignored");
    let h: &'static [(&str, &str)] = Box::leak(Box::new([("HOME", home)]));
    assert_eq!(store::user_dir_for("linux", env(h)), Some(linux));
    assert_eq!(
        store::user_dir_for("macos", env(h)),
        Some(PathBuf::from(home).join("Library").join("Application Support").join("camcraft").join("machines"))
    );
    let xdg_dir = if cfg!(windows) { "C:\\xdg" } else { "/xdg" };
    let xdg: &'static [(&str, &str)] = Box::leak(Box::new([("HOME", home), ("XDG_DATA_HOME", xdg_dir)]));
    assert_eq!(store::user_dir_for("linux", env(xdg)), Some(PathBuf::from(xdg_dir).join("camcraft").join("machines")));
    let appdata = if cfg!(windows) { "C:\\Users\\u\\AppData\\Roaming" } else { "/appdata" };
    let win: &'static [(&str, &str)] = Box::leak(Box::new([("APPDATA", appdata)]));
    assert_eq!(store::user_dir_for("windows", env(win)), Some(PathBuf::from(appdata).join("camcraft").join("machines")));
    assert_eq!(store::user_dir_for("macos", env(&[])), None, "no home, no folder");
}

#[test]
fn the_catalogue_lists_the_user_folder_after_the_shipped_library() {
    let d = dir("catalogue");
    let mut m = MachineDraft::blank(Kind::Mill);
    edit(&mut m, "Model", "USER MILL");
    m.save_as(Some(&d), "user-mill", false).expect("saves");
    std::fs::write(d.join("broken.mch"), "{ not json").expect("write");
    let world = camcraft_app::world::new_world();
    let list = library::enumerate_with(&world, Some(&d));
    let n = camcraft_app::posting::MACHINES.len();
    assert_eq!(list.len(), n + 2);
    assert_eq!(list[n].name, "broken.mch");
    assert!(!list[n].is_valid(), "a broken user file lists with its refusal");
    assert_eq!(list[n + 1].name, "USER MILL");
    // Its text opens in the editor again, not read-only.
    let again = MachineDraft::open(&list[n + 1].source).expect("opens");
    assert!(!again.read_only);
    assert_eq!(again.model(), "USER MILL");
    // The document mounting a listed file does not list it twice.
    let p = d.join("user-mill.mch");
    let parsed = camcraft_app::machine::parse_mch(&m.text()).expect("parse");
    camcraft_app::machine::set(&world, &p.display().to_string(), Some(&parsed));
    assert_eq!(library::enumerate_with(&world, Some(&d)).len(), n + 2);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_field_pointer_through_a_non_object_is_refused_not_a_panic() {
    let mut v: Value = serde_json::json!({ "general": "not an object", "list": [1] });
    assert!(fields::set(&mut v, "/general/model", FieldKind::Text, "x").is_err());
    assert!(fields::set(&mut v, "/list/5/x", FieldKind::Text, "x").is_err());
    assert!(fields::set(&mut v, "", FieldKind::Text, "x").is_err());
    // Removing under an absent parent is a no-op, not a creation.
    assert!(fields::set(&mut v, "/absent/key", FieldKind::Number, "").is_ok());
    assert!(v.get("absent").is_none());
}

/// The M-code words the editor takes: `M` and a whole number 0..=9999,
/// leading zeros allowed. Anything else - a bare `M`, a letter after
/// it, a lowercase `m`, a number past M9999 - is refused on its field
/// and blocks the save (GUI-test finding on PR #478).
const GOOD_M: [&str; 6] = ["M7", "M08", "M155", "M0", "M9999", "M0008"];
const BAD_M: [&str; 8] = ["M", "MX", "M10000", "m8", "8", "M8.5", "M-1", "M8 M7"];

/// The refusals on one field after typing `code` into it.
fn refusals_on(m: &mut MachineDraft, label: &str, code: &str) -> Vec<String> {
    edit(m, label, code);
    let r = m.refusals();
    field(&m.groups(), label).refusals(&r).iter().map(|r| r.to_string()).collect()
}

#[test]
fn a_coolant_code_must_be_a_well_formed_m_word() {
    let d = dir("mcode_coolant");
    let mut m = MachineDraft::blank(Kind::Mill);
    m.add_coolant("Mist", "M7").expect("adds");
    for label in ["Coolant Flood code", "Coolant Mist code"] {
        for good in GOOD_M {
            assert_eq!(refusals_on(&mut m, label, good), Vec::<String>::new(), "{label} {good}");
            assert!(m.refusals().is_empty(), "{good}: {:?}", m.refusals());
        }
        for bad in BAD_M {
            let on = refusals_on(&mut m, label, bad);
            assert_eq!(on.len(), 1, "{label} {bad:?} is refused on its field: {on:?}");
            assert!(on[0].contains("M word"), "{on:?}");
            assert!(matches!(m.save_as(Some(&d), "bad", false), Err(SaveError::Refused(_))), "{bad:?} saved");
            assert!(!d.join("bad.mch").exists());
        }
        // Empty is refused by name (#869): the editor never saves a
        // coolant the posts would silently ignore.
        let on = refusals_on(&mut m, label, "");
        assert_eq!(on.len(), 1, "{label} empty: {on:?}");
        assert!(on[0].contains("has no M code"), "{on:?}");
        edit(&mut m, label, "M8");
    }
    assert_eq!(refusals_on(&mut m, "Coolant Flood code", "M"), [r#"coolant Flood: code "M" is not an M word like M8 (no number after M)"#]);
    assert!(refusals_on(&mut m, "Coolant Flood code", "M10000")[0].contains("above M9999"));
    assert!(refusals_on(&mut m, "Coolant Flood code", "m8")[0].contains("uppercase M"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_live_tool_code_must_be_a_well_formed_m_word() {
    let d = dir("mcode_live");
    let words = ["Live tool C on", "Live tool C off", "Live tool spindle on", "Live tool spindle off"];
    let mut m = MachineDraft::blank(Kind::MillTurn);
    for (l, c) in words.iter().zip(["M154", "M155", "M133", "M135"]) {
        edit(&mut m, l, c);
    }
    assert!(m.refusals().is_empty(), "{:?}", m.refusals());
    for label in words {
        for good in GOOD_M {
            assert_eq!(refusals_on(&mut m, label, good), Vec::<String>::new(), "{label} {good}");
        }
        for bad in BAD_M {
            let on = refusals_on(&mut m, label, bad);
            assert_eq!(on.len(), 1, "{label} {bad:?} is refused on its field: {on:?}");
            assert!(on[0].contains("M word"), "{on:?}");
            // Only on its own field, not on the other three words.
            let r = m.refusals();
            let g = m.groups();
            for other in words.iter().filter(|w| **w != label) {
                assert!(field(&g, other).refusals(&r).is_empty(), "{bad:?} on {label} leaks onto {other}");
            }
            assert!(matches!(m.save_as(Some(&d), "bad", false), Err(SaveError::Refused(_))), "{bad:?} saved");
            assert!(!d.join("bad.mch").exists());
        }
        edit(&mut m, label, "M100");
    }
    assert_eq!(
        refusals_on(&mut m, "Live tool C on", "M"),
        [r#"live_tool.c_axis_on: code "M" is not an M word like M8 (no number after M)"#]
    );
    let _ = std::fs::remove_dir_all(&d);
}

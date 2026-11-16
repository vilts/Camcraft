//! A saved `.mch` keeps the key order it was read in (issue #480): the
//! machine editor's save writes each object's keys in the source's
//! order, new keys last in their object; a new machine writes the text
//! `serde_json` would.
//!
//! Fixture law (clean-room): the crafted file is our own values in the
//! `.mch` layout of `assets/machines/AGENTS.md`.
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! Gate map (M4.4): covers app::machine, post::machine::order, app::machine, post::machine::draft.

use std::path::{Path, PathBuf};

use camcraft_app::machine::draft::MachineDraft;
use camcraft_app::machine::fields::{self, FieldGroup};
use camcraft_app::machine::library::MachineSource;
use camcraft_app::machine::Kind;
use serde_json::Value;

/// A fresh, empty folder for one test.
fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_machine_key_order_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

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

/// A hand-written mill in no sorted order: `x_vendor_block` first,
/// `kind` before `model`, `type` before `id` (issue #480).
const UNSORTED: &str = r#"{
  "x_vendor_block": { "warmup_minutes": 10, "probe": "OMP40" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "type": "table", "name": "table", "id": "base", "parts": [
      { "type": "linear", "name": "X", "id": "x", "travel": { "max": 150.0, "min": -150.0 } },
      { "type": "linear", "name": "Y", "id": "y", "travel": { "max": 100.0, "min": -100.0 } },
      { "type": "linear", "name": "Z", "id": "z", "travel": { "max": 0.0, "min": -200.0 } },
      { "type": "head", "name": "head", "id": "head",
        "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
    ] } ]
  } },
  "general": { "shop_asset_tag": "B-17", "kind": "mill", "model": "SHOP MILL" }
}"#;

/// Every `"key":` in `text`, in the order written.
fn keys_in(text: &str) -> Vec<String> {
    let parts: Vec<&str> = text.split('"').collect();
    parts
        .windows(2)
        .enumerate()
        .filter(|(i, w)| i % 2 == 1 && w[1].trim_start().starts_with(':'))
        .map(|(_, w)| w[0].to_owned())
        .collect()
}

#[test]
fn a_saved_file_keeps_its_key_order_and_new_keys_go_last() {
    let d = dir("keyorder");
    let p = d.join("unsorted.mch");
    std::fs::write(&p, UNSORTED).expect("write");
    let mut m = MachineDraft::open(&MachineSource::File(p.clone())).expect("opens");
    edit(&mut m, "X travel max", "175");
    edit(&mut m, "Spindle max speed", "9000");
    m.save().expect("saves");
    let saved = std::fs::read_to_string(&p).expect("read back");
    let mut want = keys_in(UNSORTED);
    // The new spindle block lands last in the head, before nothing else.
    let at = want.iter().position(|k| k == "mCode").expect("mCode") + 1;
    want.insert(at, "spindle".to_owned());
    want.insert(at + 1, "max_speed".to_owned());
    assert_eq!(keys_in(&saved), want, "saved:\n{saved}");
    assert_eq!(read_json(&p).pointer("/kinematics/default/parts/0/parts/0/travel/max").and_then(Value::as_f64), Some(175.0));
}

#[test]
fn a_new_machine_writes_the_same_text_serde_json_would() {
    let m = MachineDraft::blank(Kind::MillTurn);
    let want = serde_json::to_string_pretty(&m.value).expect("pretty") + "\n";
    assert_eq!(m.text(), want);
}

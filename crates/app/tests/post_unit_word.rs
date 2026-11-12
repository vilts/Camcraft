//! #901: the Fanuc, Haas, Mazak, Siemens 840D and Doosan DNM mill posts
//! state the unit they write - millimetres - before any motion: G21
//! (G710 on the 840D: metric lengths and feeds), so a control left in
//! inch by power-on or the last program never reads millimetre numbers
//! as inches. A machine file's unit does not change that (where the
//! program's unit is chosen, and inch output, is #902): the program
//! says mm because it is mm.
//! Provenance: our own tests over our own posts; the words are RS-274 /
//! ISO 6983 (G20 / G21) and the 840D family's public programming manual
//! (G71 / G710).
//!
//! Gate map (M4.4): covers post::post (fanuc.cps, haas.cps, mazak.cps,
//! sinumerik.cps, doosan-dnm.cps opening block).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use serde_json::{json, Value};
use std::path::Path;

/// One section: a rapid over the start, a plunge, one feed.
const STREAM: &str = "PARAM SpindleSpeed 2000\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";

fn cps(post: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts").join(format!("{post}.cps"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{post}: {e}"))
}

/// A machine file in the file form, its length unit `length`.
fn file_form(length: &str) -> String {
    json!({
        "general": { "model": "BUILDER VMC", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": {
            "units": { "length": length, "angle": "degrees" },
            "parts": [{ "id": "base", "name": "table", "type": "table", "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 } },
                { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 } },
                { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -300.0, "max": 0.0 }, "safe_retract": 0.0 }
            ] }]
        } }
    })
    .to_string()
}

/// The machine as the app mounts it (parsed, mirrored) and as the CLI
/// hands the raw file over - both doors carry the unit.
fn doors(length: &str) -> Vec<Value> {
    let text = file_form(length);
    let mounted = machine::mirror(&parse_mch(&text).unwrap_or_else(|e| panic!("parses: {e}")));
    let raw: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("json: {e}"));
    vec![mounted, raw]
}

fn is_motion(line: &str) -> bool {
    line.split_whitespace().any(|w| matches!(w, "G0" | "G00" | "G1" | "G01" | "G2" | "G02" | "G3" | "G03" | "G28" | "G53"))
}

/// The unit word is written, before the first motion line, on no
/// machine and on metric and inch machine files alike (the numbers are
/// mm either way), and no inch word is ever written.
fn states_unit(post: &str, word: &str) {
    for m in [vec![Value::Null], doors("mm"), doors("inch")].concat() {
        let out = engine::run(&cps(post), &m, STREAM).expect("posts");
        let p = &out.program;
        let unit = p.lines().position(|l| l.split_whitespace().any(|w| w == word));
        let first_move = p.lines().position(is_motion);
        match (unit, first_move) {
            (Some(u), Some(f)) => assert!(u < f, "{post}: `{word}` after the first motion line:\n{p}"),
            _ => panic!("{post}: no `{word}` or no motion:\n{p}"),
        }
        for other in ["G20", "G70", "G700"] {
            assert!(!p.split_whitespace().any(|w| w == other), "{post}: inch word `{other}`:\n{p}");
        }
    }
}

#[test]
fn fanuc_states_g21_before_any_motion() {
    states_unit("fanuc", "G21");
}

#[test]
fn haas_states_g21_before_any_motion() {
    states_unit("haas", "G21");
}

#[test]
fn mazak_states_g21_before_any_motion() {
    states_unit("mazak", "G21");
}

#[test]
fn sinumerik_states_g710_before_any_motion() {
    states_unit("sinumerik", "G710");
}

#[test]
fn doosan_dnm_states_g21_before_any_motion() {
    states_unit("doosan-dnm", "G21");
}

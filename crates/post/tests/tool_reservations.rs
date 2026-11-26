//! Provenance: original machine reservation grammar tests from issue #2604.
//! Gate map (M4.4): covers camcraft_post::machine::tooling,
//! camcraft_post::machine::parse, camcraft_post::machine::def, camcraft_post::machine::reservations.

use camcraft_post::machine::{Machine, parse_mch, reservations::check_stream};
use serde_json::{Value, json};

fn file(reserved: Value) -> String {
    json!({"tooling": {"reserved": reserved}}).to_string()
}

fn refuses(reserved: Value) {
    let error = parse_mch(&file(reserved)).expect_err("malformed reservation must refuse");
    assert!(error.contains("tooling.reserved"), "{error}");
}

#[test]
fn malformed_reserved_tool_numbers_refuse_by_key() {
    for number in [json!(0), json!(100_000), json!(-1), json!(1.5), json!("1"), json!(null), json!(true)] {
        refuses(json!([{"number": number, "use": "probe"}]));
    }
}

#[test]
fn malformed_reserved_tool_entries_refuse_by_key() {
    for entry in [
        json!({"use": "probe"}),
        json!({"number": 1}),
        json!({"number": 1, "use": "cutter"}),
        json!({"number": 1, "use": 1}),
        json!({"number": 1, "use": "probe", "typo": true}),
        json!({"name": "", "use": "probe"}),
        json!({"name": "   ", "use": "probe"}),
        json!({"name": 1, "use": "probe"}),
        json!(null), json!(1), json!("probe"),
    ] {
        refuses(json!([entry]));
    }
    for wrong_list in [json!({}), json!("probe"), json!(1), json!(true)] {
        refuses(wrong_list);
    }
}

#[test]
fn duplicate_reserved_tool_numbers_and_names_refuse_by_key() {
    refuses(json!([{"number": 1, "use": "probe"}, {"number": 1, "use": "probe"}]));
    refuses(json!([{"name": "PROBE", "use": "probe"}, {"name": "probe", "use": "probe"}]));
}

#[test]
fn reserved_tools_survive_stored_machine_round_trip() {
    let entries = json!([{"number": 1, "use": "probe"}, {"name": "PROBE", "use": "probe"}]);
    let machine = parse_mch(&file(entries.clone())).expect("valid reservations");
    let stored = serde_json::to_value(&machine).expect("stored machine");
    assert_eq!(stored.get("reserved_tools"), Some(&entries));
    let restored: Machine = serde_json::from_value(stored).expect("restore machine");
    assert_eq!(restored, machine);
    assert!(restored.is_sane());
}

#[test]
fn an_old_machine_has_no_reserved_tools() {
    let old = parse_mch("{}").expect("old machine");
    let stored = serde_json::to_value(&old).expect("stored machine");
    assert!(stored.get("reserved_tools").is_none_or(|v| v == &json!([])));
    assert!(old.is_sane());
}

#[test]
fn invalid_stored_reservations_make_machine_insane() {
    for entries in [
        json!([{"number": 0, "use": "probe"}]),
        json!([{"number": 100_000, "use": "probe"}]),
        json!([{"number": 1, "use": "probe"}, {"number": 1, "use": "probe"}]),
        json!([{"name": "PROBE", "use": "probe"}, {"name": "probe", "use": "probe"}]),
        json!([{"name": "   ", "use": "probe"}]),
    ] {
        let machine: Machine = serde_json::from_value(json!({"reserved_tools": entries}))
            .expect("stored shape can deserialize before semantic validation");
        assert!(!machine.is_sane(), "invalid stored reservations: {machine:?}");
    }
}

#[test]
fn shipped_machines_reserve_no_pocket_without_a_cited_manual() {
    // No public maker manual is cited for a reserved pocket yet (#2604
    // review). A shipped reservation needs that source, and this pin then
    // names the file and its document.
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines");
    let mut read = 0;
    for entry in std::fs::read_dir(&directory).expect("shipped machines") {
        let path = entry.expect("machine entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("mch") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("shipped machine");
        let value: Value = serde_json::from_str(&text).expect("machine JSON");
        assert!(value.pointer("/tooling/reserved").is_none(),
            "{} reserves a pocket with no cited public manual", path.display());
        read += 1;
    }
    assert!(read > 18, "read only {read} machine files");
}

/// Review of #3046: a waterjet stream on a machine that states
/// `tooling.reserved`. A `JET` record is cutting, as a `TORCH` record is:
/// it needs a stated TOOL, and a nozzle in the probe pocket refuses. The
/// `WATERJET` header is no motion, as the `PLASMA` header is: it can
/// come before the TOOL record.
#[test]
fn a_waterjet_stream_obeys_the_reserved_pockets() {
    let file = json!({
        "general": {"model": "Reserved waterjet", "kind": "waterjet"},
        "kinematics": {"default": {"units": {"length": "mm", "angle": "degrees"}, "parts": [
            {"id": "x", "name": "X", "type": "linear", "travel": {"min": 0.0, "max": 460.0}},
            {"id": "y", "name": "Y", "type": "linear", "travel": {"min": -305.0, "max": 0.0}},
            {"id": "nozzle", "name": "nozzle", "type": "head"}
        ]}},
        "tooling": {"reserved": [{"number": 1, "use": "probe"}]}
    });
    let machine = parse_mch(&file.to_string()).expect("waterjet machine with a reserved pocket");
    let stream = |tool: &str| {
        format!("; HSMW-CNC v1\nWATERJET ABRASIVE DWELL800 LAG300\n{tool}SECTION\nJET ON C0 OPEN\nJET OFF C0\nENDSECTION\n")
    };
    // The header comes before the TOOL record (the order of the app's stream).
    assert_eq!(check_stream(&machine, &stream("TOOL 2 NOZZLE 1.0\n")), Ok(()));
    let reserved = check_stream(&machine, &stream("TOOL 1 NOZZLE 1.0\n")).expect_err("a nozzle in the probe pocket");
    assert!(reserved.contains("NOZZLE 1.0") && reserved.contains("(T1) is reserved for probe"), "{reserved}");
    // A jet with no TOOL record: the JET record itself must ask for the selector.
    let bare = "; HSMW-CNC v1\nWATERJET\nJET ON C0 OPEN\n";
    let missing = check_stream(&machine, bare).expect_err("a jet with no stated tool");
    assert!(missing.contains("TOOL selector is not stated"), "{missing}");
}

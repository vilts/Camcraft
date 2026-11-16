//! Setup-change stops retract and name the next setup (#1647).
//! Provenance: our synthetic streams and the shipped dialects' retract conventions.
//! Gate map: covers assets/posts/haas.cps, assets/posts/mazak.cps,
//! assets/posts/doosan-dnm.cps, assets/posts/sinumerik.cps,
//! assets/posts/heidenhain.cps, assets/posts/grblhal.cps, assets/posts/tinyg.cps,
//! camcraft_post::post::engine.

use camcraft_post::cl::{MoveKind, PathBuilder};
use camcraft_post::geometry::Point3;
use camcraft_post::post::{AfterOperation, Dump, cnc, dump, engine};
use serde_json::{Value, json};
use std::path::Path;

const POSTS: &[&str] = &["haas", "mazak", "doosan-dnm", "sinumerik", "heidenhain", "grblhal", "tinyg"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel))
        .expect("fixture")
}

fn machine() -> Value {
    let mut m: Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    m["safeRetractZ"] = json!(150.0);
    m
}

fn records(after: AfterOperation, next: Option<&str>) -> Vec<cnc::Record> {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 25.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 25.0), None);
    b.push(MoveKind::Feed, Point3::new(10.0, 10.0, 10.0), Some(300.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 25.0), None);
    let path = b.finish();
    let mut all = Vec::new();
    for (i, op) in ["Cut1", "Cut2", "Cut3"].iter().enumerate() {
        let d = Dump {
            op_name: op, tool_number: 1, tool_name: "EM6", tool_kind: "flat",
            tool_diameter: 6.0, rpm: 3000.0, clearance_z: 25.0, retract_z: 25.0,
            stock_top_z: 20.0, origin: [0.0, 0.0, 5.0],
            after: if i < 2 { after } else { AfterOperation::default() },
            ..Default::default()
        };
        let mut r = dump(&path, &d);
        if i == 0 && let Some(next) = next {
            r.insert(0, cnc::Record::Param { name: "SetupChange".into(), value: json!(next) });
        }
        all.extend(r);
    }
    all
}

fn post(stem: &str, m: &Value, r: &[cnc::Record]) -> engine::PostOutput {
    engine::run(&read(&format!("assets/posts/{stem}.cps")), m, &cnc::serialize(r)).expect("post runs")
}

/// Isolate the section end, before the next operation's preamble can retract.
fn gap(nc: &str, next: &str) -> String {
    let before = &nc[..nc.find(&format!("OPERATION: {next}")).expect("next operation")];
    before[before.rfind("M5").expect("section spindle stop")..].to_string()
}

fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().map(|w| w.trim_end_matches('*'))
        .filter(|w| !w.is_empty() && !w.starts_with('N')).collect()
}

fn has_block(nc: &str, expected: &[&str]) -> bool {
    nc.lines().any(|line| {
        let w = words(line);
        expected.iter().all(|word| w.contains(word))
    })
}

fn pin(stem: &str, name: &str, displayed: &str, stops: &[(AfterOperation, &str)]) {
    for &(after, stop) in stops {
        let r = records(after, Some(name));
        let out = post(stem, &machine(), &r);
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let first = gap(&out.program, "Cut2");
        let note = first.find(&format!("SETUP CHANGE - NEXT: {displayed}")).unwrap_or_else(|| panic!("{stem}: missing next setup:\n{first}"));
        let stop_at = first.lines().position(|l| words(l).contains(&stop)).expect("requested stop");
        let note_at = first[..note].lines().count() - 1;
        assert!(note_at < stop_at, "{stem}: comment precedes stop:\n{first}");
        let lift = &first[..note];
        match stem {
            "haas" | "mazak" | "doosan-dnm" => {
                assert!(has_block(lift, &["G91", "G28", "Z0"]), "{stem}: Z reference return before note:\n{first}");
                assert!(has_block(lift, &["G90"]), "{stem}: restore absolute mode:\n{first}");
            }
            "sinumerik" | "grblhal" | "tinyg" => {
                assert!(has_block(lift, &["G0", "G53", "Z150"]), "{stem}: stated machine retract before note:\n{first}");
            }
            "heidenhain" => {
                assert!(has_block(lift, &["G00", "Z+20"]), "{stem}: clearance in the work offset before note:\n{first}");
                assert!(!has_block(lift, &["G53"]) && !has_block(lift, &["M91"]), "no guessed machine datum: {first}");
            }
            _ => unreachable!(),
        }
        for l in first.lines() {
            let w = words(l);
            assert!(!w.iter().any(|v| v.starts_with('X') || v.starts_with('Y')), "Z only: {l}");
        }
        let note_line = first.lines().find(|l| l.contains("SETUP CHANGE")).expect("note");
        assert!(!note_line.contains("(B)") && !note_line.contains(";*"), "sanitized setup: {note_line}");
        if stem == "heidenhain" {
            assert_eq!(note_line.matches('*').count(), 1, "one block terminator: {note_line}");
        }
        let plain = post(stem, &machine(), &records(after, None));
        let next_gap = gap(&out.program, "Cut3");
        let plain_gap = gap(&plain.program, "Cut3");
        assert_eq!(next_gap.lines().map(words).collect::<Vec<_>>(), plain_gap.lines().map(words).collect::<Vec<_>>(), "{stem}: marker resets before the ordinary stop");
        assert_eq!(out.program.matches("SETUP CHANGE").count(), 1, "{stem}: {out:?}");
    }
}

macro_rules! pin_post {
    ($name:ident, $stem:literal) => {
        #[test] fn $name() {
            pin($stem, "Flip (B);*", "Flip B", &[
                (AfterOperation::ProgramStop, "M0"), (AfterOperation::OptionalStop, "M1"),
            ]);
        }
    };
}
pin_post!(haas_setup_change_retracts_and_names_next, "haas");
pin_post!(mazak_setup_change_retracts_and_names_next, "mazak");
pin_post!(doosan_setup_change_retracts_and_names_next, "doosan-dnm");
pin_post!(sinumerik_setup_change_retracts_and_names_next, "sinumerik");
pin_post!(heidenhain_setup_change_retracts_and_names_next, "heidenhain");
pin_post!(grblhal_setup_change_retracts_and_names_next, "grblhal");
pin_post!(tinyg_setup_change_retracts_and_names_next, "tinyg");

// A permitted name can disappear entirely in comment cleaning. Each stop mode
// has its own pin so both bypasses are visible when the handler is regressed.
macro_rules! pin_empty_setup_name {
    ($program:ident, $optional:ident, $stem:literal) => {
        #[test] fn $program() {
            pin($stem, "()", "UNNAMED SETUP", &[(AfterOperation::ProgramStop, "M0")]);
        }
        #[test] fn $optional() {
            pin($stem, "()", "UNNAMED SETUP", &[(AfterOperation::OptionalStop, "M1")]);
        }
    };
}
pin_empty_setup_name!(haas_empty_setup_name_m0, haas_empty_setup_name_m1, "haas");
pin_empty_setup_name!(mazak_empty_setup_name_m0, mazak_empty_setup_name_m1, "mazak");
pin_empty_setup_name!(doosan_empty_setup_name_m0, doosan_empty_setup_name_m1, "doosan-dnm");
pin_empty_setup_name!(sinumerik_empty_setup_name_m0, sinumerik_empty_setup_name_m1, "sinumerik");
pin_empty_setup_name!(heidenhain_empty_setup_name_m0, heidenhain_empty_setup_name_m1, "heidenhain");
pin_empty_setup_name!(grblhal_empty_setup_name_m0, grblhal_empty_setup_name_m1, "grblhal");
pin_empty_setup_name!(tinyg_empty_setup_name_m0, tinyg_empty_setup_name_m1, "tinyg");

#[test]
fn ordinary_stops_do_not_add_setup_change_motion() {
    for stem in POSTS {
        let out = post(stem, &machine(), &records(AfterOperation::ProgramStop, None));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let first = gap(&out.program, "Cut2");
        assert!(!first.contains("SETUP CHANGE"), "{stem}: {first}");
        assert!(!first.lines().flat_map(words).any(|w| w.starts_with('Z')), "ordinary stop must not retract: {stem}: {first}");
    }
}

#[test]
fn heidenhain_setup_change_refuses_missing_or_unsafe_clearance_by_name() {
    for key in ["ClearancePlane", "StockZTop", "unsafe"] {
        let mut r = records(AfterOperation::ProgramStop, Some("Flip B"));
        for record in &mut r {
            if let cnc::Record::Param { name, value } = record {
                if name == key { *name = "Unused".into(); }
                // A missing height must not become numeric zero above negative stock.
                if key == "ClearancePlane" && name == "StockZTop" { *value = json!(-10.0); }
                if key == "unsafe" && name == "ClearancePlane" { *value = json!(20.0); }
            }
        }
        let out = post("heidenhain", &machine(), &r);
        assert!(out.warnings.iter().any(|w| w.contains("Cut1") && w.to_lowercase().contains("setup")), "{key}: named setup-change refusal required: {:?}", out.warnings);
    }
}

#[test]
fn unhomed_grblhal_and_tinyg_setup_changes_use_work_clearance() {
    let mut m = machine();
    m.as_object_mut().expect("machine object").remove("safeRetractZ");
    for stem in ["grblhal", "tinyg"] {
        let out = post(stem, &m, &records(AfterOperation::ProgramStop, Some("Flip B")));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let first = gap(&out.program, "Cut2");
        assert!(has_block(&first, &["G0", "Z20"]), "{stem}: work clearance before stop:\n{first}");
        assert!(!first.contains("G53") && !first.contains("G28") && !first.contains("G30"), "no guessed home: {first}");
    }
}

#[test]
fn heidenhain_setup_change_never_lowers_a_tip_already_above_clearance() {
    let mut r = records(AfterOperation::ProgramStop, Some("Flip B"));
    let end = r.iter().position(|v| matches!(v, cnc::Record::Section { begin: false })).expect("end");
    let last = r[..end].iter_mut().rev().find_map(|v| match v {
        cnc::Record::Move(m) => Some(m), _ => None,
    }).expect("last move");
    last.z = 40.0;
    let out = post("heidenhain", &machine(), &r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let first = gap(&out.program, "Cut2");
    assert!(has_block(&first, &["G00", "Z+35"]), "keep the higher tip position: {first}");
    assert!(!has_block(&first, &["G00", "Z+20"]), "never descend at setup change: {first}");
}

#[test]
fn heidenhain_setup_change_does_not_reuse_previous_sections_stock_top() {
    let mut r = records(AfterOperation::ProgramStop, None);
    let mut section = 0;
    for record in &mut r {
        if matches!(record, cnc::Record::Section { begin: false }) { section += 1; }
        if let cnc::Record::Param { name, .. } = record
            && section == 1 && name == "StockZTop" {
            *name = "Unused".into();
        }
    }
    let second = r.iter().enumerate().filter(|(_, v)| matches!(v, cnc::Record::Section { begin: true })).nth(1).expect("second section").0;
    r.insert(second, cnc::Record::Param { name: "SetupChange".into(), value: json!("Flip C") });
    let out = post("heidenhain", &machine(), &r);
    assert!(out.warnings.iter().any(|w| w.contains("Cut2") && w.to_lowercase().contains("setup")), "no previous section's stock top: {:?}", out.warnings);
}

#[test]
fn doosan_setup_change_cancels_tcpm_before_reference_return() {
    let mut r = records(AfterOperation::ProgramStop, Some("Flip B"));
    for record in &mut r {
        if let cnc::Record::Move(m) = record { m.abc = Some([0.0, 0.0, 0.0]); }
    }
    let m = camcraft_post::post::machine::mirror(
        &camcraft_post::machine::parse_mch(&read("assets/machines/tilthead.mch")).expect("tilthead"),
    );
    let out = engine::run_with(&read("assets/posts/doosan-dnm.cps"), &m,
        &cnc::serialize(&r), &json!({ "rotaryBrake": true })).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G43.4"), "fixture arms TCPM: {}", out.program);
    let first = gap(&out.program, "Cut2");
    let cancel = first.find("G49").expect("cancel TCPM before setup-change retract");
    let home = first.find("G28").expect("reference return");
    assert!(cancel < home, "cancel before return: {first}");
}

#[test]
fn sinumerik_setup_change_refuses_an_unstated_machine_retract() {
    let mut m = machine();
    m.as_object_mut().expect("machine object").remove("safeRetractZ");
    let out = post("sinumerik", &m, &records(AfterOperation::ProgramStop, Some("Flip B")));
    assert!(out.warnings.iter().any(|w| w.contains("safe Z retract")), "named refusal: {:?}", out.warnings);
    let first = gap(&out.program, "Cut2");
    assert!(!first.contains("G53") && !first.contains("SETUP CHANGE") && !has_block(&first, &["M0"]), "no guessed Z or invitation to change setup: {first}");
}

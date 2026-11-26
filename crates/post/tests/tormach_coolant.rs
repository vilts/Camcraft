//! Provenance: #3446, original tests of our stream and Tormach post coolant contract.
//! Gate map: covers assets/posts/tormach.cps, camcraft_post::post::engine.

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine, PostOutput};
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path))
        .expect("fixture")
}

/// One face section on `mch`, with `coolant` as its `PARAM Coolant`
/// line (empty: the stream's silent flood).
fn post(mch: &str, coolant: &str) -> PostOutput {
    post_stream(mch, &section("F", coolant))
}

fn section(op: &str, coolant: &str) -> String {
    format!(
        "PARAM OperationName {op}\nPARAM SpindleSpeed 10000\n{coolant}TOOL 1 ROUTER\nPARAM ClearancePlane 25\nPARAM RetractPlane 10\nSECTION\nRapid 0 0 10000\nFeed 0 0 -1000 F500000\nFeed 20000 0 -1000 F500000\nRapid 20000 0 10000\nENDSECTION\n"
    )
}

fn post_stream(mch: &str, stream: &str) -> PostOutput {
    let mch = parse_mch(&read(mch)).expect("machine");
    engine::run(&read("assets/posts/tormach.cps"), &machine::mirror(&mch), stream).expect("post")
}

/// #3446: an Off ask holds for its own section only; the next section,
/// which states no ask, floods.
#[test]
fn tormach_coolant_ask_does_not_leak_into_the_next_section() {
    let stream = section("A", "PARAM Coolant off\n") + &section("B", "");
    let out = post_stream("assets/machines/tormach-770mx.mch", &stream);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(coolant_words(&out.program), ["M8", "M9"], "{}", out.program);
}

fn coolant_words(program: &str) -> Vec<&str> {
    program
        .split_whitespace()
        .filter(|w| matches!(*w, "M7" | "M8" | "M9"))
        .collect()
}

/// #3446: the 24R states no coolant. Off posts with no warning and no
/// coolant word; Flood still posts silently on that machine.
#[test]
fn tormach_24r_posts_coolant_off_with_no_coolant_word() {
    let off = post("assets/machines/tormach-24r.mch", "PARAM Coolant off\n");
    assert!(off.warnings.is_empty(), "{:?}", off.warnings);
    assert!(coolant_words(&off.program).is_empty(), "{}", off.program);
    let flood = post("assets/machines/tormach-24r.mch", "");
    assert!(flood.warnings.is_empty(), "{:?}", flood.warnings);
    assert!(coolant_words(&flood.program).is_empty(), "{}", flood.program);
}

/// #3446: on a machine that states Flood M8 and Mist M7, Off writes no
/// coolant word, Mist writes M7, and Flood writes M8.
#[test]
fn tormach_770mx_posts_each_coolant_ask_from_the_machine_list() {
    let m = "assets/machines/tormach-770mx.mch";
    let off = post(m, "PARAM Coolant off\n");
    assert!(off.warnings.is_empty(), "{:?}", off.warnings);
    assert!(coolant_words(&off.program).is_empty(), "{}", off.program);
    let mist = post(m, "PARAM Coolant mist\n");
    assert!(mist.warnings.is_empty(), "{:?}", mist.warnings);
    assert_eq!(coolant_words(&mist.program), ["M7", "M9"], "{}", mist.program);
    let flood = post(m, "");
    assert!(flood.warnings.is_empty(), "{:?}", flood.warnings);
    assert_eq!(coolant_words(&flood.program), ["M8", "M9"], "{}", flood.program);
}

/// #3446: an ask that the machine cannot serve is refused by name,
/// never posted as flood.
#[test]
fn tormach_refuses_a_coolant_the_machine_does_not_state() {
    let out = post("assets/machines/tormach-770mx.mch", "PARAM Coolant through-tool\n");
    assert!(
        out.warnings.iter().any(|w| w.contains("through-tool") && w.contains("operation F")),
        "{:?}",
        out.warnings
    );
    assert!(!coolant_words(&out.program).contains(&"M8"), "{}", out.program);
}

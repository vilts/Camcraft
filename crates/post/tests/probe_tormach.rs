//! #362 acceptance: the probing hooks of the Tormach PathPilot post. Every
//! probe mode the generic dialect covers (face, edge, outside corner, bore,
//! boss; tests/probe_op.rs) posts through the whole post seam (dump ->
//! stream text -> engine -> tormach.cps) as LinuxCNC words: G38.2 touches
//! (the control alarms on no contact), trips read from #5061-#5063, an
//! incremental back-off, an O-word `if` tolerance test that ends the
//! program, and the work offset set with G10 L20 from the current position.
//!
//! Provenance: the LinuxCNC G-code reference (G38.n, G10 L20, numbered
//! parameters #5061-#5070 / #5420-#5428, O-code conditionals, (MSG,...))
//! and Tormach's PathPilot post-processor guidelines; our own fixtures.
//!
//! The streams are recorded (#926): `recorded/inputs/probe_t99_*`,
//! written and guarded by app's `recorded_post_streams` from
//! `motion::probing::probe_cycle` (the five modes of tests/probe_op.rs).
//!
//! Gate map (M4.4): covers assets/posts (tormach probing), post::probe_rec,
//! post::machine (the 770MX's inch units and coolant).

mod recorded;

use camcraft_post::machine::parse_mch;
use camcraft_post::post::{engine, machine};

const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");
const MX770: &str = include_str!("../../../assets/machines/tormach-770mx.mch");

/// The 770MX as the app hands it to a post (parser, then mirror): an
/// inch machine with a flood coolant entry.
fn mx770() -> serde_json::Value {
    machine::mirror(&parse_mch(MX770).expect("770MX parses"))
}

/// A recorded probe stream of one of the five modes of tests/probe_op.rs
/// `each_mode_lays_down_its_touches`, on tool 99 (PathPilot's
/// conventional probe tool number), setting G55 or measuring only.
fn stream(mode: &str, set_wcs: Option<u8>) -> String {
    let set = match set_wcs {
        Some(1) => "g55",
        None => "none",
        Some(n) => panic!("no recorded {mode} setting offset {n}"),
    };
    recorded::input(&format!("probe_t99_{mode}_{set}.cnc"))
}

fn post_ok(machine: &serde_json::Value, stream: &str) -> String {
    let out = engine::run_with(TORMACH, machine, stream, &doc_units(machine)).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let prog = out.program;
    assert!(
        !prog.lines().any(|l| l.split_whitespace().any(|w| w == "M3" || w == "M8" || w.starts_with('S'))),
        "a touch probe starts no spindle and no coolant:\n{prog}"
    );
    assert!(!prog.contains("G31"), "PathPilot probes with G38.x, not G31:\n{prog}");
    prog
}

/// Each O-word test opens and closes under one number, used by no other.
fn o_blocks_pair_uniquely(prog: &str) {
    let ifs: Vec<&str> = prog.lines().filter(|l| l.contains(" if [")).map(|l| l.split(' ').next().unwrap_or("")).collect();
    let ends: Vec<&str> = prog.lines().filter(|l| l.ends_with(" endif")).map(|l| l.split(' ').next().unwrap_or("")).collect();
    assert_eq!(ifs, ends, "every if has its endif:\n{prog}");
    let mut uniq = ifs.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), ifs.len(), "unique O numbers: {ifs:?}");
}

fn count(prog: &str, prefix: &str) -> usize {
    prog.lines().filter(|l| l.starts_with(prefix)).count()
}

#[test]
fn face_posts_g38_2_and_sets_z_on_the_770mx() {
    let prog = post_ok(&mx770(), &stream("face", Some(1)));
    assert!(prog.starts_with("(Tormach 770MX)\nG20 "), "an inch machine posts G20:\n{prog}");
    assert_eq!(count(&prog, "G38.2 "), 2, "a seek and a measure");
    assert!(prog.contains("#100=#5061\n#101=#5062\n#102=#5063\n"), "the measured trip is kept");
    assert!(prog.contains("G91 G1 X0 Y0 Z0.0394 F11.81\nG90\n"), "the back-off is incremental, in inches:\n{prog}");
    assert!(prog.contains("#190=[#102-0.7874]\no901 if [ABS[#190] GT 0.0197]\n(MSG, PROBE Z OUT OF TOLERANCE)\nM2\no901 endif\n"), "{prog}");
    assert!(prog.contains("G10 L20 P2 Z[#5422-#190]"), "the result sets G55 Z: {prog}");
    o_blocks_pair_uniquely(&prog);
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_face.nc", &prog);
}

#[test]
fn edge_posts_g38_2_and_sets_x_on_the_770mx() {
    let prog = post_ok(&mx770(), &stream("edge", Some(1)));
    assert_eq!(count(&prog, "G38.2 "), 2, "a seek and a measure");
    assert!(prog.contains("G10 L20 P2 X[#5420-#190]\n"), "the result sets G55 X: {prog}");
    o_blocks_pair_uniquely(&prog);
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_edge.nc", &prog);
}

#[test]
fn corner_posts_g38_2_and_sets_x_and_y_on_the_770mx() {
    let prog = post_ok(&mx770(), &stream("corner", Some(1)));
    assert_eq!(count(&prog, "G38.2 "), 4, "two touches, seek + measure");
    assert!(prog.contains("G10 L20 P2 X[#5420-#190] Y[#5421-#191]\n"), "{prog}");
    assert_eq!(prog.lines().filter(|l| l.ends_with(" OUT OF TOLERANCE)")).count(), 2, "one tolerance test per axis");
    o_blocks_pair_uniquely(&prog);
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_corner.nc", &prog);
}

#[test]
fn bore_posts_g38_2_and_sets_the_centre_on_the_770mx() {
    let prog = post_ok(&mx770(), &stream("bore", Some(1)));
    assert_eq!(count(&prog, "G38.2 "), 8, "four touches, seek + measure");
    assert!(prog.contains("#190=[[[#100+#103]/2]-1.1811]\n"), "the centre is the mean of opposite trips: {prog}");
    assert!(prog.contains("G10 L20 P2 X[#5420-#190] Y[#5421-#191]\n"), "{prog}");
    o_blocks_pair_uniquely(&prog);
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_bore_centre.nc", &prog);
}

#[test]
fn boss_posts_g38_2_and_sets_the_centre_on_the_770mx() {
    let prog = post_ok(&mx770(), &stream("boss", Some(1)));
    assert_eq!(count(&prog, "G38.2 "), 8, "four touches, seek + measure");
    assert!(prog.contains("G10 L20 P2 X[#5420-#190] Y[#5421-#191]\n"), "{prog}");
    o_blocks_pair_uniquely(&prog);
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_boss_centre.nc", &prog);
}

/// A metric machine posts the same program in mm (the bore centre of
/// nc/fanuc/probe_bore_centre.nc, word for word in the LinuxCNC dialect).
#[test]
fn bore_posts_in_mm_on_a_metric_machine() {
    let prog = post_ok(&serde_json::Value::Null, &stream("bore", Some(1)));
    assert!(prog.contains("\nG21 "), "{prog}");
    assert!(prog.contains("G38.2 X43 Y30 Z15 F300\n"), "{prog}");
    assert!(prog.contains("G91 G1 X-1 Y0 Z0 F300\nG90\n"), "{prog}");
    assert!(prog.contains("o901 if [ABS[#190] GT 0.5]\n"), "{prog}");
    assert!(prog.contains("G10 L20 P2 X[#5420-#190] Y[#5421-#191]\n"), "{prog}");
    assignments_are_bracketed(&prog);
    recorded::app_golden("nc/tormach/probe_bore_centre_mm.nc", &prog);
}

#[test]
fn without_an_offset_to_set_it_stops_for_the_operator() {
    let prog = post_ok(&mx770(), &stream("bore", None));
    assert!(!prog.contains("G10"), "nothing is set");
    assert!(prog.contains("(MSG, CHECK PROBE RESULT #190..)\nM0\n"), "{prog}");
}

/// LinuxCNC evaluates arithmetic only inside brackets: a parameter
/// assignment is `#n=#m` or `#n=[expr]`, never a bare `#n=#m-0.7874`
/// (PathPilot Hub 2.14.4 refused that with "Bad character '-' used"),
/// and no `--` / `+-` is ever written.
fn assignments_are_bracketed(prog: &str) {
    for l in prog.lines().filter(|l| l.starts_with('#')) {
        let Some((_, rhs)) = l.split_once('=') else { panic!("an assignment without '=': {l}") };
        let plain = rhs.starts_with('#') && rhs[1..].chars().all(|c| c.is_ascii_digit());
        assert!(plain || (rhs.starts_with('[') && rhs.ends_with(']')), "unbracketed expression: {l}");
        assert!(!rhs.contains("--") && !rhs.contains("+-"), "a doubled sign: {l}");
    }
}

/// #902: the program's unit is the document's, never the machine file's.
/// These runs on an inch machine file stand for the inch document they
/// posted before #902 (the file's own unit, through either door).
fn doc_units(m: &serde_json::Value) -> serde_json::Value {
    let word = m
        .get("units")
        .and_then(serde_json::Value::as_str)
        .or_else(|| m.pointer("/kinematics/default/units/length").and_then(serde_json::Value::as_str));
    let inch = matches!(word, Some("inch" | "inches"));
    serde_json::json!({ "Units": if inch { "inch" } else { "mm" } })
}

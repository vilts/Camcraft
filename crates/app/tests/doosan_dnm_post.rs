//! Doosan DNM post processor tests (Issue #614).
//!
//! Verifies the Doosan DNM Fanuc 0i/31i-M dialect:
//! - Through-spindle coolant (TSC) support (M88 or M8) and refusal when undeclared
//! - 4th-axis rotary table brake clamp/unclamp (M10/M11 on separate blocks) and refusal when undeclared
//! - Fanuc AI Contour Control (AICC) high-speed smoothing (G5.1 Q1 R<n> / G5.1 Q0) and bound enforcement
//! - Chip conveyor start/stop (M31/M33)
//! - Probing: standard Fanuc G31 skip and Renishaw Inspection Plus macros (G65 P9832 / P9833)
//!
//! Gate map: covers post::post (doosan-dnm.cps), app::posting.

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn dnm_post() -> String {
    read("assets/posts/doosan-dnm.cps")
}

fn dnm_machine() -> serde_json::Value {
    let text = read("assets/machines/doosan-dnm.mch");
    machine::mirror(&parse_mch(&text).expect("dnm machine parses"))
}

fn default_vmc() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

#[test]
fn doosan_dnm_posts_basic_milling_clean() {
    let cps = dnm_post();
    let mch = dnm_machine();
    let stream = read("crates/app/tests/golden/face_square_40x40.cl");
    let out = engine::run(&cps, &mch, &stream).expect("runs clean");

    assert!(out.warnings.is_empty(), "unexpected warnings: {:?}", out.warnings);
    let p = out.program;

    // Header & preamble
    assert!(p.starts_with("%\nO0001 (Doosan DNM 5700)\n"));
    assert!(p.contains("G90 G17 G54 G40 G49 G80"));

    // Linear motion & feed
    assert!(p.contains("G01 X45 Y5 Z-2 F500"));

    // Coolant and spindle stop
    assert!(p.contains("M9"));
    assert!(p.contains("M5"));

    // Retract and safe end
    assert!(p.contains("G91 G28 Z0"));
    assert!(p.contains("G28 X0 Y0"));
    assert!(p.contains("M30\n%"));
}

#[test]
fn doosan_dnm_through_spindle_coolant_modes() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // 1. Through-tool coolant with default Doosan high-pressure TSC (M88 / M89)
    let stream_tsc = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM Coolant through-tool
PARAM WorkOffset G54
TOOL 1 "10mm Flat Endmill"
PARAM SpindleSpeed 6000
SECTION FaceTSC
Rapid 0 0 10000
Feed 50000 0 -1000 F500000
SECTION END
"#;
    let out_tsc = engine::run(&cps, &mch, stream_tsc).expect("posts TSC");
    assert!(out_tsc.warnings.is_empty(), "{:?}", out_tsc.warnings);
    assert!(out_tsc.program.contains("M88"), "program should start TSC with M88:\n{}", out_tsc.program);
    assert!(out_tsc.program.contains("M89"), "program should shut off TSC with M89:\n{}", out_tsc.program);
    assert!(out_tsc.program.contains("M9"), "program should shut off coolant with M9:\n{}", out_tsc.program);

    // 2. Through-tool coolant configured with useM8ForTSC
    let stream_m8 = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM Coolant through-tool
PARAM WorkOffset G54
TOOL 1 "10mm Flat Endmill"
PARAM SpindleSpeed 6000
SECTION FaceM8
Rapid 0 0 10000
Feed 50000 0 -1000 F500000
SECTION END
"#;
    let options_m8 = serde_json::json!({ "useM8ForTSC": true });
    let out_m8 = engine::run_with(&cps, &default_vmc(), stream_m8, &options_m8).expect("posts TSC M8");
    assert!(out_m8.warnings.is_empty(), "{:?}", out_m8.warnings);
    assert!(out_m8.program.contains("M8"), "program should command M8:\n{}", out_m8.program);
    assert!(!out_m8.program.contains("M88"), "program should not command M88 when useM8ForTSC is true");
}

#[test]
fn doosan_dnm_rotary_brake_clamp_unclamp() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // Rotary table 4th-axis indexing move
    let stream_rotary = r#"; HSMW-CL v1 scale=0.001mm moves=4
PARAM WorkOffset G54
TOOL 1 "10mm Endmill"
PARAM SpindleSpeed 5000
SECTION RotaryIndex
Rapid 0 0 10000 A90000
Feed 50000 0 -1000 F400000
Rapid 50000 0 10000 A180000
Feed 0 0 -1000 F400000
SECTION END
"#;
    let options = serde_json::json!({ "rotaryBrake": true });
    let out = engine::run_with(&cps, &mch, stream_rotary, &options).expect("posts rotary moves");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();

    // Find the A90 move
    let idx_a90 = lines.iter().position(|l| l.contains("A90")).expect("A90 commanded");
    // Verify unclamp M11 is on its own block immediately preceding A90 motion
    assert_eq!(lines[idx_a90 - 1].trim(), "M11", "M11 must be on its own line immediately before A90 move");
    // Motion block must NOT combine with M10/M11
    assert!(!lines[idx_a90].contains("M10") && !lines[idx_a90].contains("M11"), "Motion block must not contain clamp words");
    // Verify clamp M10 is on its own block immediately following A90 motion
    assert_eq!(lines[idx_a90 + 1].trim(), "M10", "M10 must be on its own line immediately after A90 move");

    // Same check for the second index A180
    let idx_a180 = lines.iter().position(|l| l.contains("A180")).expect("A180 commanded");
    assert_eq!(lines[idx_a180 - 1].trim(), "M11", "M11 must be on its own line immediately before A180 move");
    assert!(!lines[idx_a180].contains("M10") && !lines[idx_a180].contains("M11"), "Motion block must not contain clamp words");
    assert_eq!(lines[idx_a180 + 1].trim(), "M10", "M10 must be on its own line immediately after A180 move");
}

#[test]
fn doosan_dnm_high_speed_smoothing_aicc() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // #994: the hole carries its chain (plunge, retract to R) and the
    // stream leaves it by a rapid; a cut leaving the hole inside the
    // cycle group is refused by name, never swallowed.
    let stream = r#"; HSMW-CL v1 scale=0.001mm moves=8
PARAM WorkOffset G54
TOOL 1 "10mm Ball Endmill"
PARAM SpindleSpeed 8000
SECTION 3DFinish
Rapid 0 0 10000
Feed 10000 10000 -2000 F800000
Feed 20000 10000 -2000 F800000
Rapid 20000 10000 2000
CYCLE DRILL X20000 Y10000 Z-5000 R2000 F200000
Plunge 20000 10000 -5000 F200000
Rapid 20000 10000 2000
Rapid 30000 10000 2000
Feed 30000 10000 -2000 F800000
SECTION END
"#;
    // 1. Default smoothing level R5
    let options = serde_json::json!({ "smoothing": true });
    let out = engine::run_with(&cps, &mch, stream, &options).expect("posts smoothing");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    let lines: Vec<&str> = out.program.lines().collect();

    // G5.1 Q1 R5 engaged for high-speed cutting (default R5 per Doosan DNM sample)
    assert!(lines.iter().any(|l| l.contains("G5.1 Q1 R5")), "G5.1 Q1 R5 must be armed:\n{}", out.program);

    // G5.1 Q0 disabled before cycle
    let cycle_idx = lines.iter().position(|l| l.contains("G81")).expect("cycle present");
    let smoothing_off = lines[..cycle_idx].iter().rposition(|l| l.contains("G5.1 Q0"));
    assert!(
        smoothing_off.is_some(),
        "G5.1 Q0 must precede canned drilling cycles:\n{}",
        out.program
    );

    // 2. Custom bounded level R8
    let options_r8 = serde_json::json!({ "smoothing": true, "smoothingLevel": 8 });
    let out_r8 = engine::run_with(&cps, &mch, stream, &options_r8).expect("posts smoothing R8");
    assert!(out_r8.warnings.is_empty(), "{:?}", out_r8.warnings);
    assert!(out_r8.program.contains("G5.1 Q1 R8"), "G5.1 Q1 R8 must be emitted:\n{}", out_r8.program);
}

#[test]
fn doosan_dnm_chip_conveyor() {
    let cps = dnm_post();
    let mch = dnm_machine();

    let stream = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 1 "12mm Endmill"
PARAM SpindleSpeed 4000
SECTION Rough
Rapid 0 0 10000
Feed 50000 0 -2000 F600000
SECTION END
"#;
    let options = serde_json::json!({
        "chipConveyor": true,
        "chipConveyorStart": "M31",
        "chipConveyorStop": "M33"
    });
    let out = engine::run_with(&cps, &mch, stream, &options).expect("posts conveyor");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    assert!(out.program.contains("M31"), "M31 chip conveyor start must be commanded:\n{}", out.program);
    assert!(out.program.contains("M33"), "M33 chip conveyor stop must be commanded before end:\n{}", out.program);
}

#[test]
fn doosan_dnm_probing_modes() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // 1. Standard Fanuc G31 skip probing
    let stream_g31 = r#"; HSMW-CL v1 scale=0.001mm moves=3
PARAM WorkOffset G54
TOOL 99 "Touch Probe"
SECTION ProbeTouch
GUARD 0 0 5000 F 300000
TOUCH 0 0 -2000 EXPECT 0 0 0 F 50000 M 0
RETREAT 0 0 5000 EXPECT 0 0 5000 F 300000
SECTION END
"#;
    let out_g31 = engine::run(&cps, &mch, stream_g31).expect("posts standard probe");
    assert!(out_g31.warnings.is_empty(), "{:?}", out_g31.warnings);
    assert!(out_g31.program.contains("G31"), "standard probing uses G31:\n{}", out_g31.program);
    assert!(out_g31.program.contains("#3000=2(PROBE NO CONTACT)"));
    assert!(!out_g31.program.contains("P9832"), "standard probing does not emit optical turn-on P9832");

    // 2. Renishaw Inspection Plus macro style with optical turn-on (P9832) and turn-off (P9833)
    let options_insp = serde_json::json!({ "probingStyle": "inspection-plus" });
    let out_insp = engine::run_with(&cps, &mch, stream_g31, &options_insp).expect("posts inspection plus");
    assert!(out_insp.warnings.is_empty(), "{:?}", out_insp.warnings);
    assert!(out_insp.program.contains("G65 P9832"), "Inspection Plus uses P9832 for probe optical turn-on:\n{}", out_insp.program);
    assert!(out_insp.program.contains("G65 P9810"), "Inspection Plus uses P9810 for protected positioning:\n{}", out_insp.program);
    assert!(out_insp.program.contains("G65 P9811"), "Inspection Plus uses P9811 for surface measurement:\n{}", out_insp.program);
    assert!(out_insp.program.contains("G65 P9833"), "Inspection Plus uses P9833 for probe optical turn-off:\n{}", out_insp.program);

    // Verify ordering: P9832 comes before P9810, and P9833 comes after P9811 before M5
    let lines: Vec<&str> = out_insp.program.lines().collect();
    let idx_turn_on = lines.iter().position(|l| l.contains("G65 P9832")).expect("P9832 present");
    let idx_guard = lines.iter().position(|l| l.contains("G65 P9810")).expect("P9810 present");
    let idx_touch = lines.iter().position(|l| l.contains("G65 P9811")).expect("P9811 present");
    let idx_turn_off = lines.iter().position(|l| l.contains("G65 P9833")).expect("P9833 present");

    assert!(idx_turn_on < idx_guard, "P9832 probe turn-on must precede P9810 positioning");
    assert!(idx_guard < idx_touch, "P9810 positioning must precede P9811 touch");
    assert!(idx_touch < idx_turn_off, "P9811 touch must precede P9833 probe turn-off");
}

#[test]
fn doosan_dnm_refuses_undeclared_codes_by_name() {
    let cps = dnm_post();
    let vmc = default_vmc(); // lacks through-tool coolant and rotary A brake

    // 1. Refusal when through-tool coolant is requested on machine without TSC declaration
    let stream_tsc = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM Coolant through-tool
PARAM WorkOffset G54
TOOL 1 "10mm Flat Endmill"
PARAM SpindleSpeed 6000
SECTION FaceTSC
Rapid 0 0 10000
Feed 50000 0 -1000 F500000
SECTION END
"#;
    let out_tsc = engine::run(&cps, &vmc, stream_tsc).expect("runs engine");
    assert!(
        out_tsc.warnings.iter().any(|w| w.contains("the machine states no through-tool coolant code")),
        "must refuse undeclared through-tool coolant by name: {:?}",
        out_tsc.warnings
    );

    // 2. Refusal when rotary table positioning is commanded without declared rotary brake codes
    // (verified on both generic VMC and Doosan DNM machine definition to ensure no machine-name sniffing)
    let stream_rotary = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 1 "10mm Endmill"
PARAM SpindleSpeed 5000
SECTION RotaryIndex
Rapid 0 0 10000 A90000
SECTION END
"#;
    let out_rotary_vmc = engine::run(&cps, &vmc, stream_rotary).expect("runs engine");
    assert!(
        out_rotary_vmc.warnings.iter().any(|w| w.contains("rotary clamp/unclamp codes not declared")),
        "must refuse undeclared rotary clamp on generic vmc: {:?}",
        out_rotary_vmc.warnings
    );

    let out_rotary_dnm = engine::run(&cps, &dnm_machine(), stream_rotary).expect("runs engine");
    assert!(
        out_rotary_dnm.warnings.iter().any(|w| w.contains("rotary clamp/unclamp codes not declared")),
        "must refuse undeclared rotary clamp on dnm without explicit declaration: {:?}",
        out_rotary_dnm.warnings
    );

    // 3. Refusal when probing style is unknown
    let stream_probe = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 99 "Touch Probe"
SECTION ProbeTouch
GUARD 0 0 5000 F 300000
SECTION END
"#;
    let options_probe = serde_json::json!({ "probingStyle": "unknown-probe-macro" });
    let out_probe = engine::run_with(&cps, &dnm_machine(), stream_probe, &options_probe).expect("runs engine");
    assert!(
        out_probe.warnings.iter().any(|w| w.contains("unknown probing style unknown-probe-macro")),
        "must refuse unknown probing style by name: {:?}",
        out_probe.warnings
    );

    // 4. Refusal when smoothing level is out of bounds (R1 to R10 required)
    let stream_smooth = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 1 "10mm Ball Endmill"
PARAM SpindleSpeed 8000
SECTION Finish
Rapid 0 0 10000
Feed 10000 10000 -2000 F800000
SECTION END
"#;
    let options_smooth = serde_json::json!({ "smoothing": true, "smoothingLevel": 15 });
    let out_smooth = engine::run_with(&cps, &dnm_machine(), stream_smooth, &options_smooth).expect("runs engine");
    assert!(
        out_smooth.warnings.iter().any(|w| w.contains("smoothing R level must be between 1 and 10")),
        "must refuse out-of-range smoothing level by name: {:?}",
        out_smooth.warnings
    );

    // 5. Refusal when chip conveyor is commanded without declared codes
    let stream_cc = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 1 "10mm Endmill"
PARAM SpindleSpeed 5000
SECTION Cut
Rapid 0 0 10000
Feed 10000 0 0 F500000
SECTION END
"#;
    let options_cc = serde_json::json!({ "chipConveyor": true });
    let out_cc = engine::run_with(&cps, &dnm_machine(), stream_cc, &options_cc).expect("runs engine");
    assert!(
        out_cc.warnings.iter().any(|w| w.contains("chip conveyor codes not declared for this machine or post options")),
        "must refuse undeclared chip conveyor codes by name: {:?}",
        out_cc.warnings
    );
}

/// #997: the RTCP path names only the rotaries the machine states - the
/// A+B tilthead's arm writes A and B, never C0 for the C it has none of.
#[test]
fn doosan_dnm_absent_rotary_takes_no_angle_word() {
    let cps = dnm_post();
    let mch = machine::mirror(&parse_mch(&read("assets/machines/tilthead.mch")).expect("tilthead parses"));
    let stream = "TOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 20000 A 30000 B 0 C 0\nFeed X 20000 Y 0 Z 0 F 300000 A 0 B 30000 C 0\nENDSECTION\n";
    let options = serde_json::json!({ "rotaryBrake": true });
    let out = engine::run_with(&cps, &mch, stream, &options).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G43.4"), "{}", out.program);
    assert!(out.program.lines().any(|l| l.contains("X0 Y0 Z20 A30 B0") && !l.contains(" C")), "{}", out.program);
    let c_word = |l: &str| l.split(' ').any(|w| w.len() > 1 && w.starts_with('C') && w[1..].parse::<f64>().is_ok());
    assert!(!out.program.lines().any(c_word), "no C word on a machine with no C: {}", out.program);
}

/// #1155: the indexing path (no RTCP) names only a rotary the machine
/// states. An A index on a machine with no A is refused by name with no A
/// word written; a stated C that turns is refused (this path writes only
/// A); the shipped A machine posts its index words unchanged.
#[test]
fn doosan_dnm_index_names_only_a_stated_rotary() {
    let cps = dnm_post();
    let opts = serde_json::json!({ "rotaryBrake": true });
    let stream = |a: &str, c: &str| format!("TOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 10000 A {a} C {c}\nFeed X 50000 Y 0 Z -1000 F 400000 A {a} C {c}\nENDSECTION\n");
    let axis = |l: &str, k: char| l.split(' ').any(|w| w.len() > 1 && w.starts_with(k) && w[1..].parse::<f64>().is_ok());
    let out = engine::run_with(&cps, &serde_json::json!({}), &stream("90000", "0"), &opts).expect("post");
    assert_eq!(out.warnings, vec!["indexed move commands A - the machine states no A rotary"]);
    assert!(!out.program.lines().any(|l| axis(l, 'A')), "no A word without an A: {}", out.program);
    let mt = machine::mirror(&parse_mch(&read("assets/machines/millturn.mch")).expect("millturn parses"));
    let out = engine::run_with(&cps, &mt, &stream("0", "45000"), &opts).expect("post");
    assert_eq!(out.warnings, vec!["indexed move commands C - this dialect's indexing path writes only an A rotary"]);
    assert!(!out.program.lines().any(|l| axis(l, 'C')), "no C word: {}", out.program);
    let out = engine::run_with(&cps, &dnm_machine(), &stream("90000", "0"), &opts).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let moves: Vec<&str> = out.program.lines().filter(|l| l.starts_with("G00") || l.starts_with("G01") || *l == "M10" || *l == "M11").collect();
    assert_eq!(moves, vec!["M11", "G00 X0 Y0 Z10 A90", "M10", "G01 X50 Y0 Z-1 F400"], "{}", out.program);
}

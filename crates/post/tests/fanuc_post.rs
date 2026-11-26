//! The Fanuc post golden law (M1.7b): `assets/posts/fanuc.cps` posts
//! every committed `.cl` golden to byte-identical G-code. The .nc
//! files join generator code and the post script in one lock - if
//! either changes meaning, this test says so, and the diff is the
//! review (UPDATE workflow per tests/golden/CLAUDE.md).
//!
//! The probe streams are recorded (#926): `recorded/inputs/probe_t7_*`,
//! written and guarded by app's `recorded_post_streams` from
//! `motion::probing::probe_cycle`.
//!
//! Gate map (M4.4): covers post::post, post::probe_rec (probe result
//! arithmetic, #377), assets/posts/fanuc.cps.

mod recorded;
// #1357: the legacy goldens dressed the way app's tests post them (a
// cycle's own chain feed; a real program's tool and spindle).
#[path = "../../app/tests/common/legacy_program.rs"]
#[allow(dead_code)]
mod legacy_program;

use camcraft_post::post::{self, cnc, engine};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const GOLDENS: &[&str] = &[
    "contour_disc_hole",
    "contour_plate_with_hole",
    "contour_tangential_lead",
    "face_square_40x40",
    "face_square_40x40_stepped",
    "pocket_waterline_plate_with_island",
    "pocket_zigzag_plate_with_island",
    "slot_fullwidth_centerline",
    "slot_wide_channel_rows",
    "trace_plate_with_hole",
    "drill_plate",
    "drill_peck",
    "tap_plate",
    "trochoid_slot_small",
];

#[test]
fn fanuc_posts_every_golden_byte_exact() {
    let cps = read("assets/posts/fanuc.cps");
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    for name in GOLDENS {
        // #1357, #1359: the golden as the program a real operation
        // streams (tool change, spindle speed, each cycle's own chain
        // feed) - conformance.rs posts the same golden files from the
        // same dressed stream. A cut with the spindle stopped is refused.
        let stream = legacy_program::as_program(name, &read(&format!("crates/app/tests/golden/{name}.cl")));
        let out = engine::run(&cps, &mch, &stream)
            .unwrap_or_else(|e| panic!("{name} must post: {e}"));
        if std::env::var_os("WRITE_GOLDEN").is_some() && *name == "trochoid_slot_small" {
            // The one golden here conformance.rs does not post (the
            // golden_cl workflow: the review is the git diff).
            std::fs::write(repo(&format!("crates/app/tests/golden/nc/{name}.nc")), &out.program)
                .unwrap_or_else(|e| panic!("{name}.nc: {e}"));
            continue;
        }
        let want = read(&format!("crates/app/tests/golden/nc/{name}.nc"));
        assert_eq!(out.program, want, "{name}.nc is stale; regenerate with the CLI (see tests/golden/CLAUDE.md)");
        assert!(out.warnings.is_empty(), "{name}: the gate counts warnings: {:?}", out.warnings);
    }
}

#[test]
fn cnc_roundtrip_posts_the_same_program() {
    // The superset law end-to-end: reading a .cl, serializing v1, and
    // reposting gives the identical program (the two input dialects
    // differ only in what they carry, not in what the script sees).
    let cps = read("assets/posts/fanuc.cps");
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    let a = engine::run(&cps, &mch, &stream).expect("legacy posts");
    let records = cnc::parse(&stream).expect("parses");
    let b = engine::run(&cps, &mch, &cnc::serialize(&records)).expect("v1 posts");
    assert_eq!(a.program, b.program);
}

#[test]
fn a_dumped_lean_posts_the_same_program_as_a_tilted_stream() {
    // M5.1 A end to end: a generator's `Move.axis` and a hand-written
    // stream's keyed letters must reach the script through ONE door -
    // the record's canonical ABC. If the two ever disagree, the dump
    // is inventing poses or losing them, and this says so.
    use camcraft_post::cl::{Move, MoveKind, Toolpath};
    use camcraft_post::geometry::Point3;
    use camcraft_post::post::Dump;
    let cps = read("assets/posts/fanuc.cps");
    let mch = serde_json::json!({
        "name": "TILTHEAD",
        "kinematics": { "static": { "rotaries": [
            { "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0],
              "travel": [-120.0, 120.0] },
            { "letter": "B", "type": "rotary", "direction": [0.0, 1.0, 0.0],
              "travel": [-120.0, 120.0] } ] } }
    });
    let (sa, ca) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Feed, Point3 { x: 0.0, y: 0.0, z: 5.0 }, Point3 { x: 0.0, y: 0.0, z: 0.0 }, Some(150.0)),
        Move::new(MoveKind::Feed, Point3 { x: 0.0, y: 0.0, z: 0.0 }, Point3 { x: 10.0, y: 0.0, z: 0.0 }, Some(150.0))
            .with_axis([sa, 0.0, ca]),
    ]);
    let d = Dump {
        op_name: "Lean1",
        part_load: None,
        part_unload: false,
        part_transfer: false,
        probe_tol: None,
        probe_result: None,
        tool_number: 1,
        tool_name: "EM-0400",
        tool_diameter: 4.0,
        rpm: 2500.0,
        clearance_z: 12.0,
        retract_z: 10.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec![],
        coolant: Default::default(),
        after: Default::default(),
        ..Default::default()
    };
    let stream = cnc::serialize(&camcraft_post::post::dump(&path, &d));
    // The seam wrote the letters onto the stream it dumped - the
    // same keyed words stage D's hand-written streams carry.
    assert!(stream.contains(" A 0 B 20000 C 0"), "{stream}");
    let out = engine::run(&cps, &mch, &stream).expect("a lean posts on a machine that leans");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G43.4"), "armed: {}", out.program);
    assert!(
        out.program
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'B' && (w[1].is_ascii_digit() || w[1] == b'-')),
        "a B angle word rides the program: {}",
        out.program
    );
    // The upright prefix of the same run states nothing: cut the lean
    // move and the program is the ordinary 3-axis text.
    let flat = Toolpath::new(vec![path.moves[0]]);
    let plain = engine::run(
        &cps,
        &mch,
        &cnc::serialize(&camcraft_post::post::dump(&flat, &d)),
    )
    .expect("prismatic posts");
    assert!(plain.warnings.is_empty());
    assert!(!plain.program.contains("G43.4"), "{}", plain.program);
}

#[test]
fn post_errors_are_refusals_not_output() {
    // Degenerate intermediates must refuse cleanly through the whole
    // stack (parse -> drive), never emit a partial program.
    let cps = read("assets/posts/fanuc.cps");
    for bad in ["Goto 1 2\n", "Feed X zz Y 0 Z 0\n", "TOOL abc\n"] {
        let e = engine::run(&cps, &serde_json::Value::Null, bad);
        assert!(e.is_err(), "degenerate input must error: {bad:?}");
    }
}

#[test]
fn rtcp_output_arms_once_and_walks_the_angles() {
    // Stage D: the trunnion machine + a tilted stream posts G43.4
    // once, the angle words modally (only the changed letter on the
    // next block), and the home return carries its zeros. The
    // machine states a tilting rotary (A about X); the post reads
    // machine.rtcp and honors it.
    let cps = read("assets/posts/fanuc.cps");
    let mch = serde_json::json!({
        "name": "TRUNNION",
        "kinematics": { "static": { "rotaries": [
            { "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0],
              "travel": [-120.0, 120.0] } ] } }
    });
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 10MM BALL
PARAM SpindleSpeed 6000
SECTION
Rapid X 10000 Y 0 Z 5000 A 40000 B 0 C 0
Feed X 10000 Y 0 Z 0 F 150000 A 40000 B 0 C 0
Feed X 20000 Y 0 Z 0 F 150000 A 45000 B 0 C 0
Feed X 30000 Y 0 Z 0 F 150000 A 0 B 0 C 0
ENDSECTION
";
    let out = engine::run(&cps, &mch, stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let at = |needle: &str| -> usize {
        lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("no {needle:?} in:\n{}", out.program))
    };
    let arm = at("G43.4 H01");
    let tilt1 = at("A40");
    assert!(arm < tilt1, "the arm precedes the first tilt");
    assert_eq!(lines.iter().filter(|l| l.contains("G43.4")).count(), 1, "armed once");
    // #997: the arm names every rotary the machine states - here only A;
    // the stream's B0 and C0 are letters of rotaries it has none of.
    // #1633: the first traverse's stated height, under the arm (the work
    // frame), then the record with the stated rotaries.
    assert_eq!(lines[arm + 1], "G00 Z5.", "the clearance stated under G43.4");
    assert_eq!(lines[arm + 2], "G00 X10. Y0. Z5. A40.", "the stated rotaries on the arm");
    // The cut continues: only A changed since the last block.
    assert_eq!(lines[at("A45.")], "G01 X20. Y0. Z0. A45. F150.", "modal words: unchanged angles rest");
    // (#997: with A the only stated rotary, the arm and the modal block
    // carry the same one angle word - the exact lines above pin both.)
    // The home return: only A had left zero, so only A comes back -
    // the modal law runs in both directions. TCPM stays armed (the
    // next G49 would be a new preamble's; this program ends first).
    assert_eq!(lines[at("X30.")], "G01 X30. Y0. Z0. A0. F150.");
}

/// #997: on the A+B tilthead (no C) the arm writes A and B, never a C
/// word - a control without a C axis alarms on the address; a C that
/// turns stays refused by name.
#[test]
fn an_absent_rotary_takes_no_angle_word() {
    let cps = read("assets/posts/fanuc.cps");
    let text = read("assets/machines/tilthead.mch");
    let mch = post::machine::mirror(&camcraft_post::machine::parse_mch(&text).expect("tilthead parses"));
    let stream = |c: &str| {
        format!("PARAM ClearancePlane 20\nTOOL 1 T1\nPARAM SpindleSpeed 6000\nSECTION\nRapid X 0 Y 0 Z 20000 A 30000 B 0 C {c}\nFeed X 20000 Y 0 Z 0 F 300000 A 0 B 30000 C {c}\nENDSECTION\n")
    };
    let out = engine::run(&cps, &mch, &stream("0")).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l == "G00 X0. Y0. Z20. A30. B0."), "{}", out.program);
    assert!(out.program.lines().any(|l| l == "G01 X20. Y0. Z0. A0. B30. F300."), "{}", out.program);
    let c_word = |l: &str| l.split(' ').any(|w| w.len() > 1 && w.starts_with('C') && w[1..].parse::<f64>().is_ok());
    assert!(!out.program.lines().any(c_word), "no C word on a machine with no C: {}", out.program);
    let out = engine::run(&cps, &mch, &stream("10000")).expect("post");
    assert_eq!(out.warnings, vec!["tilted move commands C - the machine states no C rotary"]);
}

#[test]
fn tool_change_rearms_tcpm() {
    // G43.4 belongs to the tool: the next tool's first tilted move
    // re-arms it (the second G43.4 sits with the second G43).
    let cps = read("assets/posts/fanuc.cps");
    let mch = serde_json::json!({
        "kinematics": { "static": { "rotaries": [
            { "letter": "B", "type": "rotary", "direction": [0.0, 1.0, 0.0] } ] } }
    });
    let stream = "\
PARAM ClearancePlane 5
TOOL 1 T1
SECTION
Rapid X 0 Y 0 Z 5 B 30000
ENDSECTION
PARAM ClearancePlane 5
TOOL 2 T2
SECTION
Rapid X 0 Y 0 Z 5 B 30000
ENDSECTION
";
    let out = engine::run(&cps, &mch, stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G43.4").count(), 2, "{}", out.program);
    // The second tilt re-commands B even though its value repeats:
    // the arm, not the angle, reset.
    assert_eq!(out.program.matches("B30").count(), 2, "{}", out.program);
}

#[test]
fn tilted_move_on_a_prismatic_machine_is_refused_by_name() {
    // The refusal law (stage D): a stream that turns a tool the
    // machine never said it could tilt is blocked, not reposted as
    // prismatic motion (the same XYZ means different metal without
    // RTCP - reinterpretation would be the silent wrong answer).
    let cps = read("assets/posts/fanuc.cps");
    let stream = "\
TOOL 1 T1
SECTION
Rapid X 0 Y 0 Z 5 A 40000
ENDSECTION
";
    let out = engine::run(&cps, &serde_json::json!({}), stream).expect("run");
    assert_eq!(
        out.warnings,
        vec!["tilted move on a machine without a tilting rotary"],
        "counted, named, and (via the gate) write-blocking"
    );
    // An axis word is A followed by a number or sign - a bare
    // contains('A') would trip on any prose in the header comments
    // (the brand name has an A in it; the axis law does not care).
    assert!(
        !out.program
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'A' && (w[1].is_ascii_digit() || w[1] == b'-')),
        "no angle word rides a prismatic program: {}",
        out.program
    );
}

#[test]
fn non_rtcp_dialects_refuse_a_tilted_stream() {
    // Haas (and the four share the law): a tilted stream is a
    // refusal, never a prismatic approximation of 5-axis motion.
    for name in ["haas", "mazak", "sinumerik", "heidenhain", "tormach", "grblhal", "tinyg", "grbl", "mach3"] {
        let cps = read(&format!("assets/posts/{name}.cps"));
        let stream = "TOOL 1 T1\nSECTION\nFeed X 0 Y 0 Z -1 F 1000 A 40000\nENDSECTION\n";
        let out = engine::run(&cps, &serde_json::json!({}), stream).expect("run");
        assert!(!out.warnings.is_empty(), "{name}: a tilt must warn");
        // Any of the refusals: grblHAL also refuses its Z lift here (no
        // clearance height in this bare stream, #710), before the tilt.
        assert!(
            out.warnings.iter().any(|w| w.contains("RTCP")),
            "{name} refuses by name: {:?}",
            out.warnings
        );
    }
}

// ------------------------------------------------------------ #377
// A probe result's deviation is `found - expected`. Edge and outside-corner
// probes whose expected value is negative in the work offset once wrote a
// doubled sign (`#190=#100--15`); the sign now folds into the operator
// (`#190=#100+15`), the arithmetic form Fanuc's published custom macro B
// documentation gives (`#i=#j+#k`, `#i=#j-#k`).

/// Posts a recorded probe cycle (an edge or corner touch toward +X/+Y,
/// tool 7, setting G54) through fanuc.cps, and refuses any doubled sign
/// on any line.
fn post_probe(input: &str) -> String {
    let stream = recorded::input(input);
    let out = engine::run(&read("assets/posts/fanuc.cps"), &serde_json::Value::Null, &stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    for l in out.program.lines() {
        assert!(!l.contains("--") && !l.contains("+-") && !l.contains("-+"), "doubled sign in `{l}`");
    }
    out.program
}

#[test]
fn edge_probe_at_negative_x_folds_the_sign() {
    let nc = post_probe("probe_t7_edge_neg_x_g54.cnc");
    // The nominal trip is the ball centre, a radius short of the edge: -17.
    assert!(nc.lines().any(|l| l == "#190=#100+17"), "{nc}");
    assert!(nc.contains("IF[ABS[#5061+12]"), "the no-contact check folds a negative target too: {nc}");
    recorded::app_golden("nc/fanuc/probe_edge_negative_x.nc", &nc);
}

#[test]
fn corner_probe_at_negative_xy_folds_both_signs() {
    let nc = post_probe("probe_t7_corner_neg_g54.cnc");
    assert!(nc.lines().any(|l| l == "#190=#100+17"), "{nc}");
    assert!(nc.lines().any(|l| l == "#191=#104+12"), "{nc}");
    recorded::app_golden("nc/fanuc/probe_corner_negative.nc", &nc);
}

#[test]
fn edge_probe_at_zero_writes_the_bare_mean() {
    // An edge at X 2 trips the 4 mm ball's centre at X 0: expected 0 adds
    // no `-0` / `+0` term, just the measured value. A hair under it
    // (formats to -0) does the same - the recorded stream is in microns,
    // so its record already reads as the zero edge's.
    let nc = post_probe("probe_t7_edge_zero_g54.cnc");
    assert!(nc.lines().any(|l| l == "#190=#100"), "{nc}");
    let nc = post_probe("probe_t7_edge_hair_g54.cnc");
    assert!(nc.lines().any(|l| l == "#190=#100"), "{nc}");
}

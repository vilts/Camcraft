//! Tormach Explain G-code (#1387): original tests of the public LinuxCNC
//! comment contract, using the repository's recorded post matrix.
//! Gate map (M4.4): covers assets/posts/tormach.cps, camcraft_post::post::engine.

#[path = "../../app/tests/common/legacy_program.rs"]
mod legacy_program;
mod recorded;

use camcraft_post::post::engine;
use serde_json::{json, Value};

const CPS: &str = include_str!("../../../assets/posts/tormach.cps");

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
];

fn machine(case: &str) -> Value {
    let Some(path) = recorded::machine_of(case) else {
        return Value::Null;
    };
    let text = std::fs::read_to_string(path).expect("recorded machine");
    let parsed = camcraft_post::machine::parse_mch(&text).expect("machine parses");
    camcraft_post::post::machine::mirror(&parsed)
}

fn assert_comments_only(plain: &str, explained: &str) {
    let old: Vec<_> = plain.lines().collect();
    let new: Vec<_> = explained.lines().collect();
    assert_eq!(old.len(), new.len(), "Explain never adds program blocks");
    let mut stripped = String::new();
    for (old, new) in old.into_iter().zip(new) {
        let stripped_line = if old != new {
            let tail = new
                .strip_prefix(old)
                .expect("Explain never changes an existing byte");
            let comment = tail
                .strip_prefix(" (")
                .and_then(|s| s.strip_suffix(')'))
                .expect("one appended comment");
            assert!(!comment.contains(['(', ')', ';', '%']), "{new}");
            assert!(comment.bytes().all(|b| (0x20..=0x7e).contains(&b)), "{new}");
            assert!(comment.len() + 2 <= 60, "{new}");
            // Check explanations only: probe diagnostics intentionally use MSG.
            let first = comment
                .split(|c: char| !c.is_ascii_alphabetic())
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            assert!(
                ![
                    "MSG",
                    "DEBUG",
                    "PRINT",
                    "LOG",
                    "LOGOPEN",
                    "LOGAPPEND",
                    "LOGCLOSE",
                    "PROBEOPEN",
                    "PROBECLOSE",
                    "ABORT",
                    "AXIS",
                    "PY",
                    "PYRUN"
                ]
                .contains(&first.as_str()),
                "active comment: {new}"
            );
            new.strip_suffix(tail).expect("validated appended suffix")
        } else {
            new
        };
        stripped.push_str(stripped_line);
        stripped.push('\n');
    }
    assert_eq!(
        stripped, plain,
        "stripping explanations recovers the plain program bytewise"
    );
}

#[test]
fn tormach_explain_matrix_adds_only_inert_comments_and_keeps_refusals() {
    for case in recorded::CASES {
        let stream = std::fs::read_to_string(recorded::stream_path(case)).expect("recorded stream");
        let mch = machine(case);
        for units in ["mm", "inch"] {
            let off = engine::run_with(
                CPS,
                &mch,
                &stream,
                &json!({"Units": units, "Explain": false}),
            );
            let on = engine::run_with(
                CPS,
                &mch,
                &stream,
                &json!({"Units": units, "Explain": true}),
            );
            match (off, on) {
                (Ok(off), Ok(on)) => {
                    assert_eq!(
                        on.warnings, off.warnings,
                        "{case} {units}: warnings stay unchanged"
                    );
                    assert_eq!(
                        on.swallowed, off.swallowed,
                        "{case} {units}: swallowed records stay unchanged"
                    );
                    assert_comments_only(&off.program, &on.program);
                    assert_ne!(off.program, on.program, "{case} {units}: Explain adds text");
                }
                (Err(off), Err(on)) => assert_eq!(
                    off.to_string(),
                    on.to_string(),
                    "{case} {units}: same refusal"
                ),
                other => panic!("{case} {units}: Explain changed whether the post runs: {other:?}"),
            }
        }
    }
}

#[test]
fn tormach_explains_every_conformance_golden_without_changing_the_program() {
    let text = std::fs::read_to_string(recorded::repo().join("assets/machines/tormach-770mx.mch"))
        .expect("770MX");
    let mch = camcraft_post::machine::parse_mch(&text).expect("770MX parses");
    let mch = camcraft_post::post::machine::mirror(&mch);
    for case in GOLDENS {
        let cl = std::fs::read_to_string(
            recorded::repo().join(format!("crates/app/tests/golden/{case}.cl")),
        )
        .expect("CL golden");
        let stream = legacy_program::as_program(case, &cl);
        let off = engine::run_with(
            CPS,
            &mch,
            &stream,
            &json!({"Units": "inch", "Explain": false}),
        )
        .expect("plain posts");
        let on = engine::run_with(
            CPS,
            &mch,
            &stream,
            &json!({"Units": "inch", "Explain": true}),
        )
        .expect("Explain posts");
        assert!(off.warnings.is_empty(), "{case}: {:?}", off.warnings);
        assert_eq!(on.warnings, off.warnings, "{case}");
        // Even during generation, compare off output with the committed program.
        let want = std::fs::read_to_string(
            recorded::repo().join(format!("crates/app/tests/golden/nc/tormach/{case}.nc")),
        )
        .expect("plain golden");
        assert_eq!(
            off.program,
            want.replace("\r\n", "\n"),
            "{case}: Explain off stays byte-identical"
        );
        assert_comments_only(&off.program, &on.program);
        assert_ne!(off.program, on.program, "{case}: Explain adds comments");
        recorded::app_golden(&format!("nc/tormach/explain_{case}.nc"), &on.program);
    }
}

#[test]
fn tormach_explains_linuxcnc_probe_and_extended_offsets() {
    let cps = format!("{CPS}\nfunction onOpen() {{ block([\"G38.2\", \"G59.1\", \"G59.2\", \"G59.3\"]); }}\nfunction onClose() {{}}\n");
    let on = engine::run_with(
        &cps,
        &Value::Null,
        &camcraft_post::post::cnc::serialize(&[]),
        &json!({"Explain": true}),
    )
    .expect("probe explanation");
    assert!(
        on.program.contains("PROBE TOWARD CONTACT"),
        "{}",
        on.program
    );
    // One word per block avoids the normal comment-length truncation.
    for (word, meaning) in [
        ("G59.1", "WORK OFFSET 7"),
        ("G59.2", "WORK OFFSET 8"),
        ("G59.3", "WORK OFFSET 9"),
    ] {
        let cps = format!(
            "{CPS}\nfunction onOpen() {{ block([\"{word}\"]); }}\nfunction onClose() {{}}\n"
        );
        let on = engine::run_with(
            &cps,
            &Value::Null,
            &camcraft_post::post::cnc::serialize(&[]),
            &json!({"Explain": true}),
        )
        .expect("offset explanation");
        assert_eq!(on.program, format!("{word} ({meaning})\n"));
    }
}

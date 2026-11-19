//! #2344: a cumulative A refusal names a remedy without widening travel.
//! #2471: the shipped microARC 4 states no A soft limit (continuous rotary on PathPilot).
//! Provenance: original boundary streams over our microARC machine and post.
//!
//! Gate map (M4.4): covers assets/posts/tormach.cps, camcraft_post::post::engine,
//! camcraft_post::post::machine.

use camcraft_post::cl::MoveKind;
use camcraft_post::machine::parse_mch;
use camcraft_post::post::{cnc, engine, machine};
use serde_json::json;

const POST: &str = include_str!("../../../assets/posts/tormach.cps");
const MICROARC: &str = include_str!("../../../assets/machines/tormach-770mx-microarc4.mch");

/// The tip lies on the rotary centreline, so A-only turns need no chords.
/// Each turn is under 180 degrees; the last target must accumulate to
/// +/-361, rather than being reinterpreted as a harmless +/-1 index.
fn stream(sign: f64, end: f64) -> String {
    let mut records = vec![
        cnc::Record::Param {
            name: "OperationName".into(),
            value: json!("Journal"),
        },
        cnc::Record::Param {
            name: "OperationKind".into(),
            value: json!("Rotary Offset"),
        },
        cnc::Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(6000),
        },
        cnc::Record::Param {
            name: "ClearancePlane".into(),
            value: json!(40.0),
        },
        cnc::Record::Param {
            name: "WorkOffset".into(),
            value: json!("G54"),
        },
        cnc::Record::Tool {
            number: 3,
            name: "BALL".into(),
        },
        cnc::Record::Section { begin: true },
    ];
    for (i, angle) in [0.0, 90.0, 180.0, 270.0, end].into_iter().enumerate() {
        records.push(cnc::Record::Move(cnc::MoveRec {
            kind: if i == 0 {
                MoveKind::Rapid
            } else {
                MoveKind::Feed
            },
            x: 0.0,
            y: 0.0,
            z: 0.0,
            feed: (i != 0).then_some(600.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: Some([sign * angle, 0.0, 0.0]),
        }));
    }
    records.push(cnc::Record::Section { begin: false });
    cnc::serialize(&records)
}

fn bounded_microarc() -> serde_json::Value {
    let mut mch = machine::mirror(&parse_mch(MICROARC).expect("shipped microARC parses"));
    mch["kinematics"]["static"]["rotaries"][0]["travel"] = json!([-360.0, 360.0]);
    mch
}

fn run_bounded(sign: f64, end: f64) -> engine::PostOutput {
    engine::run_with(POST, &bounded_microarc(), &stream(sign, end), &json!({"Units": "mm"}))
        .expect("post runs")
}

/// Multi-turn Rotary Offset stream stepping 90 degrees at a time up to `end`
/// so every increment stays under 180 degrees.
fn multi_turn_stream(sign: f64, end: f64) -> String {
    let mut records = vec![
        cnc::Record::Param {
            name: "OperationName".into(),
            value: json!("Journal"),
        },
        cnc::Record::Param {
            name: "OperationKind".into(),
            value: json!("Rotary Offset"),
        },
        cnc::Record::Param {
            name: "SpindleSpeed".into(),
            value: json!(6000),
        },
        cnc::Record::Param {
            name: "ClearancePlane".into(),
            value: json!(40.0),
        },
        cnc::Record::Param {
            name: "WorkOffset".into(),
            value: json!("G54"),
        },
        cnc::Record::Tool {
            number: 3,
            name: "BALL".into(),
        },
        cnc::Record::Section { begin: true },
    ];
    let mut angles_seq = Vec::new();
    let mut cur = 0.0;
    while cur < end - 1e-9 {
        angles_seq.push(cur);
        cur += 90.0;
    }
    angles_seq.push(end);
    for (i, angle) in angles_seq.into_iter().enumerate() {
        records.push(cnc::Record::Move(cnc::MoveRec {
            kind: if i == 0 {
                MoveKind::Rapid
            } else {
                MoveKind::Feed
            },
            x: 0.0,
            y: 0.0,
            z: 0.0,
            feed: (i != 0).then_some(600.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: Some([sign * angle, 0.0, 0.0]),
        }));
    }
    records.push(cnc::Record::Section { begin: false });
    cnc::serialize(&records)
}

fn angles(program: &str) -> Vec<f64> {
    program
        .lines()
        .filter(|line| !line.starts_with('('))
        .flat_map(str::split_whitespace)
        .filter_map(|word| word.strip_prefix('A').and_then(|a| a.parse().ok()))
        .collect()
}

fn assert_actionable_refusal(sign: f64) {
    let out = run_bounded(sign, 361.0);
    let warning = out.warnings.join("\n");
    assert!(
        warning.contains(&format!("A{} in operation Journal", sign * 361.0)),
        "{warning}"
    );
    assert!(
        warning.contains("beyond the A axis travel -360..360 degrees"),
        "{warning}"
    );
    assert!(
        warning.contains("A counts on through 360 (PathPilot does not roll A over)"),
        "{warning}"
    );
    assert!(
        warning
            .contains("use a machine with verified A travel wide enough for the whole operation"),
        "{warning}"
    );
    assert!(
        warning.contains("For Rotary Offset, cutting levels cover two laps plus seam padding"),
        "{warning}"
    );
    let posted = angles(&out.program);
    assert!(
        posted.iter().all(|a| (-360.0..=360.0).contains(a)),
        "{posted:?}"
    );
    assert_eq!(
        posted.last(),
        Some(&(sign * 270.0)),
        "the refused target is never written"
    );
}

#[test]
fn positive_a_overtravel_names_the_rotary_offset_limit_and_remedy() {
    assert_actionable_refusal(1.0);
}

#[test]
fn negative_a_overtravel_names_the_rotary_offset_limit_and_remedy() {
    assert_actionable_refusal(-1.0);
}

#[test]
fn rotary_offset_at_either_a_travel_boundary_still_posts() {
    for sign in [-1.0, 1.0] {
        let out = run_bounded(sign, 360.0);
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert_eq!(angles(&out.program).last(), Some(&(sign * 360.0)));
    }
}

#[test]
fn microarc_states_no_a_soft_limit_and_cites_sources() {
    let parsed = parse_mch(MICROARC).expect("shipped microARC parses");
    let a = parsed.axes.iter().find(|a| a.name == "A").expect("A axis");
    assert_eq!(a.travel, None, "PathPilot has no A soft limit (continuous rotary)");
    let raw: serde_json::Value = serde_json::from_str(MICROARC).expect("shipped microARC JSON");
    let desc = raw["general"]["description"].as_str().unwrap_or("");
    assert!(
        desc.contains("TD10749") && desc.contains("1484.663"),
        "machine file must name the Tormach source and Hub reading: {desc}"
    );
}

#[test]
fn microarc_posts_multi_turn_rotary_offset_past_four_turns() {
    let mch = machine::mirror(&parse_mch(MICROARC).expect("shipped microARC parses"));
    for sign in [-1.0, 1.0] {
        let out = engine::run_with(POST, &mch, &multi_turn_stream(sign, 1500.0), &json!({"Units": "mm"}))
            .expect("post runs");
        assert!(out.warnings.is_empty(), "sign {sign}: {:?}", out.warnings);
        assert_eq!(angles(&out.program).last(), Some(&(sign * 1500.0)));
    }
}

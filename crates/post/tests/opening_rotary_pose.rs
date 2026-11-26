//! #3300: a stream that leans states its rotaries before its first
//! tilted feed. A 5-axis stamp states the upright Z column on its
//! opening moves; the dump commands that column as explicit zero angles
//! on the first move that states it, so the Fanuc post's angle memory
//! knows where the rotaries stand when the first tilted feed is capped
//! at the stated rotary rate. A stream that states no pose before its
//! first tilted feed still refuses: the post never guesses an angle.
//!
//! Provenance: RS-274 modal rotary words, FANUC tool centre point
//! control (G43.4) as the post script already implements it; our own
//! design notes only.
//!
//! Gate map: covers post::dump (the opening zero angles),
//! assets/posts/fanuc.cps (rotaryFeed angle memory).

use camcraft_post::cl::{Move, MoveKind, Toolpath};
use camcraft_post::geometry::Point3;
use camcraft_post::post::{self, cnc, engine, Dump};
use std::path::Path;

const UNKNOWN_A_OR_B: &str = "from an angle the post does not know";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// A tilting head with A and B rates stated (deg/min), as on
/// `tilthead.mch`: the rate guard is armed.
fn rated_head() -> serde_json::Value {
    let rot = |l: &str, d: [f64; 3]| {
        serde_json::json!({ "letter": l, "type": "rotary", "direction": d,
            "travel": [-120.0, 120.0], "speeds": { "normal": 2000.0 } })
    };
    serde_json::json!({ "name": "RATED TILTHEAD", "feedRateMax": 5000.0,
        "kinematics": { "static": { "rotaries": [rot("A", [1.0, 0.0, 0.0]), rot("B", [0.0, 1.0, 0.0])] } } })
}

fn dump_of(path: &Toolpath) -> String {
    let d = Dump {
        op_name: "Lean1",
        tool_number: 1,
        tool_name: "BN-0600",
        tool_diameter: 6.0,
        rpm: 8000.0,
        clearance_z: 20.0,
        retract_z: 10.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        ..Default::default()
    };
    cnc::serialize(&post::dump(path, &d))
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3 { x, y, z }
}

/// The opening of a stamped path: upright rapids down to the clearance
/// and the plunge, then the first contact feed that leans 20 degrees
/// about Y. `opening` is the axis the upright moves state (`None`: a
/// stream that states no pose before its lean).
fn leaning_path(opening: Option<[f64; 3]>) -> Toolpath {
    let (s, c) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    let up = |m: Move| match opening {
        Some(a) => m.with_axis(a),
        None => m,
    };
    Toolpath::new(vec![
        up(Move::new(MoveKind::Rapid, p(0.0, 0.0, 50.0), p(0.0, 0.0, 20.0), None)),
        up(Move::new(MoveKind::Rapid, p(0.0, 0.0, 20.0), p(0.0, 0.0, 5.0), None)),
        up(Move::new(MoveKind::Feed, p(0.0, 0.0, 5.0), p(0.0, 0.0, 0.0), Some(300.0))),
        Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), Some(500.0)).with_axis([s, 0.0, c]),
    ])
}

fn post_fanuc(stream: &str) -> engine::PostOutput {
    engine::run(&read("assets/posts/fanuc.cps"), &rated_head(), stream).expect("the stream posts or refuses by warning")
}

/// The index of the first program line that holds `word` as a word.
fn line_of(program: &str, pred: impl Fn(&str) -> bool) -> Option<usize> {
    program.lines().position(pred)
}

fn has_word(line: &str, letter: char, value: f64) -> bool {
    line.split_whitespace().any(|w| {
        w.strip_prefix(letter).and_then(|v| v.parse::<f64>().ok()).is_some_and(|v| (v - value).abs() < 1e-6)
    })
}

#[test]
fn a_leaning_stream_commands_zero_angles_on_its_first_upright_move() {
    let stream = dump_of(&leaning_path(Some([0.0, 0.0, 1.0])));
    let out = post_fanuc(&stream);
    assert!(out.warnings.is_empty(), "no refusal: {:?}\n{}", out.warnings, out.program);
    let prog = &out.program;
    let zero = line_of(prog, |l| has_word(l, 'A', 0.0) && has_word(l, 'B', 0.0))
        .unwrap_or_else(|| panic!("the opening states A0 B0:\n{prog}"));
    let lean = line_of(prog, |l| has_word(l, 'B', 20.0)).unwrap_or_else(|| panic!("the lean posts:\n{prog}"));
    assert!(zero < lean, "the zero angles come before the first tilted feed:\n{prog}");
    let zl = prog.lines().nth(zero).unwrap_or_default();
    assert!(zl.starts_with("G00"), "the zero angles ride a rapid positioning move: {zl}\n{prog}");
    assert!(
        prog.lines().take(zero).any(|l| l.starts_with("G00") && has_word(l, 'Z', 20.0)),
        "the tool stands at the clearance before the rotaries move:\n{prog}"
    );
    assert!(
        !prog.lines().skip(zero).take(lean - zero).any(|l| l.starts_with("G01") && (has_word(l, 'A', 0.0) || l.contains('B'))),
        "no feed between the zero and the lean turns a rotary:\n{prog}"
    );
}

#[test]
fn a_stream_with_no_pose_before_its_first_tilted_feed_still_refuses() {
    // The guard stays (#885): no upright pose stated, so the post does
    // not know A or B when the tilted feed is capped - it refuses.
    for (what, path) in [
        ("prismatic opening", leaning_path(None)),
        ("feed first", Toolpath::new(vec![leaning_path(None).moves[3]])),
    ] {
        let out = post_fanuc(&dump_of(&path));
        assert!(
            out.warnings.iter().any(|w| w.contains(UNKNOWN_A_OR_B)),
            "{what}: the unknown angle is refused by name: {:?}\n{}",
            out.warnings,
            out.program
        );
    }
}

#[test]
fn an_upright_stream_that_never_leans_commands_no_angles() {
    // Byte for byte as before: stating the Z column alone writes no
    // rotary word and arms no tool centre point control.
    let mut path = leaning_path(Some([0.0, 0.0, 1.0]));
    path.moves[3] = path.moves[3].with_axis([0.0, 0.0, 1.0]);
    let stated = post_fanuc(&dump_of(&path));
    path.moves.iter_mut().for_each(|m| m.axis = None);
    let silent = post_fanuc(&dump_of(&path));
    assert!(stated.warnings.is_empty() && silent.warnings.is_empty());
    assert_eq!(stated.program, silent.program);
    assert!(!stated.program.contains("G43.4"), "{}", stated.program);
}

//! A feed held under a machine rate is judged on its POSTED word
//! (#1392): the Fanuc lathe post's C-axis live-tool section caps F so
//! the X/Z part stays within the machine's maximum feed and the C part
//! within its C rate, then writes F at one decimal. Rounding to nearest
//! could lift a feed within half a step of its cap up to 0.05 mm/min
//! over it; the word is floored to the step at or under the cap instead.
//! A feed far from its cap posts exactly as before.
//!
//! Gate map (M4.4): covers post::post (engine on fanuc-lathe.cps's
//! liveMove/capWord), post::machine (the mirror the script reads).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_post::cl::MoveKind;
use camcraft_post::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_post::post::{cnc, engine};
use serde_json::json;

const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

/// A test-bench mill-turn whose C rate is `c_rate` deg/min and whose
/// maximum linear feed is `fmax` mm/min. Placeholder live-tool words.
fn mill_turn(c_rate: f64, fmax: f64) -> serde_json::Value {
    let mut v = camcraft_post::post::machine::mirror(&Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(fmax), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(fmax), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                feed: Some(c_rate),
                rapid: Some(2.0 * c_rate),
                ..Default::default()
            },
        ],
        live_tool: Some(LiveToolCodes {
            c_axis_on: "M1001".into(),
            c_axis_off: "M1002".into(),
            spindle_on: "M1003".into(),
            spindle_off: "M1004".into(),
        }),
        ..Default::default()
    });
    // The bench states its maximum feed outright, so the test does not
    // lean on how the mirror folds the axis feeds into it.
    v["feedRateMax"] = json!(fmax);
    v
}

/// A live-tool move: the tip at radius `r` turned onto the station by
/// `c` degrees (part frame: the tip sits at angle -c), height `z`.
fn cmv(kind: MoveKind, r: f64, c: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    let a = -c.to_radians();
    cnc::Record::Move(cnc::MoveRec {
        kind,
        x: r * a.cos(),
        y: r * a.sin(),
        z,
        feed,
        center: None,
        ccw: false,
        plane: Default::default(),
        abc: Some([0.0, 0.0, c]),
    })
}

/// One live-tool section: position at C0 r10 z1, then `cut`.
fn live_post(machine: &serde_json::Value, cut: cnc::Record) -> engine::PostOutput {
    let r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Param { name: "LiveTool".into(), value: json!("radial") },
        cnc::Record::Tool { number: 2, name: "EM".into() },
        cnc::Record::Section { begin: true },
        cmv(MoveKind::Rapid, 10.0, 0.0, 1.0, None),
        cut,
        cnc::Record::Section { begin: false },
    ];
    engine::run(LATHE_CPS, machine, &cnc::serialize(&r)).expect("the script runs")
}

/// The F word of the one feed G01 that turns C.
fn f_word(out: &engine::PostOutput) -> f64 {
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let line = out
        .program
        .lines()
        .find(|l| l.starts_with("G01") && l.contains(" C"))
        .unwrap_or_else(|| panic!("a G01 with C: {}", out.program));
    let f = line.split_whitespace().find_map(|w| w.strip_prefix('F')).unwrap_or_else(|| panic!("F in {line}"));
    f.parse().unwrap_or_else(|_| panic!("F number in {line}"))
}

/// The planned feed whose composite (C-degrees-as-mm) rate is `target`
/// for a pure C turn of `dc` degrees at radius `r`.
fn planned_for(target: f64, r: f64, dc: f64) -> f64 {
    let chord = 2.0 * r * (dc.to_radians() / 2.0).sin();
    target * chord / dc
}

#[test]
fn a_feed_capped_at_a_c_rate_between_steps_posts_at_or_under_it() {
    // C rate 6000.06 deg/min: a pure C turn at a planned feed far over
    // it is capped to 6000.06 exactly. Rounded to nearest that wrote
    // F6000.1 - 0.04 deg/min over the machine's stated rate.
    let m = mill_turn(6000.06, 4000.0);
    let out = live_post(&m, cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(50000.0)));
    let f = f_word(&out);
    assert!(f <= 6000.06, "F{f} passes the C rate 6000.06: {}", out.program);
    assert_eq!(f, 6000.0, "floored to the step under the cap: {}", out.program);
}

#[test]
fn a_feed_one_step_under_its_c_cap_posts_at_or_under_it() {
    // The issue's case: the planned composite feed 6000.06 is UNDER the
    // C rate 6000.08 (so the cap does not bind), but rounds to 6000.1.
    let m = mill_turn(6000.08, 4000.0);
    let out = live_post(&m, cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(planned_for(6000.06, 10.0, 90.0))));
    let f = f_word(&out);
    assert!(f <= 6000.08, "F{f} passes the C rate 6000.08: {}", out.program);
    assert_eq!(f, 6000.0, "{}", out.program);
}

#[test]
fn a_feed_one_step_under_the_linear_cap_posts_at_or_under_it() {
    // C turns a little while Z runs far: the linear part binds. The
    // composite feed sits under the maximum feed's cap but rounds over.
    let m = mill_turn(1.0e6, 4000.07);
    let (r, dc, dz) = (10.0_f64, 0.5_f64, -40.0_f64);
    let a = dc.to_radians();
    let ltrue = ((r * a.cos() - r).powi(2) + (r * a.sin()).powi(2) + dz * dz).sqrt();
    let lcmd = (dz * dz + dc * dc).sqrt();
    let cap = 4000.07 * lcmd / dz.abs();
    // The composite rate 0.02 under the cap, and still over its step.
    let target = cap - 0.02;
    assert!(format!("{target:.1}").parse::<f64>().expect("number") > cap, "the bench must round over: {target} vs {cap}");
    let out = live_post(&m, cmv(MoveKind::Feed, r, dc, 1.0 + dz, Some(target * ltrue / lcmd)));
    let f = f_word(&out);
    assert!(f <= cap, "F{f} passes the linear cap {cap}: {}", out.program);
    assert!(cap - f < 0.1, "floored by one step, no more: F{f} vs {cap}");
}

#[test]
fn a_feed_far_from_its_cap_posts_unchanged() {
    // 1234.56 composite under a 6000 C rate: rounded to nearest as ever.
    let m = mill_turn(6000.0, 4000.0);
    let out = live_post(&m, cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(planned_for(1234.56, 10.0, 90.0))));
    assert_eq!(f_word(&out), 1234.6, "{}", out.program);
    // And a feed exactly at its cap keeps the cap's own word.
    let out = live_post(&mill_turn(6000.0, 4000.0), cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(50000.0)));
    assert_eq!(f_word(&out), 6000.0, "{}", out.program);
}

/// One live-tool section over `moves` (the first must be a rapid).
fn live_moves(machine: &serde_json::Value, moves: Vec<cnc::Record>) -> engine::PostOutput {
    let mut r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Param { name: "LiveTool".into(), value: json!("radial") },
        cnc::Record::Tool { number: 2, name: "EM".into() },
        cnc::Record::Section { begin: true },
    ];
    r.extend(moves);
    r.push(cnc::Record::Section { begin: false });
    engine::run(LATHE_CPS, machine, &cnc::serialize(&r)).expect("the script runs")
}

/// Every C-section G01 as the control reads it: (X/Z rate, C rate) in
/// mm/min and deg/min, recomputed from the POSTED words alone - X a
/// diameter, C degrees counted as mm in the composite length.
fn posted_rates(program: &str) -> Vec<(f64, f64, String)> {
    let (mut x, mut z, mut c) = (None::<f64>, None::<f64>, None::<f64>);
    let mut out = Vec::new();
    for line in program.lines() {
        let word = |l: char| {
            line.split_whitespace().find_map(|w| w.strip_prefix(l)).map(|v| v.parse::<f64>().unwrap_or(f64::NAN))
        };
        let (nx, nz, nc, f) = (word('X'), word('Z'), word('C'), word('F'));
        if line.starts_with("G01") && nc.is_some() {
            let (Some(x0), Some(z0), Some(c0), Some(f)) = (x, z, c, f) else {
                panic!("a G01 with C and no prior X Z C or no F: {program}");
            };
            let dr = (nx.unwrap_or(x0) - x0) / 2.0;
            let dz = nz.unwrap_or(z0) - z0;
            let dc = nc.unwrap_or(c0) - c0;
            let lin = (dr * dr + dz * dz).sqrt();
            let lcmd = (lin * lin + dc * dc).sqrt();
            if lcmd > 0.0 {
                out.push((f * lin / lcmd, f * dc.abs() / lcmd, line.to_string()));
            }
        }
        x = nx.or(x);
        z = nz.or(z);
        c = nc.or(c);
    }
    out
}

#[test]
fn a_capped_move_is_judged_on_its_posted_words() {
    // #1398 (M3's probe on #1397): the move is short, so rounding the
    // words to their lattice lengthens it - raw r 10.00074 -> 10.00376
    // posts X20.001 -> X20.008, raw C 30.0004 -> 30.0096 posts C30 ->
    // C30.01. A cap measured on the raw points let the posted words
    // drive X/Z (or C) several percent past what the machine states.
    let moves = || {
        vec![
            cmv(MoveKind::Rapid, 10.00074, 30.0004, 5.0, None),
            cmv(MoveKind::Feed, 10.00376, 30.0096, 5.0, Some(50000.0)),
        ]
    };
    for (c_rate, fmax, what) in [(1.0e6, 1000.0, "the maximum feed binds"), (100.0, 1.0e6, "the C rate binds")] {
        let out = live_moves(&mill_turn(c_rate, fmax), moves());
        assert!(out.warnings.is_empty(), "{what}: {:?}", out.warnings);
        assert!(out.program.contains("X20.008 Z5. C30.01 F"), "{what}: the probe's words: {}", out.program);
        let rates = posted_rates(&out.program);
        assert!(!rates.is_empty(), "{what}: {}", out.program);
        for (xz, cr, line) in rates {
            assert!(xz <= fmax, "{what}: `{line}` drives X/Z at {xz} over {fmax}: {}", out.program);
            assert!(cr <= c_rate, "{what}: `{line}` drives C at {cr} over {c_rate}: {}", out.program);
        }
    }
}

#[test]
fn a_cap_that_floors_under_the_f_step_is_refused_by_name() {
    // M3's probe: a C rate of 0.05 deg/min caps a 90 degree turn to a
    // composite 0.05, which floors to F0 - a word the control reads as
    // no feed. Refused by name, never written.
    let out = live_post(&mill_turn(0.05, 4000.0), cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(50000.0)));
    assert!(
        out.warnings.iter().any(|w| w.contains("cap leaves under 0.1 mm/min")),
        "refused by name: {:?}\n{}",
        out.warnings,
        out.program
    );
    assert!(!out.program.lines().any(|l| l.starts_with("G01") && l.contains(" C")), "no G01 written: {}", out.program);
    assert!(!out.program.contains("F0\n") && !out.program.ends_with("F0"), "no F0: {}", out.program);
}

#[test]
fn an_uncapped_feed_under_the_f_step_is_refused_by_name() {
    // A planned feed whose composite word rounds to F0 with no cap near
    // it (the C rate is generous): refused by name as well.
    let out = live_post(&mill_turn(6000.0, 4000.0), cmv(MoveKind::Feed, 10.0, 90.0, 1.0, Some(planned_for(0.04, 10.0, 90.0))));
    assert!(
        out.warnings.iter().any(|w| w.contains("a C-axis feed under 0.1 mm/min")),
        "refused by name: {:?}\n{}",
        out.warnings,
        out.program
    );
    assert!(!out.program.lines().any(|l| l.starts_with("G01") && l.contains(" C")), "no G01 written: {}", out.program);
}

#[test]
fn an_off_lattice_stream_is_judged_on_its_posted_words() {
    // The engine reads stream values as real thousandths, so a stream
    // (a recorded or hand-made one) can carry C and Z between the words'
    // 0.001 steps. With a Z origin off that step too, each word the
    // post writes - C at 0.001, Z origin-relative at 0.001 - rounds
    // differently from the raw point: the move is judged on the words.
    // Raw Z 5.0007 -> 5.0033 under origin 0.0004 posts Z5 -> Z5.003
    // (world-rounded it would read Z5.001 -> Z5.003); raw C 30.0004 ->
    // 30.0096 posts C30 -> C30.01.
    for (c_rate, fmax, what) in [(1.0e6, 1000.0, "the maximum feed binds"), (100.0, 1.0e6, "the C rate binds")] {
        let r = vec![
            cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) },
            cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
            cnc::Record::Param { name: "OriginZ".into(), value: json!(0.0004) },
            cnc::Record::Param { name: "LiveTool".into(), value: json!("radial") },
            cnc::Record::Tool { number: 2, name: "EM".into() },
            cnc::Record::Section { begin: true },
            cmv(MoveKind::Rapid, 10.00074, 30.0, 5.0, None),
            cmv(MoveKind::Feed, 10.00376, 30.01, 5.0, Some(50000.0)),
            cnc::Record::Section { begin: false },
        ];
        let stream = cnc::serialize(&r);
        let rapid = "Rapid X 8661 Y -5000 Z 5000 A 0 B 0 C 30000\n";
        let feed = "Feed X 8663 Y -5003 Z 5000 F 50000000 A 0 B 0 C 30010\n";
        assert!(stream.contains(rapid) && stream.contains(feed), "the bench's stream lines: {stream}");
        let stream = stream
            .replace(rapid, "Rapid X 8661 Y -5000 Z 5000.7 A 0 B 0 C 30000.4\n")
            .replace(feed, "Feed X 8663 Y -5003 Z 5003.3 F 50000000 A 0 B 0 C 30009.6\n");
        let out = engine::run(LATHE_CPS, &mill_turn(c_rate, fmax), &stream).expect("the script runs");
        assert!(out.warnings.is_empty(), "{what}: {:?}", out.warnings);
        assert!(out.program.contains("X20.008 Z5.003 C30.01 F"), "{what}: the posted words: {}", out.program);
        let rates = posted_rates(&out.program);
        assert!(!rates.is_empty(), "{what}: {}", out.program);
        for (xz, cr, line) in rates {
            assert!(xz <= fmax, "{what}: `{line}` drives X/Z at {xz} over {fmax}: {}", out.program);
            assert!(cr <= c_rate, "{what}: `{line}` drives C at {cr} over {c_rate}: {}", out.program);
        }
    }
}

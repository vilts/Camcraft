//! #796: wrapped (simultaneous) A on the Tormach 770MX + microARC 4. The
//! PathPilot post writes an A that turns inside an operation from the
//! PathPilot Hub readings recorded on the issue (v2.14.4, 770MX SIM, 4th
//! axis on): A is linear degrees with no rollover, so A is posted
//! cumulative (179 -> 181, 359 -> 361); a move that turns A and moves X Y Z
//! is G93 inverse time, its time the longer of the tip's path at the
//! programmed feed and the turn at the machine's stated A rate; an A-only
//! move stays G94 in degrees per minute; G94 comes back before any other
//! motion and at the section's end; and what the post cannot hold (no
//! stated A rate, a count past the travel) is refused by name.
//! Provenance: our own tests over our own machine file and post; the
//! readings are mkinney's Hub runs on issue #796.
//!
//! Gate map (M4.4): covers post::post (tormach.cps wrapped A), post::machine
//! (rotary speeds mirror), app::posting (MACHINES registry).

use camcraft_app::cl::MoveKind;
use camcraft_app::machine::parse_mch;
use camcraft_app::post::{cnc, engine, machine};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ARC4: &str = "Tormach 770MX + microARC 4";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn mirrored() -> Value {
    let text = camcraft_app::posting::builtin_machine_text(ARC4).unwrap_or_else(|why| panic!("{ARC4}: {why}"));
    machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("{ARC4}: {e}")))
}

/// A metric document, so the expected words are plain mm and mm/min.
fn run(mch: &Value, stream: &str) -> engine::PostOutput {
    let cps = std::fs::read_to_string(repo("assets/posts/tormach.cps")).expect("the post");
    engine::run_with(&cps, mch, stream, &json!({ "Units": "mm" })).unwrap_or_else(|e| panic!("the engine runs: {e}"))
}

/// A point stated in the frame the tool stands up in at tool angle `a`
/// (the work offset is the world origin), carried to the world the
/// stream speaks: the part turned by +a about X - written by hand, so
/// the post is checked against an independent rotation.
fn world(a: f64, l: [f64; 3]) -> [f64; 3] {
    let (s, c) = a.to_radians().sin_cos();
    [l[0], l[1] * c - l[2] * s, l[1] * s + l[2] * c]
}

fn mv(kind: MoveKind, a: f64, l: [f64; 3], feed: Option<f64>) -> cnc::Record {
    let p = world(a, l);
    cnc::Record::Move(cnc::MoveRec {
        kind,
        x: p[0],
        y: p[1],
        z: p[2],
        feed,
        center: None,
        ccw: false,
        plane: Default::default(),
        abc: Some([a, 0.0, 0.0]),
    })
}

fn section(recs: &mut Vec<cnc::Record>, name: &str, moves: Vec<cnc::Record>) {
    recs.push(cnc::Record::Param { name: "OperationName".into(), value: json!(name) });
    recs.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(6000) });
    recs.push(cnc::Record::Param { name: "ClearancePlane".into(), value: json!(40.0) });
    recs.push(cnc::Record::Section { begin: true });
    recs.extend(moves);
    recs.push(cnc::Record::Section { begin: false });
}

fn head() -> Vec<cnc::Record> {
    vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(6000) },
        cnc::Record::Param { name: "ClearancePlane".into(), value: json!(40.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Tool { number: 3, name: "6MM BALL".into() },
    ]
}

/// Approach on the A axis at A0: the tip on the centerline, so no
/// turning move bows and every F word below is the time law alone.
fn approach() -> Vec<cnc::Record> {
    vec![
        mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 40.0], None),
        mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 5.0], None),
        mv(MoveKind::Plunge, 0.0, [0.0, 0.0, 0.0], Some(200.0)),
    ]
}

/// The pinned job: the feed-capped and the rate-capped G93 moves, an
/// XYZ-only feed (G94 back), A-only turns through 180 and 360 (G94,
/// deg/min), a rapid away; then an upright operation (A reset at safe Z).
fn wrap_stream(end_deg: f64) -> String {
    let mut recs = head();
    let mut w = approach();
    w.extend([
        // 50 mm of X while A turns 1 degree: the feed decides the time.
        mv(MoveKind::Feed, 1.0, [50.0, 0.0, 0.0], Some(600.0)),
        // 1 mm of X while A turns 90: the A rate (360 deg/min) decides.
        mv(MoveKind::Feed, 91.0, [51.0, 0.0, 0.0], Some(600.0)),
        // X alone: back to G94 at the programmed feed.
        mv(MoveKind::Feed, 91.0, [60.0, 0.0, 0.0], Some(600.0)),
        // A alone, on through 180 (the stream's -179 posts as 181) ...
        mv(MoveKind::Feed, 181.0, [60.0, 0.0, 0.0], Some(600.0)),
        mv(MoveKind::Feed, 271.0, [60.0, 0.0, 0.0], Some(600.0)),
        // ... and on through 360 (the stream's 1 posts as 361).
        mv(MoveKind::Feed, end_deg, [60.0, 0.0, 0.0], Some(600.0)),
        mv(MoveKind::Rapid, end_deg, [60.0, 0.0, 40.0], None),
    ]);
    section(&mut recs, "WRAP", w);
    section(
        &mut recs,
        "TOP",
        vec![
            mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 40.0], None),
            mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 5.0], None),
            mv(MoveKind::Plunge, 0.0, [0.0, 0.0, 0.0], Some(200.0)),
            mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 40.0], None),
        ],
    );
    cnc::serialize(&recs)
}

fn lines(nc: &str) -> Vec<&str> {
    nc.lines().collect()
}

fn after<'a>(nc: &'a str, first: &str) -> Vec<&'a str> {
    let l = lines(nc);
    let i = l.iter().position(|x| *x == first).unwrap_or_else(|| panic!("no line {first}\n{nc}"));
    l[i..].to_vec()
}

fn f_of(line: &str) -> f64 {
    line.split(' ')
        .find_map(|w| w.strip_prefix('F').and_then(|v| v.parse().ok()))
        .unwrap_or_else(|| panic!("no F on {line}"))
}

/// The G93 word the time law gives: the longer of (path + one rounding
/// step of slack) / feed and the turn / the A rate, floored to 0.001.
fn inverse_time(path_mm: f64, feed: f64, turn_deg: f64, rate: f64) -> f64 {
    let t = ((path_mm + 0.001 * 3f64.sqrt()) / feed).max(turn_deg / rate);
    (1.0 / t * 1000.0 + 1e-9).floor() / 1000.0
}

#[test]
fn the_microarc_states_its_a_rate() {
    let m = mirrored();
    let rate = m["kinematics"]["static"]["rotaries"][0]["speeds"]["normal"].as_f64();
    assert_eq!(rate, Some(360.0), "{m}");
}

#[test]
fn a_long_xyz_move_takes_its_time_from_the_feed() {
    let out = run(&mirrored(), &wrap_stream(361.0));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let want = inverse_time(50.0, 600.0, 1.0, 360.0);
    assert_eq!(want, 11.999);
    let tail = after(&out.program, "G1 X0 Y0 Z0 F200");
    assert_eq!(tail[1], "G93 G1 X50 Y0 Z0 A1 F11.999", "{}", out.program);
    // The tip moves 50 mm in 1 / 11.999 min: never faster than F600.
    assert!(50.0 * f_of(tail[1]) <= 600.0);
}

#[test]
fn a_mostly_rotary_move_takes_its_time_from_the_a_rate() {
    let out = run(&mirrored(), &wrap_stream(361.0));
    let tail = after(&out.program, "G93 G1 X50 Y0 Z0 A1 F11.999");
    // 90 degrees at 360 deg/min is 0.25 min: F4, still in G93 (no word).
    assert_eq!(inverse_time(1.0, 600.0, 90.0, 360.0), 4.0);
    assert_eq!(tail[1], "G1 X51 Y0 Z0 A91 F4", "{}", out.program);
}

#[test]
fn g94_comes_back_before_an_xyz_only_feed_and_a_turns_in_deg_per_min() {
    let out = run(&mirrored(), &wrap_stream(361.0));
    let tail = after(&out.program, "G1 X51 Y0 Z0 A91 F4");
    assert_eq!(
        &tail[1..6],
        ["G94", "G1 X60 Y0 Z0 F600", "G1 A181 F360", "G1 A271 F360", "G1 A361 F360"],
        "{}",
        out.program
    );
    assert_eq!(tail[6], "G0 X60 Y0 Z40", "{}", out.program);
    assert_eq!(out.program.matches("G93").count(), 1, "one G93 stretch\n{}", out.program);
}

#[test]
fn g94_comes_back_before_a_turning_rapid_after_a_g93_feed() {
    let mut recs = head();
    let mut moves = approach();
    moves.extend([
        mv(MoveKind::Feed, 30.0, [10.0, 0.0, 0.0], Some(600.0)),
        mv(MoveKind::Rapid, 60.0, [20.0, 0.0, 0.0], None),
    ]);
    section(&mut recs, "TURNING RAPID", moves);
    let out = run(&mirrored(), &cnc::serialize(&recs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let tail = after(&out.program, "G93 G1 X10 Y0 Z0 A30 F12");
    assert_eq!(&tail[1..3], ["G94", "G0 X20 Y0 Z0 A60"], "{}", out.program);

    // Read the modal state through the whole program: section-end G94
    // alone cannot repair a rapid that already ran in inverse-time mode.
    let mut g93 = false;
    for line in out.program.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.contains(&"G93") { g93 = true; }
        if words.contains(&"G94") { g93 = false; }
        if words.contains(&"G0") {
            assert!(!g93, "rapid in G93: {line}\n{}", out.program);
        }
    }
}

#[test]
fn a_counts_on_through_180_and_360_never_back_through_0() {
    let out = run(&mirrored(), &wrap_stream(361.0));
    let a: Vec<f64> = lines(&out.program)
        .iter()
        .flat_map(|l| l.split(' '))
        .filter_map(|w| w.strip_prefix('A').and_then(|v| v.parse().ok()))
        .collect();
    // The index to A0, the turning moves, then the next section's reset.
    assert_eq!(a, [0.0, 1.0, 91.0, 181.0, 271.0, 361.0, 0.0], "{}", out.program);
}

#[test]
fn the_next_operation_resets_a_at_the_safe_z() {
    let out = run(&mirrored(), &wrap_stream(361.0));
    let tail = after(&out.program, "(OPERATION: TOP - G54)");
    let i = tail.iter().position(|l| *l == "G0 A0").unwrap_or_else(|| panic!("{}", out.program));
    assert_eq!(tail[i - 1], "G53 G0 Z0", "{}", out.program);
}

#[test]
fn a_section_that_ends_in_g93_restates_g94_before_m5() {
    let mut recs = head();
    let mut w = approach();
    w.push(mv(MoveKind::Feed, 30.0, [10.0, 0.0, 0.0], Some(600.0)));
    section(&mut recs, "SHORT", w);
    let out = run(&mirrored(), &cnc::serialize(&recs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let tail = after(&out.program, "G1 X0 Y0 Z0 F200");
    assert!(tail[1].starts_with("G93 G1 X10 Y0 Z0 A30 F"), "{}", out.program);
    assert_eq!(tail[2], "G94", "{}", out.program);
    let m5 = tail.iter().position(|l| *l == "M5").unwrap_or_else(|| panic!("{}", out.program));
    assert!(m5 > 2, "{}", out.program);
}

/// A feed that turns A 90 degrees from 10 mm above the axis to the point
/// it turns to: the stream's move is the straight chord between the two,
/// but the machine moves Y Z in a line while A turns, so the post splits
/// it until every piece stays within 0.005 mm of the chord. Every posted
/// point, turned back into the part, lies on the chord, and every piece
/// is held to both the A rate and the feed.
#[test]
fn a_turning_move_off_the_axis_is_split_onto_its_chord() {
    let mut recs = head();
    let w = vec![
        mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 40.0], None),
        mv(MoveKind::Rapid, 0.0, [0.0, 0.0, 15.0], None),
        mv(MoveKind::Plunge, 0.0, [0.0, 0.0, 10.0], Some(200.0)),
        mv(MoveKind::Feed, 90.0, [0.0, 0.0, 10.0], Some(600.0)),
    ];
    section(&mut recs, "TURN", w);
    let out = run(&mirrored(), &cnc::serialize(&recs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let tail = after(&out.program, "G1 X0 Y0 Z10 F200");
    let turns: Vec<&str> = tail[1..].iter().copied().take_while(|l| l.contains(" A")).collect();
    assert!(turns.len() >= 16, "split: {turns:?}\n{}", out.program);
    assert!(turns[0].starts_with("G93 G1 "), "{}", out.program);
    assert!(turns.last().is_some_and(|l| l.contains(" A90 ")), "{}", out.program);
    let word = |l: &str, c: char| -> f64 {
        l.split(' ').find_map(|w| w.strip_prefix(c).and_then(|v| v.parse().ok())).unwrap_or_else(|| panic!("{c} on {l}"))
    };
    let (mut a0, mut p0) = (0.0_f64, [0.0_f64, 10.0_f64]);
    for l in &turns {
        let (y, z, a) = (word(l, 'Y'), word(l, 'Z'), word(l, 'A'));
        let (s, c) = a.to_radians().sin_cos();
        let p = [y * c - z * s, y * s + z * c];
        // The chord from (Y0 Z10) to (Y-10 Z0): y - z + 10 = 0.
        assert!((p[0] - p[1] + 10.0).abs() / 2f64.sqrt() <= 0.003, "off the chord: {l}");
        let minutes = 1.0 / f_of(l);
        let len = ((p[0] - p0[0]).powi(2) + (p[1] - p0[1]).powi(2)).sqrt();
        assert!(minutes >= (a - a0).abs() / 360.0 - 1e-9, "A rate passed: {l}");
        assert!(minutes >= len / 600.0 - 1e-9, "feed passed: {l}");
        (a0, p0) = (a, p);
    }
}

#[test]
fn shipped_microarc_posts_wrapped_a_past_four_turns() {
    let mut recs = head();
    let mut moves = approach();
    for k in 1..=16 {
        moves.push(mv(MoveKind::Feed, f64::from(k) * 90.0, [f64::from(k) * 5.0, 0.0, 0.0], Some(600.0)));
    }
    moves.push(mv(MoveKind::Feed, 1500.0, [85.0, 0.0, 0.0], Some(600.0)));
    moves.push(mv(MoveKind::Rapid, 1500.0, [85.0, 0.0, 40.0], None));
    section(&mut recs, "WRAP", moves);
    let out = run(&mirrored(), &cnc::serialize(&recs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("A1500"), "{}", out.program);
}

#[test]
fn a_count_past_the_travel_is_refused_by_name() {
    let mut bounded = mirrored();
    bounded["kinematics"]["static"]["rotaries"][0]["travel"] = json!([-360.0, 360.0]);
    let out = run(&bounded, &wrap_stream(361.0));
    assert!(
        out.warnings.iter().any(|w| w.contains(
            "A361 in operation WRAP is beyond the A axis travel -360..360 degrees - A counts on through 360 (PathPilot does not roll A over)"
        )),
        "{:?}",
        out.warnings
    );
    for (mch, deg) in [(&bounded, 300.0), (&mirrored(), 361.0)] {
        let ok = run(mch, &wrap_stream(deg));
        assert!(ok.warnings.is_empty(), "{deg}: {:?}", ok.warnings);
    }
}

#[test]
fn a_machine_with_no_a_rate_is_refused_by_name() {
    let mut m = mirrored();
    m["kinematics"]["static"]["rotaries"][0]["speeds"] = json!({});
    let out = run(&m, &wrap_stream(361.0));
    assert!(
        out.warnings.iter().any(|w| w.contains("simultaneous 4-axis motion in operation WRAP")
            && w.contains("states no A feed rate")),
        "{:?}",
        out.warnings
    );
}

/// Read the control's rounded words and independently sample its linear
/// XYZ/A interpolation in the part frame. Endpoint distance alone misses
/// the bow, even when both stream endpoints are the same world point.
#[test]
fn posted_trajectory_length_obeys_the_feed_even_for_coincident_world_endpoints() {
    let cps = std::fs::read_to_string(repo("assets/posts/tormach.cps")).expect("the post");
    for (units, unit_mm, places) in [("mm", 1.0, 1_000.0), ("inch", 25.4, 10_000.0)] {
        for (angle, radius) in [(3.0, 10.0), (-3.0, 10.0), (0.003, 10.0), (3.0, 0.0001)] {
            let feed = 0.1;
            let mut recs = head();
            let mut end = mv(MoveKind::Feed, angle, [0.0, 0.0, radius], Some(feed));
            if let cnc::Record::Move(m) = &mut end {
                m.x = 0.0;
                m.y = 0.0;
                m.z = radius;
            }
            section(&mut recs, "STATIONARY WORLD", vec![
                mv(MoveKind::Rapid, 0.0, [0.0, 0.0, radius], None),
                end,
            ]);
            let out = engine::run_with(&cps, &mirrored(), &cnc::serialize(&recs), &json!({"Units": units}))
                .expect("post the coincident endpoints");
            assert!(out.warnings.is_empty(), "{:?}", out.warnings);
            let mut xyz0 = [0.0, 0.0, (radius / unit_mm * places).round() / places * unit_mm];
            let mut a0 = 0.0_f64;
            let mut inverse = false;
            let mut checked = 0;
            for line in out.program.lines() {
                let words: Vec<_> = line.split_whitespace().collect();
                if words.contains(&"G93") { inverse = true; }
                if words.contains(&"G94") { inverse = false; }
                if !words.contains(&"G1") || !words.iter().any(|w| w.starts_with('A')) { continue; }
                let mut xyz1 = xyz0;
                for (i, axis) in ['X', 'Y', 'Z'].iter().enumerate() {
                    if let Some(v) = words.iter().find_map(|w| w.strip_prefix(*axis).and_then(|s| s.parse::<f64>().ok())) {
                        xyz1[i] = v * unit_mm;
                    }
                }
                let a1 = words.iter().find_map(|w| w.strip_prefix('A').and_then(|s| s.parse::<f64>().ok())).expect("A");
                let duration = if inverse { 1.0 / f_of(line) } else { (a1 - a0).abs() / f_of(line) };
                let mut previous = world(a0, xyz0);
                let mut length = 0.0;
                for j in 1..=10_000 {
                    let u = f64::from(j) / 10_000.0;
                    let machine = std::array::from_fn(|i| xyz0[i] + (xyz1[i] - xyz0[i]) * u);
                    let part = world(a0 + (a1 - a0) * u, machine);
                    length += part.iter().zip(previous).map(|(p, q)| (p - q).powi(2)).sum::<f64>().sqrt();
                    previous = part;
                }
                assert!(length / duration <= feed + 1e-9,
                    "{units}, A{angle}, radius {radius}: sampled {length} mm in {duration} min exceeds F{feed}: {line}");
                assert!((a1 - a0).abs() / duration <= 360.0 + 1e-9, "A rate: {line}");
                xyz0 = xyz1;
                a0 = a1;
                checked += 1;
            }
            assert!(checked > 0, "no turning feed: {}", out.program);
        }
    }
}

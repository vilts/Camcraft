//! The Fanuc LATHE post's refusals (M13.5), on synthetic streams: what
//! a 2-axis lathe cannot express is refused BY NAME through the
//! engine's counted `message()` path, once per reason, and never
//! written as a guess - a move off every meridian (milling work), a
//! posed A/B/C move, a drilling cycle off the spindle axis, a
//! back-bore, an XY arc that is no thread half-turn, a feed with no
//! spindle to pace G99 against. The one XY shape it DOES speak - the
//! threading kernel's half-turn helix - posts as G32 with F = lead.
//! A section stating no surface speed after one that did posts its
//! own G97, never the stale G96 (#772); a CSS section cancels its G96 at
//! its end, so a live-tool, thread or drill section opens under G97 (#1173).
//! A face cycle closes on the rapid leaving its hole, and a cut leaving
//! the hole inside an open group is refused (#1011).
//! The turning documents' goldens live in `lathe_post.rs`.
//!
//! Gate map (M4.4): covers post::post (engine, cnc on the lathe
//! dialect), app::machine, post::machine (the mirror the script reads).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::post::{cnc, engine};
use serde_json::json;

const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

fn lathe_mch() -> serde_json::Value {
    camcraft_app::post::machine::mirror(&Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    })
}


fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

fn preface() -> Vec<cnc::Record> {
    vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Tool { number: 1, name: "TN-1600-0R8".into() },
        cnc::Record::Section { begin: true },
    ]
}

fn run(records: Vec<cnc::Record>) -> engine::PostOutput {
    engine::run(LATHE_CPS, &lathe_mch(), &cnc::serialize(&records)).expect("the script runs")
}

#[test]
fn a_move_off_every_meridian_is_refused_by_name() {
    // A mill contour's corner: both X and Y off the axis - no single
    // meridian holds it, and the post must not project it onto one.
    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None));
    r.push(mv(MoveKind::Feed, 40.0, 30.0, -1.0, Some(800.0)));
    r.push(mv(MoveKind::Feed, 40.0, 31.0, -1.0, Some(800.0)));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert_eq!(out.warnings.len(), 1, "one named refusal per reason, not per line: {:?}", out.warnings);
    assert!(out.warnings[0].contains("milling work"), "{:?}", out.warnings);
    assert!(out.program.contains("G00 X12 Z10"), "the meridian rapid still posts as a diameter: {}", out.program);
    assert!(!out.program.contains("X80"), "the refused move wrote no guess: {}", out.program);
}

#[test]
fn a_posed_move_is_refused_by_name() {
    let mut r = preface();
    r.push(cnc::Record::Move(cnc::MoveRec {
        kind: MoveKind::Rapid, x: 0.0, y: 6.0, z: 10.0, feed: None, center: None, ccw: false,
        plane: Default::default(), abc: Some([30.0, 0.0, 0.0]),
    }));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("posed move")), "{:?}", out.warnings);
    assert!(!out.program.contains("A30"), "{}", out.program);
}

#[test]
fn a_cycle_off_the_axis_and_a_back_bore_are_refused_by_name() {
    let cycle = |kind: CycleKind, y: f64| {
        cnc::Record::Cycle(cnc::CycleRec {
            kind, x: 0.0, y, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 0.0, dwell: 0.0,
            first_move: 0, feed: Some(100.0),
        })
    };
    let mut r = preface();
    r.push(cycle(CycleKind::Drill, 5.0));
    r.push(mv(MoveKind::Rapid, 0.0, 5.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("off the spindle axis")), "{:?}", out.warnings);
    assert!(!out.program.contains("G83"), "{}", out.program);

    let mut r = preface();
    r.push(cycle(CycleKind::BackBore, 0.0));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("back-boring")), "{:?}", out.warnings);
}

#[test]
fn a_thread_helix_posts_as_g32_and_a_stray_xy_arc_refuses() {
    // The threading kernel's spelling: a plunge on the meridian, then
    // XY half-turns about the axis falling half a pitch each - one
    // lead of Z under G32 with F = lead, the X word modal. Clockwise
    // seen from +Z falling toward the chuck: right-hand under M03 (#629).
    let arc_ccw = |x: f64, y: f64, z: f64, ccw: bool| {
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x, y, z, feed: Some(100.0), center: Some((0.0, 0.0)), ccw,
            plane: Default::default(), abc: None,
        })
    };
    let arc = |x: f64, y: f64, z: f64| arc_ccw(x, y, z, false);
    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 10.0, None));
    r.push(mv(MoveKind::Plunge, 0.0, 2.5, 10.0, Some(100.0)));
    r.push(arc(0.0, -2.5, 9.0));
    r.push(arc(0.0, 2.5, 8.0));
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 8.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G01 X5 Z10 F0.1"), "the plunge in diameter and per-rev: {}", out.program);
    assert!(out.program.contains("G32 Z9 F2\nG32 Z8 F2\n"), "two half-turns, one lead: {}", out.program);
    assert!(out.program.contains("M03") && !out.program.contains("M04"), "{}", out.program);

    // #629: the same passes turning counter-clockwise under the M03 the
    // stream left standing would cut the other hand - refused by name,
    // never posted (every pre-#629 stream turned this way).
    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 10.0, None));
    r.push(mv(MoveKind::Plunge, 0.0, 2.5, 10.0, Some(100.0)));
    r.push(arc_ccw(0.0, -2.5, 9.0, true));
    r.push(arc_ccw(0.0, 2.5, 8.0, true));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("would cut the other hand")), "{:?}", out.warnings);
    assert!(!out.program.contains("G32"), "{}", out.program);

    // An XY arc that is not a half-turn about the axis is no thread.
    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 10.0, None));
    r.push(cnc::Record::Move(cnc::MoveRec {
        kind: MoveKind::Feed, x: 4.0, y: 0.0, z: 10.0, feed: Some(100.0), center: Some((0.0, 0.0)),
        ccw: false, plane: Default::default(), abc: None,
    }));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("not a thread half-turn")), "{:?}", out.warnings);
    assert!(!out.program.contains("G32") && !out.program.contains("G02"), "{}", out.program);
}

#[test]
fn a_feed_without_a_spindle_speed_is_refused_by_name() {
    let r = vec![
        cnc::Record::Tool { number: 1, name: "TN-1600-0R8".into() },
        cnc::Record::Section { begin: true },
        mv(MoveKind::Feed, 0.0, 2.0, 10.0, Some(100.0)),
        cnc::Record::Section { begin: false },
    ];
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("needs a spindle speed")), "{:?}", out.warnings);
}

#[test]
fn a_section_without_surface_speed_posts_g97_after_one_with_it() {
    // #772: a CSS turning section, then an axial drill that states no
    // SurfaceSpeed. The drill must run at its own rpm (G97), never at
    // the stale G96 - on centre that is the G50 clamp, the machine max.
    let mut r = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
    r.extend(preface());
    r.push(mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None));
    r.push(mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0)));
    r.push(cnc::Record::Section { begin: false });
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(500.0) });
    r.push(cnc::Record::Tool { number: 2, name: "DRILL-6".into() });
    r.push(cnc::Record::Section { begin: true });
    // #1026: the drill approaches its seat (on centre at R) before the
    // cycle is read, as the generator's stream does: moves 0-1 are the
    // turning section's, the approach is move 2, the chain starts at 3.
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Cycle(cnc::CycleRec {
        kind: CycleKind::Drill, x: 0.0, y: 0.0, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 0.0,
        dwell: 0.0, first_move: 3, feed: Some(100.0),
    }));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert_eq!(p.matches("G96").count(), 1, "only the turning section is CSS: {p}");
    assert!(p.contains("G96 S200 M03\n"), "{p}");
    let drill = &p[p.find("T0202").expect("the drill's tool change")..];
    assert!(drill.contains("G97 S500 M03\n"), "the drill states its own rpm: {drill}");
}

#[test]
fn a_face_cycle_ends_on_the_rapid_leaving_its_hole_and_refuses_a_cut_that_leaves() {
    // #1011: the chain on the hole (on centre, at or below R) is the
    // cycle's; the exit rapid up to clearance closes the group with G80
    // and posts as the stream says. A feed off the hole inside the open
    // group is no chain the cycle replaces: refused by name, never dropped.
    // #1026: the approach to the seat (move 0) comes before the cycle
    // is read; the chain it replaces starts at move 1.
    let cycle = cnc::Record::Cycle(cnc::CycleRec {
        kind: CycleKind::Drill, x: 0.0, y: 0.0, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 0.0,
        dwell: 0.0, first_move: 1, feed: Some(100.0),
    });
    let chain = |r: &mut Vec<cnc::Record>| {
        r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
        r.push(mv(MoveKind::Feed, 0.0, 0.0, 10.0, Some(100.0)));
        r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    };
    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cycle.clone());
    chain(&mut r);
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 40.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G83 Z10 F0.1\nG80\nG00 X0 Z40\n"), "the exit posts after G80: {}", out.program);
    assert_eq!(out.program.matches("G80").count(), 2, "the header's G80 and the group's, once: {}", out.program);

    let mut r = preface();
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cycle);
    chain(&mut r);
    r.push(mv(MoveKind::Feed, 0.0, 3.0, 10.0, Some(100.0)));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.iter().any(|w| w.contains("leaves its hole inside a face-cycle group")), "{:?}", out.warnings);
    assert!(!out.program.contains("X6"), "the refused cut wrote no guess: {}", out.program);
}

// ------------------------------------------------- #1173: G96 never leaks
// A section that posted G96 cancels it at its end (G97 after M05), so
// the next section - whatever its kind - opens under constant rpm unless
// it states G96 itself.

/// Test-bench live-tool words - placeholders, not any builder's codes.
fn mill_turn_mch() -> serde_json::Value {
    use camcraft_app::machine::{AxisLimits, LiveToolCodes};
    camcraft_app::post::machine::mirror(&Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(6000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                feed: Some(6000.0),
                rapid: Some(12000.0),
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
    })
}

/// A CSS turning section (200 m/min) on tool 1, closed.
fn css_turning() -> Vec<cnc::Record> {
    let mut r = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
    r.extend(preface());
    r.push(mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None));
    r.push(mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0)));
    r.push(cnc::Record::Section { begin: false });
    r
}

/// A live-tool (C) move: the part-frame tip and the spindle angle C.
fn cmv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec {
        kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: Some([0.0, 0.0, 0.0]),
    })
}

/// The program text from the first `from` on.
fn after<'a>(p: &'a str, from: &str) -> &'a str {
    &p[p.find(from).unwrap_or_else(|| panic!("{from} in the program: {p}"))..]
}

#[test]
fn a_live_tool_section_after_a_css_one_posts_g97_before_its_live_spindle() {
    // The review's stream (#1173): CSS turning, then a C-axis live-tool
    // section at 3000 rpm. Its S word must be read as rpm - G97 in force
    // before the live spindle starts, never the turning section's G96.
    let mut r = css_turning();
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) });
    r.push(cnc::Record::Param { name: "LiveTool".into(), value: json!("radial") });
    r.push(cnc::Record::Tool { number: 2, name: "EM".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(cmv(MoveKind::Rapid, 10.0, 5.0, None));
    r.push(cmv(MoveKind::Rapid, 10.0, 1.0, None));
    r.push(cmv(MoveKind::Feed, 8.0, 1.0, Some(200.0)));
    r.push(cmv(MoveKind::Rapid, 10.0, 5.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = engine::run(LATHE_CPS, &mill_turn_mch(), &cnc::serialize(&r)).expect("the script runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    let css = after(p, "G96 S200 M03\n");
    assert!(
        css.contains("M05\nG97\nG54\n(T0202 EM)\nT0202\nG98\nM1001\nM1003 S3000\n"),
        "main spindle stopped, CSS cancelled, then the live branch: {p}"
    );
    assert_eq!(p.matches("G96").count(), 1, "{p}");
    assert!(p.ends_with("M1004\nM1002\nG99\nG28 U0 W0\nM30\n%\n"), "a live section adds no G97 of its own: {p}");
}

#[test]
fn a_thread_after_a_css_section_opens_under_g97() {
    // A G32 thread synchronises Z to spindle revolutions: under a stale
    // G96 its rpm would follow the diameter. G97 stands before its S.
    let mut r = css_turning();
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(400.0) });
    r.push(cnc::Record::Tool { number: 3, name: "THREAD-60".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 10.0, None));
    r.push(mv(MoveKind::Plunge, 0.0, 2.5, 10.0, Some(100.0)));
    for (y, z) in [(-2.5, 9.0), (2.5, 8.0)] {
        r.push(cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 0.0, y, z, feed: Some(100.0), center: Some((0.0, 0.0)), ccw: false,
            plane: Default::default(), abc: None,
        }));
    }
    r.push(mv(MoveKind::Rapid, 0.0, 4.0, 8.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    let thread = after(p, "T0303");
    let first_s = thread.find(" S").expect("the thread's spindle word");
    assert!(thread[..first_s].ends_with("\nG97"), "G97 is the mode word before the thread's S: {thread}");
    assert!(after(p, "G96 S200 M03\n").contains("M05\nG97\nG54\n(T0303"), "the CSS section cancels at its end: {p}");
    assert!(thread.contains("G32 Z9 F2\n"), "the passes still post: {thread}");
}

#[test]
fn a_drill_after_a_css_section_finds_g97_already_in_force() {
    // The #772 twin, read for #1173: the CSS section's end states G97,
    // and the drill states it again with its own rpm.
    let mut r = css_turning();
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(500.0) });
    r.push(cnc::Record::Tool { number: 2, name: "DRILL-6".into() });
    r.push(cnc::Record::Section { begin: true });
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Cycle(cnc::CycleRec {
        kind: CycleKind::Drill, x: 0.0, y: 0.0, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 0.0,
        dwell: 0.0, first_move: 3, feed: Some(100.0),
    }));
    r.push(mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None));
    r.push(cnc::Record::Section { begin: false });
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert!(after(p, "G96 S200 M03\n").contains("M05\nG97\nG54\n(T0202"), "{p}");
    let drill = after(p, "T0202");
    let first_s = drill.find(" S").expect("the drill's spindle word");
    assert!(drill[..first_s].ends_with("\nG97"), "{drill}");
}

#[test]
fn two_css_sections_in_a_row_each_state_their_own_g96() {
    // The rule's price, stated: each CSS section cancels at its end, so
    // the second one re-states G96 after the first's G97 - one bare G97
    // line per CSS section, no other flip, and the program ends in G97.
    let mut r = css_turning();
    let mut second = css_turning();
    if let Some(cnc::Record::Tool { number, name }) = second.iter_mut().find(|x| matches!(x, cnc::Record::Tool { .. })) {
        *number = 2;
        *name = "TN-1600-0R4".into();
    }
    r.extend(second);
    let out = run(r);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert_eq!(p.matches("G96 S200 M03\n").count(), 2, "{p}");
    assert_eq!(p.matches("\nG97\n").count(), 2, "one cancel per CSS section: {p}");
    assert!(!p.contains("G97 S"), "no constant-rpm section, no G97 S: {p}");
    assert!(p.contains("M05\nG97\nG54\n(T0202 TN-1600-0R4)\nT0202\nG96 S200 M03\n"), "{p}");
    assert!(p.ends_with("M05\nG97\nG28 U0 W0\nM30\n%\n"), "{p}");
}

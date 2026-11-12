//! The staleness guard of the recorded post inputs (#926): every stream
//! under `recorded/streams/` that cl and the post's own dump can build is
//! rebuilt here and compared byte for byte, so a change to the dump, the
//! stream writer or the path builder fails this suite instead of leaving
//! the script suites posting a stream nothing writes any more. The two
//! streams motion generates (probe, drill_retract) are guarded on app's
//! side (`crates/app/tests/recorded_post_streams.rs`, the one crate that
//! sees both motion and post); the one hand-written stream
//! (`full_circle_raw`, a single 360-degree arc record the dump never
//! writes) is guarded by parsing and writing back unchanged.
//!
//! Also guarded: every shipped post script has its suite, and the stream
//! directory holds exactly the recorded cases.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test recorded_streams`
//!
//! Gate map (M4.4): covers camcraft_post::post::dump, camcraft_post::post::cnc,
//! camcraft_post::cl (PathBuilder, ThreadCycle).

mod recorded;

use camcraft_post::cl::{ArcPlane, MoveKind, PathBuilder, ThreadCycle, ThreadHand, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::{self, cnc, Css, Dump};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The resolved context every built case posts with: one mill op on
/// tool 1, G54, a 50 mm clearance plane.
fn ctx<'a>(op: &'a str, tool: u32, name: &'a str, rpm: f64) -> Dump<'a> {
    Dump {
        op_name: op,
        tool_number: tool,
        tool_name: name,
        tool_diameter: 6.0,
        rpm,
        clearance_z: 50.0,
        retract_z: 5.0,
        top_z: 0.0,
        wcs: "G54",
        stock: [-50.0, 50.0, -50.0, 50.0],
        ..Dump::default()
    }
}

fn stream(sections: &[(&Toolpath, Dump)]) -> String {
    let recs: Vec<cnc::Record> = sections.iter().flat_map(|(t, d)| post::dump(t, d)).collect();
    cnc::serialize(&recs)
}

/// Arcs in each plane and a helix: XY ccw and cw, XZ, YZ, then an XY
/// quarter that descends 2 mm.
fn arcs() -> String {
    let mut b = PathBuilder::new(p(10.0, 0.0, 15.0));
    b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
    b.push(MoveKind::Plunge, p(10.0, 0.0, 0.0), Some(300.0));
    b.push_arc(MoveKind::Feed, p(0.0, 10.0, 0.0), Some(800.0), Point2::new(0.0, 0.0), true);
    b.push_arc(MoveKind::Feed, p(10.0, 20.0, 0.0), Some(800.0), Point2::new(10.0, 10.0), false);
    b.push_arc_in(MoveKind::Feed, p(20.0, 20.0, -10.0), Some(600.0), Point2::new(20.0, 0.0), false, ArcPlane::Xz);
    b.push_arc_in(MoveKind::Feed, p(20.0, 30.0, 0.0), Some(600.0), Point2::new(30.0, -10.0), true, ArcPlane::Yz);
    b.push_arc(MoveKind::Feed, p(30.0, 40.0, -2.0), Some(500.0), Point2::new(30.0, 30.0), false);
    b.push(MoveKind::Rapid, p(30.0, 40.0, 15.0), None);
    let t = b.finish();
    stream(&[(&t, ctx("Arcs", 1, "FLAT-6", 8000.0))])
}

/// A full circle as the generators write it: a ring of two XY halves
/// (the builder drops a zero-sweep arc, so `fit_arcs` splits rings).
fn full_circle() -> String {
    let mut b = PathBuilder::new(p(10.0, 0.0, 15.0));
    b.push(MoveKind::Rapid, p(10.0, 0.0, 1.0), None);
    b.push(MoveKind::Plunge, p(10.0, 0.0, -1.0), Some(300.0));
    b.push_arc(MoveKind::Feed, p(-10.0, 0.0, -1.0), Some(800.0), Point2::new(0.0, 0.0), true);
    b.push_arc(MoveKind::Feed, p(10.0, 0.0, -1.0), Some(800.0), Point2::new(0.0, 0.0), true);
    b.push(MoveKind::Rapid, p(10.0, 0.0, 15.0), None);
    let t = b.finish();
    stream(&[(&t, ctx("Ring", 1, "FLAT-6", 8000.0))])
}

/// A tool change with a spindle direction change: a facing pass on tool
/// 1 (spindle forward), then a left-hand thread toward -Z on tool 2,
/// which only M04 lays - the dump states `SpindleDirection reverse`.
fn tool_change() -> String {
    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 15.0), None);
    b.push(MoveKind::Feed, p(0.0, 0.0, 15.0), Some(150.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 16.0), None);
    let face = b.finish();

    let hand = ThreadHand::Left;
    let ccw = hand.ccw_for(true);
    let start = p(6.05, 0.0, 14.0);
    let mut b = PathBuilder::new(p(6.05, 0.0, 16.0));
    b.push(MoveKind::Rapid, start, None);
    b.push(MoveKind::Feed, p(5.6, 0.0, 14.0), Some(1600.0));
    for k in 1..=6 {
        let x = if k % 2 == 1 { -5.6 } else { 5.6 };
        b.push_arc(MoveKind::Feed, p(x, 0.0, 14.0 - f64::from(k)), Some(1600.0), Point2::new(0.0, 0.0), ccw);
    }
    b.push(MoveKind::Feed, p(6.05, 0.0, 8.0), Some(1600.0));
    b.push(MoveKind::Rapid, start, None);
    let mut thread = b.finish();
    let last = thread.moves.len() - 1;
    thread.threads.push(ThreadCycle {
        start,
        end: p(5.6, 0.0, 8.0),
        major_diameter: 12.0,
        minor_diameter: 11.2,
        lead: 2.0,
        height: 0.4,
        first_depth: 0.4,
        min_depth: 0.1,
        finish_allowance: 0.0,
        infeed_angle: 0.0,
        spring_passes: 0,
        pull_out: 0.0,
        first: 0,
        last,
        hand,
    });
    assert!(thread.threads[0].is_sane(&thread.moves), "the thread record sits on its passes");
    assert!(thread.threads[0].reverse_spindle(), "a left-hand thread toward -Z needs M04");
    let mut d2 = ctx("Thread", 2, "THREAD-60", 800.0);
    d2.tool_diameter = 0.0;
    stream(&[(&face, ctx("Face", 1, "FACE-12", 1200.0)), (&thread, d2)])
}

/// A rotary index and a tilted move: the head indexes to a fixed lean
/// and cuts there (3+2), then a move that changes the lean while it
/// cuts (simultaneous), then the commanded return upright.
fn rotary() -> String {
    let s = 0.5_f64;
    let c = 0.75_f64.sqrt();
    let mut b = PathBuilder::new(p(0.0, 0.0, 50.0));
    b.push(MoveKind::Rapid, p(0.0, 0.0, 20.0), None);
    b.set_axis(Some([0.0, -s, c]));
    b.push(MoveKind::Rapid, p(0.0, 0.0, 20.0), None);
    b.push(MoveKind::Rapid, p(0.0, 0.0, 5.0), None);
    b.push(MoveKind::Feed, p(0.0, 0.0, 0.0), Some(300.0));
    b.push(MoveKind::Feed, p(20.0, 0.0, 0.0), Some(800.0));
    b.set_axis(Some([s, 0.0, c]));
    b.push(MoveKind::Feed, p(20.0, 20.0, 0.0), Some(800.0));
    b.set_axis(Some([0.0, 0.0, 1.0]));
    b.push(MoveKind::Rapid, p(20.0, 20.0, 50.0), None);
    let t = b.finish();
    stream(&[(&t, ctx("Index", 1, "BALL-6", 8000.0))])
}

/// An operation whose section holds no motion at all.
fn empty_section() -> String {
    stream(&[(&Toolpath::new(Vec::new()), ctx("Nothing", 1, "FLAT-6", 8000.0))])
}

/// Coordinates at the lattice's floor (1 um moves, a 2 um arc) and far
/// out (+-9999.999, 99999.999 mm), with the slowest and a fast feed.
fn extreme_coords() -> String {
    let mut b = PathBuilder::new(p(0.0, 0.0, 10.0));
    b.push(MoveKind::Rapid, p(0.001, -0.001, 1.0), None);
    b.push(MoveKind::Plunge, p(0.001, -0.001, -0.001), Some(1.0));
    b.push(MoveKind::Feed, p(0.002, -0.001, -0.001), Some(1.0));
    b.push_arc(MoveKind::Feed, p(0.0, 0.001, -0.001), Some(1.0), Point2::new(0.0, -0.001), true);
    b.push(MoveKind::Rapid, p(0.0, 0.001, 10.0), None);
    b.push(MoveKind::Rapid, p(9999.999, -9999.999, 10.0), None);
    b.push(MoveKind::Feed, p(99999.999, -9999.999, -0.5), Some(99999.0));
    b.push(MoveKind::Rapid, p(99999.999, -9999.999, 10.0), None);
    let t = b.finish();
    stream(&[(&t, ctx("Extremes", 1, "FLAT-6", 8000.0))])
}

/// #1173: a constant-surface-speed facing pass (200 m/min, the tool's own
/// 3000 rpm clamp), then a C-axis live-tool section at 3000 rpm - a
/// radial plunge at C0 and one at C-90 (the tip on +Y turned onto the
/// station). The live tool's S must be read as rpm: on the lathe post
/// the CSS section ends in G97 before the live branch starts.
fn css_then_live() -> String {
    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 15.0), None);
    b.push(MoveKind::Feed, p(0.0, 0.0, 15.0), Some(150.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 16.0), None);
    let face = b.finish();
    let mut turn = ctx("Face CSS", 1, "FACE-12", 1200.0);
    turn.tool_diameter = 0.0;
    turn.css = Some(Css { vc: 200.0, max_rpm: Some(3000.0) });

    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    b.push(MoveKind::Feed, p(8.0, 0.0, 5.0), Some(200.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    b.push(MoveKind::Rapid, p(0.0, 12.0, 5.0), None);
    b.push(MoveKind::Feed, p(0.0, 8.0, 5.0), Some(200.0));
    b.push(MoveKind::Rapid, p(0.0, 12.0, 5.0), None);
    let mut live = b.finish();
    // The spindle angle each move states: C0 for the tip on +X, C-90 for
    // the tip on +Y (Rz(-90) turns +Y onto the +X station).
    for m in &mut live.moves {
        m.rotary = Some(if m.to.y.abs() > 1e-9 { -90.0 } else { 0.0 });
    }
    stream(&[(&face, turn), (&live, ctx("Cross mill", 2, "EM-6", 3000.0))])
}

/// A constant-rpm turning section, a C-axis live-tool cross drill asking
/// an optional stop after it, then a second turning section (#1188): the
/// stop belongs at the end of the live section, never carried into the
/// next one.
fn live_then_stop() -> String {
    let face = |z: f64| {
        let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
        b.push(MoveKind::Rapid, p(12.0, 0.0, z), None);
        b.push(MoveKind::Feed, p(0.0, 0.0, z), Some(150.0));
        b.push(MoveKind::Rapid, p(12.0, 0.0, 16.0), None);
        b.finish()
    };
    let (one, two) = (face(15.0), face(14.0));
    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    b.push(MoveKind::Feed, p(8.0, 0.0, 5.0), Some(200.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    let mut live = b.finish();
    for m in &mut live.moves {
        m.rotary = Some(0.0);
    }
    let mut turn1 = ctx("Face one", 1, "FACE-12", 1200.0);
    turn1.tool_diameter = 0.0;
    let mut turn2 = ctx("Face two", 3, "FACE-8", 1500.0);
    turn2.tool_diameter = 0.0;
    let mut drill = ctx("Cross drill", 2, "DR-6", 3000.0);
    drill.after = post::AfterOperation::OptionalStop;
    stream(&[(&one, turn1), (&live, drill), (&two, turn2)])
}

/// Compare a built stream with its recorded file (or record it).
fn guard(case: &str, text: &str) {
    let path = recorded::stream_path(case);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, text).expect("write the recorded stream");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        want.replace("\r\n", "\n"),
        text,
        "recorded stream {} is stale: its generator writes something else now - regenerate with WRITE_GOLDEN=1, then re-read every script golden it feeds",
        path.display()
    );
}

#[test]
fn arcs_stream_is_current() {
    guard("arcs", &arcs());
}

#[test]
fn full_circle_stream_is_current() {
    guard("full_circle", &full_circle());
}

#[test]
fn tool_change_stream_is_current() {
    guard("tool_change", &tool_change());
}

#[test]
fn rotary_stream_is_current() {
    guard("rotary", &rotary());
}

#[test]
fn empty_section_stream_is_current() {
    guard("empty_section", &empty_section());
}

#[test]
fn extreme_coords_stream_is_current() {
    guard("extreme_coords", &extreme_coords());
}

#[test]
fn css_then_live_stream_is_current() {
    guard("css_then_live", &css_then_live());
}

#[test]
fn live_then_stop_stream_is_current() {
    guard("live_then_stop", &live_then_stop());
}

/// The hand-written stream: no generator writes it, so its guard is that
/// it parses and the writer gives back exactly the recorded text, and
/// that it holds the one record it exists for - an arc ending where it
/// began.
#[test]
fn full_circle_raw_stream_parses_and_round_trips() {
    let path = recorded::stream_path("full_circle_raw");
    let text = std::fs::read_to_string(&path).expect("the hand-written stream").replace("\r\n", "\n");
    let recs = cnc::parse(&text).expect("the hand-written stream parses");
    assert_eq!(cnc::serialize(&recs), text, "the hand-written stream is in the writer's canonical form");
    let moves: Vec<&cnc::MoveRec> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) => Some(m),
            _ => None,
        })
        .collect();
    let full = moves.windows(2).any(|w| w[1].center.is_some() && (w[0].x, w[0].y, w[0].z) == (w[1].x, w[1].y, w[1].z));
    assert!(full, "the stream carries a 360-degree arc record");
}

#[test]
fn the_stream_dir_holds_exactly_the_recorded_cases() {
    let mut got: Vec<String> = std::fs::read_dir(recorded::here().join("streams"))
        .expect("the streams dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    got.sort();
    let mut want: Vec<String> = recorded::CASES.iter().map(|c| format!("{c}.cnc")).collect();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn every_shipped_post_script_has_its_suite() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for e in std::fs::read_dir(recorded::repo().join("assets/posts")).expect("assets/posts").filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".cps") else { continue };
        let suite = tests.join(format!("script_{}.rs", stem.replace('-', "_")));
        let src = std::fs::read_to_string(&suite).unwrap_or_else(|_| panic!("{name} has no suite {}", suite.display()));
        assert!(src.contains(&format!("post_suite!(\"{name}\")")), "{} runs another script", suite.display());
    }
}

#[test]
fn a_known_post_bug_has_no_golden() {
    for (script, case, issue) in recorded::KNOWN_BUGS {
        let dir = recorded::here().join("golden").join(script.trim_end_matches(".cps"));
        for ext in ["nc", "refusal"] {
            let g = dir.join(format!("{case}.{ext}"));
            assert!(!g.exists(), "{} pins known post bug #{issue}: wrong output is never a golden", g.display());
        }
    }
}

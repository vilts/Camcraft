//! #49: the Heidenhain DIN/ISO post's XY-arc centre words mean what the
//! control reads. Provenance: HEIDENHAIN "TNC 640 User's Manual, ISO
//! programming", NC software 340590-07/340591-07/340595-07, ID
//! 892909-25, English, 9/2016 (content.heidenhain.de) - p. 260 "Circle
//! center I, J": I/J are the Cartesian coordinates of the circle centre
//! in the working plane, modal until a new centre is programmed, and
//! relative to the last programmed position only when entered
//! incrementally (G91 before the word, p. 128); p. 261: the centre is
//! entered before the G02/G03 block, the last programmed position is
//! the arc's start, and start and end must lie on the circle (input
//! tolerance 0.016 mm). These tests read the posted program exactly so
//! and rebuild every arc's centre from the words.
//!
//! Gate map (M4.4): covers post::post (assets/posts/heidenhain.cps).

use camcraft_app::cl::MoveKind;
use camcraft_app::post::{cnc, engine};
use serde_json::json;
use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// One XY arc as the CL stream states it (stream frame, absolute).
struct Arc {
    end: (f64, f64),
    centre: (f64, f64),
    ccw: bool,
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, center: Option<(f64, f64)>, ccw: bool) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec {
        kind,
        x,
        y,
        z,
        feed: (kind != MoveKind::Rapid).then_some(500.0),
        center,
        ccw,
        plane: Default::default(),
        abc: None,
    })
}

/// A program whose arcs sit far from the origin (absolute and
/// incremental centre words differ by ~100 mm), CW then CCW, under a
/// work-origin shift `org`.
fn program(org: (f64, f64)) -> (String, Vec<Arc>) {
    use cnc::Record as R;
    let arcs = vec![
        // CW half circle about (120, -75), r 10: (110,-75) -> (130,-75).
        Arc { end: (130.0, -75.0), centre: (120.0, -75.0), ccw: false },
        // CCW quarter about (130, -60), r 15: (130,-75) -> (145,-60).
        Arc { end: (145.0, -60.0), centre: (130.0, -60.0), ccw: true },
        // CW quarter about (-80, 95), r 7 after a line to (-87, 95).
        Arc { end: (-80.0, 102.0), centre: (-80.0, 95.0), ccw: false },
    ];
    let mut recs = vec![
        R::Param { name: "SpindleSpeed".into(), value: json!(9000) },
        R::Param { name: "OriginX".into(), value: json!(org.0) },
        R::Param { name: "OriginY".into(), value: json!(org.1) },
        R::Tool { number: 1, name: "6MM END MILL".into() },
        R::Section { begin: true },
        mv(MoveKind::Rapid, 110.0, -75.0, 5.0, None, false),
        mv(MoveKind::Feed, 110.0, -75.0, -1.0, None, false),
    ];
    recs.push(mv(MoveKind::Feed, 130.0, -75.0, -1.0, Some(arcs[0].centre), false));
    recs.push(mv(MoveKind::Feed, 145.0, -60.0, -1.0, Some(arcs[1].centre), true));
    recs.push(mv(MoveKind::Feed, -87.0, 95.0, -1.0, None, false));
    recs.push(mv(MoveKind::Feed, -80.0, 102.0, -1.0, Some(arcs[2].centre), false));
    recs.push(mv(MoveKind::Rapid, -80.0, 102.0, 5.0, None, false));
    recs.push(R::Section { begin: false });
    (cnc::serialize(&recs), arcs)
}

fn post(stream: &str) -> String {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let out = engine::run(&read("assets/posts/heidenhain.cps"), &mch, stream)
        .unwrap_or_else(|e| panic!("heidenhain must post: {e}"));
    assert!(out.warnings.is_empty(), "the gate counts warnings: {:?}", out.warnings);
    out.program
}

/// A posted arc as the manual reads it: start = last programmed
/// position, centre = the modal I/J (absolute; this post never writes
/// G91), end and sense from the G2/G3 block. Program frame.
struct Read {
    start: (f64, f64),
    centre: (f64, f64),
    end: (f64, f64),
    ccw: bool,
}

fn read_arcs(nc: &str) -> Vec<Read> {
    assert!(!nc.contains("G91"), "this post writes no incremental word:\n{nc}");
    let (mut pos, mut cc) = ((f64::NAN, f64::NAN), None::<(f64, f64)>);
    let mut out = Vec::new();
    // #50: every block is `N<n> <words>*` (manual p. 130); the program
    // begin/end blocks (`%..`, `N99999999 %..`) carry no motion.
    for line in nc.lines() {
        let Some(body) = line.strip_prefix('N').and_then(|l| l.split_once(' ')).and_then(|(_, b)| b.strip_suffix('*')) else {
            continue;
        };
        let words: Vec<&str> = body.split(' ').collect();
        let get = |k: char| {
            words.iter().find(|w| w.starts_with(k)).and_then(|w| w[1..].parse::<f64>().ok())
        };
        let (i, j) = (get('I'), get('J'));
        if i.is_some() || j.is_some() {
            let (i, j) = (i.expect("I with J"), j.expect("J with I"));
            assert!(!words.iter().any(|w| w.starts_with('G')), "a centre block moves nothing: {line}");
            cc = Some((i, j));
            continue;
        }
        let g = words.first().copied().unwrap_or("");
        let end = (get('X').unwrap_or(pos.0), get('Y').unwrap_or(pos.1));
        if g == "G02" || g == "G03" {
            let centre = cc.unwrap_or_else(|| panic!("an arc before any centre: {line}"));
            out.push(Read { start: pos, centre, end, ccw: g == "G03" });
        }
        if matches!(g, "G00" | "G01" | "G02" | "G03") {
            pos = end;
        }
    }
    out
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn check(org: (f64, f64)) {
    let (stream, arcs) = program(org);
    let nc = post(&stream);
    let got = read_arcs(&nc);
    assert_eq!(got.len(), arcs.len(), "every arc posts as an arc:\n{nc}");
    // Three decimals: half a micron of rounding per word.
    let tol = 0.0005 * 2f64.sqrt() + 1e-9;
    for (k, (r, a)) in got.iter().zip(&arcs).enumerate() {
        let want = (a.centre.0 - org.0, a.centre.1 - org.1);
        assert!(dist(r.centre, want) <= tol, "arc {k}: centre {:?}, CL says {want:?}\n{nc}", r.centre);
        let end = (a.end.0 - org.0, a.end.1 - org.1);
        assert!(dist(r.end, end) <= tol, "arc {k}: end {:?} vs {end:?}", r.end);
        assert_eq!(r.ccw, a.ccw, "arc {k}: the sense");
        // The manual's radius law: start and end on one circle.
        let (ra, rb) = (dist(r.start, r.centre), dist(r.end, r.centre));
        assert!((ra - rb).abs() <= 2.0 * tol, "arc {k}: radii {ra} vs {rb}");
        // Read as Fanuc increments (centre - start) it would sit
        // elsewhere: the test tells the two meanings apart.
        let inc = (r.start.0 + r.centre.0, r.start.1 + r.centre.1);
        assert!(dist(inc, want) > 1.0, "arc {k}: absolute and incremental must differ here");
    }
}

#[test]
fn heidenhain_arc_centre_words_are_the_absolute_centre() {
    check((0.0, 0.0));
}

#[test]
fn heidenhain_arc_centre_shifts_with_the_work_origin() {
    check((10.0, -20.0));
}

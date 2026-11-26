//! #3474 (review of PR #3500): the same-angle arc through the stream
//! writer. The app posts through `cnc::serialize`, which writes whole
//! micrometres. A mm post with three decimals then writes the stream's own
//! numbers, and a record whose rounded ends are on one ray from its rounded
//! centre has no sweep that a post can read. Only the writer knows the true
//! sweep, so the writer gives the short arc as a `Feed`. Each test here
//! starts from true (unrounded) geometry and goes through `serialize`.
//!
//! Provenance: our own invariants; the full-circle reading of an arc whose
//! end is at its start (angle) is RS-274 / ISO 6983 circular
//! interpolation. The arc reader is the #3442 one (our own code).
//!
//! Gate map (M4.4): covers assets/posts/carvera.cps,
//! assets/posts/doosan-dnm.cps, assets/posts/fanuc.cps,
//! assets/posts/fanuc-lathe.cps,
//! assets/posts/grbl.cps, assets/posts/grblhal.cps, assets/posts/haas.cps,
//! assets/posts/heidenhain.cps, assets/posts/heidenhain-klartext.cps,
//! assets/posts/linuxcnc.cps, assets/posts/mach3.cps, assets/posts/mach4.cps,
//! assets/posts/mazak.cps, assets/posts/sinumerik.cps,
//! assets/posts/tinyg.cps, assets/posts/tormach.cps,
//! assets/posts/tormach-lathe.cps,
//! camcraft_post::post::cnc, camcraft_post::post::cnc_arc_lattice,
//! camcraft_post::post::arc_guard, camcraft_post::post::engine.

use camcraft_post::cl::{ArcPlane, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::{dump, engine, Dump};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[path = "coincident_arcs/reader.rs"]
#[allow(dead_code)]
mod reader;
use reader::arc_blocks;

/// Every shipped mill post that writes XY arcs from a mill stream.
const MILLS: &[&str] = &[
    "carvera",
    "doosan-dnm",
    "fanuc",
    "grbl",
    "grblhal",
    "haas",
    "heidenhain",
    "heidenhain-klartext",
    "linuxcnc",
    "mach3",
    "mach4",
    "mazak",
    "sinumerik",
    "tinyg",
    "tormach",
];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    let text = std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    text.lines().map(|l| format!("{l}\n")).collect()
}

fn machine(stem: &str) -> Value {
    if stem == "heidenhain-klartext" {
        let rel = "assets/machines/fehlmann-picomax55.mch";
        let parsed = camcraft_post::machine::parse_mch(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        return camcraft_post::post::machine::mirror(&parsed);
    }
    json!({"name": "ARC MILL", "spindleMax": 12000, "safeRetractZ": 0})
}

fn post(stem: &str, stream: &str, unit: &str) -> String {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    match engine::run_with(&cps, &machine(stem), stream, &json!({ "Units": unit })) {
        Ok(out) => format!("{}--- warnings: {:?}", out.program, out.warnings),
        Err(e) => format!("--- error: {e}"),
    }
}

fn units(stem: &str) -> Vec<&'static str> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let line = cps.lines().find(|l| l.starts_with("programUnits")).unwrap_or("");
    if line.split('"').nth(1).unwrap_or("").contains("inch") { vec!["mm", "inch"] } else { vec!["mm"] }
}

/// The arc of #3474 as the generator states it (mm, not rounded): centre
/// X10 Y5, radius 10.51 um, a sweep of 0.047 rad from X10.01051 Y5 to
/// X10.010498 Y5.000494. On the lattice: start X10.011 Y5, end X10.010 Y5,
/// the same angle about X10 Y5. `ccw` false is the same end points the
/// long way round (a turn 0.047 rad short of a circle).
const START: (f64, f64) = (10.01051, 5.0);
const END: (f64, f64) = (10.010498, 5.000494);
const CENTRE: (f64, f64) = (10.0, 5.0);

/// A plunge at the start, the arc (or, for the reference, the straight
/// feed to its end) at Z 5.5, and a retract.
fn path(arc: bool, ccw: bool) -> Toolpath {
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut b = PathBuilder::new(p(START, 18.0));
    b.push(MoveKind::Plunge, p(START, 5.5), Some(45.0));
    if arc {
        b.push_arc(MoveKind::Feed, p(END, 5.5), Some(180.0), Point2::new(CENTRE.0, CENTRE.1), ccw);
    } else {
        b.push(MoveKind::Feed, p(END, 5.5), Some(180.0));
    }
    b.push(MoveKind::Rapid, p(END, 18.0), None);
    b.finish()
}

/// The stream that the app would post: `dump` then `cnc::serialize`.
fn stream(t: &Toolpath) -> String {
    let d = Dump {
        op_name: "Same angle",
        tool_number: 1,
        tool_name: "EM-0400-2FL",
        tool_kind: "Flat end mill",
        tool_diameter: 4.0,
        rpm: 6000.0,
        clearance_z: 18.0,
        retract_z: 18.0,
        top_z: 6.0,
        stock: [-20.0, 40.0, -20.0, 40.0],
        stock_top_z: 6.0,
        stock_bottom_z: Some(-10.0),
        ..Dump::default()
    };
    cnc::serialize(&dump(t, &d))
}

/// The pin that the review named: the arc of #3474 as a `Toolpath`,
/// through `cnc::serialize`, posts on Fanuc in mm and in inch as one
/// straight move. Before, mm wrote `G03 X10.01 Y5. I-0.011 J0.`.
#[test]
fn the_arc_of_3474_through_the_stream_writer_posts_as_a_straight_move_on_fanuc() {
    let (arc, line) = (stream(&path(true, true)), stream(&path(false, true)));
    assert!(arc.lines().any(|l| l == "Feed X 10010 Y 5000 Z 5500 F 180000"), "the writer gives the feed: {arc}");
    assert!(!arc.lines().any(|l| l.starts_with("Arc ")), "an arc record: {arc}");
    for (unit, end) in [("mm", "X10.01 Y5."), ("inch", "X0.3941 Y0.1969")] {
        let got = post("fanuc", &arc, unit);
        assert!(arc_blocks(&got, false).is_empty(), "fanuc {unit}: an arc block: {got}");
        assert!(got.ends_with("warnings: []"), "fanuc {unit}: {got}");
        assert!(got.lines().any(|l| l.contains("G01") && l.contains(end)), "fanuc {unit}: no G01 to {end}: {got}");
        assert_eq!(got, post("fanuc", &line, unit), "fanuc {unit}: not the straight move");
    }
}

/// Every shipped mill post, in each unit that it offers: the arc of #3474
/// through the writer posts exactly as the straight feed to its end.
#[test]
fn every_mill_post_writes_the_arc_of_3474_through_the_stream_writer_as_its_straight_move() {
    let (arc, line) = (stream(&path(true, true)), stream(&path(false, true)));
    let mut failures = Vec::new();
    let mut checked = 0;
    for &stem in MILLS {
        for unit in units(stem) {
            let (got, want) = (post(stem, &arc, unit), post(stem, &line, unit));
            checked += 1;
            if !arc_blocks(&got, false).is_empty() {
                failures.push(format!("{stem} {unit}: an arc block: {got}"));
            } else if got != want || got.starts_with("--- error") {
                failures.push(format!("{stem} {unit}: not the straight move; got: {got} want: {want}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 24, "only {checked} post and unit pairs ran");
}

/// The same end points the long way round are a turn of nearly a full
/// circle: the writer keeps the arc record, and Fanuc posts an arc block.
#[test]
fn the_long_way_round_keeps_its_arc_through_the_stream_writer() {
    let long = stream(&path(true, false));
    assert!(long.lines().any(|l| l == "Arc CW X 10010 Y 5000 Z 5500 F 180000 I 10000 J 5000"), "{long}");
    let got = post("fanuc", &long, "mm");
    assert_eq!(arc_blocks(&got, false).len(), 1, "one arc block: {got}");
}

/// The two lathes (mm, diameter X): a meridian arc about X24 (radius 12)
/// Z-1, radius 10.51 um, a CW sweep of 0.047 rad. On the lattice the ends
/// are radius 12.011 and 12.010 at Z-1: one ray. The writer gives the
/// feed, and each lathe posts the straight move to X24.02.
#[test]
fn the_lathes_post_a_same_angle_meridian_arc_through_the_stream_writer_as_a_straight_move() {
    let rec = |center, x: f64, z: f64| {
        Record::Move(MoveRec { kind: MoveKind::Feed, x, y: 0.0, z, feed: Some(300.0), center, ccw: false, plane: ArcPlane::Xz, abc: None })
    };
    let written = cnc::serialize(&[rec(None, 12.01051, -1.0), rec(Some((12.0, -1.0)), 12.010498, -0.999506)]);
    let cut: Vec<&str> = written.lines().skip(1).collect();
    assert_eq!(cut, ["Feed X 12011 Y 0 Z -1000 F 300000", "Feed X 12010 Y 0 Z -1000 F 300000"]);
    let mut lines: Vec<&str> = vec![
        "; HSMW-CNC v1",
        "PARAM StockXYLeft -20",
        "PARAM StockXYRight 20",
        "PARAM StockXYFront -20",
        "PARAM StockXYBack 20",
        "PARAM StockZTop 0",
        "PARAM StockTurnRadius 20",
        "PARAM ClearancePlane 5",
        "PARAM SpindleSpeed 1500",
        "PARAM WorkOffset 1",
        "TOOL 1 TURN",
        "SECTION",
        "Rapid X 22000 Y 0 Z 2000",
    ];
    lines.extend(cut);
    lines.extend(["Rapid X 22000 Y 0 Z 2000", "ENDSECTION"]);
    let stream: String = lines.iter().map(|l| format!("{l}\n")).collect();
    for stem in ["fanuc-lathe", "tormach-lathe"] {
        let lathe = json!({"name": "ARC LATHE", "spindleMax": 4000});
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let out = engine::run_with(&cps, &lathe, &stream, &json!({ "Units": "mm" })).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(arc_blocks(&out.program, true).is_empty(), "{stem}: an arc block: {}", out.program);
        assert!(
            out.program.lines().any(|l| l.starts_with("G01 X24.02 ") || l.starts_with("G1 X24.02 ")),
            "{stem}: no feed move to X24.02: {}",
            out.program
        );
    }
}

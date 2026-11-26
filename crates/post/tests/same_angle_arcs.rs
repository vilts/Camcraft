//! #3474: a short arc whose written start and written end differ only on
//! the radial line through the written centre (the same angle about it)
//! is no arc block. A control that reads an arc by the angles of its ends
//! sees no sweep there, and an end at the start angle reads as a full turn
//! (a spiral of nearly one turn). The record posts as the straight move to
//! its end, which stays within the output step of the programmed arc.
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
//! assets/posts/mazak.cps, assets/posts/richauto.cps,
//! assets/posts/sinumerik.cps,
//! assets/posts/tinyg.cps, assets/posts/tormach.cps,
//! assets/posts/tormach-lathe.cps,
//! camcraft_post::post::arc_guard, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[path = "coincident_arcs/reader.rs"]
#[allow(dead_code)]
mod reader;
use reader::arc_blocks;

/// Every shipped mill post (the posts that write XY arcs from a mill stream).
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
    "mach4",
    "mazak",
    "richauto",
    "sinumerik",
    "tinyg",
    "tormach",
];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
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
        Ok(out) => format!("{}--- warnings: {:?}\n", out.program, out.warnings),
        Err(e) => format!("--- error: {e}\n"),
    }
}

fn units(stem: &str) -> Vec<&'static str> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let line = cps.lines().find(|l| l.starts_with("programUnits")).unwrap_or("");
    if line.split('"').nth(1).unwrap_or("").contains("inch") { vec!["mm", "inch"] } else { vec!["mm"] }
}

/// One witness: the start (micrometres), the record's end and its centre.
struct Case {
    unit: &'static str,
    start: (&'static str, &'static str),
    end: (&'static str, &'static str),
    centre: (&'static str, &'static str),
}

/// mm (3 decimals): centre X10 Y5, radius 10.51 um, a CCW sweep of 0.047
/// rad. Written start X10.011 Y5.000, written end X10.010 Y5.000: the same
/// angle about the written centre X10.000 Y5.000.
const MM: Case = Case {
    unit: "mm",
    start: ("10010.51", "5000"),
    end: ("10010.498393", "5000.493788"),
    centre: ("10000", "5000"),
};

/// inch (4 decimals): centre X0.4 Y0.2 in, radius 26.68 um, a CCW sweep
/// of 0.04 rad. Written start X0.4011 Y0.2, written end X0.401 Y0.2.
const INCH: Case = Case {
    unit: "inch",
    start: ("10186.68", "5080"),
    end: ("10186.658660", "5081.066911"),
    centre: ("10160", "5080"),
};

/// The review witness of #3500 (mm, 3 decimals): a CW record about X10
/// Y5.00098. The absolute centre rounds to X10. Y5.001, but the control
/// adds the written I-0.011 J0. to the written start X10.011 Y5.: its
/// centre is X10. Y5. The written end X10.01 Y5. is on the same ray.
const MM_OFFSET: Case = Case {
    unit: "mm",
    start: ("10010.51", "5000.49"),
    end: ("10010.475372320", "4999.996752949"),
    centre: ("10000", "5000.98"),
};

/// The same in inch (4 decimals): the absolute centre rounds to X0.4
/// Y0.2001; the written start X0.4011 Y0.2 plus I-0.0011 J0. gives X0.4
/// Y0.2. The written end X0.401 Y0.2 is on the same ray (a CW sweep of
/// 0.051 rad).
const INCH_OFFSET: Case = Case {
    unit: "inch",
    start: ("10186.72", "5081.143"),
    end: ("10186.645370", "5079.768811"),
    centre: ("10160", "5081.905"),
};

/// A mill section that plunges at the case's start to Z 5.5, cuts the
/// arc (or, for the reference, the straight feed to its end) and retracts.
fn stream(c: &Case, arc: bool) -> String {
    stream_dir(c, arc, "CCW")
}

fn stream_dir(c: &Case, arc: bool, dir: &str) -> String {
    let (sx, sy) = c.start;
    let (ex, ey) = c.end;
    let (cx, cy) = c.centre;
    let cut = if arc {
        format!("Arc {dir} X {ex} Y {ey} Z 5500 F 180000 I {cx} J {cy}\n")
    } else {
        format!("Feed X {ex} Y {ey} Z 5500 F 180000\n")
    };
    format!(
        "; HSMW-CNC v1\nPARAM OperationName Same angle\nPARAM SpindleSpeed 6000\nPARAM WorkOffset 1\n\
         PARAM OriginX 0\nPARAM OriginY 0\nPARAM OriginZ 0\nPARAM ToolKind Flat end mill\nPARAM ToolDiameter 4\n\
         PARAM ClearancePlane 18\nPARAM RetractPlane 18\nPARAM TopOfStock 6\nPARAM StockXYLeft -20\nPARAM StockXYRight 40\n\
         PARAM StockXYFront -20\nPARAM StockXYBack 40\nPARAM StockZTop 6\nPARAM StockZBottom -10\n\
         TOOL 1 EM-0400-2FL\nSECTION Same angle\n\
         Rapid X {sx} Y {sy} Z 18000\nPlunge X {sx} Y {sy} Z 5500 F 45000\n{cut}Rapid X {ex} Y {ey} Z 18000\nENDSECTION\n"
    )
}

/// The blocks that the issue asked for: Fanuc and LinuxCNC, mm and inch,
/// post the same-angle arc as one straight move to the written end.
#[test]
fn fanuc_and_linuxcnc_post_a_same_angle_arc_as_a_straight_move() {
    for (stem, case, end) in [
        ("fanuc", &MM, "X10.01 Y5."),
        ("fanuc", &INCH, "X0.401 Y0.2"),
        ("linuxcnc", &MM, "X10.01"),
        ("linuxcnc", &INCH, "X0.401"),
    ] {
        let got = post(stem, &stream(case, true), case.unit);
        assert!(arc_blocks(&got, false).is_empty(), "{stem} {}: an arc block:\n{got}", case.unit);
        assert!(got.ends_with("warnings: []\n"), "{stem} {}: {got}", case.unit);
        assert!(got.lines().any(|l| l.contains("G01") || l.contains("G1 ")), "{stem} {}: no straight move:\n{got}", case.unit);
        assert!(got.contains(end), "{stem} {}: the move ends at {end}:\n{got}", case.unit);
    }
}

/// The lathe witness (mm, diameter X with 3 decimals): a meridian arc
/// about X24 (radius 12) Z-1, radius 10.26 um, a sweep of 0.046 rad.
/// Written start X24.021 Z-1., written end X24.02 Z-1.: the same angle.
fn lathe_stream(arc: bool) -> String {
    let cut = if arc {
        "Arc CW X 12010.249147 Y 0 Z -999.528206 F 300000 I 12000 K -1000\n"
    } else {
        "Feed X 12010.249147 Y 0 Z -999.528206 F 300000\n"
    };
    format!(
        "; HSMW-CNC v1\nPARAM StockXYLeft -20\nPARAM StockXYRight 20\nPARAM StockXYFront -20\nPARAM StockXYBack 20\n\
         PARAM StockZTop 0\nPARAM StockTurnRadius 20\nPARAM ClearancePlane 5\nPARAM SpindleSpeed 1500\nPARAM WorkOffset 1\n\
         TOOL 1 TURN\nSECTION\nRapid X 22000 Y 0 Z 2000\nFeed X 12010.26 Y 0 Z -1000 F 300000\n{cut}\
         Rapid X 22000 Y 0 Z 2000\nENDSECTION\n"
    )
}

/// The table over every shipped mill post and each unit that it offers,
/// and the two lathes: the same-angle arc posts exactly as the straight
/// feed to its end. TinyG writes 4 decimals in mm: there the mm witness
/// is a visible arc (X10.0105 Y5 to X10.0105 Y5.0005) and stays one.
#[test]
fn every_post_writes_a_same_angle_arc_as_its_straight_move() {
    let mut failures = Vec::new();
    for stem in ["fanuc-lathe", "tormach-lathe"] {
        let lathe = json!({"name": "ARC LATHE", "spindleMax": 4000});
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let run = |arc: bool| match engine::run_with(&cps, &lathe, &lathe_stream(arc), &json!({ "Units": "mm" })) {
            Ok(out) => format!("{}--- warnings: {:?}\n", out.program, out.warnings),
            Err(e) => format!("--- error: {e}\n"),
        };
        let (got, want) = (run(true), run(false));
        if !want.lines().any(|l| l.starts_with("G01 X24.02 ") || l.starts_with("G1 X24.02 ")) {
            failures.push(format!("{stem} mm: the reference has no feed move to X24.02\n{want}"));
        }
        if !arc_blocks(&got, true).is_empty() || got != want {
            failures.push(format!("{stem} mm: not the straight move\n--- got\n{got}--- want\n{want}"));
        }
    }
    for &stem in MILLS {
        for unit in units(stem) {
            let case = if unit == "inch" { &INCH } else { &MM };
            let got = post(stem, &stream(case, true), unit);
            let want = post(stem, &stream(case, false), unit);
            if stem == "tinyg" && unit == "mm" {
                if arc_blocks(&got, false).len() != 1 {
                    failures.push(format!("{stem} {unit}: the visible arc is not one arc block\n{got}"));
                }
                continue;
            }
            if !arc_blocks(&got, false).is_empty() {
                failures.push(format!("{stem} {unit}: an arc block\n{got}"));
            } else if got != want {
                failures.push(format!("{stem} {unit}: output differs from the straight move\n--- got\n{got}--- want\n{want}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// #3500 review: the guard reads the centre that the control computes, the
/// written start plus the written I J increments, not the rounded absolute
/// centre. Each mill post that writes increments, in each unit that it
/// offers, posts the offset-centre witness exactly as the straight feed.
/// The Heidenhain posts write an absolute centre (I J or CC): their
/// written centre is the rounded absolute one, so this witness is not on
/// one ray there. TinyG writes 4 decimals in mm: the mm witness is a
/// visible arc there.
#[test]
fn a_same_angle_arc_about_the_written_incremental_centre_posts_as_its_straight_move() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for &stem in MILLS.iter().chain(["mach3"].iter()) {
        if stem.starts_with("heidenhain") {
            continue;
        }
        for unit in units(stem) {
            if stem == "tinyg" && unit == "mm" {
                continue;
            }
            let case = if unit == "inch" { &INCH_OFFSET } else { &MM_OFFSET };
            let got = post(stem, &stream_dir(case, true, "CW"), unit);
            let want = post(stem, &stream_dir(case, false, "CW"), unit);
            checked += 1;
            if !arc_blocks(&got, false).is_empty() {
                failures.push(format!("{stem} {unit}: an arc block\n{got}"));
            } else if got != want {
                failures.push(format!("{stem} {unit}: output differs from the straight move\n--- got\n{got}--- want\n{want}"));
            } else if !want.ends_with("warnings: []\n") {
                failures.push(format!("{stem} {unit}: the reference warns\n{want}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(checked >= 20, "only {checked} post and unit pairs ran");
    // The review's own blocks: Fanuc mm wrote G02 X10.01 Y5. Z5.5 I-0.011 J0.
    let fanuc = post("fanuc", &stream_dir(&MM_OFFSET, true, "CW"), "mm");
    assert!(fanuc.lines().any(|l| l.contains("G01") && l.contains("X10.01 Y5.")), "{fanuc}");
}

/// The rule itself: the written ends differ, but the written end is on the
/// written start's ray from the written centre (or on the centre). A short
/// record is a straight move; a long record (a turn of more than half a
/// circle) stays the arc, and so does a normal short arc. A record whose
/// own ends (in the stream) are on one ray states a full turn and stays
/// the arc (the 359.99 degree record of `radius_arcs.rs` arrives so).
#[test]
fn arc_fate_reads_a_same_angle_short_arc_as_a_straight_move() {
    let cps = r#"
        function w(k, v) { return k + (Math.round(v * 1000) / 1000).toFixed(3); }
        function fate(s, e, c, ccw) {
          return arcFateOf({ plane: "XY", ccw: ccw, rotary: false,
            start: { X: s[0], Y: s[1], Z: 0 }, end: { X: e[0], Y: e[1], Z: 0 },
            center: { X: c[0], Y: c[1], Z: 0 } }, w);
        }
        function inc(s, e, c, ccw, scale) {
          return arcFateOf({ plane: "XY", ccw: ccw, rotary: false,
            start: { X: s[0], Y: s[1], Z: 0 }, end: { X: e[0], Y: e[1], Z: 0 },
            center: { X: c[0], Y: c[1], Z: 0 } },
            function (k, v) { return w(k, (k === "X" ? scale : 1) * v); },
            function (k, d) { return (k === "X" ? scale : 1) * _arcWordNum(w(k, d)); });
        }
        function onOpen() {
          // #3500: the centre is the written start plus the written I J.
          writeln("offset-absolute " + fate([10.01051, 5.00049], [10.01047537232, 4.999996752949], [10, 5.00098], false));
          writeln("offset-incremental " + inc([10.01051, 5.00049], [10.01047537232, 4.999996752949], [10, 5.00098], false, 1));
          writeln("offset-incremental-ccw-is-long " + inc([10.01051, 5.00049], [10.01047537232, 4.999996752949], [10, 5.00098], true, 1));
          writeln("incremental-normal " + inc([11, 5], [10, 6], [10, 5], true, 1));
          // A diameter X (the lathes): the radius I is doubled to the X scale.
          writeln("diameter-incremental " + inc([12.01026, -1], [12.01, -1.0004], [12, -1], false, 2));
          writeln("same-angle " + fate([10.01051, 5], [10.010498393, 5.000493788], [10, 5], true));
          writeln("same-angle-cw-is-long " + fate([10.01051, 5], [10.010498393, 5.000493788], [10, 5], false));
          writeln("end-on-centre " + fate([10.0006, 5], [10.00044, 5.00041], [10, 5], true));
          writeln("opposite " + fate([10.0006, 5], [9.9994, 5.0000001], [10, 5], true));
          writeln("normal " + fate([11, 5], [10, 6], [10, 5], true));
          writeln("stream-ray-is-full " + fate([11.235, 20], [11.234, 20], [10, 20], true));
        }
    "#;
    let out = engine::run(cps, &Value::Null, "").expect("the rule script runs");
    assert_eq!(
        out.program,
        "offset-absolute arc\noffset-incremental linear\noffset-incremental-ccw-is-long arc\n\
         incremental-normal arc\ndiameter-incremental linear\n\
         same-angle linear\nsame-angle-cw-is-long arc\nend-on-centre linear\nopposite arc\nnormal arc\nstream-ray-is-full arc\n"
    );
}

//! #3442: no shipped post writes an arc block whose end point, as written
//! (output unit, precision, work origin and modal words applied), is its
//! start point in the arc plane. A control reads such a block as a full
//! circle. A short arc under the output resolution posts as a straight
//! move (a helix) or as no block; a normal arc and a full circle (two
//! halves) post as before.
//!
//! Provenance: our own invariants; the full-circle reading of an arc block
//! whose end is its start is RS-274 / ISO 6983 circular interpolation.
//! `coincident_arcs/tiny.cnc` is the reproduction that qsmits gave on
//! #3442. The arc reader below is our own.
//!
//! Gate map (M4.4): covers assets/posts/carvera.cps,
//! assets/posts/doosan-dnm.cps, assets/posts/fanuc.cps,
//! assets/posts/fanuc-lathe.cps, assets/posts/grbl.cps,
//! assets/posts/grblhal.cps, assets/posts/haas.cps,
//! assets/posts/heidenhain.cps, assets/posts/heidenhain-klartext.cps,
//! assets/posts/linuxcnc.cps, assets/posts/mach3.cps, assets/posts/mach4.cps,
//! assets/posts/mazak.cps, assets/posts/richauto.cps,
//! assets/posts/sinker-iso.cps,
//! assets/posts/sinumerik.cps, assets/posts/sodick-wire.cps,
//! assets/posts/tinyg.cps, assets/posts/tormach.cps,
//! assets/posts/tormach-lathe.cps, assets/posts/tormach-plasma.cps,
//! assets/posts/waterjet-iso.cps, assets/posts/wire-iso.cps,
//! camcraft_post::post::arc_guard,
//! camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[path = "coincident_arcs/reader.rs"]
mod reader;
use reader::{arc_blocks, collapsed};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// The kind of witness a post takes.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Mill,
    Lathe,
    Plasma,
    Waterjet,
    Wire,
    Sinker,
}

/// Every shipped post script, with the witness it takes.
const POSTS: &[(&str, Kind)] = &[
    ("carvera", Kind::Mill),
    ("doosan-dnm", Kind::Mill),
    ("fanuc", Kind::Mill),
    ("fanuc-lathe", Kind::Lathe),
    ("grbl", Kind::Mill),
    ("grblhal", Kind::Mill),
    ("haas", Kind::Mill),
    ("heidenhain", Kind::Mill),
    ("heidenhain-klartext", Kind::Mill),
    ("linuxcnc", Kind::Mill),
    ("mach3", Kind::Mill),
    ("mach4", Kind::Mill),
    ("mazak", Kind::Mill),
    ("richauto", Kind::Mill),
    ("sinker-iso", Kind::Sinker),
    ("sinumerik", Kind::Mill),
    ("sodick-wire", Kind::Wire),
    ("tinyg", Kind::Mill),
    ("tormach", Kind::Mill),
    ("tormach-lathe", Kind::Lathe),
    ("tormach-plasma", Kind::Plasma),
    ("waterjet-iso", Kind::Waterjet),
    ("wire-iso", Kind::Wire),
];

const ARC_FEED: &str = "F 180000";

/// The mill witness: two sub-resolution XY arcs (0.03 and 0.01 um - under
/// the 1 um and 0.1 um metric steps and the 2.54 um inch step), one sub-resolution
/// helical arc, one normal quarter arc, one full circle as two halves and
/// one sub-resolution XZ (G18) arc. `reference` gives the same path with
/// the sub-resolution arcs as what they must post as: nothing, or the
/// straight move to the helix's end.
fn mill_cuts(reference: bool) -> String {
    let f = ARC_FEED;
    let mut s = String::new();
    if !reference {
        s += &format!("Arc CCW X 18123 Y 5432.03 Z 5500 {f} I 10123 J 5432\n");
        s += &format!("Arc CCW X 18123 Y 5432.04 Z 5500 {f} I 10123 J 5432\n");
        s += &format!("Arc CW X 18123 Y 5432.02 Z 5400 {f} I 10123 J 5432\n");
    } else {
        s += &format!("Feed X 18123 Y 5432.02 Z 5400 {f}\n");
    }
    s += &format!("Arc CCW X 10123 Y 13432 Z 5400 {f} I 10123 J 5432\n");
    s += &format!("Arc CCW X 10123 Y -2568 Z 5400 {f} I 10123 J 5432\n");
    s += &format!("Arc CCW X 10123 Y 13432 Z 5400 {f} I 10123 J 5432\n");
    if !reference {
        s += &format!("Arc CCW X 10123 Y 13432 Z 5399.97 {f} I 2123 K 5400\n");
    }
    s
}

/// The flat witness of the 2D posts (plasma, waterjet, wire): the XY part of the
/// mill witness, at Z 0.
fn flat_cuts(reference: bool) -> String {
    let f = ARC_FEED;
    let mut s = String::new();
    if !reference {
        s += &format!("Arc CCW X 18123 Y 5432.03 Z 0 {f} I 10123 J 5432\n");
        s += &format!("Arc CCW X 18123 Y 5432.04 Z 0 {f} I 10123 J 5432\n");
    }
    s += &format!("Arc CCW X 10123 Y 13432 Z 0 {f} I 10123 J 5432\n");
    s += &format!("Arc CCW X 10123 Y -2568 Z 0 {f} I 10123 J 5432\n");
    s += &format!("Arc CCW X 10123 Y 13432 Z 0 {f} I 10123 J 5432\n");
    s
}

/// The lathe witness: a sub-resolution meridian (XZ) arc and a normal one.
fn lathe_cuts(reference: bool) -> String {
    let mut s = String::new();
    if !reference {
        s += "Arc CCW X 20000 Y 0 Z -1000.03 F 300000 I 12000 K -1000\n";
    }
    s += "Arc CCW X 12000 Y 0 Z -9000 F 300000 I 12000 K -1000\n";
    s
}

/// A mill section that plunges at X 18.123 Y `y` (micrometres) to Z 5.5,
/// cuts `cuts` and retracts to Z 18 over `end`.
fn mill_stream(y: &str, cuts: &str, end: &str) -> String {
    format!(
        "; HSMW-CNC v1\nPARAM OperationName Arc witness\nPARAM SpindleSpeed 6000\nPARAM WorkOffset 1\n\
         PARAM StartX 18.123\nPARAM StartY 5.432\nPARAM StartZ 18\n\
         PARAM OriginX 0\nPARAM OriginY 0\nPARAM OriginZ 0\nPARAM ToolKind Flat end mill\nPARAM ToolDiameter 4\n\
         PARAM ClearancePlane 18\nPARAM RetractPlane 18\nPARAM TopOfStock 6\nPARAM StockXYLeft -20\nPARAM StockXYRight 40\n\
         PARAM StockXYFront -20\nPARAM StockXYBack 40\nPARAM StockZTop 6\nPARAM StockZBottom -10\n\
         TOOL 1 EM-0400-2FL\nSECTION Arc witness\n\
         Rapid X 18123 Y {y} Z 18000\nPlunge X 18123 Y {y} Z 5500 F 45000\n{cuts}Rapid {end} Z 18000\nENDSECTION\n"
    )
}

fn stream(kind: Kind, reference: bool) -> String {
    match kind {
        Kind::Mill | Kind::Sinker => mill_stream("5432", &mill_cuts(reference), "X 10123 Y 13432"),
        Kind::Lathe => format!(
            "; HSMW-CNC v1\nPARAM StockXYLeft -20\nPARAM StockXYRight 20\nPARAM StockXYFront -20\nPARAM StockXYBack 20\n\
             PARAM StockZTop 0\nPARAM StockTurnRadius 20\nPARAM ClearancePlane 5\nPARAM SpindleSpeed 1500\nPARAM WorkOffset 1\n\
             TOOL 1 TURN\nSECTION\nRapid X 22000 Y 0 Z 2000\nFeed X 20000 Y 0 Z -1000 F 300000\n{}Rapid X 22000 Y 0 Z 2000\nENDSECTION\n",
            lathe_cuts(reference)
        ),
        Kind::Plasma => format!(
            "; HSMW-CNC v1\nPARAM OperationName Plasma witness\nPARAM WorkOffset 1\nPLASMA KERF1500\nSECTION\n\
             Rapid X 26123 Y 5432 Z 0\nTORCH ON C0 OPEN\nFeed X 18123 Y 5432 Z 0 F 2000\n{}TORCH OFF C0\nRapid X 26123 Y 5432 Z 0\nENDSECTION\n",
            flat_cuts(reference)
        ),
        Kind::Waterjet => format!(
            "; HSMW-CNC v1\nPARAM OperationName Waterjet witness\nPARAM WorkOffset 1\nWATERJET ABRASIVE DWELL800 LAG300\nSECTION\n\
             Rapid X 26123 Y 5432 Z 0\nJET ON C0 OPEN\nFeed X 18123 Y 5432 Z 0 F 300000\n{}JET OFF C0\nRapid X 26123 Y 5432 Z 0\nENDSECTION\n",
            flat_cuts(reference)
        ),
        Kind::Wire => format!(
            "; HSMW-CNC v1\nPARAM OperationName Punch1\nPARAM WorkOffset G54\nPARAM EdmCondition C110\nPARAM WireSide punch\nSECTION\n\
             Rapid X 26123 Y 5432 Z 0\nFeed X 18123 Y 5432 Z 0 F 2000\n{}ENDSECTION\n",
            flat_cuts(reference)
        ),
    }
}

fn machine(stem: &str) -> Value {
    let mirrored = |rel: &str| {
        let parsed = camcraft_post::machine::parse_mch(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        camcraft_post::post::machine::mirror(&parsed)
    };
    match stem {
        "heidenhain-klartext" => mirrored("assets/machines/fehlmann-picomax55.mch"),
        "tormach-plasma" => mirrored("assets/machines/tormach-1300pl.mch"),
        "waterjet-iso" => json!({"name": "ARC WATERJET", "kind": "waterjet"}),
        "wire-iso" => serde_json::from_str(&read("crates/app/tests/edm/wire-iso-fixture.mch")).expect("wire fixture"),
        "sodick-wire" => json!({"kind": "wire_edm", "name": "Synthetic wire", "edm": {"awt": true}}),
        "sinker-iso" => serde_json::from_str(&read("crates/app/tests/edm/sinker-iso-dialect.mch")).expect("sinker fixture"),
        "fanuc-lathe" | "tormach-lathe" => json!({"name": "ARC LATHE", "spindleMax": 4000}),
        _ => json!({"name": "ARC MILL", "spindleMax": 12000, "safeRetractZ": 0}),
    }
}

/// The program and the warnings (or the error) of one post run.
fn post(stem: &str, stream: &str, opts: &Value) -> String {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    match engine::run_with(&cps, &machine(stem), stream, opts) {
        Ok(out) => format!("{}--- warnings: {:?}\n", out.program, out.warnings),
        Err(e) => format!("--- error: {e}\n"),
    }
}

fn units(stem: &str) -> Vec<&'static str> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let line = cps.lines().find(|l| l.starts_with("programUnits")).unwrap_or("");
    let value = line.split('"').nth(1).unwrap_or("");
    if value.contains("inch") { vec!["mm", "inch"] } else { vec!["mm"] }
}

fn lathe(kind: Kind) -> bool {
    kind == Kind::Lathe
}

/// (b) The table over every shipped post and each of its units (and the
/// R-word mode where the script offers it): the law holds, and the output
/// is the reference's - so the normal arc and the full circle are
/// unchanged and each sub-resolution arc became what the rule says.
#[test]
fn every_post_writes_no_coincident_arc_block_and_keeps_its_real_arcs() {
    let mut failures = Vec::new();
    for &(stem, kind) in POSTS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let mut modes = vec![false];
        if cps.contains("useRadiusArcs") {
            modes.push(true);
        }
        for unit in units(stem) {
            for &r_mode in &modes {
                let mut opts = json!({ "Units": unit });
                if r_mode {
                    opts["useRadiusArcs"] = json!(true);
                }
                let ctx = format!("{stem} {unit}{}", if r_mode { " R" } else { "" });
                let got = post(stem, &stream(kind, false), &opts);
                let bad = collapsed(&got, lathe(kind));
                if !bad.is_empty() {
                    failures.push(format!("{ctx}: coincident arc blocks {bad:?}"));
                }
                if kind == Kind::Sinker {
                    if !arc_blocks(&got, false).is_empty() {
                        failures.push(format!("{ctx}: the sinker post wrote an arc block:\n{got}"));
                    }
                    continue;
                }
                let want = post(stem, &stream(kind, true), &opts);
                if arc_blocks(&want, lathe(kind)).is_empty() {
                    failures.push(format!("{ctx}: the reference posts no arc:\n{want}"));
                }
                if got != want {
                    failures.push(format!("{ctx}: output differs from the reference\n--- got\n{got}--- want\n{want}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// (a) The reporter's reproduction (qsmits, #3442): Fanuc in inch wrote
/// `G03 X0.7135 Y0.2139 Z0.2165 I-0.315 J0. F7.09` twice, each a full
/// circle on the control. Each arc is under the 0.0001 in step: no arc
/// block, and the program is released without a warning.
#[test]
fn fanuc_inch_writes_no_full_circle_for_the_reporters_short_arcs() {
    let tiny = read("crates/post/tests/coincident_arcs/tiny.cnc");
    let cps = read("assets/posts/fanuc.cps");
    let out = engine::run_with(&cps, &Value::Null, &tiny, &json!({ "Units": "inch" })).expect("tiny.cnc posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let text = format!("{}--- warnings: []\n", out.program);
    assert!(collapsed(&text, false).is_empty(), "coincident arc blocks:\n{}", out.program);
    assert!(arc_blocks(&text, false).is_empty(), "an arc block under the output step:\n{}", out.program);
    assert!(out.program.contains("X0.7135 Y0.2139"), "{}", out.program);
}

/// (c) Metric collides too: two ends under 1 um apart write the same
/// three-decimal word. Fanuc mm, a 0.4 um arc and a 0.4 um helix.
#[test]
fn fanuc_mm_three_decimal_collision_is_no_arc_block() {
    let cps = read("assets/posts/fanuc.cps");
    let body = "Arc CCW X 18123 Y 5432.04 Z 5500 F 180000 I 10123 J 5432\n\
                Arc CW X 18123 Y 5432 Z 5000 F 180000 I 10123 J 5432\n";
    let s = format!(
        "; HSMW-CNC v1\nPARAM OperationName Metric\nPARAM SpindleSpeed 6000\nPARAM WorkOffset 1\nPARAM ClearancePlane 18\n\
         PARAM RetractPlane 18\nTOOL 1 EM\nSECTION\nRapid X 18123 Y 5432 Z 18000\nPlunge X 18123 Y 5432 Z 5500 F 45000\n{body}\
         Rapid X 18123 Y 5432 Z 18000\nENDSECTION\n"
    );
    let out = engine::run_with(&cps, &Value::Null, &s, &Value::Null).expect("metric witness posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let text = format!("{}--- warnings: []\n", out.program);
    assert!(collapsed(&text, false).is_empty(), "coincident arc blocks:\n{}", out.program);
    assert!(arc_blocks(&text, false).is_empty(), "an arc block under the output step:\n{}", out.program);
    // The helix posts as the straight move to its end.
    assert!(out.program.lines().any(|l| l.starts_with("G01 X18.123 Y5.432 Z5. ")), "{}", out.program);
}

/// R-word output: a short arc that the output can still show (a 3 um
/// chord on R8) is legal and keeps its minor-arc solution - a positive R,
/// never the 180+ degree arc through the same two points.
#[test]
fn a_short_resolvable_r_arc_keeps_its_minor_solution() {
    let s = format!(
        "; HSMW-CNC v1\nPARAM OperationName Short R\nPARAM SpindleSpeed 6000\nPARAM WorkOffset 1\nPARAM ClearancePlane 18\n\
         PARAM RetractPlane 18\nTOOL 1 EM\nSECTION\nRapid X 18000 Y 5000 Z 18000\nPlunge X 18000 Y 5000 Z 5500 F 45000\n\
         Arc CCW X 18000 Y 5003 Z 5500 {ARC_FEED} I 10000 J 5000\nRapid X 18000 Y 5003 Z 18000\nENDSECTION\n"
    );
    for &(stem, _) in POSTS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        if !cps.contains("useRadiusArcs") {
            continue;
        }
        let out = post(stem, &s, &json!({ "useRadiusArcs": true }));
        let arcs = arc_blocks(&out, false);
        assert_eq!(arcs.len(), 1, "{stem}: one arc block:\n{out}");
        let r = arcs[0].r.unwrap_or_else(|| panic!("{stem}: an R word in `{}`", arcs[0].line));
        assert!(r > 0.0, "{stem}: the minor arc takes a positive R: `{}`", arcs[0].line);
        assert!(collapsed(&out, false).is_empty(), "{stem}:\n{out}");
    }
}

fn mills() -> impl Iterator<Item = &'static str> {
    POSTS.iter().filter(|(_, k)| *k == Kind::Mill).map(|(s, _)| *s)
}

/// The rule itself, through the PRELUDE that every script sees. A record
/// with no usable centre is not read by the rule ("arc"): the scripts that
/// refuse it by name (sodick-wire "an arc states no centre", heidenhain
/// "arc without an XY centre", tinyg "an arc needs a finite center") still
/// do, also when its written ends coincide. The `.cnc` reader already
/// refuses an arc without two centre words, so the posts cannot be given
/// one from a file; the rule's answer is pinned here.
#[test]
fn arc_fate_reads_the_written_words_and_leaves_a_centreless_arc_to_its_script() {
    let cps = r#"
        function w(k, v) { return (Math.round(v * 1000) / 1000).toFixed(3); }
        function fate(s, e, c, ccw, plane, rotary) {
          return arcFateOf({ plane: plane || "XY", ccw: ccw, rotary: !!rotary,
            start: { X: s[0], Y: s[1], Z: s[2] }, end: { X: e[0], Y: e[1], Z: e[2] },
            center: { X: c[0], Y: c[1], Z: c[2] } }, w);
        }
        function onOpen() {
          writeln("nocentre " + fate([1, 0, 0], [1, 0, 0], [null, null, null], true));
          writeln("nocentre-short " + fate([1, 0, 0], [1, 0.0002, 0], [NaN, 0, 0], true));
          writeln("short " + fate([1, 0, 0], [1, 0.0002, 0], [0, 0, 0], true));
          writeln("short-cw-is-near-full " + fate([1, 0, 0], [1, 0.0002, 0], [0, 0, 0], false));
          writeln("helix " + fate([1, 0, 0], [1, 0.0002, -1], [0, 0, 0], true));
          writeln("rotary " + fate([1, 0, 0], [1, 0.0002, 0], [0, 0, 0], true, "XY", true));
          writeln("noise-ahead " + fate([1, 0, 0], [1, 0.000001, -1], [0, 0, 0], true));
          writeln("noise-behind " + fate([1, 0, 0], [1, -0.000001, -1], [0, 0, 0], true));
          writeln("exact-full " + fate([1, 0, 0], [1, 0, 0], [0, 0, 0], true));
          writeln("tiny-radius " + fate([0.0001, 0, 0], [0.0001, 0, 0], [0, 0, 0], true));
          writeln("visible " + fate([1, 0, 0], [1, 0.002, 0], [0, 0, 0], true));
          writeln("xz-short " + fate([1, 0, 0], [1, 0, -0.0002], [0, 0, 0], true, "XZ"));
          writeln("xz-short-cw " + fate([1, 0, 0], [1, 0, -0.0002], [0, 0, 0], false, "XZ"));
        }
    "#;
    let out = engine::run(cps, &Value::Null, "").expect("the rule script runs");
    let want = "nocentre arc\nnocentre-short arc\nshort none\nshort-cw-is-near-full arc\nhelix linear\nrotary linear\n\
                noise-ahead linear\nnoise-behind arc\nexact-full arc\ntiny-radius none\nvisible arc\nxz-short none\nxz-short-cw arc\n";
    assert_eq!(out.program, want);
}

/// A helical turn stated as one record whose end is 1e-6 mm short of its
/// start (its sweep from direction and centre is nearly a full turn)
/// posts as the same full helical turn as the record whose ends are equal,
/// never as a straight Z move. (A record whose end is 1e-6 mm AHEAD of
/// its start is a true short helix by its own geometry, and posts as the
/// straight move: the rule test pins that case.)
#[test]
fn a_full_helical_turn_off_by_float_noise_stays_a_helix() {
    let noisy = mill_stream("5432", "Arc CCW X 18123 Y 5431.999 Z 5000 F 180000 I 10123 J 5432\n", "X 18123 Y 5432");
    let exact = mill_stream("5432", "Arc CCW X 18123 Y 5432 Z 5000 F 180000 I 10123 J 5432\n", "X 18123 Y 5432");
    for stem in mills() {
        for unit in units(stem) {
            let opts = json!({ "Units": unit });
            let (got, want) = (post(stem, &noisy, &opts), post(stem, &exact, &opts));
            assert_eq!(got, want, "{stem} {unit}: the noisy full turn posts as the exact one");
        }
    }
    let fanuc = post("fanuc", &noisy, &Value::Null);
    assert!(fanuc.lines().any(|l| l.starts_with("G03 X18.123 Y5.432 Z5. I-8. J0.")), "{fanuc}");
}

/// A near-full turn (20 nm short of a full circle) whose written ends
/// coincide posts as the dialect's full circle: the same blocks as the
/// record that states the full turn. Klartext, whose cited manual does
/// not state a full-circle C block, refuses it by name.
#[test]
fn a_near_full_turn_with_coincident_written_ends_posts_one_full_circle() {
    let near = mill_stream("5432.04", "Arc CCW X 18123 Y 5432.02 Z 5500 F 180000 I 10123 J 5432.04\n", "X 18123 Y 5432.02");
    let full = mill_stream("5432.04", "Arc CCW X 18123 Y 5432.04 Z 5500 F 180000 I 10123 J 5432.04\n", "X 18123 Y 5432.02");
    for stem in mills() {
        for unit in units(stem) {
            let opts = json!({ "Units": unit });
            let got = post(stem, &near, &opts);
            if stem == "heidenhain-klartext" {
                assert!(got.contains("rounding would turn the XY arc into a full circle"), "{stem}: {got}");
                continue;
            }
            if stem == "tinyg" {
                // TinyG writes a turn as two halves at the sweep's midpoint, which can
                // round one step off the full turn's midpoint: two arcs, distinct ends.
                assert_eq!(arc_blocks(&got, false).len(), 2, "{stem} {unit}: {got}");
                assert!(collapsed(&got, false).is_empty() && got.ends_with("warnings: []\n"), "{stem} {unit}: {got}");
                continue;
            }
            assert_eq!(got, post(stem, &full, &opts), "{stem} {unit}: the near-full turn posts as the full circle");
        }
    }
    for unit in ["mm", "inch"] {
        let fanuc = post("fanuc", &near, &json!({ "Units": unit }));
        let circles = collapsed(&fanuc, false);
        assert_eq!(circles.len(), 1, "fanuc {unit}: one full-circle block:\n{fanuc}");
        assert!(circles[0].starts_with("G03 ") && circles[0].contains(" I-"), "fanuc {unit}: {}", circles[0]);
    }
}

/// A short arc under the output step that turns a rotary keeps the
/// rotary move: it posts as the straight move with its angle word.
#[test]
fn a_short_arc_with_a_rotary_word_posts_the_rotary_move() {
    let parsed = camcraft_post::machine::parse_mch(&read("assets/machines/tilthead.mch")).expect("tilthead");
    let mch = camcraft_post::post::machine::mirror(&parsed);
    let cuts = "Rapid X 18123 Y 5432 Z 18000 A 0 B 0 C 0\nFeed X 18123 Y 5432 Z 5500 F 45000 A 0 B 0 C 0\n\
                Arc CCW X 18123 Y 5432.02 Z 5500 F 180000 A 1000 B 0 C 0 I 10123 J 5432\n";
    let s = mill_stream("5432", cuts, "X 18123 Y 5432.02");
    // Two posts that write tilted feed moves on this machine (Haas refuses
    // a tilted move without RTCP by name); Doosan states its rotary brake.
    for stem in ["fanuc", "doosan-dnm"] {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        for unit in ["mm", "inch"] {
            let opts = json!({ "Units": unit, "rotaryBrake": stem == "doosan-dnm" });
            let out = engine::run_with(&cps, &mch, &s, &opts).expect("rotary witness posts");
            assert!(out.warnings.is_empty(), "{stem} {unit}: {:?}", out.warnings);
            let text = format!("{}--- warnings: []\n", out.program);
            assert!(arc_blocks(&text, false).is_empty(), "{stem} {unit}: no arc block:\n{}", out.program);
            let a1 = |l: &str| (l.starts_with("G01 ") || l.starts_with("G1 ")) && l.split_whitespace().any(|w| w == "A1." || w == "A1");
            assert!(out.program.lines().any(a1), "{stem} {unit}: the A move is posted:\n{}", out.program);
        }
    }
}

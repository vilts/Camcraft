//! #1481: the reaming cycle through every shipped LATHE post - an axial
//! ream on the spindle centreline. fanuc-lathe has a canned feed-in /
//! feed-out face cycle and writes G85 (never a different cycle), with
//! the hole's expanded chain suppressed; tormach-lathe has no canned
//! cycle (PathPilot's lathe lists none) and posts the expanded chain -
//! feed in, FEED out, never a rapid over the reamed wall. A ream off the
//! spindle axis is refused by name on both. Goldens for both posts sit
//! in `tests/golden/ream/` beside the mill posts' (`ream_cycle.rs`) -
//! our mapping's regression record, not a reading taken on a control.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test ream_lathe`
//!
//! Gate map (M4.4): covers assets/posts/fanuc-lathe.cps,
//! tormach-lathe.cps (their cycle handlers), camcraft_post::post::cnc
//! (the REAM cycle word), camcraft_post::post::engine.
//! Lane shape (M4.4): all tests here run in milliseconds.

mod recorded;

use camcraft_post::cl::{Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::{Path, PathBuf};

/// Every shipped lathe post, with the shipped machine file it posts on.
const LATHE_POSTS: [(&str, &str); 2] =
    [("fanuc-lathe", "lathe.mch"), ("tormach-lathe", "tormach-15l-slant-pro.mch")];

/// Every canned hole-cycle word a control could read a ream as.
const CANNED_WORDS: [&str; 9] = ["G73", "G81", "G82", "G83", "G84", "G85", "G86", "G87", "G89"];

const R: f64 = 2.0;
const HOP: f64 = 10.0;
const BOTTOM: f64 = -12.0;
const FEED: f64 = 150.0;
/// The bar's radius; the tool waits outside it, in front of the face.
const BAR: f64 = 15.0;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// One reamed hole along the spindle axis at radius `x` (0 = on the
/// centreline), the chain the drilling generator writes for
/// `CycleKind::Ream` (crates/motion/src/drilling/cycle.rs): approach in
/// front of the face, rapid to R, feed to the bottom, feed back to R at
/// the same feed, rapid out.
fn program(x: f64, dwell: f64) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(BAR + 5.0, 0.0, HOP));
    b.push(MoveKind::Rapid, Point3::new(x, 0.0, HOP), None);
    b.push(MoveKind::Rapid, Point3::new(x, 0.0, R), None);
    let fm = b.moves_len();
    b.push(MoveKind::Plunge, Point3::new(x, 0.0, BOTTOM), Some(FEED));
    b.push(MoveKind::Feed, Point3::new(x, 0.0, R), Some(FEED));
    b.push_cycle(Cycle {
        kind: CycleKind::Ream,
        xy: Point2::new(x, 0.0),
        r_z: R,
        bottom_z: BOTTOM,
        entry_z: 0.0,
        peck: 0.0,
        dwell,
        first_move: fm,
    });
    b.push(MoveKind::Rapid, Point3::new(x, 0.0, HOP), None);
    b.finish()
}

fn stream_of(x: f64, dwell: f64) -> String {
    let d = Dump {
        op_name: "Ream",
        tool_number: 3,
        tool_name: "REAMER 8H7",
        tool_diameter: 8.0,
        rpm: 400.0,
        clearance_z: HOP,
        retract_z: HOP,
        top_z: 0.0,
        wcs: 1,
        stock: [-BAR, BAR, -BAR, BAR],
        stock_top_z: 0.0,
        stock_turn_radius: Some(BAR),
        ..Dump::default()
    };
    cnc::serialize(&dump(&program(x, dwell), &d))
}

fn run(stem: &str, mch: &str, stream: &str) -> Result<engine::PostOutput, String> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let mch: serde_json::Value =
        serde_json::from_str(&read(&format!("assets/machines/{mch}"))).unwrap_or_else(|e| panic!("{mch}: {e}"));
    engine::run(&cps, &mch, stream).map_err(|e| e.to_string())
}

/// The on-centre ream, posted clean (no warning: the gate counts them).
fn post(stem: &str, mch: &str) -> String {
    let out = run(stem, mch, &stream_of(0.0, 0.0)).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: the gate counts warnings: {:?}", out.warnings);
    out.program
}

/// The program's block lines, comments dropped.
fn blocks(program: &str) -> Vec<&str> {
    program.lines().filter(|l| !l.trim_start().starts_with(['(', ';', '%'])).collect()
}

fn has_word(line: &str, w: &str) -> bool {
    line.split_whitespace().any(|x| x == w)
}

/// The canned hole-cycle words the program writes, in order.
fn canned(program: &str) -> Vec<&'static str> {
    let mut seen = Vec::new();
    for l in blocks(program) {
        for w in CANNED_WORDS {
            if has_word(l, w) {
                seen.push(w);
            }
        }
    }
    seen
}

#[test]
fn every_lathe_post_is_covered() {
    // "Done means: every lathe post either posts a ream or refuses it":
    // a lathe post added later must join LATHE_POSTS (and get its pin).
    let dir = repo("assets/posts");
    let mut shipped: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "cps"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .filter(|s| s.contains("lathe"))
        .collect();
    shipped.sort();
    let covered: Vec<&str> = LATHE_POSTS.iter().map(|(s, _)| *s).collect();
    assert_eq!(shipped, covered, "a lathe post without a ream pin");
}

#[test]
fn the_stream_names_the_axial_ream() {
    let s = stream_of(0.0, 0.0);
    assert!(s.contains("CYCLE REAM X 0 Y 0 Z -12000 R 2000"), "the cycle word rides the stream:\n{s}");
    let recs = cnc::parse(&s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&recs), s, "parse . serialize is the identity");
}

#[test]
fn fanuc_lathe_posts_g85_and_no_other_cycle() {
    let prog = post("fanuc-lathe", "lathe.mch");
    assert_eq!(canned(&prog), ["G85"], "a ream posts as one G85, never another cycle:\n{prog}");
    let lines = blocks(&prog);
    let at = lines.iter().position(|l| has_word(l, "G85")).unwrap_or_else(|| panic!("no G85:\n{prog}"));
    let g85 = lines[at];
    assert!(has_word(g85, "Z-12."), "the cycle states the hole's bottom: {g85}");
    assert!(g85.split_whitespace().any(|w| w.starts_with('F')), "the cycle states its own feed: {g85}");
    assert!(!g85.split_whitespace().any(|w| w.starts_with('P')), "a ream with no dwell carries no P word: {g85}");
    assert!(!g85.split_whitespace().any(|w| w.starts_with('Q')), "a ream carries no peck word: {g85}");
    // The cycle is read with the tool seated on centre at R.
    assert_eq!(lines[at - 1], "G00 X0. Z2.", "the tool is seated on R on the centreline before G85:\n{prog}");
    // The hole's expanded chain is the cycle's: nothing between G85 and
    // its G80, and the exit rapid posts after the cancel.
    assert_eq!(lines[at + 1], "G80", "the chain is suppressed and the cycle is cancelled:\n{prog}");
    assert_eq!(lines[at + 2], "G00 X0. Z10.", "the exit posts after G80:\n{prog}");
    assert!(!lines.iter().any(|l| has_word(l, "G01")), "no chain move doubles the cycle:\n{prog}");
}

#[test]
fn tormach_lathe_feeds_in_and_feeds_out_of_the_reamed_hole() {
    let prog = post("tormach-lathe", "tormach-15l-slant-pro.mch");
    assert_eq!(canned(&prog), [] as [&str; 0], "PathPilot's lathe has no canned cycle:\n{prog}");
    assert!(!blocks(&prog).iter().any(|l| has_word(l, "G80")), "no cycle to cancel:\n{prog}");
    let lines = blocks(&prog);
    let bottoms: Vec<usize> = (0..lines.len()).filter(|&i| has_word(lines[i], "Z-12")).collect();
    assert_eq!(bottoms.len(), 1, "one hole reaches the bottom:\n{prog}");
    let i = bottoms[0];
    assert!(has_word(lines[i], "G1") && has_word(lines[i], "X0"), "the tool feeds in on the centreline: {}", lines[i]);
    assert!(has_word(lines[i - 1], "Z2") && has_word(lines[i - 1], "G0"), "the feed starts at R:\n{prog}");
    // The retract from the bottom to R is a feed at the cutting feed,
    // never a rapid over the reamed wall.
    let out = lines[i + 1];
    assert!(has_word(out, "G1") && has_word(out, "X0") && has_word(out, "Z2"), "the retract feeds back to R: {out}");
    let f = |l: &str| l.split_whitespace().find(|w| w.starts_with('F')).map(str::to_string);
    assert!(f(lines[i]).is_some(), "the feed in states F: {}", lines[i]);
    assert_eq!(f(out), f(lines[i]), "the feed out runs at the feed in's F:\n{prog}");
    assert!(!blocks(&prog).iter().any(|l| has_word(l, "G4")), "a ream with no dwell posts no G4:\n{prog}");
}

#[test]
fn a_ream_with_a_dwell_keeps_its_cycle_on_both_posts() {
    // The dwell is the cycle's own: G85 P (ms) on the Fanuc lathe, a G4
    // (seconds) at the floor of the PathPilot chain. Never another cycle.
    let s = stream_of(0.0, 0.5);
    let out = run("fanuc-lathe", "lathe.mch", &s).unwrap_or_else(|e| panic!("fanuc-lathe: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(canned(&out.program), ["G85"], "{}", out.program);
    assert!(out.program.lines().any(|l| has_word(l, "G85") && has_word(l, "P500")), "{}", out.program);

    let out = run("tormach-lathe", "tormach-15l-slant-pro.mch", &s).unwrap_or_else(|e| panic!("tormach-lathe: {e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(canned(&out.program), [] as [&str; 0], "{}", out.program);
    let lines = blocks(&out.program);
    let i = lines.iter().position(|l| has_word(l, "Z-12")).unwrap_or_else(|| panic!("no bottom:\n{}", out.program));
    assert_eq!(lines[i + 1], "G4 P0.5", "the dwell sits at the floor, before the feed out:\n{}", out.program);
    assert!(has_word(lines[i + 2], "G1") && has_word(lines[i + 2], "Z2"), "then the feed out:\n{}", out.program);
}

#[test]
fn a_ream_off_the_spindle_axis_is_refused_by_name() {
    // A 2-axis lathe reams on centre only: no cycle word, and a warning
    // that names the reason (a program with a warning is never released).
    for (stem, mch) in LATHE_POSTS {
        let out = match run(stem, mch, &stream_of(6.0, 0.0)) {
            Ok(out) => out,
            Err(e) => {
                assert!(e.contains("off the spindle axis"), "{stem}: the refusal names the reason: {e}");
                continue;
            }
        };
        assert!(
            out.warnings.iter().any(|w| w.contains("drilling cycle off the spindle axis")),
            "{stem}: the refusal names the reason: {:?}",
            out.warnings
        );
        assert_eq!(canned(&out.program), [] as [&str; 0], "{stem}: no cycle is posted:\n{}", out.program);
    }
}

#[test]
fn the_lathe_posts_post_the_ream_byte_exact() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (stem, mch) in LATHE_POSTS {
        let out = recorded::with_synthetic_marker(stem, &post(stem, mch));
        let rel = format!("crates/post/tests/golden/ream/{stem}.nc");
        if write {
            let p = repo(&rel);
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{rel}: {e}"));
            }
            std::fs::write(p, &out).unwrap_or_else(|e| panic!("{rel}: {e}"));
            continue;
        }
        assert_eq!(out, read(&rel), "{stem}: {rel} is stale; regenerate with WRITE_GOLDEN=1");
    }
}

//! #1445: the reaming cycle through every shipped mill post. A post with
//! a canned feed-in/feed-out cycle writes G85 (RS-274/NGC's G85 - NIST IR
//! 6556, The NIST RS274NGC Interpreter v3; LinuxCNC's G-code reference),
//! never a different cycle; a post without one posts the expanded chain
//! (feed in, FEED out - never a rapid over the reamed wall); a post with
//! no drilling at all refuses by name. Goldens for the six posts the
//! issue names sit in `tests/golden/ream/` - our mapping's regression
//! record, not a reading taken on a control.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test ream_cycle`
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, haas.cps, mazak.cps,
//! doosan-dnm.cps, sinumerik.cps, tormach.cps, grblhal.cps, heidenhain.cps,
//! tinyg.cps, carvera.cps (their cycle maps), camcraft_post::post::cnc
//! (the REAM cycle word), camcraft_post::post::engine.

mod recorded;

use camcraft_post::cl::{Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::{Path, PathBuf};

/// Posts that write a canned cycle word for drilling.
const CANNED: [&str; 7] = ["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik", "tormach", "grblhal"];
/// Posts that write the expanded chain for a ream.
const EXPANDED: [&str; 3] = ["heidenhain", "tinyg", "carvera"];
/// Posts that write no drilling at all.
const REFUSING: [&str; 3] = ["sinker-iso", "wire-iso", "tormach-plasma"];
/// The posts #1445 names, each with a golden.
const GOLDEN: [&str; 6] = ["fanuc", "haas", "heidenhain", "sinumerik", "grblhal", "tormach"];

const R: f64 = 2.0;
const HOP: f64 = 10.0;
const BOTTOM: f64 = -12.0;
const FEED: f64 = 150.0;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// Two reamed holes, the chain the drilling generator writes for
/// `CycleKind::Ream` (crates/motion/src/drilling/cycle.rs): rapid to R,
/// feed to the bottom, feed back to R at the same feed, hop.
fn program() -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, HOP));
    for (i, (x, y)) in [(10.0, 10.0), (30.0, 10.0)].into_iter().enumerate() {
        if i > 0 {
            b.push(MoveKind::Rapid, Point3::new(x, y, HOP), None);
        }
        b.push(MoveKind::Rapid, Point3::new(x, y, R), None);
        let fm = b.moves_len();
        b.push(MoveKind::Plunge, Point3::new(x, y, BOTTOM), Some(FEED));
        b.push(MoveKind::Feed, Point3::new(x, y, R), Some(FEED));
        b.push_cycle(Cycle {
            kind: CycleKind::Ream,
            xy: Point2::new(x, y),
            r_z: R,
            bottom_z: BOTTOM,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: fm,
        });
        b.push(MoveKind::Rapid, Point3::new(x, y, HOP), None);
    }
    b.finish()
}

fn stream() -> String {
    let d = Dump {
        op_name: "Ream",
        tool_number: 3,
        tool_name: "REAMER 8H7",
        tool_diameter: 8.0,
        rpm: 400.0,
        clearance_z: HOP,
        retract_z: HOP,
        top_z: 0.0,
        ..Dump::default()
    };
    cnc::serialize(&dump(&program(), &d))
}

fn mch() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn run(stem: &str) -> Result<engine::PostOutput, String> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &mch(), &stream()).map_err(|e| e.to_string())
}

fn post(stem: &str) -> String {
    let out = run(stem).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: the gate counts warnings: {:?}", out.warnings);
    out.program
}

/// Whitespace-split words of the program, comments dropped.
fn words(program: &str) -> Vec<String> {
    program
        .lines()
        .filter(|l| !l.trim_start().starts_with(['(', ';']))
        .flat_map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

#[test]
fn the_stream_names_the_ream_and_reads_back() {
    let s = stream();
    assert!(s.contains("CYCLE REAM X 10000 Y 10000 Z -12000 R 2000"), "the cycle word rides the stream:\n{s}");
    let recs = cnc::parse(&s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&recs), s, "parse . serialize is the identity");
    let kinds: Vec<CycleKind> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Cycle(c) => Some(c.kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [CycleKind::Ream, CycleKind::Ream]);
}

#[test]
fn canned_posts_write_g85_and_no_other_cycle() {
    for stem in CANNED {
        let ws = words(&post(stem));
        let has = |g: &str| ws.iter().any(|w| w == g);
        assert!(has("G85"), "{stem}: a ream posts as G85: {ws:?}");
        for other in ["G81", "G82", "G83", "G73", "G84", "G86", "G89"] {
            assert!(!has(other), "{stem}: a ream must never post as {other}: {ws:?}");
        }
        assert!(has("G80"), "{stem}: the cycle is cancelled: {ws:?}");
        let g85 = post(stem).lines().find(|l| l.split_whitespace().any(|w| w == "G85")).map(str::to_string);
        let g85 = g85.unwrap_or_default();
        assert!(
            !g85.split_whitespace().any(|w| w.starts_with('P')),
            "{stem}: G85 carries no dwell word: {g85}"
        );
    }
}

#[test]
fn expanding_posts_feed_out_of_the_reamed_hole() {
    for stem in EXPANDED {
        let prog = post(stem);
        let ws = words(&prog);
        for canned in ["G85", "G81", "G86", "G89"] {
            assert!(!ws.iter().any(|w| w == canned), "{stem}: no canned {canned} in an expanded post: {prog}");
        }
        // The retract from the bottom to R is a feed (G1), never a rapid:
        // the line after the one at the bottom moves to R on G1/G01.
        let lines: Vec<&str> = prog.lines().collect();
        let at = |l: &str, z: &str| l.split_whitespace().any(|w| w == format!("Z{z}") || w == format!("Z+{z}"));
        let bottoms: Vec<usize> = (0..lines.len()).filter(|&i| at(lines[i], "-12")).collect();
        assert_eq!(bottoms.len(), 2, "{stem}: two holes reach the bottom:\n{prog}");
        for i in bottoms {
            let next = lines.get(i + 1).copied().unwrap_or("");
            assert!(at(next, "2"), "{stem}: the move after the bottom returns to R:\n{prog}");
            // The bottom line feeds (G1 modal); the retract may restate it
            // or ride it, but never switches to a rapid.
            assert!(
                !next.split_whitespace().any(|w| w == "G0" || w == "G00"),
                "{stem}: the ream retract is a feed, not a rapid: {next}"
            );
        }
    }
}

#[test]
fn posts_without_drilling_refuse_a_ream() {
    for stem in REFUSING {
        match run(stem) {
            Err(_) => {}
            Ok(out) => {
                assert!(!out.warnings.is_empty(), "{stem}: a ream must be refused by name, not dropped");
                assert!(!words(&out.program).iter().any(|w| w == "G85"), "{stem}");
            }
        }
    }
}

#[test]
fn the_named_posts_post_the_ream_byte_exact() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for stem in GOLDEN {
        let out = recorded::with_synthetic_marker(stem, &post(stem));
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

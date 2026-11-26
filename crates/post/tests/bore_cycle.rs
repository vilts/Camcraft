//! #1445 slice 2: the boring cycle (feed in, spindle stop, rapid out)
//! through every shipped mill post. A post with the canned cycle writes
//! G86 (RS-274/NGC's G86 - NIST IR 6556, The NIST RS274NGC Interpreter
//! v3; LinuxCNC's G-code reference), never a different cycle. A post
//! without canned cycles and without a spindle stop (heidenhain, tinyg,
//! carvera) refuses a bore by name. A post with no drilling refuses by name.
//! A bore dwell is posted only where the G86
//! format has a P word (LinuxCNC/PathPilot and grblHAL, P in seconds,
//! writing P0 when the dwell is 0);
//! the Fanuc-family posts and Sinumerik refuse it by name, because their
//! G86 has no dwell word (the published Fanuc and Haas G86 formats are
//! X Y Z R F). Goldens for the five canned posts the issue names sit in
//! `tests/golden/bore/` - our mapping's regression record, not a reading
//! taken on a control.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test bore_cycle`
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, haas.cps, mazak.cps,
//! doosan-dnm.cps, sinumerik.cps, tormach.cps, grblhal.cps, linuxcnc.cps, heidenhain.cps,
//! tinyg.cps, carvera.cps, mach3.cps (their BORE cycle maps and the G86 dwell door),
//! camcraft_post::post::cnc (the BORE cycle word), camcraft_post::post::engine.

mod recorded;

use camcraft_post::cl::{Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::{Path, PathBuf};

/// Posts that write G86 and have no dwell word on it.
const NO_DWELL: [&str; 5] = ["fanuc", "haas", "mazak", "doosan-dnm", "sinumerik"];
/// Posts that write G86 with a dwell P in seconds.
const SECONDS: [&str; 4] = ["tormach", "grblhal", "linuxcnc", "mach3"];
/// Posts that require a non-negative P on every G86 cycle block (#2362).
const ZERO_DWELL_P0: [&str; 3] = ["tormach", "grblhal", "linuxcnc"];
/// Posts that refuse a bore: they cannot stop the spindle before the rapid retract.
const EXPANDED: [&str; 4] = ["heidenhain", "tinyg", "grbl", "carvera"];
/// Posts that write no drilling at all.
const REFUSING: [&str; 3] = ["sinker-iso", "wire-iso", "tormach-plasma"];
/// The posts #1445 names, each with a golden.
const GOLDEN: [&str; 5] = ["fanuc", "haas", "sinumerik", "grblhal", "tormach"];

const R: f64 = 2.0;
const HOP: f64 = 10.0;
const BOTTOM: f64 = -12.0;
const FEED: f64 = 120.0;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// Two bored holes, the chain the drilling generator writes for
/// `CycleKind::Bore` (crates/motion/src/drilling/cycle.rs): rapid to R,
/// feed to the bottom, rapid back to R, hop.
fn program_dwells(dwells: &[f64]) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, HOP));
    for (i, &dwell) in dwells.iter().enumerate() {
        let (x, y) = (10.0 + 20.0 * i as f64, 10.0);
        if i > 0 {
            b.push(MoveKind::Rapid, Point3::new(x, y, HOP), None);
        }
        b.push(MoveKind::Rapid, Point3::new(x, y, R), None);
        let fm = b.moves_len();
        b.push(MoveKind::Plunge, Point3::new(x, y, BOTTOM), Some(FEED));
        b.push(MoveKind::Rapid, Point3::new(x, y, R), None);
        b.push_cycle(Cycle {
            kind: CycleKind::Bore,
            xy: Point2::new(x, y),
            r_z: R,
            bottom_z: BOTTOM,
            entry_z: 0.0,
            peck: 0.0,
            dwell,
            first_move: fm,
        });
        b.push(MoveKind::Rapid, Point3::new(x, y, HOP), None);
    }
    b.finish()
}

fn stream_dwells(dwells: &[f64]) -> String {
    let d = Dump {
        op_name: "Bore",
        tool_number: 4,
        tool_name: "BORING BAR 12",
        tool_diameter: 12.0,
        rpm: 900.0,
        clearance_z: HOP,
        retract_z: HOP,
        top_z: 0.0,
        ..Dump::default()
    };
    cnc::serialize(&dump(&program_dwells(dwells), &d))
}

fn stream(dwell: f64) -> String {
    stream_dwells(&[dwell, dwell])
}

fn mch() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn run_dwells(stem: &str, dwells: &[f64]) -> Result<engine::PostOutput, String> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &mch(), &stream_dwells(dwells)).map_err(|e| e.to_string())
}

fn run(stem: &str, dwell: f64) -> Result<engine::PostOutput, String> {
    run_dwells(stem, &[dwell, dwell])
}

fn post_dwells(stem: &str, dwells: &[f64]) -> String {
    let out = run_dwells(stem, dwells).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: the gate counts warnings: {:?}", out.warnings);
    out.program
}

fn post(stem: &str, dwell: f64) -> String {
    post_dwells(stem, &[dwell, dwell])
}

/// Whitespace-split words of the program, comments dropped.
fn words(program: &str) -> Vec<String> {
    program
        .lines()
        .filter(|l| !l.trim_start().starts_with(['(', ';']))
        .flat_map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

fn g86_lines(program: &str) -> Vec<String> {
    program
        .lines()
        .filter(|l| l.split_whitespace().any(|w| w == "G86"))
        .map(str::to_string)
        .collect()
}

fn g86_line(program: &str) -> String {
    g86_lines(program).into_iter().next().unwrap_or_default()
}

#[test]
fn the_stream_names_the_bore_and_reads_back() {
    let s = stream(0.0);
    assert!(s.contains("CYCLE BORE X 10000 Y 10000 Z -12000 R 2000"), "the cycle word rides the stream:\n{s}");
    let recs = cnc::parse(&s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&recs), s, "parse . serialize is the identity");
    let kinds: Vec<CycleKind> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Cycle(c) => Some(c.kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [CycleKind::Bore, CycleKind::Bore]);
}

#[test]
fn canned_posts_write_g86_and_no_other_cycle() {
    for stem in NO_DWELL.into_iter().chain(SECONDS) {
        let prog = post(stem, 0.0);
        let ws = words(&prog);
        let has = |g: &str| ws.iter().any(|w| w == g);
        assert!(has("G86"), "{stem}: a bore posts as G86: {ws:?}");
        for other in ["G81", "G82", "G83", "G73", "G84", "G85", "G89"] {
            assert!(!has(other), "{stem}: a bore must never post as {other}: {ws:?}");
        }
        assert!(has("G80"), "{stem}: the cycle is cancelled: {ws:?}");
        for line in g86_lines(&prog) {
            if ZERO_DWELL_P0.contains(&stem) {
                assert!(
                    line.split_whitespace().any(|w| w == "P0"),
                    "{stem}: zero-dwell G86 requires P0: {line}"
                );
            } else {
                assert!(
                    !line.split_whitespace().any(|w| w.starts_with('P')),
                    "{stem}: no dwell, no P: {line}"
                );
            }
        }
    }
}

fn assert_zero_dwell_g86_has_p0(stem: &str) {
    let prog = post(stem, 0.0);
    let lines = g86_lines(&prog);
    assert!(!lines.is_empty(), "{stem}: expected at least one G86 block:\n{prog}");
    for line in &lines {
        let p_words: Vec<&str> = line.split_whitespace().filter(|w| w.starts_with('P')).collect();
        assert_eq!(p_words, ["P0"], "{stem}: zero-dwell G86 must emit P0 on every G86 block: {line}");
    }
    let mixed = post_dwells(stem, &[0.5, 0.0]);
    let mixed_lines = g86_lines(&mixed);
    assert_eq!(mixed_lines.len(), 2, "{stem}: expected two G86 blocks:\n{mixed}");
    assert!(
        mixed_lines[0].split_whitespace().any(|w| w == "P0.5"),
        "{stem}: first G86 must emit P0.5: {}",
        mixed_lines[0]
    );
    assert!(
        mixed_lines[1].split_whitespace().any(|w| w == "P0"),
        "{stem}: second G86 must emit P0: {}",
        mixed_lines[1]
    );
}

#[test]
fn grblhal_zero_dwell_g86_has_p0() {
    assert_zero_dwell_g86_has_p0("grblhal");
}

#[test]
fn tormach_zero_dwell_g86_has_p0() {
    assert_zero_dwell_g86_has_p0("tormach");
}

#[test]
fn a_bore_dwell_is_refused_by_name_where_g86_has_no_dwell_word() {
    for stem in NO_DWELL {
        let out = run(stem, 0.5).unwrap_or_else(|e| panic!("{stem}: runs: {e}"));
        assert!(
            out.warnings.iter().any(|w| w.contains("G86 (bore) has no dwell word") && w.contains("0.5 s")),
            "{stem}: the dwell is refused by name: {:?}",
            out.warnings
        );
        assert!(!words(&out.program).iter().any(|w| w == "G86"), "{stem}: no G86 is written without its dwell");
    }
}

#[test]
fn a_bore_dwell_posts_as_p_in_seconds_on_the_linuxcnc_family() {
    for stem in SECONDS {
        let g86 = g86_line(&post(stem, 0.5));
        assert!(g86.split_whitespace().any(|w| w == "P0.5"), "{stem}: P is seconds: {g86}");
    }
}

#[test]
fn expanding_posts_refuse_a_bore_they_cannot_stop_the_spindle_for() {
    // Heidenhain, TinyG and Carvera cannot stop the spindle before the
    // rapid retract. The expanded chain would pull a turning boring bar
    // out of the hole, so each post refuses by name and sets a counted warning that blocks the post.
    for stem in EXPANDED {
        for dwell in [0.0, 0.5] {
            let out = run(stem, dwell).unwrap_or_else(|e| panic!("{stem}: runs: {e}"));
            assert!(
                out.warnings.iter().any(|w| w.contains("bore (G86) requires a spindle stop before rapid retract")
                    && w.contains("refused, not posted")),
                "{stem}: the bore is refused by name (dwell {dwell}): {:?}",
                out.warnings
            );
            // The refusal is the counted warning (the host blocks the post
            // on it). The cycle is not expanded: no dwell block is written.
            assert!(
                !out.program.contains("FUNCTION DWELL TIME")
                    && !out.program.lines().any(|l| l.split_whitespace().any(|w| w == "G4")),
                "{stem}: the refused bore writes no dwell block:
{}",
                out.program
            );
            assert!(!words(&out.program).iter().any(|w| w == "G86"), "{stem}: no G86 is written");
        }
    }
}

#[test]
fn posts_without_drilling_refuse_a_bore() {
    for stem in REFUSING {
        match run(stem, 0.0) {
            Err(_) => {}
            Ok(out) => {
                assert!(!out.warnings.is_empty(), "{stem}: a bore must be refused by name, not dropped");
                assert!(!words(&out.program).iter().any(|w| w == "G86"), "{stem}");
            }
        }
    }
}

#[test]
fn the_named_posts_post_the_bore_byte_exact() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for stem in GOLDEN {
        let out = recorded::with_synthetic_marker(stem, &post(stem, 0.0));
        let rel = format!("crates/post/tests/golden/bore/{stem}.nc");
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

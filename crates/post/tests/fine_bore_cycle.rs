//! #1445, #1479: the fine-boring cycle through every shipped post. A fine
//! bore feeds in, dwells, stops the spindle at its orientation, shifts the
//! tip off the wall and rapids out (the published G76 fine boring canned
//! cycle - Haas Mill Operator's Manual, G76, words I and J). The shift
//! words differ by control, so only a post that declares `fineBoring`
//! writes the cycle; the engine refuses it by name for every other post,
//! and never lets the chain post as plain moves. The golden in
//! `tests/golden/fine_bore/` is our mapping's regression record, not a
//! reading taken on a control.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test fine_bore_cycle`
//!
//! Gate map (M4.4): covers assets/posts/haas.cps (G76), every other
//! assets/posts/*.cps (the refusal), camcraft_post::post::fine_bore,
//! camcraft_post::post::cnc (the FINEBORE cycle word),
//! camcraft_post::post::engine, camcraft_post::machine::judge
//! (fine_bore_gate).

mod recorded;

use camcraft_post::cl::{Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::machine::{fine_bore_gate, parse_mch, MachineCheck};
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::{Path, PathBuf};

/// The posts that write the fine-bore cycle.
const VOICED: [&str; 1] = ["haas"];

const R: f64 = 2.0;
const HOP: f64 = 10.0;
const BOTTOM: f64 = -12.0;
const FEED: f64 = 60.0;
const HOLES: [(f64, f64); 2] = [(10.0, 10.0), (30.0, 10.0)];
/// The shift: 0.3 along X, -0.4 along Y.
const SHIFT: (f64, f64) = (0.3, -0.4);

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// Every shipped post script, by stem.
fn all_posts() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(repo("assets/posts"))
        .expect("assets/posts")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "cps"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
        .collect();
    v.sort();
    v
}

/// Two fine-bored holes, the chain the drilling generator writes for
/// `CycleKind::FineBore` (crates/motion/src/drilling/cycle.rs): rapid to
/// R, feed to the bottom, shift, rapid out at the shifted XY, shift back
/// at R, hop. `shift: None` leaves the shift out (a plain bore chain under
/// a fine-bore record), which must be refused.
fn program(shift: Option<(f64, f64)>, dwell: f64) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(HOLES[0].0, HOLES[0].1, HOP));
    for (i, (x, y)) in HOLES.into_iter().enumerate() {
        if i > 0 {
            b.push(MoveKind::Rapid, Point3::new(x, y, HOP), None);
        }
        b.push(MoveKind::Rapid, Point3::new(x, y, R), None);
        let fm = b.moves_len();
        b.push(MoveKind::Plunge, Point3::new(x, y, BOTTOM), Some(FEED));
        if let Some((dx, dy)) = shift {
            b.push(MoveKind::Rapid, Point3::new(x + dx, y + dy, BOTTOM), None);
            b.push(MoveKind::Rapid, Point3::new(x + dx, y + dy, R), None);
        }
        b.push(MoveKind::Rapid, Point3::new(x, y, R), None);
        b.push_cycle(Cycle {
            kind: CycleKind::FineBore,
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

fn stream_of(t: &Toolpath) -> String {
    let d = Dump {
        op_name: "Fine bore",
        tool_number: 4,
        tool_name: "BORING BAR 20",
        tool_diameter: 20.0,
        rpm: 900.0,
        clearance_z: HOP,
        retract_z: HOP,
        top_z: 0.0,
        ..Dump::default()
    };
    cnc::serialize(&dump(t, &d))
}

fn stream() -> String {
    stream_of(&program(Some(SHIFT), 0.5))
}

fn mch() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

/// The Haas VF-2's file, with `spindle.orientation` stated as `word`
/// (JSON text) when given. The shipped file leaves it unstated.
fn mch_text(word: Option<String>) -> String {
    let text = read("assets/machines/haas-vf2.mch");
    let Some(word) = word else { return text };
    let key = r#""spindle": {"#;
    assert_eq!(text.matches(key).count(), 1, "one spindle in the VF-2 file");
    text.replace(key, &format!(r#"{key} "orientation": {word},"#))
}

fn run(stem: &str, stream: &str) -> Result<engine::PostOutput, String> {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &mch(), stream).map_err(|e| e.to_string())
}

fn post(stem: &str, stream: &str) -> String {
    let out = run(stem, stream).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: the gate counts warnings: {:?}", out.warnings);
    out.program
}

/// The program's code lines (comments dropped), each split into words.
fn blocks(program: &str) -> Vec<Vec<String>> {
    program
        .lines()
        .filter(|l| !l.trim_start().starts_with(['(', ';']))
        .map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|ws| !ws.is_empty())
        .collect()
}

#[test]
fn the_stream_names_the_fine_bore_and_reads_back() {
    let s = stream();
    assert!(s.contains("CYCLE FINEBORE X 10000 Y 10000 Z -12000 R 2000"), "the cycle word rides the stream:\n{s}");
    let recs = cnc::parse(&s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&recs), s, "parse . serialize is the identity");
    let kinds: Vec<CycleKind> = recs
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Cycle(c) => Some(c.kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [CycleKind::FineBore, CycleKind::FineBore]);
}

#[test]
fn haas_writes_g76_with_the_shift_of_the_chain() {
    let prog = post("haas", &stream());
    let bs = blocks(&prog);
    let g76: Vec<&Vec<String>> = bs.iter().filter(|b| b.iter().any(|w| w == "G76")).collect();
    assert_eq!(g76.len(), HOLES.len(), "every hole posts its own whole G76 block:\n{prog}");
    for (b, (x, _)) in g76.iter().zip(HOLES) {
        let has = |w: &str| b.iter().any(|v| v == w);
        assert!(has(&format!("X{x:.0}.")) && has("Y10."), "the hole centre: {b:?}");
        assert!(has("Z-12.") && has("R2."), "bottom and R: {b:?}");
        assert!(has("I0.3") && has("J-0.4"), "the shift words are the chain's shift: {b:?}");
        assert!(has("P500"), "the 0.5 s dwell, in whole milliseconds: {b:?}");
        assert!(has("F60."), "the cycle's own feed, with its point (#3681): {b:?}");
        assert!(!b.iter().any(|w| w.starts_with('Q')), "Q takes its direction from a control setting: never written: {b:?}");
    }
    let ws: Vec<&String> = bs.iter().flatten().collect();
    for other in ["G81", "G82", "G83", "G73", "G84", "G85", "G86", "G89"] {
        assert!(!ws.iter().any(|w| *w == other), "a fine bore must never post as {other}:\n{prog}");
    }
    assert!(ws.iter().any(|w| *w == "G80"), "the cycle is cancelled:\n{prog}");
    // The control makes the shift and the rapid out itself: the chain's
    // copies are not posted (no move to the shifted XY, none to the bottom).
    for b in bs.iter().filter(|b| !b.iter().any(|w| w == "G76")) {
        let has = |w: &str| b.iter().any(|v| v == w);
        assert!(!has("X10.3") && !has("X30.3") && !has("Y9.6"), "a chain rapid at the shifted XY was posted: {b:?}\n{prog}");
        assert!(!has("Z-12."), "a chain move to the bottom was posted: {b:?}\n{prog}");
    }
}

#[test]
fn haas_clears_the_dwell_word_when_no_dwell_is_asked() {
    let prog = post("haas", &stream_of(&program(Some(SHIFT), 0.0)));
    let g76 = prog.lines().find(|l| l.split_whitespace().any(|w| w == "G76")).unwrap_or_default();
    assert!(g76.split_whitespace().any(|w| w == "P0"), "P is modal on Haas dwell cycles: P0 clears it: {g76}");
}

#[test]
fn a_fine_bore_with_no_shift_in_its_chain_is_refused_by_name() {
    let out = run("haas", &stream_of(&program(None, 0.5))).expect("the engine runs");
    // Two doors, each by name: the engine reads no shift in the chain,
    // and the script will not write G76 without one.
    for said in ["has no shift move at the bottom of its chain", "G76 (fine bore) has no shift"] {
        assert!(out.warnings.iter().any(|w| w.contains(said)), "{said:?} missing: {:?}", out.warnings);
    }
    assert!(!out.program.contains("G76"), "no G76 with a guessed shift:\n{}", out.program);
}

#[test]
fn haas_refuses_a_fine_bore_shift_that_rounds_to_zero_in_inch_output() {
    let cps = read("assets/posts/haas.cps");
    for shift in [(0.001, 0.0), (0.0, -0.001)] {
        let s = stream_of(&program(Some(shift), 0.0));
        let out = engine::run_with(&cps, &mch(), &s, &serde_json::json!({"Units": "inch"})).expect("the engine runs");
        assert!(
            out.warnings.iter().any(|w| w.contains("G76 (fine bore) shift rounds to zero in the output unit")),
            "the unrepresentable shift is refused by name: {:?}",
            out.warnings
        );
        assert!(!out.program.contains("G76"), "no cycle with a zero output shift:\n{}", out.program);
    }
    // The guard uses the output unit, and either nonzero axis is sufficient.
    for (units, shift, word) in [("mm", (0.001, 0.0), "I0.001"), ("inch", (0.0, -0.002), "J-0.0001")] {
        let s = stream_of(&program(Some(shift), 0.0));
        let out = engine::run_with(&cps, &mch(), &s, &serde_json::json!({"Units": units})).expect("the engine runs");
        assert!(out.warnings.is_empty(), "a representable shift posts: {:?}", out.warnings);
        assert!(
            blocks(&out.program).iter().any(|b| b.iter().any(|w| w == "G76") && b.iter().any(|w| w == word)),
            "the cycle retains a nonzero output shift:\n{}",
            out.program
        );
    }
}

#[test]
fn every_other_post_refuses_a_fine_bore_by_name() {
    let posts = all_posts();
    assert!(posts.len() >= 18, "the shipped posts: {posts:?}");
    for stem in posts.iter().filter(|s| !VOICED.contains(&s.as_str())) {
        match run(stem, &stream()) {
            Err(e) => assert!(!e.is_empty(), "{stem}"),
            Ok(out) => {
                assert!(
                    out.warnings.iter().any(|w| w.contains("fine boring is not supported by this post")),
                    "{stem}: a fine bore must be refused by name: {:?}",
                    out.warnings
                );
                assert!(!out.program.split_whitespace().any(|w| w == "G76"), "{stem}: no G76 from a post that does not declare it");
            }
        }
    }
}

#[test]
fn a_machine_that_cannot_stop_oriented_refuses_a_fine_bore() {
    let path = program(Some(SHIFT), 0.0);
    let machine = |orientation: Option<bool>| {
        parse_mch(&mch_text(orientation.map(|b| b.to_string()))).unwrap_or_else(|e| panic!("{e}"))
    };
    let said = |orientation: Option<bool>| {
        let mut out = MachineCheck::default();
        fine_bore_gate(&mut out, &machine(orientation), "Fine bore", &path);
        out
    };
    let no = said(Some(false));
    assert_eq!(no.problems.len(), 1, "{:?}", no.problems);
    assert!(
        no.problems[0].contains("Fine bore: 2 fine bore cycle(s) need an oriented spindle stop"),
        "{:?}",
        no.problems
    );
    assert!(said(Some(true)).problems.is_empty(), "stated yes: posts");
    assert!(said(None).problems.is_empty(), "unstated: not limited");
    // A plain bore on the same machine is not this gate's business.
    let mut bore = path.clone();
    for c in &mut bore.cycles {
        c.kind = CycleKind::Bore;
    }
    let mut out = MachineCheck::default();
    fine_bore_gate(&mut out, &machine(Some(false)), "Bore", &bore);
    assert!(out.problems.is_empty());
}

#[test]
fn spindle_orientation_must_be_a_boolean_or_the_file_is_refused() {
    let err = parse_mch(&mch_text(Some(r#""yes""#.to_owned()))).map(|_| ()).unwrap_err();
    assert!(err.contains("spindle.orientation must be true or false"), "{err}");
}

#[test]
fn haas_posts_the_fine_bore_byte_exact() {
    let out = recorded::with_synthetic_marker("haas", &post("haas", &stream()));
    let rel = "crates/post/tests/golden/fine_bore/haas.nc";
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        let p = repo(rel);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{rel}: {e}"));
        }
        std::fs::write(p, &out).unwrap_or_else(|e| panic!("{rel}: {e}"));
        return;
    }
    assert_eq!(out, read(rel), "{rel} is stale; regenerate with WRITE_GOLDEN=1");
}

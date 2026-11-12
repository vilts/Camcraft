//! The `fanuc-lathe.cps` script suite (#926): the post binary on every recorded
//! edge-case stream (`recorded/streams/*.cnc`), its program or its named
//! refusal compared with the golden in `recorded/golden/fanuc-lathe/`. No app, no
//! toolpath generation: the inputs are recorded, and their generators keep
//! them current (`recorded_streams.rs`, app's `recorded_post_streams.rs`).
//!
//! Plus the after-operation law of a live-tool section (#1188): a stop or
//! go-home asked after a C-axis section is written at the end of THAT
//! section - after its live spindle and C axis are released - and never
//! carried into the next one.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_fanuc_lathe`
//!
//! Gate map (M4.4): covers assets/posts/fanuc-lathe.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("fanuc-lathe.cps");

use camcraft_post::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_post::geometry::Point3;
use camcraft_post::post::{self, cnc, engine, AfterOperation, Dump};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn ctx<'a>(op: &'a str, tool: u32, name: &'a str, rpm: f64) -> Dump<'a> {
    Dump {
        op_name: op,
        tool_number: tool,
        tool_name: name,
        tool_diameter: 0.0,
        rpm,
        clearance_z: 50.0,
        retract_z: 5.0,
        top_z: 0.0,
        wcs: "G54",
        stock: [-50.0, 50.0, -50.0, 50.0],
        ..Dump::default()
    }
}

fn face() -> Toolpath {
    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 15.0), None);
    b.push(MoveKind::Feed, p(0.0, 0.0, 15.0), Some(150.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 16.0), None);
    b.finish()
}

fn cross_drill() -> Toolpath {
    let mut b = PathBuilder::new(p(12.0, 0.0, 16.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    b.push(MoveKind::Feed, p(8.0, 0.0, 5.0), Some(200.0));
    b.push(MoveKind::Rapid, p(12.0, 0.0, 5.0), None);
    let mut t = b.finish();
    for m in &mut t.moves {
        m.rotary = Some(0.0);
    }
    t
}

/// A turning section, a live-tool section asking `after`, then a second
/// turning section - posted on the test-bench mill-turn (M1001-M1004).
fn post_live_between(after: AfterOperation) -> String {
    let mut live = ctx("Cross drill", 2, "DR-6", 3000.0);
    live.tool_diameter = 6.0;
    live.after = after;
    let (a, b, c) = (face(), cross_drill(), face());
    let sections = [
        (&a, ctx("Face one", 1, "FACE-12", 1200.0)),
        (&b, live),
        (&c, ctx("Face two", 3, "FACE-8", 1500.0)),
    ];
    let recs: Vec<cnc::Record> = sections.iter().flat_map(|(t, d)| post::dump(t, d)).collect();
    let stream = cnc::serialize(&recs);
    let path = recorded::here().join("machines/millturn-live.mch");
    let text = std::fs::read_to_string(&path).expect("read the test mill-turn");
    let parsed = camcraft_post::machine::parse_mch(&text).expect("parse the test mill-turn");
    let mch = post::machine::mirror(&parsed);
    let cps = std::fs::read_to_string(recorded::repo().join("assets/posts/fanuc-lathe.cps")).expect("read the script");
    let out = engine::run(&cps, &mch, &stream).expect("the post runs");
    assert!(out.warnings.is_empty(), "the stream posts clean: {:?}\n{}", out.warnings, out.program);
    out.program
}

fn lines(prog: &str) -> Vec<&str> {
    prog.lines().map(str::trim).collect()
}

fn at(ls: &[&str], line: &str) -> usize {
    ls.iter().position(|l| *l == line).unwrap_or_else(|| panic!("no `{line}` block in\n{}", ls.join("\n")))
}

/// The asked words sit between the live section's release (live spindle
/// off M1004, C off M1002) and the next section's first block, and appear
/// exactly once in the program.
fn assert_after_live(after: AfterOperation, words: &[&str]) {
    let prog = post_live_between(after);
    let ls = lines(&prog);
    let spindle_off = at(&ls, "M1004");
    let c_off = at(&ls, "M1002");
    let next = at(&ls, "(OPERATION: Face two - G54)");
    assert!(spindle_off < c_off, "live spindle stops before C is released:\n{prog}");
    let mut last = c_off;
    for w in words {
        let i = at(&ls, w);
        assert!(i > last && i < next, "`{w}` belongs after the live release, before the next section:\n{prog}");
        assert_eq!(ls.iter().filter(|l| *l == w).count(), 1, "`{w}` is posted once, never carried over:\n{prog}");
        last = i;
    }
}

#[test]
fn an_optional_stop_after_a_live_section_posts_at_its_end() {
    assert_after_live(AfterOperation::OptionalStop, &["M01"]);
}

#[test]
fn a_program_stop_after_a_live_section_posts_at_its_end() {
    assert_after_live(AfterOperation::ProgramStop, &["M00"]);
}

#[test]
fn a_go_home_after_a_live_section_posts_at_its_end() {
    assert_after_live(AfterOperation::GoHome, &["G28 U0", "G28 W0"]);
}

#[test]
fn no_stop_asked_after_a_live_section_posts_no_stop() {
    let prog = post_live_between(AfterOperation::None);
    let ls = lines(&prog);
    for w in ["M01", "M00", "G28 U0", "G28 W0"] {
        assert!(!ls.contains(&w), "`{w}` posted with nothing asked:\n{prog}");
    }
    // The program-end home stays the one trip home.
    assert_eq!(ls.iter().filter(|l| l.starts_with("G28")).count(), 1, "{prog}");
}

//! #995: a touch probe section starts no coolant. Every built-in mill
//! post that opens a section with a stated speed of 0 (a touch probe,
//! #523) writes no coolant-on word for it - no M7 / M8 / M88 - and so no
//! M9 / M89 for it either, whatever the operation's coolant ask and
//! whether or not the mounted machine states coolant codes. A cutting
//! section on the same machine still posts its coolant (the control).
//!
//! Provenance: touch-probe practice (a stylus and the faces it measures
//! are kept dry and clean); RS-274 / ISO 6983 M07 / M08 / M09; our own
//! synthetic program and the bore-centre probe of tests/probe_op.rs.
//!
//! Gate map (M4.4): covers post::dump, post::engine and every mill post
//! script's preamble coolant (assets/posts/*.cps startPreamble, lathe
//! excluded).

use camcraft_app::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::CoolantMode;
use camcraft_app::post::{self, cnc, dump, engine, Dump};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use std::path::Path;

/// Every built-in mill post, the machine file it is posted against and
/// the coolant word a cutting section on that machine starts.
const MILL_POSTS: &[(&str, &str, &str)] = &[
    ("fanuc", "3axis-vmc", "M8"),
    ("haas", "3axis-vmc", "M8"),
    ("tormach", "3axis-vmc", "M8"),
    ("sinumerik", "3axis-vmc", "M8"),
    ("heidenhain", "3axis-vmc", "M8"),
    ("mazak", "3axis-vmc", "M8"),
    ("doosan-dnm", "doosan-dnm", "M8"),
    ("carvera", "makera-carvera", "M7"),
];

/// Coolant on / off words of every shipped mill dialect.
const COOLANT_WORDS: &[&str] = &["M7", "M07", "M8", "M08", "M88", "M89", "M9", "M09"];

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// The shipped machine file as a post sees it: a script-shaped file
/// (`kinematics.static`) as it is, a kinematic-tree file parsed and
/// mirrored the way the app hands it over - either way its coolant list
/// must reach the script, or the "with a machine" cases prove nothing.
fn machine(stem: &str) -> serde_json::Value {
    let text = read(&format!("assets/machines/{stem}.mch"));
    let raw: serde_json::Value = serde_json::from_str(&text).expect("machine json");
    let v = if raw.pointer("/kinematics/static").is_some() {
        raw
    } else {
        post::machine::mirror(&parse_mch(&text).expect("machine parses"))
    };
    let cs = v.pointer("/kinematics/static/head/toolStation/coolants").and_then(|c| c.as_array());
    assert!(cs.is_some_and(|c| !c.is_empty()), "{stem}: the posted machine states coolants: {v}");
    v
}

/// A plain approach and one feed: enough for a preamble to open.
fn program() -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 50.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 25.0), None);
    b.push(MoveKind::Feed, Point3::new(10.0, 10.0, 15.0), Some(300.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.finish()
}

fn stream(rpm: f64, coolant: CoolantMode) -> String {
    let d = Dump {
        op_name: "Probe1",
        tool_number: 99,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm,
        clearance_z: 50.0,
        retract_z: 50.0,
        coolant,
        ..Dump::default()
    };
    cnc::serialize(&dump(&program(), &d))
}

fn post_with(stem: &str, mch: &serde_json::Value, stream: &str) -> engine::PostOutput {
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, mch, stream).unwrap_or_else(|e| panic!("{stem}: must post: {e}"))
}

/// The coolant words a program writes, comments and Heidenhain `*` ends
/// aside.
fn coolant_words(prog: &str) -> Vec<String> {
    prog.lines()
        .filter(|l| !(l.trim_start().starts_with('(') || l.trim_start().starts_with(';')))
        .flat_map(|l| l.split_whitespace().map(|w| w.trim_end_matches('*').to_string()).collect::<Vec<_>>())
        .filter(|w| COOLANT_WORDS.contains(&w.as_str()))
        .collect()
}

#[test]
fn a_probe_section_starts_no_coolant_on_a_machine_that_states_coolants() {
    for &(stem, mch, _) in MILL_POSTS {
        let out = post_with(stem, &machine(mch), &stream(0.0, CoolantMode::Flood));
        let words = coolant_words(&out.program);
        assert!(words.is_empty(), "{stem} on {mch}: a probe section posts coolant {words:?}:\n{}", out.program);
    }
}

#[test]
fn a_probe_section_starts_no_coolant_without_a_machine() {
    for &(stem, _, _) in MILL_POSTS {
        let out = post_with(stem, &serde_json::Value::Null, &stream(0.0, CoolantMode::Flood));
        let words = coolant_words(&out.program);
        assert!(words.is_empty(), "{stem}: a machine-less probe section posts coolant {words:?}:\n{}", out.program);
    }
}

/// An explicit ask on a probe op (the op page offers the row) is not
/// honoured and not refused: the probe stays dry. Tormach reads no
/// coolant ask at all (the engine refuses a non-Flood ask by name), so
/// it sits this one out.
#[test]
fn a_probe_section_ignores_an_explicit_coolant_ask() {
    for &(stem, mch, _) in MILL_POSTS.iter().filter(|p| p.0 != "tormach") {
        for ask in [CoolantMode::Mist, CoolantMode::Air, CoolantMode::ThroughTool] {
            let out = post_with(stem, &machine(mch), &stream(0.0, ask));
            let words = coolant_words(&out.program);
            assert!(words.is_empty(), "{stem}: a probe section asking {ask:?} posts coolant {words:?}:\n{}", out.program);
            assert!(
                !out.warnings.iter().any(|w| w.contains("coolant")),
                "{stem}: a probe section's {ask:?} ask is refused: {:?}",
                out.warnings
            );
        }
    }
}

/// The control: the gate is the probe's, not the post's - a cutting
/// section on the same machine still switches its coolant on and off.
#[test]
fn a_cutting_section_still_starts_its_coolant() {
    for &(stem, mch, on) in MILL_POSTS {
        let ask = if stem == "carvera" { CoolantMode::Air } else { CoolantMode::Flood };
        let out = post_with(stem, &machine(mch), &stream(8000.0, ask));
        let words = coolant_words(&out.program);
        assert!(words.iter().any(|w| w == on), "{stem}: a cutting section posts no {on} ({:?}):\n{}", out.warnings, out.program);
        assert!(words.iter().any(|w| w == "M9"), "{stem}: a cutting section posts no M9:\n{}", out.program);
    }
}

/// The issue's own case: the bore-centre probe (TOUCH / FOUND records)
/// through the Doosan DNM post, which posted M8 for it even with no
/// machine, and on its own machine file (Flood M8 listed).
#[test]
fn a_bore_centre_probe_posts_no_coolant_on_doosan_dnm() {
    let p = ProbeParams {
        mode: ProbeMode::Bore,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: Point3::new(30.0, 30.0, 20.0),
        radius: 10.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    };
    let t = probe_cycle(&p).expect("probe cycle");
    let d = Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs: 1,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs: Some(1), tol_mm: 0.5 }),
        ..Dump::default()
    };
    let s = cnc::serialize(&dump(&t, &d));
    assert!(s.contains("TOUCH"), "the stream carries touch records:\n{s}");
    for mch in [serde_json::Value::Null, machine("doosan-dnm")] {
        let out = post_with("doosan-dnm", &mch, &s);
        let words = coolant_words(&out.program);
        assert!(words.is_empty(), "doosan-dnm: the bore probe posts coolant {words:?}:\n{}", out.program);
    }
}

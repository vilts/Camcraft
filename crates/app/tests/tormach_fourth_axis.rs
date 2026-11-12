//! #785: the Tormach 4th axis - the 770MX + microARC 4 machine file states
//! an A rotary about X, and the PathPilot post indexes it between
//! operations (G53 G0 Z up, then G0 A alone), writes each indexed section
//! in the turned frame, and refuses by name what it cannot state: a tilt
//! the A axis cannot reach, an A past travel, an A that changes inside an
//! operation (simultaneous 4-axis, unconfirmed on PathPilot), an indexed
//! arc, and a tilt on a machine with no A about X.
//! Provenance: our own tests over our own machine file and post; the
//! machine facts from Tormach's TD10749 (microARC 4 owner's guide) and the
//! frame law from the LinuxCNC reference (rotary positions in degrees).
//!
//! Regenerate the golden after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test tormach_fourth_axis`
//!
//! Gate map (M4.4): covers post::post (tormach.cps), post::machine (rotary
//! mirror), app::posting (MACHINES registry, post_setup).

use camcraft_app::cl::MoveKind;
use camcraft_app::machine::{Kind, Machine, kind, parse_mch};
use camcraft_app::post::{cnc, engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ARC4: &str = "Tormach 770MX + microARC 4";
const GOLDEN: &str = "crates/app/tests/golden/nc/tormach/index_4face_microarc4.nc";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn shipped(name: &str) -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} is not a shipped machine"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn mirrored(name: &str) -> Value {
    machine::mirror(&shipped(name))
}

fn run(mch: &Value, stream: &str) -> engine::PostOutput {
    let cps = read("assets/posts/tormach.cps");
    engine::run(&cps, mch, stream).unwrap_or_else(|e| panic!("the engine runs: {e}"))
}

fn mv(kind: MoveKind, p: [f64; 3], feed: Option<f64>, abc: Option<[f64; 3]>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec {
        kind,
        x: p[0],
        y: p[1],
        z: p[2],
        feed,
        center: None,
        ccw: false,
        plane: Default::default(),
        abc,
    })
}

const ORIGIN: [f64; 3] = [5.0, 0.0, 0.0];

/// A point stated in the INDEXED frame (relative to the work offset, the
/// tool standing up Z), carried to the world the stream speaks: the part
/// turned by +a about X, `Rx(a)`, plus the offset - so the tool axis is
/// (0, -sin a, cos a), the pose law's A. Written here by hand, so the post
/// is checked against an independent rotation, not against itself.
fn world(a: f64, l: [f64; 3]) -> [f64; 3] {
    let (s, c) = a.to_radians().sin_cos();
    [
        l[0] + ORIGIN[0],
        l[1] * c - l[2] * s + ORIGIN[1],
        l[1] * s + l[2] * c + ORIGIN[2],
    ]
}

/// One face's square contour at 22 mm from the A centerline, in the
/// indexed frame: approach, plunge, the square, retract.
fn face_local() -> Vec<(MoveKind, [f64; 3], Option<f64>)> {
    vec![
        (MoveKind::Rapid, [0.0, -10.0, 40.0], None),
        (MoveKind::Rapid, [0.0, -10.0, 27.0], None),
        (MoveKind::Plunge, [0.0, -10.0, 22.0], Some(200.0)),
        (MoveKind::Feed, [30.0, -10.0, 22.0], Some(600.0)),
        (MoveKind::Feed, [30.0, 10.0, 22.0], Some(600.0)),
        (MoveKind::Feed, [0.0, 10.0, 22.0], Some(600.0)),
        (MoveKind::Feed, [0.0, -10.0, 22.0], Some(600.0)),
        (MoveKind::Rapid, [0.0, -10.0, 40.0], None),
    ]
}

/// The stream's opening: spindle, offset and its origin, the tool.
fn head() -> Vec<cnc::Record> {
    vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(6000) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Param { name: "OriginX".into(), value: json!(ORIGIN[0]) },
        cnc::Record::Param { name: "OriginY".into(), value: json!(ORIGIN[1]) },
        cnc::Record::Param { name: "OriginZ".into(), value: json!(ORIGIN[2]) },
        cnc::Record::Tool { number: 3, name: "6MM END MILL".into() },
    ]
}

/// One operation at tool angle `a` (0 = upright: no A words at all, the
/// way every prismatic dump reads).
fn face_section(recs: &mut Vec<cnc::Record>, name: &str, a: f64) {
    recs.push(cnc::Record::Param { name: "OperationName".into(), value: json!(name) });
    recs.push(cnc::Record::Section { begin: true });
    for (k, l, f) in face_local() {
        let abc = (a != 0.0).then_some([a, 0.0, 0.0]);
        recs.push(mv(k, world(a, l), f, abc));
    }
    recs.push(cnc::Record::Section { begin: false });
}

/// The 4-face job: the same square on the faces at A 0, 90, 180 and -90.
const FACES: [(&str, f64); 4] = [("TOP", 0.0), ("FACE 90", 90.0), ("FACE 180", 180.0), ("FACE M90", -90.0)];

fn four_face_stream() -> String {
    let mut recs = head();
    for (n, a) in FACES {
        face_section(&mut recs, n, a);
    }
    cnc::serialize(&recs)
}

/// A one-operation stream whose every move is at `abc`.
fn tilted_stream(abc: [f64; 3]) -> String {
    let mut recs = head();
    recs.push(cnc::Record::Section { begin: true });
    for (k, l, f) in face_local() {
        recs.push(mv(k, world(abc[0], l), f, Some(abc)));
    }
    recs.push(cnc::Record::Section { begin: false });
    cnc::serialize(&recs)
}

fn word(t: &str) -> Option<(char, f64)> {
    let mut c = t.chars();
    let l = c.next()?;
    let n: f64 = c.as_str().parse().ok()?;
    Some((l, n))
}

#[test]
fn the_microarc_machine_states_an_a_about_x() {
    let m = shipped(ARC4);
    assert_eq!(m.model, ARC4);
    assert_eq!(kind(Some(&m)), Kind::Mill, "an A about X leans the tool: a mill");
    let a = m.axes.iter().find(|a| a.name == "A").expect("an A axis");
    assert!(a.rotary, "A is a rotary");
    assert_eq!(a.axis, Some([1.0, 0.0, 0.0]), "A turns about X");
    assert_eq!(a.travel, Some((-360.0, 360.0)), "degrees, never unit-scaled");
    // TD10749 states no rate for the microARC 4: unstated, never a zero.
    assert_eq!((a.feed, a.rapid), (None, None));
    // X Y Z are the 770MX's own figures, unchanged by the add-on.
    let base = shipped("Tormach 770MX");
    let lin: Vec<_> = m.axes.iter().filter(|a| !a.rotary).cloned().collect();
    assert_eq!(lin, base.axes);
    assert_eq!(m.safe_retract_z, Some(0.0));
    // The head behind the rotary leaf still reads (#785: the head search
    // once stopped at the first part with no children, dropping the
    // spindle, belt ranges and coolant of any such file).
    assert_eq!((m.spindle_min, m.spindle_max), (base.spindle_min, base.spindle_max));
    assert_eq!((m.spindle_min, m.spindle_max), (Some(175.0), Some(10000.0)));
    assert_eq!(m.spindle_ranges, base.spindle_ranges);
    assert_eq!(m.rigid_tapping, base.rigid_tapping);
    assert_eq!(m.coolants, base.coolants);
    assert!(!m.coolants.is_empty());
    let p = machine::script_props(&machine::mirror(&m));
    assert_eq!(p["rtcp"], true, "the A about X is a tilting rotary");
    assert_eq!(p["rotaries"][0]["name"], "A");
    assert_eq!(p["rotaries"][0]["travel"], json!([-360.0, 360.0]));
}

/// The golden: four faces, each indexed alone with Z up, each posted in
/// its own turned frame - the same square every time.
#[test]
fn an_indexed_four_face_job_posts_a_between_operations() {
    let out = run(&mirrored(ARC4), &four_face_stream());
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let nc = out.program;
    let path = repo(GOLDEN);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &nc).unwrap_or_else(|e| panic!("{GOLDEN}: {e}"));
    } else {
        assert_eq!(nc, read(GOLDEN), "{GOLDEN} is stale; regenerate with WRITE_GOLDEN=1");
    }
    let lines: Vec<&str> = nc.lines().collect();
    // Every A word stands alone on a G0 line, right after Z goes up.
    let a_lines: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].contains('A')
        && lines[i].split(' ').any(|t| t.starts_with('A') && word(t).is_some())).collect();
    let want = ["G0 A0", "G0 A90", "G0 A180", "G0 A-90"];
    assert_eq!(a_lines.iter().map(|&i| lines[i]).collect::<Vec<_>>(), want, "{nc}");
    for &i in &a_lines {
        assert_eq!(lines[i - 1], "G53 G0 Z0", "Z up before the table turns\n{nc}");
    }
    // The operator note, once, before the first index.
    assert_eq!(nc.matches("(4TH AXIS A ABOUT X - G54 Y0 Z0 ON THE A CENTERLINE)").count(), 1);
    assert_eq!(nc.matches("(NO A CLAMP CODE - RELEASE ANY MANUAL ROTARY CLAMP)").count(), 1);
    // Each section, after its index, posts the indexed-frame square: the
    // same X Y Z (inch) on every face - the turn undone exactly.
    let mut sections: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut cur = [f64::NAN; 3];
    for l in &lines {
        if l.starts_with("G0 A") {
            sections.push(Vec::new());
            cur = [f64::NAN; 3];
            continue;
        }
        if !(l.starts_with("G0 X") || l.starts_with("G1 ")) {
            continue;
        }
        for t in l.split(' ') {
            match word(t) {
                Some(('X', v)) => cur[0] = v,
                Some(('Y', v)) => cur[1] = v,
                Some(('Z', v)) => cur[2] = v,
                _ => {}
            }
        }
        if l.starts_with("G1 ")
            && let Some(s) = sections.last_mut()
        {
            s.push(cur);
        }
    }
    assert_eq!(sections.len(), 4);
    let want: Vec<[f64; 3]> = face_local()
        .iter()
        .filter(|(k, _, _)| *k != MoveKind::Rapid)
        .map(|(_, l, _)| l.map(|v| v / 25.4))
        .collect();
    for (i, s) in sections.iter().enumerate() {
        assert_eq!(s.len(), want.len(), "face {i}\n{nc}");
        for (got, w) in s.iter().zip(&want) {
            for k in 0..3 {
                assert!((got[k] - w[k]).abs() < 6e-5, "face {i}: {got:?} vs {w:?}\n{nc}");
            }
        }
    }
    // No A ever rides a cutting move, and no G93 is written.
    assert!(!lines.iter().any(|l| l.starts_with("G1") && l.contains(" A")), "{nc}");
    assert!(!nc.contains("G93"), "{nc}");
}

/// A program on the 4th-axis machine never assumes where the last run
/// left the table: an upright job states A0 first. The same job on the
/// plain 770MX writes no A at all.
#[test]
fn an_upright_job_states_a0_on_the_4th_axis_machine_only() {
    let mut recs = head();
    face_section(&mut recs, "TOP", 0.0);
    let s = cnc::serialize(&recs);
    let arc = run(&mirrored(ARC4), &s);
    assert!(arc.warnings.is_empty(), "{:?}", arc.warnings);
    assert_eq!(arc.program.matches("G0 A0").count(), 1, "{}", arc.program);
    let plain = run(&mirrored("Tormach 770MX"), &s);
    assert!(plain.warnings.is_empty(), "{:?}", plain.warnings);
    assert!(!plain.program.contains(" A") && !plain.program.contains("G0 A"), "{}", plain.program);
}

/// The direction sense is the axis vector's sign: a machine whose A is
/// stated about -X turns the other way for the same tool angle.
#[test]
fn a_reversed_a_vector_posts_the_angle_negated() {
    let mut m = mirrored(ARC4);
    m["kinematics"]["static"]["rotaries"][0]["direction"] = json!([-1.0, 0.0, 0.0]);
    let out = run(&m, &tilted_stream([90.0, 0.0, 0.0]));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l == "G0 A-90"), "{}", out.program);
}

fn refused(mch: &Value, stream: &str, why: &str) {
    let out = run(mch, stream);
    assert!(
        out.warnings.iter().any(|w| w.contains(why)),
        "must refuse by name ({why}), got {:?}\n{}",
        out.warnings,
        out.program
    );
}

#[test]
fn a_tilt_the_a_axis_cannot_reach_is_refused() {
    refused(&mirrored(ARC4), &tilted_stream([30.0, 15.0, 0.0]), "the A axis about X cannot reach it");
    refused(&mirrored(ARC4), &tilted_stream([0.0, 0.0, 45.0]), "the A axis about X cannot reach it");
}

/// The tool angle is read through the axis, so it lies in -180..180 and
/// the shipped -360..360 holds every face; a machine stating less travel
/// (a table stopped at +-100 degrees) refuses the face underneath.
#[test]
fn an_a_past_travel_is_refused() {
    let mut m = mirrored(ARC4);
    m["kinematics"]["static"]["rotaries"][0]["travel"] = json!([-100.0, 100.0]);
    refused(&m, &tilted_stream([180.0, 0.0, 0.0]), "A180 is beyond the A axis travel -100..100");
    let ok = run(&m, &tilted_stream([90.0, 0.0, 0.0]));
    assert!(ok.warnings.is_empty(), "A90 is inside: {:?}", ok.warnings);
}

#[test]
fn an_a_that_changes_inside_an_operation_is_refused() {
    let mut recs = head();
    recs.push(cnc::Record::Param { name: "OperationName".into(), value: json!("WRAP") });
    recs.push(cnc::Record::Section { begin: true });
    for (i, (k, l, f)) in face_local().into_iter().enumerate() {
        let a = i as f64 * 10.0;
        recs.push(mv(k, world(a, l), f, Some([a, 0.0, 0.0])));
    }
    recs.push(cnc::Record::Section { begin: false });
    refused(&mirrored(ARC4), &cnc::serialize(&recs), "simultaneous 4-axis motion in operation WRAP");
}

#[test]
fn an_arc_on_an_indexed_a_is_refused() {
    let mut recs = head();
    recs.push(cnc::Record::Section { begin: true });
    recs.push(mv(MoveKind::Rapid, world(90.0, [0.0, 0.0, 40.0]), None, Some([90.0, 0.0, 0.0])));
    recs.push(cnc::Record::Move(cnc::MoveRec {
        kind: MoveKind::Feed,
        x: 10.0,
        y: 0.0,
        z: 22.0,
        feed: Some(500.0),
        center: Some((5.0, 0.0)),
        ccw: true,
        plane: Default::default(),
        abc: Some([90.0, 0.0, 0.0]),
    }));
    recs.push(cnc::Record::Section { begin: false });
    refused(&mirrored(ARC4), &cnc::serialize(&recs), "arc on an indexed A");
}

#[test]
fn a_tilt_on_a_machine_with_no_a_about_x_is_refused() {
    refused(&mirrored("Tormach 770MX"), &tilted_stream([90.0, 0.0, 0.0]), "states no A rotary about X");
    // An A stated about Y is not this post's 4th axis either.
    let mut m = mirrored(ARC4);
    m["kinematics"]["static"]["rotaries"][0]["direction"] = json!([0.0, 1.0, 0.0]);
    refused(&m, &tilted_stream([90.0, 0.0, 0.0]), "states no A rotary about X");
}

/// The pose law's canonical letters for the face underneath are A0 B180
/// (a 3+2 tilt keeps A in -90..90): the tool axis (0, 0, -1) has no X
/// component, so the A about X reaches it as A180, the same program the
/// A180 letters post.
#[test]
fn the_canonical_b180_pose_posts_as_a180() {
    let canon = run(&mirrored(ARC4), &tilted_stream([0.0, 180.0, 0.0]));
    assert!(canon.warnings.is_empty(), "{:?}", canon.warnings);
    assert!(canon.program.lines().any(|l| l == "G0 A180"), "{}", canon.program);
    // The tips are the same points either way (the stream builder turns by
    // the A letter), so the two programs agree line for line.
    let mut recs = head();
    recs.push(cnc::Record::Section { begin: true });
    for (k, l, f) in face_local() {
        recs.push(mv(k, world(180.0, l), f, Some([0.0, 180.0, 0.0])));
    }
    recs.push(cnc::Record::Section { begin: false });
    let canon = run(&mirrored(ARC4), &cnc::serialize(&recs));
    let letters = run(&mirrored(ARC4), &tilted_stream([180.0, 0.0, 0.0]));
    assert_eq!(canon.program, letters.program);
}

/// Each section's opening lines, M6 (or the section's first block) to its
/// first cutting move, for the two-ops-on-one-face pins below.
fn openings(nc: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut open = false;
    for l in nc.lines() {
        if l.starts_with("G17 G90") {
            out.push(Vec::new());
            open = true;
        } else if l.starts_with("G1 ") {
            open = false;
        }
        if open && let Some(s) = out.last_mut() {
            s.push(l);
        }
    }
    out
}

/// Two operations on the same face, both at `a`; `tool_between` re-issues
/// a tool (T4) before the second, the way the app does before every op.
fn same_face_stream(a: f64, tool_between: bool) -> String {
    let mut recs = head();
    face_section(&mut recs, "ROUGH", a);
    if tool_between {
        recs.push(cnc::Record::Tool { number: 4, name: "3MM END MILL".into() });
    }
    face_section(&mut recs, "FINISH", a);
    cnc::serialize(&recs)
}

/// Review of PR #798: the bare G30 before every `T.. M6` moves EVERY axis
/// to the stored G30 position, A included (LinuxCNC G30 with no axis
/// words), so the A the control held before it is gone. Two ops on the
/// same non-zero face with a tool change between must both index after
/// their M6 - the second never cuts wherever G30 left the table.
#[test]
fn a_tool_change_on_the_same_face_restates_a_after_the_g30() {
    let out = run(&mirrored(ARC4), &same_face_stream(90.0, true));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let nc = out.program;
    let secs = openings(&nc);
    assert_eq!(secs.len(), 2, "{nc}");
    for (i, s) in secs.iter().enumerate() {
        let m6 = s.iter().position(|l| l.ends_with(" M6"));
        let g30 = s.iter().position(|l| *l == "G30");
        let a = s.iter().position(|l| *l == "G0 A90");
        assert!(g30.is_some() && m6.is_some(), "section {i} changes tools\n{nc}");
        assert!(a > m6, "section {i}: G0 A90 after its M6\n{nc}");
        let a = a.unwrap_or(0);
        assert_eq!(s[a - 1], "G53 G0 Z0", "section {i}: Z up before A\n{nc}");
    }
}

/// The same-tool continuation (no tool record, so no G30): A is restated
/// at every section all the same. Nothing in the program moved A, but a
/// section boundary is where an operator may stop (M0/M1, a probe check)
/// and jog, so a section never inherits the last one's A.
#[test]
fn a_same_tool_continuation_on_the_same_face_restates_a() {
    let out = run(&mirrored(ARC4), &same_face_stream(90.0, false));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let nc = out.program;
    assert_eq!(nc.matches(" M6").count(), 1, "one tool change only\n{nc}");
    let secs = openings(&nc);
    assert_eq!(secs.len(), 2, "{nc}");
    for (i, s) in secs.iter().enumerate() {
        let a = s.iter().position(|l| *l == "G0 A90");
        assert!(a.is_some_and(|a| a > 0 && s[a - 1] == "G53 G0 Z0"), "section {i}: G53 G0 Z0 / G0 A90\n{nc}");
    }
}

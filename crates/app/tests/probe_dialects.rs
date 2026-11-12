//! #173 acceptance: the probing hooks of the Haas, Mazak and Siemens 840D
//! posts, and the Heidenhain post's named refusal. Each voiced dialect
//! posts the same bore-centre probe the generic dialect's golden
//! (`nc/fanuc/probe_bore_centre.nc`, tests/probe_op.rs) posts, through
//! the whole post seam (dump -> stream text -> engine -> <dialect>.cps).
//!
//! Provenance: the public programming documentation each post cites in
//! its probing section (Haas Mill Operator's Manual; Mazak EIA/ISO
//! programming manual; SINUMERIK 840D sl Fundamentals / Job Planning);
//! our own fixtures.
//!
//! #174 on these dialects: every positioning feed of the cycle (GUARD)
//! posts as a protected move that alarms on an unexpected trip, with an
//! alarm distinct from the no-contact one.
//!
//! Gate map (M4.4): covers assets/posts (haas, mazak, sinumerik,
//! heidenhain probing; onProbeGuard), post::probe_rec (GUARD),
//! motion::probing.

mod common;

use camcraft_app::geometry::Point3;
use camcraft_app::post::{self, cnc, engine};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};

const HAAS: &str = include_str!("../../../assets/posts/haas.cps");
const MAZAK: &str = include_str!("../../../assets/posts/mazak.cps");
const SINUMERIK: &str = include_str!("../../../assets/posts/sinumerik.cps");
const HEIDENHAIN: &str = include_str!("../../../assets/posts/heidenhain.cps");

/// The bore centre of tests/probe_op.rs: a 20 mm bore at (30, 30),
/// rim Z 20, four touches (seek + measure each).
fn bore_stream(set_wcs: Option<u8>, wcs: &str) -> String {
    probe_stream(ProbeMode::Bore, ProbeDir::ZNeg, Point3::new(30.0, 30.0, 20.0), 10.0, set_wcs, wcs)
}

/// An edge +X touch at the part edge X 0: the 4 mm ball trips at X -2,
/// so the expected value is negative.
fn edge_stream() -> String {
    probe_stream(ProbeMode::Edge, ProbeDir::XPos, Point3::new(0.0, 5.0, 20.0), 0.0, Some(1), "G54")
}

fn probe_stream(mode: ProbeMode, dir: ProbeDir, target: Point3, radius: f64, set_wcs: Option<u8>, wcs: &str) -> String {
    let p = ProbeParams {
        mode,
        dir,
        dir_y: ProbeDir::YPos,
        target,
        radius,
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
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs, tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    let recs = post::dump(&t, &d);
    assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "a sane probe path carries no WARN");
    cnc::serialize(&recs)
}

fn post_ok(cps: &str, stream: &str) -> String {
    let out = engine::run(cps, &serde_json::Value::Null, stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        !out.program.lines().any(|l| l.split_whitespace().any(|w| w == "M3" || w == "M8" || w == "MOTON")),
        "a touch probe starts no spindle and no coolant:\n{}",
        out.program
    );
    out.program
}

#[test]
fn haas_bore_centre_posts_g31_skips() {
    let stream = bore_stream(Some(1), "G54");
    let prog = post_ok(HAAS, &stream);
    let n = 8 + guard_count(&stream);
    assert_eq!(prog.lines().filter(|l| l.starts_with("G31 ")).count(), n, "four touches (seek + measure) + guards");
    assert!(prog.contains("IF[ABS[#5061-43]LT0.001]THEN#3000=2(PROBE NO CONTACT)"), "{prog}");
    assert!(prog.contains("#100=#5061"), "the measured trip is kept");
    assert!(prog.contains("G91 G1 X-1 Y0 Z0 F300"), "the back-off is incremental");
    assert!(prog.contains("G10 L2 P2 X[#5221+#190] Y[#5222+#191]"), "the result sets G55 from G54: {prog}");
    common::write_golden("nc/haas/probe_bore_centre.nc", &prog);
}

#[test]
fn haas_without_an_offset_to_set_stops_for_the_operator() {
    let prog = post_ok(HAAS, &bore_stream(None, "G54"));
    assert!(!prog.contains("G10"), "nothing is set");
    assert!(prog.contains("#3006=1(CHECK PROBE RESULT #190..)"), "{prog}");
}

#[test]
fn mazak_bore_centre_posts_g31_skips_with_goto_tests() {
    let stream = bore_stream(Some(1), "G54");
    let prog = post_ok(MAZAK, &stream);
    let n = 8 + guard_count(&stream);
    assert_eq!(prog.lines().filter(|l| l.starts_with("G31 ")).count(), n, "four touches (seek + measure) + guards");
    assert!(!prog.contains("THEN"), "the branch is IF..GOTO, not IF..THEN");
    // The label numbers count the guards ahead: #570's protected descent
    // below the safe plane is one more (#695), so the first no-contact
    // test's label is 9003.
    assert!(prog.contains("IF[ABS[#5061-43]GE0.001]GOTO9003\n#3000=2(PROBE NO CONTACT)\nN9003\n"), "{prog}");
    assert!(prog.contains("G91 G01 X-1 Y0 Z0 F300"), "the back-off is incremental");
    assert!(prog.contains("G10 L2 P2 X[#5221+#190] Y[#5222+#191]"), "the result sets G55 from G54: {prog}");
    // Every GOTO label is unique, so no jump can land on another test's.
    let labels: Vec<&str> = prog.lines().filter(|l| l.starts_with("N9")).collect();
    let mut uniq = labels.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(labels.len(), uniq.len(), "unique labels: {labels:?}");
    common::write_golden("nc/mazak/probe_bore_centre.nc", &prog);
}

#[test]
fn mazak_without_an_offset_to_set_stops_for_the_operator() {
    let prog = post_ok(MAZAK, &bore_stream(None, "G54"));
    assert!(!prog.contains("G10"), "nothing is set");
    assert!(prog.contains("(CHECK PROBE RESULT #190..)\nM00\n"), "{prog}");
}

#[test]
fn sinumerik_bore_centre_posts_meas_blocks() {
    let stream = bore_stream(Some(1), "G54");
    let prog = post_ok(SINUMERIK, &stream);
    let n = 8 + guard_count(&stream);
    assert_eq!(prog.lines().filter(|l| l.contains(" MEAS=1 G1 ")).count(), n, "four touches (seek + measure) + guards");
    assert!(!prog.contains("G31"), "a Siemens control measures with MEAS, not G31");
    assert!(prog.contains(" IF $AC_MEA[1]==0\n"), "a touch that never trips alarms: {prog}");
    assert!(prog.contains(" R50=$AA_MW[X]\n"), "the trip is read in workpiece coordinates");
    assert!(prog.contains(" G91 G1 X-1 Y0 Z0 F300\n"), "the back-off is incremental");
    assert!(prog.contains(" $P_UIFR[2,X,TR]=$P_UIFR[1,X,TR]+R90\n"), "the result sets G55 from G54: {prog}");
    assert!(prog.contains(" $P_UIFR[2,X,FI]=$P_UIFR[1,X,FI]\n"), "the fine part follows the active frame");
    common::write_golden("nc/sinumerik/probe_bore_centre.nc", &prog);
}

/// #934: the sixth work offset is the settable frame G506 on an 840D
/// (G58 / G59 are axial programmable offsets there, never written): a
/// probe asked to set it writes frame 6 and re-selects G506.
#[test]
fn sinumerik_sets_the_sixth_offset_as_frame_g506() {
    let prog = post_ok(SINUMERIK, &bore_stream(Some(5), "G54"));
    assert!(prog.contains(" $P_UIFR[6,X,TR]=$P_UIFR[1,X,TR]+R90\n"), "sets frame 6 (G506) from G54: {prog}");
    assert!(!prog.lines().any(|l| l.split_whitespace().any(|w| w == "G58" || w == "G59")), "{prog}");
}

/// Heidenhain's ISO manual states no skip move and no trip-position
/// variable (probing is the touch-probe cycles, another manual): the
/// post defines no probing hook and the engine refuses by name - a
/// counted warning, which blocks the post (posting::post_script).
#[test]
fn heidenhain_refuses_a_probe_by_name() {
    let out = engine::run(HEIDENHAIN, &serde_json::Value::Null, &bore_stream(Some(1), "G54")).expect("engine runs");
    for hook in ["onProbeMove", "onProbeRetreat", "onProbeResult"] {
        assert!(out.warnings.iter().any(|w| w.contains(hook)), "refused by {hook}: {:?}", out.warnings);
    }
}

/// A negative expected value (edge probe) folds its sign into the
/// operator: no doubled sign like `#190=#100--2` is ever written.
fn no_doubled_sign(prog: &str) {
    assert!(!prog.contains("--") && !prog.contains("+-") && !prog.contains("-+"), "doubled sign:\n{prog}");
}

#[test]
fn haas_edge_deviation_folds_its_sign() {
    let prog = post_ok(HAAS, &edge_stream());
    no_doubled_sign(&prog);
    assert!(prog.contains("#190=#100+2\n"), "{prog}");
}

#[test]
fn mazak_edge_deviation_folds_its_sign() {
    let prog = post_ok(MAZAK, &edge_stream());
    no_doubled_sign(&prog);
    assert!(prog.contains("#190=#100+2\n"), "{prog}");
}

#[test]
fn sinumerik_edge_deviation_folds_its_sign() {
    let prog = post_ok(SINUMERIK, &edge_stream());
    no_doubled_sign(&prog);
    assert!(prog.contains(" R90=R50+2\n"), "{prog}");
}

/// #174: the bore's GUARD records - the descent from the rapid plane to
/// the touch depth and the rise back - in the stream every dialect posts.
fn guard_count(stream: &str) -> usize {
    stream.lines().filter(|l| l.starts_with("GUARD ")).count()
}

/// Haas: each guard is a G31 followed by one #5061+ test per moving
/// axis, alarming #3000=3 - not the no-contact alarm 2.
#[test]
fn haas_guards_positioning_moves_with_a_hit_alarm() {
    let stream = bore_stream(Some(1), "G54");
    let guards = guard_count(&stream);
    assert!(guards > 0, "the bore has protected moves:\n{stream}");
    let prog = post_ok(HAAS, &stream);
    let lines: Vec<&str> = prog.lines().collect();
    assert!(!lines.iter().any(|l| l.starts_with("G1 ") || l.starts_with("G01 ")), "no plain feed:\n{prog}");
    let hits = lines.iter().filter(|l| l.ends_with("]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)")).count();
    assert!(hits >= guards, "a test per guard's moving axis:\n{prog}");
    // The descent to the touch depth (Z 15) is guarded on Z, the axis it moves.
    assert!(prog.contains("IF[ABS[#5063-15]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)"), "{prog}");
    assert_eq!(lines.iter().filter(|l| l.starts_with("G31 ")).count(), 8 + guards, "touches + guards");
}

/// Mazak: the same test in the IF[..]GOTOn form, jumping past a
/// #3000=3 alarm only when the move reached its target.
#[test]
fn mazak_guards_positioning_moves_with_a_hit_alarm() {
    let stream = bore_stream(Some(1), "G54");
    let guards = guard_count(&stream);
    let prog = post_ok(MAZAK, &stream);
    assert!(!prog.contains("THEN"), "IF..GOTO, not IF..THEN");
    assert!(!prog.lines().any(|l| l.starts_with("G1 ") || l.starts_with("G01 ")), "no plain feed:\n{prog}");
    assert!(prog.matches("#3000=3(PROBE HIT ON POSITIONING)").count() >= guards, "{prog}");
    let at = prog.find("IF[ABS[#5063-15]LE0.001]GOTO").expect("the descent's Z test");
    let tail = &prog[at..];
    let label = tail["IF[ABS[#5063-15]LE0.001]GOTO".len()..].lines().next().unwrap_or_default();
    assert!(
        tail.contains(&format!("GOTO{label}\n#3000=3(PROBE HIT ON POSITIONING)\nN{label}\n")),
        "the alarm sits between the test and its label:\n{prog}"
    );
    assert_eq!(prog.lines().filter(|l| l.starts_with("G31 ")).count(), 8 + guards, "touches + guards");
}

/// Sinumerik: each guard is a MEAS=1 G1 block followed by a $AC_MEA[1]
/// test that alarms SETAL(65002) - not the no-contact 65000 - and stops.
#[test]
fn sinumerik_guards_positioning_moves_with_a_hit_alarm() {
    let stream = bore_stream(Some(1), "G54");
    let guards = guard_count(&stream);
    let prog = post_ok(SINUMERIK, &stream);
    assert_eq!(prog.lines().filter(|l| l.contains(" MEAS=1 G1 ")).count(), 8 + guards, "touches + guards");
    assert_eq!(prog.matches(" IF $AC_MEA[1]==1\n").count(), guards, "{prog}");
    assert_eq!(prog.matches(" SETAL(65002)\n").count(), guards, "{prog}");
    let at = prog.find(" IF $AC_MEA[1]==1\n").expect("a guard test");
    let block: Vec<&str> = prog[at..].lines().take(5).collect();
    assert!(block[1].ends_with(" MSG(\"PROBE HIT ON POSITIONING\")"), "{block:?}");
    assert!(block[2].ends_with(" SETAL(65002)") && block[3].ends_with(" M0") && block[4].ends_with(" ENDIF"), "{block:?}");
    assert!(!prog.lines().any(|l| l.contains(" G1 ") && !l.contains("MEAS=1") && !l.contains("G91")), "no plain feed:\n{prog}");
}

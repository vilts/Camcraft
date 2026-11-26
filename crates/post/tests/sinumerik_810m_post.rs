//! Pinning suite for Siemens Sinumerik 810M (`assets/posts/sinumerik-810m.cps`)
//! and EMCO VMC-200 (`assets/machines/emco-vmc-200.mch`) - issue #2888.
//!
//! Provenance: public ISO 6983-1 / DIN 66025, Siemens SINUMERIK 810M/820M GA3
//! programming docs, EMCO WinNC Sinumerik 810M cycle parameter tables, and
//! EMCO VMC-200 public specifications.

use camcraft_post::cl::{ArcPlane, CompSide, Cycle, CycleKind, MoveKind, PathBuilder};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::machine::parse_mch;
use camcraft_post::post::{cnc, dump, engine, machine, Dump, PostError, ProgramId};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("missing {rel}: {e}"))
}

fn cps() -> String {
    read("assets/posts/sinumerik-810m.cps")
}

fn emco_vmc200_mirror() -> Value {
    let parsed = parse_mch(&read("assets/machines/emco-vmc-200.mch")).expect("EMCO VMC-200 parses");
    machine::mirror(&parsed)
}

fn param(name: &str, value: Value) -> cnc::Record {
    cnc::Record::Param { name: name.into(), value }
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec {
        kind, x, y, z, feed, center: None, ccw: false, plane: ArcPlane::Xy, abc: None,
    })
}

fn simple_stream(wcs: Value, extra: Vec<cnc::Record>) -> String {
    let mut recs = vec![
        param("OperationName", json!("Pocket1")),
        param("SetupName", json!("Fixture (1); A%")),
        param("SpindleSpeed", json!(2000.0)),
        param("ClearancePlane", json!(10.0)),
        param("RetractPlane", json!(5.0)),
        param("WorkOffset", wcs),
    ];
    recs.extend(extra);
    recs.push(cnc::Record::Tool { number: 2, name: "6MM END MILL".into() });
    recs.push(cnc::Record::Section { begin: true });
    recs.push(mv(MoveKind::Rapid, 10.0, 20.0, 5.0, None));
    recs.push(mv(MoveKind::Feed, 10.0, 20.0, -2.0, Some(300.0)));
    recs.push(mv(MoveKind::Feed, 30.0, 20.0, -2.0, Some(600.0)));
    recs.push(mv(MoveKind::Rapid, 30.0, 20.0, 10.0, None));
    recs.push(cnc::Record::Section { begin: false });
    cnc::serialize(&recs)
}

fn run(mch: &Value, stream: &str, opts: &Value) -> engine::PostOutput {
    engine::run_with(&cps(), mch, stream, opts).expect("engine runs")
}

#[test]
fn header_footer_units_and_extension() {
    let script = cps();
    assert_eq!(engine::extension_of(&script).expect("extension"), "mpf");
    let mch = emco_vmc200_mirror();
    let out = run(&mch, &simple_stream(json!(1), vec![]), &json!({}));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert_eq!(lines[0], "%MPF1", "{}", out.program);
    assert_eq!(lines.last().copied(), Some("M30"), "{}", out.program);
    assert!(!out.program.contains(';'), "810M never emits 840D semicolons:\n{}", out.program);
    assert!(!out.program.contains("%_N_"), "{}", out.program);
    assert!(out.program.contains("G17 G71 G90 G94 G54 G40 G80\n"), "{}", out.program);

    let id = ProgramId {
        number: 1234,
        numbered: true,
        comment: "Bracket A".into(),
        notes: vec!["Check ; (A) % 50%".into()],
    };
    let numbered = engine::run_program_with(&script, &mch, &simple_stream(json!(1), vec![]), &json!({}), &id)
        .expect("numbered");
    assert_eq!(numbered.program.lines().next(), Some("%MPF1234"), "{}", numbered.program);
    assert!(numbered.program.contains("(PROGRAM 1234 - Bracket A)\n"), "{}", numbered.program);
    assert!(numbered.program.contains("(Check    A    50 )\n"), "{}", numbered.program);

    let bad_id = ProgramId { number: 10000, numbered: true, ..Default::default() };
    let err = engine::run_program_with(&script, &mch, &simple_stream(json!(1), vec![]), &json!({}), &bad_id)
        .expect_err("past %MPF9999 must refuse")
        .to_string();
    assert!(err.contains("9999"), "{err}");

    let Err(PostError::Refused(inch_err)) = engine::run_with(&script, &mch, &simple_stream(json!(1), vec![]), &json!({ "Units": "inch" })) else {
        panic!("inch document must be refused by name");
    };
    assert!(inch_err.contains("mm"), "{inch_err}");

    let end_inch = run(&mch, &simple_stream(json!(1), vec![]), &json!({ "EndUnits": "inch" }));
    assert!(end_inch.program.contains("END UNITS NOT WRITTEN"), "{}", end_inch.program);
    assert!(
        !end_inch.program.lines().filter(|l| !l.starts_with('(')).any(|l| l.split_whitespace().any(|w| w == "G70")),
        "never emit G70: {}",
        end_inch.program
    );
}

#[test]
fn comments_block_limit_and_sequence_numbers() {
    let mch = emco_vmc200_mirror();
    let plain = run(&mch, &simple_stream(json!(1), vec![]), &json!({}));
    assert!(!plain.program.lines().any(|l| l.starts_with('N')), "default saves RAM:\n{}", plain.program);
    for l in &plain.program.lines().collect::<Vec<_>>() {
        assert!(l.len() <= 120, "810M 120-char line limit: {l}");
    }

    let with_n = run(&mch, &simple_stream(json!(1), vec![]), &json!({ "showSequenceNumbers": "every-block" }));
    assert!(with_n.program.lines().any(|l| l.starts_with("N10 G17")), "{}", with_n.program);

    let overflow_n = run(
        &mch,
        &simple_stream(json!(1), vec![]),
        &json!({ "showSequenceNumbers": "every-block", "sequenceNumberStart": 9950, "sequenceNumberIncrement": 20 }),
    );
    assert!(overflow_n.warnings.iter().any(|w| w.contains("N9999")), "{:?}", overflow_n.warnings);
    assert!(
        !overflow_n.program.lines().filter(|l| !l.starts_with('(')).any(|l| l.starts_with("N100")),
        "must not emit 5-digit N block: {}",
        overflow_n.program
    );

    let big_block = format!(
        "{}\nfunction onOpen() {{ block([\"{}\"]); }}\nfunction onClose() {{}}\n",
        cps(),
        "X100.123 ".repeat(14).trim()
    );
    let over120 = engine::run(&big_block, &mch, "").expect("runs");
    assert!(over120.warnings.iter().any(|w| w.contains("120-character limit")), "{:?}", over120.warnings);
}

#[test]
fn work_offsets_are_g54_through_g57_only() {
    let mch = emco_vmc200_mirror();
    for (idx, word) in [(1, "G54"), (2, "G55"), (3, "G56"), (4, "G57")] {
        let out = run(&mch, &simple_stream(json!(idx), vec![]), &json!({}));
        assert!(out.warnings.is_empty(), "{idx}: {:?}", out.warnings);
        assert!(out.program.contains(&format!(" {word} ")), "{word}:\n{}", out.program);
    }
    for bad in [5, 6] {
        let out = run(&mch, &simple_stream(json!(bad), vec![]), &json!({}));
        assert!(!out.warnings.is_empty(), "offset {bad} must be refused by name:\n{}", out.program);
        assert!(!out.program.contains("G58") && !out.program.contains("G59") && !out.program.contains("G505"), "{}", out.program);
    }
}

#[test]
fn tool_change_safe_retract_and_safety_guards() {
    let mch = emco_vmc200_mirror();
    let out = run(&mch, &simple_stream(json!(1), vec![param("Smoothing", json!(0.02))]), &json!({}));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert!(p.contains("T2 M6\nD2\n"), "T.. M6 then D..:\n{p}");
    assert!(p.contains("G0 G53 Z0 D0\n"), "G53 safe retract with D0:\n{p}");
    assert!(p.contains("(SMOOTHING NOT WRITTEN - NO 810M FORM STATED)"), "{p}");
    assert!(!p.contains("CYCLE832"), "{p}");

    let mut custom_mch = mch.clone();
    custom_mch["safeRetractZ"] = json!(-15.0);
    let custom = run(&custom_mch, &simple_stream(json!(1), vec![]), &json!({}));
    assert!(custom.program.contains("G0 G53 Z-15 D0\n"), "{}", custom.program);

    let mut no_retract = mch.clone();
    no_retract.as_object_mut().unwrap().remove("safeRetractZ");
    let missed = run(&no_retract, &simple_stream(json!(1), vec![]), &json!({}));
    assert!(missed.warnings.iter().any(|w| w.contains("safe Z retract")), "{:?}", missed.warnings);

    let no_spindle: String = simple_stream(json!(1), vec![])
        .lines()
        .filter(|l| !l.contains("SpindleSpeed"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(!no_spindle.contains("SpindleSpeed"), "stream fixture check");
    let stopped = run(&mch, &no_spindle, &json!({}));
    assert!(stopped.warnings.iter().any(|w| w.contains("with the spindle stopped")), "{:?}", stopped.warnings);

    let bad_tool = simple_stream(json!(1), vec![]).replace("TOOL 2 6MM END MILL", "TOOL 105 BIG");
    let too_high = run(&mch, &bad_tool, &json!({}));
    assert!(too_high.warnings.iter().any(|w| w.contains("D1") && w.contains("D99")), "{:?}", too_high.warnings);

    let tilted = cnc::serialize(&[
        param("OperationName", json!("Tilt")),
        param("SpindleSpeed", json!(1500.0)),
        param("ClearancePlane", json!(10.0)),
        cnc::Record::Tool { number: 1, name: "T1".into() },
        cnc::Record::Section { begin: true },
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Rapid, x: 0.0, y: 0.0, z: 10.0, feed: None, center: None, ccw: false,
            plane: ArcPlane::Xy, abc: Some([15.0, 0.0, 0.0]),
        }),
        cnc::Record::Section { begin: false },
    ]);
    let tilt_out = run(&mch, &tilted, &json!({}));
    assert!(tilt_out.warnings.iter().any(|w| w.contains("3-axis")), "{:?}", tilt_out.warnings);

    let probe_stream = cnc::serialize(&[
        param("OperationName", json!("Probe1")),
        param("ToolKind", json!("TOUCH PROBE")),
        param("SpindleSpeed", json!(0.0)),
        param("ClearancePlane", json!(10.0)),
        cnc::Record::Tool { number: 9, name: "PROBE".into() },
        cnc::Record::Section { begin: true },
        cnc::Record::Touch(camcraft_post::post::probe_rec::TouchRec {
            target: [10.0, 0.0, 0.0], expect: [10.0, 0.0, 0.0], feed: 100.0, measure: Some(1),
        }),
        cnc::Record::Section { begin: false },
    ]);
    let probe_out = run(&mch, &probe_stream, &json!({}));
    assert!(!probe_out.warnings.is_empty(), "probing must be refused cleanly");
}

#[test]
fn arcs_comp_and_dwell_match_810m_dialect() {
    let mch = emco_vmc200_mirror();
    let recs = vec![
        param("OperationName", json!("ArcComp")),
        param("SpindleSpeed", json!(1800.0)),
        param("ClearancePlane", json!(15.0)),
        param("RetractPlane", json!(5.0)),
        cnc::Record::Tool { number: 1, name: "FLAT-6".into() },
        cnc::Record::Section { begin: true },
        mv(MoveKind::Rapid, 10.0, 0.0, 5.0, None),
        mv(MoveKind::Feed, 10.0, 0.0, -2.0, Some(200.0)),
        cnc::Record::Comp { side: Some(CompSide::Left), radius_mm: 3.0, wear: true },
        mv(MoveKind::Feed, 20.0, 0.0, -2.0, Some(500.0)),
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 0.0, y: 20.0, z: -2.0, feed: Some(500.0),
            center: Some((0.0, 0.0)), ccw: true, plane: ArcPlane::Xy, abc: None,
        }),
        cnc::Record::Comp { side: None, radius_mm: 0.0, wear: false },
        mv(MoveKind::Feed, 0.0, 30.0, -2.0, Some(500.0)),
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 10.0, y: 30.0, z: -4.0, feed: Some(400.0),
            center: Some((10.0, -2.0)), ccw: false, plane: ArcPlane::Xz, abc: None,
        }),
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 10.0, y: 40.0, z: -6.0, feed: Some(400.0),
            center: Some((40.0, -4.0)), ccw: true, plane: ArcPlane::Yz, abc: None,
        }),
        cnc::Record::Move(cnc::MoveRec {
            kind: MoveKind::Feed, x: 10.0, y: 50.0, z: -7.0, feed: Some(400.0),
            center: Some((10.0, 45.0)), ccw: false, plane: ArcPlane::Xy, abc: None,
        }),
        mv(MoveKind::Rapid, 10.0, 50.0, 15.0, None),
        cnc::Record::Section { begin: false },
    ];
    let out = run(&mch, &cnc::serialize(&recs), &json!({}));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let p = &out.program;
    assert!(p.contains("G41 "), "{p}");
    assert!(p.contains("G40 "), "{p}");
    assert!(p.contains("(WEAR COMP - D1 HOLDS WEAR DELTA ONLY)"), "{p}");
    assert!(!p.contains("NORM") && !p.contains("KONT") && !p.contains("CR="), "{p}");
    assert!(p.contains("G18 G2 X10 Y30 Z-4 I10 K0 F400"), "{p}");
    assert!(p.contains("G19 G3 X10 Y40 Z-6 J10 K0 F400"), "{p}");
    assert!(p.contains("G17 G2 X10 Y50 Z-7 I0 J5 F400"), "{p}");

    // Plane change while G41 is active must be refused by name.
    let bad_plane = recs.into_iter().filter(|r| !matches!(r, cnc::Record::Comp { side: None, .. })).collect::<Vec<_>>();
    let plane_refused = run(&mch, &cnc::serialize(&bad_plane), &json!({}));
    assert!(plane_refused.warnings.iter().any(|w| w.contains("plane change G18")), "{:?}", plane_refused.warnings);
}

#[test]
fn canned_cycles_emit_l81_to_l89_with_r_parameters_and_safe_hops() {
    let mch = emco_vmc200_mirror();
    let cases: &[(CycleKind, f64, f64, &str, &[&str])] = &[
        (CycleKind::Drill, 0.0, 0.0, "L81 P1", &["R02=1", "R03=-10", "R10=4"]),
        (CycleKind::Spot, 0.0, 0.5, "L82 P1", &["R02=1", "R03=-10", "R04=0.5", "R10=4"]),
        (CycleKind::Peck, 3.0, 0.25, "L83 P1", &["R00=0", "R01=3", "R02=1", "R03=-10", "R04=0.25", "R05=0", "R10=4", "R11=0"]),
        (CycleKind::Deep, 4.0, 0.0, "L83 P1", &["R01=4", "R11=1"]),
        (CycleKind::Tap, 0.0, 0.0, "L84 P1", &["R02=1", "R03=-10", "R06=0", "R08=-3", "R09=1.5", "R10=4", "R11=3"]),
        (CycleKind::FloatingTap, 0.0, 0.2, "L84 P1", &["R04=0.2", "R06=4", "R07=3", "R08=-4", "R10=4", "R11=3"]),
        (CycleKind::Ream, 0.0, 0.0, "L85 P1", &["R02=1", "R03=-10", "R04=0", "R10=4", "R16=150", "R17=150"]),
        (CycleKind::Bore, 0.0, 0.5, "L86 P1", &["R02=1", "R03=-10", "R04=0.5", "R07=3", "R10=4", "R12=0", "R13=0"]),
        (CycleKind::BackBore, 0.0, 0.5, "L89 P1", &["R02=1", "R03=-10", "R04=0.5", "R10=4"]),
    ];
    for (kind, peck, dwell, call, expect_words) in cases {
        let mut b = PathBuilder::new(Point3::new(10.0, 10.0, 12.0));
        for (i, &x) in [10.0, 30.0].iter().enumerate() {
            if i > 0 { b.push(MoveKind::Rapid, Point3::new(x, 10.0, 4.0), None); }
            b.push(MoveKind::Rapid, Point3::new(x, 10.0, 1.0), None);
            let fm = b.moves_len();
            b.push(MoveKind::Plunge, Point3::new(x, 10.0, -10.0), Some(150.0));
            b.push(MoveKind::Rapid, Point3::new(x, 10.0, 1.0), None);
            b.push_cycle(Cycle { kind: *kind, xy: Point2::new(x, 10.0), r_z: 1.0, bottom_z: -10.0, entry_z: 0.0, peck: *peck, dwell: *dwell, first_move: fm });
            b.push(MoveKind::Rapid, Point3::new(x, 10.0, 4.0), None);
        }
        let path = b.finish();
        let d = Dump {
            op_name: "CycleOp", tool_number: 3, tool_name: "TOOL3", rpm: 100.0,
            clearance_z: 12.0, retract_z: 4.0, top_z: 0.0, ..Dump::default()
        };
        let out = run(&mch, &cnc::serialize(&dump(&path, &d)), &json!({}));
        assert!(out.warnings.is_empty(), "{kind:?}: {:?}", out.warnings);
        assert_eq!(out.program.matches(call).count(), 2, "{kind:?}: two holes call {call}:\n{}", out.program);
        for w in *expect_words {
            assert!(out.program.contains(w), "{kind:?}: expected {w} in:\n{}", out.program);
        }
        assert!(!out.program.contains("CYCLE8"), "{kind:?}: 840D CYCLE8x forbidden:\n{}", out.program);
        assert!(!out.program.contains("Z-10"), "{kind:?}: expanded plunge Z-10 must be suppressed:\n{}", out.program);
    }

    // Rigid tap with dwell > 0 is refused on 810M (R04 only valid without encoder R08=-4).
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, 10.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    let fm = b.moves_len();
    b.push(MoveKind::Feed, Point3::new(10.0, 10.0, -8.0), Some(150.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    b.push_cycle(Cycle { kind: CycleKind::Tap, xy: Point2::new(10.0, 10.0), r_z: 1.0, bottom_z: -8.0, entry_z: 0.0, peck: 0.0, dwell: 0.3, first_move: fm });
    let tap_dwell = dump(&b.finish(), &Dump { op_name: "TapDwell", tool_number: 3, tool_name: "M6", rpm: 100.0, clearance_z: 10.0, retract_z: 4.0, ..Dump::default() });
    let tap_refused = run(&mch, &cnc::serialize(&tap_dwell), &json!({}));
    assert!(tap_refused.warnings.iter().any(|w| w.contains("L84") && w.contains("dwell")), "{:?}", tap_refused.warnings);
}

#[test]
fn emco_vmc_200_specs_and_explain_mode() {
    let mch = parse_mch(&read("assets/machines/emco-vmc-200.mch")).expect("emco-vmc-200 parses");
    assert!(mch.is_sane());
    assert_eq!(mch.kind.as_deref(), Some("mill"));
    assert_eq!(mch.spindle_min, Some(50.0));
    assert_eq!(mch.spindle_max, Some(4500.0));
    assert_eq!(mch.safe_retract_z, Some(0.0));
    assert_eq!(mch.tool_count, Some(12));
    assert_eq!(mch.post.as_deref(), Some("Siemens 810M"));
    assert_eq!(mch.coolants.len(), 1);
    assert_eq!(mch.coolants[0].m_code, "M8");
    let x = mch.axis(0).expect("X");
    let y = mch.axis(1).expect("Y");
    let z = mch.axis(2).expect("Z");
    assert_eq!(x.travel, Some((-420.0, 420.0)));
    assert_eq!(y.travel, Some((-330.0, 330.0)));
    assert_eq!(z.travel, Some((-240.0, 240.0)));
    assert_eq!(x.rapid, Some(10500.0));
    assert_eq!(x.feed, Some(4000.0));

    let explained = run(&machine::mirror(&mch), &simple_stream(json!(1), vec![]), &json!({ "Explain": true }));
    assert!(explained.warnings.is_empty(), "{:?}", explained.warnings);
    assert!(explained.program.contains("MM GEOMETRY INPUT"), "{}", explained.program);
    for l in explained.program.lines() {
        assert!(l.len() <= 120, "810M block limit with Explain: {l}");
    }
}

fn single_cycle_records(
    op_name: &str,
    kind: CycleKind,
    peck: f64,
    dwell: f64,
    feed: f64,
    reverse: bool,
) -> Vec<cnc::Record> {
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, 12.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    let fm = b.moves_len();
    b.push(MoveKind::Plunge, Point3::new(10.0, 10.0, -10.0), Some(feed));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    b.push_cycle(Cycle {
        kind,
        xy: Point2::new(10.0, 10.0),
        r_z: 1.0,
        bottom_z: -10.0,
        entry_z: 0.0,
        peck,
        dwell,
        first_move: fm,
    });
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 4.0), None);
    let d = Dump {
        op_name,
        tool_number: 3,
        tool_name: "TOOL3",
        rpm: 100.0,
        clearance_z: 12.0,
        retract_z: 4.0,
        top_z: 0.0,
        ..Dump::default()
    };
    let mut recs = dump(&b.finish(), &d);
    if reverse {
        camcraft_post::post::ask_reverse_spindle(&mut recs);
    }
    recs
}

fn line_before_call<'a>(program: &'a str, call: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = program.lines().collect();
    lines
        .windows(2)
        .filter_map(|w| if w[1] == call { Some(w[0]) } else { None })
        .collect()
}

#[test]
fn cycles_preserve_forward_and_reverse_spindle_direction() {
    let mch = emco_vmc200_mirror();

    // Reverse FLOATINGTAP: S100 M4, retract CW (R06=3), resume CCW (R07=4).
    let rev_tap = single_cycle_records("RevTap", CycleKind::FloatingTap, 0.0, 0.2, 150.0, true);
    let rev_tap_out = run(&mch, &cnc::serialize(&rev_tap), &json!({}));
    assert!(rev_tap_out.warnings.is_empty(), "{:?}", rev_tap_out.warnings);
    assert!(rev_tap_out.program.contains("S100 M4\n"), "{}", rev_tap_out.program);
    let rev_tap_params = line_before_call(&rev_tap_out.program, "L84 P1");
    assert_eq!(rev_tap_params.len(), 1, "{}", rev_tap_out.program);
    assert!(
        rev_tap_params[0].contains("R06=3") && rev_tap_params[0].contains("R07=4") && rev_tap_params[0].contains("R08=-4"),
        "reverse floating tap must retract CW (R06=3) and resume CCW (R07=4):\n{}",
        rev_tap_out.program
    );

    // Forward FLOATINGTAP: S100 M3, retract CCW (R06=4), resume CW (R07=3).
    let fwd_tap = single_cycle_records("FwdTap", CycleKind::FloatingTap, 0.0, 0.2, 150.0, false);
    let fwd_tap_out = run(&mch, &cnc::serialize(&fwd_tap), &json!({}));
    assert!(fwd_tap_out.warnings.is_empty(), "{:?}", fwd_tap_out.warnings);
    assert!(fwd_tap_out.program.contains("S100 M3\n"), "{}", fwd_tap_out.program);
    let fwd_tap_params = line_before_call(&fwd_tap_out.program, "L84 P1");
    assert_eq!(fwd_tap_params.len(), 1, "{}", fwd_tap_out.program);
    assert!(
        fwd_tap_params[0].contains("R06=4") && fwd_tap_params[0].contains("R07=3") && fwd_tap_params[0].contains("R08=-4"),
        "forward floating tap must retract CCW (R06=4) and resume CW (R07=3):\n{}",
        fwd_tap_out.program
    );

    // Reverse BORE (L86): S100 M4, R07=4.
    let rev_bore = single_cycle_records("RevBore", CycleKind::Bore, 0.0, 0.5, 150.0, true);
    let rev_bore_out = run(&mch, &cnc::serialize(&rev_bore), &json!({}));
    assert!(rev_bore_out.warnings.is_empty(), "{:?}", rev_bore_out.warnings);
    assert!(rev_bore_out.program.contains("S100 M4\n"), "{}", rev_bore_out.program);
    let rev_bore_params = line_before_call(&rev_bore_out.program, "L86 P1");
    assert_eq!(rev_bore_params.len(), 1, "{}", rev_bore_out.program);
    assert!(
        rev_bore_params[0].contains("R07=4"),
        "reverse bore must keep R07=4:\n{}",
        rev_bore_out.program
    );

    // Forward BORE (L86) after a reverse section must not inherit spinRev.
    let moved = rev_tap.iter().filter(|r| matches!(r, cnc::Record::Move(_))).count();
    let mut fwd_bore = single_cycle_records("FwdBore", CycleKind::Bore, 0.0, 0.5, 150.0, false);
    for r in &mut fwd_bore {
        if let cnc::Record::Cycle(c) = r {
            c.first_move += moved;
        }
    }
    let mut combined = rev_tap;
    combined.extend(fwd_bore);
    let combined_out = run(&mch, &cnc::serialize(&combined), &json!({}));
    assert!(combined_out.warnings.is_empty(), "{:?}", combined_out.warnings);
    assert!(combined_out.program.contains("S100 M4\n"), "{}", combined_out.program);
    assert!(combined_out.program.contains("S100 M3\n"), "{}", combined_out.program);
    let bore_params = line_before_call(&combined_out.program, "L86 P1");
    assert_eq!(bore_params.len(), 1, "{}", combined_out.program);
    assert!(
        bore_params[0].contains("R07=3"),
        "subsequent forward section must reset spindle direction to R07=3:\n{}",
        combined_out.program
    );
}

#[test]
fn deep_to_tap_initializes_z_axis_before_every_l84() {
    let mch = emco_vmc200_mirror();
    let mut b = PathBuilder::new(Point3::new(10.0, 10.0, 12.0));

    // Hole 1: DEEP (L83) writes R11=1.
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    let fm1 = b.moves_len();
    b.push(MoveKind::Plunge, Point3::new(10.0, 10.0, -20.0), Some(120.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 1.0), None);
    b.push_cycle(Cycle {
        kind: CycleKind::Deep,
        xy: Point2::new(10.0, 10.0),
        r_z: 1.0,
        bottom_z: -20.0,
        entry_z: 0.0,
        peck: 4.0,
        dwell: 0.0,
        first_move: fm1,
    });
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 4.0), None);

    // Hole 2: rigid TAP (L84 R08=-3).
    b.push(MoveKind::Rapid, Point3::new(30.0, 10.0, 4.0), None);
    b.push(MoveKind::Rapid, Point3::new(30.0, 10.0, 1.0), None);
    let fm2 = b.moves_len();
    b.push(MoveKind::Plunge, Point3::new(30.0, 10.0, -12.0), Some(150.0));
    b.push(MoveKind::Rapid, Point3::new(30.0, 10.0, 1.0), None);
    b.push_cycle(Cycle {
        kind: CycleKind::Tap,
        xy: Point2::new(30.0, 10.0),
        r_z: 1.0,
        bottom_z: -12.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: fm2,
    });
    b.push(MoveKind::Rapid, Point3::new(30.0, 10.0, 4.0), None);

    // Hole 3: floating TAP (L84 R08=-4).
    b.push(MoveKind::Rapid, Point3::new(50.0, 10.0, 4.0), None);
    b.push(MoveKind::Rapid, Point3::new(50.0, 10.0, 1.0), None);
    let fm3 = b.moves_len();
    b.push(MoveKind::Plunge, Point3::new(50.0, 10.0, -12.0), Some(150.0));
    b.push(MoveKind::Rapid, Point3::new(50.0, 10.0, 1.0), None);
    b.push_cycle(Cycle {
        kind: CycleKind::FloatingTap,
        xy: Point2::new(50.0, 10.0),
        r_z: 1.0,
        bottom_z: -12.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.2,
        first_move: fm3,
    });
    b.push(MoveKind::Rapid, Point3::new(50.0, 10.0, 4.0), None);

    let d = Dump {
        op_name: "DeepThenTap",
        tool_number: 4,
        tool_name: "TAP4",
        rpm: 100.0,
        clearance_z: 12.0,
        retract_z: 4.0,
        top_z: 0.0,
        ..Dump::default()
    };
    let out = run(&mch, &cnc::serialize(&dump(&b.finish(), &d)), &json!({}));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    let l83_params = line_before_call(&out.program, "L83 P1");
    assert_eq!(l83_params.len(), 1, "{}", out.program);
    assert!(l83_params[0].contains("R11=1"), "DEEP sets R11=1:\n{}", out.program);

    let l84_params = line_before_call(&out.program, "L84 P1");
    assert_eq!(l84_params.len(), 2, "two L84 calls expected:\n{}", out.program);
    assert!(
        l84_params[0].contains("R08=-3") && l84_params[0].contains("R11=3"),
        "rigid L84 after L83 DEEP must reset R11=3 (Z axis):\n{}",
        out.program
    );
    assert!(
        l84_params[1].contains("R08=-4") && l84_params[1].contains("R11=3"),
        "floating L84 must initialize R11=3 (Z axis):\n{}",
        out.program
    );
}

#[test]
fn ream_initializes_cutting_and_retraction_feeds() {
    let mch = emco_vmc200_mirror();
    for (feed, dwell, want) in [(150.0, 0.0, "R16=150 R17=150"), (240.0, 0.25, "R16=240 R17=240")] {
        let recs = single_cycle_records("ReamOp", CycleKind::Ream, 0.0, dwell, feed, false);
        let out = run(&mch, &cnc::serialize(&recs), &json!({}));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let params = line_before_call(&out.program, "L85 P1");
        assert_eq!(params.len(), 1, "{}", out.program);
        assert!(
            params[0].contains(want),
            "L85 must initialize cutting feed R16 and retraction feed R17 ({want}):\n{}",
            out.program
        );
    }
}

#[test]
fn bore_initializes_lateral_retraction_offsets() {
    let mch = emco_vmc200_mirror();
    for reverse in [false, true] {
        let recs = single_cycle_records("BoreOp", CycleKind::Bore, 0.0, 0.5, 150.0, reverse);
        let out = run(&mch, &cnc::serialize(&recs), &json!({}));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let params = line_before_call(&out.program, "L86 P1");
        assert_eq!(params.len(), 1, "{}", out.program);
        assert!(
            params[0].contains("R12=0") && params[0].contains("R13=0"),
            "L86 must initialize lateral retraction offsets R12=0 and R13=0:\n{}",
            out.program
        );
    }
}

//! Post engine core tests (M1.7a): the intermediate's two directions,
//! the driver's hook contract, and the clean-refusal law on every
//! malformed path.
//!
//! Gate map (M4.4): covers camcraft_post::cl, ::geo2d, ::geometry,
//! ::post.

use camcraft_post::cl::{CycleKind, MoveKind, Toolpath};
use camcraft_post::post::cnc::{self, CycleRec, MoveRec, Record};
use camcraft_post::post::{engine, machine, normalize_program_order, Dump, PostError};
use serde_json::json;

/// The `.cl` goldens are generator output (written by app's golden_cl),
/// so they stay in app's fixture dir; this suite reads them as the
/// post's input side of the same contract (M4.1 R3 note in
/// docs/M4.1-CRATE-SPLIT.md).
fn golden(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/tests/golden")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {name}: {e}"))
}

/// A stream with one of everything, in program order.
fn sample() -> Vec<Record> {
    vec![
        Record::Param { name: "SpindleSpeed".into(), value: json!(8000.0) },
        Record::Param { name: "OperationName".into(), value: json!("Face Mill 40") },
        Record::Warn { text: "shank band grazes stock".into() },
        Record::Tool { number: 3, name: "1/4 INCH 4FL".into() },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Rapid,
            x: 1.5,
            y: -2.0,
            z: 0.0,
            feed: None,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 1.5,
            y: -2.0,
            z: -1.0,
            feed: Some(1200.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 2.0,
            y: -2.0,
            z: -1.0,
            feed: Some(1200.0),
            center: Some((1.5, -2.0)),
            ccw: true,
            plane: Default::default(),
            abc: None,
        }),
        Record::Section { begin: false },
    ]
}

#[test]
fn v1_roundtrip_is_exact() {
    let records = sample();
    let text = cnc::serialize(&records);
    let back = cnc::parse(&text).expect("own output must parse");
    assert_eq!(back, records, "serialize/parse must be an identity");
}

#[test]
fn malformed_streams_refuse_cleanly() {
    for bad in [
        "Goto X 1 Y 2\n",
        "Feed X\n",
        "Feed X q Y 0 Z 0\n",
        "Arc 1 2\n",
        "Arc CW X 1 Y 2 I 0\n",
        "PARAM Speed\n",
        "TOOL abc Name\n",
        "; CYCLE NOPE X0\n",
        "; CYCLE DRILL ZAP 3\n",
        "Feed X nan Y 0 Z 0\n",
        "Feed 1 2 3 4 5 6\n",
    ] {
        let e = cnc::parse(bad).expect_err("degenerate input must error, never crash");
        assert!(format!("{e}").contains("line"), "error must name its line: {e}");
    }
}

#[test]
fn every_golden_is_a_legal_post_input() {
    // The superset law: `.cl` goldens parse by the legacy rules.
    for name in [
        "contour_disc_hole.cl",
        "contour_plate_with_hole.cl",
        "contour_tangential_lead.cl",
        "face_square_40x40.cl",
        "face_square_40x40_stepped.cl",
        "pocket_waterline_plate_with_island.cl",
        "pocket_zigzag_plate_with_island.cl",
        "slot_fullwidth_centerline.cl",
        "slot_wide_channel_rows.cl",
        "trace_plate_with_hole.cl",
        "drill_plate.cl",
        "drill_peck.cl",
        "tap_plate.cl",
    ] {
        let records = cnc::parse(&golden(name))
            .unwrap_or_else(|e| panic!("golden {name} must parse: {e}"));
        let moves = records.iter().filter(|r| matches!(r, Record::Move(_))).count();
        // Drilling goldens are hole-group chains - a tap can be one
        // cycle on four motion records. Count intent, not just moves.
        let records_here = moves
            + records.iter().filter(|r| matches!(r, Record::Cycle(_))).count();
        let floor = if name.starts_with("drill") || name.starts_with("tap") { 4 } else { 10 };
        assert!(records_here > floor, "{name}: {records_here} records is not a toolpath");
        if name.starts_with("drill") || name.starts_with("tap") {
            assert!(
                records.iter().any(|r| matches!(r, Record::Cycle(_))),
                "{name} carries v3 cycle markers"
            );
        }
    }
}

#[test]
fn legacy_markers_normalize_to_program_order() {
    // v3 goldens append their `; CYCLE` lines after the chain; the
    // driver re-seats each cycle before its AT-anchored move.
    let records = cnc::parse(&golden("drill_plate.cl")).expect("parses");
    let norm = normalize_program_order(records.clone());
    assert_ne!(norm, records, "the golden appends its markers: order changes");
    // In the normalized stream each cycle sits at (or before) its
    // anchor: the moves counted when the cycle emits equal AT.
    let mut mi = 0usize;
    for r in &norm {
        match r {
            Record::Move(_) => mi += 1,
            Record::Cycle(c) => assert!(
                mi <= c.first_move,
                "cycle for move {} emitted after {mi} moves",
                c.first_move
            ),
            _ => {}
        }
    }
    assert!(norm.iter().any(|r| matches!(r, Record::Move(_))));
    // Idempotence: re-normalizing changes nothing.
    assert_eq!(normalize_program_order(norm.clone()), norm);
}

const ECHO_CPS: &str = r#"
setLanguageVersion(3);
function onOpen() { writeln("ECHO"); }
function onCommand(c) {
  writeln(";cmd " + c + " from " + formatUnit(currentPosition.X) + " " + formatUnit(currentPosition.Z));
  return true;
}
function onSpindleSpeed() { writeBlock("S" + formatFeed(properties.SpindleSpeed)); }
function onToolChange(n, name) { writeBlock(";tool", n, name); }
function onRapid() { writeBlock("RAPID", formatUnit(properties.X), formatUnit(properties.Y), formatUnit(properties.Z)); }
function onLinear() {
  writeBlock("LIN", formatUnit(properties.X), formatUnit(properties.Y), formatUnit(properties.Z), properties.FeedRate === null ? "" : "F" + formatFeed(properties.FeedRate));
}
function onCircular() { writeBlock("ARC", properties.CircularDirection, formatUnit(properties.X)); }
function onCyclePoint() { writeBlock("CYC", properties.CycleType, formatUnit(properties.X), formatUnit(properties.R), formatUnit(properties.Z)); }
function onClose() { writeln("DONE"); }
"#;

#[test]
fn echo_script_drives_hooks_and_line_map() {
    let text = cnc::serialize(&sample());
    let out = engine::run(ECHO_CPS, &json!(null), &text).expect("echo post");
    assert!(out.program.starts_with("ECHO\n"));
    // The PARAM arm publishes properties (onSpindleSpeed read the
    // rpm), and the TOOL record fired onToolChange with both fields.
    assert!(out.program.contains("S8000\n"), "{}", out.program);
    assert!(out.program.contains(";tool 3 1/4 INCH 4FL\n"));
    assert!(out.program.contains("RAPID 1.5 -2 0\n"));
    assert!(out.program.contains("LIN 1.5 -2 -1 F1200\n"));
    assert!(out.program.contains("ARC CCW 2\n"));
    assert!(out.program.ends_with("DONE\n"));
    // The stream warning rode through, counted not printed.
    assert_eq!(out.warnings, vec!["shank band grazes stock".to_string()]);
    // line_map: the RAPID line belongs to the first Move record.
    let rapid_line = out.program.lines().position(|l| l.starts_with("RAPID")).unwrap();
    let mapped = out.line_map.iter().find(|li| li.line == rapid_line).expect("mapped");
    let rec = mapped.record.expect("a record line");
    // The mapped record's index is past the params/tool block.
    assert!(rec >= 4, "params and tool come first; got record {rec}");
    // Header/footer lines: onClose's DONE maps to None.
    let done_line = out.program.lines().count() - 1;
    assert!(out.line_map.iter().any(|li| li.line == done_line && li.record.is_none()));
}

#[test]
fn on_command_false_swallows_but_chains() {
    let cps = "function onCommand(c) { return c !== \"Rapid\"; }\n\
               function onLinear() { writeBlock(\"LIN\", formatUnit(properties.X)); }\n\
               function onRapid() { writeBlock(\"SHOULD NOT APPEAR\"); }";
    let text = cnc::serialize(&sample());
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert!(!out.program.contains("SHOULD NOT APPEAR"));
    assert_eq!(out.swallowed, 1);
    // The feed after the swallowed rapid still starts from its end
    // point: onCommand's currentPosition proves it.
    let cps2 = "function onCommand(c) { if (c === \"Linear\") writeln(\"AT \" + formatUnit(currentPosition.X) + \" \" + formatUnit(currentPosition.Y)); return false; }";
    let out2 = engine::run(cps2, &json!(null), &text).expect("post");
    assert!(out2.program.contains("AT 1.5 -2\n"), "{}", out2.program);
}

#[test]
fn cycle_sequence_and_dwell_gate() {
    let cps = "var seq = [];\n\
               function onCycle() { seq.push(\"C\"); }\n\
               function onCyclePoint() { seq.push(\"P\"); }\n\
               function onDwell(v) { seq.push(\"D\" + v); }\n\n\
               function onCycleEnd() { seq.push(\"E\"); }\n\
               function onClose() { writeln(seq.join(\" \")); }";
    let mut recs = vec![Record::Section { begin: true }];
    recs.push(Record::Cycle(CycleRec {
        kind: CycleKind::Peck,
        x: 1.0,
        y: 2.0,
        r_z: 5.0,
        bottom_z: -3.0,
        entry_z: 0.0,
        peck: 1.0,
        dwell: 0.0,
        first_move: 0,
        feed: None,
    }));
    recs.push(Record::Cycle(CycleRec {
        kind: CycleKind::Deep,
        x: 1.0,
        y: 2.0,
        r_z: 5.0,
        bottom_z: -8.0,
        entry_z: 0.0,
        peck: 2.0,
        dwell: 0.5,
        first_move: 0,
        feed: None,
    }));
    recs.push(Record::Section { begin: false });
    let out = engine::run(cps, &json!(null), &cnc::serialize(&recs)).expect("post");
    assert!(out.program.contains("C P E C P D0.5 E\n"), "{}", out.program);
    // The chain continues from the last cycle's R plane.
    assert_eq!(out.swallowed, 0);
}

#[test]
fn script_errors_are_refusals() {
    let cps = "function onLinear() { undefinedFn(); }";
    let text = cnc::serialize(&sample());
    let e = engine::run(cps, &json!(null), &text).expect_err("a throwing script must refuse");
    assert!(matches!(e, PostError::Script(_)), "{e:?}");
    assert!(!format!("{e}").is_empty(), "the refusal names the failure");
}

#[test]
fn empty_stream_is_a_clean_two_line_program() {
    let out = engine::run(ECHO_CPS, &json!(null), "").expect("empty parses");
    assert_eq!(out.program, "ECHO\nDONE\n");
    assert!(out.line_map.iter().all(|li| li.record.is_none()));
}

#[test]
fn messages_count_as_warnings() {
    let cps = "function onLinear() { message(\"feed over machine max\"); }";
    let text = cnc::serialize(&sample());
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert_eq!(out.warnings.len(), 2, "stream warn + script message");
    assert_eq!(out.warnings[1], "feed over machine max");
}

#[test]
fn machine_flattens_for_the_script() {
    let mch = json!({
        "name": "VMC 3-AXIS",
        "capabilities": { "toolChange": true },
        "kinematics": { "static": {
            "head": { "spindle": { "min": 0.0, "max": 12000.0 },
                      "toolStation": { "coolants": [
                          { "name": "Flood", "mCode": "M8" }, "Mist" ] } },
            "axes": [
                { "letter": "X", "speeds": { "rapid": 6000.0, "max": 5000.0 } },
                { "letter": "Z", "speeds": { "rapid": 4000.0, "max": 3000.0 } } ]
        } }
    });
    let p = machine::script_props(&mch);
    assert_eq!(p["rapidRate"], json!(6000.0));
    assert_eq!(p["spindleMax"], json!(12000.0));
    assert_eq!(p["coolants"].as_array().map(Vec::len), Some(2));
    assert_eq!(p["name"], json!("VMC 3-AXIS"));
    // A script reads it as the documented global.
    let cps = "function onClose() { writeln(\"M \" + machine.name + \" \" + machine.rapidRate + \" \" + machine.spindleMax); }";
    let out = engine::run(cps, &mch, "").expect("post");
    assert_eq!(out.program, "M VMC 3-AXIS 6000 12000\n");
}

#[test]
fn the_spindle_inventory_crosses_every_script_door() {
    // M5.3 C: a dialect script asks `machine.spindles > 1` before it
    // handshakes, and the answer must be the FILE's - through the
    // mirror, through the serde field, through a raw .mch parts
    // tree - never a number the reader invented for a machine that
    // said nothing (the unstated-limit law with its own door).
    let mirror = json!({ "kinematics": { "static": { "head": { "spindleCount": 2 } } } });
    assert_eq!(machine::script_props(&mirror)["spindles"], json!(2));
    let serde_form = json!({ "spindles": 3 });
    assert_eq!(machine::script_props(&serde_form)["spindles"], json!(3));
    let raw = json!({ "kinematics": { "default": { "parts": [
        { "type": "head", "spindle": { "max": 4000.0 }, "parts": [
            { "type": "axis", "letter": "Z" } ] },
        { "type": "head", "spindle": 0 },
        { "type": "head", "name": "no spindle stated" } ] } } });
    assert_eq!(machine::script_props(&raw)["spindles"], json!(2), "heads stating a spindle, tree-walked");
    // Unstated stays absent - `machine.spindles > 1` reads false in
    // a script, and a millturn dialect refuses to handoff, not guesses.
    let quiet = json!({ "name": "PLAIN" });
    assert!(machine::script_props(&quiet)["spindles"].is_null());
    let cps = "function onClose() { writeln(machine.spindles === undefined || machine.spindles === null ? \"silent\" : \"n=\" + machine.spindles); }";
    let out = engine::run(cps, &quiet, "").expect("post");
    assert_eq!(out.program, "silent\n", "the script sees the silence, not a 1");
}

#[test]
fn the_mirror_states_spindles_only_when_the_file_did() {
    let m = camcraft_post::machine::Machine { model: "MT".into(), spindles: Some(2), ..Default::default() };
    let v = machine::mirror(&m);
    assert_eq!(v["kinematics"]["static"]["head"]["spindleCount"], json!(2));
    let m = camcraft_post::machine::Machine { model: "VMC".into(), ..Default::default() };
    assert!(machine::mirror(&m)["kinematics"]["static"]["head"].get("spindleCount").is_none());
}

#[test]
fn dump_interleaves_cycles_at_their_anchor_moves() {
    use camcraft_post::geo2d::Point2;
    use camcraft_post::geometry::Point3;
    let mut t = Toolpath::default();
    for i in 0..4 {
        let (kind, z) = if i % 2 == 0 { (MoveKind::Rapid, 10.0) } else { (MoveKind::Plunge, -5.0) };
        t.moves.push(camcraft_post::cl::Move {
            rotary: None,
            kind,
            from: Point3 { x: 0.0, y: 0.0, z: 10.0 },
            to: Point3 { x: i as f64, y: 0.0, z },
            feed: if kind == MoveKind::Rapid { None } else { Some(900.0) },
            arc: None,
            axis: None,
        });
    }
    t.cycles = vec![
        camcraft_post::cl::Cycle {
            kind: CycleKind::Drill,
            xy: Point2 { x: 0.0, y: 0.0 },
            r_z: 5.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 0,
        },
        camcraft_post::cl::Cycle {
            kind: CycleKind::Tap,
            xy: Point2 { x: 2.0, y: 0.0 },
            r_z: 5.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 2,
        },
    ];
    let d = Dump {
        op_name: "Drill1",
        part_load: None,
        part_unload: false,
        tool_number: 1,
        tool_name: "DR-0300",
        tool_diameter: 3.0,
        rpm: 2500.0,
        clearance_z: 12.0,
        retract_z: 10.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec!["drill vs hole dia advisory".into()],
        part_transfer: false,
        probe_tol: None,
        coolant: Default::default(),
        after: Default::default(),
        probe_result: None,
        ..Default::default()
    };
    let recs = camcraft_post::post::dump(&t, &d);
    // dump emits each cycle with first_move <= mi just before move mi:
    // the shape is c m m c m m ENDSECTION.
    let mut mi = 0usize;
    let mut seen_cycle_at: Vec<usize> = Vec::new();
    for r in &recs {
        match r {
            Record::Move(_) => mi += 1,
            Record::Cycle(c) => {
                assert!(c.first_move <= mi, "cycle after its anchor: {:?} vs {mi}", c.kind);
                seen_cycle_at.push(c.first_move);
            }
            _ => {}
        }
    }
    assert_eq!(seen_cycle_at, vec![0, 2]);
    // And the text round-trips through parse unchanged (program order
    // is stable under normalize).
    let text = cnc::serialize(&recs);
    let back = normalize_program_order(cnc::parse(&text).expect("parses"));
    assert_eq!(back, recs, "dump -> text -> parse is an identity");
}

#[test]
fn a_leaning_move_dumps_the_angles_a_stream_carries() {
    // Pose (M5.1 A) through the dump: a stated leaning axis becomes the
    // canonical letter angles the stream grammar already reads back
    // (stage D's keyed words); prismatic moves - no axis, or the exact
    // vertical - state nothing, so every committed golden's text is
    // unchanged, byte for byte.
    use camcraft_post::geometry::Point3;
    let (sa, ca) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    let feed = |x: f64| {
        camcraft_post::cl::Move::new(
            MoveKind::Feed,
            Point3 { x: x - 1.0, y: 0.0, z: 1.0 },
            Point3 { x, y: 0.0, z: 1.0 },
            Some(200.0),
        )
    };
    let m2 = feed(2.0).with_axis([sa, 0.0, ca]);
    let m3 = feed(3.0).with_axis([0.0, 0.0, 1.0]);
    let t = Toolpath::new(vec![feed(1.0), m2, m3]);
    let d = Dump {
        op_name: "Lean1",
        part_load: None,
        part_unload: false,
        tool_number: 1,
        tool_name: "EM-0400",
        tool_diameter: 4.0,
        rpm: 2500.0,
        clearance_z: 12.0,
        retract_z: 10.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec![],
        part_transfer: false,
        probe_tol: None,
        coolant: Default::default(),
        after: Default::default(),
        probe_result: None,
        ..Default::default()
    };
    let recs = camcraft_post::post::dump(&t, &d);
    let angles: Vec<Option<[f64; 3]>> = recs
        .iter()
        .filter_map(|r| match r {
            Record::Move(m) => Some(m.abc),
            _ => None,
        })
        .collect();
    assert_eq!(angles.len(), 3);
    assert_eq!(angles[0], None);
    let [a, b, c] = angles[1].expect("the lean states its angles");
    assert!(
        a.abs() < 1e-9 && (b - 20.0).abs() < 1e-9 && c.abs() < 1e-9,
        "20 deg toward +x is the canonical B 20: {angles:?}"
    );
    // The letters are modal on the control: the stated Z column after
    // a lean must COMMAND the zero angles, or the machine would cut the
    // "upright" move under the old lean (the pivot law's post half).
    assert_eq!(angles[2], Some([0.0, 0.0, 0.0]), "the return to vertical states its zeros");
    // The text carries the letters on the millidegree lattice, and the
    // reader takes them back exactly as the stream law defines them.
    let text = cnc::serialize(&recs);
    assert!(text.contains(" A 0 B 20000 C 0"), "{text}");
    let back = cnc::parse(&text).expect("parses");
    let got: Vec<Option<[f64; 3]>> = back
        .iter()
        .filter_map(|r| match r {
            Record::Move(m) => Some(m.abc),
            _ => None,
        })
        .collect();
    assert_eq!(got[1], Some([0.0, 20.0, 0.0]), "{got:?}");
    assert_eq!(got[0], None);
    assert_eq!(got[2], Some([0.0, 0.0, 0.0]));
    // A Z column stated with nothing leaning before it still states
    // nothing: prismatic streams stay byte-identical.
    let upright = Toolpath::new(vec![feed(1.0).with_axis([0.0, 0.0, 1.0]), feed(2.0)]);
    let recs = camcraft_post::post::dump(&upright, &d);
    assert!(
        recs.iter().all(|r| !matches!(r, Record::Move(m) if m.abc.is_some())),
        "an upright-only stream commands no angles"
    );
}

#[test]
fn coordinates_read_as_millimetres() {
    // The scale law (`scale=0.001mm`, µm integers both directions) is
    // pinned by value here - a symmetric serialize/parse mistake
    // survives the round-trip identity test and must not survive this.
    let a = cnc::parse("Feed X 1000 Y -2000 Z 3500 F 250000\n").expect("spaced");
    let Some(Record::Move(m)) = a.first() else {
        panic!("a move");
    };
    assert_eq!((m.x, m.y, m.z), (1.0, -2.0, 3.5));
    assert_eq!(m.feed, Some(250.0));
    assert!(cnc::serialize(&a).contains("Feed X 1000 Y -2000 Z 3500 F 250000"));
    // The goldens themselves: the 40x40 face plate spans +-45 mm.
    let face = cnc::parse(&golden("face_square_40x40.cl")).expect("parses");
    let first = face.iter().find_map(|r| match r {
        Record::Move(mv) if mv.kind == MoveKind::Feed => Some(mv),
        _ => None,
    });
    let m = first.expect("a feed move");
    assert_eq!(m.x, 45.0);
    assert_eq!(m.z, -2.0);
    assert_eq!(m.feed, Some(500.0));
}

#[test]
fn spaced_and_joined_key_forms_agree() {
    let spaced = "SECTION\nRapid X 1000 Y 2000 Z 3000\nENDSECTION\n";
    let joined = "SECTION\nRapid X1000 Y2000 Z3000\nENDSECTION\n";
    let legacy = "SECTION\nRapid 1000 2000 3000\nENDSECTION\n";
    let a = cnc::parse(spaced).expect("spaced");
    let b = cnc::parse(joined).expect("joined");
    let c = cnc::parse(legacy).expect("legacy positional");
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn keyed_with_equals_and_colon_forms_agree() {
    let a = cnc::parse("Feed X=1000 Y=2000 Z=3000 F=500000\n").expect("equals");
    let b = cnc::parse("Feed X:1000 Y:2000 Z:3000 F:500000\n").expect("colons");
    let (Some(Record::Move(m1)), Some(Record::Move(m2))) = (a.first(), b.first()) else {
        panic!("moves");
    };
    assert_eq!(m1, m2);
    assert_eq!(m1.feed, Some(500.0));
}

#[test]
fn tilted_moves_roundtrip_their_words() {
    // Stage D (RTCP): A/B/C ride the stream as keyed words in
    // milli-degrees - the same lattice as the um coordinates. A
    // tilted record states ALL THREE letters; unstated ones read
    // back zero, never a leftover. Absence of the words is the
    // vertical home, and every in-app record is that: the dump
    // writes no angles (prismatic by fact, not omission).
    let recs = vec![Record::Move(MoveRec {
        kind: MoveKind::Feed,
        x: 10.0,
        y: 0.0,
        z: -5.0,
        feed: Some(500.0),
        center: None,
        ccw: false,
        plane: Default::default(),
        abc: Some([40.0, -90.0, 0.125]),
    })];
    let text = cnc::serialize(&recs);
    assert!(text.contains("A 40000 B -90000 C 125"), "{text}");
    let back = cnc::parse(&text).expect("angles parse");
    assert_eq!(back, recs, "angles are identity through the stream");
    // One letter stated, the pair unstated: zeros come along, per law.
    let one = cnc::parse("Feed X 1 Y 1 Z 1 A 30000\n").expect("single word");
    let Some(Record::Move(m)) = one.first() else {
        panic!("move");
    };
    assert_eq!(m.abc, Some([30.0, 0.0, 0.0]));
    // And the plain record stays angle-free.
    let plain = cnc::parse("Feed X 1 Y 1 Z 1\n").expect("plain");
    let Some(Record::Move(m)) = plain.first() else {
        panic!("move");
    };
    assert_eq!(m.abc, None);
}

#[test]
fn the_trace_maps_records_to_lines() {
    // Post debugging (stage D): the engine says which record wrote
    // which lines - header and footer owned by no record, and a
    // swallowed motion showing up owning NOTHING, the loss visible.
    let cps = r#"
function onOpen() { writeln("; head"); }
function onRapid() { writeln("G00 X0"); }
function onLinear() { writeln("G01 X1"); }
function onCommand(cmd) { return cmd !== "Linear"; }
function onClose() { writeln("M30"); }
"#;
    let text = cnc::serialize(&[
        Record::Move(MoveRec {
            kind: MoveKind::Rapid, x: 0.0, y: 0.0, z: 0.0,
            feed: None, center: None, ccw: false, plane: Default::default(), abc: None,
        }),
        Record::Move(MoveRec {
            kind: MoveKind::Feed, x: 1.0, y: 0.0, z: 0.0,
            feed: Some(100.0), center: None, ccw: false, plane: Default::default(), abc: None,
        }),
    ]);
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert_eq!(out.swallowed, 1);
    assert_eq!(out.blocks.len(), 4, "header, both moves, end");
    let h = &out.blocks[0];
    assert_eq!((h.record.as_ref(), h.first, h.last), (None, 0, 1));
    assert_eq!(h.label, "header");
    let r = &out.blocks[1];
    assert_eq!((r.record, r.first, r.last), (Some(0), 1, 2));
    assert!(r.label.starts_with("rapid →"), "kind and place: {}", r.label);
    let f = &out.blocks[2];
    assert_eq!((f.record, f.first, f.last), (Some(1), 2, 2));
    assert!(f.label.contains("(no output)"), "swallowed is said: {}", f.label);
    let e = &out.blocks[3];
    assert_eq!((e.record.as_ref(), e.first, e.last, e.label.as_str()), (None, 2, 3, "end"));
}

// ---------------------------------------------------------------- M5.2 G.2
// The bar feeder's lifecycle: PART records are stream events, not
// commands - they bracket the section, voice through dedicated
// hooks, and round-trip their micron numbers exactly.

#[test]
fn part_events_roundtrip_and_strays_refuse() {
    for recs in [
        vec![Record::Part { loading: true, advance: 25.0 }],
        vec![Record::Part { loading: true, advance: 0.0 }],
        vec![Record::Part { loading: false, advance: 0.0 }],
    ] {
        let text = cnc::serialize(&recs);
        assert_eq!(cnc::parse(&text).expect("own output must parse"), recs);
    }
    // The three legal spellings of the same micron number.
    for w in ["ADVANCE 25000", "ADVANCE:25000", "ADVANCE25000"] {
        let text = format!("PART LOAD {w}\n");
        assert_eq!(
            cnc::parse(&text).expect("a legal ADVANCE spelling must read")[0],
            Record::Part { loading: true, advance: 25.0 },
            "{text}"
        );
    }
    for bad in [
        "PART\n",
        "PART FIZZ\n",
        "PART LOAD SIDE\n",
        "PART LOAD ADVANCE zap\n",
        "PART UNLOAD ADVANCE 1\n",
    ] {
        let e = cnc::parse(bad).expect_err("a confused PART line must refuse, never half-read");
        assert!(format!("{e}").contains("line"), "error must name its line: {e}");
    }
}

#[test]
fn part_events_voice_the_lifecycle_defaults() {
    // A bare script (prelude only) still says what the program asks
    // the machine to do - the defaults speak in comments.
    let text = cnc::serialize(&[
        Record::Section { begin: true },
        Record::Part { loading: true, advance: 25.0 },
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 0.0,
            y: 0.0,
            z: -1.0,
            feed: Some(100.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: None,
        }),
        Record::Part { loading: false, advance: 0.0 },
        Record::Section { begin: false },
    ]);
    let out = engine::run("", &json!(null), &text).expect("the prelude alone posts");
    assert!(out.program.contains("(PART LOAD ADVANCE 25)"), "{}", out.program);
    assert!(out.program.contains("(PART UNLOAD)"), "{}", out.program);
    // The act alone, no measured number, says just the act.
    let out = engine::run("", &json!(null), "PART LOAD\n").expect("posts");
    assert!(out.program.contains("(PART LOAD)"), "{}", out.program);
    assert!(!out.program.contains("ADVANCE"), "{}", out.program);
}

#[test]
fn part_events_are_lifecycle_hooks_not_commands() {
    // onCommand never sees a PART record - the contract SECTION
    // already keeps (engine.rs): the dedicated hooks do the voice,
    // and the properties carry the command name and the advance
    // only where an advance means something.
    let cps = r#"
var SEEN = [];
function onCommand() { SEEN.push("cmd:" + properties.command); }
function onPartLoading() { SEEN.push("load:" + properties.command + ":" + formatUnit(properties.Advance)); }
function onPartUnloading() { SEEN.push("unload:" + (properties.Advance === undefined ? "silent" : "leak")); }
function onClose() { SEEN.forEach(function (x) { writeln(x); }); }
"#;
    let text = cnc::serialize(&[
        Record::Part { loading: true, advance: 25.0 },
        Record::Part { loading: false, advance: 0.0 },
    ]);
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert!(out.program.contains("load:PartLoading:25"), "{}", out.program);
    assert!(out.program.contains("unload:silent"), "{}", out.program);
    assert!(!out.program.contains("cmd:"), "onCommand must never see it: {}", out.program);
}

#[test]
fn dump_brackets_the_section_with_the_feeder() {
    use camcraft_post::geometry::Point3;
    let t = Toolpath::new(vec![camcraft_post::cl::Move::new(
        MoveKind::Feed,
        Point3 { x: 0.0, y: 0.0, z: 1.0 },
        Point3 { x: 0.0, y: 0.0, z: -1.0 },
        Some(100.0),
    )]);
    let d = Dump {
        op_name: "Part1",
        part_load: Some(12.0),
        part_unload: true,
        tool_number: 1,
        tool_name: "GT-0300",
        tool_diameter: 16.0,
        rpm: 2000.0,
        clearance_z: 20.0,
        retract_z: 20.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec![],
        part_transfer: false,
        probe_tol: None,
        coolant: Default::default(),
        after: Default::default(),
        probe_result: None,
        ..Default::default()
    };
    let recs = camcraft_post::post::dump(&t, &d);
    let open = recs.iter().position(|r| matches!(r, Record::Section { begin: true })).expect("open");
    let close = recs.iter().rposition(|r| matches!(r, Record::Section { begin: false })).expect("close");
    assert_eq!(recs[open + 1], Record::Part { loading: true, advance: 12.0 }, "load leads the section");
    assert_eq!(recs[close - 1], Record::Part { loading: false, advance: 0.0 }, "unload trails it");
    // Silent flags write no events at all - the default document
    // posts byte-identically to before the feeder existed.
    let d2 = Dump { op_name: "Part1", part_load: None, part_unload: false, ..d };
    assert!(!camcraft_post::post::dump(&t, &d2).iter().any(|r| matches!(r, Record::Part { .. })));
}

// M5.3 stage C: the handoff joins the same family - `PART TRANSFER
// from to` on the stream, one dedicated hook, one flag on the dump -
// and the choreography puts it after the cut, before the unload.

#[test]
fn part_transfer_roundtrips_and_strays_refuse() {
    for recs in [
        vec![Record::Transfer { from: 1, to: 2 }],
        vec![Record::Transfer { from: 2, to: 1 }],
        vec![Record::Transfer { from: 3, to: 17 }],
    ] {
        let text = cnc::serialize(&recs);
        assert_eq!(cnc::parse(&text).expect("own output must parse"), recs);
    }
    for bad in [
        "PART TRANSFER\n",
        "PART TRANSFER 1\n",
        "PART TRANSFER 0 2\n",
        "PART TRANSFER 1 x\n",
        "PART TRANSFER -1 2\n",
    ] {
        let e = cnc::parse(bad).expect_err("a station the stream cannot name must refuse");
        let msg = format!("{e}");
        assert!(msg.contains("line"), "error must name its line: {msg}");
        assert!(
            msg.contains("station") || msg.contains("FROM and TO") || msg.contains("TO station"),
            "the refusal says which operand went wrong: {msg}"
        );
    }
    // The old unknown-word refusal still speaks, now that TRANSFER
    // is one of the words PART accepts.
    let e = cnc::parse("PART FIZZ\n").expect_err("still not a direction");
    assert!(format!("{e}").contains("TRANSFER"), "the door lists its words: {e}");
}

#[test]
fn part_transfer_voices_the_default_and_the_override() {
    // Prelude alone: the comment names both stations, the "->" the
    // feeder family's comments already use.
    let out = engine::run("", &json!(null), "PART TRANSFER 1 2\n").expect("posts");
    assert!(out.program.contains("(PART TRANSFER 1->2)"), "{}", out.program);
    // The hook is the whole contract - onCommand never sees a
    // lifecycle record, and the stations ride as From/To, never a
    // number the engine invented.
    let cps = r#"
var SEEN = [];
function onCommand() { SEEN.push("cmd:" + properties.command); }
function onPartTransfer() {
    SEEN.push("hand:" + properties.command + ":" + properties.From + properties.To);
}
function onClose() { SEEN.forEach(function (x) { writeln(x); }); }
"#;
    let out = engine::run(cps, &json!(null), "PART TRANSFER 2 5\n").expect("post");
    assert!(out.program.contains("hand:PartTransfer:25"), "{}", out.program);
    assert!(!out.program.contains("cmd:"), "onCommand must never see it: {}", out.program);
}

#[test]
fn dump_places_the_handoff_after_the_cut_before_the_unload() {
    use camcraft_post::geometry::Point3;
    let t = Toolpath::new(vec![camcraft_post::cl::Move::new(
        MoveKind::Feed,
        Point3 { x: 0.0, y: 0.0, z: 1.0 },
        Point3 { x: 0.0, y: 0.0, z: -1.0 },
        Some(100.0),
    )]);
    let d = Dump {
        op_name: "Part1",
        part_load: Some(0.0),
        part_unload: true,
        tool_number: 1,
        tool_name: "GT-0300",
        tool_diameter: 16.0,
        rpm: 2000.0,
        clearance_z: 20.0,
        retract_z: 20.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec![],
        part_transfer: true,
        probe_tol: None,
        coolant: Default::default(),
        after: Default::default(),
        probe_result: None,
        ..Default::default()
    };
    let recs = camcraft_post::post::dump(&t, &d);
    let at = |pat: &dyn Fn(&Record) -> bool| {
        recs.iter().position(pat).expect("record present")
    };
    let load = at(&|r| matches!(r, Record::Part { loading: true, .. }));
    let move_ = at(&|r| matches!(r, Record::Move(_)));
    let hand = at(&|r| matches!(r, Record::Transfer { .. }));
    let unload = at(&|r| matches!(r, Record::Part { loading: false, .. }));
    let close = recs.iter().rposition(|r| matches!(r, Record::Section { begin: false })).expect("close");
    assert!(load < move_ && move_ < hand && hand < unload && unload < close, "LOAD..cut..TRANSFER..UNLOAD: {recs:?}");
    assert_eq!(recs[hand], Record::Transfer { from: 1, to: 2 }, "the app's one handoff convention");
    // The flag off writes no handoff at all - the byte-identity the
    // goldens keep watching.
    let d2 = Dump { part_transfer: false, ..d };
    assert!(!camcraft_post::post::dump(&t, &d2).iter().any(|r| matches!(r, Record::Transfer { .. })));
}

// M5.3 stage D: the measurement ask joins the lifecycle family -
// `PROBE <µm>` on the stream (a length, so the lattice law, unlike
// the transfer's ordinals), one dedicated hook, and the choreography
// seats it after the last cut and before any handoff: measure the
// part where it was machined, before it changes hands.

#[test]
fn probe_roundtrips_and_strays_refuse() {
    for recs in [
        vec![Record::Probe { tol_mm: 0.05 }],
        vec![Record::Probe { tol_mm: 0.0 }],
        vec![Record::Probe { tol_mm: 12.345 }],
    ] {
        let text = cnc::serialize(&recs);
        assert_eq!(cnc::parse(&text).expect("own output must parse"), recs);
    }
    assert!(cnc::serialize(&[Record::Probe { tol_mm: 0.05 }]).contains("PROBE 50\n"));
    for bad in ["PROBE\n", "PROBE x\n", "PROBE -1\n"] {
        let e = cnc::parse(bad).expect_err("a budget the stream cannot state must refuse");
        let msg = format!("{e}");
        assert!(msg.contains("line"), "error must name its line: {msg}");
        assert!(msg.contains("PROBE"), "the refusal names the word: {msg}");
    }
    // The unknown-word door lists its words as before - PROBE stands
    // beside them, not inside PART.
    let e = cnc::parse("PROBED 50\n").expect_err("not a word");
    assert!(format!("{e}").contains("PROBED"), "the refusal echoes the stray: {e}");
}

#[test]
fn probe_voices_the_default_and_the_override() {
    // Prelude alone: the comment states the budget - which protocol
    // measures to it is the script's dialect, and the default
    // invents none.
    let out = engine::run("", &json!(null), "PROBE 50\n").expect("posts");
    assert!(out.program.contains("(PROBE TOL 0.05)"), "{}", out.program);
    // The hook is the whole contract: lifecycle, so onCommand never
    // sees it, and the budget rides as a property, never invented M.
    let cps = r#"
var SEEN = [];
function onCommand() { SEEN.push("cmd:" + properties.command); }
function onProbe() {
    SEEN.push("ask:" + properties.command + ":" + properties.Tolerance);
}
function onClose() { SEEN.forEach(function (x) { writeln(x); }); }
"#;
    let out = engine::run(cps, &json!(null), "PROBE 250\n").expect("post");
    assert!(out.program.contains("ask:Probe:0.25"), "{}", out.program);
    assert!(!out.program.contains("cmd:"), "onCommand must never see it: {}", out.program);
}

#[test]
fn dump_places_the_probe_after_the_cut_before_the_handoff() {
    use camcraft_post::geometry::Point3;
    let t = Toolpath::new(vec![camcraft_post::cl::Move::new(
        MoveKind::Feed,
        Point3 { x: 0.0, y: 0.0, z: 1.0 },
        Point3 { x: 0.0, y: 0.0, z: -1.0 },
        Some(100.0),
    )]);
    let d = Dump {
        op_name: "Part1",
        part_load: Some(0.0),
        part_unload: true,
        tool_number: 1,
        tool_name: "GT-0300",
        tool_diameter: 16.0,
        rpm: 2000.0,
        clearance_z: 20.0,
        retract_z: 20.0,
        top_z: 0.0,
        wcs: 1,
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec![],
        part_transfer: true,
        probe_tol: Some(0.05),
        coolant: Default::default(),
        after: Default::default(),
        probe_result: None,
        ..Default::default()
    };
    let recs = camcraft_post::post::dump(&t, &d);
    let at = |pat: &dyn Fn(&Record) -> bool| recs.iter().position(pat).expect("record present");
    let load = at(&|r| matches!(r, Record::Part { loading: true, .. }));
    let move_ = at(&|r| matches!(r, Record::Move(_)));
    let ask = at(&|r| matches!(r, Record::Probe { .. }));
    let hand = at(&|r| matches!(r, Record::Transfer { .. }));
    let unload = at(&|r| matches!(r, Record::Part { loading: false, .. }));
    assert!(
        load < move_ && move_ < ask && ask < hand && hand < unload,
        "LOAD..cut..PROBE..TRANSFER..UNLOAD: {recs:?}"
    );
    assert_eq!(recs[ask], Record::Probe { tol_mm: 0.05 }, "the authored budget, verbatim");
    // No ask writes no event - the byte-identity the goldens watch.
    let d2 = Dump { probe_tol: None, ..d };
    assert!(!camcraft_post::post::dump(&t, &d2).iter().any(|r| matches!(r, Record::Probe { .. })));
}

#[test]
fn dump_states_coolant_only_off_the_flood_default() {
    // #112: flood is what a section states by saying nothing (every
    // golden and legacy stream), any other ask rides one PARAM ahead
    // of the section, and it round-trips the text form.
    use camcraft_post::geometry::Point3;
    use camcraft_post::post::CoolantMode;
    let t = Toolpath::new(vec![camcraft_post::cl::Move::new(
        MoveKind::Feed,
        Point3 { x: 0.0, y: 0.0, z: 1.0 },
        Point3 { x: 0.0, y: 0.0, z: -1.0 },
        Some(100.0),
    )]);
    let coolant_params = |c: CoolantMode| -> Vec<serde_json::Value> {
        let d = Dump { op_name: "Op1", coolant: c, ..Default::default() };
        camcraft_post::post::dump(&t, &d)
            .into_iter()
            .filter_map(|r| match r {
                Record::Param { name, value } if name == "Coolant" => Some(value),
                _ => None,
            })
            .collect()
    };
    assert!(coolant_params(CoolantMode::Flood).is_empty(), "flood writes no word");
    assert_eq!(coolant_params(CoolantMode::Off), vec![json!("off")]);
    assert_eq!(coolant_params(CoolantMode::Mist), vec![json!("mist")]);
    assert_eq!(coolant_params(CoolantMode::ThroughTool), vec![json!("through-tool")]);
    let d = Dump { op_name: "Op1", coolant: CoolantMode::Mist, ..Default::default() };
    let records = camcraft_post::post::dump(&t, &d);
    let section = records.iter().position(|r| matches!(r, Record::Section { begin: true }));
    let param = records
        .iter()
        .position(|r| matches!(r, Record::Param { name, .. } if name == "Coolant"));
    assert!(param < section, "the ask stands ahead of its section");
    let back = cnc::parse(&cnc::serialize(&records)).expect("own output parses");
    assert_eq!(back, records, "the word round-trips the text form");
}

#[test]
fn dump_states_the_after_action_only_off_the_none_default() {
    // #110: carrying on is silent (every golden and legacy stream); a
    // stop or a trip home rides one PARAM ahead of the section, and it
    // round-trips the text form.
    use camcraft_post::geometry::Point3;
    use camcraft_post::post::AfterOperation;
    let t = Toolpath::new(vec![camcraft_post::cl::Move::new(
        MoveKind::Feed,
        Point3 { x: 0.0, y: 0.0, z: 1.0 },
        Point3 { x: 0.0, y: 0.0, z: -1.0 },
        Some(100.0),
    )]);
    let words = |a: AfterOperation| -> Vec<serde_json::Value> {
        let d = Dump { op_name: "Op1", after: a, ..Default::default() };
        camcraft_post::post::dump(&t, &d)
            .into_iter()
            .filter_map(|r| match r {
                Record::Param { name, value } if name == "AfterOperation" => Some(value),
                _ => None,
            })
            .collect()
    };
    assert!(words(AfterOperation::None).is_empty(), "carrying on writes no word");
    assert_eq!(words(AfterOperation::OptionalStop), vec![json!("optional-stop")]);
    assert_eq!(words(AfterOperation::ProgramStop), vec![json!("stop")]);
    assert_eq!(words(AfterOperation::GoHome), vec![json!("home")]);
    let d = Dump { op_name: "Op1", after: AfterOperation::GoHome, ..Default::default() };
    let records = camcraft_post::post::dump(&t, &d);
    let section = records.iter().position(|r| matches!(r, Record::Section { begin: true }));
    let param = records
        .iter()
        .position(|r| matches!(r, Record::Param { name, .. } if name == "AfterOperation"));
    assert!(param < section, "the ask stands ahead of its section");
    let back = cnc::parse(&cnc::serialize(&records)).expect("own output parses");
    assert_eq!(back, records, "the word round-trips the text form");
}

#[test]
fn a_stop_asked_of_a_script_without_after_modes_is_refused_by_name() {
    // #110 review B2: a script that never reads PARAM AfterOperation
    // would run straight on past the operator's stop - the engine
    // refuses the ask by operation name; `afterModes = true` lets the
    // script's own words through.
    let stream = cnc::serialize(&[
        Record::Param { name: "OperationName".into(), value: json!("Adaptive2") },
        Record::Param { name: "AfterOperation".into(), value: json!("stop") },
        Record::Section { begin: true },
        Record::Section { begin: false },
    ]);
    let body = r#"
var ask = "";
function onParameter(name, value) { if (name === "AfterOperation") ask = String(value); }
function onSectionEnd() { if (ask === "stop") writeln("M0"); ask = ""; }
"#;
    let deaf = engine::run(body, &serde_json::Value::Null, &stream).expect("the script runs");
    assert_eq!(deaf.warnings.len(), 1, "{:?}", deaf.warnings);
    let w = &deaf.warnings[0];
    assert!(w.starts_with("operation Adaptive2: asks a program stop"), "{w}");
    assert!(w.contains("afterModes"), "{w}");

    let hearing = format!("afterModes = true;\n{body}");
    let out = engine::run(&hearing, &serde_json::Value::Null, &stream).expect("the script runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l == "M0"), "{}", out.program);
}

//! Posting for a machine with an inverted axis (#374): conformance
//! goldens for an inverted-X Fanuc and Haas program (lines, G2/G3 in
//! G17/G18/G19, I/J/K, G41/G42, a drilled hole, a work-offset origin);
//! every refusal by name; and a standard machine untouched. Also the
//! staleness guard of `recorded/inputs/inverted_probe.cnc`, the fixture
//! stream app's `inverted_axis_display` reads (#926: app no longer carries
//! a copy of `inverted_support`).
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test inverted_post`
//!
//! Gate map (M4.4): covers post::invert, post::engine, post::machine, machine::direction, assets/posts.

mod inverted_support;
mod recorded;

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::machine::{AxisDirection, AxisLimits, Machine};
use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::{engine, invert, probe_rec};
use inverted_support::{arc, cps, machine, mv, probe_records, probe_stream, read, repo};
use serde_json::json;

#[test]
fn inverted_x_goldens_for_fanuc_and_haas() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for stem in ["fanuc", "haas"] {
        let out =
            engine::run(&cps(stem), &machine(&["X"]), &probe_stream()).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let rel = format!("crates/app/tests/golden/nc/inverted/{stem}_x.nc");
        if write {
            let path = repo(&rel);
            std::fs::create_dir_all(path.parent().expect("dir")).expect("golden dir");
            std::fs::write(&path, &out.program).expect("write golden");
            continue;
        }
        assert_eq!(out.program, read(&rel), "{rel} is stale; regenerate with WRITE_GOLDEN=1");
    }
}

/// The fixture stream, as app's `inverted_axis_display` reads it.
#[test]
fn the_recorded_inverted_probe_stream_is_current() {
    let path = recorded::here().join("inputs/inverted_probe.cnc");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, probe_stream()).expect("write the recorded stream");
        return;
    }
    assert_eq!(
        recorded::input("inverted_probe.cnc"),
        probe_stream(),
        "{} is stale; regenerate with WRITE_GOLDEN=1",
        path.display()
    );
}

#[test]
fn the_inverted_x_golden_carries_the_translated_words() {
    let out = engine::run(&cps("fanuc"), &machine(&["X"]), &probe_stream()).expect("posts");
    let std = engine::run(&cps("fanuc"), &machine(&[]), &probe_stream()).expect("posts");
    let (s, p) = (&std.program, &out.program);
    let both = |a: &str, b: &str| {
        assert!(s.contains(a), "standard lacks {a}:\n{s}");
        assert!(p.contains(b), "inverted lacks {b}:\n{p}");
    };
    // X 50 - origin 10 = 40 in the program; the inverted controller reads -40.
    both("G01 X40. Y10. Z-1. F600", "G01 X-40. Y10. Z-1. F600");
    // G17: the CCW quarter becomes a G02; I stays 0 (it is an X increment).
    both("G03 X50. Y20. Z-1. I0. J10.", "G02 X-50. Y20. Z-1. I0. J10.");
    // G18 holds X: the sense reverses and I flips sign.
    both("G18 G03 X25. Y30. Z4. I-5. K0.", "G18 G02 X-25. Y30. Z4. I5. K0.");
    // G19 holds no X: its sense and J/K are unchanged.
    both("G19 G02 X25. Y25. Z-1. J-5. K0.", "G19 G02 X-25. Y25. Z-1. J-5. K0.");
    // Left compensation is right in the mirrored controller frame.
    both("G17 G03 X25. Y15. Z-1. I0. J-5.", "G17 G02 X-25. Y15. Z-1. I0. J-5.");
    both("G01 G41 X25. Y10.", "G01 G42 X-25. Y10.");
    // The hole.
    both("G81 X60. Y0.", "G81 X-60. Y0.");
}

#[test]
fn a_standard_machine_posts_byte_identical() {
    for stem in ["fanuc", "haas"] {
        let a = engine::run(&cps(stem), &machine(&[]), &probe_stream()).expect("posts");
        let mut no_key = machine(&[]);
        if let Some(o) = no_key.as_object_mut() {
            o.insert("invertedAxes".into(), json!([]));
        }
        let b = engine::run(&cps(stem), &no_key, &probe_stream()).expect("posts");
        assert_eq!(a.program, b.program, "{stem}: an empty inverted list is a standard machine");
    }
    let records = probe_records();
    assert_eq!(invert::translate(records.clone(), invert::Flip::default()), Ok(records));
}

#[test]
fn the_mirror_states_the_inverted_axes_only_when_there_are_any() {
    let axis = |n: &str, d| AxisLimits { name: n.into(), direction: d, ..Default::default() };
    let mut m = Machine {
        model: "Flip 1".into(),
        axes: vec![
            axis("X", AxisDirection::Standard),
            axis("Y", AxisDirection::Standard),
            axis("Z", AxisDirection::Standard),
        ],
        ..Default::default()
    };
    assert!(camcraft_post::post::machine::mirror(&m).get("invertedAxes").is_none());
    m.axes[1].direction = AxisDirection::Inverted;
    assert_eq!(camcraft_post::post::machine::mirror(&m)["invertedAxes"], json!(["Y"]));
}

#[test]
fn the_stock_box_and_origin_mirror() {
    let f = invert::Flip { x: true, y: false, z: false };
    let out = invert::translate(probe_records(), f).expect("translates");
    let get = |k: &str| {
        out.iter()
            .find_map(|r| match r {
                Record::Param { name, value } if name == k => value.as_f64(),
                _ => None,
            })
            .unwrap_or(f64::NAN)
    };
    assert_eq!((get("StockXYLeft"), get("StockXYRight")), (-80.0, 0.0), "the box's edges swap as they flip");
    assert_eq!((get("StockXYFront"), get("StockXYBack")), (0.0, 60.0), "a standard Y is untouched");
    assert_eq!((get("OriginX"), get("OriginY")), (-10.0, 20.0), "the work-offset origin follows the sign rule");
}

// ---------------------------------------------------------------- refusals

fn refused(stem: &str, letters: &[&str], records: &[Record]) -> String {
    match engine::run(&cps(stem), &machine(letters), &cnc::serialize(records)) {
        Ok(out) => panic!("must refuse, posted:\n{}", out.program),
        Err(e) => e.to_string(),
    }
}

fn with(extra: Vec<Record>) -> Vec<Record> {
    let mut r = vec![
        Record::Param { name: "OperationName".into(), value: json!("Op1") },
        Record::Section { begin: true },
        mv(MoveKind::Rapid, [10.0, 10.0, 5.0], None),
    ];
    r.extend(extra);
    r.push(Record::Section { begin: false });
    r
}

#[test]
fn a_helical_arc_refuses_by_name() {
    let e = refused("fanuc", &["X"], &with(vec![arc([20.0, 10.0, 2.0], (15.0, 10.0), true, ArcPlane::Xy)]));
    assert_eq!(
        e,
        "machine VMC 3-AXIS has an inverted X axis: operation Op1: a helical arc is not supported for an inverted axis yet"
    );
}

#[test]
fn a_tilted_move_refuses_by_name() {
    let mut m = mv(MoveKind::Feed, [20.0, 10.0, 5.0], Some(100.0));
    if let Record::Move(r) = &mut m {
        r.abc = Some([0.0, 30.0, 0.0]);
    }
    let e = refused("fanuc", &["Y"], &with(vec![m]));
    assert!(e.contains("inverted Y axis: operation Op1: a tilted (A/B/C rotary) move is not supported"), "{e}");
}

#[test]
fn live_tooling_refuses_by_name() {
    let e = refused("fanuc", &["X"], &with(vec![Record::Param { name: "LiveTool".into(), value: json!("radial") }]));
    assert!(e.contains("C-axis live tooling (polar / G12.1 work) is not supported for an inverted axis"), "{e}");
}

#[test]
fn a_threading_cycle_refuses_by_name() {
    let t = Record::Thread(Some(camcraft_post::post::cnc_thread::ThreadRec {
        start: [10.0, 0.0, 2.0],
        end: [8.0, 0.0, -10.0],
        major_diameter: 10.0,
        minor_diameter: 8.0,
        lead: 1.5,
        height: 0.9,
        first_depth: 0.3,
        min_depth: 0.05,
        finish_allowance: 0.02,
        infeed_angle: 0.0,
        spring_passes: 1,
        pull_out: 0.0,
        alternating: false,
        repeat_each_depth: false,
        hand: camcraft_post::cl::ThreadHand::Right,
    }));
    let e = refused("fanuc", &["X"], &with(vec![t, Record::Thread(None)]));
    assert!(e.contains("a threading cycle is not supported for an inverted axis"), "{e}");
}

#[test]
fn probing_refuses_by_name() {
    let t = Record::Touch(probe_rec::TouchRec {
        target: [10.0, 10.0, -5.0],
        expect: [10.0, 10.0, 0.0],
        feed: 100.0,
        measure: Some(0),
    });
    // #392 lifted it for Fanuc and Haas (probeAxisInversion = true,
    // pinned by inverted_probe); a script that translates the motion but
    // does not declare its probing verified still refuses it.
    let haas = cps("haas");
    let flag = haas.lines().find(|l| l.starts_with("probeAxisInversion = true;")).expect("haas declares it");
    let e = match engine::run(&haas.replacen(flag, "", 1), &machine(&["X"]), &cnc::serialize(&with(vec![t]))) {
        Ok(out) => panic!("must refuse, posted:\n{}", out.program),
        Err(e) => e.to_string(),
    };
    assert!(
        e.contains("operation Op1: probing with a post script that does not declare probeAxisInversion = true"),
        "{e}"
    );
}

#[test]
fn every_refusal_is_named_once_in_one_run() {
    let e = refused(
        "fanuc",
        &["X"],
        &with(vec![
            arc([20.0, 10.0, 2.0], (15.0, 10.0), true, ArcPlane::Xy),
            arc([20.0, 10.0, 4.0], (15.0, 10.0), true, ArcPlane::Xy),
            Record::Param { name: "LiveTool".into(), value: json!("radial") },
        ]),
    );
    assert_eq!(e.matches("helical arc").count(), 1, "{e}");
    assert!(e.contains("live tooling"), "{e}");
}

#[test]
fn an_inverted_z_or_other_letter_refuses_by_name() {
    for letters in [&["Z"][..], &["X", "Z"]] {
        let e = refused("fanuc", letters, &probe_records());
        assert!(e.contains("posting for an inverted Z is not supported yet - tool-length compensation (G43)"), "{e}");
    }
    let e = refused("fanuc", &["U"], &probe_records());
    assert!(e.contains("inverted U is not supported yet - the post translates an inverted X or Y only"), "{e}");
}

#[test]
fn every_unverified_dialect_refuses_by_name() {
    for stem in ["sinumerik", "heidenhain", "mazak", "tormach", "grblhal", "tinyg", "fanuc-lathe"] {
        let e = refused(stem, &["X"], &probe_records());
        assert_eq!(
            e,
            "machine VMC 3-AXIS has an inverted X axis, and this post script is not verified for an inverted \
             axis (it does not declare axisInversion = true) - post with a script that does (Fanuc, Haas)",
            "{stem}"
        );
    }
}

#[test]
fn an_unread_machine_word_refuses_by_name() {
    let mut m = machine(&[]);
    m["kinematics"]["static"]["axes"][0]["reversal"] = json!(true);
    let e = engine::run(&cps("fanuc"), &m, &probe_stream()).map(|o| o.program).expect_err("refused").to_string();
    assert!(e.contains("axis X states \"reversal\": true, which the post does not read"), "{e}");
    let mut m = machine(&[]);
    m["invertedAxes"] = json!("X");
    let e = engine::run(&cps("fanuc"), &m, &probe_stream()).map(|o| o.program).expect_err("refused").to_string();
    assert!(e.contains("invertedAxes must be a list of axis letters"), "{e}");
}

#[test]
fn the_raw_door_reads_a_files_direction() {
    let file = json!({"general": {"model": "Flip 1"}, "kinematics": {"default": {"parts": [
        {"type": "linear", "name": "X", "direction": -1}]}}});
    let out = engine::run(&cps("fanuc"), &file, &probe_stream()).expect("an inverted X posts");
    assert!(out.program.contains("G01 X-40. Y10. Z-1. F600"), "{}", out.program);
    let bad = json!({"general": {"model": "Flip 1"}, "kinematics": {"default": {"parts": [
        {"type": "linear", "name": "X", "direction": 2}]}}});
    let e = engine::run(&cps("fanuc"), &bad, &probe_stream()).map(|o| o.program).expect_err("garbled").to_string();
    assert!(e.contains("axis X direction must be +1 or -1"), "{e}");
}

//! The safety law of #1649: a section that states a flipped WCS axis
//! (`PARAM WcsFlip`), posted and then read by a controller whose work
//! offset runs that axis reversed, drives the tool along the CAM path -
//! lines, G2/G3 in all three planes, a helix, the drilled hole and the
//! physical side of cutter compensation. And every case the mirror is not
//! verified for refuses by name, probing included.
//!
//! Provenance: RS-274 / NIST RS274NGC word semantics through the reader in
//! `inverted_support`; fixture values are our own.
//!
//! Gate map (M4.4): covers camcraft_post::post::wcs_flip, post::invert, post::engine, assets/posts fanuc.cps + haas.cps.

mod inverted_support;
mod recorded;

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::{engine, wcs_flip};
use inverted_support::{
    arc, assert_same_path, cam_events, controller_events, cps, machine, probe_records, Event,
};
use serde_json::{json, Value};

const TOL: f64 = 0.002;

const CASES: &[(&str, [bool; 3])] =
    &[("X", [true, false, false]), ("Y", [false, true, false]), ("XY", [true, true, false])];

fn param(name: &str, value: Value) -> Record {
    Record::Param { name: name.into(), value }
}

/// The probe stream plus a two-half helix down from the first rapid, the
/// section stating `flip` (nothing when `None`), named `op`.
fn section(flip: Option<&str>, op: &str) -> Vec<Record> {
    section_with(flip, op, true)
}

/// [`section`], with or without the helix (the machine mirror of #374
/// refuses one; the WCS flip carries it).
fn section_with(flip: Option<&str>, op: &str, helix: bool) -> Vec<Record> {
    let mut out = Vec::new();
    for r in probe_records() {
        match r {
            Record::Param { name, .. } if name == "OperationName" => out.push(param("OperationName", json!(op))),
            Record::Param { ref name, .. } if name == "WorkOffset" => {
                out.push(r);
                if let Some(f) = flip {
                    out.push(param("WcsFlip", json!(f)));
                }
            }
            Record::Move(ref m) if m.kind == MoveKind::Plunge && (m.z + 1.0).abs() < 1e-9 && (m.x - 30.0).abs() < 1e-9 && helix => {
                out.push(arc([40.0, 30.0, 2.75], (35.0, 30.0), true, ArcPlane::Xy));
                out.push(arc([30.0, 30.0, 0.5], (35.0, 30.0), true, ArcPlane::Xy));
                out.push(r);
            }
            Record::Cycle(mut c) => {
                c.first_move += if helix { 2 } else { 0 };
                out.push(Record::Cycle(c));
            }
            other => out.push(other),
        }
    }
    out
}

fn post(stem: &str, letters: &[&str], records: &[Record]) -> String {
    let out = engine::run(&cps(stem), &machine(letters), &cnc::serialize(records))
        .unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    out.program
}

fn refused(stem: &str, records: &[Record]) -> String {
    match engine::run(&cps(stem), &machine(&[]), &cnc::serialize(records)) {
        Ok(out) => panic!("must refuse, posted:\n{}", out.program),
        Err(e) => e.to_string(),
    }
}

fn code_lines(program: &str) -> Vec<&str> {
    program.lines().map(str::trim).filter(|l| !l.starts_with('(') && !l.is_empty()).collect()
}

fn count(program: &str, word: &str) -> usize {
    let padded = format!("G0{}", word.trim_start_matches('G'));
    code_lines(program).iter().filter(|l| l.split_whitespace().any(|w| w == word || w == padded)).count()
}

#[test]
fn a_flipped_wcs_cuts_the_cam_path() {
    let want = cam_events(&section(None, "Probe"));
    assert!(want.iter().any(|e| matches!(e, Event::Engage(_))) && want.iter().any(|e| matches!(e, Event::Hole(_))));
    for stem in ["fanuc", "haas"] {
        let plain = post(stem, &[], &section(None, "Probe"));
        assert_same_path(&want, &controller_events(&plain, [false; 3]), TOL, &format!("{stem} no flip"));
        for (letters, flip) in CASES {
            let what = format!("{stem} WCS flip {letters}");
            let program = post(stem, &[], &section(Some(letters), "Probe"));
            assert_same_path(&want, &controller_events(&program, *flip), TOL, &what);
            // The converse: the reader is not a tautology.
            let got = controller_events(&plain, *flip);
            let same = std::panic::catch_unwind(|| assert_same_path(&want, &got, TOL, "plain"));
            assert!(same.is_err(), "{what}: the unmirrored program must not pass the flipped reader");
        }
    }
}

#[test]
fn one_flipped_axis_reverses_arcs_and_the_compensation_side_and_two_do_not() {
    for stem in ["fanuc", "haas"] {
        let plain = post(stem, &[], &section(None, "Probe"));
        let (g2, g3) = (count(&plain, "G2"), count(&plain, "G3"));
        assert!(g2 > 0 && g3 > 0 && g2 != g3, "{stem}: the fixture tells G2 from G3: {g2} / {g3}");
        assert_eq!((count(&plain, "G41"), count(&plain, "G42")), (1, 0), "{stem}:\n{plain}");
        for letters in ["X", "Y"] {
            let p = post(stem, &[], &section(Some(letters), "Probe"));
            assert_eq!((count(&p, "G41"), count(&p, "G42")), (0, 1), "{stem} {letters}: the side swaps:\n{p}");
            assert_ne!((count(&p, "G2"), count(&p, "G3")), (g2, g3), "{stem} {letters}: arcs reverse:\n{p}");
        }
        // Both flipped is a half turn about Z: the side holds; only the
        // vertical-plane arcs (one flipped axis each) reverse.
        let p = post(stem, &[], &section(Some("XY"), "Probe"));
        assert_eq!((count(&p, "G41"), count(&p, "G42")), (1, 0), "{stem} XY:\n{p}");
        assert_eq!(count(&p, "G2") + count(&p, "G3"), g2 + g3, "{stem} XY: no arc is lost");
    }
}

#[test]
fn the_program_names_the_flipped_axes_and_only_then() {
    for stem in ["fanuc", "haas"] {
        let plain = post(stem, &[], &section(None, "Probe"));
        assert!(!plain.contains("FLIPPED"), "{stem}:\n{plain}");
        let x = post(stem, &[], &section(Some("X"), "Probe"));
        assert!(x.contains("(WCS X FLIPPED - PROGRAM MIRRORED)"), "{stem}:\n{x}");
        let xy = post(stem, &[], &section(Some("XY"), "Probe"));
        assert!(xy.contains("(WCS X AND Y FLIPPED - PROGRAM MIRRORED)"), "{stem}:\n{xy}");
        // The first rapid: (30, 30) from the origin (10, 20), mirrored on X.
        assert!(code_lines(&x).iter().any(|l| l.contains("X-20.") && l.contains("Y10.")), "{stem}:\n{x}");
        assert!(code_lines(&xy).iter().any(|l| l.contains("X-20.") && l.contains("Y-10.")), "{stem}:\n{xy}");
    }
}

#[test]
fn a_flipped_axis_on_an_inverted_machine_axis_posts_the_standard_signs() {
    let strip = |p: &str| -> Vec<String> {
        p.lines().filter(|l| !l.contains("FLIPPED") && !l.contains("INVERTED")).map(str::to_owned).collect()
    };
    for stem in ["fanuc", "haas"] {
        let plain = post(stem, &[], &section_with(None, "Probe", false));
        let both = post(stem, &["X"], &section_with(Some("X"), "Probe", false));
        assert_eq!(code_lines(&strip(&plain).join("\n")), code_lines(&strip(&both).join("\n")), "{stem}");
    }
}

#[test]
fn a_stream_mixes_a_flipped_and_a_plain_section() {
    let want = cam_events(&section(None, "Probe"));
    for stem in ["fanuc", "haas"] {
        let mut records = section(Some("X"), "Mirrored");
        // A change of flip needs the section's own start point (the dump
        // always states one).
        records.extend([param("StartX", json!(30.0)), param("StartY", json!(30.0)), param("StartZ", json!(15.0))]);
        // Move indices run over the whole stream: the second cycle's too.
        let moves = records.iter().filter(|r| matches!(r, Record::Move(_))).count();
        records.extend(section(None, "Plain").into_iter().map(|r| match r {
            Record::Cycle(mut c) => {
                c.first_move += moves;
                Record::Cycle(c)
            }
            other => other,
        }));
        let program = post(stem, &[], &records);
        let cut = program.find("(OPERATION: Plain").unwrap_or_else(|| panic!("{stem}: the second section is named:\n{program}"));
        let (a, b) = program.split_at(cut);
        assert!(a.contains("FLIPPED") && !b.contains("FLIPPED"), "{stem}: the note is the section's:\n{program}");
        assert_same_path(&want, &controller_events(a, [true, false, false]), TOL, &format!("{stem} mirrored half"));
        assert_same_path(&want, &controller_events(b, [false; 3]), TOL, &format!("{stem} plain half"));
    }
}

#[test]
fn a_stream_without_the_parameter_is_untouched() {
    let records = section(None, "Probe");
    let s = camcraft_post::post::js::Script::load(&cps("mazak")).unwrap_or_else(|e| panic!("{e}"));
    let out = wcs_flip::apply(records.clone(), &s).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cnc::serialize(&out), cnc::serialize(&records));
}

// ---------------------------------------------------------------- refusals

#[test]
fn a_flipped_z_refuses_by_name() {
    for letters in ["Z", "XZ", "XYZ"] {
        let e = refused("fanuc", &section(Some(letters), "Probe"));
        assert!(e.contains("operation Probe: the WCS has its"), "{e}");
        assert!(e.contains("posting a flipped Z is not supported yet") && e.contains("clear Flip Z"), "{e}");
    }
}

#[test]
fn a_script_that_is_not_verified_for_a_mirror_refuses_by_name() {
    for stem in ["mazak", "doosan-dnm", "sinumerik", "heidenhain", "mach4", "grblhal", "tormach", "tinyg", "carvera"] {
        let e = refused(stem, &section(Some("X"), "Probe"));
        assert!(e.contains("the WCS has its X axis flipped"), "{stem}: {e}");
        assert!(e.contains("does not declare axisInversion = true"), "{stem}: {e}");
    }
}

#[test]
fn a_flipped_and_rotated_section_refuses_by_name() {
    let mut records = section(Some("Y"), "Probe");
    records.insert(3, param("WcsYaw", json!(30.0)));
    let e = refused("fanuc", &records);
    assert!(e.contains("its Y axis flipped and is also rotated 30.000 deg about Z"), "{e}");
}

#[test]
fn what_the_mirror_cannot_carry_refuses_with_the_flip_as_the_cause() {
    let mut records = section(Some("X"), "Probe");
    records.insert(3, param("LiveTool", json!("radial")));
    let e = refused("fanuc", &records);
    assert!(e.contains("the WCS has its X axis flipped: "), "{e}");
    assert!(e.contains("is not supported for a flipped WCS axis yet"), "{e}");
    assert!(!e.contains("helical"), "a helix is carried: {e}");

    let mut turning = section(Some("X"), "Probe");
    turning.insert(3, param("StockTurnRadius", json!(20.0)));
    let e = refused("fanuc", &turning);
    assert!(e.contains("a turning section is not supported for a flipped WCS axis yet"), "{e}");
}

/// The recorded face probe (sets G55), stating `flip` after its offset.
fn probe_section(flip: Option<&str>) -> Vec<Record> {
    let mut out = Vec::new();
    for r in recorded::input_records("inverted_probe_face_zneg.cnc") {
        let offset = matches!(&r, Record::Param { name, .. } if name == "WorkOffset");
        out.push(r);
        if let (true, Some(f)) = (offset, flip) {
            out.push(param("WcsFlip", json!(f)));
        }
    }
    out
}

/// #1649, the maintainer's decision of 2026-11-18: probing in a flipped
/// WCS is refused by name, the flipped axes named - on a script that
/// declares `probeAxisInversion` too.
#[test]
fn probing_in_a_flipped_wcs_refuses_by_name() {
    for stem in ["fanuc", "haas"] {
        for (letters, named) in [("X", "its X axis flipped"), ("Y", "its Y axis flipped"), ("XY", "its X and Y axes flipped")] {
            let e = refused(stem, &probe_section(Some(letters)));
            assert!(e.contains(&format!("operation Probe1: the WCS has {named}; probing does not post")), "{stem} {letters}: {e}");
            assert!(e.contains("work-offset write is not verified"), "{stem} {letters}: {e}");
        }
    }
}

/// The same probe with no flip posts as it did before #1649: the pass
/// leaves the stream untouched and the G10 L2 write stays.
#[test]
fn probing_without_a_flip_posts_unchanged() {
    let records = probe_section(None);
    for stem in ["fanuc", "haas"] {
        let s = camcraft_post::post::js::Script::load(&cps(stem)).unwrap_or_else(|e| panic!("{e}"));
        let out = wcs_flip::apply(records.clone(), &s).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(cnc::serialize(&out), cnc::serialize(&records), "{stem}");
        let program = post(stem, &[], &records);
        assert!(program.contains("G10 L2 P2"), "{stem}: the probe sets G55:
{program}");
        assert!(!program.contains("FLIPPED"), "{stem}:
{program}");
    }
}

#[test]
fn a_value_the_pass_cannot_read_refuses() {
    for bad in [json!("Q"), json!("XX"), json!("x"), json!(1), json!(true)] {
        let mut records = section(None, "Probe");
        records.insert(3, param("WcsFlip", bad.clone()));
        let e = refused("fanuc", &records);
        assert!(e.contains("stream parameter WcsFlip must be axis letters"), "{bad}: {e}");
    }
    assert!(wcs_flip::parse(&json!("")).is_err(), "no letter is not a flip");
    assert_eq!(wcs_flip::letters([false; 3]), None);
    assert_eq!(wcs_flip::letters([true, false, true]).as_deref(), Some("XZ"));
    assert_eq!(wcs_flip::parse(&json!("YX")).ok(), Some([true, true, false]));
}

#[test]
fn a_section_that_leans_on_a_modal_origin_across_a_flip_change_refuses() {
    let mut records = section(Some("X"), "Mirrored");
    records.extend(section(None, "Plain").into_iter().filter(|r| {
        !matches!(r, Record::Param { name, .. } if name == "OriginX")
    }));
    let e = refused("fanuc", &records);
    assert!(e.contains("operation Plain: the section does not state OriginX"), "{e}");
    assert!(e.contains("the WCS X flip changes"), "{e}");
    // The same stream with Y flipped instead: X never changes, so the
    // modal X origin holds and the stream posts.
    let mut ok = section(Some("Y"), "Mirrored");
    ok.extend(section(Some("Y"), "Again").into_iter().filter(|r| {
        !matches!(r, Record::Param { name, .. } if name == "OriginX" || name == "OriginY")
    }));
    post("fanuc", &[], &ok);
}

#[test]
fn the_dump_states_the_flip_only_when_an_axis_is_flipped() {
    use camcraft_post::cl::PathBuilder;
    use camcraft_post::geometry::Point3;
    use camcraft_post::post::{dump, Dump};
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 10.0));
    b.push(MoveKind::Rapid, Point3::new(0.0, 0.0, 5.0), None);
    b.push(MoveKind::Feed, Point3::new(5.0, 0.0, 0.0), Some(300.0));
    let t = b.finish();
    let d = Dump { op_name: "Op", tool_number: 1, tool_name: "T", tool_diameter: 6.0, rpm: 1000.0, ..Dump::default() };
    let plain = cnc::serialize(&dump(&t, &d));
    assert!(!plain.contains("WcsFlip"), "{plain}");
    let flipped = cnc::serialize(&dump(&t, &Dump { wcs_flip: [true, true, false], ..d }));
    assert!(flipped.contains("PARAM WcsFlip XY"), "{flipped}");
}

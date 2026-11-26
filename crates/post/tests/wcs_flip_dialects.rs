//! #2475: the safety law of #1649 and #374 for each mill dialect. A
//! section with a flipped WCS X or Y (`PARAM WcsFlip`), or a program for
//! a machine with an inverted X or Y, is posted by each mill script and
//! then read by a controller whose axis runs reversed. The trace is on
//! the CAM path: lines, G2/G3 in the three planes (native, split, as
//! chords or with R words), a helix where supported, the drilled hole (a canned cycle, a
//! Heidenhain G200 call or plain moves) and the physical compensation
//! side. The standard program on that controller is not on the path, so
//! the reader is not a tautology.
//!
//! Provenance: RS-274 / NIST RS274NGC word semantics through the reader
//! in `mirror_trace`; fixture values are our own (`inverted_support`).
//!
//! Regenerate the goldens after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test wcs_flip_dialects`
//!
//! Gate map (M4.4): covers camcraft_post::post::wcs_flip, post::invert, post::engine, assets/posts (each mill script).

mod inverted_support;
mod mirror_trace;
mod recorded;

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::engine;
use inverted_support::{arc, cps, machine, probe_records};
use mirror_trace::{assert_same_trace, cam_trace, controller_trace, Dialect, Trace};
use serde_json::{json, Value};

/// One mill script: its file stem, if it writes control compensation
/// (the others refuse G41/G42 by name), how its words differ, the
/// tolerance its arc form needs, and how it writes a comment.
struct Post {
    stem: &'static str,
    comp: bool,
    dialect: Dialect,
    tol: f64,
    note: fn(&str) -> String,
}

fn paren(s: &str) -> String {
    format!("({s})")
}

fn semicolon(s: &str) -> String {
    format!("; {s}")
}

const RS274: Dialect = Dialect { heidenhain: false };

/// Fanuc and Haas (verified by #374 and #1649) calibrate the reader; the
/// others are the scripts of #2475. Heidenhain writes vertical arcs as
/// 1 um chords and TinyG as 5 um chords, so their paths are longer to
/// compare point by point.
const POSTS: &[Post] = &[
    Post { stem: "fanuc", comp: true, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "haas", comp: true, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "mazak", comp: true, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "doosan-dnm", comp: true, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "sinumerik", comp: true, dialect: RS274, tol: 0.003, note: semicolon },
    Post { stem: "mach4", comp: true, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "heidenhain", comp: false, dialect: Dialect { heidenhain: true }, tol: 0.006, note: semicolon },
    Post { stem: "tormach", comp: false, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "linuxcnc", comp: false, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "grblhal", comp: false, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "mach3", comp: false, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "carvera", comp: false, dialect: RS274, tol: 0.003, note: paren },
    Post { stem: "tinyg", comp: false, dialect: RS274, tol: 0.03, note: paren },
];

/// The scripts that #2475 verified.
fn new_posts() -> impl Iterator<Item = &'static Post> {
    POSTS.iter().filter(|p| !matches!(p.stem, "fanuc" | "haas"))
}

const CASES: &[(&str, [bool; 3])] =
    &[("X", [true, false, false]), ("Y", [false, true, false]), ("XY", [true, true, false])];

fn param(name: &str, value: Value) -> Record {
    Record::Param { name: name.into(), value }
}

/// The probe stream, named `op`: `flip` stated after the work offset
/// (nothing when `None`), a two-half helix down from the first rapid
/// when `helix`, and the control compensation records only when `comp`.
fn section(flip: Option<&str>, op: &str, helix: bool, comp: bool) -> Vec<Record> {
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
            Record::Comp { .. } if !comp => {}
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

fn post_with(p: &Post, letters: &[&str], records: &[Record], options: &Value) -> String {
    let out = engine::run_with(&cps(p.stem), &machine(letters), &cnc::serialize(records), options)
        .unwrap_or_else(|e| panic!("{}: must post: {e}", p.stem));
    assert!(out.warnings.is_empty(), "{}: {:?}", p.stem, out.warnings);
    out.program
}

fn post(p: &Post, letters: &[&str], records: &[Record]) -> String {
    post_with(p, letters, records, &Value::Null)
}

/// The program is on the CAM path for a controller that reads `flip`
/// reversed, and `plain` (the standard program) is not.
fn assert_mirrored(p: &Post, want: &Trace, program: &str, plain: &str, flip: [bool; 3], what: &str) {
    assert_same_trace(want, &controller_trace(program, flip, p.dialect), p.tol, what);
    let (dialect, tol) = (p.dialect, p.tol);
    let stray = std::panic::catch_unwind(|| {
        assert_same_trace(want, &controller_trace(plain, flip, dialect), tol, "plain");
    });
    assert!(stray.is_err(), "{what}: the unmirrored program must not pass the reversed reader");
}

#[test]
fn a_flipped_wcs_cuts_the_cam_path_in_each_mill_dialect() {
    for p in POSTS {
        let cam = section(None, "Probe", !p.dialect.heidenhain, p.comp);
        let want = cam_trace(&cam);
        assert_eq!(want.cuts.len(), 2, "the fixture is the milled run and the hole");
        assert_eq!(want.engages.len(), usize::from(p.comp), "{}: the fixture engages compensation", p.stem);
        let plain = post(p, &[], &cam);
        assert_same_trace(&want, &controller_trace(&plain, [false; 3], p.dialect), p.tol, &format!("{} no flip", p.stem));
        for (letters, flip) in CASES {
            let what = format!("{} WCS flip {letters}", p.stem);
            let program = post(p, &[], &section(Some(letters), "Probe", !p.dialect.heidenhain, p.comp));
            assert_mirrored(p, &want, &program, &plain, *flip, &what);
        }
    }
}

#[test]
fn an_inverted_machine_axis_cuts_the_cam_path_in_each_mill_dialect() {
    for p in POSTS {
        // The machine mirror of #374 refuses a helix: the stream has none.
        let cam = section(None, "Probe", false, p.comp);
        let want = cam_trace(&cam);
        let plain = post(p, &[], &cam);
        for (letters, flip) in CASES {
            let axes: Vec<&str> = letters.split("").filter(|l| !l.is_empty()).collect();
            let what = format!("{} inverted machine {letters}", p.stem);
            let program = post(p, &axes, &cam);
            assert_mirrored(p, &want, &program, &plain, *flip, &what);
        }
    }
}

/// The R-word arc form (`useRadiusArcs`) is made from the mirrored start,
/// end and centre: the trace holds for it too.
#[test]
fn radius_word_arcs_follow_the_mirror() {
    let options = json!({ "useRadiusArcs": true });
    for p in POSTS.iter().filter(|p| cps(p.stem).contains("useRadiusArcs")) {
        let cam = section(None, "Probe", !p.dialect.heidenhain, p.comp);
        let want = cam_trace(&cam);
        let plain = post_with(p, &[], &cam, &options);
        assert!(plain.lines().any(|l| l.contains(" R") && (l.contains("G2") || l.contains("G02") || l.contains("G3") || l.contains("G03"))), "{}: the option writes R arcs:\n{plain}", p.stem);
        // An R word is rounded to three decimals: the centre moves more.
        let tol = p.tol.max(0.01);
        assert_same_trace(&want, &controller_trace(&plain, [false; 3], p.dialect), tol, &format!("{} R arcs", p.stem));
        for (letters, flip) in CASES {
            let what = format!("{} R arcs, WCS flip {letters}", p.stem);
            let program = post_with(p, &[], &section(Some(letters), "Probe", !p.dialect.heidenhain, p.comp), &options);
            assert_same_trace(&want, &controller_trace(&program, *flip, p.dialect), tol, &what);
        }
    }
}

#[test]
fn each_dialect_names_the_flipped_axes_and_the_inverted_axes() {
    for p in POSTS {
        let plain = post(p, &[], &section(None, "Probe", !p.dialect.heidenhain, p.comp));
        assert!(!plain.contains("FLIPPED") && !plain.contains("INVERTED"), "{}:\n{plain}", p.stem);
        let x = post(p, &[], &section(Some("X"), "Probe", !p.dialect.heidenhain, p.comp));
        assert_eq!(x.matches(&(p.note)("WCS X FLIPPED - PROGRAM MIRRORED")).count(), 1, "{}:\n{x}", p.stem);
        let xy = post(p, &[], &section(Some("XY"), "Probe", !p.dialect.heidenhain, p.comp));
        assert_eq!(xy.matches(&(p.note)("WCS X AND Y FLIPPED - PROGRAM MIRRORED")).count(), 1, "{}:\n{xy}", p.stem);
        let inv = post(p, &["X", "Y"], &section(None, "Probe", false, p.comp));
        for note in ["MACHINE X+ MOVES LEFT - INVERTED AXIS", "MACHINE Y+ MOVES TOWARD THE FRONT - INVERTED AXIS"] {
            assert_eq!(inv.matches(&(p.note)(note)).count(), 1, "{}:\n{inv}", p.stem);
        }
    }
}

/// The note is the section's own: a plain section after a flipped one
/// does not repeat it.
#[test]
fn a_plain_section_after_a_flipped_one_writes_no_flip_note() {
    for p in POSTS {
        let mut records = section(Some("X"), "Mirrored", !p.dialect.heidenhain, p.comp);
        // A change of flip needs the section's own start point (the dump
        // always states one).
        records.extend([param("StartX", json!(30.0)), param("StartY", json!(30.0)), param("StartZ", json!(15.0))]);
        // Move indices run over the whole stream: the second cycle's too.
        let moves = records.iter().filter(|r| matches!(r, Record::Move(_))).count();
        records.extend(section(None, "Plain", !p.dialect.heidenhain, p.comp).into_iter().map(|r| match r {
            Record::Cycle(mut c) => {
                c.first_move += moves;
                Record::Cycle(c)
            }
            other => other,
        }));
        let program = post(p, &[], &records);
        assert_eq!(program.matches("FLIPPED").count(), 1, "{}:\n{program}", p.stem);
        let (note, plain_op) = (program.find("FLIPPED"), program.find("OPERATION: Plain"));
        assert!(note < plain_op && plain_op.is_some(), "{}: the note is in the first section:\n{program}", p.stem);
    }
}

/// The scripts of #2475 do not declare `probeAxisInversion`: probing on
/// an inverted machine axis stays refused by name.
#[test]
fn probing_on_an_inverted_axis_stays_refused_in_the_new_dialects() {
    let stream = recorded::input("inverted_probe_face_zneg.cnc");
    for p in new_posts() {
        let e = match engine::run(&cps(p.stem), &machine(&["X"]), &stream) {
            Ok(out) => panic!("{}: must refuse, posted:\n{}", p.stem, out.program),
            Err(e) => e.to_string(),
        };
        assert!(e.contains("probing with a post script that does not declare probeAxisInversion = true"), "{}: {e}", p.stem);
    }
}

/// TNC 640 ISO manual 892909-25, pp. 271-272 requires a polar helix.
/// Refuse the unsupported native form, including each WCS mirror.
#[test]
fn heidenhain_refuses_cartesian_helices_by_operation_name() {
    for flip in [None, Some("X"), Some("Y"), Some("XY")] {
        let records = section(flip, "Helix", true, false);
        let out = engine::run(&cps("heidenhain"), &machine(&[]), &cnc::serialize(&records)).expect("the script runs");
        assert!(out.warnings.iter().any(|w| w.contains("operation Helix:") && w.contains("Heidenhain helix requires polar interpolation")), "{flip:?}: {:?}", out.warnings);
        assert!(!out.program.contains("Z+2.75") && !out.program.contains("Z+0.5"), "{flip:?}: the refused helix is not written:\n{}", out.program);
    }
}

/// A Cartesian G02/G03 cannot prove a TNC helix, with I/J or R words.
#[test]
fn heidenhain_reader_rejects_cartesian_helices() {
    let dialect = Dialect { heidenhain: true };
    for direction in ["G02", "G03"] {
        for centre in ["I0 J1\n", ""] {
            let radius = if centre.is_empty() { " R1" } else { "" };
            let program = format!("G00 X0 Y0 Z0\n{centre}{direction} X1 Y1 Z0{radius}\n");
            let planar = controller_trace(&program, [false; 3], dialect);
            assert_eq!(planar.cuts.len(), 1, "the planar arc remains supported");
            let helix = program.replace("X1 Y1 Z0", "X1 Y1 Z-1");
            assert!(std::panic::catch_unwind(|| controller_trace(&helix, [false; 3], dialect)).is_err(), "the reader accepted a Cartesian Heidenhain helix: {helix}");
            assert_eq!(controller_trace(&helix, [false; 3], RS274).cuts.len(), 1, "the dialect rule must leave RS-274 helices supported");
        }
    }
}

/// Conformance goldens: the whole X-flipped program of each new dialect.
#[test]
fn flipped_x_goldens_for_the_new_dialects() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for p in new_posts() {
        let program = post(p, &[], &section(Some("X"), "Probe", !p.dialect.heidenhain, p.comp));
        let path = recorded::here().join("../golden/wcs_flip").join(format!("{}_x.nc", p.stem));
        if write {
            std::fs::create_dir_all(path.parent().expect("dir")).expect("golden dir");
            std::fs::write(&path, &program).expect("write golden");
            continue;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n");
        assert_eq!(program, want, "{} is stale; regenerate with WRITE_GOLDEN=1", path.display());
    }
}

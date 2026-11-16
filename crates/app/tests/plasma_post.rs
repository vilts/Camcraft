//! #671: the PathPilot plasma post for the Tormach 1300PL - the
//! plate-with-holes golden (#670's `plasma_plate_with_holes.cl`) posted
//! in inches and in millimetres, the control-compensated cut (G41.1 /
//! G42.1 D<kerf>, G40), the per-contour block (G0, M200, M207/M209 as
//! stated, G15, G16, the XY cut, M205, G30), the stream round trip of
//! the new `PLASMA` / `TORCH` records, and the in-app post of a plasma
//! document through the gate.
//! Provenance: our own tests over our own fixtures; the words they pin
//! are cited in `assets/posts/tormach-plasma.cps` (UM10720 §6-7, the
//! PathPilot Post Processor Guidelines).
//!
//! Gate map (#671): covers post::cnc (PLASMA/TORCH records), post::
//! cnc_torch, post::engine (the torch law), post::dump (torch records),
//! post::machine (kind), app::posting (PLASMA_POSTS), the
//! tormach-plasma.cps script.

mod common;

use camcraft_app::cl::Toolpath;
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::plasma::{plasma_profile, PlasmaParams};
use camcraft_app::post::{self, cnc, engine, Dump};
use common::write_golden;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const PLASMA: &str = "Tormach PathPilot Plasma";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps() -> &'static str {
    camcraft_app::posting::BUILTINS.iter().find(|(n, _)| *n == PLASMA).map(|(_, s)| *s).expect("registered")
}

/// The shipped 1300PL file (inch), or its metric twin.
fn pl1300(mm: bool) -> Value {
    let text = read("assets/machines/tormach-1300pl.mch");
    let mut v: Value = serde_json::from_str(&text).expect("json");
    if mm {
        v["kinematics"]["default"]["units"]["length"] = Value::from("mm");
    }
    v
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn circle(c: Point2, r: f64, n: usize) -> Loop {
    Loop::new(
        (0..n)
            .map(|k| {
                let t = std::f64::consts::TAU * k as f64 / n as f64;
                Point2::new(c.x + r * t.cos(), c.y + r * t.sin())
            })
            .collect(),
    )
    .unwrap()
}

/// #670's golden plate (`golden_cl::plasma_plate_with_holes_matches_golden`),
/// cut with the control compensating or the computer offsetting.
pub fn plate(control: bool) -> Toolpath {
    let loops = vec![
        rect(0.0, 0.0, 120.0, 80.0),
        circle(Point2::new(35.0, 40.0), 20.0, 64),
        rect(29.0, 34.0, 41.0, 46.0),
        circle(Point2::new(80.0, 60.0), 5.0, 32),
        rect(70.0, 15.0, 100.0, 25.0),
    ];
    let part = Region::new(loops.clone()).unwrap();
    let p = PlasmaParams {
        kerf: 1.5,
        control_comp: control,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 1.5,
        thickness: 6.0,
        feed: Some(2500.0),
        clearance_z: 15.0,
        cut_z: 6.0,
        pierce_height: Some(3.8),
        cut_height: None,
        pierce_delay: Some(0.5),
        model_excess: 0.0,
    };
    plasma_profile(&loops, &[], &part, &p).unwrap()
}

/// The stream an in-app post hands the script for one plasma op.
pub fn stream_of(t: &Toolpath) -> String {
    let d = Dump {
        op_name: "Plasma1",
        tool_number: 1,
        tool_name: "PT-KERF-150",
        tool_diameter: 1.5,
        op_kind: "Plasma Profile",
        tool_kind: "plasma torch",
        ..Dump::default()
    };
    cnc::serialize(&post::dump(t, &d))
}

/// #902: the program's unit is the document's; a run on the inch file
/// stands for the inch document it posted before #902.
fn post_clean(mch: &Value, stream: &str) -> String {
    let inch = mch.pointer("/kinematics/default/units/length").and_then(Value::as_str) == Some("inch");
    let units = serde_json::json!({ "Units": if inch { "inch" } else { "mm" } });
    let out = engine::run_with(cps(), mch, stream, &units).expect("posts");
    assert!(out.warnings.is_empty(), "the gate counts warnings: {:?}", out.warnings);
    out.program
}

/// The laws every plasma program keeps, whatever the plate.
fn laws(nc: &str, contours: usize) {
    for l in nc.lines() {
        let words: Vec<&str> = l.split_whitespace().filter(|w| !w.starts_with('(')).collect();
        if l.starts_with('(') {
            continue;
        }
        for w in &words {
            assert!(!w.starts_with('Z') && !w.starts_with('S') && !w.starts_with('T'), "no Z, S or T word: {l}");
            assert!(!["M3", "M4", "M5", "M6", "M7", "M8", "M9", "G95", "G96"].contains(w), "not a plasma word: {l}");
        }
        assert!(!l.contains('%') && !l.contains(';'), "no fence, no ; - {l}");
    }
    for w in ["M200", "G15", "G16", "M205"] {
        assert_eq!(nc.lines().filter(|l| *l == w).count(), contours, "{w} once per contour");
    }
    // Each contour: G0 X Y, M200, [M207/M208/M209], G15, G16 ... M205, G30.
    let lines: Vec<&str> = nc.lines().collect();
    for (i, _) in lines.iter().enumerate().filter(|(_, l)| **l == "M200") {
        assert!(lines[i - 1].starts_with("G0 X") && !lines[i - 1].contains('Z'), "{}", lines[i - 1]);
        let g15 = lines[i..].iter().position(|l| *l == "G15").map(|k| k + i).expect("G15");
        assert!(lines[i + 1..g15].iter().all(|l| ["M207", "M208", "M209"].contains(&&l[..4])), "{:?}", &lines[i..g15]);
        assert_eq!(lines[g15 + 1], "G16");
        let off = lines[i..].iter().position(|l| *l == "M205").map(|k| k + i).expect("M205");
        assert_eq!(lines[off + 1], "G30", "G30 after every M205");
    }
    assert!(nc.contains("\nG43 H1\n") && nc.ends_with("G30\nM30\n"), "{nc}");
}

#[test]
fn the_plate_with_holes_posts_to_its_golden_in_inches_and_millimetres() {
    let stream = read("crates/app/tests/golden/plasma_plate_with_holes.cl");
    for (mm, name) in [(false, "plasma_plate_with_holes"), (true, "plasma_plate_with_holes_mm")] {
        let nc = post_clean(&pl1300(mm), &stream);
        laws(&nc, 5);
        assert!(nc.starts_with(if mm { "(Tormach 1300PL)\nG21\n" } else { "(Tormach 1300PL)\nG20\n" }), "{nc}");
        // G64 P is inches on both (the guidelines' rule).
        assert!(nc.contains("\nG64 P0.005 Q0.005\n"));
        // Stated pierce height and delay ride after M200; the cut height
        // is unstated - AutoFS's.
        let ph = if mm { "M207 P3.8" } else { "M207 P0.1496" };
        assert_eq!(nc.matches(ph).count(), 5, "{nc}");
        assert_eq!(nc.matches("M209 P0.5").count(), 5);
        assert!(!nc.contains("M208"));
        write_golden(&format!("nc/tormach-plasma/{name}.nc"), &nc);
    }
}

#[test]
fn the_in_app_stream_posts_what_the_cl_golden_posts() {
    // The dump's records carry the same torch as the `.cl` comments: the
    // only difference is the commentary the PARAMs add.
    let t = plate(false);
    let from_dump = post_clean(&pl1300(false), &stream_of(&t));
    let from_cl = post_clean(&pl1300(false), &read("crates/app/tests/golden/plasma_plate_with_holes.cl"));
    let strip = |s: &str| s.lines().filter(|l| !l.starts_with('(')).collect::<Vec<_>>().join("\n");
    assert_eq!(strip(&from_dump), strip(&from_cl));
    assert!(from_dump.contains("(OPERATION: Plasma1 - PLASMA PROFILE)"), "{from_dump}");
}

#[test]
fn a_control_compensated_plate_engages_g41_1_with_the_kerf_on_a_straight_lead() {
    let t = plate(true);
    let nc = post_clean(&pl1300(false), &stream_of(&t));
    laws(&nc, 5);
    let lines: Vec<&str> = nc.lines().collect();
    let engages: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| l.starts_with("G41.1") || l.starts_with("G42.1")).map(|(i, _)| i).collect();
    assert!(!engages.is_empty(), "{nc}");
    for i in engages {
        // D is the kerf (1.5 mm = 0.0591 in), after G16, then a straight G1.
        assert!(lines[i].ends_with(" D0.0591"), "{}", lines[i]);
        assert_eq!(lines[i - 1], "G16");
        assert!(lines[i + 1].starts_with("G1 "), "{}", lines[i + 1]);
        let cancel = lines[i..].iter().position(|l| *l == "G40").map(|k| k + i).expect("G40");
        assert!(lines[cancel + 1].starts_with("G1 ") && lines[cancel + 2] == "M205", "{:?}", &lines[cancel..cancel + 3]);
    }
    write_golden("nc/tormach-plasma/plasma_plate_with_holes_control.nc", &nc);
}

#[test]
fn the_plasma_records_round_trip_the_stream() {
    for control in [false, true] {
        let t = plate(control);
        let d = Dump { op_name: "Plasma1", ..Dump::default() };
        let recs = post::dump(&t, &d);
        let p = t.plasma.as_ref().expect("plasma");
        let heads = recs.iter().filter(|r| matches!(r, cnc::Record::Plasma(_))).count();
        let torches = recs.iter().filter(|r| matches!(r, cnc::Record::Torch(_))).count();
        assert_eq!((heads, torches), (1, 2 * p.torches.len()));
        // The header stands before the section; each ON before a cutting
        // record, each OFF after one.
        let h = recs.iter().position(|r| matches!(r, cnc::Record::Plasma(_))).expect("header");
        let s = recs.iter().position(|r| matches!(r, cnc::Record::Section { begin: true })).expect("section");
        assert!(h < s);
        let text = cnc::serialize(&recs);
        assert!(text.contains("\nPLASMA KERF1500 ") && text.contains("\nTORCH ON C0 INNER PARENT4 SMALL650\n"), "{text}");
        // The lattice law: the text is the record set, byte for byte.
        let back = cnc::parse(&text).expect("parses");
        assert_eq!(cnc::serialize(&back), text, "control {control}");
        let pick = |rs: &[cnc::Record]| -> Vec<cnc::Record> {
            rs.iter().filter(|r| matches!(r, cnc::Record::Plasma(_) | cnc::Record::Torch(_))).cloned().collect()
        };
        assert_eq!(pick(&back), pick(&recs));
    }
    // The `.cl` comment form reads as the same records the dump writes.
    let cl = cnc::parse(&read("crates/app/tests/golden/plasma_plate_with_holes.cl")).expect("parses");
    let dumped = post::dump(&plate(false), &Dump::default());
    let pick = |rs: &[cnc::Record]| -> Vec<cnc::Record> {
        rs.iter().filter(|r| matches!(r, cnc::Record::Plasma(_) | cnc::Record::Torch(_))).cloned().collect()
    };
    assert_eq!(pick(&cl), pick(&dumped));
    // Malformed records are refused at the reader, by line.
    for bad in ["PLASMA CONTROL", "PLASMA KERF0", "PLASMA KERF1500 PIERCE-1", "TORCH ON C0 SIDEWAYS", "TORCH OFF", "TORCH LIT C0", "TORCH ON C0 INNER SMALL2000"] {
        let e = cnc::parse(&format!("; HSMW-CNC v1\n{bad}\n")).err().unwrap_or_else(|| panic!("{bad} must refuse"));
        assert!(e.to_string().starts_with("line 2:"), "{bad}: {e}");
    }
}

#[test]
fn a_plasma_document_posts_through_the_app_gate() {
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost};
    use camcraft_app::geometry::{Mesh, Point3, Triangle};
    use camcraft_app::operation::{Height, OpParams, PlasmaComp, PlasmaOp, SrcRef};
    use camcraft_app::schema::PostRef;
    use camcraft_app::tools::torch_tool;

    // A 40x40x5 plate with a 10x10 hole (plasma_profile.rs's fixture).
    let ring = |h: f64, z: f64| [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)];
    let quad = |a, b, c, d| [Triangle::new(a, b, c), Triangle::new(a, c, d)];
    let mut f = Vec::new();
    for (z, up) in [(5.0, true), (0.0, false)] {
        let (o, i) = (ring(20.0, z), ring(5.0, z));
        for k in 0..4 {
            let (o0, o1, i0, i1) = (o[k], o[(k + 1) % 4], i[k], i[(k + 1) % 4]);
            f.extend(if up { quad(o0, o1, i1, i0) } else { quad(o0, i0, i1, o1) });
        }
    }
    for (r, out) in [(20.0, true), (5.0, false)] {
        let (lo, hi) = (ring(r, 0.0), ring(r, 5.0));
        for k in 0..4 {
            let (a, b, c, d) = (lo[k], lo[(k + 1) % 4], hi[(k + 1) % 4], hi[k]);
            f.extend(if out { quad(a, b, c, d) } else { quad(a, d, c, b) });
        }
    }
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    h.execute(&world, AddTool::new(&world, "PT-KERF-150", torch_tool(1.5)).expect("torch"));
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(f)));
    let op = PlasmaOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        kerf: None,
        comp: PlasmaComp::Computer,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 1.0,
        feed: None,
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
    };
    let src = SrcRef::Entity("plate".into());
    h.execute(&world, CreateOperation::new(&world, "Plasma1", "PT-KERF-150", src, OpParams::PlasmaProfile(op)).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Plasma1").expect("generates"));
    // #1232: the default (Fanuc, a milling post) does not serve the
    // table - refused by name before a line is written.
    let file = std::env::temp_dir().join(format!("camcraft_plasma_unserved_{}.nc", std::process::id()));
    let why = camcraft_app::posting::post_setup(&world, &file).expect_err("a milling post on a plasma table");
    assert!(why.contains("Plasma1") && why.contains("post Fanuc does not serve") && why.contains("plasma cutting"), "{why}");
    assert!(!file.exists(), "nothing written");
    // The mount offers the plasma post; the user takes it.
    let (serves, offer) = camcraft_app::posting::post_offer(&world).expect("a mill post on a plasma table is offered the plasma post");
    assert_eq!((serves, &offer), ("mill", &PostRef::Named(PLASMA.into())));
    h.execute(&world, SelectPost::new(&world, offer).expect("select"));
    assert!(camcraft_app::posting::post_offer(&world).is_none());
    let file = std::env::temp_dir().join(format!("camcraft_plasma_post_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &file).expect("the gate passes a plasma program");
    let _ = std::fs::remove_file(&file);
    let nc = posted.program;
    laws(&nc, 2);
    // AutoFS owns the feed: no F word at all, and the small hole says so.
    assert!(!nc.lines().filter(|l| !l.starts_with('(')).flat_map(str::split_whitespace).any(|w| w.starts_with('F')), "{nc}");
    assert!(nc.contains("(C0 INNER IN C1 - SMALL HOLE AT THE AUTOFS FEED)"), "{nc}");
}

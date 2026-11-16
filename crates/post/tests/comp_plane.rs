//! The plane cutter compensation engages in (#391): RS-274 / ISO 6983
//! offset the cutter in the ACTIVE plane, and every mill generator
//! compensates in XY - so a G41/G42/G40 block after an XZ (G18) or YZ
//! (G19) arc must state G17, and no arc may leave G17 while compensation
//! is engaged (refused by name). Pinned for every post that writes
//! control compensation: Fanuc, Haas, Mazak and Siemens 840D.
//!
//! Gate map (M4.4): covers camcraft_post::post (engine) over the shipped
//! assets/posts fanuc, haas, mazak and sinumerik scripts.

use camcraft_post::post::engine;
use serde_json::Value;

const POSTS: &[&str] = &["fanuc", "haas", "mazak", "sinumerik"];

fn post(stem: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{stem}.cps"));
    std::fs::read_to_string(path).expect("post script")
}

/// An XZ (I K) or YZ (J K) arc leaves G18/G19 modal, then COMP RIGHT
/// engages on the lead and the contour runs in XY.
fn stream(arc: &str) -> String {
    format!(
        "; HSMW-CNC v1
PARAM OperationName C1
PARAM SpindleSpeed 8000
PARAM ClearancePlane 5
TOOL 2 EM6
SECTION
Rapid X -8000 Y 0 Z 5000
Plunge X -8000 Y 0 Z -2000 F 100000
{arc}
COMP RIGHT 3000
Feed X 0 Y 0 Z -2000 F 500000
Feed X 40000 Y 0 Z -2000 F 500000
Feed X 40000 Y 40000 Z -2000 F 500000
Feed X 0 Y 0 Z -2000 F 500000
COMP OFF
Feed X 8000 Y 0 Z -2000 F 500000
Rapid X 8000 Y 0 Z 5000
ENDSECTION
"
    )
}

const XZ_ARC: &str = "Arc CW X -4000 Y 0 Z -2000 F 500000 I 2000 K 0";
const YZ_ARC: &str = "Arc CW X -4000 Y 0 Z -2000 F 500000 J 0 K 2000";

/// A small RS-274 modal reading of the program: the plane each G41 /
/// G42 / G40 word executes in, and whether any G18/G19 word arrives
/// while compensation is engaged. Words in a block are read left to
/// right (a plane word precedes the comp word in every post's blocks);
/// an 840D N block number is skipped.
fn comp_plane_faults(program: &str) -> (usize, Vec<String>) {
    let mut plane = 17;
    let mut comp = false;
    let mut comp_words = 0;
    let mut faults = Vec::new();
    for line in program.lines() {
        let code = line.split(['(', ';']).next().unwrap_or("");
        for word in code.split_whitespace() {
            let w = word.to_ascii_uppercase();
            let g = match w.strip_prefix('G').and_then(|n| n.parse::<u32>().ok()) {
                Some(g) => g,
                None => continue,
            };
            match g {
                17..=19 => {
                    if comp && g != 17 {
                        faults.push(format!("plane switch G{g} while comp is on: {line}"));
                    }
                    plane = g;
                }
                40..=42 => {
                    comp_words += 1;
                    if plane != 17 {
                        faults.push(format!("G{g} in plane G{plane}: {line}"));
                    }
                    comp = g != 40;
                }
                _ => {}
            }
        }
    }
    (comp_words, faults)
}

#[test]
fn every_comp_post_states_g17_before_engaging_after_an_xz_or_yz_arc() {
    for stem in POSTS {
        for (arc, leaves) in [(XZ_ARC, "G18"), (YZ_ARC, "G19")] {
            let out = engine::run(&post(stem), &Value::Null, &stream(arc)).unwrap();
            assert!(out.warnings.is_empty(), "{stem} {leaves}: {:?}", out.warnings);
            assert!(out.program.contains(leaves), "{stem}: the arc must post in {leaves}: {}", out.program);
            let (comp_words, faults) = comp_plane_faults(&out.program);
            // preamble G40, the engage and the cancel
            assert_eq!(comp_words, 3, "{stem} {leaves}: {}", out.program);
            assert!(faults.is_empty(), "{stem} {leaves}: {faults:?}\n{}", out.program);
            let engage = out.program.lines().find(|l| l.contains("G42")).unwrap_or("");
            assert!(engage.contains("G17"), "{stem} {leaves}: the engage block states the plane: {engage}");
        }
    }
}

#[test]
fn a_program_that_never_left_g17_gets_no_extra_plane_word() {
    let xy = "Arc CW X -4000 Y 0 Z -2000 F 500000 I 2000 J 0";
    for stem in POSTS {
        let out = engine::run(&post(stem), &Value::Null, &stream(xy)).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let engage = out.program.lines().find(|l| l.contains("G42")).unwrap_or("");
        assert!(!engage.contains("G17"), "{stem}: {engage}");
        assert!(comp_plane_faults(&out.program).1.is_empty(), "{stem}: {}", out.program);
    }
}

#[test]
fn every_comp_post_refuses_an_xz_or_yz_arc_while_compensation_is_engaged() {
    for stem in POSTS {
        for arc in [XZ_ARC, YZ_ARC] {
            let mid = stream("").replace(
                "Feed X 40000 Y 40000 Z -2000 F 500000\n",
                &format!("Feed X 40000 Y 40000 Z -2000 F 500000\n{}\n", arc.replace("X -4000", "X 40000").replace("Y 0", "Y 40000")),
            );
            let out = engine::run(&post(stem), &Value::Null, &mid).unwrap();
            assert!(
                out.warnings.iter().any(|w| w.contains("while cutter compensation is engaged")),
                "{stem} {arc}: {:?}",
                out.warnings
            );
            assert!(comp_plane_faults(&out.program).1.is_empty(), "{stem}: no plane switch is written: {}", out.program);
        }
    }
}

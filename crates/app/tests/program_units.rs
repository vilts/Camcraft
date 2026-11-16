//! #902: the program's unit is a document setting. The app states the
//! document's unit to every post (`postOptions.Units`); each shipped
//! post either writes the whole program in it - the unit word before
//! any motion, every length / 25.4 and every feed in in/min on an inch
//! program - or refuses an inch document by name. No post takes the
//! program's unit from the machine file any more.
//!
//! The inch programs are checked word by word against the same post's
//! mm program (every length and feed / 25.4, every other word equal),
//! and committed as goldens under `golden/nc/inch/<post>/`.
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test program_units`
//!
//! Provenance: our own tests over our own posts; G20 / G21 are RS-274 /
//! ISO 6983, G700 / G710 the 840D family's published programming manual.
//!
//! Gate map (M4.4): covers camcraft_post::post (every assets/posts/*.cps unit
//! handling), app::posting (program_options), post::units.

mod common;

use camcraft_app::post::{engine, PostError};
use common::legacy_program::as_program;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn vmc() -> Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

fn units(u: &str) -> Value {
    json!({ "Units": u })
}

/// The mill posts that write inch: (post stem, mm unit word, inch unit word).
const INCH_POSTS: &[(&str, &str, &str)] = &[
    ("fanuc", "G21", "G20"),
    ("haas", "G21", "G20"),
    ("mazak", "G21", "G20"),
    ("sinumerik", "G710", "G700"),
    ("doosan-dnm", "G21", "G20"),
    ("tormach", "G21", "G20"),
];

/// The shipped posts that refuse an inch document, and the name the
/// refusal carries.
const MM_ONLY: &[(&str, &str)] = &[
    ("carvera", "Makera Carvera"),
    ("heidenhain", "Heidenhain TNC"),
];

/// Mill `.cl` goldens every inch post must post (arcs, plunges, pecks,
/// a tap's feed).
const GOLDENS: &[&str] = &[
    "contour_plate_with_hole",
    "contour_tangential_lead",
    "face_square_40x40",
    "pocket_zigzag_plate_with_island",
    "slot_wide_channel_rows",
    "drill_plate",
    "drill_peck",
    "tap_plate",
];

/// The goldens committed in inch, per post.
const COMMITTED: &[&str] = &["contour_plate_with_hole", "drill_peck"];

const LENGTHS: &str = "XYZIJKRQ";

fn post(cps: &str, stream: &str, u: &str) -> Result<engine::PostOutput, PostError> {
    engine::run_with(cps, &vmc(), stream, &units(u))
}

/// The words of a line, comments dropped (`( )`, and `;` to the end).
fn words(line: &str) -> Vec<String> {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            ';' if depth == 0 => break,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().map(str::to_owned).collect()
}

fn number(word: &str) -> Option<(char, f64)> {
    let mut cs = word.chars();
    let letter = cs.next()?;
    let v: f64 = cs.as_str().parse().ok()?;
    letter.is_ascii_uppercase().then_some((letter, v))
}

fn is_motion(line: &str) -> bool {
    words(line).iter().any(|w| matches!(w.as_str(), "G0" | "G00" | "G1" | "G01" | "G2" | "G02" | "G3" | "G03"))
}

/// The inch program is the mm program in inches: the same lines and
/// words, the unit word swapped, every length / 25.4 (to its four
/// decimals) and every feed / 25.4 (to its two), everything else equal.
fn same_program_in_inch(stem: &str, g: &str, mm: &str, inch: &str, (mm_word, inch_word): (&str, &str)) {
    let (a, b): (Vec<&str>, Vec<&str>) = (mm.lines().collect(), inch.lines().collect());
    assert_eq!(a.len(), b.len(), "{stem} on {g}: line count differs\nmm:\n{mm}\ninch:\n{inch}");
    for (la, lb) in a.iter().zip(&b) {
        // LinuxCNC's blend tolerance is its own pair (0.025 mm / 0.001 in).
        if la.starts_with("G64") {
            continue;
        }
        let (wa, wb) = (words(la), words(lb));
        assert_eq!(wa.len(), wb.len(), "{stem} on {g}: `{la}` vs `{lb}`");
        for (x, y) in wa.iter().zip(&wb) {
            if x == mm_word {
                assert_eq!(y, inch_word, "{stem} on {g}: `{la}` vs `{lb}`");
                continue;
            }
            match (number(x), number(y)) {
                (Some((lx, vx)), Some((ly, vy))) if lx == ly && LENGTHS.contains(lx) => {
                    let want = vx / 25.4;
                    // mm is rounded to 0.0005 before the divide, inch to 0.00005.
                    assert!((vy - want).abs() <= 0.0005 / 25.4 + 0.00005 + 1e-9, "{stem} on {g}: {x} -> {y} (want {want:.5})");
                }
                (Some(('F', vx)), Some(('F', vy))) => {
                    let want = vx / 25.4;
                    // mm/min is whole before the divide, in/min at two decimals.
                    assert!((vy - want).abs() <= 0.5 / 25.4 + 0.005 + 1e-9, "{stem} on {g}: {x} -> {y} (want {want:.3})");
                }
                _ => assert_eq!(x, y, "{stem} on {g}: `{la}` vs `{lb}`"),
            }
        }
    }
    // The unit word comes before the first motion, and the other never shows.
    let unit = b.iter().position(|l| words(l).iter().any(|w| w == inch_word));
    let first = b.iter().position(|l| is_motion(l));
    assert!(matches!((unit, first), (Some(u), Some(f)) if u < f), "{stem} on {g}: no {inch_word} before motion:\n{inch}");
    assert!(!b.iter().any(|l| words(l).iter().any(|w| w == mm_word)), "{stem} on {g}: {mm_word} in an inch program:\n{inch}");
}

#[test]
fn every_inch_post_writes_the_mm_program_in_inches() {
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (stem, mm_word, inch_word) in INCH_POSTS {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let mut posted = 0;
        for g in GOLDENS {
            // #1357: the golden as the program a real operation streams
            // (tool, spindle, each cycle's feed) - a bare legacy stream is
            // refused by name (a cycle with no feed, a stopped spindle).
            let stream = as_program(g, &read(&format!("crates/app/tests/golden/{g}.cl")));
            let mm = post(&cps, &stream, "mm").unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            let inch = post(&cps, &stream, "inch").unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            assert_eq!(mm.warnings, inch.warnings, "{stem} on {g}: the unit changes no verdict");
            if !mm.warnings.is_empty() {
                continue; // refused in either unit alike
            }
            posted += 1;
            same_program_in_inch(stem, g, &mm.program, &inch.program, (mm_word, inch_word));
            // The mm program is the one an unstated unit posts: byte-identical.
            let plain = engine::run(&cps, &vmc(), &stream).unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            assert_eq!(plain.program, mm.program, "{stem} on {g}: Units mm must post what no unit posted");
            if COMMITTED.contains(g) {
                let path = repo(&format!("crates/app/tests/golden/nc/inch/{stem}/{g}.nc"));
                if write {
                    std::fs::create_dir_all(path.parent().expect("dir")).expect("golden dir");
                    std::fs::write(&path, &inch.program).expect("write golden");
                } else {
                    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    assert_eq!(inch.program, want, "{stem} on {g}: inch golden moved");
                }
            }
        }
        // Every golden posts: a refusal here would skip the comparison
        // (and the committed inch golden) silently.
        assert_eq!(posted, GOLDENS.len(), "{stem}: every golden posts");
    }
}

#[test]
fn a_post_that_cannot_write_inch_refuses_the_document_by_name() {
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    for (stem, name) in MM_ONLY {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        match post(&cps, &stream, "inch") {
            Err(PostError::Refused(why)) => {
                assert!(why.contains(name) && why.contains("inch"), "{stem}: refusal must name the post and the unit: {why}");
            }
            Err(e) => panic!("{stem}: refused, but not by name: {e}"),
            Ok(out) => panic!("{stem}: posted an inch document:\n{}", out.program),
        }
    }
}

#[test]
fn a_script_that_declares_no_units_refuses_inch_and_posts_mm() {
    // A shop's own script from before #902: no `programUnits`.
    let cps = "function onOpen() { writeln(\"G21\"); }\nfunction onLinear() { writeln(\"G1 X\" + properties.X); }";
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    match post(cps, &stream, "inch") {
        Err(PostError::Refused(why)) => assert!(why.contains("does not declare programUnits"), "{why}"),
        other => panic!("an undeclared script must refuse inch by name, got {other:?}"),
    }
    assert!(post(cps, &stream, "mm").is_ok(), "mm needs no declaration");
    // An unknown unit word is refused, never read as mm.
    assert!(matches!(post(cps, &stream, "in"), Err(PostError::Refused(_))));
}

#[test]
fn no_shipped_post_takes_the_program_unit_from_the_machine_file() {
    let dir = repo("assets/posts");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).expect("posts dir") {
        let p = e.expect("entry").path();
        if p.extension().and_then(|x| x.to_str()) != Some("cps") {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&p).expect("cps");
        assert!(!text.contains("machine.units"), "{}: reads machine.units", p.display());
        assert!(text.lines().any(|l| l.starts_with("programUnits = \"")), "{}: no programUnits manifest", p.display());
    }
    assert!(seen >= 11, "every shipped post is checked ({seen})");
}

#[test]
fn an_inch_machine_file_no_longer_makes_an_inch_program() {
    // The Tormach mill's shipped machine file states inch; the document
    // (here: no unit stated, mm) decides.
    let mch: Value = serde_json::from_str(&read("assets/machines/tormach-1100mx.mch")).expect("machine json");
    let cps = read("assets/posts/tormach.cps");
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    let mm = engine::run_with(&cps, &mch, &stream, &units("mm")).expect("posts");
    assert!(mm.program.contains("G21") && !mm.program.contains("G20"), "{}", mm.program);
    let inch = engine::run_with(&cps, &mch, &stream, &units("inch")).expect("posts");
    assert!(inch.program.contains("G20") && !inch.program.contains("G21"), "{}", inch.program);
}

fn mounted(rel: &str) -> Value {
    camcraft_app::post::machine::mirror(&camcraft_app::machine::parse_mch(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")))
}

/// The A/B/C words of a program, in order (comments dropped).
fn angle_words(program: &str) -> Vec<String> {
    program
        .lines()
        .flat_map(words)
        .filter(|w| matches!(number(w), Some(('A' | 'B' | 'C', _))))
        .collect()
}

/// #981 review: a rotary angle is degrees in either program unit. Fanuc
/// and Doosan DNM write their angle words through `angleWord` (main's
/// #997 gate on a stated rotary); it must use the degree formatter, never
/// the length one this change made unit-aware - an inch program dividing
/// A/B/C by 25.4 would turn the table 25.4 times short. Covers the RTCP
/// arm and modal walk on the A+B tilthead (both posts) and Doosan's
/// 4th-axis index path.
#[test]
fn rotary_angle_words_are_degrees_in_an_inch_program() {
    // A real stream states its spindle (a cut with it stopped is refused).
    let rtcp = "PARAM ClearancePlane 20\nTOOL 1 T1\nPARAM SpindleSpeed 6000\nSECTION\nRapid X 0 Y 0 Z 20000 A 37500 B 0 C 0\nFeed X 20000 Y 0 Z 0 F 300000 A 0 B 12250 C 0\nFeed X 25400 Y 5000 Z 0 F 300000 A -45000 B 12250 C 0\nENDSECTION\n";
    let index = "PARAM ClearancePlane 10\nTOOL 1 T1\nPARAM SpindleSpeed 6000\nSECTION\nRapid X 0 Y 0 Z 10000 A 90000\nFeed X 50000 Y 0 Z -1000 F 400000 A 90000\nRapid X 50000 Y 0 Z 10000 A 127500\nFeed X 0 Y 0 Z -1000 F 400000 A 127500\nENDSECTION\n";
    let cases: [(&str, &str, &str, &[&str]); 3] = [
        ("fanuc", "assets/machines/tilthead.mch", rtcp, &["A37.5", "B0.", "A0.", "B12.25", "A-45."]),
        ("doosan-dnm", "assets/machines/tilthead.mch", rtcp, &["A37.5", "B0.", "A0.", "B12.25", "A-45."]),
        ("doosan-dnm", "assets/machines/doosan-dnm.mch", index, &["A90.", "A127.5"]),
    ];
    for (stem, mch_rel, stream, want) in cases {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let mch = mounted(mch_rel);
        let run = |u: &str| {
            let out = engine::run_with(&cps, &mch, stream, &json!({ "Units": u, "rotaryBrake": true }))
                .unwrap_or_else(|e| panic!("{stem} on {mch_rel}: {e}"));
            assert!(out.warnings.is_empty(), "{stem} on {mch_rel} ({u}): {:?}", out.warnings);
            out.program
        };
        let (mm, inch) = (run("mm"), run("inch"));
        assert!(inch.contains("G20") && !mm.contains("G20"), "{stem}: the inch program is inch:\n{inch}");
        assert_eq!(angle_words(&mm), *want, "{stem} on {mch_rel}: mm angle words\n{mm}");
        assert_eq!(angle_words(&inch), *want, "{stem} on {mch_rel}: an inch program's angles are degrees, unscaled\n{inch}");
        // And the rest of the program is the mm program in inches, word by word.
        same_program_in_inch(stem, mch_rel, &mm, &inch, ("G21", "G20"));
    }
}

/// #981 review: grblHAL and TinyG (new on main) take the program's unit
/// from the document too, and their inch program is their mm program in
/// inches word by word, on every mill golden.
#[test]
fn grbl_family_posts_write_the_mm_program_in_inches() {
    for (stem, machine) in [("grblhal", "grblHAL 3-axis"), ("tinyg", "TinyG 3-axis")] {
        let cps = read(&format!("assets/posts/{stem}.cps"));
        let text = camcraft_app::posting::MACHINES
            .iter()
            .find(|(n, _)| *n == machine)
            .map(|(_, t)| *t)
            .unwrap_or_else(|| panic!("the library ships {machine}"));
        let mch = camcraft_app::post::machine::mirror(&camcraft_app::machine::parse_mch(text).unwrap_or_else(|e| panic!("{machine}: {e}")));
        let mut posted = 0;
        for g in GOLDENS {
            // #1357, #1359: the golden as a real program (tool change,
            // spindle, each cycle's feed) - a cut with the spindle stopped
            // is refused by name - with the clearance height a real stream
            // states, which these posts lift to at a tool change.
            let stream = as_program(g, &read(&format!("crates/app/tests/golden/{g}.cl")))
                .replacen("PARAM SetupName", "PARAM ClearancePlane 15\nPARAM SetupName", 1);
            let mm = engine::run_with(&cps, &mch, &stream, &units("mm")).unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            let inch = engine::run_with(&cps, &mch, &stream, &units("inch")).unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            assert_eq!(mm.warnings, inch.warnings, "{stem} on {g}: the unit changes no verdict");
            if !mm.warnings.is_empty() {
                continue; // refused in either unit alike (e.g. a tap)
            }
            posted += 1;
            same_program_in_inch(stem, g, &mm.program, &inch.program, ("G21", "G20"));
            let plain = engine::run(&cps, &mch, &stream).unwrap_or_else(|e| panic!("{stem} on {g}: {e}"));
            assert_eq!(plain.program, mm.program, "{stem} on {g}: Units mm must post what no unit posted");
        }
        assert!(posted >= 5, "{stem}: most goldens post ({posted})");
    }
}

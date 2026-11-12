//! The Tormach post's end-units option (#596): LinuxCNC's M2 / M30 does
//! not reset G20 / G21 (linuxcnc.org/docs, M-codes, "M2, M30 Program
//! End"), so a mm program leaves an inch-run PathPilot control in mm.
//! With the option set and the program's unit different, the target's
//! word goes right before M30; matching units or the option left at its
//! default post byte-identically. The option rides `SetPostOptions`
//! (undo / redo) and the project file.
//! Provenance: our own tests over our own post, command and file.
//!
//! Gate map (M4.4): covers post::post (tormach.cps), app::posting
//! (options_value), app::commands (SetPostOptions), app::project
//! (post_options end_units), app::schema (EndUnits).

use camcraft_app::commands::{History, SetPostOptions};
use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use camcraft_app::posting;
use camcraft_app::schema::{EndUnits, PostOptions};
use camcraft_app::world::new_world;
use std::path::{Path, PathBuf};

const INCH_MCH: &str = "assets/machines/tormach-770mx.mch";
const METRIC_MCH: &str = "assets/machines/3axis-vmc.mch";
const GOLDENS: [&str; 3] = ["face_square_40x40", "contour_disc_hole", "drill_peck"];

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn machine_for(mch: &str) -> serde_json::Value {
    if mch == INCH_MCH {
        machine::mirror(&parse_mch(&read(mch)).unwrap_or_else(|e| panic!("{mch}: {e}")))
    } else {
        serde_json::from_str(&read(mch)).unwrap_or_else(|e| panic!("{mch}: {e}"))
    }
}

/// Post one golden stream through tormach.cps with the given end units.
fn post(mch: &str, golden: &str, end: Option<EndUnits>) -> String {
    let cps = read("assets/posts/tormach.cps");
    let stream = read(&format!("crates/app/tests/golden/{golden}.cl"));
    let m = machine_for(mch);
    let out = match end {
        None => engine::run(&cps, &m, &stream),
        Some(e) => engine::run_with(
            &cps,
            &m,
            &stream,
            &posting::options_value(PostOptions { end_units: e, ..PostOptions::default() }),
        ),
    }
    .unwrap_or_else(|e| panic!("{golden}: {e}"));
    assert!(out.warnings.is_empty(), "{golden} on {mch}: {:?}", out.warnings);
    out.program
}

fn tail(nc: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = nc.lines().collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

#[test]
fn the_default_option_writes_no_end_units_key() {
    let v = posting::options_value(PostOptions::default());
    assert!(v.get("EndUnits").is_none(), "{v}");
    let v = posting::options_value(PostOptions { end_units: EndUnits::Inch, ..PostOptions::default() });
    assert_eq!(v["EndUnits"], "inch");
    let v = posting::options_value(PostOptions { end_units: EndUnits::Millimeters, ..PostOptions::default() });
    assert_eq!(v["EndUnits"], "mm");
}

#[test]
fn left_at_the_default_the_program_is_byte_identical() {
    for mch in [INCH_MCH, METRIC_MCH] {
        for g in GOLDENS {
            assert_eq!(post(mch, g, None), post(mch, g, Some(EndUnits::Leave)), "{g} on {mch}");
        }
    }
}

#[test]
fn a_mm_program_for_an_inch_control_ends_g20_right_before_m30() {
    for g in GOLDENS {
        let plain = post(METRIC_MCH, g, None);
        let nc = post(METRIC_MCH, g, Some(EndUnits::Inch));
        assert!(plain.contains("G21 G17 G90"), "{g}: a mm program\n{plain}");
        assert_eq!(tail(&nc, 4), ["G53 G0 Z0", "G30", "G20", "M30"], "{g}\n{nc}");
        // Exactly one line added, nothing else touched.
        let added = plain.replacen("G30\nM30", "G30\nG20\nM30", 1);
        assert_eq!(nc, added, "{g}: only the G20 line is new");
    }
}

#[test]
fn an_inch_program_for_a_mm_control_ends_g21_right_before_m30() {
    for g in GOLDENS {
        let nc = post(INCH_MCH, g, Some(EndUnits::Millimeters));
        assert_eq!(tail(&nc, 4), ["G53 G0 Z0", "G30", "G21", "M30"], "{g}\n{nc}");
    }
}

#[test]
fn matching_units_add_nothing() {
    for g in GOLDENS {
        assert_eq!(post(INCH_MCH, g, Some(EndUnits::Inch)), post(INCH_MCH, g, None), "{g}: inch on inch");
        assert_eq!(post(METRIC_MCH, g, Some(EndUnits::Millimeters)), post(METRIC_MCH, g, None), "{g}: mm on mm");
    }
}

#[test]
fn the_end_units_choice_undoes_and_redoes() {
    let world = new_world();
    let mut history = History::default();
    assert_eq!(posting::post_options(&world).end_units, EndUnits::Leave);
    let inch = PostOptions { end_units: EndUnits::Inch, ..PostOptions::default() };
    history.execute(&world, SetPostOptions::new(&world, inch).expect("choose inch"));
    assert_eq!(posting::post_options(&world).end_units, EndUnits::Inch);
    assert!(SetPostOptions::new(&world, inch).is_err(), "the same choice again is no step");
    history.undo(&world);
    assert_eq!(posting::post_options(&world).end_units, EndUnits::Leave, "undo restores leaving it");
    history.redo(&world);
    assert_eq!(posting::post_options(&world).end_units, EndUnits::Inch, "redo reapplies it");
}

#[test]
fn the_end_units_choice_round_trips_and_old_files_leave_it() {
    let dir = std::env::temp_dir();
    for (end, word) in [(EndUnits::Inch, "\"end_units\": \"inch\""), (EndUnits::Millimeters, "\"end_units\": \"mm\"")] {
        let world = new_world();
        let mut history = History::default();
        let next = PostOptions { end_units: end, ..PostOptions::default() };
        history.execute(&world, SetPostOptions::new(&world, next).expect("choose"));
        let path = dir.join(format!("camcraft-end-units-{}-{}.json", std::process::id(), end as u8));
        camcraft_app::project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("written");
        assert!(text.contains(word), "{text}");
        let back = new_world();
        camcraft_app::project::load(&back, &path).expect("load");
        assert_eq!(posting::post_options(&back), next);

        // An unknown word is a corrupt file, refused by name.
        std::fs::write(&path, text.replace(word, "\"end_units\": \"furlong\"")).expect("rewrite");
        let err = camcraft_app::project::load(&new_world(), &path).expect_err("refused");
        assert!(format!("{err}").contains("furlong"), "{err}");
        let _ = std::fs::remove_file(&path);
    }
    // The default writes no key, and a file without one leaves the unit.
    let world = new_world();
    let path = dir.join(format!("camcraft-end-units-{}-default.json", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(!text.contains("end_units") && !text.contains("post_options"), "{text}");
}

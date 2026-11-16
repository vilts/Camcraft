//! #616: per-operation smoothing. Off (the default) a program carries no
//! smoothing word and is byte-identical to before; on, each operation's
//! section states its ask (a finish tolerance, or a rough one from the
//! stock it leaves) and each dialect writes its control's words, undone
//! at the section's end.
//!
//! Gate map (#616): covers app::smoothing (of, rough_tol), post::dump
//! (the Smoothing PARAMs), the mill posts' smoothOn/smoothOff (Carvera: #757;
//! grblHAL's #710 note included).
//! Provenance: our own fixtures; each dialect's words from its public
//! programming documentation (see the posts).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{self, Height, OpParams, OpType};
use camcraft_app::post::{self, cnc, Dump, Smoothing};
use camcraft_app::posting::BUILTINS;
use camcraft_app::smoothing::{self, FINISH_SHARE, ROUGH_MAX_MM};

fn script(name: &str) -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).unwrap_or_else(|| panic!("{name} is built in"))
}

/// A one-feed section, with or without a smoothing ask.
fn program(name: &str, ask: Option<Smoothing>) -> String {
    let p = |x: f64, z: f64| Point3::new(x, 0.0, z);
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 25.0), p(0.0, 5.0), None),
        Move::new(MoveKind::Feed, p(0.0, 5.0), p(20.0, -1.0), Some(600.0)),
        Move::new(MoveKind::Rapid, p(20.0, -1.0), p(20.0, 25.0), None),
    ]);
    let d = Dump {
        op_name: "Contour1",
        tool_number: 1,
        tool_name: "EM",
        tool_diameter: 6.0,
        rpm: 8000.0,
        clearance_z: 25.0,
        retract_z: 25.0,
        smoothing: ask,
        ..Dump::default()
    };
    let text = cnc::serialize(&post::dump(&t, &d));
    match post::post(script(name), &serde_json::Value::Null, &text) {
        Ok(out) => out.program,
        Err(e) => panic!("{name}: {e}"),
    }
}

fn lines_with(prog: &str, word: &str) -> Vec<String> {
    prog.lines().filter(|l| l.contains(word)).map(str::to_owned).collect()
}

const FINISH: Option<Smoothing> = Some(Smoothing { tol_mm: 0.01, finish: true });
const ROUGH: Option<Smoothing> = Some(Smoothing { tol_mm: 0.1, finish: false });

#[test]
fn off_is_byte_identical_and_writes_no_smoothing_word() {
    // #542: a sinker post serves sinker EDMs only (no smoothing words).
    let sinker = camcraft_app::posting::SINKER_POSTS;
    for (name, _) in BUILTINS.iter().filter(|(n, _)| *n != "Fanuc Lathe" && !sinker.contains(n)) {
        let off = program(name, None);
        for w in ["G5.1", "G187", "CYCLE832", "G62 ", "SMOOTHING"] {
            assert!(!off.contains(w), "{name} wrote {w} with smoothing off:\n{off}");
        }
        // The Tormach header's fixed blending is unchanged (one G64).
        if *name == "Tormach PathPilot" {
            assert_eq!(lines_with(&off, "G64").len(), 1, "{off}");
        }
    }
}

#[test]
fn each_dialect_writes_its_words_and_undoes_them_at_the_section_end() {
    // (post, finish words, rough words, the reset line's words)
    let cases: [(&str, &[&str], &[&str], &str); 4] = [
        ("Tormach PathPilot", &["G64", "P0.01"], &["G64", "P0.1"], "G64 P0.025"),
        ("Haas", &["G187", "P3", "E0.01"], &["G187", "P1", "E0.1"], "G187"),
        ("Siemens 840D", &["CYCLE832(0.01,_FINISH,1)"], &["CYCLE832(0.1,_ROUGH,1)"], "CYCLE832()"),
        ("Heidenhain", &["G62", "T0.01"], &["G62", "T0.1"], "G62"),
    ];
    for (name, fin, rough, reset) in cases {
        for (ask, want) in [(FINISH, fin), (ROUGH, rough)] {
            let prog = program(name, ask);
            let on = prog
                .lines()
                .position(|l| want.iter().all(|w| l.contains(w)))
                .unwrap_or_else(|| panic!("{name}: no line with {want:?}:\n{prog}"));
            let feed = prog
                .lines()
                .position(|l| l.split_whitespace().any(|w| matches!(w.trim_end_matches('*'), "G1" | "G01")))
                .unwrap_or(usize::MAX);
            assert!(on < feed, "{name}: smoothing stated before the first cut:\n{prog}");
            let off = prog
                .lines()
                .skip(on + 1)
                .position(|l| l.contains(reset))
                .unwrap_or_else(|| panic!("{name}: no reset '{reset}' after the section:\n{prog}"));
            let _ = off;
        }
    }
}

#[test]
fn fanuc_and_mazak_name_the_ask_once_and_write_no_unbounded_word() {
    // Fanuc's G5.1 states no tolerance (review of #635), Mazak has no
    // stated form and grblHAL lists G61 only (#710): the ask is named,
    // never written loose.
    for name in ["Fanuc", "Mazak", "grblHAL"] {
        let prog = program(name, FINISH);
        assert_eq!(lines_with(&prog, "SMOOTHING NOT WRITTEN").len(), 1, "{name}:\n{prog}");
        assert!(!prog.contains("G5.1 Q"), "{name}: no unbounded G5.1:\n{prog}");
    }
}

#[test]
fn carvera_names_the_ask_once_and_otherwise_posts_the_off_program() {
    // #757: the Carvera firmware's supported-codes tables list no blending
    // or smoothing word, so the ask is named once - never a guessed G64 -
    // and the program is otherwise byte-identical to smoothing off.
    let off = program("Makera Carvera", None);
    for ask in [FINISH, ROUGH] {
        let on = program("Makera Carvera", ask);
        let named = lines_with(&on, "SMOOTHING NOT WRITTEN");
        assert_eq!(named, ["(SMOOTHING NOT WRITTEN - NO CARVERA FORM STATED)"], "{on}");
        for w in ["G64", "G61", "G5.1", "G187"] {
            assert!(!on.contains(w), "Carvera wrote {w}:\n{on}");
        }
        let feed = on.lines().position(|l| l.starts_with("G1 ")).unwrap_or(usize::MAX);
        let note = on.lines().position(|l| l.contains("SMOOTHING NOT WRITTEN")).unwrap_or(usize::MAX);
        assert!(note < feed, "named before the first cut:\n{on}");
        let stripped: String = on.lines().filter(|l| !l.contains("SMOOTHING NOT WRITTEN")).map(|l| format!("{l}\n")).collect();
        let off_lines: String = off.lines().map(|l| format!("{l}\n")).collect();
        assert_eq!(stripped, off_lines, "only the note differs");
    }
}

#[test]
fn the_smoothing_reset_comes_before_the_program_end() {
    // The last section's reset is written before the program ends, so no
    // tolerance outlives the program.
    for (name, reset) in [("Tormach PathPilot", "G64 P0.025"), ("Haas", "G187"), ("Siemens 840D", "CYCLE832()"), ("Heidenhain", "G62")] {
        let prog = program(name, FINISH);
        let lines: Vec<&str> = prog.lines().collect();
        let last_reset = lines.iter().rposition(|l| l.trim_end_matches('*').trim_end().ends_with(reset) || l.contains(&format!("{reset} ")) || l.contains(&format!("{reset}*")));
        let end = lines
            .iter()
            .rposition(|l| l.split_whitespace().any(|w| matches!(w.trim_end_matches('*'), "M30" | "M02" | "M2")));
        match (last_reset, end) {
            (Some(r), Some(e)) => assert!(r < e, "{name}: reset after the program end:\n{prog}"),
            other => panic!("{name}: reset/end not found {other:?}:\n{prog}"),
        }
    }
}

#[test]
fn finish_ops_ask_their_tolerance_and_rough_ops_ask_from_their_stock() {
    // A contour that finishes asks its own tolerance; leaving stock, it roughs.
    let base = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let seed = |ty: OpType| operation::retype_params(&base, ty.ordinal());
    let mut contour = seed(OpType::Contour);
    if let OpParams::Contour { stock_to_leave, tolerance, .. } = &mut contour {
        *stock_to_leave = 0.0;
        *tolerance = 0.02;
    }
    // Half the tolerance (the control's deviation stacks on the verified path).
    assert_eq!(smoothing::of(&contour), Some(Smoothing { tol_mm: 0.02 * FINISH_SHARE, finish: true }));
    if let OpParams::Contour { stock_to_leave, .. } = &mut contour {
        *stock_to_leave = 0.5;
    }
    assert_eq!(smoothing::of(&contour), Some(Smoothing { tol_mm: 0.1, finish: false }), "a fifth of 0.5 mm");
    // Never more than a fifth of the stock (no floor), capped at the max.
    for leave in [0.001, 0.02, 0.04, 0.5, 1.25] {
        let t = smoothing::rough_tol(leave).expect("a positive allowance asks");
        assert!(t <= leave * 0.2 + 1e-12 && t > 0.0, "leave {leave}: {t}");
    }
    assert_eq!(smoothing::rough_tol(10.0), Some(ROUGH_MAX_MM));
    assert_eq!(smoothing::rough_tol(0.0), None);
    assert_eq!(smoothing::rough_tol(f64::NAN), None);
    // Drilling asks nothing.
    assert_eq!(smoothing::of(&seed(OpType::Drill)), None);
}

#[test]
fn the_switch_undoes_redoes_and_round_trips_the_project_file() {
    use camcraft_app::commands::{History, SetPostOptions};
    use camcraft_app::schema::PostOptions;
    use camcraft_app::{posting, project, world::new_world};
    let world = new_world();
    let mut h = History::default();
    assert!(!posting::post_options(&world).smoothing, "off by default");
    let on = PostOptions { smoothing: true, ..posting::post_options(&world) };
    h.execute(&world, SetPostOptions::new(&world, on).expect("set"));
    assert!(posting::post_options(&world).smoothing);
    assert!(h.undo(&world) && !posting::post_options(&world).smoothing, "undo turns it off");
    assert!(h.redo(&world) && posting::post_options(&world).smoothing, "redo turns it on");
    let path = std::env::temp_dir().join(format!("camcraft-smoothing-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"smoothing\": true") || text.contains("\"smoothing\":true"), "the key is written when on");
    let back = new_world();
    project::load(&back, &path).expect("load");
    assert!(posting::post_options(&back).smoothing, "the key round-trips");
    // Off writes no key at all.
    assert!(h.undo(&world));
    project::save(&world, &path).expect("save off");
    let off = std::fs::read_to_string(&path).expect("read off");
    assert!(!off.contains("smoothing"), "off writes no key");
    let _ = std::fs::remove_file(&path);
}

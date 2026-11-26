//! The Fanuc LATHE post writes inch programs (#977): an inch document
//! posts G20 with every value in its inch unit - X diameter, Z, I K and
//! G76 R / 25.4 at four decimals; G99 feeds in in/rev; thread leads in
//! inches at six; the no-point increment words (G76 P Q, G83 Q) in the
//! inch least increment (0.0001 in) where mm counts microns; G96 S in
//! surface feet per minute. Rpm (G97, G50), dwell ms and the G76 P code
//! never scale. A C-axis live-tool section on an inch program is refused
//! by name - never written with an unconfirmed composite feed.
//!
//! Each turning stream of the lathe suites (facing, rough + finish,
//! groove + part, the axial drill, G32 and G76 threads) posts in mm
//! byte-identical to its committed mm golden, and in inch checked word
//! by word against that mm program; the inch programs are committed
//! under `golden/nc/inch/fanuc-lathe/`. Regenerate after an
//! intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_inch`
//!
//! Provenance: our own tests over our own post; G20 / G21, G96 / G99
//! and the two-block G76 from the public Fanuc 0i-T programming
//! vocabulary (behaviour only).
//!
//! Gate map (M4.4): covers camcraft_post::post (engine on the lathe dialect, inch),
//! app::posting (program_options Units), app::commands (SetUnits).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod fanuc_lathe_inch;

use serde_json::json;

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{PostOptions, ThreadingOutput};
use fanuc_lathe_inch::{fixture, lathe_mch, words, CPS, FIXTURES};

fn units(o: PostOptions, u: &str) -> serde_json::Value {
    let mut v = posting::options_value(o);
    if let Some(m) = v.as_object_mut() {
        m.insert("Units".into(), json!(u));
    }
    v
}

fn num(w: &str) -> Option<(char, f64)> {
    let mut cs = w.chars();
    let l = cs.next()?;
    cs.as_str().parse().ok().filter(|_| l.is_ascii_uppercase()).map(|v| (l, v))
}

/// The inch program is the mm program in its inch units, word by word.
fn same_program_in_inch(name: &str, mm: &str, inch: &str) {
    let (a, b): (Vec<&str>, Vec<&str>) = (mm.lines().collect(), inch.lines().collect());
    assert_eq!(a.len(), b.len(), "{name}: line count differs\nmm:\n{mm}\ninch:\n{inch}");
    for (la, lb) in a.iter().zip(&b) {
        let (wa, wb) = (words(la), words(lb));
        assert_eq!(wa.len(), wb.len(), "{name}: `{la}` vs `{lb}`");
        let has = |g: &str| wa.iter().any(|w| w == g);
        let g76_second = has("G76") && wa.iter().any(|w| w.starts_with('X'));
        let cycle = has("G83") || has("G84") || has("G85");
        for (x, y) in wa.iter().zip(&wb) {
            if x == "G21" {
                assert_eq!(y, "G20", "{name}: `{la}` vs `{lb}`");
                continue;
            }
            let (Some((lx, vx)), Some((ly, vy))) = (num(x), num(y)) else {
                assert_eq!(x, y, "{name}: `{la}` vs `{lb}`");
                continue;
            };
            assert_eq!(lx, ly, "{name}: `{la}` vs `{lb}`");
            let scaled = |want: f64, tol: f64| {
                assert!((vy - want).abs() <= tol + 1e-9, "{name}: {x} -> {y} (want {want:.6}) in `{lb}`");
            };
            match lx {
                // Lengths: mm at three decimals, inch at four.
                'X' | 'Z' | 'I' | 'K' | 'R' => scaled(vx / 25.4, 0.0005 / 25.4 + 0.00005),
                // Per-rev feeds (four decimals both) and leads (inch at six).
                'F' => scaled(vx / 25.4, 0.00005 / 25.4 + 0.00005),
                // No-point increments: microns -> 0.0001 in.
                'Q' if has("G76") || cycle => scaled(vx / 2.54, 1.0),
                'P' if g76_second => scaled(vx / 2.54, 1.0),
                // Surface speed: m/min -> ft/min.
                'S' if has("G96") => scaled(vx / 0.3048, 0.5 + 0.5 / 0.3048),
                _ => assert_eq!(x, y, "{name}: `{la}` vs `{lb}`"),
            }
        }
    }
    assert!(!b.iter().any(|l| words(l).iter().any(|w| w == "G21")), "{name}: G21 in an inch program");
    let unit = b.iter().position(|l| words(l).iter().any(|w| w == "G20"));
    let first = b.iter().position(|l| words(l).iter().any(|w| matches!(w.as_str(), "G00" | "G01" | "G02" | "G03" | "G32")));
    assert!(matches!((unit, first), (Some(u), Some(f)) if u < f), "{name}: no G20 before motion:\n{inch}");
}

#[test]
fn every_turning_stream_posts_the_mm_program_in_inches() {
    for &(name, mm_golden) in FIXTURES {
        let (world, options) = fixture(name);
        let records = posting::stream_records(&world).expect("generated");
        let stream = cnc::serialize(&records);
        let mch = posting::machine_value(&world);
        let mm = engine::run_with(CPS, &mch, &stream, &units(options, "mm")).expect("mm posts");
        let inch = engine::run_with(CPS, &mch, &stream, &units(options, "inch")).expect("inch posts");
        let stream_warns = records.iter().filter(|r| matches!(r, cnc::Record::Warn { .. })).count();
        assert_eq!(mm.warnings.len(), stream_warns, "{name}: the script refused in mm: {:?}", mm.warnings);
        assert_eq!(mm.warnings, inch.warnings, "{name}: the unit changes no verdict");
        // mm is untouched: the committed mm golden, byte for byte.
        if let Some(g) = mm_golden {
            let want = std::fs::read_to_string(common_golden(g)).expect("mm golden");
            assert_eq!(mm.program.replace("\r\n", "\n"), want.replace("\r\n", "\n"), "{name}: the mm program moved");
        }
        let plain = engine::run_with(CPS, &mch, &stream, &posting::options_value(options)).expect("runs");
        assert_eq!(plain.program, mm.program, "{name}: Units mm posts what no unit posted");
        same_program_in_inch(name, &mm.program, &inch.program);
        if name == "thread_g32_dips" {
            // #419 dips in inch: G32 X Z F segments, every word with its
            // decimal point (#1836), the lead at six decimals.
            let dips: Vec<&str> = inch.program.lines().filter(|l| l.starts_with("G32 X")).collect();
            assert!(!dips.is_empty(), "no dip segment:\n{}", inch.program);
            for l in &dips {
                assert!(words(l).iter().skip(1).all(|w| w.contains('.')), "a word without its point: {l}");
                assert!(l.ends_with(" F0.07874"), "{l}");
            }
        }
        if name == "groove_part" {
            // The bar feeder's 30 mm advance is named in inches, never a bare mm number.
            assert!(inch.program.contains("(PART LOAD ADVANCE 1.1811 IN)"), "{}", inch.program);
            assert!(mm.program.contains("(PART LOAD ADVANCE 30)"), "{}", mm.program);
        }
        common::write_golden(&format!("nc/inch/fanuc-lathe/{name}.nc"), &inch.program);
    }
}

fn common_golden(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(rel)
}

#[test]
fn the_inch_thread_words_are_pinned() {
    let (world, options) = fixture("thread_g76");
    assert_eq!(options.threading, ThreadingOutput::G76Cycle);
    let records = posting::stream_records(&world).expect("generated");
    let mch = posting::machine_value(&world);
    let out = engine::run_with(CPS, &mch, &cnc::serialize(&records), &units(options, "inch")).expect("posts");
    let p = &out.program;
    // mm: G00 X12.1 Z16. / G76 P010000 Q400 R0.4 / G76 X9.6 Z8. P1200 Q400 F2.
    // 0.4 mm = 157 ten-thousandths, 1.2 mm = 472, lead 2 mm = 0.07874 in.
    // R is 0.0158, not 0.4 mm's 0.0157: the control reads the rounded
    // words, and 472 - 157 = 315 would sit one past the second rung 314
    // and cut a fourth pass (PR #517 review; lathe_post_g76_schedule).
    assert!(p.contains("G00 X0.4764 Z0.6299\nG76 P010000 Q157 R0.0158\n"), "{p}");
    assert!(p.contains("G76 X0.378 Z0.315 P472 Q157 F0.07874\n"), "{p}");
    assert!(p.contains("G20 G18 G40 G80 G99\n"), "{p}");
    // The G32 passes carry the lead at six decimals.
    let (world, options) = fixture("thread_g32");
    let records = posting::stream_records(&world).expect("generated");
    let out = engine::run_with(CPS, &posting::machine_value(&world), &cnc::serialize(&records), &units(options, "inch"))
        .expect("posts");
    assert!(out.program.contains(" F0.07874\n") && out.program.contains("G32 Z"), "{}", out.program);
}

#[test]
fn an_inch_document_posts_inch_through_the_gate() {
    use camcraft_app::commands::{SelectPost, SetUnits};
    use camcraft_app::schema::PostRef;
    use camcraft_app::units::Unit;
    let (world, options) = fixture("axial_drill");
    let mut history = camcraft_app::commands::History::default();
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    history.execute(&world, SetUnits::new(&world, Unit::Inches).expect("inch document"));
    let path = std::env::temp_dir().join(format!("camcraft-lathe-inch-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("an inch lathe document posts through the gate");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    let records = posting::stream_records(&world).expect("generated");
    let want = engine::run_with(CPS, &posting::machine_value(&world), &cnc::serialize(&records), &units(options, "inch"))
        .expect("posts");
    assert_eq!(text, want.program, "the gated program is the stream's inch translation");
    assert!(text.contains("G20 G18 G40 G80 G99\n") && text.contains("G83 Z0.3937 "), "{text}");
    assert!(text.contains("G50 S12000\n") && text.contains("G97 S10000 M03\n"), "rpm never scales: {text}");
}

// ------------------------------------------------------- synthetic

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

fn section(pre: Vec<cnc::Record>, body: Vec<cnc::Record>) -> String {
    let mut r = pre;
    r.push(cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) });
    r.push(cnc::Record::Tool { number: 1, name: "TN-1600-0R8".into() });
    r.push(cnc::Record::Section { begin: true });
    r.extend(body);
    r.push(cnc::Record::Section { begin: false });
    cnc::serialize(&r)
}

fn run(stream: &str, u: &str) -> engine::PostOutput {
    engine::run_with(CPS, &lathe_mch(), stream, &json!({ "Units": u })).expect("the script runs")
}

#[test]
fn surface_speed_is_feet_per_minute_and_rpm_never_scales() {
    let css = vec![cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) }];
    let s = section(css, vec![mv(MoveKind::Rapid, 0.0, 6.0, 10.0, None), mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0))]);
    let mm = run(&s, "mm");
    let inch = run(&s, "inch");
    assert!(mm.warnings.is_empty() && inch.warnings.is_empty(), "{:?} {:?}", mm.warnings, inch.warnings);
    assert!(mm.program.contains("G96 S200 M03\n"), "{}", mm.program);
    assert!(inch.program.contains("G96 S656 M03\n"), "200 m/min is 656 ft/min: {}", inch.program);
    assert!(inch.program.contains("G50 S12000\n"), "{}", inch.program);
    // 100 mm/min at 1000 rpm: 0.1 mm/rev = 0.0039 in/rev; X12 -> X0.4724.
    assert!(inch.program.contains("G01 X0.4724 Z0. F0.0039\n"), "{}", inch.program);
    let s = section(vec![], vec![mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(100.0))]);
    assert!(run(&s, "inch").program.contains("G97 S1000 M03\n"));
}

#[test]
fn a_peck_counts_the_inch_increment() {
    let peck = cnc::Record::Cycle(cnc::CycleRec {
        kind: CycleKind::Deep, x: 0.0, y: 0.0, r_z: 22.0, bottom_z: 10.0, entry_z: 20.0, peck: 1.0, dwell: 0.5,
        first_move: 1, feed: Some(100.0),
    });
    // The approach to R first: a cycle that is its section's first motion
    // cannot be seated (#1026) and is refused.
    let s = section(vec![], vec![mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None), peck, mv(MoveKind::Rapid, 0.0, 0.0, 22.0, None)]);
    let (mm, inch) = (run(&s, "mm"), run(&s, "inch"));
    assert!(mm.program.contains("G83 Z10. Q1000 P500 F0.1\n"), "{}", mm.program);
    // 1 mm = 394 ten-thousandths; the 500 ms dwell is unchanged.
    assert!(inch.program.contains("G83 Z0.3937 Q394 P500 F0.0039\n"), "{}", inch.program);
}

#[test]
fn what_inch_cannot_say_soundly_is_refused_by_name() {
    // A feed per rev under 0.0001 in/rev would post F0.
    let s = section(vec![], vec![mv(MoveKind::Feed, 0.0, 6.0, 0.0, Some(0.1))]);
    assert!(run(&s, "mm").program.contains("F0.0001"), "0.0001 mm/rev is a word in mm");
    let inch = run(&s, "inch");
    assert!(inch.warnings.iter().any(|w| w.contains("least increment")), "{:?}", inch.warnings);
    assert!(!inch.program.contains("F0\n") && !inch.program.contains("F0 "), "{}", inch.program);
    // A C-axis live-tool section: refused on an inch program, by name.
    let live = vec![cnc::Record::Param { name: "LiveTool".into(), value: json!(true) }];
    let s = section(live, vec![mv(MoveKind::Rapid, 10.0, 0.0, 5.0, None)]);
    let out = run(&s, "inch");
    assert!(out.warnings.iter().any(|w| w.contains("C-axis") && w.contains("inch")), "{:?}", out.warnings);
    assert!(!out.program.contains("G98") && !out.program.contains(" C"), "{}", out.program);
}

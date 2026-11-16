//! #671: what the PathPilot plasma post refuses by name - a non-plasma
//! op, a Plasma Profile path with no plasma record, a spindle rpm, a
//! rotary, a coolant ask, a Z cutting move, a canned cycle, a machine
//! that is not a plasma table, a feed with the torch off, a lit move with
//! no feed, an arc outside XY, G41/G42 left open at TORCH OFF, engaged on
//! an arc or still pending, wear compensation - and what the engine refuses for any
//! post (a torch with no header, an ON while lit, a torch never put
//! out, a rapid while lit, a mill post handed torch records); and the
//! registry: offered for the 1300PL, never for a mill, and the mills'
//! posts decline the 1300PL.
//! Provenance: our own tests over our own fixtures; the refusals are
//! the PathPilot Post Processor Guidelines' plasma column and UM10720
//! §7 as `assets/posts/tormach-plasma.cps` cites them.
//!
//! Gate map (#671): covers post::cnc_torch (the torch law), post::
//! engine, app::posting (PLASMA_POSTS, post_serves, default_post_for),
//! the tormach-plasma.cps script.

use camcraft_app::machine::Kind;
use camcraft_app::post::engine;
use camcraft_app::posting::{self, BUILTINS, LATHE_POSTS, PLASMA_POSTS};
use camcraft_app::schema::PostRef;
use serde_json::Value;
use std::path::{Path, PathBuf};

const PLASMA: &str = "Tormach PathPilot Plasma";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn script(name: &str) -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("registered")
}

fn pl1300() -> Value {
    serde_json::from_str(&read("assets/machines/tormach-1300pl.mch")).expect("json")
}

fn warnings(cps: &str, mch: &Value, stream: &str) -> Vec<String> {
    engine::run(cps, mch, stream).expect("the engine runs; the post refuses").warnings
}

fn refused(stream: &str, why: &str) {
    let w = warnings(script(PLASMA), &pl1300(), stream);
    assert!(w.iter().any(|m| m.contains(why)), "want {why:?}, got {w:?}");
}

/// A one-contour plasma section, with `extra` PARAMs ahead of it and
/// `cut` as its lit records.
fn section(extra: &str, header: &str, cut: &str) -> String {
    format!(
        "; HSMW-CNC v1\nPARAM OperationName Plasma1\nPARAM OperationKind Plasma Profile\n{extra}TOOL 1 PT-KERF-150\n\
         {header}SECTION\nRapid X 0 Y 0 Z 6000\nTORCH ON C0 OUTER\n{cut}TORCH OFF C0\nRapid X 10000 Y 0 Z 15000\nENDSECTION\n"
    )
}

const CUT: &str = "Feed X 10000 Y 0 Z 6000 F 2500000\nFeed X 10000 Y 10000 Z 6000 F 2500000\n";
const HEAD: &str = "PLASMA KERF1500\n";
const CONTROL: &str = "PLASMA KERF1500 CONTROL\n";

#[test]
fn a_clean_section_posts_and_every_mill_golden_is_refused_by_name() {
    assert!(warnings(script(PLASMA), &pl1300(), &section("", HEAD, CUT)).is_empty());
    for g in ["face_square_40x40", "contour_plate_with_hole", "drill_plate", "tap_plate"] {
        let w = warnings(script(PLASMA), &pl1300(), &read(&format!("crates/app/tests/golden/{g}.cl")));
        assert!(w.iter().any(|m| m.contains("is not plasma cutting")), "{g}: {w:?}");
    }
}

#[test]
fn the_post_refuses_what_a_plasma_table_cannot_say() {
    // A Plasma Profile whose path lost its record, and a mill op.
    let bare = |kind: &str| {
        format!("; HSMW-CNC v1\nPARAM OperationName Plasma1\nPARAM OperationKind {kind}\nSECTION\nRapid X 0 Y 0 Z 6000\n{CUT}ENDSECTION\n")
    };
    refused(&bare("Plasma Profile"), "operation Plasma1: the Plasma Profile path carries no plasma record");
    refused(&bare("2D Contour"), "operation Plasma1: a 2D Contour operation is not plasma cutting");
    // A spindle, a coolant, a tilt, a Z cutting move, a cycle.
    refused(&section("PARAM SpindleSpeed 3000\n", HEAD, CUT), "a plasma torch turns no spindle");
    refused(&section("PARAM Coolant mist\n", HEAD, CUT), "a plasma table has none");
    refused(&section("", HEAD, "Feed X 10000 Y 0 Z 6000 F 2500000 A 10000 B 0 C 0\n"), "a tilted move");
    refused(&section("", HEAD, "Feed X 10000 Y 0 Z 4000 F 2500000\n"), "the control owns the torch height");
    refused(&section("", HEAD, &format!("{CUT}CYCLE DRILL X 0 Y 0 Z -1000 R 2000\n")), "a canned cycle");
    // A G41/G42 span under a computer offset.
    refused(&section("", HEAD, &format!("COMP LEFT 750\n{CUT}COMP OFF\n")), "the stream disagrees with its own plasma header");
    // A feed outside any TORCH span: the engine's law does not see it,
    // the script is the only guard against cutting with the torch off.
    refused(
        &section("", HEAD, CUT).replace("TORCH OFF C0\n", "TORCH OFF C0\nFeed X 0 Y 0 Z 6000 F 2500000\n"),
        "a cutting move with the torch off",
    );
    // A lit move with no feed while the op states one (no AUTOFEED).
    refused(&section("", HEAD, "Feed X 10000 Y 0 Z 6000\n"), "a lit move states no feed while the op states one");
    // An arc outside XY (I K names the XZ plane).
    refused(
        &section("", HEAD, "Arc CCW X 10000 Y 0 Z 6000 F 2500000 I 5000 K 6000\n"),
        "a plasma cut is flat",
    );
    // Under control compensation: the torch put out with G41/G42 still
    // in force, an engage on an arc, and wear compensation.
    refused(
        &section("", CONTROL, &format!("COMP LEFT 750\n{CUT}")),
        "the torch goes out under G41/G42 - the compensation was never cancelled",
    );
    refused(
        &section(
            "",
            CONTROL,
            &format!("COMP LEFT 750\nArc CCW X 10000 Y 0 Z 6000 F 2500000 I 5000 J 0\n{CUT}COMP OFF\nFeed X 9000 Y 10000 Z 6000 F 2500000\n"),
        ),
        "G41/G42 engages or cancels on an arc",
    );
    refused(
        &section("", CONTROL, &format!("COMP LEFT 750 WEAR\n{CUT}COMP OFF\nFeed X 9000 Y 10000 Z 6000 F 2500000\n")),
        "wear compensation is not a plasma mode",
    );
    // The machine: a mill, or a plasma table that grew a rotary.
    let vmc: Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("json");
    let w = warnings(script(PLASMA), &vmc, &section("", HEAD, CUT));
    assert!(w.iter().any(|m| m.contains("is not declared a plasma machine")), "{w:?}");
    // A document with no machine: the sentence must read, not double-negate
    // ("no machine is not declared ...") - #998.
    let w = warnings(script(PLASMA), &Value::Null, &section("", HEAD, CUT));
    assert!(
        w.iter().any(|m| m.contains("no machine is stated, so none is declared a plasma machine")),
        "{w:?}"
    );
    assert!(!w.iter().any(|m| m.contains("no machine is not declared")), "{w:?}");
    let mut rot = pl1300();
    // (The mirror's door: the parsed 1300PL itself refuses a rotary.)
    rot["kinematics"]["static"] = serde_json::json!({ "rotaries": [{ "letter": "A", "direction": [1, 0, 0] }] });
    let w = warnings(script(PLASMA), &rot, &section("", HEAD, CUT));
    assert!(w.iter().any(|m| m.contains("states a rotary axis")), "{w:?}");
}

#[test]
fn the_engine_refuses_a_torch_stream_that_cannot_mean_what_it_says() {
    // A torch with no header in force (the header-less stream).
    refused(&section("", "", CUT), "TORCH C0 with no PLASMA header in force");
    // Struck twice, put out under another name, never put out.
    refused(&section("", HEAD, &format!("TORCH ON C1 INNER\n{CUT}")), "C1 is struck while C0 is still lit");
    refused(&section("", HEAD, CUT).replace("TORCH OFF C0", "TORCH OFF C7"), "TORCH OFF C7 but C0 is the lit contour");
    refused(&section("", HEAD, CUT).replace("TORCH OFF C0\n", ""), "C0 is never put out");
    // A rapid while lit.
    refused(&section("", HEAD, &format!("{CUT}Rapid X 0 Y 0 Z 6000\n")), "a rapid move while C0 is lit");
    // A second header in one section.
    refused(&section("", "PLASMA KERF1500\nPLASMA KERF1500\n", CUT), "a second PLASMA header");
}

#[test]
fn a_pending_kerf_engage_never_outlives_its_torch_span() {
    // COMP LEFT read after the last lit straight move: no G1 carried
    // the G41.1, so it is refused by name at TORCH OFF - and dropped, so
    // the next contour's first G1 never takes it.
    let stream = section("", CONTROL, &format!("{CUT}COMP LEFT 750\n")).replace(
        "ENDSECTION\n",
        "TORCH ON C1 OUTER\nFeed X 20000 Y 0 Z 6000 F 2500000\nFeed X 20000 Y 10000 Z 6000 F 2500000\nTORCH OFF C1\nENDSECTION\n",
    );
    let out = engine::run(script(PLASMA), &pl1300(), &stream).expect("the engine runs; the post refuses");
    assert!(
        out.warnings.iter().any(|m| m.contains("a G41/G42 engage no lit straight move carried")),
        "{:?}",
        out.warnings
    );
    assert!(!out.program.contains("G41.1"), "the pending engage leaked onto C1:\n{}", out.program);
    // A cancel with its lead-out posts clean, and the engage is written.
    let clean = section("", CONTROL, &format!("COMP LEFT 750\n{CUT}COMP OFF\nFeed X 9000 Y 10000 Z 6000 F 2500000\n"));
    let out = engine::run(script(PLASMA), &pl1300(), &clean).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("G41.1 D"), "{}", out.program);
}

#[test]
fn a_small_hole_factor_out_of_range_names_the_factor_range() {
    let e = engine::run(script(PLASMA), &pl1300(), &section("", HEAD, CUT).replace("TORCH ON C0 OUTER", "TORCH ON C0 OUTER SMALL1500"))
        .map(|o| o.warnings.join("; "))
        .unwrap_or_else(|e| e.to_string());
    assert!(e.contains("a feed factor in (0, 1]"), "{e}");
}

#[test]
fn the_mills_posts_decline_a_plasma_stream_and_the_1300pl() {
    let stream = read("crates/app/tests/golden/plasma_plate_with_holes.cl");
    for (n, cps) in BUILTINS.iter().filter(|(n, _)| *n != PLASMA) {
        // #542: the sinker post refuses any machine not declared a
        // sinker by name (the app's serves door, #1232, refuses first).
        if posting::SINKER_POSTS.contains(n) {
            let w = warnings(cps, &pl1300(), &stream);
            assert!(w.iter().any(|m| m.contains("is not declared a sinker EDM")), "{n}: {w:?}");
            assert!(!posting::post_serves(n, Kind::Plasma), "{n} must not serve a plasma table");
            continue;
        }
        let w = warnings(cps, &pl1300(), &stream);
        assert!(w.iter().any(|m| m.contains("does not define onTorchOn / onTorchOff")), "{n}: {w:?}");
        assert!(!posting::post_serves(n, Kind::Plasma), "{n} must not serve a plasma table");
    }
}

#[test]
fn the_registry_offers_the_plasma_post_for_the_1300pl_only() {
    assert_eq!(PLASMA_POSTS, &[PLASMA]);
    assert!(BUILTINS.iter().any(|(n, _)| *n == PLASMA));
    assert!(posting::post_serves(PLASMA, Kind::Plasma));
    for k in [Kind::Mill, Kind::Lathe, Kind::MillTurn] {
        assert!(!posting::post_serves(PLASMA, k), "{k:?}");
    }
    assert!(!LATHE_POSTS.contains(&PLASMA));
    assert_eq!(posting::default_post_for(Kind::Plasma), PostRef::Named(PLASMA.into()));
    assert_eq!(engine::extension_of(script(PLASMA)).expect("loads"), "nc");
}

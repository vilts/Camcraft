//! The Tormach post's safe Z retract (#495): `G53 G0 Z0` is "straight
//! up" only where machine Z0 is the top of travel (Tormach's PathPilot
//! convention). The machine file states its own height (the Z part's
//! `safe_retract`); every shipped Tormach file states 0 and posts
//! byte-identical to before, a machine stating another height posts
//! that height, and a non-Tormach machine stating none is refused by
//! name - never a guess that could rapid the spindle down into the part.
//! Provenance: our own tests over our own post and machine files.
//!
//! Gate map (M4.4): covers post::post (tormach.cps safe retract),
//! post::machine (safeRetractZ, model), machine::parse (safe_retract).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use serde_json::{json, Value};
use std::path::Path;

const STREAM: &str = "PARAM SpindleSpeed 2000\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                      PARAM SpindleSpeed 2000\nTOOL 2 T2\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn cps() -> String {
    read("assets/posts/tormach.cps")
}

/// A machine file in the file form, its Z part carrying `z_extra` keys.
fn file_form(model: &str, z_extra: Value) -> String {
    let mut z = json!({ "id": "col", "name": "Z", "type": "linear", "travel": { "min": -300.0, "max": 0.0 } });
    if let (Some(o), Some(e)) = (z.as_object_mut(), z_extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    json!({
        "general": { "model": model, "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": {
            "units": { "length": "mm", "angle": "degrees" },
            "parts": [{ "id": "base", "name": "table", "type": "table", "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 } },
                { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 } },
                z
            ] }]
        } }
    })
    .to_string()
}

fn mounted(text: &str) -> Value {
    machine::mirror(&parse_mch(text).unwrap_or_else(|e| panic!("parses: {e}")))
}

/// Every shipped Tormach file (and the generic VMCs) states Z0 as its
/// safe retract, and a Tormach program is byte-identical with or without
/// the statement: the mirror minus `safeRetractZ` is the pre-#495 input.
#[test]
fn the_shipped_tormach_machines_post_byte_identical() {
    for (name, text) in MACHINES {
        let m = parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        // #669: the plasma table retracts with G30 under its own post
        // (#671), never the mill post's G53 - it states no safe_retract.
        if camcraft_app::machine::kind(Some(&m)) == camcraft_app::machine::Kind::Plasma {
            assert_eq!(m.safe_retract_z, None, "{name}: a plasma table states no G53 retract");
            continue;
        }
        // #611: a Tormach LATHE retracts in X/Z on its own lathe post
        // (#612), not the mill post's Z0 law judged here.
        if camcraft_app::machine::kind(Some(&m)) == camcraft_app::machine::Kind::Lathe {
            assert_eq!(m.safe_retract_z, None, "{name}: a lathe states no mill Z0 retract");
            continue;
        }
        let tormach = name.starts_with("Tormach");
        if tormach || ["VMC 3-axis", "Tilt-head 5-axis"].contains(name) {
            assert_eq!(m.safe_retract_z, Some(0.0), "{name} states Z0 as its safe retract");
        }
        if !tormach {
            continue;
        }
        let with = mounted(text);
        let mut without = with.clone();
        if let Some(o) = without.as_object_mut() {
            assert!(o.remove("safeRetractZ").is_some(), "{name}: the mirror carries it");
        }
        let a = engine::run(&cps(), &with, STREAM).expect("posts");
        let b = engine::run(&cps(), &without, STREAM).expect("posts");
        assert!(a.warnings.is_empty() && b.warnings.is_empty(), "{name}: {:?} {:?}", a.warnings, b.warnings);
        assert_eq!(a.program, b.program, "{name}: byte-identical");
        assert_eq!(a.program.matches("G53 G0 Z0\nG30\n").count(), 3, "{name}: two changes + the end\n{}", a.program);
    }
    // No machine at all, and the default VMC: today's retract.
    for m in [Value::Null, serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("json")] {
        let out = engine::run(&cps(), &m, STREAM).expect("posts");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert_eq!(out.program.matches("G53 G0 Z0\nG30\n").count(), 3, "{}", out.program);
    }
}

/// A machine homed with Z0 at the bottom states its own safe height: the
/// post retracts to it (in the machine's unit), without Tormach's G30, and
/// the header says where the retract goes.
#[test]
fn a_stated_safe_z_posts_that_z() {
    let text = file_form("LINUXCNC KNEE MILL", json!({ "safe_retract": 290.0 }));
    for m in [mounted(&text), serde_json::from_str(&text).expect("raw door")] {
        let out = engine::run(&cps(), &m, STREAM).expect("posts");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        let p = &out.program;
        assert_eq!(p.matches("G53 G0 Z290\n").count(), 3, "{p}");
        assert!(!p.contains("G53 G0 Z0"), "{p}");
        assert!(!p.lines().any(|l| l == "G30"), "no G30 off the Tormach convention\n{p}");
        assert!(p.contains("(SAFE RETRACT G53 Z290 - STATED BY THE MACHINE)"), "{p}");
    }
    // An inch file scales like every other length: 11.5 in is Z11.5 on G20.
    let inch = file_form("LINUXCNC KNEE MILL", json!({ "safe_retract": 11.5 }))
        .replace("\"length\":\"mm\"", "\"length\":\"inch\"");
    let m = parse_mch(&inch).expect("parses");
    assert_eq!(m.safe_retract_z, Some(11.5 * 25.4));
    let out = engine::run(&cps(), &machine::mirror(&m), STREAM).expect("posts");
    assert_eq!(out.program.matches("G53 G0 Z11.5\n").count(), 3, "{}", out.program);
}

/// A non-Tormach machine that states no safe Z is refused by name - the
/// post cannot know which way is up - through the app's mirror and the
/// CLI's raw file alike; a Tormach model stating none keeps Z0.
#[test]
fn a_non_tormach_machine_without_a_safe_z_is_refused_by_name() {
    let text = file_form("LINUXCNC KNEE MILL", json!({}));
    for m in [mounted(&text), serde_json::from_str(&text).expect("raw door")] {
        let out = engine::run(&cps(), &m, STREAM).expect("runs");
        assert_eq!(out.warnings.len(), 1, "refused once: {:?}", out.warnings);
        assert!(
            out.warnings[0].contains("machine LINUXCNC KNEE MILL does not state its safe Z retract")
                && out.warnings[0].contains("safe_retract"),
            "{:?}",
            out.warnings
        );
        assert!(!out.program.contains("G53"), "no guessed retract\n{}", out.program);
    }
    let own = file_form("Tormach 770MX (shop copy)", json!({}));
    let out = engine::run(&cps(), &mounted(&own), STREAM).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G53 G0 Z0\nG30\n").count(), 3, "{}", out.program);
}

/// A garbled or misplaced `safe_retract` refuses the file naming the axis.
#[test]
fn a_garbled_safe_retract_refuses_the_file() {
    let e = parse_mch(&file_form("M", json!({ "safe_retract": "top" }))).expect_err("refused");
    assert!(e.contains("axis Z safe_retract must be a number"), "{e}");
    let on_x = file_form("M", json!({})).replace(
        "\"name\":\"X\",",
        "\"name\":\"X\",\"safe_retract\":0.0,",
    );
    let e = parse_mch(&on_x).expect_err("refused");
    assert!(e.contains("axis X states safe_retract - only the linear Z axis states one"), "{e}");
    let null = parse_mch(&file_form("M", json!({ "safe_retract": null }))).expect("null is unstated");
    assert_eq!(null.safe_retract_z, None);
}

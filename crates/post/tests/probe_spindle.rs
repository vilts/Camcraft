//! #523: a touch probe is never spun. A section that states no spindle
//! speed (S0 - a probe tool; `Tool::is_sane` refuses rpm 0 on every
//! cutter) posts no spindle start on any built-in mill post: no S word,
//! no M3 / M03. Fanuc has gated it since #108; the other five
//! posts started the spindle unconditionally, harmless only while they
//! refused probe moves. The control case keeps every cutter's spindle.
//!
//! Provenance: touch-probe practice (a wireless probe is never rotated
//! under power); our own synthetic program.
//!
//! Gate map (M4.4): covers post::dump, post::engine and every mill post
//! script's spindle preamble (assets/posts/*.cps, lathe excluded).

use camcraft_post::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_post::geometry::Point3;
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::Path;

/// Every built-in mill post (the lathe post probes nothing).
const MILL_POSTS: &[&str] = &["fanuc", "haas", "tormach", "sinumerik", "heidenhain", "mazak"];

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

/// A plain descent and one feed: enough for a preamble to open.
fn program() -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 50.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 25.0), None);
    b.push(MoveKind::Feed, Point3::new(10.0, 10.0, 15.0), Some(300.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 10.0, 50.0), None);
    b.finish()
}

fn post(stem: &str, rpm: f64) -> String {
    let d = Dump {
        op_name: "Probe1",
        tool_number: 99,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm,
        clearance_z: 50.0,
        retract_z: 50.0,
        ..Dump::default()
    };
    let stream = cnc::serialize(&dump(&program(), &d));
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let cps = read(&format!("assets/posts/{stem}.cps"));
    engine::run(&cps, &mch, &stream).unwrap_or_else(|e| panic!("{stem}: must post: {e}")).program
}

/// The words that start or speed a spindle, wherever they sit in a block.
fn spindle_words(program: &str) -> Vec<String> {
    program
        .lines()
        .filter(|l| !l.trim_start().starts_with('('))
        .flat_map(str::split_whitespace)
        // A Heidenhain block ends in `*` (`S8000*`, `M3*`).
        .map(|w| w.trim_end_matches('*'))
        .filter(|w| {
            matches!(*w, "M3" | "M03")
                || (w.len() > 1 && w.starts_with('S') && w[1..].chars().all(|c| c.is_ascii_digit() || c == '.'))
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn a_section_with_no_spindle_speed_starts_no_spindle_on_any_mill_post() {
    for stem in MILL_POSTS {
        let out = post(stem, 0.0);
        let words = spindle_words(&out);
        assert!(words.is_empty(), "{stem}: a probe section posts {words:?}:\n{out}");
    }
}

#[test]
fn a_cutting_section_still_starts_its_spindle_on_every_mill_post() {
    for stem in MILL_POSTS {
        let out = post(stem, 8000.0);
        let words = spindle_words(&out);
        assert!(words.iter().any(|w| w.starts_with('S')), "{stem}: no S word in {words:?}:\n{out}");
        assert!(
            words.iter().any(|w| matches!(w.as_str(), "M3" | "M03")),
            "{stem}: no spindle start in {words:?}:\n{out}"
        );
    }
}

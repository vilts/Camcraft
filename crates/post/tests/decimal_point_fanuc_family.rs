//! #1836: the Fanuc-family posts write every whole-number length with its
//! decimal point (`Z14.`, never `Z14`). Fanuc parameter 3401#0 (DPI, the
//! published Series 0i parameter manuals) makes a control read a word with
//! no point in least increments when DPI=0: `Z14` is then 0.014 mm, and a
//! rapid to it goes almost to Z0. With the point, DPI=0 and DPI=1 read the
//! same value. The words the control documents as "no decimal point"
//! (lathe G76 P Q, G83 Q) stay whole least increments.
//! Provenance: our own tests over our own posts and stream format.
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, assets/posts/haas.cps,
//! assets/posts/mazak.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/fanuc-lathe.cps, camcraft_post::post::engine.
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_post::cl::MoveKind;
use camcraft_post::machine::{Coolant, Machine};
use camcraft_post::post::{cnc, engine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MILL_POSTS: &[&str] = &["fanuc", "haas", "mazak", "doosan-dnm"];

/// The address letters whose value is a length or an angle on these
/// controls - each takes a decimal point.
const LENGTH_WORDS: &[char] = &['X', 'Y', 'Z', 'I', 'J', 'K', 'R', 'A', 'B', 'C', 'U', 'W'];

fn cps(stem: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts").join(format!("{stem}.cps"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Every motion-block word of `program` that is a length address with a
/// value and no decimal point, for example `Z14` or `X-30`.
fn bare_lengths(program: &str) -> Vec<String> {
    let mut bad = Vec::new();
    for line in program.lines() {
        let t = line.trim();
        if t.starts_with('(') || t.starts_with(';') || t.starts_with('%') || t.starts_with('#') || t.contains("IF[") {
            continue;
        }
        let code = t.split('(').next().unwrap_or("");
        let words: Vec<&str> = code.split_whitespace().collect();
        let motion = words.iter().any(|w| matches!(*w, "G00" | "G01" | "G02" | "G03" | "G0" | "G1" | "G2" | "G3"));
        if !motion {
            continue;
        }
        for w in words {
            let mut cs = w.chars();
            let Some(letter) = cs.next() else { continue };
            let rest: String = cs.collect();
            let numeric = !rest.is_empty() && rest.trim_start_matches('-').chars().all(|c| c.is_ascii_digit());
            if LENGTH_WORDS.contains(&letter) && numeric {
                bad.push(format!("{w} in `{t}`"));
            }
        }
    }
    bad
}

/// A mill section whose positions are all whole millimetres.
fn mill_stream() -> String {
    "PARAM SpindleSpeed 6000
PARAM ClearancePlane 50
PARAM RetractPlane 25
TOOL 1 10MM END MILL
SECTION
Rapid 30000 30000 50000
Rapid 30000 30000 14000
Feed 30000 30000 -8000 F300000
Feed 60000 30000 -8000 F300000
Rapid 60000 30000 50000
ENDSECTION
"
    .to_string()
}

#[test]
fn the_mill_posts_write_a_whole_number_length_with_its_point() {
    for stem in MILL_POSTS {
        let out = engine::run(&cps(stem), &Value::Null, &mill_stream()).unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert!(out.program.contains("Z14."), "{stem}: the rapid to 14 mm keeps its point\n{}", out.program);
        assert!(out.program.contains("Z-8."), "{stem}: a negative whole length keeps its point\n{}", out.program);
        assert!(out.program.contains("X30."), "{stem}: an X word keeps its point\n{}", out.program);
        let bad = bare_lengths(&out.program);
        assert!(bad.is_empty(), "{stem}: length words with no decimal point: {bad:?}\n{}", out.program);
    }
}

#[test]
fn the_mill_posts_write_a_whole_number_inch_length_with_its_point() {
    // 25.4 mm is 1 inch: `Z1.`, never `Z1` (0.0001 in under DPI=0).
    let s = "PARAM SpindleSpeed 6000
PARAM ClearancePlane 50.8
TOOL 1 10MM END MILL
SECTION
Rapid 25400 25400 50800
Rapid 25400 25400 25400
Feed 25400 25400 -25400 F254000
ENDSECTION
";
    for stem in MILL_POSTS {
        let out = engine::run_with(&cps(stem), &Value::Null, s, &json!({ "Units": "inch" }))
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert!(out.program.contains("G20"), "{stem}: an inch program\n{}", out.program);
        assert!(out.program.contains("Z1.") && out.program.contains("Z-1."), "{stem}\n{}", out.program);
        let bad = bare_lengths(&out.program);
        assert!(bad.is_empty(), "{stem}: length words with no decimal point: {bad:?}\n{}", out.program);
    }
}

fn lathe_mch() -> Value {
    camcraft_post::post::machine::mirror(&Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    })
}

fn mv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

#[test]
fn the_lathe_post_writes_a_whole_number_length_and_feed_per_rev_with_its_point() {
    let r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Tool { number: 1, name: "TN-1600-0R8".into() },
        cnc::Record::Section { begin: true },
        mv(MoveKind::Rapid, 15.0, 14.0, None),
        // 1000 mm/min at 1000 rpm is exactly 1 mm/rev: `F1.`, not `F1`.
        mv(MoveKind::Feed, 15.0, -8.0, Some(1000.0)),
        mv(MoveKind::Feed, 20.0, -8.0, Some(1000.0)),
        mv(MoveKind::Rapid, 20.0, 14.0, None),
        cnc::Record::Section { begin: false },
    ];
    let out = engine::run(&cps("fanuc-lathe"), &lathe_mch(), &cnc::serialize(&r)).unwrap_or_else(|e| panic!("{e}"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("X30. Z14."), "a diameter and a Z keep their points\n{}", out.program);
    assert!(out.program.contains("Z-8."), "{}", out.program);
    assert!(out.program.contains("F1."), "a whole feed per rev keeps its point\n{}", out.program);
    let bad = bare_lengths(&out.program);
    assert!(bad.is_empty(), "length words with no decimal point: {bad:?}\n{}", out.program);
}

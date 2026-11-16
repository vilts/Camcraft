//! M11.2 B, the posting half: the arc entry's TRUE vertical-plane arc
//! (one XZ arc per level for a first cut along X, one YZ arc for one
//! along Y) posts with its plane stated by the G-word posts, and
//! Heidenhain rides it as chords and never switches plane; an oblique
//! first cut's chord fan states no vertical plane anywhere.
//!
//! The paths are recorded (#926): `recorded/inputs/vertical_arc_*.cl`,
//! written and guarded by motion's `vertical_arc_entry`, which also holds
//! the generator half (true arcs, tangency, the oblique note, verify).
//!
//! Provenance: RS-274/NGC and ISO 6983 G17/G18/G19 plane selection and
//! G2/G3 sense (public standards); our own posts.
//!
//! Gate map (M11.2): covers post::post, post::dump, cl::text, assets/posts.

mod recorded;

use camcraft_post::cl::{text, ArcPlane, Toolpath};
use camcraft_post::post::{cnc, dump, engine, Dump};
use std::path::{Path, PathBuf};

/// A recorded contour path of motion's vertical_arc_entry.
fn path(name: &str) -> Toolpath {
    text::from_text(&recorded::input(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

fn post(stem: &str, t: &Toolpath) -> String {
    let d = Dump {
        op_name: "Arc entry",
        tool_number: 1,
        tool_name: "8MM FLAT",
        tool_diameter: 8.0,
        rpm: 10000.0,
        clearance_z: 5.0,
        retract_z: 8.0,
        top_z: 0.0,
        ..Dump::default()
    };
    let s = cnc::serialize(&dump(t, &d));
    let mch: serde_json::Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine");
    let cps = read(&format!("assets/posts/{stem}.cps"));
    let out = engine::run(&cps, &mch, &s).unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    out.program
}

/// The G-word posts: the plane word is stated, the other vertical one
/// never, and exactly three arc blocks carry the plane's center words
/// (`I`/`K` for G18, `J`/`K` for G19) - one per level. Heidenhain
/// switches no plane.
fn assert_posts(t: &Toolpath, plane: ArcPlane) {
    let (word, other, centers) = match plane {
        ArcPlane::Xz => ("G18", "G19", ('I', 'K')),
        _ => ("G19", "G18", ('J', 'K')),
    };
    for stem in ["fanuc", "haas", "sinumerik", "mazak"] {
        let out = post(stem, t);
        let has = |l: &str, w: &str| l.split(' ').any(|x| x == w);
        assert!(out.lines().any(|l| has(l, word)), "{stem} states {word}:\n{out}");
        assert!(!out.lines().any(|l| has(l, other)), "{stem} never {other}:\n{out}");
        let blocks = out
            .lines()
            .filter(|l| {
                let w = |k: char| l.split(' ').any(|x| x.starts_with(k) && x.len() > 1 && x[1..].parse::<f64>().is_ok());
                w(centers.0) && w(centers.1)
            })
            .count();
        assert_eq!(blocks, 3, "{stem}: one {word} arc block per level:\n{out}");
    }
    let hh = post("heidenhain", t);
    assert!(!hh.contains("G18") && !hh.contains("G19"), "Heidenhain switches no plane:\n{hh}");
}

#[test]
fn an_x_aligned_first_cut_enters_on_one_g18_arc_per_level() {
    // The linear lead runs along the bore's top wall (+X): the entry
    // descends in the XZ plane through it.
    assert_posts(&path("vertical_arc_x.cl"), ArcPlane::Xz);
}

#[test]
fn a_y_aligned_first_cut_enters_on_one_g19_arc_per_level() {
    // The arc lead-in leaves its start along +Y: the entry descends in
    // the YZ plane onto it.
    assert_posts(&path("vertical_arc_y.cl"), ArcPlane::Yz);
}

#[test]
fn an_oblique_first_cut_stays_chords_and_says_so_once() {
    // The bore turned 30 degrees: the lead runs along no axis, so no
    // G17/G18/G19 names its vertical plane.
    let t = path("vertical_arc_oblique.cl");
    assert!(t.moves.iter().all(|m| m.arc.is_none_or(|a| a.plane.is_xy())), "no axis-plane arc for an oblique cut");
    for stem in ["fanuc", "haas", "sinumerik", "mazak", "heidenhain"] {
        let out = post(stem, &t);
        assert!(!out.contains("G18") && !out.contains("G19"), "{stem}: no vertical plane:\n{out}");
    }
}

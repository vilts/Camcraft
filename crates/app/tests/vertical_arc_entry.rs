//! M11.2 B: the arc entry rides a TRUE vertical-plane arc. A first cut
//! along X enters each level on one XZ (G18) arc, one along Y on one YZ
//! (G19) arc - tangent to the cut at the bottom, never split (the arc
//! ends at its circle's bottom, the Z extremum `push_arc_in` splits
//! at) - and the G-word posts state the plane; Heidenhain rides it as
//! chords and never switches plane. An oblique first cut has no
//! standard plane: it keeps the certified chord fan and the path says
//! so once. Every path verifies silent against its part.
//! Provenance: RS-274/NGC and ISO 6983 G17/G18/G19 plane selection and
//! G2/G3 sense (public standards); our own generators and posts.
//!
//! Gate map (M11.2): covers motion::machining2d (contour arc entry,
//! motion::entry_arc), cl::cl, post::post, sim::verify.

use camcraft_app::cl::{ArcPlane, Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::post::{cnc, dump, engine, Dump};
use camcraft_app::verify::{verify, Mode, ToolGeom, VerifyContext};
use std::path::{Path, PathBuf};

const OBLIQUE: &str = "entry: oblique vertical arc posted as chords - no standard plane";

/// A 40 x 30 rectangular bore turned `deg` about the origin.
fn bore(deg: f64) -> Region {
    let (s, c) = deg.to_radians().sin_cos();
    let pts = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]
        .iter()
        .map(|&(x, y)| Point2::new(x * c - y * s, x * s + y * c))
        .collect();
    Region::new(vec![Loop::new(pts).expect("loop")]).expect("region")
}

fn params(lead: Lead) -> ContourParams {
    ContourParams {
        tool_diameter: 8.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 8.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn vertical_arcs(t: &Toolpath) -> Vec<(usize, &Move)> {
    t.moves.iter().enumerate().filter(|(_, m)| m.arc.is_some_and(|a| !a.plane.is_xy())).collect()
}

/// Chords: feeds with no arc that drop while they travel in XY.
fn chords(t: &Toolpath) -> usize {
    t.moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Feed
                && m.arc.is_none()
                && m.to.z < m.from.z
                && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9
        })
        .count()
}

fn unit(x: f64, y: f64) -> (f64, f64) {
    let n = x.hypot(y);
    (x / n, y / n)
}

/// XY direction a move leaves its start in: a line's chord, an XY
/// arc's tangent.
fn leaves(m: &Move) -> (f64, f64) {
    match m.arc {
        Some(a) if a.plane.is_xy() => {
            let (rx, ry) = (m.from.x - a.center.x, m.from.y - a.center.y);
            if a.ccw { unit(-ry, rx) } else { unit(ry, -rx) }
        }
        _ => unit(m.to.x - m.from.x, m.to.y - m.from.y),
    }
}

/// One arc per level in `plane`, its center straight above its end
/// (tangent there - horizontal), the G-sense its travel asks, and the
/// next cut leaving the end the way the arc arrives. No chord left.
fn assert_true_entries(t: &Toolpath, plane: ArcPlane) {
    assert!(t.is_chained() && t.all_finite());
    assert!(t.notes.iter().all(|n| n != OBLIQUE), "{:?}", t.notes);
    assert_eq!(chords(t), 0, "no chord fan is left");
    let arcs = vertical_arcs(t);
    let ends: Vec<f64> = arcs.iter().map(|(_, m)| m.to.z).collect();
    assert_eq!(ends, [0.0, -2.0, -4.0], "one unsplit arc per level");
    for (i, m) in arcs {
        let a = m.arc.expect("arc");
        assert_eq!(a.plane, plane);
        // The center in 3-D: the word pair plus the end's normal coord.
        let (c, along) = match plane {
            ArcPlane::Xz => ((a.center.x, m.to.y, a.center.y), m.to.x - m.from.x),
            _ => ((m.to.x, a.center.x, a.center.y), m.to.y - m.from.y),
        };
        assert!((c.0 - m.to.x).abs() <= 1e-6 && (c.1 - m.to.y).abs() <= 1e-6, "center above the end: {m:?}");
        assert!(m.from.x == m.to.x || m.from.y == m.to.y, "in the plane: {m:?}");
        let r = c.2 - m.to.z;
        let rs = (m.from.x - c.0).hypot(m.from.y - c.1).hypot(m.from.z - c.2);
        assert!(r > 0.0 && (rs - r).abs() <= 2e-6, "one circle, tangent at the bottom: {r} vs {rs}");
        // RS-274 from +normal: XZ's frame is (z, x), so travelling +X
        // at the bottom is G2; YZ's is (y, z), so travelling +Y is G3.
        let g3 = if plane == ArcPlane::Xz { along < 0.0 } else { along > 0.0 };
        assert_eq!(a.ccw, g3, "the sense: {m:?}");
        let d = unit(m.to.x - m.from.x, m.to.y - m.from.y);
        let n = leaves(&t.moves[i + 1]);
        assert!(d.0 * n.0 + d.1 * n.1 > 0.999_999, "tangent to the first cut: {d:?} vs {n:?}");
    }
}

fn verify_bore(t: &Toolpath, region: &Region) -> Vec<camcraft_app::verify::Warning> {
    let tool = ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 4.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
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
    let r = bore(0.0);
    let t = contour(&r, &params(Lead::Linear { length: 3.0 })).expect("contour");
    assert_true_entries(&t, ArcPlane::Xz);
    assert_eq!(verify_bore(&t, &r), Vec::new(), "the true arc verifies silent");
    assert_posts(&t, ArcPlane::Xz);
}

#[test]
fn a_y_aligned_first_cut_enters_on_one_g19_arc_per_level() {
    // The arc lead-in leaves its start along +Y: the entry descends in
    // the YZ plane onto it.
    let r = bore(0.0);
    let t = contour(&r, &params(Lead::Arc { radius: 3.0 })).expect("contour");
    assert_true_entries(&t, ArcPlane::Yz);
    assert_eq!(verify_bore(&t, &r), Vec::new(), "the true arc verifies silent");
    assert_posts(&t, ArcPlane::Yz);
}

#[test]
fn an_oblique_first_cut_stays_chords_and_says_so_once() {
    // The same bore turned 30 degrees: the lead runs along no axis, so
    // no G17/G18/G19 names its vertical plane.
    let r = bore(30.0);
    let t = contour(&r, &params(Lead::Linear { length: 3.0 })).expect("contour");
    assert!(t.is_chained() && t.all_finite());
    assert!(vertical_arcs(&t).is_empty(), "no axis-plane arc for an oblique cut");
    assert!(chords(&t) >= 3 * 3, "each level still descends on a chord fan");
    assert_eq!(t.notes.iter().filter(|n| *n == OBLIQUE).count(), 1, "said once: {:?}", t.notes);
    assert_eq!(verify_bore(&t, &r), Vec::new(), "the fan verifies silent");
    for stem in ["fanuc", "haas", "sinumerik", "mazak", "heidenhain"] {
        let out = post(stem, &t);
        assert!(!out.contains("G18") && !out.contains("G19"), "{stem}: no vertical plane:\n{out}");
    }
}

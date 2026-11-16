//! M11.2 B: the arc entry rides a TRUE vertical-plane arc. A first cut
//! along X enters each level on one XZ (G18) arc, one along Y on one YZ
//! (G19) arc - tangent to the cut at the bottom, never split (the arc
//! ends at its circle's bottom, the Z extremum `push_arc_in` splits
//! at). An oblique first cut has no standard plane: it keeps the
//! certified chord fan and the path says so once. Every path verifies
//! silent against its part.
//!
//! The posting half (the G-word posts state the plane; Heidenhain rides
//! it as chords) is post's `vertical_arc_entry`, which reads these paths
//! as recorded `.cl` files (#926); each test here also guards its path's
//! recording (`crates/post/tests/recorded/inputs/vertical_arc_*.cl`), so a
//! generator change cannot leave the post suite posting a stale path.
//! Regenerate after an intentional change, then re-run the post suite:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-motion --test vertical_arc_entry`
//!
//! Provenance: RS-274/NGC and ISO 6983 G17/G18/G19 plane selection and
//! G2/G3 sense (public standards); our own generators.
//!
//! Gate map (M11.2): covers motion::machining2d (contour arc entry,
//! motion::entry_arc), cl::cl, cl::text, sim::verify.

use camcraft_motion::cl::{ArcPlane, Move, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_motion::machining3d::Entry3D;
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext};
use std::path::Path;

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
        links: camcraft_motion::links::Links::PLANAR,
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

fn verify_bore(t: &Toolpath, region: &Region) -> Vec<camcraft_motion::verify::Warning> {
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

/// Compare `t` (as `cl::text`) with its recording, or write it with
/// WRITE_GOLDEN set: the post suite's input stays what this generator
/// writes today.
fn guard_recording(name: &str, t: &Toolpath) {
    let text = camcraft_motion::cl::text::to_text(t).unwrap_or_else(|e| panic!("{name}: {e}"));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../post/tests/recorded/inputs").join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        want.replace("\r\n", "\n"),
        text,
        "recorded path {} is stale: regenerate with WRITE_GOLDEN=1, then re-run post's vertical_arc_entry",
        path.display()
    );
}

#[test]
fn an_x_aligned_first_cut_enters_on_one_g18_arc_per_level() {
    // The linear lead runs along the bore's top wall (+X): the entry
    // descends in the XZ plane through it.
    let r = bore(0.0);
    let t = contour(&r, &params(Lead::Linear { length: 3.0 })).expect("contour");
    assert_true_entries(&t, ArcPlane::Xz);
    assert_eq!(verify_bore(&t, &r), Vec::new(), "the true arc verifies silent");
    guard_recording("vertical_arc_x.cl", &t);
}

#[test]
fn a_y_aligned_first_cut_enters_on_one_g19_arc_per_level() {
    // The arc lead-in leaves its start along +Y: the entry descends in
    // the YZ plane onto it.
    let r = bore(0.0);
    let t = contour(&r, &params(Lead::Arc { radius: 3.0 })).expect("contour");
    assert_true_entries(&t, ArcPlane::Yz);
    assert_eq!(verify_bore(&t, &r), Vec::new(), "the true arc verifies silent");
    guard_recording("vertical_arc_y.cl", &t);
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
    guard_recording("vertical_arc_oblique.cl", &t);
}

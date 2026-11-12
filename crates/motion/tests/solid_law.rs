//! The verifier's solid law (b2) against what plane sections cannot see:
//! a pocket ladder plunging through a 1 mm blind floor that no level
//! slices, a contour ring walked under a slab hanging over it, and a
//! cavity ring walked under an undercut lip. Each path is the flat 2D
//! generator's, unaware of the part; each is first shown silent under
//! the per-plane law the verifier read before (a replica of the old
//! model law (b) and containment law, both reading only
//! `slice::material_above` at the move's deepest plane), then convicted
//! by `verify` - and only at the moves that really enter the solid, as
//! an analytic check of each fixture's boxes says.
//!
//! Provenance: our own fixtures and design (the shadowed-islands follow-
//! up), specced from the Prime Directive; no vendor material.
//!
//! Gate map (solid law): covers camcraft_sim::verify (engine b2, solid,
//! solid_index, swept, probe), camcraft_motion::machining2d (pocket,
//! contour), camcraft_geometry::slice.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::slice;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{ContourParams, Lead, PocketParams, PocketStrategy, Side, contour, pocket};
use camcraft_motion::machining3d::Entry3D;
use camcraft_motion::verify::{Cutter, Mode, SolidLaw, ToolGeom, VerifyContext, Warning, solid_hits, verify};

const R: f64 = 3.0;
const TOL: f64 = 0.01;

/// An axis-aligned box, outward-wound.
fn boxf(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

type Boxes = Vec<([f64; 3], [f64; 3])>;

fn mesh(b: &Boxes) -> Vec<Triangle> {
    b.iter().flat_map(|(lo, hi)| boxf(*lo, *hi)).collect()
}

/// Walls over a ccw loop from z0 to z1, facing out of the loop when
/// `out` (material inside), into it otherwise (a hole's walls).
fn walls(l: &[(f64, f64)], z0: f64, z1: f64, out: bool, f: &mut Vec<Triangle>) {
    let mut v: Vec<(f64, f64)> = l.to_vec();
    if !out {
        v.reverse();
    }
    for k in 0..v.len() {
        let (a, b) = (v[k], v[(k + 1) % v.len()]);
        let (a0, b0, b1, a1) = (Point3::new(a.0, a.1, z0), Point3::new(b.0, b.1, z0), Point3::new(b.0, b.1, z1), Point3::new(a.0, a.1, z1));
        f.push(Triangle::new(a0, b0, b1));
        f.push(Triangle::new(a0, b1, a1));
    }
}

/// A horizontal face: the ccw rectangle `o` less the ccw rectangle `i`
/// inside it (or all of `o`), facing up or down.
fn cap(o: [(f64, f64); 4], i: Option<[(f64, f64); 4]>, z: f64, up: bool, f: &mut Vec<Triangle>) {
    let p = |q: (f64, f64)| Point3::new(q.0, q.1, z);
    let mut push = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        f.push(if up { Triangle::new(p(a), p(b), p(c)) } else { Triangle::new(p(a), p(c), p(b)) });
    };
    match i {
        None => {
            push(o[0], o[1], o[2]);
            push(o[0], o[2], o[3]);
        }
        Some(i) => {
            for k in 0..4 {
                let n = (k + 1) % 4;
                push(o[k], o[n], i[n]);
                push(o[k], i[n], i[k]);
            }
        }
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> [(f64, f64); 4] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// Does a flat cutter of radius R (less `tol`), its stack standing up
/// from the tip for `len`, share volume with any box?
fn enters(b: &Boxes, q: Point3, len: f64, tol: f64) -> bool {
    b.iter().any(|(lo, hi)| {
        let d = (q.x.clamp(lo[0], hi[0]) - q.x).hypot(q.y.clamp(lo[1], hi[1]) - q.y);
        d < R - tol && q.z < hi[2] - tol && q.z + len > lo[2] + tol
    })
}

/// The moves the analytic check convicts (sampled finely).
fn entering_moves(t: &Toolpath, b: &Boxes, len: f64) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, m) in t.moves.iter().enumerate() {
        let hit = m.segments3(0.001).into_iter().any(|(a, c)| {
            let n = ((c.x - a.x).hypot(c.y - a.y).max((c.z - a.z).abs()) / 0.05).ceil().clamp(1.0, 1e5) as usize;
            (0..=n).any(|k| {
                let s = k as f64 / n as f64;
                enters(b, Point3::new(a.x + (c.x - a.x) * s, a.y + (c.y - a.y) * s, a.z + (c.z - a.z) * s), len, 2.0 * TOL)
            })
        });
        if hit {
            out.push(i);
        }
    }
    out
}

/// The verdict the verifier gave before the solid law, replicated: the
/// model law (b) - containment and tangency against the PLAIN section
/// at each cutting move's deepest plane, at its law points - and the
/// containment law sampled along the move at 0.25 mm. Plain sections
/// only; this is the blindness under test.
fn plain_section_gouges(t: &Toolpath, f: &[Triangle]) -> usize {
    let mut n = 0;
    for m in &t.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let Ok(s) = slice::material_above(f, m.from.z.min(m.to.z)) else {
            n += 1;
            continue;
        };
        let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        let steps = ((b.x - a.x).hypot(b.y - a.y) / 0.25).ceil().clamp(1.0, 4096.0) as usize;
        // Containment law: sampled along the move, centre inside.
        let mut along: Vec<Point2> = (0..=steps).map(|k| {
            let u = k as f64 / steps as f64;
            Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u)
        }).collect();
        // Model law (b): at the law points (the end, or the arc's chord
        // joints), centre inside or nearer than r - tol to a wall.
        let law: Vec<Point2> = match m.arc {
            None => vec![b],
            Some(_) => m.chords(TOL).into_iter().map(|(_, q)| q).collect(),
        };
        along.extend(m.chords(TOL).into_iter().map(|(_, q)| q));
        let near = |p: Point2| s.loops().iter().any(|l| {
            let v = l.points();
            (0..v.len()).any(|k| seg_dist(p, v[k], v[(k + 1) % v.len()]) < R - TOL)
        });
        if along.iter().any(|&p| s.contains(p)) || law.iter().any(|&p| s.contains(p) || near(p)) {
            n += 1;
        }
    }
    n
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let s = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (a.x + dx * s - p.x).hypot(a.y + dy * s - p.y)
}

fn tool() -> ToolGeom {
    ToolGeom { radius: R, nose_radius: 0.0, flute_len: 25.0, shank_radius: R, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0 }
}

/// The whole stack's length - what the analytic check stands up.
const STACK: f64 = 95.0;

fn law_verify(t: &Toolpath, region: &Region, f: &[Triangle], mode: Mode, top: f64) -> Vec<Warning> {
    let g = tool();
    verify(
        t,
        &VerifyContext {
            region,
            stock: None,
            model: Some(f),
            tool: &g,
            cutter: None,
            mode,
            stock_top_z: top,
            clearance_z: top + 10.0,
            stock_to_leave: 0.0,
            gouge_tol: TOL,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

fn gouge_moves(w: &[Warning]) -> Vec<usize> {
    let mut v: Vec<usize> = w.iter().filter_map(|w| match w {
        Warning::Gouge { at } | Warning::RapidInStock { at } => Some(*at),
        _ => None,
    }).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("loop")])
        .expect("region")
}

/// A table: a plate Z [9, 20] with a 20 x 20 pocket from its top down to
/// a 1 mm floor (Z 9..10), standing over four corner legs - air under the
/// floor. The solid as boxes (the analytic check) and as one manifold
/// shell per body (what the verifier reads).
fn table() -> (Boxes, Vec<Triangle>) {
    let mut b: Boxes = vec![
        ([-20.0, -20.0, 9.0], [-10.0, 20.0, 20.0]),
        ([10.0, -20.0, 9.0], [20.0, 20.0, 20.0]),
        ([-10.0, -20.0, 9.0], [10.0, -10.0, 20.0]),
        ([-10.0, 10.0, 9.0], [10.0, 20.0, 20.0]),
        ([-10.0, -10.0, 9.0], [10.0, 10.0, 10.0]),
    ];
    let (o, h) = (rect(-20.0, -20.0, 20.0, 20.0), rect(-10.0, -10.0, 10.0, 10.0));
    let mut f = Vec::new();
    walls(&o, 9.0, 20.0, true, &mut f);
    cap(o, None, 9.0, false, &mut f);
    cap(o, Some(h), 20.0, true, &mut f);
    walls(&h, 10.0, 20.0, false, &mut f);
    cap(h, None, 10.0, true, &mut f);
    for (x, y) in [(-20.0, -20.0), (14.0, -20.0), (-20.0, 14.0), (14.0, 14.0)] {
        let leg = ([x, y, 0.0], [x + 6.0, y + 6.0, 8.5]);
        b.push(leg);
        f.extend(boxf(leg.0, leg.1));
    }
    (b, f)
}

/// The verdict, before and after, on one path: silent under the plain
/// sections, convicted by the solid law exactly where the analytic
/// check says the stack enters a box.
fn old_silent_new_exact(t: &Toolpath, b: &Boxes, f: &[Triangle], region: &Region, mode: Mode, top: f64) -> Vec<usize> {
    assert_eq!(plain_section_gouges(t, f), 0, "the per-plane law is blind here");
    let truth = entering_moves(t, b, STACK);
    assert!(!truth.is_empty(), "the fixture must really be entered");
    let got = gouge_moves(&law_verify(t, region, f, mode, top));
    assert_eq!(got, truth, "the solid law convicts exactly the entering moves");
    got
}

#[test]
fn a_pocket_plunging_through_a_thin_blind_floor_is_a_gouge() {
    let (b, f) = table();
    let region = square(-10.0, -10.0, 10.0, 10.0);
    let p = PocketParams {
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 2.0 * R,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: None,
        top_z: 20.0,
        bottom_z: 2.0,
        stepdown: 4.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let t = pocket(&region, &p).expect("pocket");
    // No level plane slices the floor: 12 is above it, 8 under it.
    let levels: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).map(|m| m.to.z).collect();
    assert!(levels.iter().all(|&z| !(9.0..=10.0).contains(&z)), "{levels:?}");
    let got = old_silent_new_exact(&t, &b, &f, &region, Mode::Area, 20.0);
    assert!(got.iter().all(|&i| t.moves[i].from.z.min(t.moves[i].to.z) < 10.0), "only moves under the floor's top");
}

/// A post Z [0, 20] and, apart from it, a 0.5 mm slab hanging over the
/// +Y side of the ring around it at Z 13.1 - between the ring's levels,
/// so no plane cuts it.
fn post_and_slab() -> Boxes {
    vec![([0.0, -5.0, 0.0], [5.0, 5.0, 20.0]), ([-20.0, 6.0, 13.1], [30.0, 30.0, 13.6])]
}

#[test]
fn a_contour_ring_under_an_overhanging_slab_is_a_gouge() {
    let b = post_and_slab();
    let f = mesh(&b);
    assert!(slice::material_above(&f, 12.0).is_ok_and(|s| !s.contains(Point2::new(2.5, 8.0))), "the plain section misses the slab");
    let region = slice::slice(&f, 20.0 - 1e-3).expect("top section");
    let p = ContourParams {
        tool_diameter: 2.0 * R,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: 25.0,
        top_z: 20.0,
        bottom_z: 0.0,
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    };
    let t = contour(&region, &p).expect("contour");
    let got = old_silent_new_exact(&t, &b, &f, &region, Mode::Boundary, 20.0);
    assert!(got.iter().all(|&i| t.moves[i].to.z < 13.6), "the rings under the slab, and their plunges");
}

/// A block Z [0, 20] with a 24 x 24 cavity Z [5, 15] reached through a
/// 10 x 10 opening: the lip Z [15, 20] overhangs the cavity's ring.
fn undercut() -> (Boxes, Vec<Triangle>) {
    let b = vec![
        ([-20.0, -20.0, 0.0], [20.0, 20.0, 5.0]),
        ([-20.0, -20.0, 5.0], [-12.0, 20.0, 15.0]),
        ([12.0, -20.0, 5.0], [20.0, 20.0, 15.0]),
        ([-12.0, -20.0, 5.0], [12.0, -12.0, 15.0]),
        ([-12.0, 12.0, 5.0], [12.0, 20.0, 15.0]),
        ([-20.0, -20.0, 15.0], [-5.0, 20.0, 20.0]),
        ([5.0, -20.0, 15.0], [20.0, 20.0, 20.0]),
        ([-5.0, -20.0, 15.0], [5.0, -5.0, 20.0]),
        ([-5.0, 5.0, 15.0], [5.0, 20.0, 20.0]),
    ];
    let (o, c, h) = (rect(-20.0, -20.0, 20.0, 20.0), rect(-12.0, -12.0, 12.0, 12.0), rect(-5.0, -5.0, 5.0, 5.0));
    let mut f = Vec::new();
    walls(&o, 0.0, 20.0, true, &mut f);
    cap(o, None, 0.0, false, &mut f);
    cap(o, Some(h), 20.0, true, &mut f);
    walls(&h, 15.0, 20.0, false, &mut f);
    cap(c, Some(h), 15.0, false, &mut f);
    walls(&c, 5.0, 15.0, false, &mut f);
    cap(c, None, 5.0, true, &mut f);
    (b, f)
}

#[test]
fn a_cavity_ring_under_an_undercut_lip_is_a_gouge() {
    let (b, f) = undercut();
    // The cavity's own outline, walked at one level inside it.
    let region = square(-12.0, -12.0, 12.0, 12.0);
    let p = ContourParams {
        tool_diameter: 2.0 * R,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: 25.0,
        top_z: 14.0,
        bottom_z: 8.0,
        stepdown: 6.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    };
    let t = contour(&region, &p).expect("contour");
    let levels: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).map(|m| m.to.z).collect();
    assert!(levels.iter().all(|&z| z < 15.0), "every level under the lip: {levels:?}");
    old_silent_new_exact(&t, &b, &f, &region, Mode::Boundary, 20.0);
}

#[test]
fn the_tool_tip_shape_is_read_exactly() {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let tri = |a: Point3, b: Point3, c: Point3| Triangle::new(a, b, c);
    let at = |x: f64, z: f64| {
        let mut b = camcraft_motion::cl::PathBuilder::new(p(x, 0.0, 30.0));
        b.push(MoveKind::Rapid, p(x, 0.0, z + 5.0), None);
        b.push(MoveKind::Plunge, p(x, 0.0, z), Some(100.0));
        b.finish()
    };
    let law = SolidLaw { step: 0.25, allow: TOL, hole_walls: false };
    let ball = Cutter { radius: R, corner_r: R, tip_angle: 0.0, flute_len: 20.0, shank_radius: R, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0, body: None, neck: None };
    let flat = Cutter { corner_r: 0.0, ..ball };
    // A wall at x 3 over a floor at z 0, its foot broken by a 1 mm 45-
    // degree chamfer (x 2..3). Tip 0.05 over the floor on the axis x 0:
    // the square end's rim stands in the chamfer, the ball's dome clears
    // it by half a millimetre.
    let mut wall = mesh(&vec![([3.0, -10.0, 0.0], [10.0, 10.0, 10.0]), ([-10.0, -10.0, -5.0], [10.0, 10.0, 0.0])]);
    let (a, b, c) = (p(2.0, -10.0, 0.0), p(3.0, -10.0, 0.0), p(3.0, -10.0, 1.0));
    let (a2, b2, c2) = (p(2.0, 10.0, 0.0), p(3.0, 10.0, 0.0), p(3.0, 10.0, 1.0));
    wall.extend([
        tri(a, b, c), tri(a2, c2, b2),
        tri(a, c2, a2), tri(a, c, c2),
        tri(a, a2, b2), tri(a, b2, b),
        tri(b, b2, c2), tri(b, c2, c),
    ]);
    let t = at(0.0, 0.05);
    assert!(solid_hits(&t, &wall, &ball, &law).is_empty(), "the ball's dome clears the chamfer");
    assert!(!solid_hits(&t, &wall, &flat, &law).is_empty(), "the square end's rim is in it");
    // Sideways into the wall itself, both stacks are in steel.
    let t = at(0.1, 5.0);
    assert!(!solid_hits(&t, &wall, &ball, &law).is_empty());
    // A 90-degree V groove along Y, apex on the axis at z 0: a 90-degree
    // drill point stands in it exactly; a flat end at the same tip is in
    // its flanks; the point 0.1 under the apex is a gouge.
    let drill = Cutter { radius: R, corner_r: 0.0, tip_angle: 90.0, flute_len: 20.0, shank_radius: R, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0, body: None, neck: None };
    let s = 10.0;
    let v = vec![
        tri(p(-s, -10.0, s), p(0.0, -10.0, 0.0), p(0.0, 10.0, 0.0)),
        tri(p(-s, -10.0, s), p(0.0, 10.0, 0.0), p(-s, 10.0, s)),
        tri(p(0.0, -10.0, 0.0), p(s, -10.0, s), p(s, 10.0, s)),
        tri(p(0.0, -10.0, 0.0), p(s, 10.0, s), p(0.0, 10.0, 0.0)),
    ];
    assert!(solid_hits(&at(0.0, 0.0), &v, &drill, &law).is_empty(), "the cone fits its own V");
    assert!(!solid_hits(&at(0.0, 0.0), &v, &Cutter { tip_angle: 0.0, ..drill }, &law).is_empty(), "a flat end does not");
    assert!(!solid_hits(&at(0.0, -0.1), &v, &drill, &law).is_empty(), "the point under the apex is a gouge");
}

#[test]
fn degenerate_input_fails_closed_never_panics() {
    let law = SolidLaw { step: 0.25, allow: TOL, hole_walls: false };
    let c = Cutter { radius: R, corner_r: 0.0, tip_angle: 0.0, flute_len: 20.0, shank_radius: R, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0, body: None, neck: None };
    let mut b = camcraft_motion::cl::PathBuilder::new(Point3::new(50.0, 50.0, 30.0));
    b.push(MoveKind::Feed, Point3::new(60.0, 50.0, 30.0), Some(100.0));
    let t = b.finish();
    let good = mesh(&vec![([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])]);
    assert!(solid_hits(&t, &good, &c, &law).is_empty(), "far from the part");
    assert!(solid_hits(&t, &[], &c, &law).is_empty(), "no part, nothing to enter");
    let mut bad = good.clone();
    bad.push(Triangle::new(Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)));
    assert_eq!(solid_hits(&t, &bad, &c, &law), vec![Warning::Gouge { at: 0 }], "an unreadable mesh fails closed");
    let nonsense = Cutter { radius: f64::NAN, ..c };
    assert_eq!(solid_hits(&t, &good, &nonsense, &law), vec![Warning::Gouge { at: 0 }], "an unreadable tool fails closed");
    // Degenerate facets (zero area, a point) are read, not tripped over.
    let mut thin = good.clone();
    thin.push(Triangle::new(Point3::new(55.0, 50.0, 31.0), Point3::new(55.0, 50.0, 31.0), Point3::new(55.0, 50.0, 31.0)));
    thin.push(Triangle::new(Point3::new(54.0, 49.0, 31.0), Point3::new(56.0, 51.0, 31.0), Point3::new(55.0, 50.0, 31.0)));
    let w = solid_hits(&t, &thin, &c, &law);
    assert_eq!(w, vec![Warning::Gouge { at: 0 }], "a sliver inside the cutter is still steel");
}

#[test]
fn a_cutter_wholly_inside_the_solid_is_found_by_parity() {
    // A tall block the whole stack fits inside: no facet crosses the
    // cutter, the ray parity says steel.
    let f = mesh(&vec![([-100.0, -100.0, -100.0], [100.0, 100.0, 200.0])]);
    let law = SolidLaw { step: 0.25, allow: TOL, hole_walls: false };
    let c = Cutter { radius: R, corner_r: 0.0, tip_angle: 0.0, flute_len: 20.0, shank_radius: R, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0, body: None, neck: None };
    let mut b = camcraft_motion::cl::PathBuilder::new(Point3::new(0.0, 0.0, 0.0));
    b.push(MoveKind::Feed, Point3::new(10.0, 0.0, 0.0), Some(100.0));
    b.push(MoveKind::Rapid, Point3::new(10.0, 0.0, 300.0), None);
    b.push(MoveKind::Rapid, Point3::new(500.0, 0.0, 300.0), None);
    let w = solid_hits(&b.finish(), &f, &c, &law);
    assert_eq!(w, vec![Warning::Gouge { at: 0 }, Warning::RapidInStock { at: 1 }]);
}

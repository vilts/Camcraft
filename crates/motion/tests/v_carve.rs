//! The V-carve kernel's laws (#1443): the tool centre rides the medial
//! axis of the closed region, the depth at each point is the depth at
//! which the cone touches the walls, and the cone never cuts outside
//! the region at any depth. The sweep test reads every cutting move and
//! measures the cone's circle in the top plane against the outline.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d,
//! camcraft_motion::machining2d (v_carve).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{v_carve, GenError, VCarveParams};

const TOP: f64 = 0.0;

fn poly(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])]).unwrap()
}

fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Loop {
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    poly(&pts)
}

/// A 90 degree V-bit, 12 mm wide: the cut is as deep as it is wide on
/// each side, so the numbers below read off the drawing.
fn params() -> VCarveParams {
    VCarveParams {
        angle_deg: 90.0,
        tool_diameter: 12.0,
        clearance_z: 10.0,
        retract_z: Some(3.0),
        top_z: TOP,
        bottom_z: -6.0,
        stepdown: 6.0,
        feed: 600.0,
        plunge_feed: 200.0,
        flat_clear: false,
        flat_stepover: 0.25,
    }
}

fn dist_to_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if dd == 0.0 {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

fn dist_to_outline(region: &Region, p: Point2) -> f64 {
    let mut best = f64::INFINITY;
    for l in region.loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            best = best.min(dist_to_seg(p, pts[i], pts[(i + 1) % pts.len()]));
        }
    }
    best
}

/// Every point of every cutting move, at most 0.02 mm apart: (x, y, z).
fn cut_samples(path: &Toolpath) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    for m in &path.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let len = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2) + (m.to.z - m.from.z).powi(2)).sqrt();
        let n = (len / 0.02).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            out.push((
                m.from.x + t * (m.to.x - m.from.x),
                m.from.y + t * (m.to.y - m.from.y),
                m.from.z + t * (m.to.z - m.from.z),
            ));
        }
    }
    out
}

/// The safety law of the issue: sweep the cone along the path. At a tip
/// depth `d` under the top plane the cone's circle in that plane has the
/// radius `d * tan(angle / 2)`; it must lie inside the region. No
/// tolerance is given to the cutter: the kernel keeps its own allowance.
fn assert_cone_inside(region: &Region, path: &Toolpath, angle_deg: f64, top: f64) {
    let tan_half = (angle_deg.to_radians() * 0.5).tan();
    let mut cut = 0usize;
    for (x, y, z) in cut_samples(path) {
        let depth = top - z;
        if depth <= 1e-9 {
            continue;
        }
        cut += 1;
        let p = Point2::new(x, y);
        let r = depth * tan_half;
        assert!(region.contains(p), "the tip is outside the region at ({x:.4}, {y:.4}, {z:.4})");
        let d = dist_to_outline(region, p);
        assert!(r <= d + 1e-9, "the cone leaves the region at ({x:.4}, {y:.4}, {z:.4}): cone radius {r:.5} over wall distance {d:.5}");
    }
    assert!(cut > 0, "the path cuts nothing");
}

fn min_z(path: &Toolpath) -> f64 {
    cut_samples(path).iter().map(|s| s.2).fold(f64::INFINITY, f64::min)
}

fn letter_l() -> Region {
    // One reflex corner at (4, 4): the L of a sign.
    Region::new(vec![poly(&[(0.0, 0.0), (12.0, 0.0), (12.0, 4.0), (4.0, 4.0), (4.0, 14.0), (0.0, 14.0)])]).unwrap()
}

fn letter_o() -> Region {
    // A stroke between two tessellated circles: outer wall and hole.
    Region::new(vec![ngon(0.0, 0.0, 8.0, 48), ngon(0.0, 0.0, 5.0, 36)]).unwrap()
}

#[test]
fn the_cone_never_leaves_the_region() {
    let shapes: Vec<(&str, Region)> = vec![
        ("rectangle", rect(0.0, 0.0, 10.0, 4.0)),
        ("letter L", letter_l()),
        ("letter O", letter_o()),
        ("circle", Region::new(vec![ngon(0.0, 0.0, 5.0, 64)]).unwrap()),
        ("triangle", Region::new(vec![poly(&[(0.0, 0.0), (9.0, 0.0), (1.0, 6.0)])]).unwrap()),
    ];
    for (name, region) in &shapes {
        for angle in [30.0, 60.0, 90.0, 120.0] {
            for (bottom, flat) in [(-20.0, false), (-1.0, false), (-1.0, true)] {
                let p = VCarveParams { angle_deg: angle, bottom_z: bottom, stepdown: 0.7, flat_clear: flat, ..params() };
                let path = v_carve(region, &p).unwrap_or_else(|e| panic!("{name} at {angle} degrees: {e}"));
                assert!(path.all_finite() && path.is_chained(), "{name}: the path is not chained");
                assert_cone_inside(region, &path, angle, TOP);
                assert!(min_z(&path) >= bottom - 1e-9, "{name}: a move is under the maximum depth");
            }
        }
    }
}

#[test]
fn the_depth_follows_the_width_of_the_region() {
    // A 10 x 4 slot under a 90 degree cone: the inscribed circle on the
    // spine has the radius 2, so the tip stands 2 deep there.
    let region = rect(0.0, 0.0, 10.0, 4.0);
    let path = v_carve(&region, &params()).unwrap();
    let floor = min_z(&path);
    assert!((floor + 2.0).abs() < 0.02, "the spine is {floor}, not -2");
    // The deepest points stand on the spine y = 2, between x = 2 and 8.
    for (x, y, z) in cut_samples(&path) {
        if z < -1.98 {
            assert!((y - 2.0).abs() < 0.05 && (1.9..=8.1).contains(&x), "deep point off the spine: ({x}, {y}, {z})");
        }
    }
    // A wider slot is cut deeper: twice the width, twice the depth.
    let wide = v_carve(&rect(0.0, 0.0, 20.0, 8.0), &params()).unwrap();
    assert!((min_z(&wide) + 4.0).abs() < 0.02, "the wide spine is {}", min_z(&wide));
    // A narrower cone is cut deeper in the same slot.
    let narrow = v_carve(&region, &VCarveParams { angle_deg: 60.0, ..params() }).unwrap();
    let want = -2.0 / (30.0f64).to_radians().tan();
    assert!((min_z(&narrow) - want).abs() < 0.03, "60 degrees: {} against {want}", min_z(&narrow));
}

#[test]
fn a_sharp_corner_rises_to_the_surface() {
    let region = rect(0.0, 0.0, 10.0, 4.0);
    let path = v_carve(&region, &params()).unwrap();
    let pts = cut_samples(&path);
    for (cx, cy) in [(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (0.0, 4.0)] {
        let near = pts.iter().any(|&(x, y, z)| (x - cx).hypot(y - cy) < 0.02 && (TOP - z) < 0.02);
        assert!(near, "the path does not reach the corner ({cx}, {cy}) at the surface");
    }
}

#[test]
fn a_stroke_is_cut_in_one_engagement() {
    // The walk around a loop is continuous: no retract inside a letter.
    // A straight wall is one move, not one move for each sample.
    let plunges = |path: &Toolpath| path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    let slot = v_carve(&rect(0.0, 0.0, 10.0, 4.0), &params()).unwrap();
    assert_eq!(plunges(&slot), 1);
    assert!(slot.moves.len() <= 20, "a slot took {} moves", slot.moves.len());
    assert_eq!(plunges(&v_carve(&letter_l(), &params()).unwrap()), 1);
    // The O has two loops: the outer wall and the hole.
    assert!(plunges(&v_carve(&letter_o(), &params()).unwrap()) <= 2);
}

#[test]
fn a_tessellated_circle_is_one_dip_not_a_star() {
    // 64 chords of a circle of radius 5: the chord corners are not
    // corners of the drawing. The tip must stay at the centre, deep, and
    // must not rise to the surface at each chord end.
    let region = Region::new(vec![ngon(0.0, 0.0, 5.0, 64)]).unwrap();
    let path = v_carve(&region, &params()).unwrap();
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    for m in &feeds {
        assert!(m.to.x.hypot(m.to.y) < 0.1, "a feed leaves the centre: ({}, {})", m.to.x, m.to.y);
        assert!(m.to.z < -4.8, "a feed rises to {}", m.to.z);
    }
    assert!((min_z(&path) + 5.0).abs() < 0.03, "the centre is {}", min_z(&path));
}

#[test]
fn the_maximum_depth_limits_the_cut() {
    let region = rect(0.0, 0.0, 20.0, 20.0);
    let p = VCarveParams { bottom_z: -2.0, stepdown: 2.0, ..params() };
    let path = v_carve(&region, &p).unwrap();
    assert!((min_z(&path) + 2.0).abs() < 1e-9, "floor {}", min_z(&path));
    // Without flat clearing the middle of the wide region is not cut.
    assert!(cut_samples(&path).iter().all(|&(x, y, _)| (x - 10.0).hypot(y - 10.0) > 5.0));
}

#[test]
fn flat_clearing_removes_the_floor_the_cone_cannot_reach() {
    let region = rect(0.0, 0.0, 20.0, 20.0);
    let p = VCarveParams { bottom_z: -2.0, stepdown: 2.0, flat_clear: true, flat_stepover: 0.5, ..params() };
    let path = v_carve(&region, &p).unwrap();
    assert_cone_inside(&region, &path, 90.0, TOP);
    let floor: Vec<(f64, f64)> = cut_samples(&path).into_iter().filter(|s| (s.2 + 2.0).abs() < 1e-9).map(|s| (s.0, s.1)).collect();
    // Each point of the floor (2 mm inside the outline) has a pass of
    // the tip within one stepover.
    let mut y = 2.5;
    while y <= 17.5 {
        let mut x = 2.5;
        while x <= 17.5 {
            let near = floor.iter().any(|&(fx, fy)| (fx - x).hypot(fy - y) <= 0.5);
            assert!(near, "the floor at ({x}, {y}) is not cleared");
            x += 0.5;
        }
        y += 0.5;
    }
}

#[test]
fn a_deep_carve_steps_down() {
    let region = rect(0.0, 0.0, 20.0, 8.0);
    let p = VCarveParams { stepdown: 1.5, ..params() };
    let path = v_carve(&region, &p).unwrap();
    assert_cone_inside(&region, &path, 90.0, TOP);
    assert!((min_z(&path) + 4.0).abs() < 0.02);
    // The first engagement is no deeper than one stepdown: read the
    // cutting moves up to the first retract.
    let mut first = f64::INFINITY;
    let mut cutting = false;
    for m in &path.moves {
        if m.kind == MoveKind::Rapid {
            if cutting {
                break;
            }
            continue;
        }
        cutting = true;
        first = first.min(m.to.z);
    }
    assert!(first >= -1.5 - 1e-9, "the first pass is {first} deep");
    // The levels are distinct: -1.5, -3.0 and the spine at -4.0.
    for level in [-1.5, -3.0] {
        assert!(cut_samples(&path).iter().any(|s| (s.2 - level).abs() < 1e-9), "no pass at {level}");
    }
}

#[test]
fn a_cone_shorter_than_the_maximum_depth_limits_the_cut_and_says_so() {
    // A 4 mm wide 90 degree cone is 2 mm high: it cannot cut 6 mm deep.
    let region = rect(0.0, 0.0, 20.0, 20.0);
    let path = v_carve(&region, &VCarveParams { tool_diameter: 4.0, ..params() }).unwrap();
    assert!((min_z(&path) + 2.0).abs() < 1e-9, "floor {}", min_z(&path));
    assert!(path.notes.iter().any(|n| n.contains("cone")), "no note: {:?}", path.notes);
}

#[test]
fn bad_input_is_refused_by_name() {
    let region = rect(0.0, 0.0, 10.0, 4.0);
    let refused = |p: VCarveParams, word: &str| match v_carve(&region, &p) {
        Err(GenError::Invalid(msg)) => assert!(msg.contains(word), "'{msg}' does not name '{word}'"),
        other => panic!("expected a refusal that names '{word}', got {other:?}"),
    };
    refused(VCarveParams { angle_deg: 0.0, ..params() }, "angle");
    refused(VCarveParams { angle_deg: 180.0, ..params() }, "angle");
    refused(VCarveParams { tool_diameter: 0.0, ..params() }, "diameter");
    refused(VCarveParams { bottom_z: 0.0, ..params() }, "maximum depth");
    refused(VCarveParams { stepdown: 0.0, ..params() }, "stepdown");
    refused(VCarveParams { feed: 0.0, ..params() }, "feed");
    refused(VCarveParams { clearance_z: -1.0, ..params() }, "clearance");
    refused(VCarveParams { retract_z: Some(-1.0), ..params() }, "retract");
    refused(VCarveParams { flat_clear: true, flat_stepover: 0.0, ..params() }, "stepover");
    refused(VCarveParams { feed: f64::NAN, ..params() }, "finite");
    assert!(matches!(v_carve(&Region::empty(), &params()), Err(GenError::Empty(_))));
}

/// Degenerate regions give a clean error or a safe path. They do not
/// crash and they do not hang. A deterministic generator makes the
/// shapes, so a failure repeats.
#[test]
fn degenerate_regions_give_a_clean_answer() {
    let mut seed = 0x1443_u64;
    let mut next = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / f64::from(1u32 << 31)
    };
    let mut shapes: Vec<Vec<(f64, f64)>> = vec![
        // Zero width: all points on one line.
        vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)],
        // A sliver 1 micron wide.
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 0.001), (0.0, 0.001)],
        // A bow tie: the outline crosses itself.
        vec![(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)],
        // A spike that returns on itself.
        vec![(0.0, 0.0), (10.0, 0.0), (5.0, 0.0), (5.0, 5.0)],
        // Far from the origin.
        vec![(1e7, 1e7), (1e7 + 3.0, 1e7), (1e7 + 3.0, 1e7 + 3.0), (1e7, 1e7 + 3.0)],
    ];
    for _ in 0..40 {
        let n = 3 + (next() * 7.0) as usize;
        shapes.push((0..n).map(|_| ((next() * 12.0).round() * 0.5, (next() * 12.0).round() * 0.5)).collect());
    }
    for pts in &shapes {
        let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { continue };
        let Ok(region) = Region::new(vec![l]) else { continue };
        for flat in [false, true] {
            let p = VCarveParams { bottom_z: -1.0, stepdown: 1.0, flat_clear: flat, ..params() };
            match v_carve(&region, &p) {
                Ok(path) => {
                    assert!(path.all_finite() && path.is_chained(), "{pts:?}");
                    if path.moves.iter().any(|m| m.kind != MoveKind::Rapid && m.to.z < TOP - 1e-9) {
                        assert_cone_inside(&region, &path, 90.0, TOP);
                    }
                }
                Err(e) => assert!(!e.to_string().is_empty()),
            }
        }
    }
}

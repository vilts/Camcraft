//! #2431: the Parallel finish crosses a hole the ball cannot enter in
//! one straight feed at the surface height - no retract, no plunge -
//! and still verifies against the real model. A hole the ball fits
//! into keeps its clean hop: its walls are model surface.
//!
//! Gate map (#2431): covers camcraft_motion::finishing3d (raster_bridge, raster).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{parallel, Finish3DParams, FinishPattern, GapLink};
use camcraft_motion::geometry::{Point3, Triangle};

const R: f64 = 1.5; // a 3 mm ball
const TOL: f64 = 0.05;
const TOP: f64 = 8.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn quad(out: &mut Vec<Triangle>, p: [Point3; 4]) {
    out.push(Triangle::new(p[0], p[1], p[2]));
    out.push(Triangle::new(p[0], p[2], p[3]));
}

/// A stepped block on an XY grid of cells: each cell stands to its
/// height (`None` = through), with top, bottom and every wall where a
/// neighbour (or the outside) stands lower.
fn block(xs: &[f64], ys: &[f64], h: &dyn Fn(usize, usize) -> Option<f64>) -> Vec<Triangle> {
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let at = |i: isize, j: isize| {
        if i < 0 || j < 0 || i >= nx as isize || j >= ny as isize {
            None
        } else {
            h(i as usize, j as usize)
        }
    };
    let mut f = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let Some(z) = h(i, j) else { continue };
            let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            quad(&mut f, [p3(x0, y0, z), p3(x1, y0, z), p3(x1, y1, z), p3(x0, y1, z)]);
            quad(&mut f, [p3(x0, y0, 0.0), p3(x0, y1, 0.0), p3(x1, y1, 0.0), p3(x1, y0, 0.0)]);
            let (ii, jj) = (i as isize, j as isize);
            let sides = [
                (at(ii - 1, jj), [p3(x0, y1, 0.0), p3(x0, y0, 0.0)]),
                (at(ii + 1, jj), [p3(x1, y0, 0.0), p3(x1, y1, 0.0)]),
                (at(ii, jj - 1), [p3(x0, y0, 0.0), p3(x1, y0, 0.0)]),
                (at(ii, jj + 1), [p3(x1, y1, 0.0), p3(x0, y1, 0.0)]),
            ];
            for (n, [a, b]) in sides {
                let lo = n.unwrap_or(0.0);
                if lo < z {
                    let (a0, b0) = (p3(a.x, a.y, lo), p3(b.x, b.y, lo));
                    quad(&mut f, [a0, b0, p3(b.x, b.y, z), p3(a.x, a.y, z)]);
                }
            }
        }
    }
    f
}

/// 40 x 20 x 8 plate, two square through holes of side `w` centred
/// at (10, 10) and (30, 10).
fn holed_plate(w: f64) -> Vec<Triangle> {
    let h = 0.5 * w;
    let xs = [0.0, 10.0 - h, 10.0 + h, 30.0 - h, 30.0 + h, 40.0];
    let ys = [0.0, 10.0 - h, 10.0 + h, 20.0];
    block(&xs, &ys, &|i, j| (!(j == 1 && (i == 1 || i == 3))).then_some(TOP))
}

/// A 2 mm through slot across the plate's full depth at x 19..21:
/// open to both edges, two bodies.
fn slotted_plate() -> Vec<Triangle> {
    let xs = [0.0, 19.0, 21.0, 40.0];
    let ys = [0.0, 20.0];
    block(&xs, &ys, &|i, _| (i != 1).then_some(TOP))
}

/// A lid edge a hair off the rim: the right half stands `dz` lower.
fn stepped_plate(dz: f64) -> Vec<Triangle> {
    let xs = [0.0, 20.0, 40.0];
    let ys = [0.0, 20.0];
    block(&xs, &ys, &|i, _| Some(if i == 1 { TOP - dz } else { TOP }))
}

fn fp() -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::Parallel,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 20.0,
        retract_z: None,
        gap_link: GapLink::Jump,
        top_z: TOP,
        bottom_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// Ericson's closest point on a triangle, distance only.
fn pt_tri(p: Point3, t: &Triangle) -> f64 {
    let (ab, ac, ap) = (t.b - t.a, t.c - t.a, p - t.a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    let n = |u: Point3| u.dot(u).sqrt();
    let ax = |o: Point3, u: Point3, s: f64| p3(o.x + u.x * s, o.y + u.y * s, o.z + u.z * s);
    if d1 <= 0.0 && d2 <= 0.0 {
        return n(ap);
    }
    let bp = p - t.b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return n(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return n(p - ax(t.a, ab, d1 / (d1 - d3)));
    }
    let cp = p - t.c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return n(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return n(p - ax(t.a, ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return n(p - ax(t.b, t.c - t.b, (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let den = 1.0 / (va + vb + vc);
    n(p - ax(ax(t.a, ab, vb * den), ac, vc * den))
}

/// The deepest bite of the ball into `model` along every straight
/// feed, sampled at 0.05 mm - a bridge is judged along its length, not
/// only at its ends.
fn deepest_bite(t: &Toolpath, model: &[Triangle]) -> f64 {
    let mut worst = f64::NEG_INFINITY;
    for m in t.moves.iter().filter(|m| matches!(m.kind, MoveKind::Feed)) {
        let len = (m.to - m.from).dot(m.to - m.from).sqrt();
        // An arc is judged at its ends only: its chord is not the path.
        let n = if m.arc.is_some() { 1 } else { ((len / 0.05).ceil() as usize).max(1) };
        for k in 0..=n {
            let s = k as f64 / n as f64;
            let q = p3(
                m.from.x + (m.to.x - m.from.x) * s,
                m.from.y + (m.to.y - m.from.y) * s,
                m.from.z + (m.to.z - m.from.z) * s + R,
            );
            let d = model.iter().map(|f| pt_tri(q, f)).fold(f64::INFINITY, f64::min);
            worst = worst.max(R - d);
        }
    }
    worst
}

/// Feeds that cross the X band `[lo, hi]` in one move, on rows inside
/// the Y band `[ylo, yhi]`, at the surface height.
fn crossings(t: &Toolpath, (lo, hi): (f64, f64), (ylo, yhi): (f64, f64), z: f64) -> usize {
    t.moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed))
        .filter(|m| m.from.x.min(m.to.x) <= lo && m.from.x.max(m.to.x) >= hi)
        .filter(|m| (ylo..=yhi).contains(&m.from.y) && (ylo..=yhi).contains(&m.to.y))
        .filter(|m| (m.from.z - z).abs() <= TOL && (m.to.z - z).abs() <= TOL)
        .count()
}

/// Rapids and plunges that end over `x` band x `y` band: the picks.
fn picks(t: &Toolpath, (xlo, xhi): (f64, f64), (ylo, yhi): (f64, f64)) -> usize {
    t.moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Rapid | MoveKind::Plunge))
        .filter(|m| (xlo..=xhi).contains(&m.to.x) && (ylo..=yhi).contains(&m.to.y))
        .count()
}

#[test]
fn a_hole_narrower_than_the_ball_is_crossed_without_a_pick() {
    let plate = holed_plate(2.0);
    let path = parallel(&plate, &fp()).expect("parallel");
    for cx in [10.0, 30.0] {
        let over = crossings(&path, (cx - 1.0, cx + 1.0), (9.0, 11.0), TOP);
        assert!(over > 0, "hole at x {cx}: the raster must cross it in one feed at the top");
        let near = picks(&path, (cx - 1.0 - R, cx + 1.0 + R), (9.0 - R, 11.0 + R));
        assert_eq!(near, 0, "hole at x {cx}: no retract or plunge over the hole");
    }
    let bite = deepest_bite(&path, &plate);
    assert!(bite <= TOL, "the bridged path bites the real plate by {bite}");
}

#[test]
fn an_open_slot_narrower_than_the_ball_is_crossed_without_a_pick() {
    let plate = slotted_plate();
    let path = parallel(&plate, &fp()).expect("parallel");
    let over = crossings(&path, (19.0, 21.0), (0.0, 20.0), TOP);
    assert!(over > 10, "every row must cross the open slot, crossed {over}");
    assert_eq!(picks(&path, (19.0 - R, 21.0 + R), (0.5, 19.5)), 0, "no pick at the slot");
    let bite = deepest_bite(&path, &plate);
    assert!(bite <= TOL, "the bridged path bites the real plate by {bite}");
}

#[test]
fn a_lid_edge_a_hair_off_the_rim_is_crossed_without_a_pick() {
    // A 0.01 mm step leaves a sqrt(2 r dz) = 0.17 mm stretch where no
    // facet answers under the center; its chord sags 0.003 mm.
    let plate = stepped_plate(0.01);
    let path = parallel(&plate, &fp()).expect("parallel");
    let over = t_cross(&path);
    assert!(over > 10, "every row must cross the step in one feed, crossed {over}");
    let bite = deepest_bite(&path, &plate);
    assert!(bite <= TOL, "the bridged path bites the real plate by {bite}");
}

/// Crossings of the 0.01 step at x 20 on the rows inside the plate.
fn t_cross(t: &Toolpath) -> usize {
    t.moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed))
        .filter(|m| m.from.x.min(m.to.x) < 20.0 && m.from.x.max(m.to.x) > 20.0)
        .filter(|m| (m.from.y - m.to.y).abs() < 1e-9 && (0.5..=19.5).contains(&m.from.y))
        .filter(|m| m.from.z >= TOP - 0.01 - TOL && m.to.z >= TOP - 0.01 - TOL)
        .count()
}

#[test]
fn a_hole_the_ball_fits_into_keeps_a_clean_hop() {
    // 3.4 mm against a 3 mm ball: the hole is model surface, and the
    // link over it goes up to the safe height, never across at the top.
    let plate = holed_plate(3.4);
    let path = parallel(&plate, &fp()).expect("parallel");
    for cx in [10.0, 30.0] {
        let over = crossings(&path, (cx - 1.7, cx + 1.7), (8.3, 11.7), TOP);
        assert_eq!(over, 0, "hole at x {cx}: wider than the tool, no bridge");
    }
    let low_rapid = path
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Rapid))
        .filter(|m| (m.from.x - m.to.x).abs() > 1e-9 || (m.from.y - m.to.y).abs() > 1e-9)
        .any(|m| m.from.z < TOP + R || m.to.z < TOP + R);
    assert!(!low_rapid, "every lateral rapid stays at the safe height");
    let bite = deepest_bite(&path, &plate);
    assert!(bite <= TOL, "the hop path bites the real plate by {bite}");
}

#[test]
fn a_bridge_never_spans_a_wide_gap_or_a_tall_step() {
    // A 1 mm step is a shelf edge, not a lid hair: the line still
    // breaks there, and nothing feeds through the corner.
    let plate = stepped_plate(1.0);
    let path = parallel(&plate, &fp()).expect("parallel");
    let bite = deepest_bite(&path, &plate);
    assert!(bite <= TOL, "the shelf path bites the real plate by {bite}");
    let through = path
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed))
        .filter(|m| m.from.x.min(m.to.x) < 19.9 && m.from.x.max(m.to.x) > 20.1)
        .any(|m| m.from.z.min(m.to.z) < TOP - 0.01 && m.from.z.max(m.to.z) > TOP - 0.99);
    assert!(!through, "no straight feed crosses a 1 mm shelf edge");
}

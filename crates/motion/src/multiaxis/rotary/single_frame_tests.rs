//! #2970 review: Rotary Parallel on a part that is not round loses no
//! cut that the single-frame path made, cuts no place twice, and names
//! the places no pass reaches. The reference is the generator as it
//! stood before the two halves (`single_frame`, kept here for tests
//! only): the swarf raster in the laid frame, the radial relink, the
//! radial pose. Both paths are read as ball centres in the world.
//!
//! Provenance: our own fixtures (a keyed bar).

use super::*;

const TAU: f64 = std::f64::consts::TAU;
const BALL: f64 = 3.0; // a 6 mm ball nose
/// Half the stepover: two centres this close stand on the same place.
const SAME: f64 = 0.75;

/// The generator before #2970, for comparison only: one laid frame.
fn single_frame(facets: &[Triangle], p: &Finish3DParams, c: &AxisControl) -> Result<Toolpath, GenError> {
    let ax = axis::fit(facets)?;
    let uz = ax.dir.dot(UP);
    let up = Point3::new(-ax.dir.x * uz, -ax.dir.y * uz, 1.0 - uz * uz);
    let up = normalize(&up).ok_or_else(|| GenError::Invalid("degenerate".into()))?;
    let v = up.cross(ax.dir);
    let f = TiltedFrame { origin: ax.origin, u: [ax.dir.x, ax.dir.y, ax.dir.z], v: [v.x, v.y, v.z], axis: [up.x, up.y, up.z] };
    let local = f.local_facets(facets);
    let mut q = stations(&local, p, &f, &|h| h - ax.origin.z);
    (q.ramp, q.entry_ramp) = (false, false);
    let r = p.tool_diameter * 0.5;
    let base = finishing3d::swarf(&local, &q, 1.0, 0.0)?;
    let hop = q.retract_z.map_or(q.clearance_z, |h| h.min(q.clearance_z));
    let rho = rotary_links::local_rho_max(&local);
    let base = rotary_links::radial_relink(&base, r, q.clearance_z, hop, rho, p.feed, p.plunge_feed, None)?;
    posed(&f, &base, &ax, c, r)
}

/// A closed prism along +x from `x0` to `x1` over the cross-section
/// `ring` (y, z), counter-clockwise and star-shaped from the axis.
fn prism(ring: &[(f64, f64)], x0: f64, x1: f64) -> Vec<Triangle> {
    let p = |x: f64, (y, z): (f64, f64)| Point3::new(x, y, z);
    let mut t = Vec::new();
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        t.push(Triangle::new(p(x0, a), p(x0, b), p(x1, b)));
        t.push(Triangle::new(p(x0, a), p(x1, b), p(x1, a)));
        t.push(Triangle::new(p(x0, (0.0, 0.0)), p(x0, b), p(x0, a)));
        t.push(Triangle::new(p(x1, (0.0, 0.0)), p(x1, a), p(x1, b)));
    }
    t
}

fn arc(r: f64, n: usize, from: f64, to: f64) -> Vec<(f64, f64)> {
    (0..=n).map(|j| from + (to - from) * j as f64 / n as f64).map(|th| (r * th.cos(), r * th.sin())).collect()
}

/// A round bar R 10, stations 0..60, with a keyway in its +y side over
/// stations 20..40: walls at z = +-5, floor at y = 6. Three closed
/// shells that touch (the generators read a facet soup).
fn keyway_bar() -> Vec<Triangle> {
    let a = (5.0f64 / 10.0).asin();
    let round = &arc(10.0, 64, 0.0, TAU)[..64];
    let mut key = arc(10.0, 60, a, TAU - a);
    key.extend([(6.0, -5.0), (6.0, 5.0)]);
    let mut t = prism(round, 0.0, 20.0);
    t.extend(prism(&key, 20.0, 40.0));
    t.extend(prism(round, 40.0, 60.0));
    t
}

fn params(bottom: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: finishing3d::GapLink::Jump,
        pattern: finishing3d::FinishPattern::Parallel,
        stepover: Some(1.5),
        tool_diameter: 2.0 * BALL,
        tolerance: 0.1,
        clearance_z: 25.0,
        retract_z: None,
        top_z: -bottom,
        bottom_z: bottom,
        feed: 800.0,
        plunge_feed: 200.0,
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

const STILL: AxisControl = AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None };

/// The ball centres of the cutting moves, each move sampled every
/// 0.25 mm, with the move's index; only over the turned work (inside
/// its ends by the ball, out to its radius plus the ball).
fn centres(path: &Toolpath) -> Vec<(usize, Point3)> {
    let mut out = Vec::new();
    for (i, m) in path.moves.iter().enumerate().filter(|(_, m)| m.kind == MoveKind::Feed) {
        let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        let c = |t: Point3| Point3::new(t.x + BALL * a[0], t.y + BALL * a[1], t.z + BALL * a[2]);
        let (p, q) = (c(m.from), c(m.to));
        let n = ((q - p).dot(q - p).sqrt() / 0.25).ceil().clamp(1.0, 1e4) as usize;
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let s = Point3::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t, p.z + (q.z - p.z) * t);
            let rho = (s.y * s.y + s.z * s.z).sqrt();
            if s.x > BALL && s.x < 60.0 - BALL && (BALL..=13.2).contains(&rho) {
                out.push((i, s));
            }
        }
    }
    out
}

/// A grid hash of centre samples, cell `SAME`: the samples within `d`
/// (at most `SAME`) of a point.
struct Grid(std::collections::HashMap<(i64, i64, i64), Vec<(usize, Point3)>>);

impl Grid {
    fn key(c: Point3) -> (i64, i64, i64) {
        ((c.x / SAME).floor() as i64, (c.y / SAME).floor() as i64, (c.z / SAME).floor() as i64)
    }
    fn of(pts: &[(usize, Point3)]) -> Grid {
        let mut g = Grid(Default::default());
        for (i, c) in pts {
            g.0.entry(Grid::key(*c)).or_default().push((*i, *c));
        }
        g
    }
    fn within(&self, c: Point3, d: f64) -> Vec<(usize, Point3)> {
        let (i, j, k) = Grid::key(c);
        let mut out = Vec::new();
        for a in -1..=1 {
            for b in -1..=1 {
                for e in -1..=1 {
                    if let Some(v) = self.0.get(&(i + a, j + b, k + e)) {
                        out.extend(v.iter().filter(|(_, q)| dist(*q, c) <= d));
                    }
                }
            }
        }
        out
    }
}

fn dist(a: Point3, b: Point3) -> f64 {
    (a - b).dot(a - b).sqrt()
}

fn deg(c: Point3) -> f64 {
    c.z.atan2(c.y).to_degrees().rem_euclid(360.0)
}

fn where_(pts: &[Point3]) -> String {
    if pts.is_empty() {
        return "none".into();
    }
    let (s0, s1) = pts.iter().fold((f64::MAX, f64::MIN), |(l, h), c| (l.min(c.x), h.max(c.x)));
    let (a0, a1) = pts.iter().fold((f64::MAX, f64::MIN), |(l, h), c| (l.min(deg(*c)), h.max(deg(*c))));
    format!("{} samples, stations {s0:.1}..{s1:.1} mm, angles {a0:.0}..{a1:.0} deg (0 = +y, 90 = top)", pts.len())
}

#[test]
fn a_keyway_loses_no_cut_of_the_single_frame_path() {
    let facets = keyway_bar();
    let p = params(-10.0);
    let old = centres(&single_frame(&facets, &p, &STILL).expect("the single frame"));
    let got = rotary_parallel(&facets, &p, &STILL).expect("the two halves");
    let grid = Grid::of(&centres(&got));
    let lost: Vec<Point3> = old.iter().map(|(_, c)| *c).filter(|c| grid.within(*c, SAME).is_empty()).collect();
    eprintln!("keyway: lost {} of {}; notes {:?}", where_(&lost), old.len(), got.notes);
    assert!(lost.is_empty(), "cuts of the single frame lost: {}", where_(&lost));
}

/// No place is cut by two passes that are not neighbours on one pass:
/// a centre sample of one move stands SAME or more from every sample
/// of each move more than two moves away, except where the two meet
/// at their ends (a ring's close, a corner).
#[test]
fn a_keyway_is_cut_once_in_each_place() {
    let facets = keyway_bar();
    let got = rotary_parallel(&facets, &params(-10.0), &STILL).expect("the two halves");
    let new = centres(&got);
    let grid = Grid::of(&new);
    let mut twice = Vec::new();
    for (i, c) in &new {
        let interior = |j: usize| {
            let m = &got.moves[j];
            let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
            let e = |t: Point3| Point3::new(t.x + BALL * a[0], t.y + BALL * a[1], t.z + BALL * a[2]);
            dist(*c, e(m.from)).min(dist(*c, e(m.to))) > SAME
        };
        if !interior(*i) {
            continue;
        }
        if grid.within(*c, 0.1).iter().any(|(j, _)| j.abs_diff(*i) > 2 && interior(*j)) {
            twice.push(*c);
        }
    }
    assert!(twice.is_empty(), "cut twice: {}", where_(&twice));
}

/// The reach reading names an arc no pass reaches: a laid round bar
/// whose path rides only the upper half (centres every 5 degrees from
/// the +y side over the top to the -y side) leaves the lower half, and
/// the note says so with its arc and stations; a path all the way
/// round leaves no note.
#[test]
fn the_reach_reading_names_the_arc_no_pass_reaches() {
    let r = 1.0;
    let local = prism(&arc(3.0, 48, 0.0, TAU)[..48], 0.0, 20.0);
    let ring = |from: f64, to: f64| {
        let mut moves = Vec::new();
        let mut th = from;
        while th <= to + 1e-9 {
            let (y, z) = (4.0 * th.to_radians().sin(), 4.0 * th.to_radians().cos());
            // Vertical-ball tips in the laid frame: the centre r over the tip.
            let (a, b) = (Point3::new(0.5, y, z - r), Point3::new(19.5, y, z - r));
            moves.push(crate::cl::Move::new(MoveKind::Feed, a, b, Some(100.0)));
            th += 5.0;
        }
        Toolpath::new(moves)
    };
    let half = gaps::unreached(&local, &gaps::Cover::of(&ring(-90.0, 90.0), r, 0.6), r, -10.0, 0.05);
    assert_eq!(half.len(), 1, "{half:?}");
    assert!(half[0].starts_with("Rotary Parallel: no pass reaches the surface over 100..260 deg about the axis"), "{half:?}");
    assert!(half[0].contains("at stations 1.000..19.000 mm"), "{half:?}");
    let whole = gaps::unreached(&local, &gaps::Cover::of(&ring(-180.0, 175.0), r, 0.6), r, -10.0, 0.05);
    assert!(whole.is_empty(), "{whole:?}");
    // A Bottom over the axis leaves the lower half as asked: no note.
    let held = gaps::unreached(&local, &gaps::Cover::of(&ring(-90.0, 90.0), r, 0.6), r, 0.0, 0.05);
    assert!(held.is_empty(), "{held:?}");
}

//! Rotary Offset Zigzag levels finish their walls with a ring that skips
//! the seam (#3168). Before the fix the wrap kept its rows alone: the
//! level's boundary walls took no ring, so cusps stood between row ends
//! on every wall perpendicular to the rows (a D-flat's edge walls), and
//! the ring the plan could not close across the seam was dropped whole.
//! The fix drops only the seam's own edges: a loop that spans a whole
//! period keeps one period of boundary per arc, so no azimuth is cut
//! twice by one ring.
//!
//! Gate map: covers camcraft_motion::machining3d (hsm plan_level's
//! Zigzag rings, entry::pocket3d_air_split's seam) through
//! camcraft_motion::multiaxis (rotary_offset).
//! Provenance: the wall-reach law is general machining practice; the
//! wrap and development construction is `multiaxis::rotary_offset`'s
//! own; no vendor material consulted.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{PocketStrategy, ARC_FIT_TOL, boundary_rings_probe};
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d_air_split};
use camcraft_motion::multiaxis::{AxisControl, rotary_offset_in};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 1.0;

/// A wall scallop deeper than this is a missing finish ring (the twins'
/// bound, `pocket_wall_finish_levels::REACH_TOL`).
const REACH_TOL: f64 = 0.02;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn params(v_top: f64, v_bot: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 1.2,
        stock_to_leave: 0.1,
        clearance_z: 100.0,
        top_z: v_top,
        bottom_z: v_bot,
        stepdown: 0.5,
        feed: 400.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        pass_start: Default::default(),
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn control() -> AxisControl {
    AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None }
}

fn same(a: Point3, b: Point3) -> bool {
    (a - b).dot(a - b) < 1e-24
}

fn tri(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3) {
    if !(same(a, b) || same(b, c) || same(c, a)) {
        f.push(Triangle::new(a, b, c));
    }
}

/// One cross-section ring of `n` points at station `z`: the circle of
/// radius RAD, or (with `flat`) the D its flat leaves.
fn ring(n: usize, z: f64, flat: Option<(f64, f64)>) -> Vec<Point3> {
    (0..n)
        .map(|j| {
            let th = TAU * j as f64 / n as f64;
            let (mut x, mut y) = (6.0 * th.cos(), 6.0 * th.sin());
            if let Some((phi, d)) = flat {
                let (nx, ny) = (phi.cos(), phi.sin());
                let over = x * nx + y * ny - d;
                if over > 0.0 {
                    x -= over * nx;
                    y -= over * ny;
                }
            }
            p3(x, y, z)
        })
        .collect()
}

/// A closed prism through the rings in order, capped by fans from the
/// axis at both ends (the dflat suite's fixture, verbatim).
fn prism(rings: &[Vec<Point3>]) -> Vec<Triangle> {
    let mut f = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let n = a.len();
        for j in 0..n {
            let k = (j + 1) % n;
            tri(&mut f, a[j], a[k], b[k]);
            tri(&mut f, a[j], b[k], b[j]);
        }
    }
    let (Some(lo), Some(hi)) = (rings.first(), rings.last()) else { return f };
    let n = lo.len();
    let (c0, c1) = (p3(0.0, 0.0, lo[0].z), p3(0.0, 0.0, hi[0].z));
    for j in 0..n {
        let k = (j + 1) % n;
        tri(&mut f, c0, lo[k], lo[j]);
        tri(&mut f, c1, hi[j], hi[k]);
    }
    f
}

/// A bar of radius 6 over stations `z0..z1`, with a D-flat at depth `d`
/// along `phi`, ground over the stations `lo..hi` only.
fn dflat_bar(n: usize, z0: f64, z1: f64, phi: f64, d: f64, lo: f64, hi: f64) -> Vec<Triangle> {
    let fl = Some((phi, d));
    prism(&[ring(n, z0, None), ring(n, lo, None), ring(n, lo, fl), ring(n, hi, fl), ring(n, hi, None), ring(n, z1, None)])
}

/// A coaxial round bar stock of radius `rho` over `z0..z1` (the dflat
/// suite's fixture).
fn stock(rho: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    prism(&[ring(64, z0, None), ring(64, z1, None)])
        .into_iter()
        .map(|t| {
            let s = |p: Point3| p3(p.x * rho / 6.0, p.y * rho / 6.0, p.z);
            Triangle::new(s(t.a), s(t.b), s(t.c))
        })
        .collect()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| p3([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn rect(x0: f64, x1: f64, y0: f64, y1: f64) -> Region {
    match Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) {
        Ok(l) => Region::new(vec![l]).unwrap_or_else(|_| Region::empty()),
        Err(_) => Region::empty(),
    }
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

fn assert_no_crossings(path: &Toolpath, what: &str) {
    let crossed = camcraft_motion::verify::crossed_pairs(path, ARC_FIT_TOL);
    assert!(crossed.is_empty(), "{what}: crossing feed pairs {crossed:?}");
}

/// The feed chords at one wrap level, in azimuth - station coordinates:
/// each chord's endpoints as (azimuth * rho_ref, station).
fn level_chords_dev(path: &Toolpath, t: f64, rho_ref: f64) -> Vec<(Point2, Point2)> {
    let at = |q: Point3| Point2::new(q.y.atan2(q.x) * rho_ref, q.z);
    let mut out = Vec::new();
    for m in &path.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let (rt, rb) = ((m.from.x * m.from.x + m.from.y * m.from.y).sqrt(), (m.to.x * m.to.x + m.to.y * m.to.y).sqrt());
        if (rt - t).abs() > 1e-6 || (rb - t).abs() > 1e-6 {
            continue;
        }
        // Every feed leg is capped at the tool radius, so a move is one
        // chord within the scallop tolerance.
        out.push((at(m.from), at(m.to)));
    }
    out
}

// ---- the seam law on a developed cover ------------------------------

/// #3168, the seam law: on a developed cover two periods wide, the
/// Zigzag target band's ring keeps one period per arc. A ring that ran
/// down both cover ends would cut the seam's one azimuth twice, in
/// opposite directions, at depth. Pinned on `pocket3d_air_split`'s own
/// seam argument - no wrap needed: the developed world is the entry's.
#[test]
fn the_wrapped_ring_cuts_no_azimuth_twice() {
    let rho = 5.0;
    let period = TAU * rho;
    // The developed work: two slabs leaving a 4 mm band at v 3..7. The
    // band's target loop spans the whole two-period cover.
    let span = 2.0 * period + 2.0;
    let facets = box_facets([-2.0, -2.0, -1.0], [span, 3.0, 5.0])
        .into_iter()
        .chain(box_facets([-2.0, 7.0, -1.0], [span, 12.0, 5.0]))
        .collect::<Vec<_>>();
    let pad = R + 0.1;
    let reach = pad;
    let fp = rect(-pad, 2.0 * period + pad, -2.0, 12.0);
    assert!(!fp.is_empty(), "fixture footprint");
    let p = Pocket3DParams {
        top_z: 4.5,
        bottom_z: 0.5,
        clearance_z: 100.0,
        ..params(0.0, 0.0)
    };
    let path = match pocket3d_air_split(&facets, &fp, &fp, 6.0, Some((period, 2.0 * pad)), &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed band roughs: {e:?}"),
    };
    assert_no_crossings(&path, "developed band");
    let mut levels: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.from.z)
        .collect();
    levels.sort_by(|a, b| a.partial_cmp(b).unwrap());
    levels.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    assert!(!levels.is_empty(), "the band takes at least one level");
    for z in levels {
        let chords: Vec<(Point2, Point2)> = path
            .moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - z).abs() < 1e-9 && (m.to.z - z).abs() < 1e-9)
            .flat_map(|m| m.chords(0.001))
            .collect();
        assert!(!chords.is_empty(), "no feed chords at Z {z}");
        // The band's own walls (v 3 and 7) are ringed: every wall point
        // stands within one radius of a cut center.
        for wall in [3.0, 7.0] {
            let mut worst = f64::NEG_INFINITY;
            let mut x = 0.0;
            while x <= period {
                let s = Point2::new(x, wall);
                let d = chords.iter().map(|&(c, e)| seg_dist(s, c, e)).fold(f64::INFINITY, f64::min) - reach;
                worst = worst.max(d);
                x += 0.5;
            }
            assert!(worst <= REACH_TOL, "band wall v {wall} scallop {worst:.3} mm at Z {z}");
        }
        // No azimuth is cut twice in the seam direction: at one azimuth
        // bin the radial feed chords run one way only. Both cover ends
        // of a naive closed ring stand at the same azimuth and would
        // give full runs of both directions there.
        let mut runs: Vec<(f64, f64, f64)> = Vec::new(); // (azimuth bin, up, down)
        for (c, e) in &chords {
            let (dx, dy) = (e.x - c.x, e.y - c.y);
            if dx.abs() > 0.01 || dy.abs() <= 0.02 {
                continue;
            }
            let key = (c.x % period / 0.5).round() * 0.5;
            let run = runs.iter_mut().find(|(k, _, _)| (*k - key).abs() < 0.25);
            match run {
                Some((_, up, down)) => {
                    if dy > 0.0 {
                        *up += dy;
                    } else {
                        *down -= dy;
                    }
                }
                None => {
                    let (up, down) = if dy > 0.0 { (dy, 0.0) } else { (0.0, -dy) };
                    runs.push((key, up, down));
                }
            }
        }
        for (key, up, down) in &runs {
            assert!(
                up.min(*down) <= 0.2,
                "the seam azimuth near s {key:.1} is cut both ways at Z {z} (up {up:.2}, down {down:.2})"
            );
        }
    }
}

// ---- the wall-finish law on a D-flat --------------------------------

/// #3168, the cusps: a D-flat's edge walls stand perpendicular to the
/// rows, so row ends leave scallops between themselves. The level's
/// ring closes the flat's loop and finishes both walls. On a plain
/// revolve every level wall runs parallel to the rows; the flat is the
/// wall the missing ring used to leave cusped.
#[test]
fn the_dflat_edge_walls_are_ringed() {
    let rho_ref = 6.5;
    let skin = 6.0;
    let d = 5.0;
    let leave = 0.1;
    let bar = dflat_bar(64, -20.0, 20.0, 0.0, d, -8.0, 8.0);
    let st = stock(rho_ref, -20.0, 20.0);
    let path = match rotary_offset_in(&bar, Some(&st), &params(12.0, -12.0), &control()) {
        Ok(t) => t,
        Err(e) => panic!("the flat bar roughs: {e:?}"),
    };
    let reach = R + leave;
    let mut ts: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| {
            if m.kind == MoveKind::Rapid {
                return false;
            }
            let rt = (m.from.x * m.from.x + m.from.y * m.from.y).sqrt();
            let rb = (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
            (rt - rb).abs() < 1e-6
        })
        .map(|m| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt())
        .collect();
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut checked = 0usize;
    for t in ts {
        // The flat's walls exist only above the flat: the flat's edge
        // azimuth is acos(d / t) off the flat's own direction.
        if t <= d + 0.25 || t >= skin - 0.25 {
            continue;
        }
        let alpha = (d / t).acos();
        let chords = level_chords_dev(&path, t, rho_ref);
        assert!(!chords.is_empty(), "no feed chords at tangent {t}");
        for sign in [-1.0, 1.0] {
            let se = sign * alpha * rho_ref;
            let mut v = -6.5;
            while v <= 6.5 {
                let s = Point2::new(se, v);
                let worst = chords.iter().map(|&(c, e)| seg_dist(s, c, e)).fold(f64::INFINITY, f64::min) - reach;
                assert!(
                    worst <= REACH_TOL,
                    "flat edge wall at tangent {t}, sign {sign}: scallop {worst:.3} mm at v {v}"
                );
                v += 0.25;
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no level stands between the flat and the skin");
}

// ---- the closing-edge law on a near-end loop ------------------------

/// PR #3496's review: the seam ring's reach gate admits a loop that
/// stands near a cover end but owns no vertex ON the bound. The split
/// then found no seam run, and returned the loop's points open - the
/// closing edge (here the wall at x = 1.5 mod 10) was dropped, and the
/// periodic copy did not restore it. A loop the gate admits but whose
/// vertices touch no cover bound must close exactly as the no-seam path
/// closes: only a loop with a real seam run may wrap the seam.
#[test]
fn the_near_end_loop_without_a_seam_vertex_keeps_its_closing_edge() {
    let loops: [Vec<Point2>; 2] = [
        [(1.5, 2.0), (8.5, 2.0), (8.5, 8.0), (1.5, 8.0)]
            .into_iter()
            .map(|(x, y)| Point2::new(x, y))
            .collect(),
        [(11.5, 2.0), (18.5, 2.0), (18.5, 8.0), (11.5, 8.0)]
            .into_iter()
            .map(|(x, y)| Point2::new(x, y))
            .collect(),
    ];
    // The reviewer's cover: two periods wide, reach 2 - both loops
    // enter the seam branch, neither stands on a cover bound.
    let seam = (0.0, 20.0, 10.0, 2.0);
    for (i, pts) in loops.iter().enumerate() {
        for climb in [true, false] {
            let passes = boundary_rings_probe(pts, climb, Some(seam));
            assert_eq!(passes.len(), 1, "loop {i}, climb {climb}: one ring");
            let ring = &passes[0];
            let (first, last) = (ring.first().copied(), ring.last().copied());
            assert_eq!(
                first,
                last,
                "loop {i}, climb {climb}: the ring stays open - its closing wall edge is dropped"
            );
            assert_eq!(ring.len(), pts.len() + 1, "loop {i}, climb {climb}: the ring is the loop plus its close");
        }
    }
}





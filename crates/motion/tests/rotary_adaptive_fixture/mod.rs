//! The rotary Adaptive suite's own fixtures and reader (issue #55 split
//! them out of `rotary_adaptive.rs` to hold the 400-line law): the
//! turned bar and its coaxial stock, the Adaptive params, and the
//! per-level load / kink / ring-cut reader on the developed path, from
//! the wrap literature in `multiaxis::wrap_exit` and the HSM planner's
//! own laws; no vendor material consulted.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::load::{load_at_rim, DEFAULT_OPTIMAL_LOAD};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::AxisControl;
use camcraft_motion::quality::{measure, stock_for, QualitySpec};

pub const TAU: f64 = std::f64::consts::TAU;
pub const D: f64 = 6.0;
pub const R: f64 = D * 0.5;

/// A closed solid of revolution about Z at `n` turns: the meridian
/// (rho, z) swept.
pub fn swept(n: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let pt = |rho: f64, z: f64, j: usize| {
        let th = TAU * (j % n) as f64 / n as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut f = Vec::new();
    for w in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, j + 1), pt(rb, zb, j), pt(rb, zb, j + 1));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// The app suites' turned bar: wall r6, shoulder, neck r5.
pub const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

pub fn bar() -> Vec<Triangle> {
    swept(16, &MERIDIAN)
}

/// A coaxial round bar of radius `rho` (the app suites clamp r6.2).
pub fn stock(rho: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    swept(32, &[(0.0, z0), (rho, z0), (rho, z1), (0.0, z1)])
}

pub fn params(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: D,
        stepover: 4.0,
        stock_to_leave: 0.5,
        clearance_z: 25.0,
        top_z: 14.0,
        bottom_z: -14.0,
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

pub fn control() -> AxisControl {
    AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None }
}

/// The developed path's own box, padded by the tool: the measuring
/// stock's footprint.
pub fn box_of(path: &Toolpath) -> Region {
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for m in &path.moves {
        lo = Point2::new(lo.x.min(m.to.x), lo.y.min(m.to.y));
        hi = Point2::new(hi.x.max(m.to.x), hi.y.max(m.to.y));
    }
    let (a, b) = (Point2::new(lo.x - R, lo.y - R), Point2::new(hi.x + R, hi.y + R));
    let Ok(l) = Loop::new(vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)]) else { panic!("box") };
    let Ok(rg) = Region::new(vec![l]) else { panic!("box region") };
    rg
}

/// Per developed level (its radius): planar feed moves, arcs, the worst
/// load (ae/D) read on the dexel stock, in-cut breaks past 5 degrees,
/// and whether a note names it ring-cut.
pub struct LevelRead {
    pub rho: f64,
    pub moves: usize,
    pub arcs: usize,
    pub load: f64,
    pub breaks: usize,
    pub named: bool,
}

/// The note every ring-cut developed level carries (seam, planner
/// fallback, hard planner refusal alike).
pub fn ring_tag(rho: f64) -> String {
    format!("Offset rings cut the developed level at radius {rho:.3} instead")
}

impl LevelRead {
    /// Cut by rings, read from the program's SHAPE: planar moves and not
    /// one proved arc (the HSM plan's arc output; rings post lines). A
    /// skin level (zero depth) counts only when named.
    pub fn ringed(&self, skin: f64) -> bool {
        self.moves > 0 && self.arcs == 0 && (self.rho < skin - 1e-6 || self.named)
    }
}

/// Every level rings cut is named by its radius, and every named level
/// is cut by rings (no proved arc).
pub fn assert_refusals_named(reads: &[LevelRead], skin: f64) {
    for r in reads {
        if r.ringed(skin) {
            assert!(r.named, "radius {:.3} is cut by rings without a note naming it", r.rho);
        }
        if r.named {
            assert_eq!(r.arcs, 0, "radius {:.3} is named ring-cut yet posts HSM arcs", r.rho);
        }
    }
}

/// Read per developed level of the ladder `top`, `top - step`, ...
/// (issue #983: only the ladder's own planes - a microlift's lifted run
/// whose down ramp passes a vertex on its height is no level, and read
/// as one it looked like a ring-cut level no note named).
pub fn read_levels(path: &Toolpath, top: f64, floor: f64, step: f64) -> Vec<LevelRead> {
    let mut spec = QualitySpec::new(D);
    spec.cell = D / 60.0;
    spec.break_deg = 5.0;
    let Ok(mut dex) = stock_for(&box_of(path), top, floor, 1.0, &spec) else { panic!("stock") };
    let q = measure(path, &mut dex, &spec).unwrap();
    // The reader's own bore law (`quality::measure`'s `bores`, as the
    // planar suite pocket3d_hsm reads it): a planar cut inside a helical
    // descent's circle is the bore's cleanup - entry engagement, not the
    // clearing's load.
    let bores: Vec<(Point2, f64)> = path
        .moves
        .iter()
        .filter(|m| (m.to.z - m.from.z).abs() > 1e-9)
        .filter_map(|m| m.arc.map(|a| (a.center, (m.from.x - a.center.x).hypot(m.from.y - a.center.y))))
        .collect();
    let in_bore = |m: &camcraft_motion::cl::Move| {
        bores.iter().any(|&(c, rr)| {
            (m.from.x - c.x).hypot(m.from.y - c.y) <= rr + 1e-6 && (m.to.x - c.x).hypot(m.to.y - c.y) <= rr + 1e-6
        })
    };
    let at = |m: &camcraft_motion::cl::Move| m.kind == MoveKind::Feed && m.from.z == m.to.z;
    let mut zs: Vec<f64> = path.moves.iter().filter(|m| at(m)).map(|m| m.to.z).collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup();
    // The CUT levels only: a plane some cutting descent (plunge, helix,
    // ramp, microlift return) ends on (to the micron: the ladder's level
    // and its descent's end differ in the last bit). A microlift's lifted
    // run is planar too, but nothing descends onto its height.
    zs.retain(|&z| path.moves.iter().any(|m| m.kind != MoveKind::Rapid && (m.to.z - z).abs() < 1e-6 && m.from.z > z + 1e-6));
    let on_ladder = |z: f64| {
        let k = (top - z) / step;
        step > 0.0 && (k - k.round()).abs() * step < 1e-5
    };
    zs.retain(|&z| on_ladder(z));
    zs.iter()
        .map(|&z| {
            let idx: Vec<usize> =
                path.moves.iter().enumerate().filter(|(_, m)| at(m) && m.to.z == z).map(|(i, _)| i).collect();
            let deg = idx.iter().filter(|&&i| !in_bore(&path.moves[i])).filter_map(|&i| q.move_max.get(i)).copied().fold(0.0, f64::max);
            let tag = ring_tag(z);
            LevelRead {
                rho: z,
                moves: idx.len(),
                arcs: idx.iter().filter(|&&i| path.moves[i].arc.is_some()).count(),
                load: load_at_rim(deg, D, q.rim),
                breaks: q.break_list.iter().filter(|(p, t)| p.z == z && *t > 5.0).count(),
                named: path.notes.iter().any(|n| n.ends_with(&tag)),
            }
        })
        .collect()
}

/// The planner's own overload notes: its worst load past the law, or a
/// spiral step out of the entry bore that could not hold it.
pub fn planner_overloads(notes: &[String]) -> Vec<&String> {
    notes.iter().filter(|n| n.contains("no spiral step holds the load") || n.contains("worst planned load")).collect()
}

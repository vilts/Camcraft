//! The independent reading of a rest Adaptive path (issue 2031): the
//! earlier operation simulated on a column stock, the stock it proves,
//! and the laws a rest path is held to against that simulation.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::load::{load_at_rim, DEFAULT_OPTIMAL_LOAD};
use camcraft_motion::machining2d::{pocket, GenError, PocketParams, PocketStrategy, RestStock, StockFloor};
use camcraft_motion::quality::{measure, rest_area, stock_for, QualitySpec};
use camcraft_motion::stock_sim::{Columns, Stock};
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

/// The earlier (roughing) tool and this operation's tool.
pub const PREV_D: f64 = 12.0;
pub const TOOL_D: f64 = 4.0;
pub const R: f64 = TOOL_D * 0.5;
/// The stock's top, and the two cut depths both operations run.
pub const TOP: f64 = 0.0;
pub const FIRST: f64 = -2.0;
pub const BOTTOM: f64 = -4.0;
/// The measured load may read this far over the optimal load: the
/// planner's own margin (`pocket_hsm::PLAN_TOL`). The stock is measured
/// on the planner's grid (D/120, the heavy lane's): the contact angle is
/// read on a rim a cell and a half outside the cutter, which in a
/// concave corner stands in more wall the coarser the grid is (the same
/// paths read up to 17.4% of D on the default D/80 grid, 12.5 to 14.7%
/// on this one - the planner's own numbers to a tenth of a percent).
pub const LOAD_SLACK: f64 = 0.05;

pub fn poly(pts: &[(f64, f64)]) -> Region {
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

/// `region` with its inside corners rounded to `cr`.
pub fn filleted(region: &Region, cr: f64) -> Region {
    region.offset(-cr).unwrap().offset(cr).unwrap()
}

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

pub fn disc(c: Point2, r: f64) -> Region {
    let pts: Vec<(f64, f64)> = (0..128)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 128.0;
            (c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    poly(&pts)
}

pub fn pp(d: f64, strategy: PocketStrategy, bottom: f64) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: d,
        stepover: d * 0.4,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: FIRST,
        bottom_z: bottom,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

pub fn rest_params() -> PocketParams {
    pp(TOOL_D, PocketStrategy::Adaptive, BOTTOM)
}

pub fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// The independent stock: a column grid of the pocket, cut by whatever
/// path it is handed.
pub struct Sim {
    pub dex: Stock,
    pub spec: QualitySpec,
}

impl Sim {
    pub fn new(region: &Region) -> Sim {
        // (The part outside the pocket stands as wall, a millimeter under
        // the floor, the way `quality::stock_for` builds it; the grid is
        // the planner's, see `LOAD_SLACK`.)
        let spec = QualitySpec { wall_below: Some(BOTTOM - 0.5), cell: TOOL_D / 120.0, ..QualitySpec::new(TOOL_D) };
        Sim { dex: stock_for(region, TOP, BOTTOM, 1.0, &spec).unwrap(), spec }
    }

    pub fn cut(&mut self, path: &Toolpath, r: f64) {
        for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            for (a, b) in m.segments3(0.005) {
                self.dex.cut_chord(a, b, r).unwrap();
            }
        }
    }

    /// A pillar the earlier operations left standing (put back).
    pub fn stand(&mut self, at: Point2, r: f64) {
        self.dex.set_tops(|_, c, top| if dist(c, at) <= r { TOP } else { top }).unwrap();
    }

    /// The highest stock under a disc of radius `rad` at `q`.
    pub fn top_under(&self, q: Point2, rad: f64) -> f64 {
        let cols: Columns<'_> = self.dex.columns().unwrap();
        let cell = cols.cell();
        let o = cols.origin();
        let span = |v: f64, o: f64, n: usize| {
            let lo = (((v - rad - o) / cell).floor().max(0.0) as usize).min(n.saturating_sub(1));
            let hi = (((v + rad - o) / cell).ceil().max(0.0) as usize).min(n.saturating_sub(1));
            lo..=hi
        };
        let mut top = f64::NEG_INFINITY;
        for iy in span(q.y, o.y, cols.ny()) {
            for ix in span(q.x, o.x, cols.nx()) {
                if dist(cols.center(ix, iy), q) <= rad {
                    top = top.max(cols.top_at(ix, iy));
                }
            }
        }
        top
    }
}

/// The roughing pass of the earlier tool, cut into `sim`, and the stock
/// it PROVES: per roughing depth, the pocket less what its feeds at that
/// depth or deeper swept (the sweep read a hundredth small, so chord
/// error only adds material). Checked against the simulation: nothing
/// the simulation still holds stands outside the proved bound.
pub fn roughed(region: &Region, sim: &mut Sim, bottom: f64) -> RestStock {
    let rough = pocket(region, &pp(PREV_D, PocketStrategy::Waterline, bottom)).unwrap();
    sim.cut(&rough, PREV_D * 0.5);
    let flat = |m: &&Move| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9;
    let mut zs: Vec<f64> = rough.moves.iter().filter(flat).map(|m| m.to.z).collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    let floors: Vec<StockFloor> = zs
        .iter()
        .map(|&z| {
            let walks: Vec<Vec<Point2>> = rough
                .moves
                .iter()
                .filter(flat)
                .filter(|m| m.to.z <= z + 1e-9)
                .flat_map(|m| m.chords(0.002))
                .map(|(a, b)| vec![a, b])
                .collect();
            let cleared = Region::sweep(&walks, PREV_D * 0.5 - 0.01).unwrap();
            StockFloor { z, remaining: region.difference(&cleared).unwrap(), uncertain: Region::empty() }
        })
        .collect();
    let cols = sim.dex.columns().unwrap();
    let inside = region.offset(-0.1).unwrap();
    for f in &floors {
        let bound = f.remaining.offset(0.1).unwrap();
        let mut outside = 0usize;
        for iy in 0..cols.ny() {
            for ix in 0..cols.nx() {
                let c = cols.center(ix, iy);
                if cols.top_at(ix, iy) > f.z + 1e-3 && inside.contains(c) && !bound.contains(c) {
                    outside += 1;
                }
            }
        }
        assert_eq!(outside, 0, "the proved stock at Z{} reads smaller than the simulated stock", f.z);
    }
    RestStock::from_verified(floors).unwrap()
}

pub fn check(path: &Toolpath, region: &Region) -> Vec<Warning> {
    let t = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &t,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: TOP,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

/// The only rapid allowed below the top of the stock: straight up, out
/// of where the tool has just cut.
pub fn lifts_out(m: &Move) -> bool {
    m.from.x == m.to.x && m.from.y == m.to.y && m.to.z > m.from.z
}

/// What a rest path did, read on the simulated stock the earlier
/// operations left (`sim`, which this cuts).
pub struct Reading {
    /// Worst load (ae/D) of the planar cuts at each requested depth.
    pub load: Vec<(f64, f64)>,
    /// Stock left standing above the bottom inside the tool's reach (mm^2).
    pub left: f64,
    pub plunges: usize,
}

/// The laws every rest Adaptive path is held to: the verifier passes;
/// no rapid meets material; every plunge goes down a column the EARLIER
/// operations cleared for the whole cutter footprint (this operation's
/// own cuts are not credited); the load holds at every depth.
pub fn read(path: &Toolpath, region: &Region, sim: &mut Sim) -> Reading {
    read_leaving(path, region, sim, 0.0)
}

/// [`read`] for a path that leaves `leave` mm on the walls.
pub fn read_leaving(path: &Toolpath, region: &Region, sim: &mut Sim, leave: f64) -> Reading {
    assert_eq!(check(path, region), Vec::new(), "verifier");
    assert!(path.is_chained() && path.all_finite());
    let mut plunges = 0;
    for (k, m) in path.moves.iter().enumerate() {
        match m.kind {
            MoveKind::Rapid if lifts_out(m) => {}
            MoveKind::Rapid => {
                let low = m.from.z.min(m.to.z);
                assert!(low >= TOP - 1e-9, "move {k}: a rapid below the top of the stock: {m:?}");
                for (a, b) in m.chords(0.5) {
                    for q in [a, b] {
                        assert!(sim.top_under(q, R + 0.05) <= low + 1e-6, "move {k}: a rapid over stock that stands higher");
                    }
                }
            }
            MoveKind::Plunge => {
                plunges += 1;
                assert!(m.from.x == m.to.x && m.from.y == m.to.y, "move {k}: a plunge that is not straight down");
                let at = Point2::new(m.to.x, m.to.y);
                let under = sim.top_under(at, R + 0.05);
                assert!(under <= m.to.z + 0.01, "move {k}: a plunge to Z{} over stock standing to Z{under} under the cutter", m.to.z);
            }
            MoveKind::Feed => assert!((m.from.z - m.to.z).abs() < 1e-9, "move {k}: a feed that is not planar"),
        }
    }
    let q = measure(path, &mut sim.dex, &sim.spec).unwrap();
    assert_eq!(q.descents, 0, "a descent bit stock");
    assert!(q.entry_max_deg < 1.0, "a plunge read {} degrees of contact", q.entry_max_deg);
    assert_eq!(q.slot_len, 0.0, "slotting");
    let load = [FIRST, BOTTOM]
        .iter()
        .map(|&z| {
            let at_depth = path.moves.iter().zip(&q.move_max).filter(|(m, _)| m.kind == MoveKind::Feed && (m.to.z - z).abs() < 1e-9);
            let deg = at_depth.map(|(_, &d)| d).fold(0.0f64, f64::max);
            (z, load_at_rim(deg, TOOL_D, q.rim))
        })
        .collect();
    // In reach: what a 4 mm tool's edge gets to, a tenth short of it.
    let reach = region.offset(-(R + leave)).unwrap().offset(R - 0.1).unwrap();
    Reading { load, left: rest_area(&sim.dex, &reach, BOTTOM).unwrap(), plunges }
}

pub fn assert_load_held(r: &Reading) {
    for &(z, load) in &r.load {
        assert!(load > 0.02, "Z{z}: nothing was cut (worst load {:.1}% of D)", load * 100.0);
        assert!(
            load <= DEFAULT_OPTIMAL_LOAD * (1.0 + LOAD_SLACK),
            "Z{z}: the measured load {:.1}% of D is past the optimal {:.1}%",
            load * 100.0,
            DEFAULT_OPTIMAL_LOAD * 100.0
        );
    }
}

pub fn refusal(r: Result<Toolpath, GenError>) -> String {
    match r {
        Err(e @ (GenError::Invalid(_) | GenError::At { .. })) => e.to_string(),
        other => panic!("expected a named refusal, got {other:?}"),
    }
}

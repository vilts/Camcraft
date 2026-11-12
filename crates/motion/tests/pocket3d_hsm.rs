//! The 3-D rough under HSM Adaptive (M11.4 owed "Pocket3D port of the
//! same planner"): a stepped block - the `adaptive_hsm` suite's 60 x 40
//! rectangle floored at Z -10 with its 12 x 12 island as a boss standing
//! to Z -7.5, roughed with a D10 in two 5 mm levels, the lower one
//! around the boss as a computed island (the planner's own verdicts on
//! those two areas: the rectangle holds every law, the island is refused
//! by name - RECOMMENDED "M11.4 C round 5"). Every level
//! verifies clean under the depth-aware law against the part solid; the
//! load each HSM level reads on the dexel stock (the measuring law of
//! `adaptive_hsm`) holds the planner's bound; no in-cut break past 5
//! degrees rides an HSM level; a level the planner refuses is cut by
//! Offset rings and SAID with its Z; and `OffsetRings` is the ring
//! tiling at the load step every earlier 3-D Adaptive posted, move for
//! move.
//!
//! Gate map (M4.4): covers camcraft_motion::machining3d (hsm, ladder,
//! level_ctx), camcraft_motion::machining2d (adaptive, pocket_hsm,
//! arc_proof, arc_cross, microlift), camcraft_motion::quality.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::load::{DEFAULT_OPTIMAL_LOAD, load_at_rim};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, engagement_step, pocket3d};
use camcraft_motion::quality::{QualitySpec, measure, stock_for};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const D: f64 = 10.0;
const LEVELS: [f64; 2] = [-5.0, -10.0];

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(l) = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) else {
        panic!("fixture loop")
    };
    let Ok(r) = Region::new(vec![l]) else { panic!("fixture region") };
    r
}

/// A solid axis-aligned box as a facet soup, outward CCW (the
/// `machining3d` suite's fixture).
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

/// The stepped block: the slab's floor at -10, the boss to -7.5 (between
/// the two planes: an island at -10 only, a staircase above it).
fn stepped() -> Vec<Triangle> {
    let mut f = box_facets([-30.0, -20.0, -20.0], [30.0, 20.0, -10.0]);
    f.extend(box_facets([-6.0, -6.0, -10.0], [6.0, 6.0, -7.5]));
    f
}

fn footprint() -> Region {
    rect(-30.0, -20.0, 30.0, 20.0)
}

/// The 2D suite's debug fixture, the user's R12.5 bore, as a footprint.
fn bore() -> Region {
    let n = 256;
    let pts: Vec<Point2> = (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            Point2::new(12.5 * a.cos(), 12.5 * a.sin())
        })
        .collect();
    let Ok(l) = Loop::new(pts) else { panic!("bore loop") };
    let Ok(r) = Region::new(vec![l]) else { panic!("bore region") };
    r
}

fn params(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: D,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        // The stock's own top (`read_levels` stocks from Z 0): the
        // first level is still the 5 mm cut at -5 (#96 review - a top
        // of -5 rapided through the 0..-5 the reader declares).
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        retract_z: Some(1.0),
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

fn verified(facets: &[Triangle], region: &Region, path: &Toolpath) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: D * 0.5,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: D * 0.5,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        // The load reader's stock (`read_levels`): Z 0. The op's top
        // stands on it too (#96), so the plunge-height rapid ends over
        // the block, never inside it.
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
    verify(path, &cx)
}

/// Per level: planar feed moves, the worst load (ae/D) the measuring law
/// read on them, in-cut breaks past 5 degrees, and whether the planner
/// fell back to rings there.
struct LevelRead {
    z: f64,
    moves: usize,
    arcs: usize,
    load: f64,
    breaks: usize,
    fell: bool,
}

fn read_levels(path: &Toolpath, region: &Region) -> Vec<LevelRead> {
    let mut spec = QualitySpec::new(D);
    // The debug lane's grid (twice as coarse as the planner's); the
    // heavy lane reads the full resolution.
    spec.cell = D / 60.0;
    spec.break_deg = 5.0;
    // Stock from Z 0: both planes are real 5 mm cuts, the boss column
    // standing at -5 after the first (the island the second rings).
    let Ok(mut dex) = stock_for(region, 0.0, -10.0, 1.0, &spec) else { panic!("stock") };
    let q = measure(path, &mut dex, &spec).unwrap();
    // The reader's own bore law: a planar cut inside a helical descent's
    // circle is the bore's cleanup, read as entry engagement - not the
    // clearing's load (`quality::measure`'s `bores`).
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
    LEVELS
        .iter()
        .map(|&z| {
            let at = |m: &camcraft_motion::cl::Move| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z;
            let idx: Vec<usize> = path.moves.iter().enumerate().filter(|(_, m)| at(m)).map(|(i, _)| i).collect();
            let deg = idx.iter().filter(|&&i| !in_bore(&path.moves[i])).filter_map(|&i| q.move_max.get(i)).copied().fold(0.0, f64::max);
            let breaks = q.break_list.iter().filter(|(p, t)| p.z == z && *t > 5.0).count();
            let fell = path.notes.iter().any(|n| n.contains(&format!("Offset rings cut the level at Z {z:.3} instead")));
            LevelRead {
                z,
                moves: idx.len(),
                arcs: idx.iter().filter(|&&i| path.moves[i].arc.is_some()).count(),
                load: load_at_rim(deg, D, q.rim),
                breaks,
                fell,
            }
        })
        .collect()
}

/// The whole stepped block: two HSM plans on the 60 x 40 areas (minutes
/// in debug), the heavy lane's - the 2D suite keeps its island there too.
#[test]
#[ignore = "heavy lane: two HSM level plans on the 60 x 40 stepped block (release)"]
fn the_stepped_block_holds_the_planner_laws_level_by_level() {
    let f = stepped();
    let path = match pocket3d(&f, &footprint(), &params(PocketStrategy::Adaptive)) {
        Ok(p) => p,
        Err(e) => panic!("adaptive 3-D rough: {e}"),
    };
    let warns = verified(&f, &footprint(), &path);
    assert!(warns.is_empty(), "every level verifies clean: {warns:?}\nnotes {:?}", path.notes);
    assert!(path.is_chained() && path.all_finite());
    let reads = read_levels(&path, &footprint());
    for r in &reads {
        println!(
            "Z {:.1}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, fell back {}",
            r.z,
            r.moves,
            r.arcs,
            r.load * 100.0,
            r.breaks,
            r.fell
        );
    }
    println!("notes: {:?}", path.notes);
    // The open level (-5): the HSM plan itself - one star cell, its
    // spiral never refused.
    let open = &reads[0];
    assert!(!open.fell, "the open level is the planner's: {:?}", path.notes);
    assert!(open.arcs > 0, "the plan posts proved arcs");
    for r in reads.iter().filter(|r| !r.fell) {
        assert!(r.moves > 0, "Z {}: the level cuts", r.z);
        // The planner's law on the debug grid (the heavy lane's 10%
        // margin reads on the planner's own grid, `adaptive_hsm`).
        assert!(
            r.load <= DEFAULT_OPTIMAL_LOAD * 1.35,
            "Z {}: load {:.1}% D past {:.1}% + 35%",
            r.z,
            r.load * 100.0,
            DEFAULT_OPTIMAL_LOAD * 100.0
        );
        assert_eq!(r.breaks, 0, "Z {}: in-cut breaks past 5 deg", r.z);
    }
    // A refused level is said by name, with its Z - never silently
    // swapped: the fallback note is the only way a level cuts rings.
    for r in reads.iter().filter(|r| r.fell) {
        assert!(path.notes.iter().any(|n| n.contains(&format!("Z {:.3}", r.z)) && n.contains("Offset rings")));
    }
    // Pinned (M11.4): the island level falls back by name - the planner
    // still refuses an island region (RECOMMENDED "M11.4 C round 5").
    let island = &reads[1];
    assert!(island.fell, "the island level's verdict changed - re-pin: {:?}", path.notes);
    // The planner's verdict on this island area is the crossing law
    // (the verifier's, asked of the plan first); the sentence names the
    // level's Z so the status line reads which level cut rings.
    // Since #13 the growth budget refuses this island before the crossing
    // law is asked ("ran past its planning budget"); either named reason
    // pins the law - the level falls back by name with its Z.
    assert!(
        path.notes.iter().any(|n| (n.starts_with("adaptive: the HSM plan for this area crosses itself at (")
            || n.starts_with("adaptive: the HSM plan is refused: growing the cleared area ran past its planning budget"))
            && n.ends_with(" - Offset rings cut the level at Z -10.000 instead")),
        "{:?}",
        path.notes
    );
    // And the rings' level is never read as the planner's: its corners
    // break and its first ring slots, as rings always have.
    assert!(island.arcs == 0 && island.moves > 0);
}

/// The debug lane: one open level over the slab, the 2D suite's own
/// debug fixture (the user's R12.5 bore) as the footprint, the
/// planner's laws read on it through the 3-D ladder.
#[test]
fn an_open_level_holds_the_planner_laws() {
    let f = stepped();
    let reg = bore();
    let p = Pocket3DParams { bottom_z: -5.0, ..params(PocketStrategy::Adaptive) };
    let path = match pocket3d(&f, &reg, &p) {
        Ok(p) => p,
        Err(e) => panic!("adaptive 3-D rough: {e}"),
    };
    let warns = verified(&f, &reg, &path);
    assert!(warns.is_empty(), "the level verifies clean: {warns:?} notes {:?}", path.notes);
    assert!(path.is_chained() && path.all_finite());
    let reads = read_levels(&path, &reg);
    let r = &reads[0];
    println!("Z {:.1}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, fell back {}", r.z, r.moves, r.arcs, r.load * 100.0, r.breaks, r.fell);
    println!("notes: {:?}", path.notes);
    assert!(!r.fell && r.arcs > 0 && r.moves > 0, "{:?}", path.notes);
    assert!(r.load <= DEFAULT_OPTIMAL_LOAD * 1.35, "load {:.1}% D past the law", r.load * 100.0);
    assert_eq!(r.breaks, 0, "in-cut breaks past 5 deg");
    assert_eq!(reads[1].moves, 0, "nothing below the one level");
}

#[test]
fn offset_rings_are_the_waterline_tiling_at_the_load_step() {
    // The default stays what it was: `OffsetRings` IS the ring tiling
    // every earlier 3-D Adaptive posted, move for move.
    let f = stepped();
    let rings = pocket3d(&f, &footprint(), &params(PocketStrategy::OffsetRings)).expect("rings");
    let step = engagement_step(D * 0.5, 45.0);
    let water = pocket3d(&f, &footprint(), &Pocket3DParams { strategy: PocketStrategy::Waterline, stepover: step, ..params(PocketStrategy::Waterline) }).expect("waterline");
    assert_eq!(rings.moves, water.moves, "one law, one geometry");
    assert!(rings.notes.is_empty(), "{:?}", rings.notes);
    assert!(verified(&f, &footprint(), &rings).is_empty());
}

#[test]
fn the_rest_pass_and_the_shell_keep_rings_by_name() {
    use camcraft_motion::machining3d::{REST_RINGS, SHELL_RINGS, pocket3d_offset, pocket3d_with_rest};
    // The bore footprint: the rings and the rest sim in seconds (the
    // 60 x 40 block's rest ladder is minutes of simulation).
    let f = stepped();
    let reg = bore();
    let p = params(PocketStrategy::Adaptive);
    let (t, _) = pocket3d_with_rest(&f, &reg, &p).expect("rest");
    assert!(t.notes.iter().any(|n| n == REST_RINGS), "{:?}", t.notes);
    let s = pocket3d_offset(&f, &reg, &p).expect("shell");
    assert!(s.notes.iter().any(|n| n == SHELL_RINGS), "{:?}", s.notes);
    assert!(s.is_chained() && s.all_finite());
}

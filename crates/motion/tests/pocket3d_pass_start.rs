//! #3361: Pass starts at (Auto / Inside / Outside) on the 3-D pocket. One
//! level of a 40 mm square pocket around a 10 mm square island, a tool
//! of radius 3. The cutter-centre band of the level runs from the island
//! offset (Chebyshev distance 8 from the island centre, 7.1 at its
//! rounded corners) to the pocket wall offset (distance 17). Inside
//! starts on the island ring and walks out; Outside starts on the wall
//! ring and walks in; Auto keeps the core-first order of the ring
//! strategies, which starts in the middle of the band. Zigzag rows have
//! no ring order: a forced value changes no move and is said by name.
//!
//! Gate map: covers camcraft_motion::machining3d (hsm, ladder, params)
//! and camcraft_motion::machining2d (level_rings, level_order,
//! pass_start), camcraft_motion::plunge_law (#3670 re-entries).
//! Provenance: our own regression for #3361; no vendor material
//! consulted.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{PASS_START_ZIGZAG_NOTE, PassStart, PocketStrategy, RINGS_FALLBACK};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d};

const R: f64 = 3.0;
/// The island centre.
const C: f64 = 20.0;

fn square(x0: f64, y0: f64, s: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x0 + s, y0),
        Point2::new(x0 + s, y0 + s),
        Point2::new(x0, y0 + s),
    ]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

/// The closed box `lo..hi`, outward normals.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| Point3::new([lo[0], hi[0]][i & 1], [lo[1], hi[1]][(i >> 1) & 1], [lo[2], hi[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

/// A floor plate under the pocket and the island that stands through
/// every plane of the ladder.
fn model() -> Vec<Triangle> {
    let mut f = cuboid([-30.0, -30.0, -5.0], [70.0, 70.0, -1.0]);
    f.extend(cuboid([15.0, 15.0, -5.0], [25.0, 25.0, 20.0]));
    f
}

fn params(strategy: PocketStrategy, pass_start: PassStart) -> Pocket3DParams {
    Pocket3DParams {
        links: Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 2.4,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        top_z: 13.0,
        bottom_z: 0.0,
        stepdown: 50.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        pass_start,
    }
}

fn run(strategy: PocketStrategy, start: PassStart) -> Toolpath {
    pocket3d(&model(), &square(0.0, 0.0, 40.0), &params(strategy, start)).expect("3D pocket")
}

/// The Chebyshev distance of `q` from the island centre.
fn cheb(q: Point3) -> f64 {
    (q.x - C).abs().max((q.y - C).abs())
}

/// The end points of the cutting feeds on the one level (Z 0), in cut
/// order.
fn level_cuts(t: &Toolpath) -> Vec<Point3> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.to.z.abs() < 1e-6 && m.from.z.abs() < 1e-6)
        .map(|m| m.to)
        .collect()
}

/// The mean distance of the first and of the last fifth of the cuts.
fn ends(cuts: &[Point3]) -> (f64, f64) {
    let n = (cuts.len() / 5).max(1);
    let mean = |s: &[Point3]| s.iter().map(|&q| cheb(q)).sum::<f64>() / s.len() as f64;
    (mean(&cuts[..n]), mean(&cuts[cuts.len() - n..]))
}

fn no_fallback(what: &str, t: &Toolpath) {
    assert!(!t.notes.iter().any(|n| n.contains(RINGS_FALLBACK)), "{what}: no completeness fallback: {:?}", t.notes);
}

/// Inside starts on the island ring and walks out to the wall; Outside
/// starts on the wall ring and walks in to the island; Auto starts in
/// the middle of the band (the core-first order of every earlier file).
/// Each value stays in the band: no cut leaves the cutter-centre area.
#[test]
fn inside_and_outside_force_the_ring_order_of_a_3d_level() {
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings] {
        let auto = run(strategy, PassStart::Auto);
        let inside = run(strategy, PassStart::Inside);
        let outside = run(strategy, PassStart::Outside);
        for (what, t) in [("auto", &auto), ("inside", &inside), ("outside", &outside)] {
            let cuts = level_cuts(t);
            assert!(cuts.len() > 10, "{strategy:?} {what}: the level cuts: {}", cuts.len());
            for &q in &cuts {
                let d = cheb(q);
                assert!((7.0..=17.0 + 1e-6).contains(&d), "{strategy:?} {what}: cut {q:?} at {d} is off the band");
            }
            no_fallback(&format!("{strategy:?} {what}"), t);
        }
        let first = |t: &Toolpath| level_cuts(t).first().map_or(f64::NAN, |&q| cheb(q));
        let (fi, fo, fa) = (first(&inside), first(&outside), first(&auto));
        assert!(fi <= 8.0 + 1e-6, "{strategy:?}: Inside starts on the island ring, at {fi}");
        assert!((fo - 17.0).abs() < 1e-6, "{strategy:?}: Outside starts on the wall ring, at {fo}");
        assert!(fa > 8.5 && fa < 16.5, "{strategy:?}: Auto starts inside the band (core first), at {fa}");
        let (i0, i1) = ends(&level_cuts(&inside));
        assert!(i0 < i1, "{strategy:?}: Inside walks out: {i0} -> {i1}");
        let (o0, o1) = ends(&level_cuts(&outside));
        assert!(o0 > o1, "{strategy:?}: Outside walks in: {o0} -> {o1}");
        assert_ne!(auto.moves, inside.moves, "{strategy:?}: Inside changes the order");
        assert_ne!(auto.moves, outside.moves, "{strategy:?}: Outside changes the order");
    }
}

/// Zigzag (and Auto, which cuts its rows) has no ring order: Inside and
/// Outside post the very moves of Auto and say so by name, once.
#[test]
fn zigzag_keeps_its_rows_and_names_the_unused_choice() {
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Auto] {
        let auto = run(strategy, PassStart::Auto);
        assert!(!auto.notes.iter().any(|n| n == PASS_START_ZIGZAG_NOTE), "{strategy:?}: Auto says nothing");
        for start in [PassStart::Inside, PassStart::Outside] {
            let t = run(strategy, start);
            assert_eq!(t.moves, auto.moves, "{strategy:?} {start:?}: the rows do not move");
            let said = t.notes.iter().filter(|n| *n == PASS_START_ZIGZAG_NOTE).count();
            assert_eq!(said, 1, "{strategy:?} {start:?}: said once: {:?}", t.notes);
        }
    }
}

/// #3670: the zigzag rows split at the island, so each row restarts on
/// its far side after a hop. With a Helix entry and a centre-cutting
/// tool, no restart feeds straight down into uncut stock: it takes the
/// helix (or the ramp along the pass), never the plain plunge.
#[test]
fn zigzag_helix_re_entries_never_plunge_into_uncut_stock() {
    use camcraft_motion::plunge_law::{Solid, first_solid_plunge};
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Auto] {
        let p = Pocket3DParams { entry: Entry3D::Helix, ..params(strategy, PassStart::Auto) };
        assert!(p.links.approach.center_cutting, "the fixture tool cuts at its centre");
        let t = pocket3d(&model(), &square(0.0, 0.0, 40.0), &p).expect("3D pocket");
        let hops = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid && m.to.z > m.from.z).count();
        assert!(hops > 2, "{strategy:?}: the rows restart after hops ({hops})");
        let solid = Solid { top: p.top_z, footprint: None };
        assert_eq!(first_solid_plunge(&t, R, &solid), None, "{strategy:?}: a restart plunges into stock: {:?}", t.notes);
    }
}

/// #3852: with the Plunge entry, each zigzag row that restarts after a
/// hop goes down by a rapid to the plunge height over the material, then
/// feeds the rest. No plunge feeds from the hop plane: none is longer
/// than the plunge height plus the stepdown.
#[test]
fn zigzag_plunge_restarts_feed_only_from_the_plunge_height() {
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Auto] {
        let p = Pocket3DParams { stepdown: 6.5, ..params(strategy, PassStart::Auto) };
        let h = p.links.approach.plunge_height;
        assert!(h > 0.0 && h < p.clearance_z - p.top_z, "the fixture tells the plunge height from the hop plane");
        let t = pocket3d(&model(), &square(0.0, 0.0, 40.0), &p).expect("3D pocket");
        let plunges: Vec<_> = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).collect();
        assert!(plunges.len() > 2, "{strategy:?}: the rows restart by plunges ({})", plunges.len());
        for m in plunges {
            let len = m.from.z - m.to.z;
            assert!(len <= h + p.stepdown + 1e-6, "{strategy:?}: a plunge of {len} mm feeds from the hop plane: {m:?}");
        }
    }
}

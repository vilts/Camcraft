//! Unit laws of `thread_boss` (#985): the boss door, the crest region
//! and the neighbour law on a hand-built boss.
//! Provenance: our own fixtures (an M8 x 1.25 boss, ISO 965-1 depth).

use super::*;
use crate::cl::ThreadHand;
use crate::machining2d::{self, ThreadHole, ThreadParams};
use crate::operation::Height;
use crate::tools::catalog;

/// M8 x 1.25's external depth: 0.6134 x 1.25.
const H: f64 = 0.766_75;

fn mill(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog thread mill")
}

fn boss(radius: f64, top: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: top, floor_z: 10.0, through: false, stepped: false }
}

fn external(bosses: Vec<Hole>) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        infeed: 0.25,
        feed: 300.0,
        plunge_feed: 150.0,
        climb: true,
        holes: bosses,
        pitch: None,
        hand: ThreadHand::Right,
        external: true,
        passes: crate::machining2d::ThreadPasses::default(),
    }
}

/// The kernel's ride round the M8 boss with the Ø6 multi-form mill.
fn ride() -> Toolpath {
    machining2d::thread_mill(&ThreadParams {
        holes: vec![ThreadHole { xy: Point2::new(0.0, 0.0), radius: 4.0, top_z: 20.0, floor_z: 10.0, stepped: false, through: false }],
        pitch: 1.25,
        tool_diameter: 6.0,
        rows: 10,
        infeed: 0.25,
        external: true,
        hand: ThreadHand::Right,
        climb: true,
        passes: crate::machining2d::ThreadPasses::default(),
        clearance_z: 30.0,
        r_z: None,
        retract_z: None,
        feed: 300.0,
        plunge_feed: 150.0,
    })
    .expect("the external ride")
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// The boss's own wall and top (48 flats) on a 60 x 60 shoulder at z 10.
fn part() -> Vec<Triangle> {
    let n = 48;
    let at = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % n) as f64 / n as f64;
        Point3::new(4.0 * a.cos(), 4.0 * a.sin(), z)
    };
    let mut f = Vec::new();
    for k in 0..n {
        quad(at(k, 10.0), at(k + 1, 10.0), at(k + 1, 20.0), at(k, 20.0), &mut f);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 20.0), at(k, 20.0), at(k + 1, 20.0)));
    }
    let p = |x: f64, y: f64| Point3::new(x, y, 10.0);
    quad(p(-30.0, -30.0), p(30.0, -30.0), p(30.0, 30.0), p(-30.0, 30.0), &mut f);
    f
}

/// A wall facing +X at `x`, from the shoulder up past the boss.
fn rib(x: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let p = |y: f64, z: f64| Point3::new(x, y, z);
    quad(p(-6.0, 10.0), p(6.0, 10.0), p(6.0, 22.0), p(-6.0, 22.0), &mut f);
    f
}

#[test]
fn the_door_names_a_boss_too_thin_and_a_thread_past_the_mills_reach() {
    let tm = mill("TM-0600-P125");
    assert_eq!(door(&tm, &[boss(4.0, 20.0)], Some(H), Some(None)), None, "M8 x 1.25, 10 mm long, 12.5 mm of teeth");
    let thin = door(&tm, &[boss(0.7, 20.0)], Some(H), Some(None)).unwrap_or_default();
    assert!(thin.contains("the 1.4 mm boss at (0, 0) is thinner than the 0.76675 mm thread is deep"), "{thin}");
    // 15 mm of thread under 12.5 mm of teeth: the Ø6 shank is wider than the tooth roots.
    let long = door(&tm, &[boss(4.0, 25.0)], Some(H), Some(None)).unwrap_or_default();
    assert!(long.contains("is 15 mm long") && long.contains("reaches only 12.5 mm") && long.contains("6 mm shank"), "{long}");
    // A stated bottom shortens it; an unresolved one leaves the reach to generate.
    assert_eq!(door(&tm, &[boss(4.0, 25.0)], Some(H), Some(Some(13.0))), None);
    assert_eq!(door(&tm, &[boss(4.0, 25.0)], Some(H), None), None);
    // The single-form mill's Ø4 neck clears the boss: its reach runs to the holder.
    let ts = mill("TMS-0600-P050-150");
    assert_eq!(door(&ts, &[boss(4.0, 45.0)], Some(H), Some(None)), None, "35 mm under 2 + 40 mm");
    let holder = door(&ts, &[boss(4.0, 60.0)], Some(H), Some(None)).unwrap_or_default();
    assert!(holder.contains("is 50 mm long") && holder.contains("reaches only 42 mm") && holder.contains("its holder"), "{holder}");
    // An unknown pitch judges nothing here: the generator names it.
    assert_eq!(door(&tm, &[boss(0.1, 20.0)], None, Some(None)), None);
}

#[test]
fn the_crest_region_is_each_boss_minor_circle_and_nothing_else() {
    let far = Hole { xy: Point2::new(30.0, 5.0), ..boss(5.0, 20.0) };
    let region = crest_region(&[boss(4.0, 20.0), far], H).expect("region");
    assert_eq!(region.loops().len(), 2);
    let reach = |c: Point2, l: &crate::geo2d::Loop| l.points().iter().map(|p| (p.x - c.x).hypot(p.y - c.y)).fold(0.0, f64::max);
    let radii: Vec<f64> = region.loops().iter().map(|l| reach(Point2::new(0.0, 0.0), l).min(reach(Point2::new(30.0, 5.0), l))).collect();
    assert!(radii.iter().any(|r| (r - (4.0 - H)).abs() < 2e-6) && radii.iter().any(|r| (r - (5.0 - H)).abs() < 2e-6), "{radii:?}");
}

#[test]
fn the_crest_ring_beside_a_free_boss_is_clean() {
    let tm = mill("TM-0600-P125");
    let warns = neighbours(&external(vec![boss(4.0, 20.0)]), &ride(), &part(), &tm);
    assert_eq!(warns, Vec::new(), "the boss's own wall is the crest law's, its shoulder the root law's");
    // Another kind of operation, or an internal thread, is not read at all.
    let mut internal = external(vec![boss(4.0, 20.0)]);
    if let OpParams::Thread { external, .. } = &mut internal {
        *external = false;
    }
    let mut walled = part();
    walled.extend(rib(-8.0));
    assert_eq!(neighbours(&internal, &ride(), &walled, &tm), Vec::new());
}

#[test]
fn a_wall_inside_the_crest_rings_sweep_is_a_gouge_with_no_thread_allowance() {
    let tm = mill("TM-0600-P125");
    let path = ride();
    let params = external(vec![boss(4.0, 20.0)]);
    // The first pass orbits at 4 + 3 - 0.25556 = 6.744: its crests reach 9.744 from the axis.
    let mut grazed = part();
    grazed.extend(rib(-9.6));
    let warns = neighbours(&params, &path, &grazed, &tm);
    assert!(!warns.is_empty(), "0.14 mm into the rib - far under the thread depth the root law grants");
    assert!(warns.iter().all(|w| matches!(w, Warning::Gouge { at } if path.moves[*at].arc.is_some())), "{warns:?}");
    let mut clear = part();
    clear.extend(rib(-9.8));
    assert_eq!(neighbours(&params, &path, &clear, &tm), Vec::new(), "0.056 mm of air is air");
    // A rib that is the SAME boss's skin by position but stands outside it is still a wall.
    let mut near = part();
    near.extend(rib(-4.2));
    assert!(!neighbours(&params, &path, &near, &tm).is_empty());
}

#[test]
fn a_second_boss_in_the_sweep_is_not_excused_as_the_threads_own_wall() {
    let tm = mill("TM-0600-P125");
    // A Ø4 pin 9 mm away: inside the crest ring's reach, and no part of the M8 boss.
    let mut f = part();
    let n = 24;
    let at = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % n) as f64 / n as f64;
        Point3::new(-9.0 + 2.0 * a.cos(), 2.0 * a.sin(), z)
    };
    for k in 0..n {
        quad(at(k, 10.0), at(k + 1, 10.0), at(k + 1, 20.0), at(k, 20.0), &mut f);
    }
    let pin = Hole { xy: Point2::new(-9.0, 0.0), ..boss(2.0, 20.0) };
    // Listed or not, the pin is a neighbour while the M8 boss is being threaded.
    for bosses in [vec![boss(4.0, 20.0)], vec![boss(4.0, 20.0), pin]] {
        let path = ride();
        assert!(!neighbours(&external(bosses), &path, &f, &tm).is_empty());
    }
}

#[test]
fn an_unreadable_tool_or_path_fails_closed() {
    let params = external(vec![boss(4.0, 20.0)]);
    let bad = Tool { flute_len: f64::NAN, ..mill("TM-0600-P125") };
    assert!(matches!(neighbours(&params, &ride(), &part(), &bad).as_slice(), [Warning::Gouge { .. }]));
    let mut path = ride();
    if let Some(m) = path.moves.last_mut() {
        m.to.x = f64::INFINITY;
    }
    assert!(matches!(neighbours(&params, &path, &part(), &mill("TM-0600-P125")).as_slice(), [Warning::Gouge { .. }]));
}

#[test]
fn the_slab_clip_and_the_polygon_distance_read_a_facet_exactly() {
    let t = Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 0.0, 0.0), Point3::new(0.0, 0.0, 4.0));
    // A vertical facet clips to a segment along X: z 1..2 keeps x 0..3.
    let poly = slab_clip(&t, 1.0, 2.0);
    let (lo, hi) = poly.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.x), b.max(p.x)));
    assert!((lo - 0.0).abs() < 1e-12 && (hi - 3.0).abs() < 1e-12, "{poly:?}");
    assert!((dist2_poly(Point2::new(5.0, 0.0), &poly) - 4.0).abs() < 1e-12);
    assert!((dist2_poly(Point2::new(1.0, 2.0), &poly) - 4.0).abs() < 1e-12);
    assert!(slab_clip(&t, 5.0, 6.0).is_empty(), "above the facet");
    // A flat facet with area holds the points inside it.
    let flat = [Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Point2::new(0.0, 4.0)];
    assert_eq!(dist2_poly(Point2::new(1.0, 1.0), &flat), 0.0);
    assert!((dist2_poly(Point2::new(-1.0, 1.0), &flat) - 1.0).abs() < 1e-12);
}

//! #3330: the ring seam's own laws, driven inside the crate - the
//! exit point's retract column law on the untrimmed ring ladder, and
//! the foot of `nearest` at a segment end.
//! Fixture: a spherical cap (radius 15) and a canopy block above the
//! exit point, below the clearance plane.

use crate::cl::PathBuilder;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::ladder::levels;
use super::params::{dirs, Finish3DParams, FinishPattern, GapLink};
use super::ring_seam::{nearest, RingSeam, SeamRun};

const R: f64 = 3.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A spherical cap of `radius` on the origin, to `polar_max` off the pole.
fn dome(radius: f64, polar_max: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| {
            let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
            p3(rr * th.cos(), rr * th.sin(), z)
        }).collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    let mut f: Vec<Triangle> = (0..n_lon).map(|j| Triangle::new(apex, first[j], first[(j + 1) % n_lon])).collect();
    for k in 1..n_lat {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..n_lon {
            let n = (j + 1) % n_lon;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    f
}

/// A closed box from `lo` to `hi`, twelve facets.
fn block(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| p3(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
    [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]
        .iter()
        .map(|f| Triangle::new(c(f[0]), c(f[1]), c(f[2])))
        .collect()
}

fn params() -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.05,
        clearance_z: 20.0,
        retract_z: None,
        top_z: 12.0,
        bottom_z: 9.8,
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

/// The ring ladder of `facets` with the picks of `seam`, as
/// `waterline_seamed` emits it, with the column trim off: the ladder
/// the 5-axis swarf keeps (`emit_rings_with_trim(.., false)`).
fn untrimmed(facets: &[Triangle], seam: &RingSeam) -> Result<bool, GenError> {
    let p = params();
    let zs = levels(&p, R, facets)?;
    let ds = dirs(R, p.tolerance);
    let mut b = PathBuilder::approached(p3(0.0, 0.0, p.clearance_z));
    let mut last = None;
    let mut run = SeamRun::new(seam);
    super::rings::emit_rings_seamed(&mut b, &p, facets, &zs, &ds, &mut last, false, Some(&mut run))
}

/// The trimmed ladder already opens a ring under a canopy, so there the
/// exit column law is a second guard. On the untrimmed ladder it is the
/// only guard: a canopy above the exit point, below the clearance
/// plane, is refused by the exit point's name.
#[test]
fn exit_retract_column_through_a_canopy_is_refused_by_name() {
    let mut facets = dome(15.0, 0.9, 8, 16);
    facets.extend(block(p3(-1.5, 11.5, 19.0), p3(1.5, 14.5, 19.5)));
    let start = Some(Point2::new(13.0, 0.0));
    // The start side stands clear of the canopy: the ladder cuts.
    let clear = untrimmed(&facets, &RingSeam { start, exit: None });
    assert!(matches!(clear, Ok(true)), "{clear:?}");
    let e = untrimmed(&facets, &RingSeam { start, exit: Some(Point2::new(0.0, 13.0)) }).err();
    let Some(GenError::Invalid(m)) = e else { panic!("the exit under the canopy is refused as invalid, got {e:?}") };
    assert!(m.starts_with("exit point (0.000, 13.000): the retract column from the ring at z ") && m.contains("crosses the part there"), "{m}");
}

/// A foot at the end of a segment is the start of the next one, and
/// the closing vertex is segment 0: the run-on adds no extra lap there.
#[test]
fn nearest_states_a_segment_end_as_the_next_segment() {
    let square = [Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), Point2::new(1.0, 1.0), Point2::new(0.0, 1.0)];
    let (i, f, _) = nearest(&square, Point2::new(2.0, -1.0)).expect("a foot");
    assert_eq!((i, f.x, f.y), (1, 1.0, 0.0), "the corner (1, 0) is the start of segment 1");
    let (i, f, _) = nearest(&square, Point2::new(-1.0, -1.0)).expect("a foot");
    assert_eq!((i, f.x, f.y), (0, 0.0, 0.0), "the closing vertex is segment 0");
    // An exit at the seam: the closed ring ends where it began, one lap.
    let mut ring: Vec<Point2> = square.iter().copied().chain([square[0]]).collect();
    let seam = RingSeam { start: None, exit: Some(Point2::new(-1.0, -1.0)) };
    let mut run = SeamRun::new(&seam);
    assert!(run.run_on(&mut ring).is_some());
    assert_eq!(ring.len(), 5, "no lap is added at the seam: {ring:?}");
}

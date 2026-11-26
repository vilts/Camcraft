//! #3109: a flat Circular with several rings and a helix entry. The
//! helix of each later ring must not touch the helix of the ring
//! before it, so that the crossing law finds no pair.
//!
//! Provenance: our own tests over the kernel's helix entry (standard
//! helical entry, RS-274 G2/G3) and the crossing law; no vendor
//! material consulted.
//!
//! Gate map: covers camcraft_motion::machining2d (circular).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{CircularParams, circular};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, crossed_pairs, verify};

fn region(c: Point2) -> Region {
    let sq = |x, y| Point2::new(c.x + x, c.y + y);
    Region::new(vec![Loop::new(vec![sq(-12.0, -12.0), sq(12.0, -12.0), sq(12.0, 12.0), sq(-12.0, 12.0)]).unwrap()])
        .unwrap()
}

/// The job of #2898 with the helix entry of #3109: R10, a Ø6 cutter,
/// stepover 2, stepdown 2, Z20 to Z0, helix radius 1.
fn params(center: Point2, climb: bool, stepover: f64, helix_radius: f64) -> CircularParams {
    CircularParams {
        pass_start: Default::default(),
        links: Links::PLANAR,
        center,
        radius: 10.0,
        vault: false,
        bottom_z: 0.0,
        top_z: 20.0,
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        stepover,
        stepdown: 2.0,
        clearance_z: 30.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 250.0,
        climb,
        entry: Entry3D::Helix,
        helix_radius,
    }
}

fn path(p: &CircularParams) -> Toolpath {
    circular(&region(p.center), p)
        .unwrap_or_else(|e| panic!("stepover {} helix {} climb {}: {e:?}", p.stepover, p.helix_radius, p.climb))
}

/// The issue's repro: no feed pair crosses at a shared depth, in both
/// directions and off the origin. Before #3109 the helix of ring k-1
/// touched the helix of ring k at the start of ring k.
#[test]
fn a_multi_ring_helix_circular_has_no_crossing() {
    for center in [Point2::new(0.0, 0.0), Point2::new(30.0, 30.0)] {
        for climb in [true, false] {
            for (stepover, helix) in [(2.0, 1.0), (1.0, 1.0), (2.0, 0.5), (1.0, 0.5), (0.5, 0.25)] {
                let p = params(center, climb, stepover, helix);
                let t = path(&p);
                let pairs = crossed_pairs(&t, 0.01);
                assert!(
                    pairs.is_empty(),
                    "center {center:?} climb {climb} stepover {stepover} helix {helix}: crossings {pairs:?}"
                );
            }
        }
    }
}

/// Where no start keeps the helices apart (orbits as wide as the
/// cutter in a small circle), the ring is refused by name - never a
/// program that verification blocks without a cause. Every other
/// helix size still generates without a crossing.
#[test]
fn a_helix_that_cannot_stand_apart_is_refused_by_name() {
    let c = Point2::new(30.0, 30.0);
    for climb in [true, false] {
        let p = params(c, climb, 3.0, 3.0);
        match circular(&region(c), &p) {
            Ok(t) => panic!("climb {climb}: generated with crossings {:?}", crossed_pairs(&t, 0.01)),
            Err(e) => assert!(format!("{e:?}").contains("helix entry: the descent"), "climb {climb}: {e:?}"),
        }
        for (stepover, helix) in [(2.0, 1.5), (3.0, 1.0), (1.5, 2.0), (0.25, 0.1)] {
            let p = params(c, climb, stepover, helix);
            match circular(&region(c), &p) {
                Ok(t) => assert!(crossed_pairs(&t, 0.01).is_empty(), "stepover {stepover} helix {helix}"),
                Err(e) => assert!(format!("{e:?}").contains("helix entry: the descent"), "{stepover}/{helix}: {e:?}"),
            }
        }
    }
}

/// Each ring still cuts its full circle: the path reaches every ring's
/// radius at the floor, so the turn of the start changes no material.
#[test]
fn every_ring_still_reaches_the_floor() {
    let c = Point2::new(30.0, 30.0);
    let p = params(c, true, 2.0, 1.0);
    let t = path(&p);
    let rc = 10.0 - 3.0;
    let mut k = 0;
    loop {
        let rho = rc - 2.0 * k as f64;
        if rho <= 0.0 {
            break;
        }
        let hit = t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed
                && m.arc.is_some()
                && m.from.z.abs() < 1e-9
                && m.to.z.abs() < 1e-9
                && ((m.to.x - c.x).hypot(m.to.y - c.y) - rho).abs() < 1e-6
        });
        assert!(hit, "ring {k} (radius {rho}) has no arc at the floor");
        k += 1;
    }
}

/// The whole verdict on the issue's job: the turned starts add no
/// crossing, gouge or other warning of their own.
#[test]
fn the_issue_job_verifies_without_crossings() {
    let c = Point2::new(0.0, 0.0);
    let n = 512;
    let disk: Vec<Point2> = (0..n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            Point2::new(c.x + 10.0 * t.cos(), c.y + 10.0 * t.sin())
        })
        .collect();
    let disk = Region::new(vec![Loop::new(disk).unwrap()]).unwrap();
    let tool = ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 25.0,
        shank_radius: 3.0,
        shank_len: 10.0,
        holder_radius: 15.0,
        holder_len: 30.0,
    };
    for climb in [true, false] {
        let p = params(c, climb, 2.0, 1.0);
        let t = path(&p);
        let cx = VerifyContext {
            region: &disk,
            tool: &tool,
            mode: Mode::Area,
            stock_top_z: p.top_z,
            clearance_z: p.clearance_z,
            gouge_tol: 0.01,
            z_tol: 0.05,
            ..VerifyContext::default()
        };
        // The rapids between rings through stock are #2898's (the ring
        // links); they stood there before #3109 and are not read here.
        let w: Vec<Warning> =
            verify(&t, &cx).into_iter().filter(|x| !matches!(x, Warning::RapidInStock { .. })).collect();
        assert!(w.is_empty(), "climb {climb}: {w:?}");
    }
}

//! M5.1 stage E3 generator law tests: multi-axis roughing is the
//! stage-A pocket ladder with the stage-C stamp read over it - so
//! the BALLS must be identical to `pocket3d`'s (the sphere envelope
//! law; the tips follow the pose, the pivot law),
//! the rapids and floors pose nothing, and flanks stand inside the
//! cone.
//! The angle identities themselves are the stamp's own unit tests;
//! what rides here is the composition onto a rough ladder with
//! floors AND walls in one path.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d,
//! camcraft_motion::geometry, camcraft_motion::machining2d,
//! camcraft_motion::machining3d, camcraft_motion::multiaxis.
//! Lane shape (M4.4): all tests here run in seconds; envelope
//! verdicts over these paths ride the app's ignored lane.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d};
use camcraft_motion::multiaxis::{AxisControl, multiaxis_rough};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [
            Triangle::new(v(a), v(b), v(c)),
            Triangle::new(v(a), v(c), v(d)),
        ]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

/// The slab with a 10x10 boss standing to z = 0 over a floor at -10:
/// one path whose rings ride flat floors (vertical poses), circle
/// boss walls (tangency on a horizontal normal - the lean), and
/// cross open air between levels (no pose).
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn params() -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

fn silhouette(facets: &[Triangle]) -> camcraft_motion::geo2d::Region {
    camcraft_motion::geometry::slice::project(facets, 64).unwrap()
}

fn tilt_from_up(a: Point3) -> f64 {
    tilt_between(a, p(0.0, 0.0, 1.0))
}

fn tilt_between(a: Point3, b: Point3) -> f64 {
    let la = (a.x * a.x + a.y * a.y + a.z * a.z).sqrt();
    let d = (a.x * b.x + a.y * b.y + a.z * b.z) / la.max(1e-12);
    d.clamp(-1.0, 1.0).acos().to_degrees()
}

/// #3414: the path's moves without the rotary indices - the standstill
/// swings the stamp inserts: zero-travel RAPIDS whose axis CHANGES off
/// the pose held. A zero-travel rapid that states no new pose stays,
/// and so does an opening upright statement from no held pose (#3300):
/// the home the path inherits, commanded as zero angles.
fn sans_swings(path: &camcraft_motion::cl::Toolpath) -> Vec<&camcraft_motion::cl::Move> {
    let mut held: Option<[f64; 3]> = None;
    path.moves
        .iter()
        .filter(|m| {
            let swing = m.kind == MoveKind::Rapid && m.from == m.to && m.axis != held
                && !(held.is_none() && m.axis.is_some_and(|a| camcraft_motion::cl::is_vertical(&a)));
            if !swing {
                held = m.axis.or(held);
            }
            !swing
        })
        .collect()
}

#[test]
fn positions_are_the_prismatic_ladders_bit_for_bit() {
    // The whole claim of law (f) in one assert: leaning the spindle
    // moves nothing. Same moves, same order, same endpoints.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let base = pocket3d(&f, &region, &params()).unwrap();
    let rough = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    // #3414: the posed ladder carries standstill rotary indices, so
    // the count and the pairing read the swing-free stream.
    let cuts = |t: &camcraft_motion::cl::Toolpath| t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).count();
    assert_eq!(cuts(&base), cuts(&rough));
    // The pivot law: the BALL is the ladder's - the posted tip lifted
    // r along the pose the machine holds (modal) is the upright tip
    // lifted r up Z. The tip itself moves with the lean.
    let r = params().tool_diameter * 0.5;
    let mut held = [0.0, 0.0, 1.0];
    for (b, m) in base.moves.iter().zip(sans_swings(&rough)) {
        if let Some(a) = m.axis {
            held = a;
        }
        let c = p(m.to.x + r * held[0], m.to.y + r * held[1], m.to.z + r * held[2]);
        let want = p(b.to.x, b.to.y, b.to.z + r);
        let d = c - want;
        assert_eq!(b.kind, m.kind);
        assert!(d.dot(d).sqrt() < 1e-9, "the stamp moved a ball: {c:?} vs {want:?}");
    }
    // And it DID pose something: an all-None stamp is a no-op, not a
    // rough - at least one cut move must state an axis.
    assert!(
        rough
            .moves
            .iter()
            .any(|m| m.kind != MoveKind::Rapid && m.axis.is_some()),
        "the rough stamp stated no pose at all"
    );
}

#[test]
fn rapids_hold_inherited_pose_and_flanks_lean() {
    // Rapids inherit the previous pose, starting upright (#3300).
    // Floors are the rough's own story: the ladder's
    // CL rides IN the machined plane, so the ball's contact point is
    // the one under the center with d = 0 - no direction out of the
    // surface to lean off, and the stamp leaves the spindle vertical
    // (an explicit upright axis), which is what a floor wants. Flanks are
    // where the kind earns its name: tangency on a vertical facet is
    // a pose DEAD ON its normal; a ring rounding a convex corner
    // touches the EDGE and states the bisector - still inside the
    // cone of both walls' normals, which is the law command_axis
    // states (the angle is measured to the normal, and an edge's two
    // normals both claim it).
    let f = slab_with_boss();
    let region = silhouette(&f);
    let rough = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    // Rapids travel under the pose they inherit (the post keeps the
    // tilt modal): they state no NEW angle. #3300: the rough leans on
    // its flanks, so it inherits the upright home and states it on
    // every move before the first lean (floors included - the post
    // commands it as zero angles); after the lean a floor states the
    // upright Z column as before. The flank count below proves the
    // lean. The exception is the zero-travel rotary index (#3414) -
    // the standstill swing in air, which the sweep below skips.
    assert_eq!(rough.moves[0].axis, Some([0.0, 0.0, 1.0]), "the opening states the upright pose");
    let mut held: Option<[f64; 3]> = Some([0.0, 0.0, 1.0]);
    for m in sans_swings(&rough) {
        if m.kind == MoveKind::Rapid {
            assert_eq!(m.axis, held, "a rapid changed the pose it inherited");
        }
        held = m.axis.or(held);
    }
    let seeds = [p(1.0, 0.0, 0.0), p(-1.0, 0.0, 0.0), p(0.0, 1.0, 0.0), p(0.0, -1.0, 0.0)];
    let (mut flanks, mut corners) = (0, 0);
    for m in rough.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        // An upright statement after a lean is the floor's own pose,
        // commanded (the post zeroes the letters): not a lean.
        let Some(a) = m.axis.filter(|a| !camcraft_motion::cl::is_vertical(a)) else { continue };
        let a = p(a[0], a[1], a[2]);
        let off = seeds.iter().map(|s| tilt_between(a, *s)).fold(f64::MAX, f64::min);
        if off < 0.1 {
            flanks += 1;
        } else if off <= 60.1 {
            corners += 1;
        } else {
            panic!("a zero-lead pose drifted {off:.2} past every wall cone: kind {:?} to {:?}", m.kind, m.to);
        }
        // Floor poses stay upright, so every remaining pose is a
        // HORIZONTAL tangency: a rough over a level floor must never
        // lean the spindle on air.
        assert!(a.z.abs() < 1e-6, "a posed move leans off a horizontal seed: {a:?}");
    }
    assert!(flanks >= 4, "wall tangency poses missing: {flanks}");
    assert!(corners >= 1, "the ladder met no boss corner: {corners}");
}

#[test]
fn every_pose_stays_inside_its_cone() {
    // The cone is seeded at the SURFACE NORMAL (tilt.rs states it:
    // the angle between command and normal) - lead and lag rotate
    // within it, the clamp walls them off, and a flank's pose may
    // legitimately stand 90 degrees off vertical: that is a side
    // cut, the kind's whole answer to a 3+2 index. Every normal
    // here is a seed direction, so "within budget of SOME seed" is
    // the law restated without re-running the stamp.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let seeds = [p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), p(-1.0, 0.0, 0.0), p(0.0, 1.0, 0.0), p(0.0, -1.0, 0.0)];
    for c in [control(0.0, 0.0, Some(60.0)), control(20.0, 10.0, Some(45.0))] {
        let rough = multiaxis_rough(&f, &region, &params(), &c, None).unwrap();
        let budget = c.max_tilt_deg.unwrap();
        for m in rough.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            if let Some(a) = m.axis {
                let a = p(a[0], a[1], a[2]);
                let off = seeds.iter().map(|s| tilt_between(a, *s)).fold(f64::MAX, f64::min);
                assert!(
                    off <= budget + 0.1,
                    "pose {a:?} leans {off:.1} past a {budget:.0} cone around any normal"
                );
            }
        }
    }
}

#[test]
fn a_wall_buys_a_lean() {
    // The kind's whole point over a 3+2 index: the ring circling the
    // boss wall machines it at tangency, and tangency on a vertical
    // facet is a leaning axis. If this test finds no lean, the stamp
    // is silently skipping the rough's poses - the exact silent
    // wrong answer the Prime Directive forbids.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let rough = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    let leaned = rough.moves.iter().any(|m| {
        m.kind != MoveKind::Rapid
            && m.axis.is_some_and(|a| tilt_from_up(p(a[0], a[1], a[2])) > 10.0)
    });
    assert!(leaned, "a rough over vertical walls posed nothing off vertical");
}

#[test]
fn the_ladders_refusals_pass_straight_through() {
    // No hidden swallow: what pocket3d refuses, the stamp never sees.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let mut p_bad = params();
    p_bad.stepover = 0.0;
    let e = multiaxis_rough(&f, &region, &p_bad, &control(0.0, 0.0, Some(60.0)), None);
    assert!(e.is_err(), "the stamp swallowed the ladder's refusal");
}

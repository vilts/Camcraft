//! The 5-Axis Contour's start and exit picks (#3330) through the
//! application: the picks seat the waterline rings and carry each lap on
//! to the exit, and the cut geometry stays the automatic ladder's.
//! A module of the `start_exit_shared` suite.

use super::*;

/// A spherical cap (radius 12, to 60 degrees off the pole) on z = 0.
fn dome() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Mesh, Point3, Triangle};
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| {
            let th = TAU * j as f64 / n_lon as f64;
            Point3::new(rr * th.cos(), rr * th.sin(), z)
        }).collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
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
    Mesh::from_facets(f)
}

/// The dome on a 40 x 40 x 12 block, with the catalog ball.
fn dome_world() -> flecs_ecs::prelude::World {
    use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
    use camcraft_app::geometry::{Point3, IDENTITY_Q};
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = camcraft_app::schema::Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let stock = SetStock::new(&world, camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 12.0 }, at);
    h.execute(&world, stock.expect("stock"));
    let t = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", t).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", dome()));
    world
}

fn contour5d(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> camcraft_app::schema::Operation {
    let pick = |(x, y)| camcraft_app::path_picks::PathPick::xy(Point2::new(x, y));
    let params = OpParams::Contour5D {
        tolerance: 0.1,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(11.0),
        bottom_z: Height::fixed(7.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
        picks: camcraft_app::path_picks::PathPicks { start: start.map(pick).into_iter().collect(), exit: exit.map(pick).into_iter().collect() },
    };
    camcraft_app::schema::Operation {
        tool: "BN-0600".into(),
        src: camcraft_app::operation::SrcRef::Entity("part".into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// Each move's ball centre: its tip backed up 3 mm (the ball's radius)
/// along the pose the machine holds there (the pivot law).
fn centres(t: &camcraft_app::cl::Toolpath) -> Vec<(camcraft_app::cl::MoveKind, [f64; 3], [f64; 3])> {
    let mut held = [0.0, 0.0, 1.0];
    t.moves.iter().map(|m| {
        let from = [m.from.x + 3.0 * held[0], m.from.y + 3.0 * held[1], m.from.z + 3.0 * held[2]];
        held = m.axis.unwrap_or(held);
        (m.kind, from, [m.to.x + 3.0 * held[0], m.to.y + 3.0 * held[1], m.to.z + 3.0 * held[2]])
    }).collect()
}

/// Each ring's (first, last) ball centre: the feed run at one level
/// that a plunge begins. A closed ring returns to its first centre.
fn rings(t: &camcraft_app::cl::Toolpath) -> Vec<([f64; 3], [f64; 3])> {
    use camcraft_app::cl::MoveKind;
    let c = centres(t);
    let mut out = Vec::new();
    let mut k = 0;
    while k < c.len() {
        if c[k].0 != MoveKind::Plunge {
            k += 1;
            continue;
        }
        let first = c[k].2;
        let mut last = first;
        k += 1;
        while k < c.len() && c[k].0 == MoveKind::Feed && (c[k].2[2] - first[2]).abs() < 1e-6 {
            last = c[k].2;
            k += 1;
        }
        out.push((first, last));
    }
    out
}

/// The step that samples each level cut along its length: much finer
/// than any chord a gouge can take across a ring.
const STEP: f64 = 0.25;

/// The ball centre segments of the level cuts of `t`: the feed moves
/// whose ball centre holds one Z.
fn level_cuts(t: &camcraft_app::cl::Toolpath) -> Vec<([f64; 3], [f64; 3])> {
    use camcraft_app::cl::MoveKind;
    centres(t).into_iter().filter(|(k, a, b)| *k == MoveKind::Feed && (a[2] - b[2]).abs() < 1e-6).map(|(_, a, b)| (a, b)).collect()
}

/// Every level cut of `got` rides a level cut of `auto` along its whole
/// length: each move is sampled every [`STEP`] mm and each sample sits
/// within the operation's tolerance of the automatic ladder. A chord
/// across a ring (a gouge) fails here, not only a wrong end point.
fn cuts_ride_the_ladder(auto: &camcraft_app::cl::Toolpath, got: &camcraft_app::cl::Toolpath) {
    let segs = level_cuts(auto);
    let off = |c: [f64; 3]| {
        segs.iter().map(|(a, b)| {
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let t = if l2 > 0.0 { (((c[0] - a[0]) * d[0] + (c[1] - a[1]) * d[1] + (c[2] - a[2]) * d[2]) / l2).clamp(0.0, 1.0) } else { 0.0 };
            (0..3).map(|i| (c[i] - a[i] - d[i] * t).powi(2)).sum::<f64>().sqrt()
        }).fold(f64::INFINITY, f64::min)
    };
    for (a, b) in level_cuts(got) {
        let len = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
        let n = ((len / STEP).ceil() as usize).max(1);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let c = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
            let e = off(c);
            assert!(e < 0.1, "a cut stands {e} mm off the automatic ladder at {c:?}");
        }
    }
}

fn same(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

#[test]
fn contour5d_starts_at_the_picked_start_and_leaves_at_the_picked_exit() {
    let world = dome_world();
    let generate = |op| camcraft_app::operation::generate(&world, &op, "").map(|(t, _)| t);
    let auto = generate(contour5d(None, None)).expect("the automatic ladder");
    // Start alone: every closed ring begins on the pick's ray (+X).
    let started = generate(contour5d(Some((13.0, 0.0)), None)).expect("the started ladder");
    let closed: Vec<_> = rings(&started).into_iter().filter(|(a, b)| same(*a, *b)).collect();
    assert!(!closed.is_empty(), "the dome ladder has closed rings");
    for (a, _) in &closed {
        assert!(a[1].abs() < 1e-6 && a[0] > 0.0, "a ring starts off the start pick's ray: {a:?}");
    }
    assert_eq!(rings(&started).len(), rings(&auto).len(), "the pick seats the rings, it adds none");
    cuts_ride_the_ladder(&auto, &started);
    // Start and exit: each seated ring leaves on the exit pick's ray (+Y).
    let both = generate(contour5d(Some((13.0, 0.0)), Some((0.0, 13.0)))).expect("the picked ladder");
    let seated: Vec<_> = rings(&both).into_iter().filter(|(a, _)| a[1].abs() < 1e-6 && a[0] > 0.0).collect();
    assert_eq!(seated.len(), closed.len(), "every closed ring is seated");
    for (_, b) in &seated {
        assert!(b[0].abs() < 1e-6 && b[1] > 0.0, "a ring leaves off the exit pick's ray: {b:?}");
    }
    cuts_ride_the_ladder(&auto, &both);
    // Without a pick the path is the automatic ladder, run for run.
    assert_eq!(generate(contour5d(None, None)).expect("again"), auto);
}

#[test]
fn contour5d_refuses_a_pick_far_from_every_ring_by_name() {
    let world = dome_world();
    for (op, what) in [(contour5d(Some((90.0, 0.0)), None), "start point (90.000, 0.000)"), (contour5d(None, Some((0.0, 90.0))), "exit point (0.000, 90.000)")] {
        let e = camcraft_app::operation::generate(&world, &op, "").err().map(|e| format!("{e:?}"));
        assert!(e.as_deref().is_some_and(|m| m.contains(what) && m.contains("farther than")), "{what}: {e:?}");
    }
}

#[test]
fn contour5d_picks_ride_edit_operation_and_undo_restores_them() {
    let mut s = job();
    assert!(!call(&mut s, "add_tool", json!({ "catalog": "BN-0600" })).0);
    let (err, r) = call(&mut s, "add_operation", json!({ "kind": "5-Axis Contour", "tool": "BN-0600", "name": "C5" }));
    assert!(!err, "{r}");
    let patch = json!({ "name": "C5", "params": { "picks": { "start": [[35.25, 0.75]], "exit": [[0.75, 25.25]] } } });
    let (err, r) = call(&mut s, "edit_operation", patch);
    assert!(!err, "the 5-axis contour takes both picks: {r}");
    let held = |s: &mut Server| call(s, "get_job", json!({})).1.to_string();
    let job_text = held(&mut s);
    assert!(job_text.contains("35.25") && job_text.contains("25.25"), "the job carries the picks: {job_text:.600}");
    assert!(!call(&mut s, "undo", json!({})).0);
    let job_text = held(&mut s);
    assert!(!job_text.contains("35.25") && !job_text.contains("25.25"), "undo restores the automatic seam");
    assert!(!call(&mut s, "redo", json!({})).0);
    assert!(held(&mut s).contains("35.25"), "redo states the picks again");
}

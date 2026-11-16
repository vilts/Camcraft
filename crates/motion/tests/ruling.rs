//! M5.1 stage D ruling and swarf law tests: what the field analysis
//! says about the four archetypes (a blade swept along a straight
//! XY direction, a wall swept along Z, a dome with no straight
//! direction at all, and plain ground), and what the two swarf kinds
//! compose to on the blade: the SAME ball-center path as
//! `finishing3d::swarf` - which is the plain cos-law raster along
//! the found direction - with the tool lying WITH the blade at every
//! ruled contact.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (ruling,
//! swarf, stamp composition) and camcraft_motion::finishing3d
//! (the swarf pattern it drives).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{self, Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::multiaxis::ruling;
use camcraft_motion::multiaxis::{swarf5d, swarf5d_advanced, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 3.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn finish(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.05,
        clearance_z: 30.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        // M8.2 B: the ring entry keeps its plunge unless asked.
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

/// A grid of quads (two triangles each) over the graph z = f(x,y).
fn graph_grid(n: usize, span: f64, f: impl Fn(f64, f64) -> f64) -> Vec<Triangle> {
    let g = |i: usize, j: usize| {
        let (x, y) = (-span + 2.0 * span * i as f64 / n as f64, -span + 2.0 * span * j as f64 / n as f64);
        p3(x, y, f(x, y))
    };
    let mut v = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b, c, d) = (g(i, j), g(i + 1, j), g(i + 1, j + 1), g(i, j + 1));
            v.push(Triangle::new(a, b, c));
            v.push(Triangle::new(a, c, d));
        }
    }
    v
}

/// A circular tube about the X axis: the blade swept along a straight
/// XY ruling. (A tube about Z is the same law with the axes swapped.)
fn tube_about(n_circ: usize, n_x: usize, radius: f64, x_len: f64) -> Vec<Triangle> {
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(-x_len + 2.0 * x_len * i as f64 / n_x as f64, radius * th.cos(), radius * th.sin())
    };
    let mut t = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            t.push(Triangle::new(a, b, c));
            t.push(Triangle::new(a, c, d));
        }
    }
    t
}

/// A vertical cylinder wall about Z: ruled, but along Z.
fn wall(n_circ: usize, n_z: usize, radius: f64, height: f64) -> Vec<Triangle> {
    let v = |j: usize, k: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(radius * th.cos(), radius * th.sin(), height * k as f64 / n_z as f64)
    };
    let mut t = Vec::new();
    for j in 0..n_circ {
        for k in 0..n_z {
            let (a, b, c, d) = (v(j, k), v(j + 1, k), v(j + 1, k + 1), v(j, k + 1));
            t.push(Triangle::new(a, b, c));
            t.push(Triangle::new(a, c, d));
        }
    }
    t
}

/// A spherical cap (the dome archetype; no direction is straight).
fn dome(radius: f64, polar_max: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon).map(|j| {
            let th = TAU * j as f64 / n_lon as f64;
            p3(rr * th.cos(), rr * th.sin(), z)
        }).collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    let mut prev = first;
    for k in 2..=n_lat {
        let cur = ring(k);
        for j in 0..n_lon {
            facets.push(Triangle::new(prev[j], cur[j], cur[(j + 1) % n_lon]));
            facets.push(Triangle::new(prev[j], cur[(j + 1) % n_lon], prev[(j + 1) % n_lon]));
        }
        prev = cur;
    }
    facets
}

fn err_text(e: GenError) -> String {
    match e {
        GenError::Invalid(s) | GenError::Empty(s) | GenError::Model(s) => s,
        GenError::At { msg, .. } | GenError::Ghosted { msg, .. } => msg,
        GenError::Geo(g) => g.to_string(),
    }
}

#[test]
fn the_field_reads_the_blades_straight_direction() {
    let blade = tube_about(32, 8, 12.0, 30.0);
    let f = ruling::analyze(&blade).expect("a tube about X is a ruled surface");
    assert!((f.ux - 1.0).abs() < 1e-9 && f.uy.abs() < 1e-9, "got ({}, {})", f.ux, f.uy);
    let held = f.ruled.iter().filter(|&&b| b).count();
    assert!(held * 20 > blade.len() * 19, "only {}/{} facets hold the ruling", held, blade.len());
}

#[test]
fn a_wall_ruled_along_z_refuses_and_points_at_contour() {
    let w = wall(32, 8, 15.0, 40.0);
    match ruling::analyze(&w) {
        Ok(f) => panic!("a vertical wall was accepted at direction ({}, {})", f.ux, f.uy),
        Err(e) => {
            let s = err_text(e);
            assert!(s.contains("along Z") && s.contains("Contour"), "got {s}");
        }
    }
}

#[test]
fn a_dome_has_no_ruling_and_names_the_kind_that_has() {
    let d = dome(18.0, 1.0, 10, 24);
    match ruling::analyze(&d) {
        Ok(f) => panic!("a sphere was handed a direction ({}, {})", f.ux, f.uy),
        Err(e) => {
            let s = err_text(e);
            assert!(s.contains("no ruling"), "got {s}");
        }
    }
}

#[test]
fn plain_ground_accepts_any_pass_and_claims_no_ruling() {
    // The pitched plane of the stage-C stamp tests, at ruling scale:
    // flat is not doubly-curved - there is nothing to follow, and
    // nothing to refuse either; the lean law alone poses the path.
    let g = graph_grid(10, 50.0, |x, _| 4.0 + x * 30f64.to_radians().tan());
    let f = ruling::analyze(&g).expect("plain ground is any-pass-any ground");
    assert!((f.ux - 1.0).abs() < 1e-12 && f.uy.abs() < 1e-12);
    assert!(f.ruled.iter().all(|&b| !b), "a plane facet claimed a ruling");
}

#[test]
fn an_empty_model_refuses_never_crashes() {
    let e = ruling::analyze(&[]).err();
    assert!(matches!(e, Some(GenError::Invalid(_))), "got {e:?}");
}

#[test]
fn swarf5d_balls_are_the_plain_swarf_balls() {
    let blade = tube_about(32, 8, 12.0, 30.0);
    let p = finish(11.0, -11.0);
    let f = ruling::analyze(&blade).unwrap();
    let base = finishing3d::swarf(&blade, &p, f.ux, f.uy).unwrap();
    let got = swarf5d(&blade, &p, &control(15.0, 0.0, None)).unwrap();
    // The pivot law: the BALLS are the plain swarf's - each posted
    // tip lifted R along the pose the machine holds (modal) lies on
    // the upright ladder lifted R up Z; the tip follows the lean, and
    // a leaning arc rides as its chords ON the planned arc.
    let plan: Vec<(Point3, Point3)> = base
        .moves
        .iter()
        .flat_map(|m| m.segments3(1e-4))
        .map(|(a, b)| (p3(a.x, a.y, a.z + R), p3(b.x, b.y, b.z + R)))
        .collect();
    let mut held = [0.0, 0.0, 1.0];
    for g in &got.moves {
        if let Some(a) = g.axis {
            held = a;
        }
        let c = p3(g.to.x + R * held[0], g.to.y + R * held[1], g.to.z + R * held[2]);
        // The link law (stamp_link): its rapids - the retract along the
        // contact normal and the swing in the air it found - are air by
        // the law's own sweep check, not balls of the plan, as the
        // plan's own rapids to clearance are; the plunge back lands ON
        // the plan and is judged.
        if g.kind == MoveKind::Rapid {
            continue;
        }
        let off = plan
            .iter()
            .map(|(a, b)| {
                let d = *b - *a;
                let l2 = d.dot(d);
                let t = if l2 > 0.0 { ((c - *a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let e = c - p3(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t);
                e.dot(e).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(off < 1e-3, "a posted ball left the plan: {c:?} is {off} off");
        let posed = !camcraft_motion::cl::is_vertical(&held);
        assert!(!posed || g.arc.is_none(), "a leaning arc must ride as chords");
    }
    assert!(!got.is_prismatic(), "swarf over a blade stated no pose anywhere");
}

#[test]
fn the_swarf_axis_lies_with_the_blade() {
    // The posed cut moves over the tube's BODY hold the ruling's own
    // direction (up to sign): the tool is side-on to the flank, which
    // is what makes a ball touch ALONG a ruling instead of at a
    // point. The two rim rows of facets see only a half-neighbourhood
    // and the analysis makes no ruling claim there, so their moves
    // fall to the lean law - that is the boundary behaving honestly,
    // not the identity; the identity is tested where the claim is
    // made, inside the body. (The exact-bit version of this law is
    // swarf.rs's unit test on one clean facet.)
    let blade = tube_about(32, 8, 12.0, 30.0);
    let p = finish(11.0, -11.0);
    let got = swarf5d(&blade, &p, &control(15.0, 10.0, Some(45.0))).unwrap();
    let (mut n_posed, mut n_body) = (0, 0);
    for m in got.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let Some(a) = m.axis else { continue };
        n_posed += 1;
        if m.to.x.abs() > 25.0 {
            continue; // rim rows: no ruling claim, lean law's business
        }
        let (ax, ay, az) = (a[0], a[1], a[2]);
        let l = (ax * ax + ay * ay + az * az).sqrt();
        assert!(l > 0.0);
        // The contact normal itself is exact on a facet face and
        // tilts at most atan(sqrt((r+band)^2 - r^2) / r) ~ 5 degrees
        // on a band-edge vertex: that is the budget here.
        assert!(ay.abs() / l < 0.09 && az.abs() / l < 0.09, "axis {a:?} left the ruling");
        assert!(ax.abs() / l > 0.995);
        n_body += 1;
    }
    assert!(n_body > 20, "only {n_body} body poses over a whole blade");
    assert!(n_posed > n_body, "the rim rows stated no pose at all");
}

#[test]
fn advanced_swarf_runs_the_coarse_first_ladder() {
    let blade = tube_about(32, 8, 12.0, 30.0);
    let p = finish(11.0, -11.0);
    let one = swarf5d_advanced(&blade, &p, &control(0.0, 0.0, None), 1).unwrap();
    let two = swarf5d_advanced(&blade, &p, &control(0.0, 0.0, None), 2).unwrap();
    // The coarse-first pass adds its own (wider-spaced) lines and
    // rings ahead of the nominal ladder; the count is the honest
    // measure - the bit-level ladder law is finishing3d's own.
    assert!(two.moves.len() > one.moves.len(), "the rough pass added nothing");
    assert!(two.all_finite() && one.all_finite());
}

#[test]
fn a_senseless_pass_count_refuses_by_name() {
    let blade = tube_about(32, 8, 12.0, 30.0);
    let p = finish(11.0, -11.0);
    for n in [0u32, 5] {
        let e = swarf5d_advanced(&blade, &p, &AxisControl::default(), n).err();
        assert!(matches!(e, Some(GenError::Invalid(_))), "passes {n} gave {e:?}");
    }
}

#[test]
fn a_dome_refuses_swarf_before_any_path_is_cut() {
    let d = dome(18.0, 1.0, 10, 24);
    let e = swarf5d(&d, &finish(15.0, 10.0), &AxisControl::default()).err();
    match e {
        Some(GenError::Invalid(s)) => assert!(s.contains("ruling") || s.contains("Z"), "got {s}"),
        other => panic!("a dome's swarf refused with {other:?}, not a named ruling refusal"),
    }
}

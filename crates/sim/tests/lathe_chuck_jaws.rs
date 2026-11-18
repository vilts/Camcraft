//! Jaw chucks' inside grip and stepped jaws (#1468): the envelope each
//! jaw's shells stand in - steel a hair inside every true face, air a
//! hair past it (past the arc's own allowance on an inside jaw's
//! gripping face) - the outward volume, and every nonsense figure
//! refused by name. A file saved before #1468 reads as plain outside
//! jaws.
//!
//! Gate map (M4.4): covers camcraft_sim::chuck, camcraft_sim::chuck::jaw.

use camcraft_sim::chuck::jaw::{ARC_STEP, arc_outline};
use camcraft_sim::chuck::{self, Grip, JawChuck};
use camcraft_sim::fixture_mesh::FixtureMesh;
use camcraft_sim::geometry::{Point3, Triangle};

/// How close (mm) to a true surface the envelope probes stand.
const HAIR: f64 = 1e-3;

fn plain() -> JawChuck {
    JawChuck {
        body_diameter: 160.0,
        body_length: 70.0,
        bore_diameter: 40.0,
        grip_diameter: 40.0,
        jaw_length: 35.0,
        jaw_width: 20.0,
        jaw_height: 30.0,
        grip: Grip::Outside,
        step_height: 0.0,
        step_depth: 0.0,
    }
}

/// Jaws opened into a 100 mm bore, running 30 inward.
fn inside() -> JawChuck {
    JawChuck { grip: Grip::Inside, grip_diameter: 100.0, jaw_length: 30.0, ..plain() }
}

fn mesh(c: &JawChuck, jaws: u32) -> FixtureMesh {
    c.check("3-jaw chuck", jaws).expect("the figures pass");
    FixtureMesh::build(&c.solid(jaws)).expect("closed shells")
}

/// The outward volume of a triangle soup (the divergence theorem).
fn volume(t: &[Triangle]) -> f64 {
    t.iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            (a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)) / 6.0
        })
        .sum()
}

/// The area of a counter-clockwise outline.
fn area(p: &[(f64, f64)]) -> f64 {
    (0..p.len()).map(|i| p[i].0 * p[(i + 1) % p.len()].1 - p[(i + 1) % p.len()].0 * p[i].1).sum::<f64>() / 2.0
}

/// The drawn body: flats on its circle, vertices on its bore (#1470).
fn body_volume(c: &JawChuck) -> f64 {
    let n = chuck::SIDES as f64;
    let half = std::f64::consts::PI / n;
    let (ro, ri) = (0.5 * c.body_diameter, 0.5 * c.bore_diameter);
    (n * half.tan() * ro * ro - n * half.sin() * half.cos() * ri * ri) * c.body_length
}

/// How far an inside jaw's drawn arc may stand past its circle `r`.
fn arc_allowance(r: f64) -> f64 {
    r * (1.0 / (0.5 * ARC_STEP).cos() - 1.0)
}

#[test]
fn a_file_from_before_reads_as_plain_outside_jaws() {
    let old = r#"{"body_diameter":160.0,"body_length":70.0,"bore_diameter":40.0,"grip_diameter":40.0,"jaw_length":35.0,"jaw_width":20.0,"jaw_height":30.0}"#;
    let c: JawChuck = serde_json::from_str(old).expect("an old chuck");
    assert_eq!(c, plain());
    assert!(!c.stepped());
    let text = serde_json::to_string(&inside()).expect("written");
    assert!(text.contains(r#""grip":"inside""#), "{text}");
    assert_eq!(serde_json::from_str::<JawChuck>(&text).expect("read back"), inside());
    assert_eq!(Grip::of("inside"), Some(Grip::Inside));
    assert_eq!(Grip::of("sideways"), None);
    assert_eq!(Grip::KEYS.map(|k| Grip::of(k).map(Grip::key)), [Some("outside"), Some("inside")]);
}

#[test]
fn outside_stepped_jaw_envelope_is_pinned() {
    // The step 10 high reaches 8 under the work: r 12..55 below z 10,
    // the plain jaw r 20..55 above it.
    let c = JawChuck { step_height: 10.0, step_depth: 8.0, ..plain() };
    assert!(c.stepped());
    let m = mesh(&c, 3);
    let at = |r: f64, v: f64, z: f64| Point3::new(r, v, z);
    // The step: steel to its inner face, its top and its sides.
    assert!(m.contains(at(12.0 + HAIR, 0.0, 5.0)), "the step's inner face");
    assert!(!m.contains(at(12.0 - HAIR, 0.0, 5.0)), "air before the step");
    assert!(m.contains(at(16.0, 0.0, 10.0 - HAIR)), "the step's top, where the work sits");
    assert!(!m.contains(at(16.0, 0.0, 10.0 + HAIR)), "the work's room over the step");
    assert!(m.contains(at(16.0, 10.0 - HAIR, 5.0)), "the step's side");
    assert!(!m.contains(at(16.0, 10.0 + HAIR, 5.0)), "air beside the step");
    // Above the step the jaw is the plain jaw.
    assert!(m.contains(at(20.0 + HAIR, 0.0, 20.0)), "the gripping face");
    assert!(!m.contains(at(20.0 - HAIR, 0.0, 20.0)), "air before the gripping face");
    assert!(m.contains(at(55.0 - HAIR, 0.0, 5.0)) && m.contains(at(55.0 - HAIR, 0.0, 25.0)), "the jaw's tip, both shells");
    assert!(!m.contains(at(55.0 + HAIR, 0.0, 5.0)) && !m.contains(at(55.0 + HAIR, 0.0, 25.0)), "air past the tip");
    assert!(m.contains(at(40.0, 0.0, 30.0 - HAIR)) && !m.contains(at(40.0, 0.0, 30.0 + HAIR)), "the jaw's top");
    // Every jaw carries the step.
    let a = std::f64::consts::TAU / 3.0;
    assert!(m.contains(Point3::new(14.0 * a.cos(), 14.0 * a.sin(), 5.0)), "the second jaw's step");
    let want = body_volume(&c) + 3.0 * 20.0 * (43.0 * 10.0 + 35.0 * 20.0);
    let v = volume(&c.solid(3));
    assert!((v - want).abs() < 1e-6, "outward volume {v} vs {want}");
    // The same figures without the step are the #1442 jaw, to the triangle.
    assert_eq!(JawChuck { step_height: 0.0, step_depth: 0.0, ..c }.solid(3), plain().solid(3));
}

#[test]
fn inside_grip_envelope_is_pinned() {
    // Jaws 20 wide in a 100 bore: r 20..50, the gripping face the bore's
    // arc between the jaw's sides (half-angle asin(10 / 50)).
    let c = inside();
    let m = mesh(&c, 3);
    let theta = (10.0f64 / 50.0).asin();
    let extra = arc_allowance(50.0);
    assert!(extra < 0.005, "the arc's allowance {extra} stays under the clash depth");
    // All across the gripping arc: steel a hair inside the bore, air a
    // hair past the arc's allowance - never less steel than the true arc.
    for k in 0..=40 {
        let phi = (theta - 1e-4) * (f64::from(k) / 20.0 - 1.0);
        let at = |r: f64| Point3::new(r * phi.cos(), r * phi.sin(), 15.0);
        assert!(m.contains(at(50.0 - HAIR)), "steel at the grip, {phi} rad");
        assert!(!m.contains(at(50.0 + extra + HAIR)), "air past the arc, {phi} rad");
    }
    // The drawn outline never stands inside the true arc.
    let outline = arc_outline(20.0, 50.0, 10.0);
    assert!(outline.iter().skip(1).take(outline.len() - 2).all(|p| p.0.hypot(p.1) >= 50.0 - 1e-9), "{outline:?}");
    assert!(outline.iter().all(|p| p.0.hypot(p.1) <= 50.0 + extra + 1e-9 && p.1.abs() <= 10.0 + 1e-9), "{outline:?}");
    // The inner end, the sides, the top.
    assert!(m.contains(Point3::new(20.0 + HAIR, 0.0, 15.0)), "the jaw's inner end");
    assert!(!m.contains(Point3::new(20.0 - HAIR, 0.0, 15.0)), "air inside the inner end");
    assert!(m.contains(Point3::new(35.0, 10.0 - HAIR, 15.0)) && !m.contains(Point3::new(35.0, 10.0 + HAIR, 15.0)), "the side");
    assert!(m.contains(Point3::new(35.0, 0.0, 30.0 - HAIR)) && !m.contains(Point3::new(35.0, 0.0, 30.0 + HAIR)), "the top");
    // No steel outside the bore the jaws open into: the work's wall.
    assert!(!m.contains(Point3::new(55.0, 0.0, 15.0)), "the work's wall");
    let a = std::f64::consts::TAU / 3.0;
    assert!(m.contains(Point3::new(40.0 * a.cos(), 40.0 * a.sin(), 15.0)), "the second jaw");
    assert!(!m.contains(Point3::new(-40.0, 0.0, 15.0)), "no jaw at 180 degrees on a 3-jaw");
    let want = body_volume(&c) + 3.0 * area(&outline) * 30.0;
    let v = volume(&c.solid(3));
    assert!((v - want).abs() < 1e-6, "outward volume {v} vs {want}");
    // The drawn jaw holds the true one: the box to the shoulder plus the
    // bore's segment between the sides.
    let truth = 20.0 * (50.0 * theta.cos() - 20.0) + 50.0 * 50.0 * (theta - theta.sin() * theta.cos());
    assert!(area(&outline) >= truth && area(&outline) < truth + 0.05, "{} vs {truth}", area(&outline));
}

#[test]
fn inside_stepped_jaw_envelope_is_pinned() {
    // The step 10 high reaches 8 past the grip, under the work's wall.
    let c = JawChuck { step_height: 10.0, step_depth: 8.0, ..inside() };
    let m = mesh(&c, 4);
    assert!(m.contains(Point3::new(58.0 - HAIR, 0.0, 5.0)), "the step's outer face");
    assert!(!m.contains(Point3::new(58.0 + arc_allowance(58.0) + HAIR, 0.0, 5.0)), "air past the step");
    assert!(m.contains(Point3::new(54.0, 0.0, 10.0 - HAIR)), "the step's top, where the work sits");
    assert!(!m.contains(Point3::new(54.0, 0.0, 10.0 + HAIR)), "the work's wall over the step");
    assert!(m.contains(Point3::new(50.0 - HAIR, 0.0, 20.0)), "the gripping face above the step");
    assert!(!m.contains(Point3::new(50.0 + arc_allowance(50.0) + HAIR, 0.0, 20.0)), "air past the gripping face");
    assert!(m.contains(Point3::new(0.0, -(58.0 - HAIR), 5.0)), "the fourth jaw's step");
    let want = body_volume(&c) + 4.0 * (area(&arc_outline(20.0, 58.0, 10.0)) * 10.0 + area(&arc_outline(20.0, 50.0, 10.0)) * 20.0);
    let v = volume(&c.solid(4));
    assert!((v - want).abs() < 1e-6, "outward volume {v} vs {want}");
}

#[test]
fn step_nonsense_is_refused_by_name() {
    let bad = |c: JawChuck, jaws: u32, needle: &str| {
        let e = c.check("3-jaw chuck", jaws).expect_err(needle);
        assert!(e.contains("3-jaw chuck") && e.contains(needle), "{e}");
    };
    let step = |h: f64, d: f64| JawChuck { step_height: h, step_depth: d, ..plain() };
    bad(step(f64::NAN, 8.0), 3, "step height must be a finite length");
    bad(step(10.0, f64::INFINITY), 3, "step depth must be a finite length");
    bad(step(-1.0, 8.0), 3, "step height must be zero (plain jaws) or more");
    bad(step(10.0, -8.0), 3, "step depth must be zero (plain jaws) or more");
    bad(step(10.0, 1e9), 3, "step depth");
    bad(step(10.0, 0.0), 3, "must both be set (stepped jaws) or both be zero");
    bad(step(0.0, 8.0), 3, "must both be set (stepped jaws) or both be zero");
    bad(step(30.0, 8.0), 3, "must be less than the jaw height");
    bad(step(10.0, 20.0), 3, "step depth 20.000 mm reaches the axis");
    // 20 wide steps 5 from the axis: three of them overlap there.
    bad(step(10.0, 15.0), 3, "the steps of 3 jaws, 5.000 mm from the axis, would meet each other");
    step(10.0, 8.0).check("3-jaw chuck", 3).expect("a sane step passes");
}

#[test]
fn inside_grip_nonsense_is_refused_by_name() {
    let bad = |c: JawChuck, jaws: u32, needle: &str| {
        let e = c.check("4-jaw chuck", jaws).expect_err(needle);
        assert!(e.contains("4-jaw chuck") && e.contains(needle), "{e}");
    };
    bad(JawChuck { grip_diameter: 20.0, ..inside() }, 4, "does not go into a 20.000 mm bore");
    bad(JawChuck { grip_diameter: 170.0, ..inside() }, 4, "past the body's 80.000 mm radius");
    bad(JawChuck { step_height: 10.0, step_depth: 31.0, ..inside() }, 4, "plus the step depth), past the body");
    bad(JawChuck { jaw_length: 50.0, ..inside() }, 4, "reaches the axis from a 100.000 mm inside grip");
    bad(JawChuck { jaw_length: 1.0, ..inside() }, 4, "jaw length 1.000 mm is too short");
    // 20 wide jaws ending 5 from the axis overlap there.
    bad(JawChuck { jaw_length: 45.0, ..inside() }, 4, "opened into a 100.000 mm bore would meet each other 5.000 mm from the axis");
    // The plain refusals still stand for an inside grip.
    bad(JawChuck { jaw_width: 0.0, ..inside() }, 4, "jaw width must be more than zero");
    bad(inside(), 13, "2 to 12 jaws");
    inside().check("4-jaw chuck", 4).expect("a sane inside grip passes");
}

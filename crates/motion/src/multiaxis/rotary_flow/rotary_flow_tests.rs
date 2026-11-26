//! Unit law tests of the Rotary Flow kernel (#2824): every refusal
//! names its input, the pass count divides the band, and a small
//! on-model band poses and clears without gouge. The rotor fixtures
//! live in the integration binary (`tests/rotary_flow.rs`).

use super::*;
use crate::cl::MoveKind;
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{cube, Point3, Triangle};

use super::super::rotary_flow_unwrap::{align_pair, Unwrapped};

const TAU: f64 = std::f64::consts::TAU;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The ball the kernel demands (the drilling tests' own tool shape,
/// kind swapped to the ball law).
fn ball(d: f64) -> Tool {
    Tool {
        kind: ToolKind::BallNose,
        diameter: d,
        corner_r: d * 0.5,
        tip_angle: 0.0,
        pitch: 0.0,
        pitch_min: 0.0,
        thread_hand: Default::default(),
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 30.0,
        shank_d: d,
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 20.0,
        material_group: "aluminum".into(),
        center_cutting: true,
        rpm: 10_000.0,
        feed: 500.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        catalog: camcraft_tools::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        turning: None,
        tip_form: Default::default(),
        tip_diameter: 0.0,
        form: None,
    }
}

fn boxy() -> Vec<Triangle> {
    cube(Point3::ZERO, Point3::new(40.0, 40.0, 15.0)).facets.to_vec()
}

/// The stated axis of the box fixture: world Z through the top face.
fn axis() -> Axis {
    Axis {
        origin: p3(20.0, 20.0, 0.0),
        dir: p3(0.0, 0.0, 1.0),
    }
}

fn params<'a>(tool: &'a Tool) -> RotaryFlowParams<'a> {
    RotaryFlowParams {
        tool,
        stepover: Some(2.0),
        tolerance: 0.05,
        clearance: 35.0,
        retract: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

/// The box's top-front edge (a convex edge, on the model).
fn front() -> Vec<Point3> {
    vec![p3(5.0, 0.0, 15.0), p3(20.0, 0.0, 15.0), p3(35.0, 0.0, 15.0)]
}

/// A chain on the top face, in from the edge, winding the same way.
fn inner() -> Vec<Point3> {
    vec![p3(5.0, 5.0, 15.0), p3(20.0, 5.0, 15.0), p3(35.0, 5.0, 15.0)]
}

#[test]
fn a_tool_that_is_not_a_ball_is_refused_by_name() {
    let mut t = ball(6.0);
    t.kind = ToolKind::EndMill;
    t.corner_r = 0.6;
    let e = rotary_flow(
        &[front(), inner()],
        &boxy(),
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap_err();
    assert!(format!("{e}").contains("ball-nose"), "{e}");
}

#[test]
fn fewer_than_two_chains_are_refused_by_name() {
    let t = ball(6.0);
    let none: &[Vec<Point3>] = &[];
    let one: &[Vec<Point3>] = &[front()];
    for (got, chains) in [("0", none), ("1", one)] {
        let e = rotary_flow(chains, &boxy(), &axis(), &params(&t), &AxisControl::default())
            .unwrap_err();
        assert!(
            format!("{e}").contains("at least two picked guide chains") && format!("{e}").contains(got),
            "got {got}: {e}"
        );
    }
}

#[test]
fn an_empty_model_is_refused_by_name() {
    let t = ball(6.0);
    let e = rotary_flow(&[front(), inner()], &[], &axis(), &params(&t), &AxisControl::default())
        .unwrap_err();
    assert!(format!("{e}").contains("no model"), "{e}");
}

#[test]
fn an_off_model_chain_is_refused_by_name_at_its_spot() {
    let t = ball(6.0);
    let floating = vec![p3(5.0, 0.0, 40.0), p3(20.0, 0.0, 40.0), p3(35.0, 0.0, 40.0)];
    let e = rotary_flow(
        &[floating, inner()],
        &boxy(),
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap_err();
    assert!(format!("{e}").contains("stands off the model surface"), "{e}");
    assert_eq!(e.location(), Some(p3(5.0, 0.0, 40.0)));
}

#[test]
fn a_chain_point_on_the_axis_line_is_refused_by_name() {
    let t = ball(6.0);
    let through = vec![p3(5.0, 0.0, 15.0), p3(20.0, 20.0, 15.0), p3(35.0, 0.0, 15.0)];
    let e = rotary_flow(
        &[through, inner()],
        &boxy(),
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap_err();
    assert!(format!("{e}").contains("stands on the rotation axis line"), "{e}");
}

#[test]
fn a_degenerate_band_is_refused_by_name() {
    let t = ball(6.0);
    let e = rotary_flow(
        &[front(), front()],
        &boxy(),
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap_err();
    assert!(format!("{e}").contains("no width to machine"), "{e}");
}

#[test]
fn chains_wound_in_opposite_senses_are_refused_by_name() {
    let t = ball(6.0);
    let mut back = front();
    back.reverse();
    let e = rotary_flow(
        &[front(), back],
        &boxy(),
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap_err();
    assert!(format!("{e}").contains("opposite senses"), "{e}");
}

#[test]
fn a_stepover_at_the_scallop_ceiling_is_refused_by_name() {
    let t = ball(6.0);
    let mut q = params(&t);
    q.stepover = Some(6.0);
    let e = rotary_flow(&[], &[], &axis(), &q, &AxisControl::default()).unwrap_err();
    assert!(format!("{e}").contains("scallop ceiling"), "{e}");
}

#[test]
fn a_tolerance_at_the_scallop_ceiling_is_refused_by_name() {
    let t = ball(6.0);
    let mut q = params(&t);
    q.tolerance = 3.0; // the ball radius: half of it is the ceiling
    let e = rotary_flow(&[], &[], &axis(), &q, &AxisControl::default()).unwrap_err();
    assert!(format!("{e}").contains("scallop ceiling"), "{e}");
}

#[test]
fn a_clearance_below_the_work_is_refused_by_name() {
    let t = ball(6.0);
    let mut q = params(&t);
    q.clearance = 20.0; // the box corner stands 28.28 mm out
    let e = rotary_flow(&[front(), inner()], &boxy(), &axis(), &q, &AxisControl::default())
        .unwrap_err();
    assert!(format!("{e}").contains("does not clear"), "{e}");
}

#[test]
fn the_pass_count_divides_the_band_and_holds_its_ceiling() {
    assert_eq!(pass_count(10.0, 3.0), Ok(5));
    assert_eq!(pass_count(9.0, 3.0), Ok(4));
    assert_eq!(pass_count(0.1, 5.0), Ok(2));
    let e = pass_count(1e9, 1.0).unwrap_err();
    assert!(format!("{e}").contains("pass ceiling"), "{e}");
    let e = pass_count(1.0, 0.0).unwrap_err();
    assert!(format!("{e}").contains("collapsed"), "{e}");
}

#[test]
fn align_pair_takes_the_nearest_whole_turn() {
    // B answers for +0.3 rad and for +0.3 - TAU alike; the band the
    // user picked is the one under half a turn wide.
    let track = |dth: f64| Unwrapped {
        th: (0..5).map(|i| i as f64 + dth).collect(),
        v: vec![0.0; 5],
        rho: vec![8.0; 5],
        u: (0..5).map(|i| i as f64 / 4.0).collect(),
    };
    let a = track(0.0);
    let mut b = track(0.3 - TAU);
    let shift = align_pair(&a, &mut b, 17);
    assert!((shift - TAU).abs() < 1e-9, "{shift}");
    assert!((b.th[0] - 0.3).abs() < 1e-9, "{:?}", b.th);
}

#[test]
fn a_model_with_a_non_finite_facet_is_refused_by_name() {
    // The review's reproduction: one NaN facet on an otherwise valid
    // model. The radius and distance folds dropped the NaN, and the
    // kernel answered Ok.
    let t = ball(6.0);
    let mut facets = boxy();
    facets.push(Triangle::new(
        p3(f64::NAN, 0.0, 0.0),
        p3(0.0, f64::NAN, 0.0),
        p3(0.0, 0.0, f64::NAN),
    ));
    let e = rotary_flow(&[front(), inner()], &facets, &axis(), &params(&t), &AxisControl::default())
        .unwrap_err();
    let want = format!("model facet {} carries a non-finite vertex", facets.len());
    assert!(format!("{e}").contains(&want), "{e}");
}

#[test]
fn the_push_out_march_refuses_a_radius_that_cannot_advance() {
    // At a 1e16 mm radius a quarter-radius step is lost in the float
    // spacing: the march must stop by name, not repeat forever.
    use super::super::rotary_flow_pass::{ball_center, boxes_of, PushFail};
    use super::super::rotary_flow_unwrap::basis;
    let ax = Axis {
        origin: Point3::ZERO,
        dir: p3(0.0, 0.0, 1.0),
    };
    let b = basis(&ax).unwrap();
    let x = 1e16;
    let facets = vec![Triangle::new(p3(x, -1.0, 0.0), p3(x, 1.0, 0.0), p3(x, 0.0, 1.0))];
    let boxes = boxes_of(&facets);
    let got = ball_center(&ax, &b, &facets, &boxes, 0.0, 0.3, x, 1.0, 0.01);
    assert_eq!(got, Err(PushFail::Precision));
    // A station at a sane radius still marches and clears.
    let near = vec![Triangle::new(p3(20.0, -1.0, 0.0), p3(20.0, 1.0, 0.0), p3(20.0, 0.0, 1.0))];
    let nb = boxes_of(&near);
    let c = ball_center(&ax, &b, &near, &nb, -std::f64::consts::FRAC_PI_2, 0.3, 20.0, 1.0, 0.01).unwrap();
    assert!(c.x > 21.0 && c.x < 21.1, "{c:?}");
}

#[test]
fn a_small_band_poses_clears_and_opens_at_clearance() {
    let facets = boxy();
    let t = ball(6.0);
    let path = rotary_flow(
        &[front(), inner()],
        &facets,
        &axis(),
        &params(&t),
        &AxisControl::default(),
    )
    .unwrap();
    assert!(path.is_chained());
    assert!(path.all_finite());
    assert!(!path.moves.is_empty());
    // The program opens ON the clearance pose (M13.8): the first
    // record states the pose where it stands.
    let m0 = &path.moves[0];
    assert!(m0.kind == MoveKind::Rapid && m0.from == m0.to, "{m0:?}");
    let a0 = m0.axis.expect("the opening record states a pose");
    let c0 = p3(m0.to.x + a0[0] * 3.0, m0.to.y + a0[1] * 3.0, m0.to.z + a0[2] * 3.0);
    let d = c0 - axis().origin;
    let s = d.dot(axis().dir);
    let w = d - p3(0.0, 0.0, s);
    assert!(
        (w.dot(w).sqrt() - 35.0).abs() < 1e-6,
        "the opening center rides {w:?}, not the 35 mm clearance cylinder"
    );
    // The pin: every cut station's ball keeps its radius off the part.
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let a = m.axis.expect("a cut station poses");
        let c = p3(m.to.x + a[0] * 3.0, m.to.y + a[1] * 3.0, m.to.z + a[2] * 3.0);
        for f in &facets {
            let d = c - closest_pt_tri(c, &[f.a, f.b, f.c]);
            assert!(
                d.dot(d).sqrt() >= 3.0 - 1e-6,
                "a station's ball at {c:?} gouges the part by {:e}",
                3.0 - d.dot(d).sqrt()
            );
        }
    }
    assert!(
        path.notes.iter().any(|n| n.contains("band 1 spans")),
        "the notes do not state the band: {:?}",
        path.notes
    );
}

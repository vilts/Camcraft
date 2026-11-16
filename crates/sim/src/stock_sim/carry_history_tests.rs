//! Internal refusal and convexity pins for exact side-on stock carry.
//! Provenance: direct interval and sphere fixtures, independent of toolpath generation.

use super::super::carry::Body;
use super::super::carry_side::SideTurn;
use super::*;
use crate::cl::Move;

fn side_frame() -> Frame {
    let turn = SideTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("turn");
    let body = Body::on_floor(
        Dexel::block(0.0, 0.0, 1.0, 1.0, 1.0, 1.0).expect("block"),
        0.0,
    )
    .carry_side(&turn)
    .expect("carry");
    body.dex.history.expect("retained history").frame
}

fn ball_cut(radius: f64) -> Cut {
    let origin = Point3::new(0.0, 0.0, 0.0);
    let motion = Move::new(MoveKind::Feed, origin, origin, Some(300.0));
    let op = SimOp {
        moves: &[],
        radius: 1.0,
        ball: true,
        cone: None,
        corner: None,
    };
    Cut {
        frame: side_frame(),
        shape: Shape::Sweep(Arc::new(Sweep::Ball {
            c0: origin,
            c1: origin,
            r: radius,
        })),
        profile: Arc::new(Profile::new(&op, motion, 0.01)),
    }
}

#[test]
fn an_unbounded_exact_ray_is_refused_by_name() {
    let history = History::block(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
    assert_eq!(
        history.spans(0.5, 0.5).err().as_deref(),
        Some("exact side-on carry cannot read an unbounded stock ray")
    );
}

#[test]
fn too_many_clip_reads_refuse_the_ray_by_name() {
    let mut history = History::block(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
    history.lo.z = 0.0;
    let grid = Dexel::block(0.0, 0.0, 1.0, 1.0, 2.0, 1.0).expect("clip above stock");
    history.clips = vec![
        Clip {
            frame: Frame::IDENTITY,
            grid
        };
        RAY_BUDGET + 1
    ];
    assert_eq!(
        history.spans(0.5, 0.5).err().as_deref(),
        Some("exact side-on carry exhausted its ray budget")
    );
}

#[test]
fn exhausted_subdivision_budget_refuses_the_ray_by_name() {
    assert_eq!(
        ball_cut(1.0)
            .keep(0.0, 0.0, (-1.0, 1.0), &mut 0, &mut Vec::new())
            .err()
            .as_deref(),
        Some("exact side-on carry exhausted its ray budget")
    );
}

#[test]
fn a_non_finite_tool_distance_is_refused_by_name() {
    assert_eq!(
        ball_cut(f64::NAN)
            .keep(0.0, 0.0, (-1.0, 1.0), &mut 100, &mut Vec::new())
            .err()
            .as_deref(),
        Some("exact side-on carry read a non-finite tool distance")
    );
}

#[test]
fn a_non_finite_original_axis_tool_floor_is_refused_by_name() {
    let motion = Move::new(
        MoveKind::Feed,
        Point3::new(-1.0, 0.0, f64::MAX),
        Point3::new(1.0, 0.0, -f64::MAX),
        Some(300.0),
    );
    let op = SimOp {
        moves: &[],
        radius: 1.0,
        ball: false,
        cone: None,
        corner: None,
    };
    let mut cut = ball_cut(1.0);
    cut.frame = Frame::IDENTITY;
    cut.profile = Arc::new(Profile::new(&op, motion, 0.01));
    assert_eq!(
        cut.keep(0.0, 0.0, (0.0, 1.0), &mut 100, &mut Vec::new())
            .err()
            .as_deref(),
        Some("exact side-on carry read a non-finite tool floor")
    );
}

#[test]
fn a_convex_sweep_removes_the_segment_between_surface_endpoints() {
    let mut kept = Vec::new();
    ball_cut(1.0)
        .keep(0.0, 0.0, (-1.0, 1.0), &mut 1000, &mut kept)
        .expect("bounded read");
    assert!(
        kept.is_empty(),
        "the sphere contains its diameter, including both surface endpoints: {kept:?}"
    );
}

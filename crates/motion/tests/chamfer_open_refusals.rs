//! Refusal doors for open chamfer runs (M2.4 B / #3086).
//!
//! Gate map: covers camcraft_motion::machining2d (chamfer, open-run refusals).
//!
//! Pinning tests for open 2D chamfer refusal conditions:
//! 1. A chain that collapses to one point (e.g. single vertex) refuses by name
//!    without mentioning "centerline".
//! 2. A chain holding a collapsed segment (< 1e-9 length) refuses by name.
//! 3. A chain with no model wall at the chamfer depth refuses by name.

use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{chamfer_open_runs, ChamferParams, GenError};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn chamfer_params() -> ChamferParams {
    ChamferParams {
        width: 2.0,
        angle_deg: 45.0,
        minimum_cone_radius: 0.0,
        clearance_z: 25.0,
        retract_z: None,
        top_z: 20.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        inside: false,
    }
}

fn err<T>(r: Result<T, GenError>) -> String {
    match r {
        Ok(_) => panic!("a door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_open_chamfer_collapsing_to_one_point_refuses_by_name() {
    let p = chamfer_params();
    // A single-point run: simple() would previously call this a "centerline run".
    let runs = vec![vec![Point2::new(10.0, 10.0)]];
    let why = err(chamfer_open_runs(
        &runs,
        &p,
        &|_| Ok(Some(rect(-20.0, -20.0, 20.0, 20.0))),
    ));
    assert!(why.contains("collapses to one point"), "{why}");
    assert!(why.contains("re-pick the edge"), "{why}");
    assert!(!why.contains("centerline"), "must not say centerline: {why}");

    // An empty run also collapses without mentioning centerline.
    let empty_runs = vec![vec![]];
    let why_empty = err(chamfer_open_runs(
        &empty_runs,
        &p,
        &|_| Ok(Some(rect(-20.0, -20.0, 20.0, 20.0))),
    ));
    assert!(why_empty.contains("collapses to one point"), "{why_empty}");
    assert!(!why_empty.contains("centerline"), "must not say centerline: {why_empty}");
}

#[test]
fn an_open_chamfer_with_a_collapsed_segment_refuses_by_name() {
    let p = chamfer_params();
    let runs = vec![vec![
        Point2::new(-20.0, 20.0),
        Point2::new(20.0, 20.0),
        Point2::new(20.0, 20.0),
    ]];
    let why = err(chamfer_open_runs(
        &runs,
        &p,
        &|_| Ok(Some(rect(-20.0, -20.0, 20.0, 20.0))),
    ));
    assert!(why.contains("collapsed segment"), "{why}");
    assert!(why.contains("re-pick the edge"), "{why}");
}

#[test]
fn an_open_chamfer_without_a_model_wall_refuses_by_name() {
    let p = chamfer_params();
    let runs = vec![vec![
        Point2::new(-20.0, 20.0),
        Point2::new(20.0, 20.0),
    ]];
    let why = err(chamfer_open_runs(&runs, &p, &|_| Ok(None)));
    assert!(why.contains("no model wall"), "{why}");
    assert!(why.contains("change Top of Part Z"), "{why}");
}

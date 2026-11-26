//! Provenance: issue #2159 sheet hold-down rules and PR #2847 nested retained-stock cases.
//! Gate map: covers camcraft_geometry::nesting::cut_order.

use camcraft_geometry::geo2d::{Loop, Point2};
use camcraft_geometry::nesting::cut_order::{
    CutFeature, CutGeometry, CutPhase, CutRole, CutSide, plan_sheet,
};

fn rectangle(id: u64, low: f64, high: f64) -> CutFeature {
    let ring = Loop::new(vec![
        Point2::new(low, low),
        Point2::new(high, low),
        Point2::new(high, high),
        Point2::new(low, high),
    ])
    .expect("fixture rectangle");
    CutFeature {
        id,
        geometry: CutGeometry::Closed(ring),
        side: CutSide::Auto,
        passes: vec![CutPhase::Breakthrough],
        release_phase: Some(CutPhase::Breakthrough),
    }
}

fn assert_hole_inside_retained_contour(phase: CutPhase) {
    let mut retained = rectangle(2, 2.0, 18.0);
    retained.passes = vec![phase];
    retained.release_phase = None;
    let features = [rectangle(1, 0.0, 20.0), retained, rectangle(3, 5.0, 8.0)];
    let plan = plan_sheet(&features, 0.2).expect("retained contour does not separate material");
    let hole = plan
        .features
        .iter()
        .find(|feature| feature.id == 3)
        .expect("hole");
    assert_eq!(
        hole.parent,
        Some(2),
        "keep the geometric parent for scheduling"
    );
    assert_eq!(hole.depth, 1, "only the outer part releases material");
    assert_eq!((hole.role, hole.side), (CutRole::Hole, CutSide::Inside));
    let corners: Vec<_> = plan
        .corner_limits
        .iter()
        .filter(|corner| corner.feature_id == 3)
        .collect();
    assert_eq!(
        corners.len(),
        4,
        "the cutter cannot reach the four inside corners"
    );
    for (vertex, corner) in corners.iter().enumerate() {
        assert_eq!(corner.vertex, vertex);
        assert!((corner.angle_radians - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((corner.min_unreachable_distance - 0.1 * (2.0_f64.sqrt() - 1.0)).abs() < 1e-12);
    }
}

#[test]
fn through_hole_inside_engraving_uses_material_depth() {
    assert_hole_inside_retained_contour(CutPhase::Marking);
}

#[test]
fn through_hole_inside_blind_pocket_uses_material_depth() {
    assert_hole_inside_retained_contour(CutPhase::Internal);
}

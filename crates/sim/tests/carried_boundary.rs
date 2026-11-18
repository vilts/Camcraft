//! #1580: continuous carried-stock boundary classification, axis-box mesh queries,
//! and sub-pitch boundary penetration helpers in `camcraft-sim`.
//! Provenance: our own geometry and rigid-turn fixtures.
//! Gate map (#1580): covers sim::fixture_mesh (column, prism), sim::stock_sim
//! (carry_boundary, carry_boundary_prune, carried_boundary_scan, carry, carry_history).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::fixture_mesh::{BoxSteel, FixtureMesh};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::stock::StockShape;
use camcraft_sim::stock_sim::{
    Body, CarriedStock, CarryTurn, Dexel, RegionStock, SimOp, boundary_box_penetration, boundary_mesh_penetration,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn closed_boxes(boxes: &[(Point3, Point3)]) -> (Vec<Triangle>, FixtureMesh, FixtureBox) {
    let tris: Vec<Triangle> = boxes.iter().flat_map(|&(a, b)| cube(a, b).facets.iter().copied().collect::<Vec<_>>()).collect();
    let mesh = FixtureMesh::build(&tris).expect("closed mesh");
    let (mut lo, mut hi) = (
        p(f64::INFINITY, f64::INFINITY, f64::INFINITY),
        p(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for t in &tris {
        for q in [t.a, t.b, t.c] {
            lo = p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
            hi = p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
        }
    }
    (tris, mesh, FixtureBox { min: lo, max: hi })
}

fn near(got: Option<Vec<(f64, f64)>>, expected: &[(f64, f64)]) -> bool {
    got.is_some_and(|s| s.len() == expected.len() && s.iter().zip(expected).all(|(a, b)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9))
}

#[test]
fn steel_on_axis_line_reads_all_signed_axes_and_rejects_bad_inputs() {
    let (_, mesh, _) = closed_boxes(&[(p(2.0, 3.0, 1.0), p(8.0, 9.0, 5.0))]);
    let mut budget = u64::MAX;
    assert!(near(mesh.steel_on_axis_line(p(0.0, 5.5, 3.2), [1.0, 0.0, 0.0], &mut budget), &[(2.0, 8.0)]));
    assert!(near(mesh.steel_on_axis_line(p(10.0, 5.5, 3.2), [-1.0, 0.0, 0.0], &mut budget), &[(2.0, 8.0)]));
    assert!(near(mesh.steel_on_axis_line(p(4.5, 1.0, 3.2), [0.0, 1.0, 0.0], &mut budget), &[(2.0, 8.0)]));
    assert!(near(mesh.steel_on_axis_line(p(4.5, 11.0, 3.2), [0.0, -1.0, 0.0], &mut budget), &[(2.0, 8.0)]));
    assert!(near(mesh.steel_on_axis_line(p(4.5, 5.5, 0.0), [0.0, 0.0, 1.0], &mut budget), &[(1.0, 5.0)]));
    assert!(near(mesh.steel_on_axis_line(p(4.5, 5.5, 7.0), [0.0, 0.0, -1.0], &mut budget), &[(2.0, 6.0)]));
    assert_eq!(mesh.steel_on_axis_line(p(0.0, 5.5, 3.2), [1.0, 1.0, 0.0], &mut budget), None);
    assert_eq!(mesh.steel_on_axis_line(p(f64::NAN, 5.5, 3.2), [1.0, 0.0, 0.0], &mut budget), None);
    let mut zero = 0;
    assert_eq!(mesh.steel_on_axis_line(p(0.0, 5.5, 3.2), [1.0, 0.0, 0.0], &mut zero), None);
}

#[test]
fn steel_in_axis_box_and_uv_cover_all_axes_and_guards() {
    let (_, mesh, _) = closed_boxes(&[(p(2.0, 3.0, 1.0), p(8.0, 9.0, 5.0))]);
    let mut budget = u64::MAX;
    assert_eq!(mesh.steel_in_axis_box(None, [(3.0, 6.0), (4.0, 7.0), (2.0, 4.0)], 2, 0.01, &mut budget), BoxSteel::WhollyInside(2.0));
    assert_eq!(mesh.steel_in_axis_box(None, [(3.0, 6.0), (4.0, 7.0), (5.0, 8.0)], 2, 0.01, &mut budget), BoxSteel::Clear);
    assert_eq!(mesh.steel_in_axis_box(None, [(3.0, 6.0), (4.0, 7.0), (0.0, 3.0)], 0, 0.01, &mut budget), BoxSteel::Boundary(3.0));
    assert_eq!(mesh.steel_in_axis_box(None, [(3.0, 6.0), (4.0, 7.0), (0.0, 3.0)], 1, 0.01, &mut budget), BoxSteel::Boundary(3.0));
    assert_eq!(mesh.steel_in_axis_box(None, [(3.0, 6.0), (4.0, 7.0), (0.0, 3.0)], 3, 0.01, &mut budget), BoxSteel::Unread);
    assert_eq!(mesh.steel_in_axis_box(None, [(6.0, 3.0), (4.0, 7.0), (0.0, 3.0)], 2, 0.01, &mut budget), BoxSteel::Unread);

    let (kind, uv) = mesh.steel_in_axis_box_uv(None, [(0.0, 10.0), (0.0, 12.0), (1.0, 5.0)], 2, 0.01, &mut budget);
    assert_eq!(kind, BoxSteel::Boundary(4.0));
    assert!((uv[0].0 - 2.0).abs() < 1e-9 && (uv[0].1 - 8.0).abs() < 1e-9 && (uv[1].0 - 3.0).abs() < 1e-9 && (uv[1].1 - 9.0).abs() < 1e-9, "{uv:?}");
}

#[test]
fn concave_l_model_stock_clears_empty_notch_and_catches_sub_pitch_step() {
    let (tris, _, _) = closed_boxes(&[(p(0.0, 0.0, 0.0), p(2.4, 5.0, 10.0)), (p(2.4, 0.0, 0.0), p(5.0, 2.4, 10.0))]);
    let body = Body::on_mesh(Dexel::block(0.0, 0.0, 5.0, 5.0, 10.0, 1.0).expect("grid"), &tris);
    assert!(body.has_continuous_history());
    let mut b = u64::MAX;
    assert_eq!(body.classify_region((2.5, 4.5), (2.5, 4.5), (1.0, 9.0), 0.01, 0.01, &mut b), RegionStock::Empty);
    assert_eq!(body.classify_region((0.2, 2.0), (0.2, 2.0), (1.0, 9.0), 0.01, 0.01, &mut b), RegionStock::Full(vec![(0.0, 10.0)]));

    let (_, clear_mesh, clear_box) = closed_boxes(&[(p(2.45, 2.45, 2.0), p(4.8, 4.8, 8.0))]);
    assert_eq!(boundary_box_penetration(&body, &clear_box, None), None);
    assert_eq!(boundary_mesh_penetration(&body, &clear_box, &clear_mesh, 0.01, &mut b, None), None);

    let (_, hit_mesh, hit_box) = closed_boxes(&[(p(2.35, 3.0, 2.0), p(4.5, 4.5, 8.0))]);
    assert_eq!(boundary_box_penetration(&body, &hit_box, None), Some(6.0));
    assert_eq!(boundary_mesh_penetration(&body, &hit_box, &hit_mesh, 0.01, &mut b, None), Some(6.0));

    let open = Body::on_mesh(Dexel::block(0.0, 0.0, 5.0, 5.0, 10.0, 1.0).expect("grid"), &tris[..tris.len() - 1]);
    assert!(!open.has_continuous_history());
    assert!(matches!(open.classify_region((0.2, 2.0), (0.2, 2.0), (1.0, 9.0), 0.01, 0.01, &mut b), RegionStock::Unread(_)));
}

#[test]
fn arc_cut_side_turn_and_cylinder_classify_and_detect_sub_pitch_overlap() {
    let dex = Dexel::from_shape(&StockShape::Cylinder { diameter: 8.6, height: 8.0 }, 1.0).expect("cyl grid");
    let cyl = Body::on_floor(dex, 0.0);
    let side = CarryTurn::from_matrix([[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]], [0.0; 3]).expect("turn");
    let turned_cyl = CarriedStock::Column(cyl).carry_through(&side).expect("carry").into_body().expect("body");
    let zero_box = FixtureBox { min: p(-1.0, -4.0, 4.25), max: p(1.0, -4.0, 4.25) };
    assert!(boundary_box_penetration(&turned_cyl, &zero_box, None).is_none());
    let hit_cyl = FixtureBox { min: p(-1.0, -5.0, 4.25), max: p(1.0, -3.0, 5.0) };
    assert!(boundary_box_penetration(&turned_cyl, &hit_cyl, None).is_some());

    let mut stock = CarriedStock::Column(Body::on_floor(Dexel::block(0.0, 0.0, 10.0, 10.0, 10.0, 1.0).expect("grid"), 0.0));
    let arc = [Move::new(MoveKind::Feed, p(8.0, 5.0, 4.0), p(5.0, 8.0, 4.0), Some(300.0)).with_arc(Point2::new(5.0, 5.0), true)];
    stock.simulate(&[SimOp { moves: &arc, radius: 3.0, ball: false, cone: None, corner: None }]).expect("arc cut");
    let body = stock.into_body().expect("body");
    let mut budget = u64::MAX;
    assert_eq!(body.classify_region((5.5, 7.5), (5.5, 7.5), (5.0, 9.0), 0.01, 0.01, &mut budget), RegionStock::Empty);
    assert_eq!(body.classify_region((5.5, 7.5), (5.5, 7.5), (0.0, 10.0), 0.01, 0.01, &mut budget), RegionStock::Full(vec![(0.0, 4.0)]));

    let mut turned = CarriedStock::Column(Body::on_floor(Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("grid"), 0.0))
        .carry_through(&side)
        .expect("side turn");
    turned.clip_under(&Dexel::block(0.0, -6.0, 12.0, 0.0, 5.0, 1.0).expect("clip"));
    let clip_body = turned.into_body().expect("body");
    assert_eq!(clip_body.classify_region((2.0, 10.0), (-5.0, -1.0), (5.2, 9.0), 0.01, 0.01, &mut budget), RegionStock::Empty);
}

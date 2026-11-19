//! A Contour on an open edge of a part (#1777): the tool stands one
//! radius off the edge on the side without material, whatever the Side
//! setting says, so the pass never cuts the part the edge bounds - and
//! the program posts clean. Before the fix the tool centre rode the edge,
//! the verifier flagged the gouge and the post was blocked.
//!
//! Gate map (#1777, #2277): covers app::operation (generate, selection,
//! contour_open), motion machining2d (contour_run_side), app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::posting;

/// Generate "C1" on the block from `picks` and return its cut feeds
/// (below the block top), its warning count and its notes.
fn cut(picks: Vec<GeoSel>, side: Side) -> (Vec<Point3>, usize, Vec<String>) {
    let world = block_world();
    let mut h = History::default();
    let create = CreateOperation::new(&world, "C1", TOOL, SrcRef::Selection(picks), contour(10.0, 0.0, side));
    h.execute(&world, create.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("a toolpath");
    let below: Vec<Point3> = feeds(&c.path).into_iter().filter(|p| p.z < 10.0 - EPS).collect();
    assert!(
        posting::post_setup_unwritten(&world).is_ok(),
        "the program posts: {:?}",
        posting::post_setup_unwritten(&world).err()
    );
    (below, c.warnings.len(), c.path.notes.clone())
}

fn north(world: &World) -> FeatureSel {
    rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9)
}

fn east(world: &World) -> FeatureSel {
    rim_edge(world, |q| (q[0] - 20.0).abs() < 1e-9)
}

/// How far `p` stands outside the 40 x 40 block.
fn off_block(p: &Point3) -> f64 {
    (p.x.abs() - 20.0).max(0.0).hypot((p.y.abs() - 20.0).max(0.0))
}

#[test]
fn an_open_edge_of_a_part_is_cut_on_its_air_side_for_every_side() {
    let w = block_world();
    for side in [Side::Auto, Side::Inside, Side::Outside] {
        let (f, warnings, notes) = cut(pick("part", north(&w)), side);
        assert!(!f.is_empty(), "{side:?}: the edge is cut");
        assert!(f.iter().all(|p| (p.y - (20.0 + R)).abs() < EPS), "{side:?}: one radius north of the edge: {f:?}");
        assert!(f.iter().all(|p| p.x > -20.0 - EPS && p.x < 20.0 + EPS), "{side:?}: no further than the edge's ends");
        assert_eq!(warnings, 0, "{side:?}: nothing gouges");
        assert!(notes.iter().any(|n| n.contains("air side")), "{side:?}: {notes:?}");
    }
}

#[test]
fn two_open_edges_turn_the_corner_on_their_air_side() {
    let w = block_world();
    let mut picks = pick("part", north(&w));
    picks.extend(pick("part", east(&w)));
    let (f, warnings, _) = cut(picks, Side::Inside);
    assert!(!f.is_empty());
    assert!(f.iter().all(|p| off_block(p) > R - EPS), "the tool stays a radius off the block: {f:?}");
    assert_eq!(warnings, 0);
}

/// Exact U-notched block from #2277 (`2d_contour_test.camproj`):
/// (0,0)-(22,0)-(22,10)-(42,10)-(42,0)-(64,0)-(64,25)-(0,25), z = 0..5.
fn u_notch_world() -> World {
    use camcraft_app::geometry::{Mesh, Triangle};
    let mut f = Vec::new();
    let p = |x, y, z| Point3::new(x, y, z);
    let tris = [
        ((0.0, 25.0), (0.0, 0.0), (22.0, 10.0)),
        ((22.0, 0.0), (22.0, 10.0), (0.0, 0.0)),
        ((42.0, 10.0), (0.0, 25.0), (22.0, 10.0)),
        ((64.0, 0.0), (42.0, 10.0), (42.0, 0.0)),
        ((64.0, 25.0), (42.0, 10.0), (64.0, 0.0)),
        ((42.0, 10.0), (64.0, 25.0), (0.0, 25.0)),
    ];
    for (a, b, c) in tris {
        f.push(Triangle::new(p(a.0, a.1, 5.0), p(b.0, b.1, 5.0), p(c.0, c.1, 5.0)));
        f.push(Triangle::new(p(a.0, a.1, 0.0), p(c.0, c.1, 0.0), p(b.0, b.1, 0.0)));
    }
    let ring = [
        (0.0, 0.0), (22.0, 0.0), (22.0, 10.0), (42.0, 10.0),
        (42.0, 0.0), (64.0, 0.0), (64.0, 25.0), (0.0, 25.0),
    ];
    for i in 0..ring.len() {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % ring.len()];
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 0.0), p(x1, y1, 5.0)));
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 5.0), p(x0, y0, 5.0)));
    }
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(f)));
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 64.0, dy: 31.0, dz: 5.0 },
            Location { pos: Point3::new(32.0, 9.5, 0.0), ..Default::default() },
        )
        .expect("stock"),
    );
    for tname in ["EM-0400-2FL", TOOL] {
        let tool = catalog().into_iter().find(|(n, _)| *n == tname).map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, tname, tool).expect("tool"));
    }
    world
}

fn edge_matching(world: &World, pred: impl Fn([f64; 3], [f64; 3]) -> bool) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .find(|c| {
            !c.closed && {
                let p = feature::chain_points(&f, c);
                p.len() == 2 && (pred(p[0], p[1]) || pred(p[1], p[0]))
            }
        })
        .map(|c| FeatureSel::Chain(c.id))
        .expect("matching rim edge")
}

#[test]
fn u_notch_picked_open_edges_turn_inside_corners() {
    let world = u_notch_world();
    let on = |a: [f64; 3], b: [f64; 3], ax: f64, ay: f64, bx: f64, by: f64| {
        (a[2] - 5.0).abs() < 1e-6
            && (b[2] - 5.0).abs() < 1e-6
            && (a[0] - ax).hypot(a[1] - ay) < 1e-6
            && (b[0] - bx).hypot(b[1] - by) < 1e-6
    };
    let picks = vec![
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| on(a, b, 42.0, 10.0, 42.0, 0.0)) },
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| on(a, b, 22.0, 10.0, 42.0, 10.0)) },
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| on(a, b, 22.0, 0.0, 22.0, 10.0)) },
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| on(a, b, 0.0, 0.0, 22.0, 0.0)) },
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| on(a, b, 42.0, 0.0, 64.0, 0.0)) },
    ];
    let mut params = contour(5.0, -1.0, Side::Auto);
    if let OpParams::Contour { lead, stepdown, .. } = &mut params {
        *lead = Lead::Linear { length: 8.0 };
        *stepdown = 5.0;
    }
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "C1", "EM-0400-2FL", SrcRef::Selection(picks), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let op_val = model::operation_of(&world, &e).expect("op");
    assert_eq!(operation::chains_of(&world, &op_val).len(), 1, "5 connected edges form 1 open chain");
    let c = model::toolpath_of(&world, &e).expect("toolpath");
    assert_eq!(c.warnings.len(), 0, "no gouges: {:?}", c.warnings);
    assert!(posting::post_setup_unwritten(&world).is_ok(), "posts clean: {:?}", posting::post_setup_unwritten(&world).err());
    let below: Vec<Point3> = feeds(&c.path).into_iter().filter(|p| p.z < 5.0 - EPS).collect();
    assert!(!below.is_empty());
    assert!(below.iter().any(|p| (p.x - 24.0).abs() < 0.05 && (p.y - 8.0).abs() < 0.05), "left corner: {below:?}");
    assert!(below.iter().any(|p| (p.x - 40.0).abs() < 0.05 && (p.y - 8.0).abs() < 0.05), "right corner: {below:?}");
}

#[test]
fn acute_inside_corner_and_oversized_tool_succeed_open_and_closed() {
    use camcraft_app::geometry::{Mesh, Triangle};
    let poly = [
        (0.0, 0.0), (16.0, 0.0), (16.0, 8.0), (20.0, 12.0), (24.0, 8.0),
        (24.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0),
    ];
    let mut f = Vec::new();
    let p = |x, y, z| Point3::new(x, y, z);
    let top_tris = [
        ((0.0, 0.0), (16.0, 0.0), (16.0, 8.0)),
        ((0.0, 0.0), (16.0, 8.0), (16.0, 30.0)),
        ((0.0, 0.0), (16.0, 30.0), (0.0, 30.0)),
        ((24.0, 0.0), (40.0, 0.0), (24.0, 8.0)),
        ((40.0, 0.0), (40.0, 30.0), (24.0, 8.0)),
        ((24.0, 8.0), (40.0, 30.0), (24.0, 30.0)),
        ((16.0, 8.0), (20.0, 12.0), (16.0, 30.0)),
        ((20.0, 12.0), (24.0, 8.0), (24.0, 30.0)),
        ((20.0, 12.0), (24.0, 30.0), (16.0, 30.0)),
    ];
    for (a, b, c) in top_tris {
        f.push(Triangle::new(p(a.0, a.1, 5.0), p(b.0, b.1, 5.0), p(c.0, c.1, 5.0)));
        f.push(Triangle::new(p(a.0, a.1, 0.0), p(c.0, c.1, 0.0), p(b.0, b.1, 0.0)));
    }
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 0.0), p(x1, y1, 5.0)));
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 5.0), p(x0, y0, 5.0)));
    }
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(f)));
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 30.0, dz: 5.0 },
            Location { pos: Point3::new(20.0, 15.0, 0.0), ..Default::default() },
        )
        .expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));

    let on = |a: [f64; 3], b: [f64; 3], ax: f64, ay: f64, bx: f64, by: f64| {
        (a[2] - 5.0).abs() < 1e-6
            && (b[2] - 5.0).abs() < 1e-6
            && (a[0] - ax).hypot(a[1] - ay) < 1e-6
            && (b[0] - bx).hypot(b[1] - by) < 1e-6
    };
    let open_picks: Vec<GeoSel> = (0..6)
        .map(|i| GeoSel {
            entity: "part".into(),
            feature: edge_matching(&world, |a, b| on(a, b, poly[i].0, poly[i].1, poly[i + 1].0, poly[i + 1].1)),
        })
        .collect();
    h.execute(
        &world,
        CreateOperation::new(&world, "Open", TOOL, SrcRef::Selection(open_picks), contour(5.0, 0.0, Side::Auto)).expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Open").expect("generate open"));
    let oe = model::find_by_name(&world, "Open").expect("op");
    assert_eq!(model::toolpath_of(&world, &oe).expect("tp").warnings.len(), 0);

    h.execute(
        &world,
        CreateOperation::new(
            &world,
            "Closed",
            TOOL,
            SrcRef::Selection(pick("part", top_face(&world, "part", 5.0))),
            contour(5.0, 0.0, Side::Auto),
        )
        .expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Closed").expect("generate closed"));
    let ce = model::find_by_name(&world, "Closed").expect("op");
    assert_eq!(model::toolpath_of(&world, &ce).expect("tp").warnings.len(), 0);
    assert!(posting::post_setup_unwritten(&world).is_ok());
}

#[test]
fn filleted_inside_corner_smaller_than_tool_radius_succeeds_as_open_runs() {
    use camcraft_app::geometry::{Mesh, Triangle};
    let mut poly = vec![(0.0, 0.0), (20.0, 0.0)];
    for k in 0..=8 {
        let a = std::f64::consts::PI - std::f64::consts::FRAC_PI_2 * f64::from(k) / 8.0;
        poly.push((22.0 + 2.0 * a.cos(), 8.0 + 2.0 * a.sin()));
    }
    poly.extend([(40.0, 10.0), (40.0, 30.0), (0.0, 30.0)]);
    let mut f = Vec::new();
    let p = |x, y, z| Point3::new(x, y, z);
    let hub = (10.0, 20.0);
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        f.push(Triangle::new(p(hub.0, hub.1, 5.0), p(x0, y0, 5.0), p(x1, y1, 5.0)));
        f.push(Triangle::new(p(hub.0, hub.1, 0.0), p(x1, y1, 0.0), p(x0, y0, 0.0)));
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 0.0), p(x1, y1, 5.0)));
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 5.0), p(x0, y0, 5.0)));
    }
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(f)));
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 30.0, dz: 5.0 },
            Location { pos: Point3::new(20.0, 15.0, 0.0), ..Default::default() },
        )
        .expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let e = model::find_by_name(&world, "part").expect("part");
    let feat = model::features_of(&world, &e).expect("features");
    let picks: Vec<GeoSel> = feature::edge_runs(&feat)
        .into_iter()
        .filter(|r| {
            !r.closed && {
                let pts = feature::chain_points(&feat, r);
                pts.iter().all(|q| (q[2] - 5.0).abs() < 1e-6 && q[0] >= 20.0 - 1e-6 && q[1] <= 10.0 + 1e-6)
            }
        })
        .map(|r| GeoSel { entity: "part".into(), feature: FeatureSel::Chain(r.id) })
        .collect();
    assert!(picks.len() >= 2, "filleted inside corner splits into multiple open runs: {}", picks.len());
    h.execute(
        &world,
        CreateOperation::new(&world, "C1", TOOL, SrcRef::Selection(picks), contour(5.0, 0.0, Side::Auto)).expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let oe = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &oe).expect("tp");
    assert_eq!(tp.warnings.len(), 0, "no gouges: {:?}", tp.warnings);
    assert!(posting::post_setup_unwritten(&world).is_ok());
    let below: Vec<Point3> = feeds(&tp.path).into_iter().filter(|q| q.z < 5.0 - EPS).collect();
    assert!(!below.is_empty());
    assert!(below.iter().all(|q| q.x >= 25.0 - EPS || q.y <= 5.0 + EPS), "{below:?}");
}

#[test]
fn a_contour_open_boundary_with_a_short_inside_corner_leg_stops_clear() {
    use camcraft_app::geometry::{Mesh, Triangle};
    let poly = [(-10.0, -10.0), (3.0, -10.0), (3.0, 0.0), (0.0, 0.0), (0.0, 20.0), (-10.0, 20.0)];
    let mut f = Vec::new();
    let p = |x, y, z| Point3::new(x, y, z);
    let top_tris = [
        ((-10.0, -10.0), (3.0, -10.0), (3.0, 0.0)),
        ((-10.0, -10.0), (3.0, 0.0), (0.0, 0.0)),
        ((-10.0, -10.0), (0.0, 0.0), (0.0, 20.0)),
        ((-10.0, -10.0), (0.0, 20.0), (-10.0, 20.0)),
    ];
    for (a, b, c) in top_tris {
        f.push(Triangle::new(p(a.0, a.1, 5.0), p(b.0, b.1, 5.0), p(c.0, c.1, 5.0)));
        f.push(Triangle::new(p(a.0, a.1, 0.0), p(c.0, c.1, 0.0), p(b.0, b.1, 0.0)));
    }
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 0.0), p(x1, y1, 5.0)));
        f.push(Triangle::new(p(x0, y0, 0.0), p(x1, y1, 5.0), p(x0, y0, 5.0)));
    }
    let world = world_with(Some(ImportModel::from_mesh("part", Mesh::from_facets(f))), 0.0, 5.0);
    let top = top_face(&world, "part", 5.0);
    let picks = vec![
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| (a[2] - 5.0).abs() < 1e-6 && (b[2] - 5.0).abs() < 1e-6 && ((a[0].hypot(a[1] - 20.0) < 1e-6 && b[0].hypot(b[1]) < 1e-6) || (b[0].hypot(b[1] - 20.0) < 1e-6 && a[0].hypot(a[1]) < 1e-6))) },
        GeoSel { entity: "part".into(), feature: edge_matching(&world, |a, b| (a[2] - 5.0).abs() < 1e-6 && (b[2] - 5.0).abs() < 1e-6 && ((a[0].hypot(a[1]) < 1e-6 && (b[0] - 3.0).hypot(b[1]) < 1e-6) || (b[0].hypot(b[1]) < 1e-6 && (a[0] - 3.0).hypot(a[1]) < 1e-6))) },
    ];
    let a = AreaControls { open: Some(OpenBoundary { picks, side: ChainSide::Outside }), ..Default::default() };
    let (t, warns) = run(&world, &op(sel("part", top), contour(5.0, 0.0, Side::Auto), a)).expect("open contour stops clear");
    assert_eq!(warns, 0, "short inside-corner leg must not gouge: {t:?}");
    let cut: Vec<Point3> = feeds(&t).into_iter().filter(|q| q.z < 5.0 - EPS).collect();
    assert!(!cut.is_empty());
    assert!(cut.iter().all(|q| q.x >= 5.0 - EPS), "stays clear of x = 0: {cut:?}");
}

//! #692 step 1: the one feature graph and pick rule - welded faces,
//! feature edges and feature points only, visible first, never through
//! the part unless asked. Pins the issue's "Done means" cases in both
//! perspective and orthographic rays, plus degenerate input.
//!
//! Gate map (#692): covers app::pick::feature_graph, app::pick::feature_arc,
//! app::pick::feature_pick, app::pick (ray_triangle).

use std::sync::Arc;

use camcraft_app::commands::{Command, ImportModel};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle, cube, cylinder};
use camcraft_app::model;
use camcraft_app::operation::FeatureSel;
use camcraft_app::pick::feature_arc::{circumcircle, fit_arc};
use camcraft_app::pick::{
    FeaturePick, PickBody, PickKinds, PickQuery, PickRay, PickTarget, PointKind, Projection, feature_graph,
    pick_bodies, pick_feature,
};
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn body(name: &str, tris: Vec<Triangle>) -> PickBody {
    PickBody::new(name, Arc::from(tris), Location::default())
}

fn block() -> Vec<Triangle> {
    cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0)).facets.to_vec()
}

fn mesh_tris(m: Mesh) -> Vec<Triangle> {
    m.facets.to_vec()
}

fn query(origin: [f64; 3], dir: [f64; 3], projection: Projection, kinds: PickKinds) -> PickQuery {
    PickQuery { ray: PickRay { origin, dir, projection }, radius: 1.0, radius_per_depth: 0.0, kinds, through: false }
}

/// A ray from `eye` aimed at `at`.
fn aim(eye: [f64; 3], at: [f64; 3], projection: Projection, kinds: PickKinds) -> PickQuery {
    query(eye, [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]], projection, kinds)
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol && (a[2] - b[2]).abs() <= tol
}

fn edge_ends(b: &PickBody, pick: &FeaturePick) -> ([f64; 3], [f64; 3]) {
    let PickTarget::Edge { edge } = pick.target else { panic!("not an edge: {pick:?}") };
    b.graph.edge_ends(edge).expect("edge resolves")
}

#[test]
fn flat_face_triangulation_edges_are_not_pickable() {
    let b = body("block", block());
    // A box: 12 real edges and 8 corners, whatever its triangulation.
    assert_eq!(b.graph.edges.len(), 12, "only the box's own edges are feature edges");
    assert_eq!(b.graph.points.len(), 8);
    assert!(b.graph.points.iter().all(|p| p.kind == PointKind::Corner));
    // Straight down onto the top face's diagonal (0,0)-(20,20).
    for proj in [Projection::Ortho, Projection::Perspective] {
        let mut q = query([10.0, 10.0, 50.0], [0.0, 0.0, -1.0], proj, PickKinds::EDGES);
        q.radius = 2.0;
        assert_eq!(pick_bodies(std::slice::from_ref(&b), &q), None, "the diagonal is no edge ({proj:?})");
        q.through = true;
        assert_eq!(pick_bodies(std::slice::from_ref(&b), &q), None, "not even picking through");
        q.through = false;
        q.kinds = PickKinds::ALL;
        let pick = pick_bodies(std::slice::from_ref(&b), &q).expect("the face");
        let PickTarget::Face { face, sel, .. } = pick.target else { panic!("not a face: {pick:?}") };
        assert_eq!(b.graph.faces[face as usize].facets.len(), 2, "the whole top face, both triangles");
        assert!(matches!(sel, Some(FeatureSel::Face(_))), "a flat face keys as a Face");
        assert!(near(pick.at, [10.0, 10.0, 10.0], 1e-9));
    }
}

#[test]
fn cylinder_facet_edges_are_not_pickable_but_its_rim_is() {
    let b = body("rod", mesh_tris(cylinder(20.0, 10.0, 48)));
    let raw = b.graph.features.edges.len();
    assert_eq!(b.graph.edges.len(), 96, "two 48-segment rims, nothing else");
    assert!(raw > 96, "the topology also carries the strip seams ({raw}); the graph filters them");
    assert!(b.graph.points.is_empty(), "a round rim has no corners");
    // Its two circles, with centres.
    assert_eq!(b.graph.arcs.len(), 2);
    for z in [0.0, 10.0] {
        let a = b.graph.arcs.iter().find(|a| (a.centre[2] - z).abs() < 1e-6).expect("rim circle");
        assert!(a.closed && (a.radius - 10.0).abs() < 1e-6, "{a:?}");
        assert!(near(a.centre, [0.0, 0.0, z], 1e-6), "{a:?}");
        assert_eq!(a.edges.len(), 48);
    }
    let one = std::slice::from_ref(&b);
    // Straight at the wall's facet seam (x = 10, y = 0): no edge there.
    for proj in [Projection::Ortho, Projection::Perspective] {
        let q = query([100.0, 0.0, 5.0], [-1.0, 0.0, 0.0], proj, PickKinds::EDGES);
        assert_eq!(pick_bodies(one, &q), None, "a facet seam is no edge ({proj:?})");
        let q = query([100.0, 0.0, 5.0], [-1.0, 0.0, 0.0], proj, PickKinds::ALL);
        let pick = pick_bodies(one, &q).expect("the wall");
        let PickTarget::Face { face, sel, .. } = pick.target else { panic!("not a face: {pick:?}") };
        assert_eq!(b.graph.faces[face as usize].facets.len(), 96, "the whole wall");
        assert!(matches!(sel, Some(FeatureSel::Patch(_))), "a smooth wall keys as a Patch");
    }
    // At the top rim from above and outside: the rim edge.
    for proj in [Projection::Ortho, Projection::Perspective] {
        let q = aim([60.0, 3.0, 40.0], [10.0, 0.0, 10.0], proj, PickKinds::ALL);
        let pick = pick_bodies(one, &q).expect("the rim");
        let (p, r) = edge_ends(&b, &pick);
        assert!((p[2] - 10.0).abs() < 1e-9 && (r[2] - 10.0).abs() < 1e-9, "top rim edge, got {p:?} {r:?}");
    }
    // The cap's fan centre is an interior vertex - but the rim's centre
    // is there when centres are asked for.
    let q = query([0.3, 0.0, 50.0], [0.0, 0.0, -1.0], Projection::Ortho, PickKinds::POINTS);
    assert_eq!(pick_bodies(one, &q), None, "an interior vertex is no point");
    let q = query([0.3, 0.0, 50.0], [0.0, 0.0, -1.0], Projection::Ortho, PickKinds::CENTRES);
    let pick = pick_bodies(one, &q).expect("the rim centre");
    assert!(matches!(pick.target, PickTarget::Centre { .. }));
    assert!(near(pick.at, [0.0, 0.0, 10.0], 1e-6), "{pick:?}");
}

/// The block with its top face fanned from (10, 10, 10): an interior
/// vertex inside a flat face.
fn fanned_block() -> Vec<Triangle> {
    let mut tris: Vec<Triangle> = block().into_iter().filter(|t| !(t.a.z == 10.0 && t.b.z == 10.0 && t.c.z == 10.0)).collect();
    let c = Point3::new(10.0, 10.0, 10.0);
    let ring = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
    for k in 0..4 {
        let (p, q) = (ring[k], ring[(k + 1) % 4]);
        tris.push(Triangle::new(c, Point3::new(p[0], p[1], 10.0), Point3::new(q[0], q[1], 10.0)));
    }
    tris
}

#[test]
fn a_box_corner_is_a_point_but_a_vertex_inside_a_face_is_not() {
    let b = body("block", block());
    for proj in [Projection::Ortho, Projection::Perspective] {
        let q = aim([60.0, -40.0, 40.0], [20.2, 0.1, 9.9], proj, PickKinds::ALL);
        let pick = pick_bodies(std::slice::from_ref(&b), &q).expect("the corner");
        let PickTarget::Point(p) = pick.target else { panic!("not a point: {pick:?}") };
        assert_eq!(p.kind, PointKind::Corner);
        assert!(near(pick.at, [20.0, 0.0, 10.0], 1e-9), "{pick:?}");
    }
    let f = body("fanned", fanned_block());
    assert_eq!(f.graph.edges.len(), 12, "the fan's spokes are no edges");
    assert!(!f.graph.points.iter().any(|p| near(p.at, [10.0, 10.0, 10.0], 1e-9)), "the fan centre is no point");
    for proj in [Projection::Ortho, Projection::Perspective] {
        let q = query([10.0, 10.0, 50.0], [0.0, 0.0, -1.0], proj, PickKinds::POINTS);
        assert_eq!(pick_bodies(std::slice::from_ref(&f), &q), None, "the fan centre is not pickable");
        let q = query([10.0, 10.0, 50.0], [0.0, 0.0, -1.0], proj, PickKinds::ALL);
        let pick = pick_bodies(std::slice::from_ref(&f), &q).expect("the face");
        let PickTarget::Face { face, .. } = pick.target else { panic!("not a face: {pick:?}") };
        assert_eq!(f.graph.faces[face as usize].facets.len(), 4, "all four fan triangles are one face");
    }
}

/// Rays through the block's back-bottom edge at (10, 20, 0), looking
/// forward and down, so the near (front) face hides it.
fn far_edge_queries() -> [PickQuery; 2] {
    let n = (1.0_f64 + 0.09).sqrt();
    let d = [0.0, 1.0 / n, -0.3 / n];
    let t = [10.0, 20.0, 0.0];
    let back = |s: f64| [t[0] - d[0] * s, t[1] - d[1] * s, t[2] - d[2] * s];
    [
        // Perspective: the ray leaves the eye, diverging sight lines.
        query(back(100.0), d, Projection::Perspective, PickKinds::EDGES),
        // Ortho: a parallel ray from the view plane, farther back.
        query(back(400.0), d, Projection::Ortho, PickKinds::EDGES),
    ]
}

#[test]
fn a_far_side_edge_is_not_picked_through_the_near_face() {
    let b = body("block", block());
    let one = std::slice::from_ref(&b);
    for q in far_edge_queries() {
        assert_eq!(pick_bodies(one, &q), None, "hidden edge picked ({:?})", q.ray.projection);
        let mut all = q;
        all.kinds = PickKinds::ALL;
        let pick = pick_bodies(one, &all).expect("the front face");
        assert!(matches!(pick.target, PickTarget::Face { .. }), "{pick:?}");
        assert!(pick.at[1].abs() < 1e-9, "the near face, y = 0: {pick:?}");
    }
}

#[test]
fn pick_through_does_pick_the_far_side_edge() {
    let b = body("block", block());
    for mut q in far_edge_queries() {
        q.through = true;
        let pick = pick_bodies(std::slice::from_ref(&b), &q).expect("the far edge, through");
        let (p, r) = edge_ends(&b, &pick);
        assert!(p[1] == 20.0 && r[1] == 20.0 && p[2] == 0.0 && r[2] == 0.0, "back-bottom edge, got {p:?} {r:?}");
        assert!(near(pick.at, [10.0, 20.0, 0.0], 1e-6), "{pick:?}");
    }
}

/// #2636: the picking rule of the "Wireframe, see-through" display mode,
/// which asks for `through`. Each edge can be picked, front or back. The
/// edge nearest to the cursor (the ray) wins. Edges at the same distance
/// from the cursor: the edge nearest to the camera wins.
#[test]
fn see_through_takes_the_edge_nearest_the_cursor_then_nearest_the_eye() {
    let b = body("block", block());
    let one = std::slice::from_ref(&b);
    let ys = |pick: &FeaturePick| {
        let (p, r) = edge_ends(&b, pick);
        assert!(p[2] == 10.0 && r[2] == 10.0, "a top edge along X, got {p:?} {r:?}");
        (p[1], r[1])
    };
    // A level ray along +Y, 1 mm below the top: the front and the back top
    // edges are at the same distance from it. The front edge is nearer.
    let mut level = query([10.0, -100.0, 9.0], [0.0, 1.0, 0.0], Projection::Ortho, PickKinds::EDGES);
    level.radius = 2.0;
    level.through = true;
    let pick = pick_bodies(one, &level).expect("an edge");
    assert_eq!(ys(&pick), (0.0, 0.0), "the same distance: the edge nearest to the eye");
    // The same ray climbing: 1 mm from the front top edge, about 0.2 mm
    // from the back one. The back edge wins, behind the front face.
    let mut climb = query([10.0, -100.0, 5.0], [0.0, 1.0, 0.04], Projection::Ortho, PickKinds::EDGES);
    climb.radius = 2.0;
    climb.through = true;
    let pick = pick_bodies(one, &climb).expect("an edge");
    assert_eq!(ys(&pick), (20.0, 20.0), "nearer to the cursor wins over nearer to the eye");
    // With `through` off the back edge is hidden, and the front edge is taken.
    climb.through = false;
    let pick = pick_bodies(one, &climb).expect("the visible edge");
    assert_eq!(ys(&pick), (0.0, 0.0), "only the edge in sight");
}

#[test]
fn the_world_pick_honours_placements() {
    let world = new_world();
    ImportModel::from_mesh("block", cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0))).apply(&world);
    let e = model::find_by_name(&world, "block").expect("entity");
    model::set_transform(&world, &e, Location { pos: Point3::new(100.0, 0.0, 0.0), rot: IDENTITY_Q });
    let q = aim([160.0, -40.0, 40.0], [120.0, 0.0, 10.0], Projection::Perspective, PickKinds::ALL);
    let pick = pick_feature(&world, &q, |_| false).expect("the placed corner");
    assert_eq!(pick.entity, "block");
    assert!(matches!(pick.target, PickTarget::Point(_)), "{pick:?}");
    assert!(near(pick.at, [120.0, 0.0, 10.0], 1e-9), "{pick:?}");
    // The unplaced spot is empty now.
    let q = aim([60.0, -40.0, 40.0], [20.0, 0.0, 10.0], Projection::Perspective, PickKinds::ALL);
    assert_eq!(pick_feature(&world, &q, |_| false), None);
    assert_eq!(pick_feature(&world, &q, |n| n == "block"), None, "a skipped entity is not there");
}

#[test]
fn degenerate_input_picks_nothing_and_never_panics() {
    let down = query([0.5, 0.5, 10.0], [0.0, 0.0, -1.0], Projection::Ortho, PickKinds::ALL);
    assert_eq!(pick_bodies(&[], &down), None, "no bodies");
    let empty = body("empty", Vec::new());
    assert!(empty.graph.edges.is_empty() && empty.graph.points.is_empty() && empty.graph.faces.is_empty());
    assert_eq!(pick_bodies(std::slice::from_ref(&empty), &down), None, "an empty mesh");

    let nan = f64::NAN;
    let junk = vec![
        Triangle::new(Point3::new(nan, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)),
        Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 0.0), Point3::new(2.0, 2.0, 0.0)),
        Triangle::new(Point3::ZERO, Point3::ZERO, Point3::ZERO),
        Triangle::new(Point3::new(f64::INFINITY, 0.0, 0.0), Point3::ZERO, Point3::new(0.0, 0.0, 1.0)),
    ];
    let g = feature_graph(&junk);
    assert!(g.points.iter().all(|p| p.at.iter().all(|v| v.is_finite())));
    let j = body("junk", junk);
    for proj in [Projection::Ortho, Projection::Perspective] {
        for through in [false, true] {
            let mut q = down;
            q.ray.projection = proj;
            q.through = through;
            assert_eq!(pick_bodies(std::slice::from_ref(&j), &q), None, "junk mesh ({proj:?}, through {through})");
        }
    }

    // Garbage rays and radii against a real block.
    let b = body("block", block());
    let one = std::slice::from_ref(&b);
    for (o, d) in [
        ([nan, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([10.0, 10.0, 50.0], [0.0, 0.0, 0.0]),
        ([10.0, 10.0, 50.0], [0.0, nan, -1.0]),
        ([10.0, 10.0, 50.0], [0.0, 0.0, f64::INFINITY]),
    ] {
        assert_eq!(pick_bodies(one, &query(o, d, Projection::Perspective, PickKinds::ALL)), None);
    }
    for r in [nan, -5.0, f64::INFINITY] {
        let mut q = aim([60.0, -40.0, 40.0], [20.2, 0.1, 9.9], Projection::Perspective, PickKinds::POINTS);
        q.radius = r;
        assert_eq!(pick_bodies(one, &q), None, "radius {r} snaps to nothing");
    }

    // The arc fit refuses straight and too-short runs.
    assert_eq!(circumcircle([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]), None);
    assert_eq!(circumcircle([0.0; 3], [0.0; 3], [nan, 0.0, 0.0]), None);
    let line: Vec<[f64; 3]> = (0..5).map(|i| [f64::from(i), 0.0, 0.0]).collect();
    assert_eq!(fit_arc(&line, false, vec![0, 1, 2, 3]), None);
    assert_eq!(fit_arc(&[], true, Vec::new()), None);
}

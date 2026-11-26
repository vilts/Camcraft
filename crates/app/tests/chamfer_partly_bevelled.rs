//! #2887: a 2D Chamfer with no picks on a plate bevelled on one side
//! only. No whole outline of the outer edge can be ridden - the wall
//! would cut the three sharp sides, the top edge would stand inside
//! the part at the bevel - so generation refuses by name and asks for
//! picks, with and without an extra vertex on the bevel's top edge.
//!
//! Gate map (#2887): covers app::operation (material_source, the
//! Chamfer2D region), motion::machining2d (chamfer_ride).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// The 90-degree chamfer mill (D6) the #303 gate rides.
const CONE: &str = "CM-0600-90";

/// Two triangles (a, b, c) and (a, c, d) for one planar quad.
fn quad(facets: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    facets.push(Triangle::new(a, b, c));
    facets.push(Triangle::new(a, c, d));
}

/// A 60 x 40 x 10 plate, x 0..60, y 0..40, z 0..10, with a modelled
/// 1 x 45 chamfer on the X0 side only: the wall x = 0 stands up to z 9
/// and the bevel face joins (0, y, 9) to (1, y, 10). A SHARP 20 x 10
/// pocket (x 20..40, y 15..25) runs down to z 5. `extra_vertex` splits
/// the bevel's top edge at (1, 20, 10). Outward-facing CCW triangles.
fn one_side_bevelled_plate(extra_vertex: bool) -> Mesh {
    let p = Point3::new;
    let mut facets = Vec::new();
    quad(&mut facets, p(0.0, 40.0, 0.0), p(60.0, 40.0, 0.0), p(60.0, 0.0, 0.0), p(0.0, 0.0, 0.0));

    // Side y = 0, a pentagon fanned from its first vertex.
    let v = [p(0.0, 0.0, 0.0), p(60.0, 0.0, 0.0), p(60.0, 0.0, 10.0), p(1.0, 0.0, 10.0), p(0.0, 0.0, 9.0)];
    facets.push(Triangle::new(v[0], v[1], v[2]));
    facets.push(Triangle::new(v[0], v[2], v[3]));
    facets.push(Triangle::new(v[0], v[3], v[4]));

    // Side x = 60.
    quad(&mut facets, p(60.0, 0.0, 0.0), p(60.0, 40.0, 0.0), p(60.0, 40.0, 10.0), p(60.0, 0.0, 10.0));

    // Side y = 40, a pentagon fanned from its first vertex.
    let v = [p(60.0, 40.0, 0.0), p(0.0, 40.0, 0.0), p(0.0, 40.0, 9.0), p(1.0, 40.0, 10.0), p(60.0, 40.0, 10.0)];
    facets.push(Triangle::new(v[0], v[1], v[2]));
    facets.push(Triangle::new(v[0], v[2], v[3]));
    facets.push(Triangle::new(v[0], v[3], v[4]));

    // Wall x = 0, up to the bevel's foot at z 9.
    quad(&mut facets, p(0.0, 40.0, 0.0), p(0.0, 0.0, 0.0), p(0.0, 0.0, 9.0), p(0.0, 40.0, 9.0));

    // The bevel face, with or without the extra vertex at (1, 20, 10).
    if extra_vertex {
        facets.push(Triangle::new(p(0.0, 40.0, 9.0), p(0.0, 0.0, 9.0), p(1.0, 0.0, 10.0)));
        facets.push(Triangle::new(p(0.0, 40.0, 9.0), p(1.0, 0.0, 10.0), p(1.0, 20.0, 10.0)));
        facets.push(Triangle::new(p(0.0, 40.0, 9.0), p(1.0, 20.0, 10.0), p(1.0, 40.0, 10.0)));
    } else {
        quad(&mut facets, p(0.0, 40.0, 9.0), p(0.0, 0.0, 9.0), p(1.0, 0.0, 10.0), p(1.0, 40.0, 10.0));
    }

    // Top face: a ring between the outer top ring T and the pocket rim R.
    let t = [p(1.0, 0.0, 10.0), p(60.0, 0.0, 10.0), p(60.0, 40.0, 10.0), p(1.0, 40.0, 10.0)];
    let r = [p(20.0, 15.0, 10.0), p(40.0, 15.0, 10.0), p(40.0, 25.0, 10.0), p(20.0, 25.0, 10.0)];
    for i in 0..3 {
        let j = i + 1;
        quad(&mut facets, t[i], t[j], r[j], r[i]);
    }
    if extra_vertex {
        let e = p(1.0, 20.0, 10.0);
        facets.push(Triangle::new(t[3], e, r[3]));
        facets.push(Triangle::new(e, r[0], r[3]));
        facets.push(Triangle::new(e, t[0], r[0]));
    } else {
        quad(&mut facets, t[3], t[0], r[0], r[3]);
    }

    // Pocket walls and floor at z 5.
    let f = [p(20.0, 15.0, 5.0), p(40.0, 15.0, 5.0), p(40.0, 25.0, 5.0), p(20.0, 25.0, 5.0)];
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(&mut facets, r[i], r[j], f[j], f[i]);
    }
    quad(&mut facets, f[0], f[1], f[2], f[3]);

    Mesh::from_facets(facets)
}

fn chamfer(width: f64) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_radius: None,
        blend_turns: false,
        picks: Default::default(),
    };
    let mut p = operation::retype_params(&base, OpType::Chamfer2D.ordinal());
    if let OpParams::Chamfer2D { width: w, .. } = &mut p {
        *w = width;
    }
    p
}

/// Sets up the plate and one no-pick 2D Chamfer of width 1, then
/// returns the generation error text. Generation must refuse.
fn refusal(mesh: Mesh) -> String {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", mesh));
    let stock = SetStock::new(&world, StockShape::FromModel { source: "plate".into() }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    let op = CreateOperation::new(&world, "OP", CONE, SrcRef::Entity("plate".into()), chamfer(1.0));
    h.execute(&world, op.expect("create"));
    GenerateOperation::new(&world, "OP")
        .map(|_| ())
        .expect_err("a partly bevelled edge loop with no picks is refused")
        .to_string()
}

// #2887: a no-pick chamfer on a plate bevelled on one side is refused
// by name, and the message asks for picks (no extra vertex on the bevel).
#[test]
fn a_no_pick_chamfer_on_a_plate_bevelled_on_one_side_refuses_by_name() {
    let msg = refusal(one_side_bevelled_plate(false));
    assert!(msg.contains("only part of its length"), "{msg}");
    assert!(msg.contains("pick the edges to break"), "{msg}");
}

// #2887: the same refusal holds when the bevel's top edge carries an
// extra collinear vertex at (1, 20, 10).
#[test]
fn the_refusal_does_not_depend_on_an_extra_vertex_on_the_bevel() {
    let msg = refusal(one_side_bevelled_plate(true));
    assert!(msg.contains("only part of its length"), "{msg}");
    assert!(msg.contains("pick the edges to break"), "{msg}");
}

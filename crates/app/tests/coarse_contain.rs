//! #904 (its point 6): the containment kinds' solid law on a model
//! imported coarser than the importer promises. `generate::contain_into`
//! sweeps a drill's own tip and stack against the part solid and forgives
//! 0.01 mm of it; #864 made that allowance give the model's excess back,
//! and this is its fixture-level pin: a drill run past a blind
//! floor by 0.008 mm is inside the allowance at full fidelity and a gouge
//! on a model flagged 0.0075 mm (0.0075 mm left).
//!
//! Gate map (#904): covers app::operation::generate (contain_into),
//! app::operation::material (the excess a job is judged with).
//! Lane shape (M4.4): runs in a second.
//!
//! Provenance: our own fixture over our own law; no vendor material.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// A 40 x 40 x [0, 20] block with a 20 x 20 square hole from the top,
/// blind at Z 10 (the blind-floor suite's part): one outward-wound shell.
fn block() -> Mesh {
    const H: f64 = 10.0;
    let flip = |t: &Triangle| Triangle::new(t.a, t.c, t.b);
    let mut f: Vec<Triangle> = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned()); // block walls
    // The top face round the hole's mouth.
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, 20.0);
    for k in 0..4 {
        let n = (k + 1) % 4;
        f.push(Triangle::new(p(o[k]), p(o[n]), p(i[n])));
        f.push(Triangle::new(p(o[k]), p(i[n]), p(i[k])));
    }
    let inner = cube(Point3::new(-H, -H, 10.0), Point3::new(H, H, 20.0));
    f.extend(inner.facets[4..12].iter().map(flip)); // hole walls, facing the axis
    f.extend(outer.facets[0..2].iter().cloned()); // full bottom
    f.extend(inner.facets[0..2].iter().map(flip)); // the blind floor, +z
    Mesh::from_facets(f)
}

/// The blocking warnings of a 4 mm drill run to `bottom` in the hole,
/// the block imported at `coarse_mm`.
fn drilled(coarse_mm: Option<f64>, bottom: f64) -> Vec<Warning> {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block()).with_fidelity(coarse_mm));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc).expect("stock"));
    let mut drill = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog drill");
    drill.flute_len = 30.0; // long series: the hole runs 10 mm from the Z 20 top
    h.execute(&world, AddTool::new(&world, "DR-0400", drill).expect("tool"));
    let params = OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: Some(Height::fixed(bottom)),
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }],
        cross: Vec::new(),
    };
    h.execute(&world, CreateOperation::new(&world, "D1", "DR-0400", SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "D1").expect("generate"));
    let e = model::find_by_name(&world, "D1").expect("op");
    model::toolpath_of(&world, &e).expect("cached").warnings.into_iter().filter(Warning::blocks_post).collect()
}

#[test]
fn a_drill_past_a_blind_floor_by_less_than_the_excess_is_a_gouge_on_a_reduced_model() {
    // A model flagged 0.0075 mm: 0.0025 mm past the promise, 0.0075 mm
    // of the solid law's allowance left.
    const COARSE: f64 = 0.0075;
    // 0.008 mm past the floor: inside the 0.01 mm the solid law forgives.
    assert_eq!(drilled(None, 9.992), Vec::new(), "full fidelity keeps today's verdict");
    // The same hole on the reduced model: past what is left.
    let w = drilled(Some(COARSE), 9.992);
    assert!(w.iter().any(|w| matches!(w, Warning::Gouge { .. })), "the tightened law convicts it: {w:?}");
    // 0.006 mm past the floor is inside both; to the floor is clean.
    assert_eq!(drilled(Some(COARSE), 9.994), Vec::new());
    assert_eq!(drilled(Some(COARSE), 10.0), Vec::new());
    // Half a millimetre past it: a gouge either way.
    assert!(!drilled(None, 9.5).is_empty() && !drilled(Some(COARSE), 9.5).is_empty());
    // Past the bound the law reads untightened (the gate refuses).
    assert_eq!(drilled(Some(0.03), 9.992), Vec::new());
}

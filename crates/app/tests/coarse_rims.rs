//! #1057: the containment kinds' rims on a model imported coarser than
//! the importer promises. A drill, bore or thread mill matched to its
//! hole stands exactly `r` off the hole's declared circle; the rim loop
//! the region carries is our own polygon of that circle, a sagitta
//! inside it. Law (b) used to read that polygon at the tolerance less
//! the model's excess, so once the excess passed about half the
//! tolerance (a model flagged above ~0.0105 mm) a matched drill was
//! convicted against its own rim. These pin that a matched cycle stays
//! clean right up to the refusal bound, that the real gouges on the same
//! coarse model still convict, and that past the bound the operation is
//! refused by name.
//!
//! Gate map (#1057): covers sim::verify::section (law (b) on rims),
//! app::operation::generate (the rims a containment verify carries),
//! app::operation::material_source (rim loops), app::model_fidelity.
//! Lane shape (M4.4): runs in seconds.
//!
//! Provenance: our own fixture over our own law; no vendor material.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// Just under the refusal bound: 0.0099 mm of excess, 0.0001 mm of the
/// gouge tolerance left.
const NEAR_BOUND: f64 = 0.0149;
/// Past the bound: refused at the program gate.
const TOO_COARSE: f64 = 0.03;

/// A 40 x 40 x [0, 20] block with a 20 x 20 square hole from the top,
/// blind at Z 10 (`coarse_contain`'s part).
fn block() -> Mesh {
    const H: f64 = 10.0;
    let flip = |t: &Triangle| Triangle::new(t.a, t.c, t.b);
    let mut f: Vec<Triangle> = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, 20.0);
    for k in 0..4 {
        let n = (k + 1) % 4;
        f.push(Triangle::new(p(o[k]), p(o[n]), p(i[n])));
        f.push(Triangle::new(p(o[k]), p(i[n]), p(i[k])));
    }
    let inner = cube(Point3::new(-H, -H, 10.0), Point3::new(H, H, 20.0));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn hole(x: f64, radius: f64) -> Hole {
    Hole { xy: Point2::new(x, 0.0), radius, entry_z: 20.0, floor_z: 10.2, through: false, stepped: false }
}

fn drill(x: f64, radius: f64, bottom: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: Some(Height::fixed(bottom)),
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![hole(x, radius)],
        cross: Vec::new(),
    }
}

fn bore(x: f64, radius: f64) -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![hole(x, radius)],
    }
}

fn thread(radius: f64) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        infeed: 0.1,
        feed: 400.0,
        plunge_feed: 150.0,
        climb: true,
        holes: vec![hole(0.0, radius)],
        pitch: None,
        hand: camcraft_app::cl::ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
    }
}

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

/// A multi-form thread mill (`mill2d_c`'s, #894's kind), long enough for
/// the hole.
fn thread_mill(diameter: f64) -> Tool {
    Tool { material_group: "steel".into(), ..camcraft_app::thread_mill::thread_mill(diameter, 1.0, 0.0, 25.0) }
}

fn long_drill() -> Tool {
    let mut d = catalog_tool("DR-0400");
    d.flute_len = 30.0; // long series: the hole runs 10 mm from the Z 20 top
    d
}

/// The world with `params` run by `tool` over the block imported at
/// `coarse_mm`.
fn run(coarse_mm: Option<f64>, tool: Tool, params: OpParams) -> flecs_ecs::prelude::World {
    run_after(coarse_mm, None, tool, params)
}

/// `run`, with `ahead` (a tool and its params) generated first as "AHEAD".
fn run_after(coarse_mm: Option<f64>, ahead: Option<(Tool, OpParams)>, tool: Tool, params: OpParams) -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block()).with_fidelity(coarse_mm));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc).expect("stock"));
    if let Some((t, p)) = ahead {
        h.execute(&world, AddTool::new(&world, "TA", t).expect("tool ahead"));
        h.execute(&world, CreateOperation::new(&world, "AHEAD", "TA", SrcRef::Entity("part".into()), p).expect("create ahead"));
        h.execute(&world, GenerateOperation::new(&world, "AHEAD").expect("generate ahead"));
    }
    h.execute(&world, AddTool::new(&world, "T", tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", "T", SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

fn blocking(world: &flecs_ecs::prelude::World) -> Vec<Warning> {
    let e = model::find_by_name(world, "OP").expect("op");
    model::toolpath_of(world, &e).expect("cached").warnings.into_iter().filter(Warning::blocks_post).collect()
}

fn verdict(coarse_mm: Option<f64>, tool: Tool, params: OpParams) -> Vec<Warning> {
    blocking(&run(coarse_mm, tool, params))
}

fn gouged(w: &[Warning]) -> bool {
    w.iter().any(|w| matches!(w, Warning::Gouge { .. }))
}

#[test]
fn a_matched_drill_is_clean_on_a_model_flagged_anywhere_below_the_refusal_bound() {
    for coarse in [None, Some(0.008), Some(0.0105), Some(0.011), Some(0.012), Some(0.0149)] {
        assert_eq!(verdict(coarse, long_drill(), drill(0.0, 2.0, 10.2)), Vec::new(), "at {coarse:?}");
    }
}

#[test]
fn a_matched_bore_is_clean_near_the_refusal_bound() {
    // An 8 mm end mill in a 6 mm-radius hole: the ring rides at 2, the
    // cutter's edge on the declared circle.
    for coarse in [None, Some(0.011), Some(NEAR_BOUND)] {
        assert_eq!(verdict(coarse, catalog_tool("EM-0800-4FL"), bore(0.0, 6.0)), Vec::new(), "bore at {coarse:?}");
    }
    // The hole declared 1 mm into the cavity's wall at X 10: the ring
    // cuts the part, and still convicts on the same coarse model.
    for coarse in [None, Some(NEAR_BOUND)] {
        let w = verdict(coarse, catalog_tool("EM-0800-4FL"), bore(5.0, 6.0));
        assert!(gouged(&w), "an offset bore convicts at {coarse:?}: {w:?}");
    }
}

#[test]
fn a_thread_mill_on_a_coarse_model_is_never_read_looser_than_at_full_fidelity() {
    // A 6 mm thread mill in an M10 (5 mm radius) hole. Its region reads
    // the thread's major circle (#1071), not the declared minor rim, so
    // Thread declares no rim (#1073 review) and keeps the tightened
    // reading: clean at full fidelity, and on a coarse model it may only
    // convict more, never less - the conservative side.
    // #992: the mill is not centre-cutting, so the matched bore opens
    // the hole flat to its floor ahead of the thread.
    let opened = |coarse| blocking(&run_after(coarse, Some((catalog_tool("EM-0800-4FL"), bore(0.0, 6.0))), thread_mill(6.0), thread(5.0)));
    let full = opened(None);
    assert!(!gouged(&full), "clean at full fidelity: {full:?}");
    for coarse in [Some(0.011), Some(NEAR_BOUND)] {
        let w = opened(coarse);
        assert!(w == full || gouged(&w), "never looser at {coarse:?}: {w:?}");
    }
}

#[test]
fn the_real_drill_gouges_still_convict_on_a_model_near_the_refusal_bound() {
    for coarse in [None, Some(0.011), Some(NEAR_BOUND)] {
        // Oversize: a 4 mm drill in a hole declared 3.98 mm.
        let w = verdict(coarse, long_drill(), drill(0.0, 1.99, 10.2));
        assert!(gouged(&w), "an oversize drill convicts at {coarse:?}: {w:?}");
        // Offset: the hole declared 1 mm into the cavity's wall at X 10.
        let w = verdict(coarse, long_drill(), drill(9.0, 2.0, 10.2));
        assert!(gouged(&w), "an offset drill convicts at {coarse:?}: {w:?}");
        // Past the floor by half a millimetre.
        let w = verdict(coarse, long_drill(), drill(0.0, 2.0, 9.5));
        assert!(gouged(&w), "a drill past the floor convicts at {coarse:?}: {w:?}");
    }
    // 0.005 mm past the floor: inside the solid law's allowance at full
    // fidelity, past the 0.0001 mm the near-bound model leaves.
    assert_eq!(verdict(None, long_drill(), drill(0.0, 2.0, 9.995)), Vec::new());
    assert!(gouged(&verdict(Some(NEAR_BOUND), long_drill(), drill(0.0, 2.0, 9.995))));
}

#[test]
fn a_matched_drill_past_the_bound_is_refused_by_name_not_convicted() {
    let world = run(Some(TOO_COARSE), long_drill(), drill(0.0, 2.0, 10.2));
    assert_eq!(blocking(&world), Vec::new(), "past the bound the laws read untightened");
    let lines: Vec<String> = posting::verification_gate(&world).into_iter().filter(|l| l.contains("was imported")).collect();
    assert_eq!(
        lines,
        vec!["OP: model 'part' was imported too coarsely to verify; re-import with a larger STEP facet budget (surfaces are approximated to within 0.030 mm)".to_string()]
    );
}

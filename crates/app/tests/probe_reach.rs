//! #172 acceptance: the Probe op's protected-move law judges the whole
//! stylus, not only its ball. The stem above the ball and the probe body
//! above the stem are swept like a cutter's shank bands; a touch whose
//! ball clears but whose stem or body meets the part is refused as a
//! shank hit on the part, while the clean touches stay clean.
//!
//! Provenance: touch-probe practice (the stylus and body must clear the
//! part; only the ball touches); our own box fixtures.
//!
//! Gate map (M4.4): covers app::operation (probe_op), motion::probing,
//! sim::verify (solid law, as the probe's reach law reads it).

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geometry::{cube, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::probe_op::{stylus_cutter, stylus_hits, stylus_reach_hits};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{probe_tool, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

fn edge(dir: ProbeDir, target: Point3, depth: f64) -> ProbeParams {
    ProbeParams {
        mode: ProbeMode::Edge,
        dir,
        dir_y: ProbeDir::YPos,
        target,
        radius: 0.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 1.0, // #570: the new-op default
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth,
        inset: 5.0,
        clearance_z: 80.0,
    }
}

fn boxes(b: &[([f64; 3], [f64; 3])]) -> Vec<Triangle> {
    b.iter()
        .flat_map(|(lo, hi)| cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.to_vec())
        .collect()
}

/// The seek move of the first touch (the move the first probe record
/// marks): where the stylus first runs toward the part.
fn seek(t: &Toolpath) -> usize {
    t.probes.first().map(|p| p.index).expect("a touch")
}

/// The whole law, as `generate_probe` runs it.
fn judge(t: &Toolpath, tool: &Tool, facets: &[Triangle]) -> (Vec<Warning>, Vec<Warning>) {
    let ball = stylus_hits(t, tool.diameter * 0.5, facets);
    let reach = stylus_reach_hits(t, tool, None, facets, &ball);
    (ball, reach)
}

/// The same probe with no body above the stylus: what is left to meet
/// the part is the ball and the stem.
fn bodyless(t: &Tool) -> Tool {
    Tool { shank_len: 0.0, holder_len: 0.0, ..t.clone() }
}

#[test]
fn a_stem_that_meets_an_overhang_is_a_shank_hit() {
    // A 10 mm step touched on its -X face 3 mm under its top, with a lip
    // standing out 1.5 mm past that face from Z 12 up. The ball (top at
    // Z 9) clears the lip; the Ø4 stem above it runs straight into it.
    let facets = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 10.0]), ([-1.5, 0.0, 12.0], [40.0, 40.0, 30.0])]);
    let tool = probe_tool(4.0, 30.0);
    let t = probe_cycle(&edge(ProbeDir::XPos, Point3::new(0.0, 20.0, 10.0), 3.0)).expect("edge");
    let (ball, reach) = judge(&t, &tool, &facets);
    assert!(ball.is_empty(), "the ball clears the lip: {ball:?}");
    assert!(
        reach.contains(&Warning::ShankHit { at: seek(&t), part: true }),
        "the stem meets the lip on the seek - refused by name: {reach:?}"
    );
    assert!(reach.iter().all(|w| w.blocks_post()), "a shank hit holds the post");
    // The stem alone convicts: the same touch with no probe body.
    let (_, stem_only) = judge(&t, &bodyless(&tool), &facets);
    assert!(stem_only.contains(&Warning::ShankHit { at: seek(&t), part: true }), "{stem_only:?}");
}

#[test]
fn a_body_that_meets_a_tall_wall_is_a_shank_hit() {
    // A 40 mm wall touched on its -X face 32 mm under its top: the ball
    // and the 30 mm stem ride tangent to the face, but the Ø20 probe
    // body starts 30 mm above the tip (Z 36), below the wall's top, and
    // half of it stands inside the wall.
    let facets = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 40.0])]);
    let tool = probe_tool(4.0, 30.0);
    let t = probe_cycle(&edge(ProbeDir::XPos, Point3::new(0.0, 20.0, 40.0), 32.0)).expect("edge");
    let (ball, reach) = judge(&t, &tool, &facets);
    assert!(ball.is_empty(), "the ball touches tangent: {ball:?}");
    assert!(
        reach.contains(&Warning::ShankHit { at: seek(&t), part: true }),
        "the probe body meets the wall - refused by name: {reach:?}"
    );
    // The body is what convicts: with no body, the tangent stem is clean.
    let (_, stem_only) = judge(&t, &bodyless(&tool), &facets);
    assert!(stem_only.is_empty(), "a stem tangent to the face is clean: {stem_only:?}");
}

#[test]
fn clean_touches_stay_clean_under_the_reach_law() {
    let tool = probe_tool(4.0, 30.0);
    let block = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 20.0])]);
    // An edge touch on each side face, 3 mm under the top.
    for (dir, target) in [
        (ProbeDir::XPos, Point3::new(0.0, 20.0, 20.0)),
        (ProbeDir::XNeg, Point3::new(40.0, 20.0, 20.0)),
        (ProbeDir::YPos, Point3::new(20.0, 0.0, 20.0)),
        (ProbeDir::YNeg, Point3::new(20.0, 40.0, 20.0)),
    ] {
        let t = probe_cycle(&edge(dir, target, 3.0)).expect("edge");
        let (ball, reach) = judge(&t, &tool, &block);
        assert!(ball.is_empty() && reach.is_empty(), "{dir:?}: {ball:?} {reach:?}");
    }
    // A -Z face touch on the top and an outside corner.
    let mut p = edge(ProbeDir::ZNeg, Point3::new(20.0, 20.0, 20.0), 0.0);
    p.mode = ProbeMode::Face;
    let t = probe_cycle(&p).expect("face");
    let (ball, reach) = judge(&t, &tool, &block);
    assert!(ball.is_empty() && reach.is_empty(), "face: {ball:?} {reach:?}");
    let mut p = edge(ProbeDir::XPos, Point3::new(0.0, 0.0, 20.0), 3.0);
    p.mode = ProbeMode::Corner;
    let t = probe_cycle(&p).expect("corner");
    let (ball, reach) = judge(&t, &tool, &block);
    assert!(ball.is_empty() && reach.is_empty(), "corner: {ball:?} {reach:?}");
    // No model, nothing to meet.
    assert!(judge(&t, &tool, &[]).1.is_empty());
}

#[test]
fn the_stylus_stack_is_ball_then_stem_then_body() {
    let c = stylus_cutter(&probe_tool(4.0, 30.0), None);
    assert_eq!((c.radius, c.corner_r, c.flute_len), (2.0, 2.0, 2.0), "the ball is the whole cutting atom");
    assert_eq!((c.shank_radius, c.shank_len), (2.0, 28.0), "the stem runs to the stylus length");
    assert_eq!((c.holder_radius, c.holder_len), (10.0, 40.0), "the probe body rides above it");
}

fn edge_world(stylus_len: f64, depth: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    h.execute(&world, AddTool::new(&world, "PROBE-4", probe_tool(4.0, stylus_len)).expect("tool"));
    let b = model::stock_world_aabb(&world).expect("stock box");
    let params = OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        mode: ProbeMode::Edge,
        dir: ProbeDir::XPos,
        dir_y: ProbeDir::YPos,
        target: [b.min.x, (b.min.y + b.max.y) * 0.5, b.max.z],
        radius: 0.0,
        approach: 5.0,
        overtravel: 1.0, // #570: the new-op default
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    };
    h.execute(&world, CreateOperation::new(&world, "Probe1", "PROBE-4", SrcRef::Stock, params).expect("create"));
    (world, h)
}

fn generate(world: &World, h: &mut History) -> Vec<Warning> {
    h.execute(world, GenerateOperation::new(world, "Probe1").expect("generate"));
    let e = model::find_by_name(world, "Probe1").expect("op");
    model::toolpath_of(world, &e).expect("cached").warnings
}

#[test]
fn a_short_stylus_deep_on_the_stock_edge_is_refused_by_the_op() {
    // A 10 mm stylus touching 12 mm under the stock top: the body
    // (from 10 mm above the tip, 4 mm under the top) meets the stock.
    let (world, mut h) = edge_world(10.0, 12.0);
    let warns = generate(&world, &mut h);
    assert!(
        warns.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })),
        "the body in the stock is refused: {warns:?}"
    );
    assert!(!warns.iter().any(|w| matches!(w, Warning::Gouge { .. })), "the ball is clean: {warns:?}");
    // The same stylus 3 mm under the top: the body stands above it.
    let (world, mut h) = edge_world(10.0, 3.0);
    let warns = generate(&world, &mut h);
    assert!(warns.is_empty(), "a shallow edge touch is clean: {warns:?}");
}

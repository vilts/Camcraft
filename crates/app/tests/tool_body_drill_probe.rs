//! #842: drill reach and the touch probe's stylus reach judge a modelled
//! tool body by its band stack - the shape the simulation draws - not the
//! cylinder enclosing it. Pinned end to end through `operation::generate`
//! (a drawn body on a drill and on a probe) and on the stylus law itself:
//! a stepped body whose nose clears and whose flange never reaches the
//! material is no longer refused, a flange that does reach it still is,
//! the enclosure wording is gone where the bands were read, and a tool
//! with no body reads exactly as before.
//!
//! Gate map (M4.4): covers app::operation (drill_reach, probe_op,
//! body_judged, generate), sim::verify (solid law, body_bands).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own box fixtures; touch-probe practice (only the ball
//! touches). No vendor material.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, ImportModel, SetToolBody};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::operation::probe_op::{stylus_cutter, stylus_hits, stylus_reach_hits};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use camcraft_app::schema::Operation;
use camcraft_app::spun::SpunPoint;
use camcraft_app::tools::{catalog, probe_tool, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use camcraft_tools::holder_profile::{HolderBand, HolderStack};

fn boxes(b: &[([f64; 3], [f64; 3])]) -> Vec<Triangle> {
    b.iter()
        .flat_map(|(lo, hi)| cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.to_vec())
        .collect()
}

fn pt(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

/// A drawn stepped body from `z0` above the tip: an r `nose_r` nose to
/// `step`, then an r `flange_r` flange to `top`.
fn stepped(z0: f64, nose_r: f64, step: f64, flange_r: f64, top: f64) -> Vec<SpunPoint> {
    vec![pt(z0, 0.0), pt(z0, nose_r), pt(step, nose_r), pt(step, flange_r), pt(top, flange_r), pt(top, 0.0)]
}

fn op(tool: &str, src: &str, params: OpParams) -> Operation {
    Operation {
        tool: tool.into(),
        src: SrcRef::Entity(src.into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// `tool` filed as `name` beside `part` (no stock), with `body` drawn on
/// it when given.
fn setup(name: &str, tool: Tool, part: Vec<Triangle>, body: Option<Vec<SpunPoint>>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(part)));
    if let Some(points) = body {
        h.execute(&world, SetToolBody::spun(&world, name, Some(points)).expect("a sane drawn body"));
    }
    (world, h)
}

// ---- drill reach: the axial holder-face law -------------------------

/// DR-0400 on 2 mm of shank: its Ø44 holder's face 14 above the tip.
fn drill() -> Tool {
    let base = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog DR-0400");
    Tool { shank_len: 2.0, ..base }
}

/// A 40 block (top z20) with a tower x 8..20 standing to z40 beside the
/// hole at the origin; the hole 10 deep, inside the 12 mm flute.
fn drill_part() -> Vec<Triangle> {
    boxes(&[([-20.0, -20.0, 0.0], [20.0, 20.0, 20.0]), ([8.0, -20.0, 20.0], [20.0, 20.0, 40.0])])
}

fn drill_op() -> Operation {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    op(
        "DR-0400",
        "part",
        OpParams::Drill {
            clearance_z: Height::fixed(50.0),
            retract_z: None,
            top_z: Height::fixed(20.0),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes: vec![hole],
            cross: vec![],
        },
    )
}

fn drill_with(body: Option<Vec<SpunPoint>>) -> Result<(), String> {
    let (world, _h) = setup("DR-0400", drill(), drill_part(), body);
    operation::generate(&world, &drill_op(), "").map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn a_drawn_stepped_body_on_a_drill_is_judged_by_its_bands_not_the_enclosing_cylinder() {
    // No body: the Ø44 holder's face (z24) under the tower's top (z40).
    let e = drill_with(None).err().unwrap_or_else(|| panic!("the holder sank beside the tower"));
    assert!(e.contains("DR-0400's holder (dia. 44.000, 14.000 mm above the tip) sinks to Z 24.000"), "{e}");
    // An r6 nose 14..44 (face z24 - the tower stands at x8, the block's
    // top at z20) and an r22 flange from 44 (face z54, above the tower):
    // it drills. The enclosing cylinder (r22 from 14) refused it.
    let r = drill_with(Some(stepped(14.0, 6.0, 44.0, 22.0, 49.0)));
    assert!(r.is_ok(), "the nose clears, the flange stands above the tower: {:?}", r.err());
    // The flange from 28 (face z38, under the tower's top): refused,
    // naming the band, without the enclosure words.
    let e = drill_with(Some(stepped(14.0, 6.0, 28.0, 22.0, 49.0))).err();
    let e = e.unwrap_or_else(|| panic!("the low flange sank beside the tower"));
    assert!(e.contains("DR-0400's body band (dia. 44.000,") && e.contains("sinks to Z"), "{e}");
    assert!(!e.contains("enclosing"), "the bands were read, not the enclosing cylinder: {e}");
}

// ---- the touch probe's stylus reach ---------------------------------

/// A 40 block (top z10) touched on its -X face 3 under the top, and a
/// tower x -40..-25 behind the approach standing to `tower_top`.
fn probe_part(tower_top: f64) -> Vec<Triangle> {
    boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 10.0]), ([-40.0, 0.0, 0.0], [-25.0, 40.0, tower_top])])
}

fn edge() -> ProbeParams {
    ProbeParams {
        mode: ProbeMode::Edge,
        dir: ProbeDir::XPos,
        dir_y: ProbeDir::YPos,
        target: Point3::new(0.0, 20.0, 10.0),
        radius: 0.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 80.0,
    }
}

/// PRB with a Ø4 ball on a 30 stylus and its Ø20 x 40 probe body, the
/// Ø60 x 45 holder enclosing a stepped body over it: an r10 nose 70..100
/// above the tip and an r30 flange 100..115.
fn probe() -> (Tool, HolderStack) {
    let t = Tool { holder_d: 60.0, holder_len: 45.0, ..probe_tool(4.0, 30.0) };
    let stack = HolderStack {
        bands: vec![HolderBand { above: 70.0, len: 30.0, r: 10.0 }, HolderBand { above: 100.0, len: 15.0, r: 30.0 }],
        top: 115.0,
        neck: Vec::new(),
    };
    (t, stack)
}

fn reach(tool: &Tool, stack: Option<&HolderStack>, facets: &[Triangle]) -> Vec<Warning> {
    let t = probe_cycle(&edge()).expect("edge");
    let ball = stylus_hits(&t, tool.diameter * 0.5, facets);
    assert!(ball.is_empty(), "the ball touches tangent: {ball:?}");
    stylus_reach_hits(&t, tool, stack, facets, &ball)
}

#[test]
fn a_stepped_body_over_the_stylus_is_judged_by_its_bands() {
    let (tool, stack) = probe();
    let c = stylus_cutter(&tool, Some(&stack));
    let bands: Vec<(f64, f64, f64)> = c.holder_bands();
    assert_eq!(bands, vec![(30.0, 10.0, 40.0), (70.0, 10.0, 30.0), (100.0, 30.0, 15.0)], "probe body, then the body's bands");
    // The tower (top z60) is met by the one enclosing band (r30 from 30
    // above the tip) on the approach; the r10 bands miss it and the
    // flange stands above it.
    let enclosed = reach(&tool, None, &probe_part(60.0));
    assert!(enclosed.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })), "{enclosed:?}");
    assert_eq!(reach(&tool, Some(&stack), &probe_part(60.0)), Vec::new(), "the flange stands above the tower");
}

#[test]
fn a_flange_over_the_stylus_that_meets_the_part_is_still_refused() {
    let (tool, stack) = probe();
    // A tower to z120: the flange (from 107 at the touch) meets it.
    let w = reach(&tool, Some(&stack), &probe_part(120.0));
    assert!(w.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })), "{w:?}");
    assert!(w.iter().all(Warning::blocks_post), "{w:?}");
}

#[test]
fn a_probe_with_no_body_reads_its_one_band_unchanged() {
    let (tool, _) = probe();
    let plain = stylus_cutter(&tool, None);
    assert_eq!((plain.holder_radius, plain.holder_len, plain.body), (30.0, 85.0, None));
    // The record's own holder cylinder is no body.
    assert_eq!(stylus_cutter(&tool, Some(&HolderStack::cylinder(&tool.geom()))), plain);
    // A stack that does not fit the stylus (wider than its enclosing
    // band) leaves the one band.
    let wide = HolderStack { bands: vec![HolderBand { above: 70.0, len: 45.0, r: 40.0 }], top: 115.0, neck: Vec::new() };
    assert_eq!(stylus_cutter(&tool, Some(&wide)), plain);
    for top in [60.0, 120.0] {
        assert_eq!(reach(&tool, None, &probe_part(top)), reach(&tool, Some(&HolderStack::cylinder(&tool.geom())), &probe_part(top)));
    }
}

fn probe_op() -> Operation {
    op(
        "PRB",
        "part",
        OpParams::Probe {
            clearance_z: Height::fixed(80.0),
            mode: ProbeMode::Edge,
            dir: ProbeDir::XPos,
            dir_y: ProbeDir::YPos,
            target: [0.0, 20.0, 10.0],
            radius: 0.0,
            approach: 5.0,
            overtravel: 1.0,
            backoff: 1.0,
            fast_feed: 300.0,
            slow_feed: 30.0,
            depth: 3.0,
            inset: 5.0,
            set_wcs: Some(0),
            tolerance: 0.5,
        },
    )
}

#[test]
fn a_drawn_body_on_a_probe_is_judged_by_its_bands_end_to_end() {
    let body = || Some(stepped(70.0, 10.0, 100.0, 30.0, 115.0));
    let (world, _h) = setup("PRB", probe_tool(4.0, 30.0), probe_part(60.0), body());
    let (path, warns) = operation::generate(&world, &probe_op(), "").expect("a probe path");
    assert!(warns.is_empty(), "the bands clear the tower: {warns:?}");
    assert!(path.notes.iter().all(|n| !n.contains("enclosing")), "no enclosure note: {:?}", path.notes);
    let (world, _h) = setup("PRB", probe_tool(4.0, 30.0), probe_part(120.0), body());
    let (_, warns) = operation::generate(&world, &probe_op(), "").expect("a probe path");
    assert!(warns.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })), "the flange meets the tall tower: {warns:?}");
}

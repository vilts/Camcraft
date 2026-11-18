//! Drill reach for the cycles that run in an existing hole (issue #84,
//! review fix 1): a tap, bore or back-bore is not held to its flute -
//! the hole is already there - but past the flute its shank must fit
//! the hole's diameter, refused by name when it does not.
//!
//! Gate map (M4.4): covers app::operation (drill_reach's axial arm,
//! the Z arm of generate's Drill).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::Operation;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const FLOOR: f64 = -5.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [-10, 20] block with a square hole `2 h` wide from the
/// top, blind to z-5 (25 mm deep). Outward-wound.
fn block_with_hole(h: f64) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-h, -h), (h, -h), (h, h), (-h, h)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-h, -h, FLOOR), Point3::new(h, h, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn tool(name: &str, grind: impl Fn(&mut Tool)) -> Tool {
    let mut t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
    grind(&mut t);
    t
}

fn world_with(hole_h: f64, name: &str, t: Tool) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("block", block_with_hole(hole_h)));
    (world, history)
}

fn params(radius: f64, cycle: CycleChoice) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

/// Generate through the command path; the cached path's post-blocking
/// warnings (empty means it posts).
fn posts(world: &World, history: &mut History, name: &str, tool: &str, p: OpParams) -> Result<(), String> {
    let op = CreateOperation::new(world, name, tool, SrcRef::Entity("block".into()), p);
    history.execute(world, op.map_err(|e| format!("{e:?}"))?);
    history.execute(world, GenerateOperation::new(world, name).map_err(|e| format!("{e:?}"))?);
    let e = model::find_by_name(world, name).ok_or("no op")?;
    let tp = model::toolpath_of(world, &e).ok_or("no cached path")?;
    let blocking: Vec<_> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    if tp.path.moves.is_empty() || !blocking.is_empty() {
        return Err(format!("empty path or blocked: {blocking:?}"));
    }
    Ok(())
}

fn raw_op(tool: &str, p: OpParams) -> Operation {
    Operation { tool: tool.into(), src: SrcRef::Entity("block".into()), params: p, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

#[test]
fn a_bore_past_its_flute_in_a_pre_drilled_hole_posts() {
    // BR-1000: 20 mm flute on a Ø10 shank, in a Ø12 hole 25 mm deep.
    let (world, mut h) = world_with(6.0, "BR-1000", tool("BR-1000", |_| {}));
    let r = posts(&world, &mut h, "B1", "BR-1000", params(6.0, CycleChoice::Bore));
    assert!(r.is_ok(), "the Ø10 shank fits the Ø12 hole: {r:?}");
}

#[test]
fn a_tap_past_its_flute_in_an_existing_hole_posts() {
    // TAP-M4: 18 mm flute on a Ø4.5 shank, in a Ø6 hole 25 mm deep.
    let (world, mut h) = world_with(3.0, "TAP-M4", tool("TAP-M4", |_| {}));
    let r = posts(&world, &mut h, "T1", "TAP-M4", params(3.0, CycleChoice::Tap));
    assert!(r.is_ok(), "the Ø4.5 shank fits the Ø6 hole: {r:?}");
}

#[test]
fn a_bore_bar_whose_shank_is_wider_than_the_hole_is_refused_by_name() {
    let (world, _h) = world_with(6.0, "BR-1000", tool("BR-1000", |t| t.shank_d = 14.0));
    let e = operation::generate(&world, &raw_op("BR-1000", params(6.0, CycleChoice::Bore)), "").err();
    // #677: the refusal marks the hole it speaks of, on its material top.
    let at = e.as_ref().and_then(|e| e.location());
    assert!(at.is_some_and(|p| p.x == 0.0 && p.y == 0.0 && p.z.is_finite()), "marks the hole at (0, 0): {at:?}");
    let e = e.map(|e| e.to_string()).unwrap_or_else(|| panic!("a Ø14 shank fed into a Ø12 hole"));
    for s in ["Bore cycle at (0.000, 0.000)", "25.000 mm deep", "BR-1000's 20.000 mm flute", "shank dia. 14.000", "dia. 12.000 hole"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
    // Within the flute the shank never enters: a 15 mm bore drills.
    let mut p = params(6.0, CycleChoice::Bore);
    if let OpParams::Drill { bottom_z, .. } = &mut p {
        *bottom_z = Some(Height::fixed(5.0));
    }
    let r = operation::generate(&world, &raw_op("BR-1000", p), "");
    assert!(r.is_ok(), "a bore inside the flute's reach: {:?}", r.err());
}

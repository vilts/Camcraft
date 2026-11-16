//! #1408 at the app level: a turning nose path's envelope verdict
//! reads a hidden floor (a groove's floor under the bar's full
//! diameter) as the turned work sees it - the scan turns the work
//! about the spindle, and a hidden top out of the nose's reach, in a
//! recess the path never stood level with, is another op's
//! jurisdiction (the Groove kinds'), never the Facing's or the
//! Profile's rest. A real gouge, a real hidden floor inside the path's
//! own band, and the floor of a groove the path went into and stopped
//! short of (PR #1429's review) still block the post by name.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the Facing and
//! Profile arms of generate's envelope verdict), sim::verify::finish
//! and sim::stock_sim::scan_hidden through the document.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;

const PLAIN: &[(f64, f64)] = &[(0.0, 0.0), (10.0, 0.0), (10.0, 48.0), (0.0, 48.0)];
const STEPPED: &[(f64, f64)] =
    &[(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (7.0, 20.0), (7.0, 40.0), (5.0, 40.0), (5.0, 48.0), (0.0, 48.0)];
const WIDER_AT_FREE_END: &[(f64, f64)] =
    &[(0.0, 0.0), (7.0, 0.0), (7.0, 20.0), (10.0, 20.0), (10.0, 48.0), (0.0, 48.0)];
const GROOVE: &[(f64, f64)] = &[
    (0.0, 0.0), (10.0, 0.0), (10.0, 34.0), (5.5, 34.0), (5.5, 37.0), (10.0, 37.0), (10.0, 48.0), (0.0, 48.0),
];
const GROOVE_NEAR_TOP: &[(f64, f64)] = &[
    (0.0, 0.0), (10.0, 0.0), (10.0, 44.0), (8.0, 44.0), (8.0, 46.0), (10.0, 46.0), (10.0, 48.0), (0.0, 48.0),
];
const SHAFT: &[(f64, f64)] = &[
    (0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (7.0, 20.0), (7.0, 34.0), (5.5, 34.0), (5.5, 37.0), (7.0, 37.0),
    (7.0, 40.0), (5.0, 40.0), (5.0, 48.0), (0.0, 48.0),
];

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A closed revolve about Z of a meridian, `segs` turns - the issue's
/// own 64-segment bars.
fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The issue's document: a lathe, a d20 x 50 bar stock standing on
/// z 0, the house insert, the bar.
fn bar_world(meridian: &[(f64, f64)]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let cmd = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Cylinder { diameter: 20.0, height: 50.0 },
        Location::default(),
    );
    history.execute(&world, cmd.expect("bar stock"));
    let tool = AddTool::new(&world, "TN-1600-0R8", catalog_tool("TN-1600-0R8")).expect("catalog tool");
    history.execute(&world, tool);
    history.execute(&world, ImportModel::from_mesh("bar", revolve(meridian, 64)));
    (world, history)
}

fn facing(bottom: f64) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(55.0),
        top_z: Height::fixed(50.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn profile_rough() -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(55.0),
        top_z: Height::fixed(48.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn profile_finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(55.0),
        top_z: Height::fixed(48.0),
        bottom_z: Height::fixed(0.0),
        feed: 120.0,
        plunge_feed: 60.0,
    }
}

const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

/// `meridian`'s bar with `ops` created and generated in order; each
/// op's warnings that block the post, by name.
fn generated(meridian: &[(f64, f64)], ops: &[(&str, OpParams)]) -> (World, Vec<(String, Vec<Warning>)>) {
    let (world, mut history) = bar_world(meridian);
    let mut out = Vec::new();
    for (name, params) in ops {
        let cmd = CreateOperation::new(&world, name, "TN-1600-0R8", SrcRef::Entity("bar".into()), params.clone());
        history.execute(&world, cmd.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        let e = model::find_by_name(&world, name).expect("op");
        let t = model::toolpath_of(&world, &e).expect("the op generated a path");
        out.push((name.to_string(), t.warnings.into_iter().filter(Warning::blocks_post).collect()));
    }
    (world, out)
}

/// The document through the lathe post's own gate - the refusal the
/// issue saw, or the program.
fn post(world: &World, tag: &str) -> Result<String, String> {
    let file = std::env::temp_dir().join(format!("camcraft_1408_{tag}_{}.nc", std::process::id()));
    let out = camcraft_app::posting::post_script(world, &file, LATHE_CPS).map(|p| p.program);
    let _ = std::fs::remove_file(&file);
    out
}

/// Every op clean and the program written.
fn assert_posts(meridian: &[(f64, f64)], ops: &[(&str, OpParams)], tag: &str) {
    let (world, verdicts) = generated(meridian, ops);
    for (name, w) in &verdicts {
        assert!(w.is_empty(), "{tag}: {name} carries a blocking finding: {:?}", w.iter().map(ToString::to_string).collect::<Vec<_>>());
    }
    let program = post(&world, tag).unwrap_or_else(|why| panic!("{tag}: {why}"));
    assert!(program.contains("G18"), "{tag}: a lathe program: {program}");
}

#[test]
fn a_facing_over_a_grooved_bar_posts() {
    // The issue's three refusals: the groove floor 11 mm (or 2 mm, on
    // the near-top groove) under the faced band is no nose's of this
    // Facing - its stations stop at z 48 - so the band clause excuses
    // it, and the program posts as it does on the plain bar.
    for (tag, bar) in [("groove", GROOVE), ("groove_top", GROOVE_NEAR_TOP), ("shaft_facing", SHAFT)] {
        assert_posts(bar, &[("F2", facing(48.0))], tag);
    }
}

#[test]
fn the_shafts_facing_and_profiles_post() {
    // The Profile half (austin62jake's reading): the groove's lower
    // annulus across the axis read as 12.5 mm of rest until the scan
    // turned the work; the inner corner a 0.8 nose cannot reach and
    // the rough's own 0.25 leave are not rest either. All three ops
    // clean, the program written.
    let ops = [("F2", facing(48.0)), ("R1", profile_rough()), ("S1", profile_finish())];
    assert_posts(SHAFT, &ops, "shaft");
}

#[test]
fn the_profiles_post_over_both_grooves() {
    // The groove bar's rough enters its 3 mm groove; the near-top
    // groove is 2 mm wide, under the rough's 0.8 nose plus its 0.25
    // leave each side - the rough cannot enter it, and the finish
    // (which can) owns it.
    for (tag, bar) in [("groove_prof", GROOVE), ("groove_top_prof", GROOVE_NEAR_TOP)] {
        let ops = [("F2", facing(48.0)), ("R1", profile_rough()), ("S1", profile_finish())];
        assert_posts(bar, &ops, tag);
    }
}

#[test]
fn bars_without_a_groove_post_as_before() {
    assert_posts(PLAIN, &[("F2", facing(48.0))], "plain");
    let ops = [("F2", facing(48.0)), ("R1", profile_rough()), ("S1", profile_finish())];
    assert_posts(STEPPED, &ops, "stepped");
    let (_, verdicts) = generated(WIDER_AT_FREE_END, &[("F2", facing(48.0))]);
    assert!(verdicts.iter().all(|(_, w)| w.is_empty()), "wider at the free end: {verdicts:?}");
}

fn profile_finish_to(bottom: f64) -> OpParams {
    match profile_finish() {
        OpParams::ProfileFinish { tolerance, clearance_z, top_z, feed, plunge_feed, .. } => OpParams::ProfileFinish {
            tolerance,
            clearance_z,
            top_z,
            bottom_z: Height::fixed(bottom),
            feed,
            plunge_feed,
        },
        other => other,
    }
}

#[test]
fn a_finish_that_stops_short_of_the_groove_floor_is_refused() {
    // PR #1429's review: a pass that goes into the groove and stops
    // short of its floor left that floor uncut, and the band clause
    // read it as another op's. The finish window here ends at z 35.5,
    // inside the 34..37 groove: the floor under it is the finish's
    // rest, and the post refuses by name. The Facing on the same bar,
    // whose stations never stand level with the groove, still posts.
    let (world, verdicts) = generated(GROOVE, &[("S1", profile_finish_to(35.5))]);
    let w = &verdicts[0].1;
    assert!(
        w.iter().any(|w| matches!(w, Warning::Envelope { rest_mm, .. } if *rest_mm > 0.3)),
        "the floor under the short finish is its rest: {:?}",
        w.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    let why = post(&world, "short").expect_err("a finish that stops short of the floor must not post");
    assert!(why.contains("post blocked") && why.contains("S1") && why.contains("finish envelope"), "{why}");
    assert_posts(GROOVE, &[("F2", facing(48.0))], "groove_facing_still");
}

#[test]
fn a_facing_that_gouges_the_grooved_bar_still_blocks_by_name() {
    // A facing plane a millimetre into the bar: the nose cuts the
    // part's own end, inside the band it claims. No excuse reaches the
    // gouge half - the envelope verdict convicts and the post refuses.
    let (world, verdicts) = generated(GROOVE, &[("F2", facing(47.0))]);
    let w = &verdicts[0].1;
    assert!(
        w.iter().any(|w| matches!(w, Warning::Envelope { gouge_mm, .. } if *gouge_mm > 0.5)),
        "the gouge stands: {:?}",
        w.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    let why = post(&world, "gouge").expect_err("a gouging facing must not post");
    assert!(why.contains("post blocked") && why.contains("F2") && why.contains("finish envelope"), "{why}");
}

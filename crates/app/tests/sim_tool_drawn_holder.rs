//! #2176: a drawn (spun) holder is drawn in the simulation as drawn -
//! its own radii, lengths and tapers - not as the stepped bands the
//! collision laws read; the checks, the verify step and the simulation
//! still read the one landed body, and the drawn outline lies inside
//! the bands the laws read. A profile edit redraws the body; undo
//! restores it. Our own tests.
//!
//! Gate map (M4.4): covers app::sim_tool_body, app::tool_body, app::commands::tool_body.

use std::sync::Arc;

use camcraft_app::commands::{AddTool, CreateOperation, History, SetStock, SetToolBody};
use camcraft_app::geometry::Triangle;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim_tool_body::{self as sim_body, JudgedOutline, OutlineError, SimToolCache};
use camcraft_app::spun::SpunPoint;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_tools::holder_profile::{HolderBand, HolderStack};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
const OP: &str = "Face1";

fn catalog_tool() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry")
}

fn shank_top() -> f64 {
    let t = catalog_tool();
    t.flute_len + t.shank_len
}

fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool()).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, OP, TOOL, SrcRef::Stock, face).expect("op"));
    (world, history)
}

/// Draw `points` (`(z above the shank top, r)`) as the tool's holder.
fn draw(world: &World, history: &mut History, points: &[(f64, f64)]) {
    let z0 = shank_top();
    let pts = points.iter().map(|&(z, r)| SpunPoint::line(z0 + z, r)).collect();
    history.execute(world, SetToolBody::spun(world, TOOL, Some(pts)).expect("a sane profile"));
}

/// The judged record and stack, read the way `fixture_struck`,
/// `operation::generate` and the verify step's `collect_job` read them.
fn judged_stack(world: &World) -> (Tool, HolderStack) {
    let te = model::find_by_name(world, TOOL).expect("tool");
    let (judged, body) = tool_body::judged(world, &te, catalog_tool()).expect("fits");
    let stack = tool_body::stack(&judged.geom(), body.as_ref());
    (judged, stack)
}

/// Every vertex of a tool-frame mesh as `(z, r)`.
fn meridian_of(tris: &[Triangle]) -> Vec<(f64, f64)> {
    tris.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| (p.z, p.x.hypot(p.y))).collect()
}

fn has(points: &[(f64, f64)], z: f64, r: f64) -> bool {
    points.iter().any(|q| (q.0 - z).abs() < 1e-9 && (q.1 - r).abs() < 1e-9)
}

/// The holder of the issue's picture: a Ø44 body, a taper out to Ø50,
/// a Ø50 block, and a Ø30 cap.
const PICTURE: [(f64, f64); 8] = [(0.0, 0.0), (0.0, 22.0), (30.0, 22.0), (40.0, 25.0), (55.0, 25.0), (55.0, 15.0), (70.0, 15.0), (70.0, 0.0)];

#[test]
fn a_drawn_holder_is_drawn_with_its_own_radii_and_lengths() {
    let (world, mut history) = face_world();
    draw(&world, &mut history, &PICTURE);
    let z0 = shank_top();
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("draws");
    let tris = outline.mesh();
    let got = meridian_of(&tris);
    // Every drawn vertex is in the mesh, at its drawn height and radius.
    for &(z, r) in &PICTURE {
        assert!(has(&got, z0 + z, r), "drawn vertex (z {z}, r {r}) is missing from the simulated body");
    }
    // The mesh is the cutter and the drawn outline, nothing else.
    let mut want = outline.cutter.clone();
    want.extend(outline.holder_shown());
    for &(z, r) in &got {
        assert!(has(&want, z, r), "mesh vertex (z {z}, r {r}) is not on the drawn outline");
    }
    // The taper is a taper: no Ø50 below its top, no stepped band ring.
    assert!(!got.iter().any(|q| q.1 > 22.0 + 1e-9 && q.0 < z0 + 40.0 - 1e-9), "the Ø44 body is drawn as Ø44 up to the taper");
    let t = catalog_tool();
    assert!(!got.iter().any(|q| q.0 < z0 - 1e-9 && q.1 > t.shank_d.max(t.diameter) * 0.5 + 1e-9), "the shank is not drawn widened to the body's foot");
    assert!(!has(&got, z0, 25.0) && !has(&got, z0 + 30.0, 25.0), "no band cylinder at the taper's larger radius");
    assert!((outline.top() - (z0 + 70.0)).abs() < 1e-9);
    assert!((outline.max_radius() - 25.0).abs() < 1e-9);
}

#[test]
fn a_drawn_cone_is_drawn_as_a_cone_and_judged_as_its_enclosing_band() {
    let (world, mut history) = face_world();
    let z0 = shank_top();
    // A cone from r = 10 to r = 30 over 20 mm: judged as one band at 30.
    draw(&world, &mut history, &[(0.0, 0.0), (0.0, 10.0), (20.0, 30.0), (20.0, 0.0)]);
    let (_, stack) = judged_stack(&world);
    assert_eq!(stack.bands, vec![HolderBand { above: z0, len: 20.0, r: 30.0 }], "the law's reading of the cone");
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("draws");
    assert_eq!(outline.holder, stack.bands, "the laws' bands are unchanged");
    let got = meridian_of(&outline.mesh());
    assert!(has(&got, z0, 10.0), "the foot is drawn at the drawn 10");
    assert!(!has(&got, z0, 30.0), "not at the judged 30");
    assert!(has(&got, z0 + 20.0, 30.0), "the top rim at 30");
    // Strict interior enclosure excludes a band that only touches the
    // shank top. The laws and the display both read the stored shank.
    let (judged, _) = judged_stack(&world);
    let t = catalog_tool();
    assert_eq!(judged.shank_d, t.shank_d, "boundary contact does not widen the judged shank");
    assert_eq!(JudgedOutline::new(&judged, &stack).expect("sane").cutter,
        JudgedOutline::new(&t, &stack).expect("sane").cutter,
        "the stored-cutter guard leaves the judged cutter unchanged");
    assert!(has(&got, z0, t.shank_d * 0.5), "the shank is drawn at its stored radius");
    assert!(!got.iter().any(|q| q.0 < z0 - 1e-9 && q.1 > t.shank_d.max(t.diameter) * 0.5 + 1e-9), "nothing below the holder is drawn wider than the stored tool");
}

#[test]
fn a_profile_edit_redraws_the_holder_and_undo_restores_it() {
    let (world, mut history) = face_world();
    let z0 = shank_top();
    let mut cache = SimToolCache::default();
    draw(&world, &mut history, &[(0.0, 0.0), (0.0, 10.0), (20.0, 30.0), (20.0, 0.0)]);
    let first = cache.tool_frame(&world, OP).expect("a mesh");
    assert!(has(&meridian_of(&first), z0, 10.0));
    let bands = sim_body::for_operation(&world, OP).expect("ok").expect("draws").holder;
    // The same enclosing band, a different drawn foot: the laws read the
    // same numbers, but the picture must follow the edit.
    draw(&world, &mut history, &[(0.0, 0.0), (0.0, 20.0), (20.0, 30.0), (20.0, 0.0)]);
    assert_eq!(sim_body::for_operation(&world, OP).expect("ok").expect("draws").holder, bands);
    let edited = cache.tool_frame(&world, OP).expect("a mesh");
    assert!(!Arc::ptr_eq(&first, &edited), "an edited profile rebuilds the mesh");
    let m = meridian_of(&edited);
    assert!(has(&m, z0, 20.0) && !has(&m, z0, 10.0), "the edited foot is drawn");
    history.undo(&world);
    let back = meridian_of(&cache.tool_frame(&world, OP).expect("a mesh"));
    assert!(has(&back, z0, 10.0) && !has(&back, z0, 20.0), "undo restores the drawn foot");
}

#[test]
fn checks_verify_and_simulation_read_one_holder_shape() {
    let (world, mut history) = face_world();
    draw(&world, &mut history, &PICTURE);
    let te = model::find_by_name(&world, TOOL).expect("tool");
    let body = tool_body::body_of(&world, &te).expect("a landed body");
    let spun = body.spun.clone().expect("a drawn body");
    let (judged, stack) = judged_stack(&world);
    // The laws (and the verify step, through the same two reads) sweep
    // exactly the landed profile's bands.
    assert_eq!(stack.bands, spun.holder_profile().bands(), "the checks read the landed body");
    let outline = sim_body::for_operation(&world, OP).expect("no refusal").expect("draws");
    // The simulation reads the same bands and the same landed outline.
    assert_eq!(outline.holder, stack.bands, "the simulation holds the checks' bands");
    assert_eq!(outline.drawn, spun.outline(), "the simulation draws the landed outline");
    let stored = JudgedOutline::new(&catalog_tool(), &stack).expect("sane").cutter;
    assert_eq!(outline.cutter, stored, "the shank is drawn as stored");
    assert_eq!(outline, JudgedOutline::new(&judged, &stack).and_then(|o| o.with_drawn(spun.outline(), stored.clone())).expect("sane"));
    // The picture is never wider than the check at any height.
    for &(z, r) in &outline.drawn {
        assert!(r <= 1e-9 || outline.holder.iter().any(|b| z >= b.above - 1e-6 && z <= b.above + b.len + 1e-6 && r <= b.r + 1e-6), "drawn (z {z}, r {r}) outside the judged bands");
    }
    // An outline wider than the bands is refused by name, never drawn.
    let mut wide = spun.outline();
    if let Some(p) = wide.iter_mut().find(|p| p.1 > 1.0) {
        p.1 += 5.0;
    }
    let plain = JudgedOutline::new(&judged, &stack).expect("sane");
    assert_eq!(plain.cutter, stored, "strict interior enclosure keeps the picture fixture's stored cutter");
    assert_eq!(plain.clone().with_drawn(wide, stored.clone()), Err(OutlineError::PastJudged));
    // So is a shank drawn wider than the judged one.
    let fat: Vec<(f64, f64)> = stored.iter().map(|&(z, r)| (z, r + 100.0)).collect();
    assert_eq!(plain.clone().with_drawn(spun.outline(), fat), Err(OutlineError::PastJudged));
    assert_eq!(plain.clone().with_drawn(vec![(f64::NAN, 1.0)], stored.clone()), Err(OutlineError::NonFinite));
    assert_eq!(plain.with_drawn(vec![(100.0, 0.0), (90.0, 0.0)], stored), Err(OutlineError::RunsBack));
}

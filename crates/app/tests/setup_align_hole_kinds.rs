//! #2276: the hole-list kinds (Drill, Bore, Thread) in a setup that
//! holds the part turned about TWO axes (90 degrees about X, then 90
//! degrees about Z). The part is a plate whose two holes run along the
//! modelled Y axis: only the turn stands them upright. Each kind is
//! read three ways:
//! - in the turned setup (the part as modelled, the setup aligned);
//! - on the part with that very turn baked into its facets, unaligned;
//! - on the part as modelled, unaligned.
//!
//! Detect holes reads the op's setup frame, so the first two find the
//! same holes and must cut and post the same program. The part as
//! modelled has no upright hole, so Detect holes finds none there: a
//! recognition that read the model frame fails the test.
//!
//! Provenance: an analytic plate with polygonal bores and a rigid turn
//! read back from the stored alignment (plain rigid-body math); ISO 965-1
//! metric thread sizes; no vendor material consulted.
//!
//! Gate map (#2276): covers app::setup_align, app::operation::material
//! (detect_holes), app::operation::job, app::posting, app::stock_sim.
//! Lane shape (M4.4): every test here runs in seconds.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, quat_conjugate, transform_point};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType, SrcRef, retype_params};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, setup_align, world::new_world};
use flecs_ecs::prelude::World;

/// About X, about Y, about Z (degrees).
const TURN: [f64; 3] = [90.0, 0.0, 90.0];
/// The plate in the setup frame: two 40 mm cells along X, 10 mm thick,
/// centred on the origin, so the turn about the part centre is a turn
/// about the origin and the target plate is exactly where it lands.
const CELL: f64 = 20.0;
const HALF_T: f64 = 5.0;
/// An M8 x 1.25 tap-drill bore beside a 10 mm one: two radii, so a
/// swapped or mirrored hole list cuts a different path.
const RADII: [f64; 2] = [3.4, 5.0];
const N: usize = 48;

fn quad(t: &mut Vec<Triangle>, q: [Point3; 4], want: [f64; 3]) {
    for [a, b, c] in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
        let (u, v) = (Point3::new(b.x - a.x, b.y - a.y, b.z - a.z), Point3::new(c.x - a.x, c.y - a.y, c.z - a.z));
        let n = [u.y * v.z - u.z * v.y, u.z * v.x - u.x * v.z, u.x * v.y - u.y * v.x];
        let dot = n[0] * want[0] + n[1] * want[1] + n[2] * want[2];
        t.push(if dot >= 0.0 { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
    }
}

/// The plate as the setup must see it: upright through holes. Each
/// bore's vertices are pushed radially onto its cell's square (N a
/// multiple of 8), so the cells share their edge and the mesh closes.
fn target() -> Vec<Triangle> {
    let mut t = Vec::new();
    let cells = RADII.len();
    for (k, &r) in RADII.iter().enumerate() {
        let cx = (2.0 * k as f64 + 1.0) * CELL - cells as f64 * CELL;
        let ang = |i: usize| std::f64::consts::TAU * (i % N) as f64 / N as f64;
        let bp = |i: usize, z: f64| Point3::new(cx + r * ang(i).cos(), r * ang(i).sin(), z);
        let sq = |i: usize, z: f64| {
            let (x, y) = (ang(i).cos(), ang(i).sin());
            let s = x.abs().max(y.abs());
            Point3::new(cx + x * CELL / s, y * CELL / s, z)
        };
        for i in 0..N {
            let j = i + 1;
            quad(&mut t, [bp(i, HALF_T), bp(j, HALF_T), sq(j, HALF_T), sq(i, HALF_T)], [0.0, 0.0, 1.0]);
            quad(&mut t, [bp(i, -HALF_T), bp(j, -HALF_T), sq(j, -HALF_T), sq(i, -HALF_T)], [0.0, 0.0, -1.0]);
            let mid = ang(i) + std::f64::consts::PI / N as f64;
            quad(&mut t, [bp(i, HALF_T), bp(j, HALF_T), bp(j, -HALF_T), bp(i, -HALF_T)], [-mid.cos(), -mid.sin(), 0.0]);
            let (a, b) = (sq(i, HALF_T), sq(j, HALF_T));
            let on = |x: f64| (a.x - x).abs() < 1e-9 && (b.x - x).abs() < 1e-9;
            let shared = (k > 0 && on(cx - CELL)) || (k + 1 < cells && on(cx + CELL));
            if !shared {
                quad(&mut t, [a, b, sq(j, -HALF_T), sq(i, -HALF_T)], [(a.x + b.x) * 0.5 - cx, (a.y + b.y) * 0.5, 0.0]);
            }
        }
    }
    t
}

fn moved(tris: Vec<Triangle>, pos: Point3, rot: [f64; 4]) -> Vec<Triangle> {
    let p = |q: Point3| transform_point(q, pos, rot);
    tris.into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
}

/// The part as modelled: the target turned back, holes along Y.
fn modelled() -> Vec<Triangle> {
    let r = setup_align::rotation_from_angles(TURN).expect("turn");
    moved(target(), Point3::new(0.0, 0.0, 0.0), quat_conjugate(r))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    Turned,
    Baked,
    Modelled,
}

/// The alignment that `TURN` stores, read back from a scratch document.
fn stored_turn() -> Location {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(modelled())));
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn"));
    setup_align::stored(&world, "Setup1").expect("the turn is stored")
}

fn document(case: Case, tool: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let facets = match case {
        Case::Baked => {
            let a = stored_turn();
            moved(modelled(), a.pos, a.rot)
        }
        Case::Turned | Case::Modelled => modelled(),
    };
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, tool, t).expect("add tool"));
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("Setup1"));
    if case == Case::Turned {
        h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn about X and Z"));
    }
    // The stock is the turned plate's own box (furniture, set after the
    // turn): the same block in each case.
    let at = Location { pos: Point3::new(0.0, 0.0, -HALF_T), ..Default::default() };
    let stock = StockShape::Box { dx: 4.0 * CELL, dy: 2.0 * CELL, dz: 2.0 * HALF_T };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(24.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(-5.0),
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Outside,
        lead: camcraft_app::machining2d::Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// One kind with heights read off the part top (so the heights read
/// the setup frame too), its tool and its own knobs.
fn params(kind: OpType) -> (OpParams, &'static str) {
    let mut p = retype_params(&contour_params(), kind as usize);
    let clear = Height::of(HeightReference::TopOfModel, 20.0);
    let top = Height::of(HeightReference::TopOfModel, 0.0);
    let tool = match &mut p {
        OpParams::Drill { clearance_z, top_z, bottom_z, retract_z, .. } => {
            (*clearance_z, *top_z, *bottom_z, *retract_z) = (clear, top, None, None);
            "DR-0600"
        }
        OpParams::Bore { clearance_z, top_z, bottom_z, retract_z, stepdown, stock_to_leave, .. } => {
            (*clearance_z, *top_z, *bottom_z, *retract_z) = (clear, top, None, None);
            *stepdown = 2.0;
            *stock_to_leave = 0.1;
            "EM-0400-2FL"
        }
        OpParams::Thread { clearance_z, top_z, bottom_z, retract_z, pitch, .. } => {
            // The thread stops 1.5 mm above the plate floor: the opening
            // bore leaves the floor of the stock box there.
            (*clearance_z, *top_z, *bottom_z, *retract_z) = (clear, top, Some(Height::fixed(1.5 - HALF_T)), None);
            *pitch = Some(1.25);
            "TM-0600-P125"
        }
        other => panic!("not a kind of this file: {other:?}"),
    };
    (p, tool)
}

/// Create the op, fill its hole list with Detect holes in its setup,
/// and generate. The turned and baked cases must also verify clean,
/// simulate without a gouge and post.
fn generate(case: Case, kind: OpType) -> Result<(Toolpath, String), String> {
    let (p, tool) = params(kind);
    let (world, mut h) = document(case, tool);
    let src = SrcRef::Entity("part".into());
    // A thread mill does not cut at its centre: a Bore opens the holes
    // first, found the same way (the shop's order).
    if kind == OpType::Thread {
        let (bore, bt) = params(OpType::Bore);
        let t = catalog().into_iter().find(|(n, _)| *n == bt).map(|(_, t)| t).expect("tool");
        h.execute(&world, AddTool::new(&world, bt, t).expect("add tool"));
        detected(&world, &mut h, "Open", bt, &src, bore)?;
    }
    detected(&world, &mut h, "Cut", tool, &src, p)?;
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let cached = model::toolpath_of(&world, &entity).ok_or("no path in the cache")?;
    assert!(!cached.path.moves.is_empty(), "{case:?}: the cut contains moves");
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "{case:?}: unsafe path: {:?}", cached.warnings);
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("the part is judged");
    assert_eq!(sim.dev.gouge_cells, 0, "{case:?}: gouge {} at {:?}", sim.dev.max_gouge, sim.dev.max_gouge_at);
    let nc = posting::post_setup_unwritten(&world).map_err(|e| e.to_string())?.program;
    Ok((cached.path, nc))
}

/// Create one op, fill its hole list with Detect holes in Setup1, and
/// generate it.
fn detected(world: &World, h: &mut History, name: &str, tool: &str, src: &SrcRef, p: OpParams) -> Result<(), String> {
    h.execute(world, CreateOperation::new(world, name, tool, src.clone(), p.clone()).expect("create"));
    let found = operation::detect_holes(world, name, &p, Some("Setup1"), src)?;
    h.execute(world, EditOperationParams::new(world, name, found.params).map_err(|e| e.to_string())?);
    let cmd = GenerateOperation::new(world, name).map_err(|e| e.to_string())?;
    h.execute(world, cmd);
    Ok(())
}

fn equivalent(kind: OpType) {
    let (turned, t_nc) = generate(Case::Turned, kind).unwrap_or_else(|e| panic!("{kind:?} generates turned: {e}"));
    let (baked, b_nc) = generate(Case::Baked, kind).unwrap_or_else(|e| panic!("{kind:?} generates baked: {e}"));
    assert_eq!(turned.moves.len(), baked.moves.len(), "{kind:?}: same move count");
    for (i, (a, b)) in turned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "{kind:?}: move {i} kind");
        let d = (a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs());
        assert!(d < 1e-6, "{kind:?}: move {i}: turned {:?} baked {:?}", a.to, b.to);
    }
    assert_eq!(t_nc, b_nc, "{kind:?}: the same setup geometry posts the same program");
    // The model frame has no upright hole: a recognition that read it
    // finds nothing to cut.
    let modelled = generate(Case::Modelled, kind);
    assert!(modelled.is_err(), "{kind:?}: the part as modelled has no upright hole, got {modelled:?}");
}

#[test]
fn turned_drill_finds_and_cuts_the_holes_of_the_baked_part() {
    equivalent(OpType::Drill);
}

#[test]
fn turned_bore_finds_and_cuts_the_holes_of_the_baked_part() {
    equivalent(OpType::Bore);
}

#[test]
fn turned_thread_finds_and_cuts_the_holes_of_the_baked_part() {
    equivalent(OpType::Thread);
}

/// A hole list detected in the turned setup is params data: when the
/// setup is set back to As modelled, the stored holes no longer stand
/// in the part. Generation must refuse that or hold the post - never
/// drill the old coordinates silently.
#[test]
fn a_hole_list_from_the_turned_setup_never_drills_the_part_as_modelled_silently() {
    let (p, tool) = params(OpType::Drill);
    let (world, mut h) = document(Case::Turned, tool);
    let src = SrcRef::Entity("part".into());
    h.execute(&world, CreateOperation::new(&world, "Cut", tool, src.clone(), p.clone()).expect("create"));
    let found = operation::detect_holes(&world, "Cut", &p, Some("Setup1"), &src).expect("holes in the turned setup");
    h.execute(&world, EditOperationParams::new(&world, "Cut", found.params).expect("edit"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", [0.0; 3]).expect("back to as modelled"));
    let Ok(cmd) = GenerateOperation::new(&world, "Cut") else { return };
    h.execute(&world, cmd);
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let Some(cached) = model::toolpath_of(&world, &entity) else { return };
    let held = cached.warnings.iter().any(|w| w.blocks_post()) || posting::post_setup_unwritten(&world).is_err();
    assert!(held, "a stale hole list drilled the part as modelled and posted: {:?}", cached.warnings);
}

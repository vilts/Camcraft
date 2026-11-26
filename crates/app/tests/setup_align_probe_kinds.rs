//! #2285: the Probe op reads its pick in the frame of its setup, so a
//! setup that holds the part turned probes the part AS IT SITS ON THE
//! TABLE. The same touch is made three ways:
//! - in the turned setup (the part as modelled, the setup aligned);
//! - on the part with that very turn baked into its facets, unaligned;
//! - on the part as modelled, unaligned.
//!
//! The first two must seed the same touch, cut and post the same
//! program; the third must seed another touch and cut another path, so
//! a seed that read the model frame fails. The second test pins the
//! safety law: a probe target written in the model frame in a turned
//! setup drives the stylus through the part, and the stylus law
//! convicts it.
//!
//! Provenance: an analytic box and a rigid turn read back from the
//! stored alignment (plain rigid-body math); no vendor material
//! consulted.
//!
//! Gate map (#2285): covers app::operation::probe_pick,
//! app::operation::probe_op, app::setup_align, app::operation::job,
//! app::posting. Lane shape: every test here runs in seconds.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetOperationGeometry, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::probing::{ProbeDir, ProbeMode};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::probe_tool;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// About X, about Y, about Z (degrees): two axes, so a seed that reads
/// only a Z turn, or the wrong order, touches another place.
const TURN: [f64; 3] = [90.0, 0.0, 90.0];
const PROBE: &str = "PROBE-4";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    /// The part as modelled in a setup that holds it turned.
    Turned,
    /// The turn baked into the part's facets; the setup is unaligned.
    Baked,
    /// The part as modelled; the setup is unaligned.
    Modelled,
}

/// The 40 x 30 x 10 box at the origin: eight corners, bottom ring then
/// top ring (CCW from above), as an outward-wound hexahedron.
fn box_mesh() -> Vec<Triangle> {
    let p = Point3::new;
    let c = [
        p(0.0, 0.0, 0.0),
        p(40.0, 0.0, 0.0),
        p(40.0, 30.0, 0.0),
        p(0.0, 30.0, 0.0),
        p(0.0, 0.0, 10.0),
        p(40.0, 0.0, 10.0),
        p(40.0, 30.0, 10.0),
        p(0.0, 30.0, 10.0),
    ];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut f = Vec::new();
    for q in quads {
        f.push(Triangle::new(c[q[0]], c[q[1]], c[q[2]]));
        f.push(Triangle::new(c[q[0]], c[q[2]], c[q[3]]));
    }
    f
}

/// The alignment that `TURN` stores, read back from a scratch document.
fn stored_turn() -> Location {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(box_mesh())));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn"));
    setup_align::stored(&world, "Setup1").expect("the turn is stored")
}

/// The part, Setup1, the probe and one stock box that holds the part in
/// each case. The stock is set after the turn: furniture, the same box.
fn document(case: Case) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let facets = match case {
        Case::Baked => {
            let a = stored_turn();
            let p = |q: Point3| transform_point(q, a.pos, a.rot);
            box_mesh().into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
        }
        Case::Turned | Case::Modelled => box_mesh(),
    };
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if case == Case::Turned {
        h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn about X and Z"));
    }
    h.execute(&world, AddTool::new(&world, PROBE, probe_tool(4.0, 30.0)).expect("add the probe"));
    // The turned box is X [15, 25], Y [-5, 35], Z [-10, 20]; the box as
    // modelled is X [0, 40], Y [0, 30], Z [0, 10]. The box holds both;
    // its top stands on the turned box's top, and its height reference
    // is the same in each case.
    let at = Location { pos: Point3::new(20.0, 15.0, -10.0), ..Default::default() };
    let stock = StockShape::Box { dx: 50.0, dy: 50.0, dz: 30.0 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

/// The typed probe: a face touch travelling -Z, the numbers the new-op
/// default carries (#171, #570). The pick replaces the mode, direction
/// and target; the numbers stay.
fn probe_params() -> OpParams {
    OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0; 3],
        radius: 10.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    }
}

/// The part's face that is flat at the top of the part's bounds in the
/// frame of the active setup (`setup_align::view_pose`): the turned
/// box's +Y wall, or the top of the box as modelled.
fn top_face(world: &World) -> GeoSel {
    let part = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &part).expect("features");
    let tris = model::facets_of(world, &part).expect("facets");
    let pose = setup_align::view_pose(world, &part).expect("pose");
    let up = |p: &Point3| transform_point(*p, pose.pos, pose.rot).z;
    let top = tris.iter().flat_map(|t| [up(&t.a), up(&t.b), up(&t.c)]).fold(f64::MIN, f64::max);
    let face = features
        .faces
        .iter()
        .find(|f| {
            !f.facets.is_empty()
                && f.facets.iter().all(|&i| {
                    let t = tris[i as usize];
                    (up(&t.a) - top).abs() < 1e-9 && (up(&t.b) - top).abs() < 1e-9 && (up(&t.c) - top).abs() < 1e-9
                })
        })
        .expect("the face flat at the top of the part");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }
}

/// The part's top face picked, the probe seeded from that pick, its
/// path, its warnings and its program.
fn picked_probe(case: Case) -> (OpParams, Toolpath, Vec<Warning>, String) {
    let (world, mut h) = document(case);
    let pick = top_face(&world);
    h.execute(&world, CreateOperation::new(&world, "Probe1", PROBE, SrcRef::Stock, probe_params()).expect("create the probe"));
    h.execute(&world, SetOperationGeometry::new(&world, "Probe1", vec![pick]).expect("pick the top face"));
    let op = model::find_by_name(&world, "Probe1").expect("the op");
    let params = model::operation_of(&world, &op).expect("the operation").params;
    h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate the probe"));
    let cached = model::toolpath_of(&world, &op).expect("the cached path");
    assert!(!cached.path.moves.is_empty(), "{case:?}: the touch cycle contains moves");
    let nc = posting::post_setup_unwritten(&world).expect("a path posts").program;
    (params, cached.path, cached.warnings.clone(), nc)
}

fn target_of(params: &OpParams) -> [f64; 3] {
    let OpParams::Probe { target, .. } = params else { panic!("a probe") };
    *target
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
}

/// #2285: the pick seeds the touch against the part as the setup holds
/// it. The turned and baked documents seed the same touch, cut the same
/// path (to float noise) and post the same program; the part as
/// modelled seeds another touch and cuts another path.
#[test]
fn turned_probe_reads_its_pick_in_the_setup_frame() {
    let (tp, turned, t_w, t_nc) = picked_probe(Case::Turned);
    let (bp, baked, b_w, b_nc) = picked_probe(Case::Baked);
    let (mp, modelled, m_w, _) = picked_probe(Case::Modelled);

    // The seed reads the part as the setup holds it. The model's +Y wall
    // stands flat at the top of the turned box, so the touch stands on
    // that wall at z 20: not on the model's top at z 10.
    let OpParams::Probe { mode, dir, .. } = tp else { panic!("a probe") };
    assert_eq!((mode, dir), (ProbeMode::Face, ProbeDir::ZNeg), "the flat top of the turned box is touched travelling down");
    assert!(near(target_of(&tp), [20.0, 15.0, 20.0], 1e-6), "the turned seed: {:?}", target_of(&tp));
    assert!(near(target_of(&bp), [20.0, 15.0, 20.0], 1e-6), "the baked seed: {:?}", target_of(&bp));
    assert!(near(target_of(&mp), [20.0, 15.0, 10.0], 1e-6), "the seed as modelled: {:?}", target_of(&mp));

    // A touch seeded against the part on the table meets nothing on the
    // way down: no warning stands on any of the three paths.
    for (case, w) in [(Case::Turned, &t_w), (Case::Baked, &b_w), (Case::Modelled, &m_w)] {
        assert!(w.is_empty(), "{case:?}: a pick seeded in the setup frame is clean: {w:?}");
    }

    // The turned part and the baked twin are the same geometry: the same
    // path (to float noise) and the same program.
    assert_eq!(turned.moves.len(), baked.moves.len(), "same move count");
    for (i, (a, b)) in turned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "move {i} kind");
        let d = (a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs());
        assert!(d < 1e-6, "move {i}: turned {:?} baked {:?}", a.to, b.to);
    }
    assert_eq!(t_nc, b_nc, "the same setup geometry posts the same program");

    // The model frame is another place: a seed that read it touches the
    // model's top and drives the stylus 10 mm off the part on the table.
    let far = modelled.moves.len() != turned.moves.len()
        || modelled.moves.iter().zip(&turned.moves).any(|(a, b)| (a.to.x - b.to.x).abs() + (a.to.y - b.to.y).abs() + (a.to.z - b.to.z).abs() > 1e-3);
    assert!(far, "the turn must move the touch");
}

/// The safety law behind the proof: a target written in the model frame
/// in a turned setup feeds the stylus down through the part on the
/// table. The protected-move law convicts that path, and the conviction
/// blocks the post: no silent wrong program.
#[test]
fn a_model_frame_target_in_a_turned_setup_is_convicted() {
    let (world, mut h) = document(Case::Turned);
    let mut stale = probe_params();
    if let OpParams::Probe { target, .. } = &mut stale {
        *target = [20.0, 15.0, 10.0];
    }
    h.execute(&world, CreateOperation::new(&world, "Probe1", PROBE, SrcRef::Stock, stale).expect("create the stale probe"));
    h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate the stale probe"));
    let op = model::find_by_name(&world, "Probe1").expect("the op");
    let cached = model::toolpath_of(&world, &op).expect("the cached path");
    assert!(
        cached.warnings.iter().any(|w| matches!(*w, Warning::Gouge { .. }) && w.blocks_post()),
        "the stylus drives through the turned part: {:?}",
        cached.warnings
    );
}

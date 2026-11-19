//! #2276: the 2D kinds that #2210 left refused (2D Offset, Trace,
//! Engrave) in a setup that holds the part turned about TWO axes
//! (90 degrees about X, then 90 degrees about Z). Each kind is
//! generated three times:
//! - in the turned setup (the part as modelled, the setup aligned);
//! - on the part with that very turn baked into its facets, unaligned;
//! - on the part as modelled, unaligned.
//!
//! The first two must cut and post the same program; the third must cut
//! a different path, so a generator that read the model frame fails.
//!
//! Provenance: an analytic L prism and a rigid turn read back from the
//! stored alignment (plain rigid-body math); no vendor material
//! consulted.
//!
//! Gate map (#2276): covers app::setup_align, app::operation::job,
//! app::operation::material, app::operation::heights, app::posting,
//! app::stock_sim. Lane shape (M4.4): every test here runs in seconds.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef, retype_params};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, setup_align, world::new_world};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
/// About X, about Y, about Z (degrees): two axes, so a generator that
/// reads only a Z turn, or the wrong order, cuts a different place.
const TURN: [f64; 3] = [90.0, 0.0, 90.0];

/// The L outline as modelled, counter-clockwise: arms of 30 and 20 mm.
const L: [(f64, f64); 6] = [(0.0, 0.0), (30.0, 0.0), (30.0, 8.0), (10.0, 8.0), (10.0, 20.0), (0.0, 20.0)];

/// An L prism, z in [0, 10], as modelled.
fn l_prism() -> Vec<Triangle> {
    let b = |i: usize| Point3::new(L[i].0, L[i].1, 0.0);
    let t = |i: usize| Point3::new(L[i].0, L[i].1, 10.0);
    let n = L.len();
    let mut f = Vec::new();
    for i in 1..n - 1 {
        f.push(Triangle::new(t(0), t(i), t(i + 1)));
        f.push(Triangle::new(b(0), b(i + 1), b(i)));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        f.push(Triangle::new(b(i), b(j), t(j)));
        f.push(Triangle::new(b(i), t(j), t(i)));
    }
    f
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    /// The part as modelled in a setup that holds it turned.
    Turned,
    /// The turn baked into the part's facets; the setup is unaligned.
    Baked,
    /// The part as modelled; the setup is unaligned.
    Modelled,
}

/// The alignment that `TURN` stores, read back from a scratch document.
fn stored_turn() -> Location {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(l_prism())));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn"));
    setup_align::stored(&world, "Setup1").expect("the turn is stored")
}

/// The part, Setup1, the tool and one stock box that holds the part in
/// each case. The stock is set after the turn: furniture, the same box.
fn document(case: Case) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let facets = match case {
        Case::Baked => {
            let a = stored_turn();
            let p = |q: Point3| transform_point(q, a.pos, a.rot);
            l_prism().into_iter().map(|t| Triangle::new(p(t.a), p(t.b), p(t.c))).collect()
        }
        Case::Turned | Case::Modelled => l_prism(),
    };
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if case == Case::Turned {
        h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn about X and Z"));
    }
    // The turned part is X [10, 20], Y [-5, 25], Z [-5, 15]; the part as
    // modelled is X [0, 30], Y [0, 20], Z [0, 10]. The box holds both;
    // its top is the turned part's top, so a mark on that top is in air.
    let at = Location { pos: Point3::new(15.0, 10.0, -6.0), ..Default::default() };
    let stock = StockShape::Box { dx: 44.0, dy: 54.0, dz: 21.0 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(24.0),
        top_z: Height::fixed(15.0),
        bottom_z: Height::fixed(4.0),
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// One kind with the heights set. The bottom reads the part's top
/// (`TopOfModel`), so the heights read the setup frame too.
fn params(kind: OpType) -> OpParams {
    let mut p = retype_params(&contour_params(), kind as usize);
    let top = Height::fixed(15.0);
    let bottom = Height::of(HeightReference::TopOfModel, -11.0);
    match &mut p {
        OpParams::Offset2D { top_z, bottom_z, stepdown, stepover, .. } => {
            *top_z = top;
            *bottom_z = bottom;
            *stepdown = 4.0;
            *stepover = 1.5;
        }
        OpParams::Trace { top_z, bottom_z, stepdown, .. } | OpParams::Engrave { top_z, bottom_z, stepdown, .. } => {
            *top_z = Height::of(HeightReference::TopOfModel, 0.0);
            *bottom_z = Height::of(HeightReference::TopOfModel, -1.0);
            *stepdown = 1.0;
        }
        // The typed kinds stand beside the turned part (X [10, 20]) in
        // the setup frame: their XY is authored, their depth reads the
        // turned part's top.
        OpParams::Circular { center, radius, top_z, bottom_z, stepdown, stepover, .. } => {
            (*center, *radius) = (Point2::new(29.0, 10.0), 4.0);
            (*top_z, *bottom_z) = (top, bottom);
            (*stepdown, *stepover) = (4.0, 1.5);
        }
        OpParams::TrochoidalSlot { a, b, width, top_z, bottom_z, stepdown, .. } => {
            (*a, *b, *width) = (Point2::new(29.0, -8.0), Point2::new(29.0, 28.0), 8.0);
            (*top_z, *bottom_z, *stepdown) = (top, bottom, 4.0);
        }
        OpParams::Slot { a, b, width, top_z, bottom_z, stepdown, stepover, .. } => {
            (*a, *b, *width) = (Point2::new(29.0, -8.0), Point2::new(29.0, 28.0), 6.0);
            (*top_z, *bottom_z) = (top, bottom);
            (*stepdown, *stepover) = (4.0, 1.5);
        }
        other => panic!("not a kind of this file: {other:?}"),
    }
    p
}

/// Generate one op from the source that `src` reads off the document;
/// the turned and baked cases must also verify clean, simulate without
/// a gouge and post.
fn generate_on(case: Case, params: &OpParams, src: impl Fn(&World) -> SrcRef) -> Result<(Toolpath, String), String> {
    let (world, mut h) = document(case);
    let src = src(&world);
    h.execute(&world, CreateOperation::new(&world, "Cut", TOOL, src, params.clone()).expect("create"));
    let cmd = GenerateOperation::new(&world, "Cut").map_err(|e| e.to_string())?;
    h.execute(&world, cmd);
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let cached = model::toolpath_of(&world, &entity).ok_or("no path in the cache")?;
    if case == Case::Modelled {
        return Ok((cached.path, String::new()));
    }
    assert!(!cached.path.moves.is_empty(), "{case:?}: the cut contains moves");
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "{case:?}: unsafe path: {:?}", cached.warnings);
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("the part is judged");
    assert_eq!(sim.dev.gouge_cells, 0, "{case:?}: gouge {} at {:?}", sim.dev.max_gouge, sim.dev.max_gouge_at);
    let nc = posting::post_setup_unwritten(&world).expect("a clean path posts").program;
    Ok((cached.path, nc))
}

/// The turned and baked documents cut the same path (to float noise)
/// and post the same program; the part as modelled cuts another path.
fn equivalent(kind: OpType) {
    equivalent_on(kind, &params(kind), |_| SrcRef::Entity("part".into()));
}

fn equivalent_on(kind: OpType, p: &OpParams, src: impl Fn(&World) -> SrcRef + Copy) {
    let p = p.clone();
    let (turned, t_nc) = generate_on(Case::Turned, &p, src).unwrap_or_else(|e| panic!("{kind:?} generates turned: {e}"));
    let (baked, b_nc) = generate_on(Case::Baked, &p, src).unwrap_or_else(|e| panic!("{kind:?} generates baked: {e}"));
    assert_eq!(turned.moves.len(), baked.moves.len(), "{kind:?}: same move count");
    for (i, (a, b)) in turned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "{kind:?}: move {i} kind");
        let d = (a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs());
        assert!(d < 1e-6, "{kind:?}: move {i}: turned {:?} baked {:?}", a.to, b.to);
    }
    assert_eq!(t_nc, b_nc, "{kind:?}: the same setup geometry posts the same program");
    // The model frame is another place: a generator that read it fails.
    let (modelled, _) = generate_on(Case::Modelled, &p, src).unwrap_or_else(|e| panic!("{kind:?} generates as modelled: {e}"));
    let far = modelled.moves.len() != turned.moves.len()
        || modelled.moves.iter().zip(&turned.moves).any(|(a, b)| (a.to.x - b.to.x).abs() + (a.to.y - b.to.y).abs() + (a.to.z - b.to.z).abs() > 1e-3);
    assert!(far, "{kind:?}: the turn must change the path");
}

#[test]
fn turned_offset2d_cuts_like_the_baked_part() {
    equivalent(OpType::Offset2D);
}

#[test]
fn turned_trace_cuts_like_the_baked_part() {
    equivalent(OpType::Trace);
}

#[test]
fn turned_engrave_cuts_like_the_baked_part() {
    equivalent(OpType::Engrave);
}

#[test]
fn turned_circular_cuts_like_the_baked_part() {
    equivalent(OpType::Circular);
}

#[test]
fn turned_trochoidal_slot_cuts_like_the_baked_part() {
    equivalent(OpType::TrochoidalSlot);
}

#[test]
fn turned_typed_slot_cuts_like_the_baked_part() {
    equivalent(OpType::Slot);
}

/// The face that faces up at the part's top in the document's active
/// setup frame (`setup_align::view_pose`): the turned prism's highest
/// side face, or the L top as modelled.
fn up_face(world: &World) -> SrcRef {
    let part = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &part).expect("features");
    let pose = setup_align::view_pose(world, &part).expect("pose");
    let b = setup_align::part_aabb_in(world, camcraft_app::setup::active(world).as_deref()).expect("bounds");
    let z_of = |v: &u32| features.verts.get(*v as usize).map(|p| transform_point(Point3::new(p[0], p[1], p[2]), pose.pos, pose.rot).z);
    // The step face of the turned L: flat in the setup frame, 20 x 10 mm,
    // between the floor and the top (the 10 x 10 top end seeds no slot).
    let face = features
        .faces
        .iter()
        .find(|f| {
            let zs: Vec<f64> = f.loops.iter().flat_map(|l| l.verts.iter()).filter_map(z_of).collect();
            let z0 = zs.first().copied().unwrap_or(b.min.z);
            !zs.is_empty() && zs.iter().all(|z| (z - z0).abs() < 1e-6) && z0 > b.min.z + 1e-3 && z0 < b.max.z - 1e-3
        })
        .expect("a flat face between the floor and the top");
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }])
}

/// #2276: a Slot picked on a face reads the room in the setup frame:
/// the pick seeds the slot that fills the turned face, and the confine
/// law judges it there. The slot rides 1 mm above the part top, so the
/// proof is the seeded XY (an air cut gouges nothing). The turned and
/// baked documents cut the same path; the part as modelled has no such
/// face, so it must not cut that path.
#[test]
fn turned_picked_slot_fills_the_turned_face() {
    let mut p = params(OpType::Slot);
    if let OpParams::Slot { top_z, bottom_z, .. } = &mut p {
        (*top_z, *bottom_z) = (Height::of(HeightReference::TopOfModel, 3.0), Height::of(HeightReference::TopOfModel, 1.0));
    }
    let (turned, t_nc) = generate_on(Case::Turned, &p, up_face).unwrap_or_else(|e| panic!("Slot generates turned: {e}"));
    let (baked, b_nc) = generate_on(Case::Baked, &p, up_face).unwrap_or_else(|e| panic!("Slot generates baked: {e}"));
    assert_eq!(turned.moves.len(), baked.moves.len(), "same move count");
    for (i, (a, b)) in turned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "move {i} kind");
        let d = (a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs());
        assert!(d < 1e-6, "move {i}: turned {:?} baked {:?}", a.to, b.to);
    }
    assert_eq!(t_nc, b_nc, "the same setup geometry posts the same program");
}

/// The kinds with no proof keep the refusal by name (#2276 safety rule).
#[test]
fn kinds_without_proof_stay_refused_in_a_turned_setup() {
    let (world, _) = document(Case::Turned);
    let kind = OpType::Probe;
    let op = camcraft_app::schema::Operation {
        tool: TOOL.into(),
        src: SrcRef::Entity("part".into()),
        params: retype_params(&contour_params(), kind as usize),
        stamp: None,
        setup: Some("Setup1".into()),
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let why = setup_align::generation_problem(&world, &op).unwrap_or_else(|| panic!("{kind:?} stays refused"));
    assert!(why.contains("not supported yet in an aligned setup"), "{why}");
    assert!(why.contains("Deburr"), "the supported kinds include Deburr: {why}");
}

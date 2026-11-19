//! #2284: Deburr reads the part and model-relative heights in the setup frame.
//! A two-axis turn must cut and post exactly like the same turn baked into
//! the mesh. The modelled part must cut a different path or refuse.
//! Provenance: analytic L prism and standard rigid-body transform algebra.
//! Gate map: covers app::setup_align, app::operation::job,
//! app::operation::material, app::operation::heights, app::posting,
//! app::stock_sim, app::stock_sim_breaks.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle, transform_point};
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, setup_align, world::new_world};
use flecs_ecs::prelude::World;

const TOOL: &str = "CM-0600-90";
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

/// The part, Setup1, the chamfer mill and one stock box that holds the
/// part in each case. The stock is set after the turn: furniture.
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
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("chamfer mill");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if case == Case::Turned {
        h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", TURN).expect("turn about X and Z"));
    }
    // The turned part is X [10, 20], Y [-5, 25], Z [-5, 15]; the part as
    // modelled is X [0, 30], Y [0, 20], Z [0, 10]. The box holds both.
    let at = Location { pos: Point3::new(15.0, 10.0, -6.0), ..Default::default() };
    let stock = StockShape::Box { dx: 44.0, dy: 54.0, dz: 21.0 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

fn deburr() -> OpParams {
    OpParams::Deburr {
        width: 1.0,
        clearance_z: Height::fixed(30.0),
        top_z: Height::of(HeightReference::TopOfModel, -1.0),
        bottom_z: Height::of(HeightReference::TopOfModel, -1.5),
        feed: 800.0,
        plunge_feed: 200.0,
    }
}

fn source(world: &World, picked: bool) -> SrcRef {
    if !picked {
        return SrcRef::Entity("part".into());
    }
    let part = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &part).expect("features");
    let face = features.faces.first().expect("one face selects the model Deburr reads");
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }])
}

fn generate_on(case: Case, picked: bool) -> Result<(Toolpath, String), String> {
    let (world, mut h) = document(case);
    h.execute(&world, CreateOperation::new(&world, "Deburr", TOOL, source(&world, picked), deburr()).expect("create"));
    let cmd = GenerateOperation::new(&world, "Deburr").map_err(|e| e.to_string())?;
    h.execute(&world, cmd);
    let entity = model::find_by_name(&world, "Deburr").expect("op");
    let cached = model::toolpath_of(&world, &entity).ok_or_else(|| format!("no path: {:?}", operation::status_of(&world, &entity)))?;
    assert!(!cached.path.moves.is_empty(), "{case:?}: cuts contain moves");
    if case == Case::Modelled {
        return Ok((cached.path, String::new()));
    }
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "{case:?}: {:?}", cached.warnings);
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("simulate");
    assert_eq!(sim.dev.gouge_cells, 0, "{case:?}: gouge {} at {:?}", sim.dev.max_gouge, sim.dev.max_gouge_at);
    let program = posting::post_setup_unwritten(&world).expect("safe Deburr posts").program;
    Ok((cached.path, program))
}

fn equivalent(picked: bool) {
    let (turned, turned_nc) = generate_on(Case::Turned, picked).unwrap_or_else(|e| panic!("Deburr generates turned: {e}"));
    let (baked, baked_nc) = generate_on(Case::Baked, picked).unwrap_or_else(|e| panic!("Deburr generates baked: {e}"));
    assert_eq!(turned.moves.len(), baked.moves.len(), "same move count");
    for (i, (a, b)) in turned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "move {i} kind");
        assert!((a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs()) < 1e-6, "move {i}: {a:?} vs {b:?}");
    }
    assert_eq!(turned_nc, baked_nc, "same geometry posts the same program");
    let cuts: Vec<_> = turned.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect();
    assert!(!cuts.is_empty(), "the cone cuts the turned top rim");
    for p in cuts {
        assert!((p.z - 14.0).abs() < 1e-6, "1 mm below the turned top: {p:?}");
        assert!(p.x >= 10.0 - 1e-6 && p.x <= 20.0 + 1e-6 && p.y >= -5.0 - 1e-6 && p.y <= 5.0 + 1e-6, "on the turned top face: {p:?}");
    }
    if let Ok((modelled, _)) = generate_on(Case::Modelled, picked) {
        let differs = modelled.moves.len() != turned.moves.len() || modelled.moves.iter().zip(&turned.moves).any(|(a, b)| (a.to.x - b.to.x).abs() + (a.to.y - b.to.y).abs() + (a.to.z - b.to.z).abs() > 1e-3);
        assert!(differs, "the modelled part must cut a different path or refuse");
    }
}

#[test]
fn turned_deburr_cuts_like_the_baked_part() {
    equivalent(false);
}

#[test]
fn turned_selected_deburr_cuts_like_the_baked_part() {
    equivalent(true);
}

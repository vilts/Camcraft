//! The #541 sinker fixtures, shared by `sinker_burn` and
//! `sinker_burn_held`: the sinker template with one synthetic condition,
//! a 10 x 6 electrode, a block with a rectangular cavity and a document
//! holding them.
//! Provenance: our own SYNTHETIC fixtures (no machine's manual).
#![allow(dead_code)]

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, SinkerBurnOp, SinkerDatum};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::Tool;
use camcraft_app::world::new_world;

pub const SINKER: &str = include_str!("sinker-edm.mch");
pub const MILL_TOOL: &str = "EM-0400-2FL";
pub const ELECTRODE: &str = "E-10x6";
/// Synthetic spark gap per side, mm.
pub const GAP: f64 = 0.05;

/// The sinker template with one synthetic condition.
pub fn sinker_machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "S1", "name": "synthetic rough", "gap": GAP, "undersize": 0.1, "role": "rough" }
    ]);
    v.to_string()
}

pub fn electrode() -> Tool {
    camcraft_tools::electrode::electrode_tool(10.0, 6.0, 30.0, 15.0)
}

pub fn quad(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    out.push(Triangle::new(a, b, c));
    out.push(Triangle::new(a, c, d));
}

/// A 60 x 60 x 20 block (z 0..20) with a rectangular blind cavity
/// `2hx` x `2hy`, `depth` deep, centred on the origin.
pub fn block_with_cavity(hx: f64, hy: f64, depth: f64) -> Mesh {
    let p = Point3::new;
    let (s, top, fl) = (30.0, 20.0, 20.0 - depth);
    let mut f = Vec::new();
    // Top, around the cavity: four strips.
    quad(&mut f, p(-s, -s, top), p(-hx, -s, top), p(-hx, s, top), p(-s, s, top));
    quad(&mut f, p(hx, -s, top), p(s, -s, top), p(s, s, top), p(hx, s, top));
    quad(&mut f, p(-hx, -s, top), p(hx, -s, top), p(hx, -hy, top), p(-hx, -hy, top));
    quad(&mut f, p(-hx, hy, top), p(hx, hy, top), p(hx, s, top), p(-hx, s, top));
    // Cavity floor and walls.
    quad(&mut f, p(-hx, -hy, fl), p(hx, -hy, fl), p(hx, hy, fl), p(-hx, hy, fl));
    quad(&mut f, p(-hx, -hy, fl), p(-hx, -hy, top), p(hx, -hy, top), p(hx, -hy, fl));
    quad(&mut f, p(hx, hy, fl), p(hx, hy, top), p(-hx, hy, top), p(-hx, hy, fl));
    quad(&mut f, p(-hx, hy, fl), p(-hx, hy, top), p(-hx, -hy, top), p(-hx, -hy, fl));
    quad(&mut f, p(hx, -hy, fl), p(hx, -hy, top), p(hx, hy, top), p(hx, hy, fl));
    // Outer walls and bottom.
    quad(&mut f, p(-s, -s, 0.0), p(s, -s, 0.0), p(s, -s, top), p(-s, -s, top));
    quad(&mut f, p(s, s, 0.0), p(-s, s, 0.0), p(-s, s, top), p(s, s, top));
    quad(&mut f, p(-s, s, 0.0), p(-s, -s, 0.0), p(-s, -s, top), p(-s, s, top));
    quad(&mut f, p(s, -s, 0.0), p(s, s, 0.0), p(s, s, top), p(s, -s, top));
    quad(&mut f, p(-s, -s, 0.0), p(-s, s, 0.0), p(s, s, 0.0), p(s, -s, 0.0));
    Mesh::from_facets(f)
}

pub fn burn(datum: SinkerDatum, depth: f64, floor: f64) -> OpParams {
    OpParams::SinkerBurn(SinkerBurnOp {
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(20.0),
        floor_z: Height::fixed(floor),
        datum,
        depth,
        x: 0.0,
        y: 0.0,
        feed: 40.0,
        condition: "S1".into(),
        pass: camcraft_app::operation::SinkerPass::Single,
        cavity_x: 0.0,
        cavity_y: 0.0,
        orbit: None,
        layer: None,
        wear_lost: None,
        wear_over: None,
    })
}

/// A sinker document: the part (cavity `cavity_depth` deep, sized to the
/// electrode plus the gap), a matching block of stock, the sinker
/// machine, the electrode.
pub fn sinker_doc(cavity_depth: f64) -> (World, History) {
    sinker_doc_on(&sinker_machine(), cavity_depth)
}

/// [`sinker_doc`] on a given machine file's text (#542: the sinker post's
/// fixture dialect).
pub fn sinker_doc_on(mch: &str, cavity_depth: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity(5.0 + GAP, 3.0 + GAP, cavity_depth)));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let m = parse_mch(mch).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, ELECTRODE, electrode()).expect("an electrode is a tool"));
    (world, h)
}

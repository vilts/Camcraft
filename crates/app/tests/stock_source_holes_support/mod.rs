//! Fixtures of the #1631 suite (`stock_source_holes`): a 30 x 30 x 20
//! plate holding one modelled round hole, in a square box stock aligned
//! or turned a quarter about Z; stock-source Drill, Bore, Thread and 2D
//! Chamfer operations; the post and MCP readers. Not a suite itself.
//!
//! Provenance: our own fixtures over `common::solid_part`; no vendor
//! material consulted.

#![allow(dead_code)]

use crate::common::solid_part::{circle, plate, rect};

pub use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
pub use camcraft_app::drilling::{CycleChoice, Hole};
pub use camcraft_app::geo2d::Point2;
pub use camcraft_app::geometry::{Mesh, Point3};
pub use camcraft_app::machining2d::Side;
pub use camcraft_app::mcp::Server;
pub use camcraft_app::model;
pub use camcraft_app::operation::{Height, OpParams, SrcRef};
pub use camcraft_app::posting;
pub use camcraft_app::schema::Location;
pub use camcraft_app::stock::StockShape;
pub use camcraft_app::tools::{catalog, Tool};
pub use camcraft_app::verify::Warning;
pub use camcraft_app::world::new_world;
pub use flecs_ecs::prelude::World;
pub use serde_json::{json, Value};

/// The plate's top; the stock's top stands flush with it.
pub const TOP: f64 = 20.0;
/// Half the side of the roomy stock: 20 mm of stock-only material
/// stands round the 30 x 30 part.
pub const HALF: f64 = 35.0;
pub const DRILL: &str = "DR-0600";
pub const BAR: &str = "EM-0600-4FL";
pub const THREAD: &str = "TM-06";
pub const CHAMFER: &str = "CM-0600-90";
/// The catalog M6 x 1 tap: its core Ø4.917 rides a Ø5 tap-drill hole.
pub const TAP: &str = "TAP-M6";
const TAP_DRILL: &str = "TAP-DRILL";

/// The part's one modelled hole at the origin: radius, blind floor,
/// and the inscribed n-gon it is tessellated as.
#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub r: f64,
    pub floor: f64,
    pub sides: usize,
}

impl Model {
    /// A 32-gon: its flats stand 0.012-0.024 mm inside the circle at
    /// these radii - past the 0.01 gouge tolerance, so a tool at the
    /// hole's size passes only by the containment law's wall excuse.
    pub fn hole(r: f64, floor: f64) -> Self {
        Model { r, floor, sides: 32 }
    }

    /// A 96-gon, for a thread mill (#1205: a thread's crest allowance
    /// holds against a finely faceted hole, not a coarse one - on a part
    /// source as well).
    pub fn fine(r: f64, floor: f64) -> Self {
        Model { r, floor, sides: 96 }
    }

    pub fn mesh(self) -> Mesh {
        let mut f = Vec::new();
        plate(&rect(-15.0, -15.0, 15.0, 15.0), Some((&circle((0.0, 0.0), self.r, self.sides), self.floor)), 0.0, TOP, &mut f);
        Mesh::from_facets(f)
    }
}

/// The hole an operation declares: centre, radius, floor.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub floor: f64,
}

impl Cut {
    pub fn at(x: f64, y: f64, r: f64, floor: f64) -> Self {
        Cut { x, y, r, floor }
    }

    fn hole(self) -> Hole {
        Hole { xy: Point2::new(self.x, self.y), radius: self.r, entry_z: TOP, floor_z: self.floor, through: false, stepped: false }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Drill,
    /// A Drill in its Tap cycle with [`TAP`] (#266's core re-read).
    Tap,
    Bore,
    Thread,
}

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A 6 mm multi-form thread mill of pitch 1 (#894: the pitch is the
/// tool's own).
fn thread_tool() -> Tool {
    Tool { material_group: "steel".into(), ..camcraft_app::thread_mill::thread_mill(6.0, 1.0, 0.0, 25.0) }
}

fn tools(world: &World, h: &mut History) {
    let mut drill = catalog_tool(DRILL);
    drill.flute_len = 30.0; // long series: the holes run 10 mm and more
    for (name, tool) in [(DRILL, drill), (BAR, catalog_tool(BAR)), (THREAD, thread_tool()), (CHAMFER, catalog_tool(CHAMFER)), (TAP, catalog_tool(TAP))] {
        h.execute(world, AddTool::new(world, name, tool).expect("tool"));
    }
}

fn stock(world: &World, h: &mut History, half: f64, rotated: bool) {
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: crate::pocket_support::stock_turn(rotated) };
    h.execute(world, SetStock::new(world, StockShape::Box { dx: 2.0 * half, dy: 2.0 * half, dz: TOP }, at).expect("stock"));
}

/// The plate as "part" with `model`'s hole in a `2 half` square stock
/// over [0, TOP], aligned or turned `pocket_support::OBLIQUE` about Z
/// (the same footprint within 1e-4 mm, a turn the program sim refuses to
/// model). #2177: a quarter turn is no such device - the sim judges a
/// square-turned box.
pub fn job(model: Model, half: f64, rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    tools(&world, &mut h);
    h.execute(&world, ImportModel::from_mesh("part", model.mesh()));
    stock(&world, &mut h, half, rotated);
    (world, h)
}

/// The plate (Ø5 tap-drill hole to Z 8) with a 20 x 20 x 4 "lid" part
/// standing on its top over the hole, in a `2 HALF` square stock 24 mm
/// tall; then a part-source M6 Tap cycle on "part" from the lid's top
/// to Z 10 - through the lid, which models no hole. Its cached warnings.
pub fn tap_through_a_lid(rotated: bool) -> Vec<Warning> {
    let world = new_world();
    let mut h = History::default();
    tools(&world, &mut h);
    h.execute(&world, ImportModel::from_mesh("part", Model::hole(2.5, 8.0).mesh()));
    let lid = camcraft_app::geometry::cube(Point3::new(-10.0, -10.0, TOP), Point3::new(10.0, 10.0, TOP + 4.0));
    h.execute(&world, ImportModel::from_mesh("lid", Mesh::from_facets(lid.facets.to_vec())));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: crate::pocket_support::stock_turn(rotated) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: TOP + 4.0 }, at).expect("stock"));
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.5, entry_z: TOP + 4.0, floor_z: 10.0, through: false, stepped: false };
    let params = OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 14.0),
        retract_z: None,
        top_z: Height::fixed(TOP + 4.0),
        bottom_z: None,
        cycle: CycleChoice::Tap,
        peck: 0.0,
        dwell: 0.0,
        feed: 600.0,
        plunge_feed: 600.0,
        holes: vec![hole],
        cross: Vec::new(),
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "S1", TAP, SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "S1").expect("generate"));
    let e = model::find_by_name(&world, "S1").expect("op");
    model::toolpath_of(&world, &e).expect("cached path").warnings.clone()
}

/// The same stock with no part in the setup.
pub fn job_without_part(half: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    tools(&world, &mut h);
    stock(&world, &mut h, half, false);
    (world, h)
}

fn drill(cut: Cut, cycle: CycleChoice) -> OpParams {
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![cut.hole()],
        cross: Vec::new(),
        picks: Default::default(),
    }
}

fn bore(cut: Cut) -> OpParams {
    OpParams::Bore {
        pass_start: camcraft_app::machining2d::PassStart::Auto,
        break_through: None,
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![cut.hole()],
        picks: Default::default(),
    }
}

fn thread(cut: Cut) -> OpParams {
    OpParams::Thread {
        break_through: None,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        infeed: 0.1,
        // #2598: the thread mill's own 300 mm/min.
        feed: 300.0,
        plunge_feed: 150.0,
        climb: true,
        holes: vec![cut.hole()],
        pitch: None,
        hand: camcraft_app::cl::ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
        picks: Default::default(),
    }
}

/// A 2D Chamfer of `width` at 45 degrees round the stock's top.
pub fn chamfer(width: f64) -> OpParams {
    OpParams::Chamfer2D {
        width,
        angle_deg: 45.0,
        minimum_cone_radius: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        tolerance: 0.01,
        tilt: None,
        picks: Default::default(),
    }
}

/// Create and generate the stock-source "S1" with `tool`; its cached
/// warnings.
pub fn generated(world: &World, h: &mut History, tool: &str, params: OpParams) -> Vec<Warning> {
    h.execute(world, CreateOperation::new(world, "S1", tool, SrcRef::Stock, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "S1").expect("generate"));
    let e = model::find_by_name(world, "S1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the cut was generated");
    tp.warnings.clone()
}

/// A stock-source hole op of `kind` on `cut`. A Thread first has its
/// hole opened off the stock to its minor diameter (#992: a thread mill
/// is not centre-cutting) by a Drill of that size, 3 mm past the
/// thread's floor for the drill's point.
pub fn hole_op(world: &World, h: &mut History, kind: Kind, cut: Cut) -> Vec<Warning> {
    match kind {
        Kind::Drill => generated(world, h, DRILL, drill(cut, CycleChoice::Drill)),
        Kind::Tap => generated(world, h, TAP, drill(cut, CycleChoice::Tap)),
        Kind::Bore => generated(world, h, BAR, bore(cut)),
        Kind::Thread => {
            let d = cut.r * 2.0;
            let tool = Tool { diameter: d, shank_d: d, flute_len: 30.0, ..catalog_tool(DRILL) };
            h.execute(world, AddTool::new(world, TAP_DRILL, tool).expect("tap drill"));
            let ahead = drill(Cut { floor: cut.floor - 3.0, ..cut }, CycleChoice::Drill);
            h.execute(world, CreateOperation::new(world, "S0", TAP_DRILL, SrcRef::Stock, ahead).expect("create drill"));
            h.execute(world, GenerateOperation::new(world, "S0").expect("generate drill"));
            generated(world, h, THREAD, thread(cut))
        }
    }
}

/// A stock-source Thread on a TURNED stock: #992's way-in law reads the
/// program ahead on the column grid, which cannot model the turn, so
/// the thread is refused by name at generation - before any part law -
/// and the setup never posts. Returns the refusal.
pub fn thread_refused_on_a_turned_stock(world: &World, h: &mut History, cut: Cut) -> String {
    let d = cut.r * 2.0;
    let tool = Tool { diameter: d, shank_d: d, flute_len: 30.0, ..catalog_tool(DRILL) };
    h.execute(world, AddTool::new(world, TAP_DRILL, tool).expect("tap drill"));
    let ahead = drill(Cut { floor: cut.floor - 3.0, ..cut }, CycleChoice::Drill);
    h.execute(world, CreateOperation::new(world, "S0", TAP_DRILL, SrcRef::Stock, ahead).expect("create drill"));
    h.execute(world, GenerateOperation::new(world, "S0").expect("generate drill"));
    h.execute(world, CreateOperation::new(world, "S1", THREAD, SrcRef::Stock, thread(cut)).expect("create"));
    match GenerateOperation::new(world, "S1") {
        Ok(_) => panic!("a thread off a turned stock generated"),
        Err(e) => format!("{e:?}"),
    }
}

pub fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. } | Warning::StockChamferGouge { .. } | Warning::StockDeburrGouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-stock-source-holes-{tag}-{}.nc", std::process::id()))
}

/// The post is refused and nothing is written. `by_op`: the refusal
/// names S1's own gouge (on an aligned stock the program sim's clause
/// may lead).
pub fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        assert!(err.contains("S1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
        assert!(crate::pocket_support::oblique_refused(world), "{tag}: the device holds: the program sim does not judge this stock");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

/// The post is refused with `why` and nothing is written.
pub fn assert_not_posted(world: &World, tag: &str, why: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("the setup never posts");
    assert!(err.contains(why), "{tag}: {err}");
    assert!(!out.exists(), "{tag}: refused output is never written");
}

pub fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: a legal stock-source cut posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

// ---- MCP ------------------------------------------------------------

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

/// The plate (r 3 hole to Z 8) over MCP in a `2 half` square stock: a
/// stock-source `kind` with `params`; verify's reply.
pub fn mcp_verify(tag: &str, half: f64, kind: &str, tool: &str, params: Value) -> Value {
    let dir = std::env::temp_dir().join(format!("camcraft-stock-source-holes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("part.stl");
    crate::pocket_support::write_stl(&Model::hole(3.0, 8.0).mesh(), &stl);
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 2.0 * half, "dy": 2.0 * half, "dz": TOP } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": tool }));
    call(&mut s, &mut id, "add_operation", json!({ "kind": kind, "tool": tool, "name": "S1", "params": params }));
    let g = call(&mut s, &mut id, "generate", json!({}));
    assert_eq!(g["generated"][0]["status"], json!("ok"), "{g}");
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    ver
}

/// The operation's own blocking gouge findings (not the program gate's).
pub fn op_gouges(ver: &Value) -> usize {
    ver["findings"]
        .as_array()
        .map_or(0, |f| f.iter().filter(|x| x["kind"] == json!("gouge") && x["op"] == json!("S1") && x["blocks_post"] == json!(true)).count())
}

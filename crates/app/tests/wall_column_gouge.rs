//! #299: a correct program on the user's own part
//! (`testdata/models/Milling-test.step`, a D-shaped plate) must not be
//! refused for a gouge the tool never cut. The program sim read a
//! lattice column whose centre sits microns inside the plate's wall,
//! reached by a tool finishing that wall, as cut the whole wall's
//! height. The law that fixes it (lateral slack of `tol` on the gouge
//! verdict) and the real-gouge convictions are pinned with the kernel
//! in `crates/sim/tests/wall_slack.rs`; this file is the issue's jobs,
//! driven through the MCP tools and the commands as reported.
//!
//! Gate map (M4.4): covers app::mcp, app::posting_sim, app::stock_sim, sim::stock_sim.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, HeightReference, KindInfo, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use serde_json::{json, Value};

fn step_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .display()
        .to_string()
}

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert_ne!(result["isError"], json!(true), "{name} refused: {}", result["content"][0]["text"]);
    result["structuredContent"].clone()
}

#[test]
fn a_3d_finish_on_the_users_part_reads_no_gouge_at_its_wall() {
    // The issue's MCP job with the 3D Finish it names: model stock, a
    // 6 mm ball, kind defaults. Before #299 the gate read one column
    // 2 um inside the D's wall as a 10.21 mm gouge at (16.69, -24.91).
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": step_path() }));
    call(&mut s, &mut id, "set_stock", json!({ "from_model": "part" }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": "BN-0600" }));
    call(&mut s, &mut id, "add_operation", json!({ "kind": "3D Finish", "tool": "BN-0600", "source": "part", "name": "Finish1" }));
    call(&mut s, &mut id, "generate", json!({}));
    let ver = call(&mut s, &mut id, "verify", json!({}));
    let gate = ver["gate"]["problems"].as_array().cloned().unwrap_or_default();
    assert!(gate.iter().all(|p| !p.as_str().unwrap_or("").contains("gouge")), "{gate:?}");
}

#[test]
fn the_issues_contour_on_model_stock_reads_no_gouge() {
    // The issue's exact contour job (10 mm deep around the part).
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "part", "path": step_path() }));
    call(&mut s, &mut id, "set_stock", json!({ "from_model": "part" }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": "EM-0600-4FL" }));
    let params = json!({ "bottom_z": { "reference": "top_of_model", "offset": -10 } });
    call(&mut s, &mut id, "add_operation", json!({ "kind": "contour", "tool": "EM-0600-4FL", "source": "part", "name": "Contour1", "params": params }));
    call(&mut s, &mut id, "generate", json!({}));
    let ver = call(&mut s, &mut id, "verify", json!({}));
    assert_eq!(ver["gate"]["problems"], json!([]), "{ver}");
}

#[test]
fn a_3d_finish_in_the_users_box_stock_reads_no_gouge_at_its_wall() {
    // The issue's realistic setup: `lead_approach.rs`'s 70 x 70 x 27
    // box at z -7, a 3D Finish with a 6 mm ball, kind defaults. Before
    // #299: 37 gouge cells along the D's curved wall, worst 22.01 mm,
    // every one a ball tangent to the wall.
    let world = new_world();
    let mut h = History::default();
    let imp = ImportModel::new(std::path::Path::new(&step_path())).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let kind = (0..KindInfo::ALL.len())
        .find(|&i| KindInfo::ALL.get(i).is_some_and(|k| k.label == "3D Finish"))
        .expect("3D Finish kind");
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    let params = operation::retype_params(&base, kind);
    h.execute(&world, CreateOperation::new(&world, "Finish1", "BN-0600", SrcRef::Entity(name), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Finish1").expect("generate"));
    let sim = simulate_document(&world).expect("the program sim runs");
    assert_eq!(
        sim.dev.gouge_cells, 0,
        "worst {:.3} mm at {:?}",
        sim.dev.max_gouge, sim.dev.max_gouge_at
    );
}

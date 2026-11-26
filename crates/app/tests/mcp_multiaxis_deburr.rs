//! The Multi-Axis Deburr through MCP (#1894): `add_operation` takes the
//! kind, `edit_operation` sets the axis mode, the lean and the tilt cap
//! (and the command door refuses a lean outside the cap),
//! `set_operation_geometry` picks the edge chains, and `generate` rides
//! the vertical corner edges with a tilted axis.
//!
//! Gate map (#1894): covers app::mcp (add, edit, pick, generate),
//! app::project (the MultiAxisDeburr params DTO), app::operation (the
//! kind row, the seed, the generate arm), app::commands
//! (CreateOperation, EditOperationParams, SetOperationGeometry).
//! Also covers app::operation::keep_law, app::operation::kept_rejudge,
//! and app::operation::multiaxis_deburr_op (the posed stock guard).

use camcraft_app::geometry::{cube, Point3};
use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::PathBuf;

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

/// A 40 x 40 x 10 brick as ASCII STL, a stock box over it, the
/// 90-degree chamfer mill and a Multi-Axis Deburr of the whole part.
fn job(tag: &str) -> (Client, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-mcp-ma-deburr-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut s = String::from("solid part\n");
    for t in cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid part\n");
    let stl = dir.join("part.stl");
    std::fs::write(&stl, s).expect("write stl");

    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "CM-0600-90" }));
    let add = c.ok("add_operation", json!({ "kind": "multi_axis_deburr", "tool": "CM-0600-90", "name": "M1", "source": "part" }));
    assert_eq!(add["operation"], json!("M1"));
    (c, dir)
}

#[test]
fn multiaxis_deburr_lists_edits_and_refuses_a_lean_outside_its_cap() {
    let (mut c, dir) = job("edit");
    let kinds = c.ok("list_operation_types", json!({})).to_string();
    assert!(kinds.contains("Multi-Axis Deburr"), "the kind lists: {kinds}");

    let edit = c.ok("edit_operation", json!({ "name": "M1", "params": {
        "axis_mode": "axis4", "lead_deg": 5.0, "max_tilt_deg": 40.0, "shank_avoid": false,
    } }));
    let text = edit["params"].to_string();
    for want in ["\"axis_mode\":\"axis4\"", "\"lead_deg\":5.0", "\"max_tilt_deg\":40.0", "\"shank_avoid\":false"] {
        assert!(text.contains(want), "the answer shows {want}: {text}");
    }
    let why = c.refused("edit_operation", json!({ "name": "M1", "params": { "lead_deg": 60.0 } }));
    assert!(!why.is_empty());

    // An edge chain is a pick: the source becomes the picked edge.
    let f = c.ok("list_features", json!({ "model": "part", "kinds": ["chains"] }));
    let chain = f["chains"][0]["id"].as_str().expect("a chain id").to_owned();
    let set = c.ok("set_operation_geometry", json!({ "operation": "M1", "picks": [{ "model": "part", "id": chain }] }));
    assert_eq!(set["source"]["picks"][0]["id"], json!(chain), "{set}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn multiaxis_deburr_generates_tilted_breaks_on_the_vertical_corners() {
    let (mut c, dir) = job("gen");
    c.ok("edit_operation", json!({ "name": "M1", "params": { "max_tilt_deg": 45.0, "bottom_z": 1.0 } }));
    let made = c.ok("generate", json!({ "name": "M1" }));
    assert!(made.to_string().contains("M1"), "{made}");
    let v = c.ok("verify", json!({ "name": "M1" }));
    assert_eq!(v["ok"], json!(true), "the tilted ride verifies clean: {v}");
    assert_eq!(v["findings"], json!([]), "{v}");
    // The program holds posed (tilted) moves: the column stock sim says
    // so by name instead of judging a tilted tool upright.
    let gate = v["gate"].to_string();
    assert!(gate.contains("posed moves"), "the ride tilts the axis: {gate}");
    // A machine with no tilting rotary refuses the tilted moves by name.
    let out = dir.join("out.nc").display().to_string();
    let why = c.refused("post", json!({ "output": out }));
    assert!(why.contains("tilted move on a machine without a tilting rotary"), "{why}");
    // The tilt-head machine owns the letters: the program posts.
    c.ok("load_machine", json!({ "builtin": "Tilt-head 5-axis" }));
    let posted = c.ok("post", json!({ "output": out }));
    assert_eq!(posted["ops"], json!(1), "{posted}");
    let nc = std::fs::read_to_string(&out).expect("the NC file is written");
    let turns = |w: &str| w.starts_with('A') && w.trim_start_matches('A').parse::<f64>().is_ok_and(|v| v.abs() > 1e-6);
    let tilted = nc.lines().filter(|l| l.split_whitespace().any(turns)).count();
    assert!(tilted > 0, "the posted program turns the A axis: {nc}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The review's 0.1 mm cube lies outside the source brick, inside a
/// formerly admitted tilted cone. It must receive no chamfer exemption.
fn import_neighbour(c: &mut Client, dir: &std::path::Path) {
    let center = Point3::new(-19.5449504775, 20.6014961319, 2.7993502387);
    let mut stl = String::from("solid other\n");
    let mesh = cube(
        Point3::new(center.x - 0.05, center.y - 0.05, center.z - 0.05),
        Point3::new(center.x + 0.05, center.y + 0.05, center.z + 0.05),
    );
    for t in mesh.facets.iter() {
        stl.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            stl.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        stl.push_str("  endloop\n endfacet\n");
    }
    stl.push_str("endsolid other\n");
    let path = dir.join("other.stl");
    std::fs::write(&path, stl).expect("write neighboring part");
    c.ok("import_model", json!({ "path": path.display().to_string() }));
}

fn generated_source(tag: &str) -> (Client, PathBuf) {
    let (mut c, dir) = job(tag);
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("edit_operation", json!({ "name": "M1", "params": {
        "clearance_z": 35.0, "top_z": 10.0, "bottom_z": 1.0, "max_tilt_deg": 45.0,
    } }));
    c.ok("load_machine", json!({ "builtin": "Tilt-head 5-axis" }));
    c.ok("generate", json!({ "name": "M1" }));
    let clear_out = dir.join("clear.nc");
    let posted = c.ok("post", json!({ "output": clear_out.display().to_string() }));
    assert_eq!(posted["ops"], json!(1), "the source alone still posts: {posted}");
    assert!(clear_out.is_file(), "the clear control writes its NC file");
    (c, dir)
}

#[test]
fn multiaxis_deburr_refuses_regeneration_with_a_separate_kept_part() {
    let (mut c, dir) = generated_source("neighbour-generate");
    import_neighbour(&mut c, &dir);
    let why = c.refused("generate", json!({ "name": "M1" }));
    assert!(why.contains("Multi-Axis Deburr") && why.contains("other kept parts"), "{why}");
    let out = dir.join("unsafe.nc");
    c.refused("post", json!({ "output": out.display().to_string() }));
    assert!(!out.exists(), "a refused generation must not post an NC file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn multiaxis_deburr_rechecks_a_separate_part_before_posting_the_cached_path() {
    let (mut c, dir) = generated_source("neighbour-post");
    import_neighbour(&mut c, &dir);
    // Do not regenerate: posting must read the newly imported part live.
    let out = dir.join("unsafe.nc");
    let why = c.refused("post", json!({ "output": out.display().to_string() }));
    for want in ["M1", "Multi-Axis Deburr", "other"] {
        assert!(why.contains(want), "the live refusal names {want}: {why}");
    }
    assert!(!out.exists(), "the cached path must not write an NC file");
    let _ = std::fs::remove_dir_all(dir);
}

/// The brick's vertical edges are at x/y = +/-20. The 60 mm stock
/// leaves 10 mm of material beside them, where the tilted entry travels.
fn stock_beside_vertical_edges(c: &mut Client) {
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 10.0 } }));
}

#[test]
fn multiaxis_deburr_refuses_stock_beside_the_vertical_edge() {
    let (mut c, dir) = generated_source("stock-generate");
    // Centre cutting can permit axial entry. It cannot permit a holder in stock.
    c.ok("edit_tool", json!({ "name": "CM-0600-90", "patch": { "center_cutting": true } }));
    stock_beside_vertical_edges(&mut c);
    let why = c.refused("generate", json!({ "name": "M1" }));
    for want in ["Multi-Axis Deburr", "stock"] {
        assert!(why.contains(want), "the generation refusal names {want}: {why}");
    }
    let out = dir.join("unsafe.nc");
    c.refused("post", json!({ "output": out.display().to_string() }));
    assert!(!out.exists(), "the refused stock cannot produce an NC file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn multiaxis_deburr_rechecks_stock_before_posting_the_cached_path() {
    let (mut c, dir) = generated_source("stock-post");
    stock_beside_vertical_edges(&mut c);
    // Do not regenerate. The cached posed path has no verdict for this stock.
    let out = dir.join("unsafe.nc");
    let why = c.refused("post", json!({ "output": out.display().to_string() }));
    for want in ["M1", "inputs moved since generation", "regenerate"] {
        assert!(why.contains(want), "the cached-path refusal names {want}: {why}");
    }
    assert!(!out.exists(), "the cached path must not write an NC file through stock");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn multiaxis_deburr_live_stock_judge_refuses_an_unjudged_cached_path() {
    use camcraft_app::{commands, model, operation, project, stock, world};
    let (mut c, dir) = generated_source("stock-live-judge");
    let saved = dir.join("clear.camcraft");
    c.ok("save_project", json!({ "path": saved.display().to_string() }));
    let world = world::new_world();
    project::load(&world, &saved).expect("load the clear source job");
    let mut history = commands::History::default();
    history.execute(&world, commands::GenerateOperation::new(&world, "M1").expect("regenerate the clear source"));
    let e = model::find_by_name(&world, "M1").expect("the operation");
    let op = model::operation_of(&world, &e).expect("the operation parameters");
    let cached = model::toolpath_of(&world, &e).expect("the clear posed path");
    assert!(operation::kept_rejudge::moved_part_problem(&world, "M1", &op, &cached.path).is_none());
    history.execute(&world, commands::SetStock::new(
        &world, stock::StockShape::Box { dx: 60.0, dy: 60.0, dz: 10.0 }, Default::default(),
    ).expect("stock beside the vertical edges"));
    // Exercise the live judge directly. The ordinary post door also refuses
    // this stock edit as stale before it reaches the live collision judge.
    let why = operation::kept_rejudge::moved_part_problem(&world, "M1", &op, &cached.path)
        .expect("the live judge must not accept the old posed path against unjudged stock");
    for want in ["M1", "Multi-Axis Deburr", "stock"] {
        assert!(why.contains(want), "the live refusal names {want}: {why}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn multiaxis_deburr_params_round_trip_through_the_project_grammar() {
    use camcraft_app::operation::{Carried, Height, OpParams, OpType};
    use camcraft_app::project::{params_from_json, params_to_json};
    let carried = Carried {
        feed: 600.0,
        plunge: 200.0,
        climb: true,
        clearance: Height::fixed(30.0),
        stepover: 1.0,
        stepdown: 1.0,
        top: Height::fixed(10.0),
        bottom: Height::fixed(0.0),
        retract: Height::fixed(15.0),
        tilt: None,
    };
    let mut p = (OpType::MultiAxisDeburr.info().seed)(&carried);
    let OpParams::MultiAxisDeburr { axis_mode, lead_deg, lag_deg, max_tilt_deg, shank_avoid, .. } = &mut p else {
        panic!("the seed is the kind's own: {p:?}");
    };
    *axis_mode = camcraft_app::multiaxis::DeburrAxisMode::Axis4;
    *lead_deg = 7.5;
    *lag_deg = 2.5;
    *max_tilt_deg = 40.0;
    *shank_avoid = false;
    let v = params_to_json(&p);
    assert!(v.get("multi_axis_deburr").is_some(), "the file tag: {v}");
    assert_eq!(params_from_json(&v).expect("reads back"), p, "{v}");
}

#[test]
fn disabling_shank_avoid_cannot_bypass_the_mandatory_holder_collision_guard() {
    use camcraft_app::finishing3d::{DeburrEdges, UnreachableEdges};
    use camcraft_app::multiaxis::{Shank, multiaxis_deburr};
    use camcraft_app::operation::{
        Carried, Height, OpParams, OpType, multiaxis_deburr_op::kernel_params,
    };
    let carried = Carried {
        feed: 400.0,
        plunge: 100.0,
        climb: true,
        clearance: Height::fixed(70.0),
        stepover: 1.0,
        stepdown: 1.0,
        top: Height::fixed(10.0),
        bottom: Height::fixed(1.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let mut params = (OpType::MultiAxisDeburr.info().seed)(&carried);
    let OpParams::MultiAxisDeburr {
        shank_avoid,
        max_tilt_deg,
        unreachable,
        ..
    } = &mut params
    else {
        panic!("the operation has its own parameters");
    };
    *shank_avoid = false;
    *max_tilt_deg = 0.0;
    *unreachable = UnreachableEdges::Refuse;
    let shank = Shank {
        flute_len: 8.0,
        shank_r: 4.0,
        shank_len: 12.0,
        holder_r: 8.0,
        holder_len: 40.0,
    };
    let p =
        kernel_params(&params, (70.0, 10.0, 1.0), 90.0, 6.0, shank).expect("the kernel parameters");
    let mut facets = cube(Point3::ZERO, Point3::new(40.0, 40.0, 10.0))
        .facets
        .to_vec();
    let chains = [vec![
        Point3::new(0.0, 0.0, 10.0),
        Point3::new(40.0, 0.0, 10.0),
    ]];
    let edges = DeburrEdges {
        include: &chains,
        exclude: &[],
    };
    let clear = multiaxis_deburr(&facets, &p, &edges)
        .expect("the clear edge still cuts with the option off");
    assert!(!clear.breaks.is_empty());
    // This block misses the cone and the shank. Only the holder reaches it.
    facets.extend(
        cube(
            Point3::new(18.0, -10.0, 35.0),
            Point3::new(22.0, -6.0, 37.0),
        )
        .facets
        .iter()
        .copied(),
    );
    let err = multiaxis_deburr(&facets, &p, &edges)
        .expect_err("the checkbox cannot allow a holder collision");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("multi-axis deburr") && msg.contains("Refuse"),
        "{msg}"
    );
}

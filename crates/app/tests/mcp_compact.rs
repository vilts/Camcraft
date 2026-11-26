//! #1944: compact MCP read modes keep a CAM job inside a model's context
//! window. `get_job` with `summary` drops the machine definition and the
//! operation fields that still hold their defaults; `list_operation_types`
//! filters by kind or section; `list_features` with `compact` lists short
//! rows under a chat-sized default limit; and `provider::tools_in` offers a
//! core tool subset for small local models. Each reply fits a budget.
//!
//! Gate map (#1944): covers camcraft_app::mcp (compact, job, features,
//! provider, tools).
//!
//! Provenance: our own fixtures (common::solid_part); no vendor material.

mod common;

use camcraft_app::geometry::Triangle;
use camcraft_app::mcp::provider::{self, Provider, ToolSet};
use camcraft_app::mcp::Server;
use common::solid_part::{circle, plate, rect};
use serde_json::{json, Value};

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let err = result["isError"] == json!(true);
    (
        err,
        if err {
            result["content"][0]["text"].clone()
        } else {
            result["structuredContent"].clone()
        },
    )
}

fn ok(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let (err, body) = call(s, id, name, args);
    assert!(!err, "{name} refused: {body}");
    body
}

fn chars(v: &Value) -> usize {
    v.to_string().chars().count()
}

/// A 60 x 40 plate, z 0..10, with eight round holes: many faces and chains.
fn write_plate_stl(dir: &std::path::Path) -> std::path::PathBuf {
    let mut f: Vec<Triangle> = Vec::new();
    plate(&rect(-30.0, -20.0, 30.0, 20.0), None, 0.0, 10.0, &mut f);
    for i in 0..8 {
        let x = -24.0 + 6.0 * f64::from(i);
        plate(&circle((x, 0.0), 2.0, 24), None, 10.0, 12.0, &mut f);
    }
    let mut s = String::from("solid plate\n");
    for t in &f {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid plate\n");
    let path = dir.join("plate.stl");
    std::fs::write(&path, s).expect("write stl");
    path
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-1944-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A job with a machine, a stock, two tools and three operations.
fn job(s: &mut Server) {
    ok(s, 1, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    ok(
        s,
        2,
        "set_stock",
        json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }),
    );
    ok(s, 3, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    ok(s, 4, "add_tool", json!({ "catalog": "EM-0600-4FL" }));
    ok(
        s,
        5,
        "add_operation",
        json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F1" }),
    );
    ok(
        s,
        6,
        "add_operation",
        json!({ "kind": "face", "tool": "EM-0600-4FL", "name": "F2", "params": { "stepover": 2.5 } }),
    );
    ok(
        s,
        7,
        "add_operation",
        json!({ "kind": "pocket", "tool": "EM-0600-4FL", "name": "P1" }),
    );
}

#[test]
fn get_job_summary_drops_the_machine_definition_and_default_fields() {
    let mut s = Server::new();
    job(&mut s);
    let full = ok(&mut s, 10, "get_job", json!({}));
    let short = ok(&mut s, 11, "get_job", json!({ "summary": true }));

    assert!(
        full["machine"]["definition"].is_object(),
        "the full job keeps the machine: {}",
        full["machine"]
    );
    assert!(
        short["machine"].get("definition").is_none(),
        "{}",
        short["machine"]
    );
    assert_eq!(short["machine"]["definition_omitted"], json!(true));
    assert_eq!(short["summary"], json!(true));

    // F2 changed its stepover: the summary keeps that field and drops the rest.
    let ops = short["operations"].as_array().expect("operations");
    assert_eq!(ops.len(), 3);
    let f2 = ops.iter().find(|o| o["name"] == json!("F2")).expect("F2");
    let changed = f2["params"]["face"].as_object().expect("face params");
    assert_eq!(changed.get("stepover"), Some(&json!(2.5)), "{f2}");
    assert!(changed.len() < 4, "only the changed fields remain: {f2}");
    let full_f2 = full["operations"]
        .as_array()
        .and_then(|o| o.iter().find(|o| o["name"] == json!("F2")))
        .expect("F2");
    assert!(full_f2["params"]["face"].as_object().map_or(0, |m| m.len()) > changed.len());

    // No null rows outside params: a summary says only what is set.
    assert!(
        f2.get("checks_against").is_none() && f2.get("needs_review").is_none(),
        "{f2}"
    );

    // The budget: the summary is at most two thirds of the full reply and fits 2500 characters.
    assert!(
        chars(&short) * 3 <= chars(&full) * 2,
        "summary {} vs full {}",
        chars(&short),
        chars(&full)
    );
    assert!(
        chars(&short) <= 2_500,
        "summary is {} characters",
        chars(&short)
    );
}

#[test]
fn get_job_summary_refuses_a_non_boolean() {
    let mut s = Server::new();
    let (err, body) = call(&mut s, 1, "get_job", json!({ "summary": "yes" }));
    assert!(err, "{body}");
    assert!(
        body.as_str().is_some_and(|t| t.contains("\"summary\"")),
        "{body}"
    );
}

#[test]
fn list_operation_types_filters_by_kind_and_by_section() {
    let mut s = Server::new();
    let all = ok(&mut s, 1, "list_operation_types", json!({}));
    let n_all = all["kinds"].as_array().map_or(0, Vec::len);

    let face = ok(&mut s, 2, "list_operation_types", json!({ "kind": "face" }));
    let kinds = face["kinds"].as_array().expect("kinds");
    assert_eq!(kinds.len(), 1, "{face}");
    assert_eq!(kinds[0]["key"], json!("face"));
    let by_label = ok(&mut s, 3, "list_operation_types", json!({ "kind": "Face" }));
    assert_eq!(
        by_label["kinds"], face["kinds"],
        "a label matches like a key"
    );

    let drilling = ok(
        &mut s,
        4,
        "list_operation_types",
        json!({ "section": "drilling" }),
    );
    let rows = drilling["kinds"].as_array().expect("kinds");
    assert!(
        !rows.is_empty() && rows.len() < n_all,
        "{} of {n_all}",
        rows.len()
    );
    assert!(
        rows.iter().all(|k| k["section"] == json!("Drilling")),
        "{drilling}"
    );
    assert!(
        rows.iter().any(|k| k["key"] == json!("drill")),
        "{drilling}"
    );

    // The budget: one kind fits 4000 characters; the whole table does not.
    assert!(
        chars(&face) <= 4_000,
        "one kind is {} characters",
        chars(&face)
    );
    assert!(chars(&all) > 4_000);

    let (err, body) = call(
        &mut s,
        5,
        "list_operation_types",
        json!({ "kind": "laser" }),
    );
    assert!(
        err && body.as_str().is_some_and(|t| t.contains("laser")),
        "{body}"
    );
    let (err, body) = call(
        &mut s,
        6,
        "list_operation_types",
        json!({ "section": "Sawing" }),
    );
    assert!(
        err && body.as_str().is_some_and(|t| t.contains("Mill2D")),
        "{body}"
    );
}

#[test]
fn list_features_compact_rows_are_short_and_the_default_limit_is_chat_sized() {
    let dir = temp_dir("features");
    let stl = write_plate_stl(&dir);
    let mut s = Server::new();
    ok(
        &mut s,
        1,
        "import_model",
        json!({ "name": "plate", "path": stl.display().to_string() }),
    );
    let full = ok(&mut s, 2, "list_features", json!({ "model": "plate" }));
    let short = ok(
        &mut s,
        3,
        "list_features",
        json!({ "model": "plate", "compact": true }),
    );
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(short["compact"], json!(true));
    let face = &short["faces"][0];
    assert!(face["id"].is_string(), "{face}");
    assert!(
        face.get("loops").is_none() && face.get("bounds").is_none(),
        "{face}"
    );
    assert!(face["loops_count"].is_number(), "{face}");
    let chain = &short["chains"][0];
    assert!(
        chain["id"].is_string() && chain.get("bounds").is_none(),
        "{chain}"
    );
    assert_eq!(
        short["faces_total"], full["faces_total"],
        "totals stay whole"
    );

    // The chat default: 25 rows per kind, and `truncated` says so.
    let rows = short["chains"].as_array().map_or(0, Vec::len);
    let total = short["chains_total"].as_u64().unwrap_or(0) as usize;
    assert_eq!(rows, total.min(25), "{rows} of {total}");
    assert!(
        total > 25,
        "the fixture has more chains than one compact page: {total}"
    );
    assert_eq!(short["truncated"], json!(true));

    // The budget: the compact listing fits 12000 characters.
    assert!(
        chars(&short) <= 12_000,
        "compact is {} characters",
        chars(&short)
    );
    assert!(
        chars(&short) * 2 < chars(&full),
        "compact {} vs full {}",
        chars(&short),
        chars(&full)
    );
}

#[test]
fn list_features_retains_dxf_listings_and_compact_options() {
    let tools = provider::tools_in(Provider::Anthropic, ToolSet::Core);
    let features = tools
        .iter()
        .find(|t| t["name"] == json!("list_features"))
        .expect("list_features");
    let properties = &features["input_schema"]["properties"];
    let kinds = properties["kinds"]["items"]["enum"]
        .as_array()
        .expect("feature kinds");
    assert!(
        kinds.contains(&json!("layers")),
        "DXF layers must remain discoverable: {properties}"
    );
    assert_eq!(properties["compact"]["type"], json!("boolean"));

    let dir = temp_dir("dxf-features");
    let path = dir.join("profile.dxf");
    std::fs::write(
        &path,
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n2\n10\n0\n20\n2\n\
         0\nLINE\n8\nmarks\n10\n2\n20\n0.5\n11\n3\n21\n0.5\n0\nENDSEC\n0\nEOF\n",
    )
    .expect("write dxf");
    let mut s = Server::new();
    ok(
        &mut s,
        1,
        "set_stock",
        json!({ "box": { "dx": 20, "dy": 20, "dz": 5 } }),
    );
    ok(
        &mut s,
        2,
        "import_dxf",
        json!({ "path": path.display().to_string() }),
    );
    let all = ok(&mut s, 3, "list_features", json!({ "model": "profile" }));
    let layers = ok(
        &mut s,
        4,
        "list_features",
        json!({ "model": "profile", "kinds": ["layers"] }),
    );
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(all["kind"], json!("sketch"));
    assert_eq!(all["chains_total"], json!(2));
    assert_eq!(all["layers_total"], json!(2));
    let chains = all["chains"].as_array().expect("DXF chains");
    assert_eq!(chains.len(), 2);
    assert!(chains.iter().all(|c| c["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("sketch_chain:"))));
    assert!(chains
        .iter()
        .any(|c| c["layer"] == json!("cut") && c["closed"] == json!(true)));
    assert_eq!(layers["layers"], all["layers"]);
    assert!(
        layers.get("chains").is_none(),
        "the DXF kind filter remains effective: {layers}"
    );
    assert!(layers["layers"]
        .as_array()
        .expect("DXF layers")
        .iter()
        .any(|l| l["id"] == json!("sketch_layer:marks")));
}

#[test]
fn the_core_tool_set_is_a_small_subset_of_the_full_table() {
    let full = provider::tools_in(Provider::Anthropic, ToolSet::Full);
    let core = provider::tools_in(Provider::Anthropic, ToolSet::Core);
    assert_eq!(
        full,
        provider::tools(Provider::Anthropic),
        "Full is today's table"
    );
    assert!(
        (20..=25).contains(&core.len()),
        "core has {} tools",
        core.len()
    );
    let names: Vec<&str> = full.iter().filter_map(|t| t["name"].as_str()).collect();
    for t in &core {
        let name = t["name"].as_str().expect("name");
        assert!(names.contains(&name), "{name} is not a live tool");
    }
    for must in [
        "get_job",
        "add_operation",
        "generate",
        "verify",
        "post",
        "undo",
    ] {
        assert!(
            core.iter().any(|t| t["name"] == json!(must)),
            "core lacks {must}"
        );
    }
    // The budget: the core schemas are under a third of the full table.
    let size = |v: &Vec<Value>| serde_json::to_string(v).map_or(0, |s| s.len());
    assert!(
        size(&core) * 3 < size(&full),
        "core {} vs full {}",
        size(&core),
        size(&full)
    );
    // Gemini wraps its declarations: the core set does the same.
    let gemini = provider::tools_in(Provider::Gemini, ToolSet::Core);
    assert_eq!(
        gemini[0]["functionDeclarations"].as_array().map(Vec::len),
        Some(core.len())
    );
}

#[test]
fn tool_advertisements_keep_import_fidelity_guidance_and_job_summary() {
    let tools = provider::tools_in(Provider::Anthropic, ToolSet::Core);
    let importer = tools
        .iter()
        .find(|t| t["name"] == json!("import_model"))
        .expect("import_model");
    let description = importer["description"]
        .as_str()
        .expect("import description");
    assert!(
        description.contains("edges lie off their surfaces"),
        "{description}"
    );
    assert!(description.contains("tighter tolerance"), "{description}");

    let job = tools
        .iter()
        .find(|t| t["name"] == json!("get_job"))
        .expect("get_job");
    assert_eq!(
        job["input_schema"]["properties"]["summary"]["type"],
        json!("boolean")
    );
}

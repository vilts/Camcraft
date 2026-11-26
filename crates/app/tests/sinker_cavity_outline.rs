//! Provenance: synthetic cavity and EDM condition for the outline size law (#1810).
//! Gate map (#1810): covers app::mcp::job, app::mcp::sinker_tools,
//! app::project::sinker_dto and app::operation::types::sinker.

mod pocket_support;

use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

fn call(server: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
    *id += 1;
    let reply = server
        .handle(&json!({
            "jsonrpc": "2.0", "id": *id, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        }))
        .expect("a request gets a response");
    let result = &reply["result"];
    if result["isError"] == true {
        Err(result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned())
    } else {
        Ok(result["structuredContent"].clone())
    }
}

#[test]
fn mcp_exposes_the_cavity_outline_and_burns_a_diamond_finish() {
    let (mut server, mut id) = (Server::new(), 0);
    let _ =
        server.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let kinds =
        call(&mut server, &mut id, "list_operation_types", json!({})).expect("operation types");
    let burn = kinds["kinds"]
        .as_array()
        .expect("kinds")
        .iter()
        .find(|k| k["key"] == "sinker_burn")
        .expect("Sinker Burn");
    assert!(
        burn["fields"]
            .as_array()
            .expect("fields")
            .iter()
            .any(|f| f["name"] == "cavity_outline"
                && f["type"] == "array"
                && f["default"].is_null()),
        "{burn}"
    );

    let dir = std::env::temp_dir().join(format!("camcraft-1810-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    let stl = dir.join("diamond.stl");
    let mch = dir.join("sinker.mch");
    let ring = [(10.0, 0.0), (0.0, 6.0), (-10.0, 0.0), (0.0, -6.0)];
    let centered = pocket_support::block_with_pocket(30.0, 30.0, &ring, 14.0, 20.0);
    let shifted = |p: Point3| Point3::new(p.x + 6.0, p.y - 4.0, p.z);
    let mesh = Mesh::from_facets(
        centered
            .facets
            .iter()
            .map(|t| Triangle::new(shifted(t.a), shifted(t.b), shifted(t.c)))
            .collect(),
    );
    pocket_support::write_stl(&mesh, &stl);
    let mut machine: Value =
        serde_json::from_str(include_str!("edm/sinker-edm.mch")).expect("machine template");
    machine["edm"]["conditions"] = json!([{ "id": "F1", "name": "synthetic finish", "gap": 0.02, "undersize": 0.02, "role": "finish" }]);
    std::fs::write(&mch, machine.to_string()).expect("machine fixture");
    call(
        &mut server,
        &mut id,
        "import_model",
        json!({ "path": stl.display().to_string() }),
    )
    .expect("import");
    call(
        &mut server,
        &mut id,
        "set_stock",
        json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }),
    )
    .expect("stock");
    call(
        &mut server,
        &mut id,
        "load_machine",
        json!({ "path": mch.display().to_string() }),
    )
    .expect("machine");
    let _ = std::fs::remove_dir_all(&dir);
    let rows = call(
        &mut server,
        &mut id,
        "list_features",
        json!({ "model": "diamond", "kinds": ["faces"] }),
    )
    .expect("faces");
    let picks: Vec<Value> = rows["faces"]
        .as_array()
        .expect("face list")
        .iter()
        .filter(|face| {
            let b = &face["bounds"];
            let n = |key: &str, i: usize| b[key][i].as_f64().unwrap_or(f64::NAN);
            n("min", 0) >= -4.000001
                && n("max", 0) <= 16.000001
                && n("min", 1) >= -10.000001
                && n("max", 1) <= 2.000001
                && n("min", 2) >= 13.999999
        })
        .map(|f| json!({ "model": "diamond", "id": f["id"] }))
        .collect();
    assert_eq!(picks.len(), 5);
    let generated = call(
        &mut server,
        &mut id,
        "electrode_from_faces",
        json!({
            "name": "E-F", "picks": picks, "condition": "F1", "stub": 10, "pickup": 15
        }),
    )
    .expect("diamond electrode");
    let outline = generated["burn"]["cavity_outline"]
        .as_array()
        .expect("the cavity outline in burn parameters");
    assert_eq!(outline.len(), 4, "{generated}");
    for p in [[10.0, 0.0], [0.0, 6.0], [-10.0, 0.0], [0.0, -6.0]] {
        assert!(
            outline
                .iter()
                .any(|q| (q[0].as_f64().expect("x") - p[0]).abs() < 1e-6
                    && (q[1].as_f64().expect("y") - p[1]).abs() < 1e-6),
            "{outline:?}"
        );
    }
    let mut params = generated["burn"].clone();
    params["condition"] = json!("F1");
    params["pass"] = json!("finish");
    let mut wrong_size = params.clone();
    wrong_size["cavity_x"] = json!(18);
    let why = call(&mut server, &mut id, "add_operation", json!({
        "kind": "Sinker Burn", "name": "Bad size", "tool": "E-F", "source": "stock", "params": wrong_size
    })).expect_err("the cavity outline must match its stated size at command creation");
    assert!(
        why.contains("cavity") && why.contains("stated size"),
        "{why}"
    );
    let mut bad = params.clone();
    bad["cavity_outline"] = json!([[0, 0], [1, 0]]);
    let why = call(&mut server, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "name": "Bad", "tool": "E-F", "source": "stock", "params": bad })).expect_err("malformed outline fails at command creation");
    assert!(
        why.contains("cavity outline") && why.contains("at least 3"),
        "{why}"
    );
    call(&mut server, &mut id, "add_operation", json!({ "kind": "Sinker Burn", "name": "Finish", "tool": "E-F", "source": "stock", "params": params })).expect("finish operation");
    let result = call(
        &mut server,
        &mut id,
        "generate",
        json!({ "name": "Finish" }),
    )
    .expect("finish generates");
    assert!(
        result["generated"]
            .as_array()
            .is_some_and(|ops| !ops.is_empty()),
        "{result}"
    );
    let cleared = call(
        &mut server,
        &mut id,
        "edit_operation",
        json!({ "name": "Finish", "params": { "cavity_outline": null } }),
    )
    .expect("the declared default clears the outline");
    assert!(cleared["params"]["sinker_burn"].is_object(), "{cleared}");
    assert!(
        cleared["params"]["sinker_burn"]
            .get("cavity_outline")
            .is_none(),
        "{cleared}"
    );
    let restored = call(&mut server, &mut id, "edit_operation", json!({ "name": "Finish", "params": { "cavity_outline": generated["burn"]["cavity_outline"] } })).expect("the outline can be restored");
    assert_eq!(
        restored["params"]["sinker_burn"]["cavity_outline"],
        generated["burn"]["cavity_outline"]
    );
    let why = call(
        &mut server,
        &mut id,
        "edit_operation",
        json!({ "name": "Finish", "params": { "cavity_outline": [[0, 0], [1, 0]] } }),
    )
    .expect_err("malformed outline edit is refused");
    assert!(
        why.contains("cavity outline") && why.contains("at least 3"),
        "{why}"
    );
}

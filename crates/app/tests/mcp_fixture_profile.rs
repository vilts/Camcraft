//! #744 MCP integration tests for DXF/profile fixtures and STEP preservation.
//! Gate map (M4.4): covers app::mcp::fixture_profile_tools,
//! app::commands::profile_fixture, app::commands::import_model.
//! Provenance: clean-room test over our own MCP dispatcher and schema validator.

use std::path::Path;

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

fn call(srv: &mut Server, tool: &str, args: Value) -> Result<Value, String> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    });
    let v = srv.handle(&req).expect("mcp response");
    if let Some(err) = v.get("error") {
        return Err(err.to_string());
    }
    let res = &v["result"];
    let text = res["content"][0]["text"].as_str().unwrap_or("");
    if res.get("isError").and_then(Value::as_bool) == Some(true) {
        Err(text.to_owned())
    } else {
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
    }
}

fn spun_dxf() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n\
     0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n30\n21\n0\n\
     0\nLINE\n8\nPROFILE\n10\n0\n20\n0\n11\n0\n21\n25\n\
     0\nLINE\n8\nPROFILE\n10\n0\n20\n25\n11\n20\n21\n25\n\
     0\nLINE\n8\nPROFILE\n10\n20\n20\n25\n11\n20\n21\n0\n\
     0\nENDSEC\n0\nEOF\n"
}

#[test]
fn mcp_fixture_profile_workflow_imports_edits_exports_and_keeps_step() {
    let mut srv = Server::new();

    // Import spun DXF fixture, edit it, export DXF back.
    let imp = call(
        &mut srv,
        "import_dxf_fixture",
        json!({ "kind": "spun", "name": "Jaw", "dxf": spun_dxf() }),
    )
    .expect("import spun DXF");
    assert_eq!(imp["fixture"], "Jaw");
    assert_eq!(imp["has_step_source"], false);
    assert_eq!(imp["profile"]["kind"], "spun");

    let edited = call(
        &mut srv,
        "set_fixture_profile",
        json!({
            "name": "Jaw",
            "profile": {
                "kind": "spun",
                "points": [
                    { "z": 0.0, "r": 0.0 },
                    { "z": 0.0, "r": 18.0 },
                    { "z": 20.0, "r": 18.0 },
                    { "z": 20.0, "r": 0.0 }
                ]
            }
        }),
    )
    .expect("edit profile");
    assert_eq!(edited["profile"]["points"][1]["r"], 18.0);

    let dxf = call(&mut srv, "export_fixture_dxf", json!({ "name": "Jaw" })).expect("export dxf");
    assert!(dxf["dxf"].as_str().unwrap_or("").contains("CENTRELINE"));

    // Import STEP model -> tag as fixture -> derive profile -> STEP source stays & set_fixture_profile is refused.
    let step_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    call(&mut srv, "import_model", json!({ "path": step_path, "name": "Block" })).expect("import STEP");
    call(&mut srv, "add_fixture", json!({ "model": "Block" })).expect("tag fixture");

    let before = call(&mut srv, "get_fixture_profile", json!({ "name": "Block" })).expect("get before");
    assert_eq!(before["has_step_source"], true);
    assert!(before["profile"].is_null());

    let derived = call(
        &mut srv,
        "derive_fixture_profile",
        json!({ "name": "Block", "kind": "extruded", "axis": "z" }),
    )
    .expect("derive");
    assert_eq!(derived["has_step_source"], true);
    assert_eq!(derived["profile"]["kind"], "extruded");

    let err = call(
        &mut srv,
        "set_fixture_profile",
        json!({
            "name": "Block",
            "profile": {
                "kind": "spun",
                "points": [{ "z": 0.0, "r": 0.0 }, { "z": 0.0, "r": 10.0 }, { "z": 10.0, "r": 0.0 }]
            }
        }),
    )
    .expect_err("must refuse overwriting STEP fixture");
    assert!(err.contains("STEP"), "{err}");
}

/// PR #2101 review P1: `derive_fixture_profile` must not replace the
/// generating profile of a DXF/profile fixture. That replaced its curved
/// chord bound with a bulge-free polygon and cleared a real collision.
#[test]
fn mcp_derive_fixture_profile_refuses_generated_fixture_and_keeps_its_profile() {
    let mut srv = Server::new();
    let imp = call(
        &mut srv,
        "import_dxf_fixture",
        json!({ "kind": "spun", "name": "Jaw", "dxf": spun_dxf() }),
    )
    .expect("import spun DXF");
    let profile = imp["profile"].clone();
    assert_eq!(profile["kind"], "spun");

    for args in [
        json!({ "name": "Jaw", "kind": "spun" }),
        json!({ "name": "Jaw", "kind": "extruded", "axis": "z" }),
        json!({ "name": "Jaw", "kind": "extruded", "axis": "x" }),
    ] {
        let err = call(&mut srv, "derive_fixture_profile", args).expect_err("derive is refused");
        assert!(err.contains("'Jaw'") && err.contains("generated from its own profile"), "{err}");
        let now = call(&mut srv, "get_fixture_profile", json!({ "name": "Jaw" })).expect("get");
        assert_eq!(now["profile"], profile, "a refused derive leaves the profile");
    }

    // The import is the only history step: a refusal adds none.
    call(&mut srv, "undo", json!({})).expect("undo the import");
    call(&mut srv, "get_fixture_profile", json!({ "name": "Jaw" })).expect_err("fixture gone");
    call(&mut srv, "undo", json!({})).expect_err("nothing else to undo");
    call(&mut srv, "redo", json!({})).expect("redo the import");
    let back = call(&mut srv, "get_fixture_profile", json!({ "name": "Jaw" })).expect("get");
    assert_eq!(back["profile"], profile);
    call(&mut srv, "derive_fixture_profile", json!({ "name": "Jaw", "kind": "spun" }))
        .expect_err("still refused after redo");
}

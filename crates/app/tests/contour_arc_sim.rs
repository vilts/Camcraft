//! #402: a clean 2D Contour around an L-shaped block must post. The
//! program sim's column engine used to cut each XY arc as chords drawn
//! INSIDE the arc; on a convex corner (arc centre = the part's corner)
//! that brought the simulated tool up to one sagitta closer to the part
//! than the real G2/G3, and one lattice column 0.088 mm inside the
//! riser's corner was clipped - read by the height verdict as a 10 mm
//! gouge, so the post gate refused a correct program. The arc sweep is
//! now exact (`stock_sim::dexel_arc`); this file is the issue's own
//! repro, driven over the MCP tools the way it was reported (the
//! planted gouges, convex and concave, live with the kernel in
//! `crates/sim/tests/arc_sweep.rs`).
//!
//! The part is our own geometry (the issue's XZ profile extruded
//! y -30..30), written as an STL by the test so the fixture is the code.
//!
//! Gate map (M4.4): covers app::mcp, app::posting, app::posting_sim,
//! app::stock_sim, sim::stock_sim, motion::machining2d.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-402-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// The issue's L-block: in XZ (-40,0) (40,0) (40,15) (10,15) (10,25)
/// (-40,25), extruded y -30..30 - a 25 mm riser over x -40..10 and a
/// step at z 15 over x 10..40. ASCII STL, wound outward.
fn write_l_block(path: &Path) {
    let prof = [(-40.0, 0.0), (40.0, 0.0), (40.0, 15.0), (10.0, 15.0), (10.0, 25.0), (-40.0, 25.0)];
    let at = |i: usize, y: f64| -> [f64; 3] { [prof[i].0, y, prof[i].1] };
    let (yf, yb) = (-30.0, 30.0);
    let mut tris: Vec<[[f64; 3]; 3]> = Vec::new();
    // Caps: the profile is star-shaped from vertex 0, CCW seen from -y.
    for i in 1..prof.len() - 1 {
        tris.push([at(0, yf), at(i, yf), at(i + 1, yf)]);
        tris.push([at(0, yb), at(i + 1, yb), at(i, yb)]);
    }
    // Walls: one quad per profile edge, normal pointing out of the CCW loop.
    for i in 0..prof.len() {
        let j = (i + 1) % prof.len();
        tris.push([at(i, yf), at(j, yb), at(j, yf)]);
        tris.push([at(i, yf), at(i, yb), at(j, yb)]);
    }
    let mut s = String::from("solid lblock\n");
    for t in tris {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for v in t {
            s.push_str(&format!("   vertex {} {} {}\n", v[0], v[1], v[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid lblock\n");
    std::fs::write(path, s).expect("write stl");
}

fn call(server: &mut Server, id: &mut i64, name: &str, args: Value) -> (bool, Value) {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

fn ok(server: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    let (err, body) = call(server, id, name, args);
    assert!(!err, "{name} refused: {body}");
    body
}

/// The issue's job, step for step: L-block, 90x70x28 box stock, a 6 mm
/// end mill, a default 2D Contour around the model, generated and
/// verified. Returns the verify report.
fn contour_job(tag: &str) -> Value {
    let dir = tmp_dir(tag);
    let stl = dir.join("lblock.stl");
    write_l_block(&stl);
    let (mut s, mut id) = (Server::new(), 0);
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" } });
    let _ = s.handle(&init);
    ok(&mut s, &mut id, "import_model", json!({ "path": stl.display().to_string(), "name": "StepBlock" }));
    ok(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 90.0, "dy": 70.0, "dz": 28.0 } }));
    ok(&mut s, &mut id, "add_tool", json!({ "catalog": "EM-0600-4FL" }));
    let add = json!({ "kind": "2D Contour", "tool": "EM-0600-4FL", "name": "Contour1", "source": "StepBlock" });
    ok(&mut s, &mut id, "add_operation", add);
    let generated = ok(&mut s, &mut id, "generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = ok(&mut s, &mut id, "verify", json!({}));
    let _ = std::fs::remove_dir_all(&dir);
    ver
}

fn post_gate_findings(ver: &Value) -> Vec<String> {
    ver["findings"]
        .as_array()
        .map(|f| {
            f.iter()
                .filter(|f| f["kind"] == json!("post_gate"))
                .map(|f| f["message"].as_str().unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_clean_contour_around_the_l_block_reads_no_gouge() {
    // The issue's exact job. The one legitimate finding is the rapid
    // below the clearance plane (top of model 25 under stock top 28,
    // nothing faced it); the post gate must carry no gouge.
    let ver = contour_job("clean");
    let gates = post_gate_findings(&ver);
    assert!(
        gates.iter().all(|m| !m.contains("gouge")),
        "a correct contour reads as a gouge: {gates:?}\n{ver}"
    );
}

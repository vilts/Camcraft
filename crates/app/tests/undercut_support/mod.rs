//! Shared fixtures of the #1444 Slot Undercut MCP suites
//! (`mcp_slot_undercut`, `mcp_undercut_fixture`): the T-slot block and
//! box STL writers and an in-process MCP client. Not a suite itself.
//! Provenance: our own fixtures, written as STL by the tests.

#![allow(dead_code)]

use camcraft_app::mcp::Server;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-undercut-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A T-slot block as ASCII STL: x -30..30, y -20..20, z 0..20, with a
/// slot along X - a 9 mm neck from the top down to Z 14 and a 20 mm
/// undercut from Z 14 down to Z 8. Built on a grid of the slot's own
/// breaks, one quad per exposed cell face, wound outward: every edge is
/// shared by exactly two facets.
pub fn write_tslot_stl(path: &Path) {
    let xs = [-30.0, 30.0];
    let ys = [-20.0, -10.0, -4.5, 4.5, 10.0, 20.0];
    let zs = [0.0, 8.0, 14.0, 20.0];
    // Material by cell (y index, z index): the slot's cells are air.
    let solid = |j: i32, k: i32| -> bool {
        if !(0..5).contains(&j) || !(0..3).contains(&k) {
            return false;
        }
        match k {
            0 => true,
            1 => j == 0 || j == 4,
            _ => j != 2,
        }
    };
    let mut s = String::from("solid tslot\n");
    let mut quad = |q: [[f64; 3]; 4]| {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for p in tri {
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    };
    for j in 0..5i32 {
        for k in 0..3i32 {
            if !solid(j, k) {
                continue;
            }
            let (y0, y1) = (ys[j as usize], ys[j as usize + 1]);
            let (z0, z1) = (zs[k as usize], zs[k as usize + 1]);
            let (x0, x1) = (xs[0], xs[1]);
            // The two end caps (the block has one cell along X).
            quad([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]]);
            quad([[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]]);
            if !solid(j - 1, k) {
                quad([[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]]);
            }
            if !solid(j + 1, k) {
                quad([[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]]);
            }
            if !solid(j, k - 1) {
                quad([[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]]);
            }
            if !solid(j, k + 1) {
                quad([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]);
            }
        }
    }
    s.push_str("endsolid tslot\n");
    std::fs::write(path, s).expect("write stl");
}

pub struct Client {
    server: Server,
    next: i64,
}

impl Client {
    pub fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    pub fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    pub fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    pub fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

pub fn write_box_stl(path: &Path, lo: [f64; 3], hi: [f64; 3]) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let quads = [
        [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
    ];
    let mut s = String::from("solid box\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for p in tri {
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write box stl");
}

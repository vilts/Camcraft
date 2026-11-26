//! Shared models and MCP client for the protection tests.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-2055-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// Write a box STL with min (x0, y0, z0) and max (x1, y1, z1).
pub fn write_box_stl(path: &std::path::Path, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) {
    let v = |i: usize| -> [f64; 3] {
        [
            if i & 1 == 0 { x0 } else { x1 },
            if i & 2 == 0 { y0 } else { y1 },
            if i & 4 == 0 { z0 } else { z1 },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut s = String::from("solid box\n");
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let p = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write stl");
}

pub struct Client {
    server: Server,
    next: i64,
}

impl Client {
    pub fn new() -> Self {
        Self {
            server: Server::new(),
            next: 1,
        }
    }

    fn call(&mut self, name: &str, args: &Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let reply = self.server.handle(&msg).expect("request answered");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err {
            result["content"][0]["text"].clone()
        } else {
            result["structuredContent"].clone()
        };
        (is_err, body)
    }

    pub fn ok(&mut self, name: &str, args: Value) -> Value {
        let (is_err, body) = self.call(name, &args);
        assert!(!is_err, "{name}({args}) failed unexpectedly: {body}");
        body
    }

    pub fn refused(&mut self, name: &str, args: Value) -> String {
        let (is_err, body) = self.call(name, &args);
        assert!(is_err, "{name}({args}) should be refused, got: {body}");
        body.as_str().expect("refusal message").to_owned()
    }
}

// Each test binary that includes this module uses one of the two setups.
#[allow(dead_code)]
pub fn setup_plate_and_tower(tag: &str) -> (PathBuf, Client, String, String, String, String) {
    setup_plate_and_tower_on(tag, [-40.0, -40.0, 40.0, 40.0])
}

/// [`setup_plate_and_tower`] with the plate's XY bounds `[x0, y0, x1, y1]`
/// (mm). The tower stays at the origin; the stock box is centred there and
/// covers the plate.
pub fn setup_plate_and_tower_on(tag: &str, plate: [f64; 4]) -> (PathBuf, Client, String, String, String, String) {
    let dir = tmp_dir(tag);
    let plate_stl = dir.join("plate.stl");
    let tower_stl = dir.join("tower.stl");
    let [x0, y0, x1, y1] = plate;
    write_box_stl(&plate_stl, x0, y0, 0.0, x1, y1, 10.0);
    let (dx, dy) = (2.0 * x0.abs().max(x1.abs()), 2.0 * y0.abs().max(y1.abs()));
    write_box_stl(&tower_stl, -6.0, -6.0, 0.0, 6.0, 6.0, 24.0);

    let mut c = Client::new();
    c.ok(
        "import_model",
        json!({ "path": plate_stl.display().to_string(), "name": "plate" }),
    );
    c.ok(
        "import_model",
        json!({ "path": tower_stl.display().to_string(), "name": "tower" }),
    );
    c.ok(
        "set_stock",
        json!({ "box": { "dx": dx, "dy": dy, "dz": 24.0 } }),
    );
    c.ok(
        "add_tool",
        json!({ "catalog": "EM-0400-2FL", "name": "SteppedMill" }),
    );
    c.ok(
        "edit_tool",
        json!({
            "name": "SteppedMill",
            "patch": {
                "diameter": 6.0,
                "flute_len": 15.0,
                "shank_d": 12.0,
                "shank_len": 12.0,
                "holder_d": 28.0,
                "holder_len": 20.0,
                "center_cutting": true
            }
        }),
    );

    let plate_feats = c.ok(
        "list_features",
        json!({ "model": "plate", "kinds": ["faces"] }),
    );
    let plate_top = plate_feats["faces"]
        .as_array()
        .expect("plate faces")
        .iter()
        .find(|f| {
            f["bounds"]["min"][2]
                .as_f64()
                .is_some_and(|z| (z - 10.0).abs() < 1e-3)
        })
        .expect("plate top face")["id"]
        .as_str()
        .expect("face id")
        .to_owned();

    let tower_feats = c.ok(
        "list_features",
        json!({ "model": "tower", "kinds": ["faces", "chains", "vertices"] }),
    );
    let tower_top_face = tower_feats["faces"]
        .as_array()
        .expect("tower faces")
        .iter()
        .find(|f| {
            f["bounds"]["min"][2]
                .as_f64()
                .is_some_and(|z| (z - 24.0).abs() < 1e-3)
        })
        .expect("tower top face")["id"]
        .as_str()
        .expect("face id")
        .to_owned();
    let tower_edge = tower_feats["chains"][0]["id"]
        .as_str()
        .expect("chain id")
        .to_owned();
    let tower_vert = tower_feats["vertices"][0]["id"]
        .as_str()
        .expect("vertex id")
        .to_owned();

    (dir, c, plate_top, tower_top_face, tower_edge, tower_vert)
}

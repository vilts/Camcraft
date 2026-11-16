//! #1407 / #1715: the as-imported `sample_ap214.step` (cylinder axis
//! horizontal, box stock from the model) still generates its 3D Finish S1
//! and verifies with no gouge. The one whole-span test took about 28 min
//! in release, so the span is cut into `BANDS` level bands: each test
//! generates S1 over one band (top and bottom heights from the model) and
//! verifies it. The bands tile the model's whole Z span, so together they
//! run every level the whole-span program ran, and libtest runs them in
//! parallel. Sibling of `shared_face_bodies.rs` (its other tests stay
//! there).
//!
//! Gate map: covers app::operation::material (from_slice), app::mcp's
//! generate / verify, through camcraft_geometry::geometry::slice's union
//! fallback and camcraft_motion::machining3d.
//!
//! Provenance: the issue's own MCP reproduction (#1407), cut into bands
//! as issue #1715 asks.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};

/// The number of level bands. Six is the release gate's test threads.
const BANDS: u32 = 6;

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Client { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = reply["result"].clone();
        (result["isError"] == json!(true), result)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, result) = self.call(name, args);
        assert!(!err, "{name} refused: {result}");
        result["structuredContent"].clone()
    }
}

fn model(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// The as-imported job of `shared_face_bodies.rs::sample_job(false)`: the
/// sample, stock from the model, a ball 3D Finish S1 and a flat 3D Pocket
/// R1 (kept so the program is the same; R1 is never generated here).
fn sample_job() -> Client {
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": model("sample_ap214.step") }));
    c.ok("set_stock", json!({ "from_model": "sample_ap214" }));
    c.ok("add_tool", json!({ "catalog": "BN-0600" }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok("add_operation", json!({ "kind": "pocket3d", "tool": "EM-0600-4FL", "name": "R1", "source": "sample_ap214" }));
    c.ok("add_operation", json!({ "kind": "finish3d", "tool": "BN-0600", "name": "S1", "source": "sample_ap214" }));
    c
}

fn statuses(generated: &Value) -> Vec<(String, String, String)> {
    generated["generated"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|g| {
            let s = |k: &str| g[k].as_str().unwrap_or_default().to_owned();
            (s("name"), s("status"), g.to_string())
        })
        .collect()
}

fn gate_problems(v: &Value) -> Vec<String> {
    v["gate"]["problems"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// The heights of band `k` (0 is the top band): offsets down from the
/// model's top, `span / BANDS` deep. The last band ends on the model's
/// bottom itself, so no level below the last cut is lost to rounding.
/// Adjacent bands share their boundary level: the bands overlap there,
/// they never leave a gap.
fn band_heights(k: u32) -> Value {
    let m = camcraft_app::geometry::import::load_model(std::path::Path::new(&model("sample_ap214.step")))
        .expect("the sample imports");
    let b = m.bounds().expect("bounds");
    let step = (b.max.z - b.min.z) / f64::from(BANDS);
    let top = json!({ "reference": "top_of_model", "offset": -step * f64::from(k) });
    let bottom = if k + 1 >= BANDS {
        json!({ "reference": "bottom_of_model", "offset": 0.0 })
    } else {
        json!({ "reference": "top_of_model", "offset": -step * f64::from(k + 1) })
    };
    json!({ "top_z": top, "bottom_z": bottom })
}

/// The old whole-span test's asserts, over band `k`: S1 alone generates
/// ok, and the verify gate names no gouge. #1840: R1 is never generated,
/// so the gate is skipped and its problem list is empty; the assert is
/// kept exactly as the old test had it until #1840 makes it real.
fn the_3d_finish_band_generates_with_no_gouge(k: u32) {
    let mut c = sample_job();
    c.ok("edit_operation", json!({ "name": "S1", "params": band_heights(k) }));
    let generated = c.ok("generate", json!({ "name": "S1" }));
    let st = statuses(&generated);
    assert_eq!(st.len(), 1, "band {k}: {generated}");
    for (name, status, row) in st {
        assert_eq!(status, "ok", "band {k}: {name}: {row}");
    }
    let v = c.ok("verify", json!({}));
    assert!(gate_problems(&v).iter().all(|p| !p.starts_with("gouge")), "band {k}: {v}");
}

macro_rules! band {
    ($name:ident, $k:expr) => {
        #[test]
        #[ignore = "heavy lane (#1407): 3D Finish over one sixth of the as-imported sample's 118 mm Z span - run at the cut with -- --ignored"]
        fn $name() {
            the_3d_finish_band_generates_with_no_gouge($k);
        }
    };
}

band!(the_sample_as_imported_3d_finish_band_1_of_6_has_no_gouge, 0);
band!(the_sample_as_imported_3d_finish_band_2_of_6_has_no_gouge, 1);
band!(the_sample_as_imported_3d_finish_band_3_of_6_has_no_gouge, 2);
band!(the_sample_as_imported_3d_finish_band_4_of_6_has_no_gouge, 3);
band!(the_sample_as_imported_3d_finish_band_5_of_6_has_no_gouge, 4);
band!(the_sample_as_imported_3d_finish_band_6_of_6_has_no_gouge, 5);

/// The bands tile the model's span: band 1 starts at the model's top,
/// the last ends at its bottom, and each band starts where the one above
/// it ends. Cheap (one import), so it runs in the default suite.
#[test]
fn the_3d_finish_bands_tile_the_whole_span() {
    let first = band_heights(0);
    assert_eq!(first["top_z"], json!({ "reference": "top_of_model", "offset": -0.0 }), "{first}");
    let last = band_heights(BANDS - 1);
    assert_eq!(last["bottom_z"], json!({ "reference": "bottom_of_model", "offset": 0.0 }), "{last}");
    for k in 1..BANDS {
        assert_eq!(band_heights(k - 1)["bottom_z"], band_heights(k)["top_z"], "band {k} starts where band {} ends", k - 1);
    }
}

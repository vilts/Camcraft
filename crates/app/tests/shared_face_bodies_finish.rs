//! #1407 / #1715: the as-imported `sample_ap214.step` (cylinder axis
//! horizontal, box stock from the model) still generates its 3D Finish S1
//! and verifies with no gouge. The one whole-span test took about 28 min
//! in release, so the span is cut into `BANDS` level bands: each test
//! generates S1 over one band (top and bottom heights from the model) and
//! verifies it. The bands tile the model's whole Z span, and libtest runs
//! them in parallel. Each band's level ladder starts at its own top, so
//! levels can shift by less than one stepdown at a band boundary compared
//! with the whole-span program. Links between bands are not generated or
//! verified. Sibling of `shared_face_bodies.rs` (its other tests stay there).
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

/// Generates a 3D Finish (`BN-0600`, kind defaults) over `[top, bottom]`
/// on `Milling-test.step` in `wall_column_gouge.rs`'s 70 x 70 x 27 mm
/// box stock and returns its toolpath, scallop stepdown, gate report,
/// and gouge cell count.
fn milling_test_finish_band(
    top: f64,
    bottom: f64,
) -> (camcraft_app::schema::Toolpath, f64, camcraft_app::posting::GateReport, usize) {
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
    use camcraft_app::geometry::Point3;
    use camcraft_app::operation::{self, Height, HeightReference, KindInfo, OpParams, SrcRef};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let imp = ImportModel::new(std::path::Path::new(&model("Milling-test.step"))).expect("Milling-test.step imports");
    let part_name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("BN-0600");
    let tool_radius = tool.diameter / 2.0;
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("add tool"));

    let kind = (0..KindInfo::ALL.len())
        .find(|&i| KindInfo::ALL.get(i).is_some_and(|k| k.label == "3D Finish"))
        .expect("3D Finish kind");
    let seed = OpParams::Face {
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
    let mut params = operation::retype_params(&seed, kind);
    let tol = match &mut params {
        OpParams::Finish3D { top_z, bottom_z, tolerance, .. } => {
            *top_z = Height::fixed(top);
            *bottom_z = Height::fixed(bottom);
            *tolerance
        }
        other => panic!("expected Finish3D params, got {other:?}"),
    };
    let stepdown = camcraft_app::finishing3d::scallop_stepdown(tool_radius, tol);

    h.execute(&world, CreateOperation::new(&world, "Finish1", "BN-0600", SrcRef::Entity(part_name), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Finish1").expect("generate"));
    let op_e = camcraft_app::model::find_by_name(&world, "Finish1").expect("Finish1 entity");
    let tp = camcraft_app::model::toolpath_of(&world, &op_e).expect("toolpath");
    let gate = camcraft_app::posting::verification_report(&world);
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("simulate");
    (tp, stepdown, gate, sim.dev.gouge_cells)
}

fn flat_feed_zs(tp: &camcraft_app::cl::Toolpath) -> Vec<f64> {
    use camcraft_app::cl::MoveKind;
    let mut zs: Vec<f64> = Vec::new();
    for m in &tp.moves {
        if m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-6 {
            let z = m.to.z;
            if zs.last().is_none_or(|prev| (prev - z).abs() > 1e-6) {
                zs.push(z);
            }
        }
    }
    zs
}

/// #1853: splitting 3D Finish at a band edge misses two checks: the
/// continuous ladder's levels straddling the edge shift by less than
/// one stepdown relative to the split bands, and only a cross-band run
/// generates and verifies the link moves across the edge.
#[test]
fn the_3d_finish_band_split_levels_move_within_stepdown_and_links_cross_edge() {
    use camcraft_app::cl::MoveKind;

    let (top_z, edge_z, bottom_z) = (20.0, 18.0, 16.0);
    let (upper, stepdown, upper_gate, upper_gouges) = milling_test_finish_band(top_z, edge_z);
    let (lower, _, lower_gate, lower_gouges) = milling_test_finish_band(edge_z, bottom_z);
    let (whole, _, whole_gate, whole_gouges) = milling_test_finish_band(top_z, bottom_z);

    for (label, gate, gouges) in [
        ("upper", &upper_gate, upper_gouges),
        ("lower", &lower_gate, lower_gouges),
        ("whole", &whole_gate, whole_gouges),
    ] {
        assert!(gate.problems.is_empty(), "{label} gate problems: {:?}", gate.problems);
        assert!(gate.not_judged.is_none(), "{label} not_judged: {:?}", gate.not_judged);
        assert_eq!(gouges, 0, "{label} gouge cells: {gouges}");
    }
    assert!(whole.warnings.is_empty(), "whole warnings: {:?}", whole.warnings);

    // Split bands end/begin their ring ladders on `edge_z` and emit no cross-edge ring link.
    let uz = flat_feed_zs(&upper.path);
    let lz: Vec<f64> = flat_feed_zs(&lower.path).into_iter().filter(|&z| z <= edge_z + 1e-6).collect();
    let wz = flat_feed_zs(&whole.path);

    assert!((*uz.last().expect("upper levels") - edge_z).abs() < 1e-6, "upper levels: {uz:?}");
    assert!((*lz.first().expect("lower ring levels") - edge_z).abs() < 1e-6, "lower levels: {lz:?}");
    assert!(
        upper.path.moves.iter().all(|m| m.from.z >= edge_z - 1e-6 && m.to.z >= edge_z - 1e-6),
        "upper band must not move below band edge {edge_z}"
    );
    assert!(
        flat_feed_zs(&lower.path).into_iter().all(|z| z <= edge_z + 1e-6 || (z - top_z).abs() < 1e-6),
        "lower band rings must not start above band edge {edge_z}"
    );

    // Whole two-band span avoids landing on `edge_z`; levels near the edge move by < stepdown.
    assert!(
        wz.iter().all(|&z| (z - edge_z).abs() > 1e-3),
        "expected edge {edge_z} off whole-span ladder at stepdown {stepdown}: {wz:?}"
    );
    let cross_idx = wz
        .windows(2)
        .position(|w| w[0] > edge_z + 1e-6 && w[1] < edge_z - 1e-6)
        .expect("whole span must straddle band edge");
    let (z_above, z_below) = (wz[cross_idx], wz[cross_idx + 1]);
    assert!(z_above - edge_z > 1e-6 && z_above - edge_z < stepdown, "z_above={z_above} stepdown={stepdown}");
    assert!(edge_z - z_below > 1e-6 && edge_z - z_below < stepdown, "z_below={z_below} stepdown={stepdown}");
    assert!(z_above - z_below <= stepdown + 1e-6, "cross-edge step {z_above} -> {z_below} exceeds {stepdown}");

    for &split_z in lz.iter().filter(|&&z| z > bottom_z + 1e-6 && z < edge_z - 1e-6) {
        let nearest = wz
            .iter()
            .copied()
            .min_by(|a, b| (a - split_z).abs().total_cmp(&(b - split_z).abs()))
            .expect("whole level");
        let shift = (split_z - nearest).abs();
        assert!(shift > 1e-6 && shift < stepdown, "split level {split_z} shifted by {shift} vs stepdown {stepdown}");
    }

    // Whole span generates the Rapid retract -> [Rapid XY] -> Plunge link across `edge_z`.
    let moves = &whole.path.moves;
    let from_idx = moves
        .iter()
        .rposition(|m| m.kind == MoveKind::Feed && (m.to.z - z_above).abs() < 1e-6)
        .expect("cut at z_above");
    let to_idx = moves[from_idx + 1..]
        .iter()
        .position(|m| m.kind == MoveKind::Feed && (m.from.z - z_below).abs() < 1e-6)
        .map(|off| from_idx + 1 + off)
        .expect("cut at z_below after z_above");
    let link = &moves[from_idx + 1..to_idx];
    assert!(
        matches!(link.len(), 2 | 3)
            && link.first().is_some_and(|m| m.kind == MoveKind::Rapid && m.to.z > top_z)
            && link[..link.len() - 1].iter().all(|m| m.kind == MoveKind::Rapid)
            && (link[link.len() - 1].from.z - link[0].to.z).abs() < 1e-6
            && link.last().is_some_and(|m| m.kind == MoveKind::Plunge && (m.to.z - z_below).abs() < 1e-6),
        "expected clearance Rapid + Plunge link from {z_above} to {z_below}, got {link:?}"
    );
    for pair in moves[from_idx..=to_idx].windows(2) {
        assert!((pair[0].to.x - pair[1].from.x).abs() < 1e-9);
        assert!((pair[0].to.y - pair[1].from.y).abs() < 1e-9);
        assert!((pair[0].to.z - pair[1].from.z).abs() < 1e-9);
    }
}

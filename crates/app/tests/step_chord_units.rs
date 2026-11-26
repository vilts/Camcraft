//! #689: the STEP chord cap is 0.005 mm in MILLIMETRES, whatever length
//! unit the file states. The importer tessellates in the file's own unit
//! and scales to mm afterwards, so the cap must be expressed in that
//! unit - an inch file capped at "0.005" got 0.005 in = 0.127 mm chords
//! and kept the #689 false gouge (and its convex-wall twin).
//!
//! Gate map: covers app::geometry::import::step (chord target vs the
//! file's length unit, #306) through app::mcp and the program gate.
//!
//! Provenance: chord sagitta `r - |midpoint|` of a chord on a circle, from
//! elementary geometry; `Milling-test-inch.step` is `Milling-test.step`
//! with its unit declared as ISO 10303-41's conversion-based INCH and
//! every length divided by 25.4 (points, radii, vector magnitudes).

use camcraft_app::geometry::Mesh;
use camcraft_app::geometry::import;
use camcraft_app::mcp::Server;
use serde_json::{Value, json};

fn model(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// The worst chord error (mm) of the mesh on the vertical cylinder of
/// radius `r` about the Z axis: at the cylinder's top and bottom rims,
/// the mesh's vertices on that circle (within 1 um) sorted by angle, and
/// the largest sagitta `r (1 - cos(gap / 2))` between neighbours (a gap
/// over 0.5 rad is the wall's open side, not a chord) - how
/// far the wall's facets stand inside the true surface. `None`: fewer
/// than three rim vertices.
fn chord_error(m: &Mesh, r: f64) -> Option<f64> {
    let on = |x: f64, y: f64| (x.hypot(y) - r).abs() < 1e-3;
    let mut worst: Option<f64> = None;
    for z in [0.0, 20.0] {
        let mut angles: Vec<f64> = m
            .facets
            .iter()
            .flat_map(|t| [t.a, t.b, t.c])
            .filter(|p| (p.z - z).abs() < 1e-6 && on(p.x, p.y))
            .map(|p| p.y.atan2(p.x))
            .collect();
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        if angles.len() < 3 {
            continue;
        }
        // Every gap round the circle, the wrap included; on a partial
        // wall (the D's R30 arc) the one gap over 0.5 rad is its open
        // side, not a chord, and is dropped.
        let wrap = std::f64::consts::TAU - (angles[angles.len() - 1] - angles[0]);
        let mut gaps: Vec<f64> = angles.windows(2).map(|w| w[1] - w[0]).chain([wrap]).collect();
        gaps.sort_by(f64::total_cmp);
        if gaps.last().is_some_and(|g| *g > 0.5) {
            gaps.pop();
        }
        let gap = gaps.last().copied().unwrap_or(0.0);
        let sag = r * (1.0 - (gap * 0.5).cos());
        worst = Some(worst.map_or(sag, |w: f64| w.max(sag)));
    }
    worst
}

#[test]
fn an_inch_step_meshes_to_the_same_millimetre_chord_cap_as_a_millimetre_one() {
    let mm = import::load_model(&model("Milling-test.step")).expect("the mm part imports");
    let inch = import::load_model(&model("Milling-test-inch.step")).expect("the inch part imports");
    // Same part, same size in mm: the inch file scales to the mm one.
    let (bm, bi) = (mm.bounds().expect("bounds"), inch.bounds().expect("bounds"));
    assert!((bm.max.x - bi.max.x).abs() < 0.01 && (bm.max.z - bi.max.z).abs() < 1e-6, "{bm:?} vs {bi:?}");
    for (name, m) in [("mm", &mm), ("inch", &inch)] {
        for r in [12.5, 30.0] {
            let e = chord_error(m, r).expect("the wall has chords");
            eprintln!("{name}: r {r} chord error {e:.5} mm, {} facets", m.facets.len());
            assert!(e <= 0.005 + 1e-6, "{name} part: r = {r} wall's chords stand {e} mm inside it");
        }
    }
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result["isError"] != json!(true), "{name} refused: {reply}");
        result["structuredContent"].clone()
    }
}

/// The first `holes` array anywhere in `v`.
fn find_holes(v: &Value) -> Option<Value> {
    match v {
        Value::Object(m) => m.get("holes").filter(|h| h.is_array()).cloned().or_else(|| m.values().find_map(find_holes)),
        Value::Array(a) => a.iter().find_map(find_holes),
        _ => None,
    }
}

/// #689's steps on the inch part: a bore on the detected Ø25 mm hole, cut
/// `oversize` mm past the wall; the `verify` reply's gate problems.
fn inch_bore_problems(oversize: f64) -> (Vec<String>, Value) {
    let mut c = Client { server: Server::new(), next: 1 };
    let path = model("Milling-test-inch.step").to_string_lossy().into_owned();
    c.ok("import_model", json!({ "path": path, "name": "part" }));
    c.ok("set_stock", json!({ "from_model": "part" }));
    c.ok("add_tool", json!({ "catalog": "EM-1000-4FL" }));
    c.ok("add_operation", json!({ "kind": "bore", "tool": "EM-1000-4FL", "source": "part", "name": "OP" }));
    c.ok("detect_holes", json!({ "name": "OP" }));
    if oversize > 0.0 {
        let job = c.ok("get_job", json!({}));
        let mut holes = find_holes(&job).expect("the bore carries its detected hole");
        for h in holes.as_array_mut().into_iter().flatten() {
            let r = h["radius"].as_f64().expect("a hole radius");
            h["radius"] = json!(r + oversize);
        }
        c.ok("edit_operation", json!({ "name": "OP", "params": { "holes": holes } }));
    }
    c.ok("generate", json!({ "name": "OP" }));
    let v = c.ok("verify", json!({}));
    let problems = v["gate"]["problems"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    (problems, v)
}

#[test]
fn a_bore_at_the_inch_holes_own_size_passes_the_gate() {
    let (problems, v) = inch_bore_problems(0.0);
    assert!(problems.is_empty(), "a nominal bore on the inch part is no gouge: {v}");
    assert_eq!(v["ok"], json!(true), "{v}");
}

/// The blocking finding that holds the post at 0.05 mm: a toolpath `gouge`
/// warning of the bore, "move N cuts past the finish surface".
fn held_by_a_toolpath_gouge(v: &Value) -> bool {
    v["findings"].as_array().is_some_and(|f| {
        f.iter().any(|w| {
            w["op"] == json!("OP")
                && w["kind"] == json!("gouge")
                && w["blocks_post"] == json!(true)
                && w["message"].as_str().is_some_and(|m| m.ends_with("cuts past the finish surface"))
        })
    })
}

/// 0.05 mm into the wall: the toolpath check holds the post. The stock
/// sim's lattice is coarser than this depth since #917 (#2846); the 0.1 mm
/// test below pins the program gate's own refusal.
#[test]
fn a_bore_cut_oversize_into_the_inch_holes_wall_is_still_refused() {
    let (_, v) = inch_bore_problems(0.05);
    assert_eq!(v["ok"], json!(false), "0.05 mm into the wall is refused: {v}");
    assert_eq!(v["held_by"], json!(["OP"]), "the oversize bore holds the post: {v}");
    assert!(held_by_a_toolpath_gouge(&v), "held by the bore's toolpath gouge warning: {v}");
}

/// 0.1 mm into the inch part's wall: the program gate names the gouge.
#[test]
fn a_bore_cut_a_tenth_into_the_inch_holes_wall_is_a_gate_gouge() {
    let (problems, v) = inch_bore_problems(0.1);
    assert!(problems.iter().any(|p| p.starts_with("gouge:")), "0.1 mm into the wall is a gouge: {v}");
    assert_eq!(v["ok"], json!(false), "{v}");
}

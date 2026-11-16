//! #979: box selection under the one pick rule (#692). A plate with two
//! square through-cutouts, seen from straight above: a window box around
//! one cutout takes exactly its four top-rim chains; a box around the
//! whole plate takes every top rim chain and nothing under the top face;
//! pick-through is what brings the hidden rims in; a crossing box takes
//! what it touches; faces box the same way; degenerate boxes take
//! nothing. Plus the MCP twin, `pick_in_box`: one undo step, undo
//! restores, refusals by name, and `through` (default off) as the
//! viewport's Pick through toggle.
//!
//! Gate map (#979): covers app::pick::box_pick, app::pick::feature_pick
//! (occluded), app::mcp::box_pick, app::commands (SetOperationGeometry).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{BoxMode, BoxQuery, PickBody, PickKinds, PickRay, Projection, box_bodies};
use camcraft_app::schema::Location;
use serde_json::{Value, json};

type P = [f64; 3];

const XS: [f64; 6] = [0.0, 20.0, 40.0, 60.0, 80.0, 100.0];
const YS: [f64; 4] = [0.0, 20.0, 40.0, 60.0];
const TOP: f64 = 10.0;
/// The cutouts: grid cells (1, 1) and (3, 1) - [20, 40] and [60, 80] in X,
/// [20, 40] in Y.
const HOLES: [(usize, usize); 2] = [(1, 1), (3, 1)];

fn quad(out: &mut Vec<Triangle>, q: [P; 4], n: P) {
    let p = |v: P| Point3::new(v[0], v[1], v[2]);
    let c = (p(q[1]) - p(q[0])).cross(p(q[2]) - p(q[0]));
    let q = if c.x * n[0] + c.y * n[1] + c.z * n[2] < 0.0 { [q[0], q[3], q[2], q[1]] } else { q };
    out.push(Triangle::new(p(q[0]), p(q[1]), p(q[2])));
    out.push(Triangle::new(p(q[0]), p(q[2]), p(q[3])));
}

/// A 100 x 60 x 10 plate on a 20 mm grid with the two cutouts: each solid
/// cell is a top and a bottom quad, and every side it shares with a hole
/// or the outside is a wall quad.
fn plate() -> Vec<Triangle> {
    let solid = |i: isize, j: isize| {
        (0..5).contains(&i) && (0..3).contains(&j) && !HOLES.contains(&(i as usize, j as usize))
    };
    let mut t = Vec::new();
    for i in 0..5isize {
        for j in 0..3isize {
            if !solid(i, j) {
                continue;
            }
            let (x0, x1, y0, y1) = (XS[i as usize], XS[i as usize + 1], YS[j as usize], YS[j as usize + 1]);
            let sq = |z: f64| [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]];
            quad(&mut t, sq(TOP), [0.0, 0.0, 1.0]);
            quad(&mut t, sq(0.0), [0.0, 0.0, -1.0]);
            for (di, dj, a, b) in [(-1, 0, [x0, y0], [x0, y1]), (1, 0, [x1, y0], [x1, y1]), (0, -1, [x0, y0], [x1, y0]), (0, 1, [x0, y1], [x1, y1])] {
                if !solid(i + di, j + dj) {
                    let wall = [[a[0], a[1], 0.0], [b[0], b[1], 0.0], [b[0], b[1], TOP], [a[0], a[1], TOP]];
                    quad(&mut t, wall, [di as f64, dj as f64, 0.0]);
                }
            }
        }
    }
    t
}

fn body() -> PickBody {
    PickBody::new("plate", Arc::from(plate()), Location::default())
}

/// The top-down view's 2D frame is world X/Y itself.
fn top_xy(p: P) -> Option<[f64; 2]> {
    Some([p[0], p[1]])
}

fn boxed(b: &PickBody, corners: [[f64; 2]; 2], mode: BoxMode, kinds: PickKinds, view: PickRay, through: bool) -> Vec<GeoSel> {
    let q = BoxQuery { project: &top_xy, corners, mode, kinds, view, through };
    box_bodies(std::slice::from_ref(b), &q, |_| true)
}

fn ortho() -> PickRay {
    PickRay { origin: [50.0, 30.0, 40.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho }
}

/// Every end point of a picked chain, local = world here.
fn ends(b: &PickBody, g: &GeoSel) -> Vec<P> {
    let FeatureSel::Chain(id) = g.feature else { panic!("not a chain: {g:?}") };
    let c = b.graph.chains().iter().find(|c| c.id == id).expect("the chain is the plate's");
    c.edges.iter().flat_map(|e| {
        let (a, z) = b.graph.edge_ends(*e).expect("edge resolves");
        [a, z]
    }).collect()
}

fn in_rect(p: P, lo: [f64; 2], hi: [f64; 2]) -> bool {
    p[0] >= lo[0] - 1e-9 && p[0] <= hi[0] + 1e-9 && p[1] >= lo[1] - 1e-9 && p[1] <= hi[1] + 1e-9
}

#[test]
fn a_window_around_one_cutout_takes_exactly_its_top_rim() {
    let b = body();
    let picks = boxed(&b, [[15.0, 15.0], [45.0, 45.0]], BoxMode::Window, PickKinds::EDGES, ortho(), false);
    assert_eq!(picks.len(), 4, "the cutout's four top-rim chains, nothing hidden: {picks:?}");
    for g in &picks {
        assert_eq!(g.entity, "plate");
        for p in ends(&b, g) {
            assert!((p[2] - TOP).abs() < 1e-9, "a rim under the top face was taken: {p:?}");
            assert!(in_rect(p, [20.0, 20.0], [40.0, 40.0]), "a chain off the cutout: {p:?}");
        }
    }
    // Visible means visible: an eye in perspective straight over the
    // cutout looks down into it and sees its walls' corners and its
    // bottom rim too - and still nothing of the rest of the plate.
    let eye = PickRay { origin: [30.0, 30.0, 200.0], dir: [0.0, 0.0, -1.0], projection: Projection::Perspective };
    let persp = boxed(&b, [[15.0, 15.0], [45.0, 45.0]], BoxMode::Window, PickKinds::EDGES, eye, false);
    assert_eq!(persp.len(), 12, "top rim, vertical corners and bottom rim: {persp:?}");
    assert!(picks.iter().all(|g| persp.contains(g)));
    assert!(persp.iter().all(|g| ends(&b, g).iter().all(|p| in_rect(*p, [20.0, 20.0], [40.0, 40.0]))));
}

#[test]
fn a_window_around_everything_takes_every_top_rim_chain_and_no_hidden_one() {
    let b = body();
    let all = [[-5.0, -5.0], [105.0, 65.0]];
    let picks = boxed(&b, all, BoxMode::Window, PickKinds::EDGES, ortho(), false);
    assert_eq!(picks.len(), 4 + 4 * HOLES.len(), "outer rim + each cutout's rim, top only: {picks:?}");
    assert!(picks.iter().all(|g| ends(&b, g).iter().all(|p| (p[2] - TOP).abs() < 1e-9)), "a hidden chain was taken");
    // The hidden rims and the vertical corner edges are there to take -
    // only pick-through takes them.
    let through = boxed(&b, all, BoxMode::Window, PickKinds::EDGES, ortho(), true);
    assert_eq!(through.len(), 3 * (4 + 4 * HOLES.len()), "top rims, bottom rims and the vertical corners: {through:?}");
    assert!(picks.iter().all(|g| through.contains(g)));
    // Seen from below, the bottom rims are the visible ones.
    let below = PickRay { origin: [50.0, 30.0, -40.0], dir: [0.0, 0.0, 1.0], projection: Projection::Ortho };
    let under = boxed(&b, all, BoxMode::Window, PickKinds::EDGES, below, false);
    assert_eq!(under.len(), picks.len());
    assert!(under.iter().all(|g| ends(&b, g).iter().all(|p| p[2].abs() < 1e-9)), "from below only the bottom rims");
}

#[test]
fn a_crossing_box_takes_what_it_touches_a_window_only_what_it_holds() {
    let b = body();
    // A sliver over the left side of the first cutout (x = 20, y 20..40).
    let sliver = [[18.0, 25.0], [22.0, 35.0]];
    assert!(boxed(&b, sliver, BoxMode::Window, PickKinds::EDGES, ortho(), false).is_empty(), "nothing lies wholly inside");
    let picks = boxed(&b, sliver, BoxMode::Crossing, PickKinds::EDGES, ortho(), false);
    assert_eq!(picks.len(), 1, "the one rim side it crosses: {picks:?}");
    assert!(ends(&b, &picks[0]).iter().all(|p| (p[0] - 20.0).abs() < 1e-9 && (p[2] - TOP).abs() < 1e-9));
    // The drag direction picks the convention.
    assert_eq!(BoxMode::of_drag(10.0, 30.0), BoxMode::Window);
    assert_eq!(BoxMode::of_drag(30.0, 10.0), BoxMode::Crossing);
}

#[test]
fn faces_box_the_same_way_and_hidden_faces_stay_out() {
    let b = body();
    let picks = boxed(&b, [[-5.0, -5.0], [105.0, 65.0]], BoxMode::Window, PickKinds::FACES, ortho(), false);
    assert_eq!(picks.len(), 1, "from above only the top face shows: {picks:?}");
    assert!(matches!(picks[0].feature, FeatureSel::Face(_)));
    let top = b.graph.faces.iter().find(|f| f.sel == Some(picks[0].feature)).expect("the face is the plate's");
    let t = &b.tris[top.facets[0] as usize];
    assert!((t.a.z - TOP).abs() < 1e-9, "the top face, not the bottom");
    // A window over one cutout holds no whole face; a crossing box
    // touches the top face (and its walls stay hidden).
    let cut = [[15.0, 15.0], [45.0, 45.0]];
    assert!(boxed(&b, cut, BoxMode::Window, PickKinds::FACES, ortho(), false).is_empty());
    assert_eq!(boxed(&b, cut, BoxMode::Crossing, PickKinds::FACES, ortho(), false), picks);
}

#[test]
fn degenerate_boxes_take_nothing() {
    let b = body();
    let k = PickKinds::EDGES;
    assert!(boxed(&b, [[10.0, 10.0], [10.0, 50.0]], BoxMode::Window, k, ortho(), false).is_empty(), "zero width");
    assert!(boxed(&b, [[f64::NAN, 0.0], [100.0, 60.0]], BoxMode::Crossing, k, ortho(), false).is_empty(), "NaN");
    let bad = PickRay { origin: [0.0; 3], dir: [0.0; 3], projection: Projection::Ortho };
    assert!(boxed(&b, [[-5.0, -5.0], [105.0, 65.0]], BoxMode::Window, k, bad, false).is_empty(), "no view direction");
    // Nothing projects: nothing is taken, nothing panics.
    let q = BoxQuery { project: &|_| None, corners: [[-5.0, -5.0], [105.0, 65.0]], mode: BoxMode::Crossing, kinds: PickKinds::ALL, view: ortho(), through: false };
    assert!(box_bodies(std::slice::from_ref(&b), &q, |_| true).is_empty());
}

// ---- the MCP twin --------------------------------------------------------

/// Writes the plate STL into a directory unique to this call (pid plus a
/// process-wide counter), so parallel tests never share or delete each other's file.
fn write_plate_stl() -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, Ordering::Relaxed);
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-box-pick-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("plate.stl");
    let mut s = String::from("solid plate\n");
    for t in plate() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid plate\n");
    std::fs::write(&path, s).expect("write stl");
    path
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let err = result["isError"] == json!(true);
    (err, if err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
}

#[test]
fn mcp_pick_in_box_adds_the_rim_in_one_undo_step() {
    let stl = write_plate_stl();
    let mut s = Server::new();
    let ok = |s: &mut Server, id: i64, name: &str, args: Value| {
        let (err, body) = call(s, id, name, args);
        assert!(!err, "{name} refused: {body}");
        body
    };
    ok(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }));
    ok(&mut s, 2, "set_stock", json!({ "box": { "dx": 100.0, "dy": 60.0, "dz": 10.0 } }));
    ok(&mut s, 3, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    ok(&mut s, 4, "add_operation", json!({ "kind": "contour", "tool": "EM-0400-2FL", "name": "C1", "source": "plate" }));
    let before = ok(&mut s, 5, "get_operation_geometry", json!({ "operation": "C1" }));

    let one = ok(&mut s, 6, "pick_in_box", json!({ "operation": "C1", "box": { "x0": 15.0, "y0": 15.0, "x1": 45.0, "y1": 45.0 } }));
    assert_eq!(one["added"], json!(4), "{one}");
    assert_eq!(one["source"]["picks"].as_array().map(Vec::len), Some(4), "{one}");
    // A second box adds the other cutout to the list.
    let two = ok(&mut s, 7, "pick_in_box", json!({ "operation": "C1", "box": { "x0": 85.0, "y0": 45.0, "x1": 55.0, "y1": 15.0 } }));
    assert_eq!(two["added"], json!(4), "{two}");
    assert_eq!(two["source"]["picks"].as_array().map(Vec::len), Some(8), "{two}");
    // The same box again changes nothing, and says so.
    let (err, msg) = call(&mut s, 8, "pick_in_box", json!({ "operation": "C1", "box": { "x0": 85.0, "y0": 45.0, "x1": 55.0, "y1": 15.0 } }));
    assert!(err && msg.as_str().is_some_and(|m| m.contains("already picked")), "{msg}");
    // Undo takes one box back at a time.
    ok(&mut s, 9, "undo", json!({}));
    let back = ok(&mut s, 10, "get_operation_geometry", json!({ "operation": "C1" }));
    assert_eq!(back["source"], one["source"], "one undo, one box");
    ok(&mut s, 11, "undo", json!({}));
    let first = ok(&mut s, 12, "get_operation_geometry", json!({ "operation": "C1" }));
    assert_eq!(first["source"], before["source"], "undo restores the previous source");

    // Everything, replacing: every top rim chain, nothing hidden.
    let all = ok(&mut s, 13, "pick_in_box", json!({ "operation": "C1", "replace": true, "box": { "x0": -5.0, "y0": -5.0, "x1": 105.0, "y1": 65.0 } }));
    assert_eq!(all["added"], json!(12), "{all}");
    // Refusals by name, never a panic.
    for (args, want) in [
        (json!({ "operation": "C1", "box": { "x0": 1.0, "y0": 1.0, "x1": 2.0, "y1": 2.0 } }), "no visible edge chain"),
        (json!({ "operation": "C1", "box": { "x0": 1.0, "y0": 1.0, "x1": 1.0, "y1": 9.0 } }), "no area"),
        (json!({ "operation": "C1", "box": { "x0": 1.0, "y0": 1.0, "x1": 9.0 } }), "\"y1\""),
        (json!({ "operation": "C1", "mode": "lasso", "box": { "x0": 0.0, "y0": 0.0, "x1": 9.0, "y1": 9.0 } }), "unknown mode"),
        (json!({ "operation": "C1", "through": "yes", "box": { "x0": 0.0, "y0": 0.0, "x1": 9.0, "y1": 9.0 } }), "\"through\" must be"),
        (json!({ "operation": "Nope", "box": { "x0": 0.0, "y0": 0.0, "x1": 9.0, "y1": 9.0 } }), "Nope"),
    ] {
        let (err, msg) = call(&mut s, 20, "pick_in_box", args);
        assert!(err && msg.as_str().is_some_and(|m| m.contains(want)), "{want}: {msg}");
    }
}

#[test]
fn mcp_pick_in_box_is_visible_only_unless_through() {
    let stl = write_plate_stl();
    let mut s = Server::new();
    let ok = |s: &mut Server, id: i64, name: &str, args: Value| {
        let (err, body) = call(s, id, name, args);
        assert!(!err, "{name} refused: {body}");
        body
    };
    ok(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }));
    ok(&mut s, 2, "set_stock", json!({ "box": { "dx": 100.0, "dy": 60.0, "dz": 10.0 } }));
    ok(&mut s, 3, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    ok(&mut s, 4, "add_operation", json!({ "kind": "contour", "tool": "EM-0400-2FL", "name": "C1", "source": "plate" }));
    let cut = json!({ "x0": 15.0, "y0": 15.0, "x1": 45.0, "y1": 45.0 });
    // No `through`, and `through: false`: the top rim only.
    let plain = ok(&mut s, 5, "pick_in_box", json!({ "operation": "C1", "replace": true, "box": cut }));
    assert_eq!(plain["added"], json!(4), "{plain}");
    let off = ok(&mut s, 6, "pick_in_box", json!({ "operation": "C1", "replace": true, "through": false, "box": cut }));
    assert_eq!(off["source"], plain["source"], "through: false is the default");
    // `through: true`: the hidden bottom rim and vertical corners too.
    let on = ok(&mut s, 7, "pick_in_box", json!({ "operation": "C1", "replace": true, "through": true, "box": cut }));
    assert_eq!(on["added"], json!(12), "{on}");
    let (plain_picks, on_picks) = (plain["source"]["picks"].as_array().cloned().unwrap_or_default(), on["source"]["picks"].as_array().cloned().unwrap_or_default());
    assert!(plain_picks.iter().all(|p| on_picks.contains(p)), "the visible ones stay in");
}

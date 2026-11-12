//! The MCP pick tools (#180): list a model's features under their stable
//! geometry-hash ids, pick a pocket's floor loop off an imported STL as
//! an operation's source through `SetOperationGeometry`, generate and
//! verify it, set the area controls through `EditOperationParams::
//! with_area`, and undo back to the previous source - every refusal a
//! tool error, never a panic.
//!
//! Gate map (#180): covers app::mcp (pick, features, area, tools,
//! fixture_tools locate_part mate ids, #692),
//! app::commands (SetOperationGeometry, EditOperationParams).

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-pick-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

type P = [f64; 3];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: P, b: P) -> P {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Two triangles of the quad a-b-c-d, wound so their normal agrees with
/// `n` (the fixture states which way each face looks, not its winding).
fn quad(out: &mut Vec<[P; 3]>, q: [P; 4], n: P) {
    let c = cross(sub(q[1], q[0]), sub(q[2], q[0]));
    let along = c[0] * n[0] + c[1] * n[1] + c[2] * n[2];
    let q = if along < 0.0 { [q[0], q[3], q[2], q[1]] } else { q };
    out.push([q[0], q[1], q[2]]);
    out.push([q[0], q[2], q[3]]);
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 blind pocket from the top,
/// floor at z 10, as ASCII STL. Written by the test so the fixture is
/// the code (the mcp suite's box precedent).
fn write_pocket_stl(path: &Path) {
    let (o, i, top, floor) = (20.0, 10.0, 20.0, 10.0);
    let sq = |h: f64, z: f64| -> [P; 4] { [[-h, -h, z], [h, -h, z], [h, h, z], [-h, h, z]] };
    let mut t: Vec<[P; 3]> = Vec::new();
    quad(&mut t, sq(o, 0.0), [0.0, 0.0, -1.0]);
    quad(&mut t, sq(i, floor), [0.0, 0.0, 1.0]);
    let (ob, ot, ib, it) = (sq(o, 0.0), sq(o, top), sq(i, floor), sq(i, top));
    for k in 0..4 {
        let n = (k + 1) % 4;
        // Side k runs corner k -> n; its outward normal points away from
        // the axis, the pocket's wall normal toward it.
        let mid = [(ob[k][0] + ob[n][0]) / 2.0, (ob[k][1] + ob[n][1]) / 2.0, 0.0];
        let out = [mid[0].signum() * (mid[0].abs() > 1.0) as i32 as f64, mid[1].signum() * (mid[1].abs() > 1.0) as i32 as f64, 0.0];
        quad(&mut t, [ob[k], ob[n], ot[n], ot[k]], out);
        quad(&mut t, [ib[k], ib[n], it[n], it[k]], [-out[0], -out[1], 0.0]);
        quad(&mut t, [ot[k], ot[n], it[n], it[k]], [0.0, 0.0, 1.0]);
    }
    let mut s = String::from("solid pocket\n");
    for tri in &t {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in tri {
            s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid pocket\n");
    std::fs::write(path, s).expect("write stl");
}

/// Bounds of the X / Y / Z words of a posted program's motion blocks
/// (the machine-home return, G28, is not a toolpath position).
fn posted_extent(path: &Path) -> ([f64; 3], [f64; 3]) {
    let text = std::fs::read_to_string(path).expect("posted program");
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for line in text.lines().filter(|l| !l.contains("G28") && !l.starts_with('(')) {
        for w in line.split_whitespace() {
            let Some(axis) = "XYZ".find(w.chars().next().unwrap_or(' ')) else { continue };
            let Ok(v) = w[1..].parse::<f64>() else { continue };
            lo[axis] = lo[axis].min(v);
            hi[axis] = hi[axis].max(v);
        }
    }
    (lo, hi)
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

/// A document with the pocketed part, a stock box over it, a 4 mm end
/// mill, and a 2D Pocket whose source is the whole part, its floor at
/// the bottom of whatever it is given to machine.
fn pocket_job(tag: &str) -> (Client, PathBuf) {
    let dir = tmp_dir(tag);
    let stl = dir.join("part.stl");
    write_pocket_stl(&stl);
    let mut c = Client::new();
    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("part"));
    c.ok("set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    let add = c.ok("add_operation", json!({
        "kind": "pocket", "tool": "EM-0400-2FL", "name": "P1", "source": "part",
        "params": {
            "top_z": { "reference": "top_of_stock", "offset": 0.0 },
            "bottom_z": { "reference": "bottom_of_selection", "offset": 0.0 },
            "stepdown": 2.5,
        },
    }));
    assert_eq!(add["operation"], json!("P1"));
    (c, dir)
}

/// The pocket floor: the one face looking up from below the top, and the
/// chain ids that run its outer ring.
fn floor_loop(c: &mut Client) -> (String, Vec<String>) {
    let f = c.ok("list_features", json!({ "model": "part", "kinds": ["faces", "chains"] }));
    let faces = f["faces"].as_array().expect("faces");
    let floors: Vec<&Value> = faces
        .iter()
        .filter(|fa| fa["facing"] == json!("up") && fa["bounds"]["max"][2].as_f64().is_some_and(|z| (z - 10.0).abs() < 1e-9))
        .collect();
    assert_eq!(floors.len(), 1, "one floor face at z 10: {f}");
    let floor = floors[0];
    let outer = floor["loops"].as_array().and_then(|l| l.iter().find(|l| l["kind"] == json!("outer"))).expect("an outer ring");
    assert_eq!(outer["complete"], json!(true), "every span of the ring is a feature edge: {outer}");
    let chains: Vec<String> = outer["chains"].as_array().expect("chains").iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
    // A square ring: four straight chains, each listed among the model's.
    assert_eq!(chains.len(), 4, "{outer}");
    let listed: Vec<&str> = f["chains"].as_array().expect("chains").iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(chains.iter().all(|id| listed.contains(&id.as_str())), "{chains:?} vs {listed:?}");
    (floor["id"].as_str().expect("face id").to_owned(), chains)
}

#[test]
fn a_picked_pocket_floor_loop_generates_verifies_and_undoes() {
    let (mut c, dir) = pocket_job("floor");
    let names: Vec<String> = c.server.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "tools/list" }))
        .and_then(|r| r["result"]["tools"].as_array().cloned())
        .map(|t| t.iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for want in ["list_features", "get_operation_geometry", "set_operation_geometry", "set_operation_area"] {
        assert!(names.iter().any(|n| n == want), "missing tool {want}: {names:?}");
    }

    let (face, chains) = floor_loop(&mut c);
    assert!(face.starts_with("face:0x"), "{face}");
    // The ids are the geometry hashes: listing again says the same.
    assert_eq!(floor_loop(&mut c), (face, chains.clone()));

    let before = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(before["source"], json!({ "model": "part" }));
    let picks: Vec<Value> = chains.iter().map(|id| json!({ "model": "part", "id": id })).collect();
    let set = c.ok("set_operation_geometry", json!({ "operation": "P1", "picks": picks }));
    assert_eq!(set["source"]["picks"], json!(picks), "{set}");
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["source"], json!({ "selection": 4 }), "{job}");

    let generated = c.ok("generate", json!({ "name": "P1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    let blocking = ver["findings"].as_array().is_some_and(|f| f.iter().any(|f| f["blocks_post"] == json!(true)));
    assert!(!blocking, "{ver}");
    // The path stays inside the 20 x 20 ring and on or above its floor.
    let posted = dir.join("p1.nc");
    let out = c.ok("post", json!({ "output": posted.display().to_string(), "post": "Fanuc" }));
    assert!(out["lines"].as_u64().is_some_and(|n| n > 0), "{out}");
    let (lo, hi) = posted_extent(&posted);
    // A 4 mm end mill inside the 20 x 20 ring: centres within +-8.
    assert!(lo[0] >= -8.0 - 1e-6 && hi[0] <= 8.0 + 1e-6 && lo[1] >= -8.0 - 1e-6 && hi[1] <= 8.0 + 1e-6, "{lo:?} {hi:?}");
    assert!((lo[2] - 10.0).abs() < 1e-6, "the deepest cut is the picked floor, z 10: {lo:?}");

    // The area controls: one undo step through the same door.
    let area = c.ok("set_operation_area", json!({ "operation": "P1", "area": { "offset": -0.5, "corner": "sharp" } }));
    assert_eq!(area["area"]["offset"], json!(-0.5), "{area}");
    assert_eq!(area["area"]["corner"], json!("sharp"));
    c.ok("undo", json!({}));
    let g = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(g["area"]["offset"], json!(0.0), "{g}");
    assert_eq!(g["area"]["corner"], json!("round"));
    assert_eq!(g["source"]["picks"], json!(picks));

    // Undo the generation, then the pick: the previous source is back.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    let g = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(g["source"], json!({ "model": "part" }), "{g}");
    c.ok("redo", json!({}));
    let g = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(g["source"]["picks"], json!(picks), "{g}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_picks_are_tool_errors_never_a_panic() {
    let (mut c, dir) = pocket_job("refuse");
    let (face, chains) = floor_loop(&mut c);
    let vertex = c.ok("list_features", json!({ "model": "part", "kinds": ["vertices"] }))["vertices"][0]["id"]
        .as_str()
        .expect("a vertex")
        .to_owned();
    let pick = |id: &str| json!([{ "model": "part", "id": id }]);

    // Unknown feature: a well-formed id the model does not hold.
    let msg = c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": pick("face:0x0000000000000001") }));
    assert!(msg.contains("not in the model"), "{msg}");
    for bad in ["face:", "face:zz", "edge:0x12", "0x12", "chain:0x1234567890abcdef0"] {
        let msg = c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": pick(bad) }));
        assert!(msg.contains("bad feature id"), "{bad}: {msg}");
    }
    // Wrong operation, wrong model, an empty or malformed pick list.
    let msg = c.refused("set_operation_geometry", json!({ "operation": "Nope", "picks": pick(&face) }));
    assert!(msg.contains("no operation named"), "{msg}");
    let msg = c.refused("set_operation_geometry", json!({ "operation": "EM-0400-2FL", "picks": pick(&face) }));
    assert!(msg.contains("no operation named"), "{msg}");
    let msg = c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": [{ "model": "EM-0400-2FL", "id": face }] }));
    assert!(msg.contains("is not a model"), "{msg}");
    let msg = c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": [{ "model": "ghost", "id": face }] }));
    assert!(msg.contains("no model named"), "{msg}");
    c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": [] }));
    c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": "all" }));
    c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": [{ "model": "part", "id": face, "x": 1 }] }));
    // A feature kind the op cannot take: a point bounds no pocket.
    let msg = c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": pick(&vertex) }));
    assert!(msg.contains("is a point"), "{msg}");
    // A wall's rim seen from above is a line: the command's own door.
    let wall = c.ok("list_features", json!({ "model": "part", "kinds": ["faces"] }))["faces"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["facing"] == json!("side")).cloned())
        .expect("a wall");
    c.refused("set_operation_geometry", json!({ "operation": "P1", "picks": pick(wall["id"].as_str().expect("id")) }));

    // Area refusals: unknown keys, bad words and numbers, a kind without an area.
    for bad in [
        json!({ "ofset": 1.0 }),
        json!({ "corner": "chamfer" }),
        json!({ "offset": "wide" }),
        json!({ "extension": -1.0 }),
        json!({ "extra": [{ "mode": "include", "from": "picks", "picks": [] }] }),
        json!({ "extra": [{ "mode": "include", "from": "stock", "picks": pick(&chains[0]) }] }),
        json!({ "open": { "picks": pick(&chains[0]) } }),
        json!([1]),
    ] {
        c.refused("set_operation_area", json!({ "operation": "P1", "area": bad }));
    }
    c.ok("add_tool", json!({ "catalog": "DR-0400" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0400", "name": "D1" }));
    c.refused("set_operation_area", json!({ "operation": "D1", "area": { "offset": 1.0 } }));

    // list_features refusals.
    c.refused("list_features", json!({ "model": "ghost" }));
    c.refused("list_features", json!({ "model": "EM-0400-2FL" }));
    c.refused("list_features", json!({ "model": "part", "kinds": ["edges"] }));
    c.refused("list_features", json!({ "model": "part", "limit": 0 }));
    let few = c.ok("list_features", json!({ "model": "part", "kinds": ["vertices"], "limit": 2 }));
    assert_eq!(few["vertices"].as_array().map(Vec::len), Some(2));
    assert_eq!(few["truncated"], json!(true));

    // No refusal reached the History: the op still machines the part.
    let g = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(g["source"], json!({ "model": "part" }), "{g}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 20 mm, 10 tall, 16-sided boss as ASCII STL.
fn write_boss_stl(path: &Path) {
    let mut s = String::from("solid boss\n");
    for t in camcraft_app::geometry::cylinder(20.0, 10.0, 16).facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid boss\n");
    std::fs::write(path, s).expect("write stl");
}

/// #692: list_features names only what a click can pick - a cylinder's
/// two rims are its chains (never the 16 facet seams of its wall), and a
/// round rim has no feature points (its chord vertices are not corners);
/// a seam chain asked for by id is refused by the one rule.
#[test]
fn listed_chains_and_vertices_follow_the_one_pick_rule() {
    let dir = tmp_dir("rule");
    let stl = dir.join("boss.stl");
    write_boss_stl(&stl);
    let mut c = Client::new();
    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    let name = imp["model"].as_str().expect("model name").to_owned();
    let f = c.ok("list_features", json!({ "model": name, "kinds": ["chains", "vertices"] }));
    assert_eq!(f["chains_total"], json!(2), "two rims, no wall seams: {f}");
    assert!(f["chains"].as_array().is_some_and(|cs| cs.iter().all(|c| c["closed"] == json!(true))), "{f}");
    assert_eq!(f["vertices_total"], json!(0), "a round rim has no corner: {f}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #692: `locate_part` takes its mate ids by the same rule - a vertex
/// along a round rim (a chord end, never a corner) is refused as a mate
/// point by name, like every other pick made by id.
#[test]
fn locate_part_refuses_a_mate_point_no_click_could_pick() {
    use camcraft_app::commands::{Command, ImportModel};
    use camcraft_app::geometry::feature;
    let dir = tmp_dir("locate-rule");
    let (boss, vise) = (dir.join("boss.stl"), dir.join("vise.stl"));
    write_boss_stl(&boss);
    write_pocket_stl(&vise);
    // The rim vertex's id, read off the same file in a world of our own:
    // ids are geometry hashes, so the server's model names it alike.
    let world = camcraft_app::world::new_world();
    ImportModel::new(&boss).expect("the boss imports").apply(&world);
    let (name, e) = camcraft_app::model::named_entities(&world).into_iter().next().expect("the boss");
    let f = camcraft_app::model::features_of(&world, &e).expect("features");
    let g = camcraft_app::pick::graph_of(&world, &name).expect("graph");
    let rim = feature::corner_verts(&f)
        .into_iter()
        .filter_map(|v| feature::vertex_id(&f, v))
        .find(|id| !g.points.iter().any(|p| p.id == *id))
        .expect("a vertex along the rim");

    let mut c = Client::new();
    let part = c.ok("import_model", json!({ "path": boss.display().to_string() }))["model"].as_str().expect("name").to_owned();
    let steel = c.ok("import_model", json!({ "path": vise.display().to_string() }))["model"].as_str().expect("name").to_owned();
    c.ok("add_fixture", json!({ "model": steel }));
    let top = flat_face(&mut c, &steel, 20.0, "up");
    let mate = |part_id: &str| json!({ "part": part, "fixture": steel, "mates": [{ "part": part_id, "fixture": top }] });
    let before = c.ok("get_job", json!({}));
    let e = c.refused("locate_part", mate(&format!("vertex:{rim:#018x}")));
    assert!(e.contains("not a feature point"), "{e}");
    // Refused at the door: placements, undo/redo state, all as they were.
    assert_eq!(c.ok("get_job", json!({})), before, "a refused locate changes nothing");
    // The check refuses no click-pickable mate: the boss's flat bottom seats on the vise top.
    let bottom = flat_face(&mut c, &part, 0.0, "down");
    c.ok("locate_part", mate(&bottom));
    let job = c.ok("get_job", json!({}));
    let boss = job["models"].as_array().and_then(|m| m.iter().find(|m| m["name"] == json!(part))).expect("the boss");
    assert!(boss["bounds"]["min"][2].as_f64().is_some_and(|z| (z - 20.0).abs() < 1e-6), "seated on the top at Z 20: {boss}");
    // One undo step back: the refusal added none, so this undoes the locate.
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["models"], before["models"], "undo restores the placements");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The id of `model`'s flat face at world Z `z` that reads `facing` (#884).
fn flat_face(c: &mut Client, model: &str, z: f64, facing: &str) -> String {
    let f = c.ok("list_features", json!({ "model": model, "kinds": ["faces"] }));
    let at = |fa: &&Value| fa["facing"] == json!(facing) && [fa["bounds"]["min"][2].as_f64(), fa["bounds"]["max"][2].as_f64()] == [Some(z); 2];
    f["faces"].as_array().and_then(|fs| fs.iter().find(at)).and_then(|fa| fa["id"].as_str()).map(str::to_owned).unwrap_or_else(|| panic!("{model}: no {facing} face at Z {z}: {f}"))
}

/// #884: a closed solid's top cap reads "up" and its bottom cap "down" -
/// the cylinder boss and a box - and the winding-free plane normal no
/// longer decides it.
#[test]
fn list_features_facing_follows_the_outward_side() {
    let dir = std::env::temp_dir().join(format!("cc_facing_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let boss = dir.join("boss.stl");
    write_boss_stl(&boss);
    let mut c = Client::new();
    let part = c.ok("import_model", json!({ "path": boss.display().to_string() }))["model"].as_str().expect("name").to_owned();
    flat_face(&mut c, &part, 10.0, "up");
    flat_face(&mut c, &part, 0.0, "down");
    let f = c.ok("list_features", json!({ "model": part, "kinds": ["faces"] }));
    let bottom = f["faces"].as_array().and_then(|fs| fs.iter().find(|fa| fa["facing"] == json!("down"))).expect("a down face");
    assert!(bottom["normal"][2].as_f64().is_some_and(|z| z < -0.999), "outward normal: {bottom}");
    let bx = dir.join("box.stl");
    let mut s = String::from("solid box\n");
    for t in camcraft_app::geometry::cube(camcraft_app::geometry::Point3::new(0.0, 0.0, 0.0), camcraft_app::geometry::Point3::new(10.0, 10.0, 10.0)).facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid box\n");
    std::fs::write(&bx, s).expect("write box");
    let b = c.ok("import_model", json!({ "path": bx.display().to_string() }))["model"].as_str().expect("name").to_owned();
    let f = c.ok("list_features", json!({ "model": b, "kinds": ["faces"] }));
    let n = |w: &str| f["faces"].as_array().map_or(0, |fs| fs.iter().filter(|fa| fa["facing"] == json!(w)).count());
    assert_eq!((n("up"), n("down"), n("side")), (1, 1, 4), "{f}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #956: the listed normal and facing are read where the part sits. The
/// boss turned 90 degrees about Y lies on its side (its bottom cap looks
/// -X, its top +X, both "side"); turned 180 degrees its bottom cap looks
/// up and its top down.
#[test]
fn list_features_normal_follows_the_part_placement() {
    let dir = std::env::temp_dir().join(format!("cc_placed_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let boss = dir.join("boss.stl");
    write_boss_stl(&boss);
    let mut c = Client::new();
    let part = c.ok("import_model", json!({ "path": boss.display().to_string() }))["model"].as_str().expect("name").to_owned();
    let caps = |c: &mut Client| -> Vec<Value> {
        let f = c.ok("list_features", json!({ "model": part, "kinds": ["faces"] }));
        let mut flat: Vec<Value> = f["faces"].as_array().expect("faces").iter().filter(|fa| fa["normal"][2].as_f64().is_some()).cloned().collect();
        flat.retain(|fa| fa["area"].as_f64().is_some_and(|a| a > 100.0));
        flat
    };
    let facings = |fs: &[Value], w: &str| fs.iter().filter(|fa| fa["facing"] == json!(w)).count();
    let h = std::f64::consts::FRAC_1_SQRT_2;
    c.ok("move_model", json!({ "name": part, "position": [0.0, 0.0, 0.0], "rotation": [0.0, h, 0.0, h] }));
    let side = caps(&mut c);
    assert_eq!(facings(&side, "side"), side.len(), "on its side every cap reads side: {side:?}");
    let x: Vec<f64> = side.iter().filter_map(|fa| fa["normal"][0].as_f64()).collect();
    assert!(x.iter().any(|v| *v < -0.999) && x.iter().any(|v| *v > 0.999), "caps look -X and +X: {x:?}");
    c.ok("move_model", json!({ "name": part, "position": [0.0, 0.0, 0.0], "rotation": [0.0, 1.0, 0.0, 0.0] }));
    let flipped = caps(&mut c);
    assert_eq!((facings(&flipped, "up"), facings(&flipped, "down")), (1, 1), "{flipped:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

//! #2173: the stock, not only the model, is located on an existing vise.
//! The stock's floor, jaw and stop faces mate the vise; the part moves
//! and its stock follows it, so the part keeps its place in the blank.
//! Undo puts both back. The fixture collision check reads the stock where
//! the locate leaves it. A follow that would leave the stock off its
//! mates (a square stock re-fitted round a turned part) is refused by
//! name. The part locate keeps its default and its result.
//! Provenance: our own fixtures over plain vise practice (machine-tool
//! handbooks); no vendor material consulted.
//!
//! Gate map (#2173): covers app::fixture_locate (LocateBody, stock_of,
//! stock_lands), app::commands::locate, app::fixture_clash
//! (after_fixture_move), app::mcp::fixture_tools (locate_part body),
//! app::mcp::features (list_features on a stock).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{CreateSetup, History, ImportModel, LocatePart, MoveModel, SetStock, TagFixture};
use camcraft_app::fixture_clash;
use camcraft_app::fixture_locate::{self, LocateBody, LocateOffset, LocateSpec, Mate};
use camcraft_app::geometry::{Aabb, Mesh, Point3, Triangle, cube};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{MateDatum, facets_centroid};
use camcraft_app::schema::Location;
use camcraft_app::stock::fit_box_sides;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Floor top at Z 0 (+Z), fixed jaw face at Y -1 (+Y), stop face at X -1 (+X).
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

/// The 20 x 10 x 8 part, square, far from the vise; Setup1; a square box
/// stock round it with `sides` = [-X, +X, -Y, +Y, bottom, top] allowance.
fn document(sides: [f64; 6]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0))));
    let at = Location { pos: p(150.0, 80.0, 30.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "part", at).expect("placed"));
    h.execute(&world, ImportModel::from_mesh("vise", Mesh::from_facets(vise())));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let (lo, hi) = bounds(&world, "part");
    let b = Aabb { min: p(lo[0], lo[1], lo[2]), max: p(hi[0], hi[1], hi[2]) };
    let (shape, pos) = fit_box_sides(&b, sides).expect("fits");
    h.execute(&world, SetStock::new(&world, shape, Location { pos, ..Location::default() }).expect("square stock"));
    (world, h)
}

/// The vise face whose facets centre on the point `c` (vise local = world).
fn vise_face(world: &World, c: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(world, "vise").expect("vise");
    let f = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .find(|f| {
            facets_centroid(&tris, f.facets.iter().map(|&i| i as usize))
                .is_some_and(|m| (0..3).all(|k| (m[k] - c[k]).abs() < 1e-6))
        })
        .map(|f| f.id)
        .unwrap_or_else(|| panic!("the vise has a face centred on {c:?}"));
    GeoSel { entity: "vise".into(), feature: FeatureSel::Face(id) }
}

/// The stock face with the outward normal `n`, read where the locate reads it.
fn stock_face(world: &World, n: [f64; 3]) -> GeoSel {
    let name = stock_name(world);
    let e = model::find_by_name(world, &name).expect("stock");
    let f = model::features_of(world, &e).expect("stock features");
    f.faces
        .iter()
        .map(|f| GeoSel { entity: name.clone(), feature: FeatureSel::Face(f.id) })
        .find(|g| {
            matches!(fixture_locate::read_pick(world, "vise", g),
                Ok(MateDatum::Plane { normal, .. }) if (0..3).all(|k| (normal[k] - n[k]).abs() < 1e-9))
        })
        .unwrap_or_else(|| panic!("the stock has a face facing {n:?}"))
}

/// The stock's floor, jaw and stop faces on the vise's.
fn spec(world: &World, offset: LocateOffset) -> LocateSpec {
    let mates = vec![
        Mate { part: stock_face(world, [0.0, 0.0, -1.0]), fixture: vise_face(world, [50.0, 30.0, 0.0]) },
        Mate { part: stock_face(world, [0.0, -1.0, 0.0]), fixture: vise_face(world, [50.0, -1.0, 20.25]) },
        Mate { part: stock_face(world, [-1.0, 0.0, 0.0]), fixture: vise_face(world, [-1.0, 30.0, 15.25]) },
    ];
    LocateSpec { part: "part".into(), fixture: "vise".into(), body: LocateBody::Stock, mates, offset }
}

fn bounds(world: &World, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(world, name).expect("entity");
    let b = model::entity_world_aabb(world, &e).expect("bounds");
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

fn stock_name(world: &World) -> String {
    camcraft_app::setup::stock_entity_name(world, Some("Setup1"))
}

#[test]
fn the_stock_locates_on_an_existing_vise_and_undo_puts_it_back() {
    // 1 mm on each side, 3 mm under the part, 2 mm on top.
    let (world, mut h) = document([1.0, 1.0, 1.0, 1.0, 3.0, 2.0]);
    assert_eq!(fixture_locate::stock_of(&world, "vise"), Some(stock_name(&world)));
    let before = (bounds(&world, &stock_name(&world)), bounds(&world, "part"));
    let c = LocatePart::new(&world, &spec(&world, LocateOffset::default())).expect("located");
    assert_eq!(c.warning(), None, "the stock sits flush on the floor, the jaw and the stop");
    h.execute(&world, c);
    let (lo, hi) = bounds(&world, &stock_name(&world));
    assert!(close(lo, [-1.0, -1.0, 0.0], 1e-6), "the stock on the stop, the jaw and the floor: {lo:?}");
    assert!(close(hi, [21.0, 11.0, 13.0], 1e-6), "the stock keeps its size: {hi:?}");
    let (plo, _) = bounds(&world, "part");
    assert!(close(plo, [0.0, 0.0, 3.0], 1e-6), "the part keeps its place in the stock: {plo:?}");
    h.undo(&world);
    assert_eq!((bounds(&world, &stock_name(&world)), bounds(&world, "part")), before, "undo restores both");
}

#[test]
fn the_fixture_collision_check_reads_the_located_stock() {
    let (world, mut h) = document([1.0, 1.0, 1.0, 1.0, 3.0, 2.0]);
    let vise_at = Location::default();
    assert_eq!(fixture_clash::after_fixture_move(&world, "vise", vise_at), None, "the stock is far from the vise");
    // 2 mm down: the stock goes into the floor (the part stays 1 mm above it).
    let sunk = LocateOffset { dz: -2.0, ..LocateOffset::default() };
    let c = LocatePart::new(&world, &spec(&world, sunk)).expect("built");
    let name = stock_name(&world);
    let w = c.warning().expect("the command warns before it lands");
    assert!(w.contains(&format!("the stock '{name}' would sink into the fixture 'vise'")), "{w}");
    h.execute(&world, c);
    let w = fixture_clash::after_fixture_move(&world, "vise", vise_at).expect("the check reads the located stock");
    assert!(w.contains(&format!("'{name}'")), "{w}");
    h.undo(&world);
    assert_eq!(fixture_clash::after_fixture_move(&world, "vise", vise_at), None, "undo: clear again");
    // Flush on the floor: the check reads the new place and finds it clear.
    h.execute(&world, LocatePart::new(&world, &spec(&world, LocateOffset::default())).expect("located"));
    assert_eq!(fixture_clash::after_fixture_move(&world, "vise", vise_at), None);
}

#[test]
fn a_stock_that_would_miss_its_mates_is_refused_by_name() {
    // Uneven allowances: a turned part re-fits its square stock with the
    // allowance of each setup side, so the stock faces would miss the mates.
    let (world, _h) = document([0.0, 2.0, 1.0, 1.0, 3.0, 2.0]);
    let turned = LocateOffset { turn_deg: 90.0, ..LocateOffset::default() };
    let Err(e) = LocatePart::new(&world, &spec(&world, turned)) else { panic!("a turned stock locate is refused") };
    assert!(e.to_string().contains("would not land on the mates"), "{e}");
    // Stock mates while the part is the held body: refused by name.
    let mut s = spec(&world, LocateOffset::default());
    s.body = LocateBody::Part;
    let Err(e) = fixture_locate::plan(&world, &s) else { panic!("the mates are on the stock, not the part") };
    assert!(e.contains("is not on the part 'part'"), "{e}");
}

#[test]
fn the_part_stays_the_default_body() {
    assert_eq!(LocateSpec::default().body, LocateBody::Part, "an API caller that names no body locates the part");
}

/// A `2x` by `2y` by `z` box as ASCII STL, centered in XY, z from 0.
fn write_box_stl(path: &std::path::Path, (x, y, z): (f64, f64, f64)) {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -x } else { x }, if i & 2 == 0 { -y } else { y }, if i & 4 == 0 { 0.0 } else { z }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
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

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let r = &reply["result"];
        let err = r["isError"] == json!(true);
        (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
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

    /// The flat face of `model` lowest in Z: its id and its Z.
    fn lowest(&mut self, model: &str) -> (String, f64) {
        let f = self.ok("list_features", json!({ "model": model, "kinds": ["faces"] }));
        f["faces"]
            .as_array()
            .and_then(|fs| {
                fs.iter()
                    .filter(|fa| fa["bounds"]["min"][2] == fa["bounds"]["max"][2])
                    .filter_map(|fa| Some((fa["id"].as_str()?.to_owned(), fa["bounds"]["min"][2].as_f64()?)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            })
            .unwrap_or_else(|| panic!("{model} has a flat face: {f}"))
    }
}

/// The MCP `locate_part` takes `body: "stock"`: the mate ids are on the
/// stock (`list_features` lists a stock's faces), one undo step.
#[test]
fn mcp_locate_part_seats_the_stock_by_its_feature_ids() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-locate-stock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (part, vise) = (dir.join("block.stl"), dir.join("vise.stl"));
    write_box_stl(&part, (10.0, 5.0, 8.0));
    write_box_stl(&vise, (40.0, 30.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("import_model", json!({ "path": vise.display().to_string() }));
    c.ok("add_fixture", json!({ "model": "vise" }));
    c.ok("move_fixture", json!({ "name": "vise", "position": [100.0, 0.0, -20.0] }));
    c.ok("set_stock", json!({ "box": { "dx": 24.0, "dy": 14.0, "dz": 12.0 } }));
    let stock = camcraft_app::schema::STOCK_NAME;
    let (bottom, z0) = c.lowest(stock);
    let f = c.ok("list_features", json!({ "model": "vise", "kinds": ["faces"] }));
    let top = f["faces"]
        .as_array()
        .and_then(|fs| fs.iter().find(|fa| fa["bounds"]["min"][2] == json!(-5.0) && fa["bounds"]["max"][2] == json!(-5.0)))
        .and_then(|fa| fa["id"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("the vise top at Z -5: {f}"));
    let mates = json!([{ "part": bottom, "fixture": top }]);
    let e = c.refused("locate_part", json!({ "part": "block", "fixture": "vise", "mates": mates, "body": "bench" }));
    assert!(e.contains("\"body\""), "{e}");
    let placed = c.ok("locate_part", json!({ "part": "block", "fixture": "vise", "mates": mates, "body": "stock" }));
    assert_eq!(placed["moved"], json!("block"), "{placed}");
    let (_, z) = c.lowest(stock);
    assert!((z + 5.0).abs() < 1e-6, "the stock bottom on the vise top at Z -5, got {z}");
    c.ok("undo", json!({}));
    let (_, z) = c.lowest(stock);
    assert!((z - z0).abs() < 1e-6, "undo puts the stock back at Z {z0}, got {z}");
    let _ = std::fs::remove_dir_all(&dir);
}

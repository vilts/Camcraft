//! #2576: a stock laid along X or Y, or turned by custom angles - the turn
//! lives in the stock's `Location.rot`, lands through the undoable
//! `SetStock` (also from the MCP `set_stock` tool's `axis` / `angles_deg`),
//! survives the project file, and a DXF profile is placed on a turned
//! stock's real top face or refused by name.
//!
//! Gate map: covers app::mcp (set_stock, undo, save_project), app::commands
//! (SetStock + History, ImportDxf), app::project (save_text, open),
//! app::sketch (stock_face), app::stock_follow (sim_frame), app::stock_fit
//! (fit_command, fitted_turned).
//!
//! Provenance: our own command, file and placement laws; standard
//! rigid-body math. No vendor material.

use serde_json::{json, Value};

use camcraft_app::commands::{History, ImportModel, SetStock};
use camcraft_app::geometry::stock_axis::{StockAxis, stock_axis_of, stock_axis_rot};
use camcraft_app::geometry::{Point3, cube, quat_rotate_point, transform_inverse_point};
use camcraft_app::mcp::Server;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_fit::{self, FitKind};
use camcraft_app::world::new_world;
use camcraft_app::{model, project, sketch};

fn rot_of(axis: StockAxis, angles: [f64; 3]) -> [f64; 4] {
    stock_axis_rot(axis, angles).expect("finite angles have a turn")
}

fn stock_rot(world: &flecs_ecs::prelude::World) -> [f64; 4] {
    let stock = camcraft_app::setup::stock_entity(world, None).expect("a stock");
    model::transform_of(world, &stock).expect("a stock pose").rot
}

fn same_turn(a: [f64; 4], b: [f64; 4]) -> bool {
    1.0 - a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>().abs() <= 1e-9
}

/// SetStock stores the X turn; undo restores the upright stock.
#[test]
fn set_stock_lays_a_cylinder_along_x_and_undo_restores_the_old_turn() {
    let world = new_world();
    let mut h = History::default();
    let cyl = StockShape::Cylinder { diameter: 40.0, height: 100.0 };
    h.execute(&world, SetStock::new(&world, cyl.clone(), Location::default()).expect("upright"));
    let laid = Location { rot: rot_of(StockAxis::X, [0.0; 3]), ..Default::default() };
    h.execute(&world, SetStock::new(&world, cyl, laid).expect("laid"));
    assert_eq!(stock_axis_of(stock_rot(&world)).0, StockAxis::X);
    assert!(h.undo(&world));
    assert!(same_turn(stock_rot(&world), [0.0, 0.0, 0.0, 1.0]), "undo restores the upright turn");
    assert!(h.redo(&world));
    assert_eq!(stock_axis_of(stock_rot(&world)).0, StockAxis::X, "redo lays it along X again");
}

/// "Fit to models" on a laid stock fits in the stock's own frame: a box
/// along X wraps the part plus the margin on every side, and a cylinder
/// along Y is snug about the part on its own axis (review of PR #2773).
#[test]
fn fit_to_models_wraps_the_part_in_a_laid_stocks_frame() {
    const M: f64 = 2.0;
    let (lo, hi) = ([0.0, 0.0, 0.0], [80.0, 30.0, 20.0]);
    let corners: Vec<Point3> = (0..8)
        .map(|i| {
            let c = |k: usize| if i >> k & 1 == 0 { lo[k] } else { hi[k] };
            Point3::new(c(0), c(1), c(2))
        })
        .collect();
    let laid_world = |axis: StockAxis, shape: StockShape| {
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2]))));
        let laid = Location { rot: rot_of(axis, [0.0; 3]), ..Default::default() };
        h.execute(&world, SetStock::new(&world, shape, laid).expect("laid"));
        (world, h)
    };

    let (world, mut h) = laid_world(StockAxis::X, StockShape::Box { dx: 10.0, dy: 10.0, dz: 10.0 });
    let (cmd, shape) = stock_fit::fit_command(&world, FitKind::Box(M)).expect("fits");
    h.execute(&world, cmd);
    assert_eq!(stock_axis_of(stock_rot(&world)).0, StockAxis::X, "the fit keeps the X turn");
    let StockShape::Box { dz, .. } = shape else { panic!("a box: {shape:?}") };
    assert!((dz - (hi[0] + 2.0 * M)).abs() < 1e-9, "the stock's own Z runs along world X: dz {dz}");
    let b = model::stock_world_aabb(&world).expect("stock box");
    let got = [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z];
    let want = [lo[0] - M, lo[1] - M, lo[2] - M, hi[0] + M, hi[1] + M, hi[2] + M];
    assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-9), "laid box {got:?}, want {want:?}");

    let (world, mut h) = laid_world(StockAxis::Y, StockShape::Cylinder { diameter: 10.0, height: 10.0 });
    let (cmd, shape) = stock_fit::fit_command(&world, FitKind::Cylinder(M)).expect("fits");
    h.execute(&world, cmd);
    let StockShape::Cylinder { diameter, height } = shape else { panic!("a cylinder: {shape:?}") };
    let want_d = 2.0 * (40f64.hypot(10.0) + M);
    assert!((diameter - want_d).abs() < 1e-9 && (height - (hi[1] + 2.0 * M)).abs() < 1e-9, "d {diameter} h {height}");
    let stock = camcraft_app::setup::stock_entity(&world, None).expect("a stock");
    let at = model::transform_of(&world, &stock).expect("a stock pose");
    assert_eq!(stock_axis_of(at.rot).0, StockAxis::Y, "the fit keeps the Y turn");
    for p in &corners {
        let l = transform_inverse_point(*p, at.pos, at.rot);
        assert!(l.x.hypot(l.y) <= 0.5 * diameter - M + 1e-9, "corner {p:?} is inside the radius less the margin: {l:?}");
        assert!(l.z >= M - 1e-9 && l.z <= height - M + 1e-9, "corner {p:?} is inside the length less the margin: {l:?}");
    }
}

/// A custom turn survives a save and an open, angle for angle.
#[test]
fn a_turned_stock_survives_the_project_file() {
    let world = new_world();
    let mut h = History::default();
    let rot = rot_of(StockAxis::Custom, [30.0, 15.0, -10.0]);
    let at = Location { pos: Point3::new(5.0, -7.0, 2.0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 40.0, dz: 20.0 }, at).expect("stock"));
    let text = project::save_text(&world).expect("saves");
    let again = new_world();
    project::open(&again, &text).expect("opens");
    assert!(same_turn(stock_rot(&again), rot), "the turn is kept: {:?}", stock_rot(&again));
    let (axis, angles) = stock_axis_of(stock_rot(&again));
    assert_eq!(axis, StockAxis::Custom);
    for (got, want) in angles.iter().zip([30.0, 15.0, -10.0]) {
        assert!((got - want).abs() < 1e-6, "angles {angles:?}");
    }
    let laid = Location { rot: rot_of(StockAxis::Y, [0.0; 3]), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 30.0, height: 80.0 }, laid).expect("laid"));
    let again = new_world();
    project::open(&again, &project::save_text(&world).expect("saves")).expect("opens");
    assert_eq!(stock_axis_of(stock_rot(&again)).0, StockAxis::Y, "a laid cylinder reopens laid");
}

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

/// The stock turn the server's document holds, read through a save.
fn saved_rot(s: &mut Server, id: i64, dir: &std::path::Path) -> [f64; 4] {
    let path = dir.join(format!("stock-{id}.camproj")).to_string_lossy().into_owned();
    let (err, body) = mcp_call(s, id, "save_project", json!({ "path": path }));
    assert!(!err, "save refused: {body}");
    let world = new_world();
    project::load(&world, std::path::Path::new(&path)).expect("loads");
    stock_rot(&world)
}

/// MCP `set_stock` takes `axis` or `angles_deg`, refuses both or a bad
/// axis by name, and `undo` walks the turn back.
#[test]
fn mcp_set_stock_takes_an_axis_and_undo_walks_it_back() {
    let dir = std::env::temp_dir().join(format!("camcraft-2576-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut s = Server::new();
    let cyl = json!({ "diameter": 40.0, "height": 100.0 });
    let (err, body) = mcp_call(&mut s, 1, "set_stock", json!({ "cylinder": cyl }));
    assert!(!err, "upright refused: {body}");
    assert_eq!(body["axis"], json!("Along Z"));
    let (err, body) = mcp_call(&mut s, 2, "set_stock", json!({ "cylinder": cyl, "axis": "y" }));
    assert!(!err, "axis y refused: {body}");
    assert_eq!(body["axis"], json!("Along Y"));
    assert_eq!(stock_axis_of(saved_rot(&mut s, 3, &dir)).0, StockAxis::Y);
    let (err, body) = mcp_call(&mut s, 4, "set_stock", json!({ "cylinder": cyl, "angles_deg": { "yaw": 45.0 } }));
    assert!(!err, "angles refused: {body}");
    assert_eq!(body["axis"], json!("Custom"));
    let (err, body) = mcp_call(&mut s, 5, "set_stock", json!({ "cylinder": cyl, "axis": "x", "angles_deg": {} }));
    assert!(err && body.as_str().is_some_and(|t| t.contains("not both")), "both refused: {body}");
    let (err, body) = mcp_call(&mut s, 6, "set_stock", json!({ "cylinder": cyl, "axis": "w" }));
    assert!(err && body.as_str().is_some_and(|t| t.contains("bad axis")), "bad axis refused: {body}");
    let (err, body) = mcp_call(&mut s, 7, "undo", json!({}));
    assert!(!err, "undo refused: {body}");
    assert_eq!(stock_axis_of(saved_rot(&mut s, 8, &dir)).0, StockAxis::Y, "undo restores the Y turn");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A box laid square takes a profile on the face that is on top; a laid
/// cylinder is refused by name, and DXF import says so.
#[test]
fn a_profile_lies_on_a_laid_box_and_a_laid_cylinder_is_refused() {
    let world = new_world();
    let mut h = History::default();
    let (dx, dy, dz) = (50.0, 40.0, 20.0);
    let at = Location { pos: Point3::new(3.0, 4.0, 5.0), rot: rot_of(StockAxis::X, [0.0; 3]) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz }, at).expect("box"));
    let top = [-0.5, 0.5]
        .iter()
        .flat_map(|&x| [-0.5, 0.5].map(move |y| (x, y)))
        .flat_map(|(x, y)| [0.0, 1.0].map(move |z| Point3::new(x * dx, y * dy, z * dz)))
        .map(|p| quat_rotate_point(at.rot, p).z + at.pos.z)
        .fold(f64::MIN, f64::max);
    let face = sketch::stock_face(&world, None).expect("a laid box has a top face").expect("a stock");
    assert!((face.pos.z - top).abs() < 1e-9, "the face is at the top: {} vs {top}", face.pos.z);
    let up = quat_rotate_point(face.rot, Point3::new(0.0, 0.0, 1.0));
    assert!(up.z > 1.0 - 1e-9, "the face looks up: {up:?}");

    let laid = Location { rot: rot_of(StockAxis::Y, [0.0; 3]), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 40.0, height: 100.0 }, laid).expect("cyl"));
    assert_eq!(sketch::stock_face(&world, None), Err(sketch::NO_TOP_FACE));
    assert!(sketch::face_frame(&world, None).is_none());
    let dxf = b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
    let opts = camcraft_app::commands::DxfOptions::default();
    let refused = camcraft_app::commands::ImportDxf::from_bytes(&world, None, "p.dxf", dxf, &opts);
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("no flat top face")),
        "DXF import names the refusal: {:?}",
        refused.as_ref().err().map(ToString::to_string)
    );
}

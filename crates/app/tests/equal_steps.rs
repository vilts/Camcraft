//! The depth ladder's equal-steps mode through the app (#2012): set by
//! `EditOperationParams` (undo restores the remainder ladder), the
//! project file writing `equal_steps` only when on and reading it back,
//! the stamp telling the two modes apart, the MCP `edit_operation` patch
//! reaching it, and the generated contour cutting the levels of its mode.
//!
//! Gate map (#2012): covers app::operation (params digest, links,
//! generate), app::project (approach DTO), app::commands
//! (CreateOperation, EditOperationParams), app::mcp (edit).

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::links::{Approach, Links};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

/// 9 mm of depth under the stock top, a 5 mm stepdown: the remainder
/// ladder steps 5 then 4, the equal ladder 4.5 twice.
fn contour_op(equal: bool) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 10.0),
        retract_z: Height::of(HeightReference::TopOfStock, 5.0),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Height::of(HeightReference::TopOfStock, -9.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Perpendicular { length: 5.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_radius: None,
        blend_turns: false,
        tilt: None,
        chain_overrides: Vec::new(),
        links: Links { approach: Approach { equal_steps: equal, ..Approach::DEFAULT }, ..Links::PLANAR }, picks: Default::default(),
    }
}

/// A 60 x 40 x 20 box stock and a 10 mm end mill.
fn job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "T", tool).expect("tool"));
    (world, h)
}

fn mode_of(world: &World) -> bool {
    let e = model::find_by_name(world, "Op1").expect("op");
    model::operation_of(world, &e).expect("op").params.links().expect("links").approach.equal_steps
}

#[test]
fn edit_sets_equal_steps_and_undo_restores_the_remainder_ladder() {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(false)).expect("create"));
    assert!(!mode_of(&world), "the default is the remainder ladder");
    h.execute(&world, EditOperationParams::new(&world, "Op1", contour_op(true)).expect("edit"));
    assert!(mode_of(&world));
    h.undo(&world);
    assert!(!mode_of(&world));
    h.redo(&world);
    assert!(mode_of(&world));
}

#[test]
fn the_project_file_writes_equal_steps_only_when_on_and_reads_it_back() {
    for equal in [false, true] {
        let (world, mut h) = job();
        h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(equal)).expect("create"));
        let path = std::env::temp_dir().join(format!("camcraft-equal-steps-{equal}-{}.camproj", std::process::id()));
        project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.contains("\"equal_steps\""), equal, "the key only when on");
        let back = new_world();
        project::load(&back, &path).expect("load");
        let _ = std::fs::remove_file(&path);
        assert_eq!(mode_of(&back), equal);
    }
}

#[test]
fn the_stamp_tells_the_two_ladders_apart() {
    let (world, _h) = job();
    let stamp = |equal: bool| operation::current_stamp(&world, "T", &SrcRef::Stock, &contour_op(equal), "X", None);
    assert!(stamp(false).is_some());
    assert_ne!(stamp(true), stamp(false));
    // An approach that is not the default hashes its fields: the mode is
    // one of them, so it also tells two such approaches apart.
    let overlapped = |equal: bool| {
        let mut p = contour_op(equal);
        if let OpParams::Contour { links, .. } = &mut p {
            links.approach.overlap = 1.0;
        }
        operation::current_stamp(&world, "T", &SrcRef::Stock, &p, "X", None)
    };
    assert!(overlapped(false).is_some());
    assert_ne!(overlapped(true), overlapped(false));
}

#[test]
fn the_region_law_reads_the_stepdown_of_the_ladder_mode() {
    // `operation::area` builds the pocket region at the planes of this
    // stepdown, so it must be the generator's.
    assert_eq!(contour_op(false).ladder_stepdown(20.0, 2.0, 5.0), 5.0);
    assert_eq!(contour_op(true).ladder_stepdown(20.0, 2.0, 5.0), 4.5);
}

/// The steps between the planes the generated path cuts at, top down.
fn steps(equal: bool) -> Vec<f64> {
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Stock, contour_op(equal)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let mut zs: Vec<f64> = tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    assert_eq!(zs.len(), 2, "two levels, none at Top Z: {zs:?} ({:?})", tp.path.notes);
    vec![zs[0] - zs[1], zs[1]]
}

#[test]
fn the_generated_contour_cuts_the_levels_of_its_mode() {
    let rem = steps(false);
    let eq = steps(true);
    assert!((rem[0] - 4.0).abs() < 1e-6, "the remainder ladder ends on a 4 mm step: {rem:?}");
    assert!((eq[0] - 4.5).abs() < 1e-6, "the equal ladder ends on a 4.5 mm step: {eq:?}");
    assert!((rem[1] - eq[1]).abs() < 1e-9, "the two modes end at Bottom Z: {rem:?} {eq:?}");
}

fn mcp(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("an answer");
    let r = reply["result"].clone();
    (r["isError"] == json!(true), r)
}

#[test]
fn mcp_edit_operation_reaches_equal_steps() {
    let mut s = Server::new();
    assert!(!mcp(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } })).0);
    assert!(!mcp(&mut s, "add_tool", json!({ "catalog": "EM-1000-4FL" })).0);
    for kind in ["Contour", "Pocket"] {
        let (err, r) = mcp(&mut s, "add_operation", json!({ "kind": kind, "tool": "EM-1000-4FL", "name": kind }));
        assert!(!err, "{r}");
        assert!(!r.to_string().contains("equal_steps"), "{kind}: a new operation has the remainder ladder: {r}");
        let patch = json!({ "name": kind, "params": { "links": { "approach": { "equal_steps": true } } } });
        let (err, r) = mcp(&mut s, "edit_operation", patch);
        assert!(!err, "{kind}: the patch is taken: {r}");
        assert!(r.to_string().contains("equal_steps"), "{kind}: the edited params carry it: {r}");
    }
}

//! #2033: the 2D Pocket's rest machining read from the previous
//! operations. A 60 x 40 pocket with 3 mm corners, roughed by a 12 mm end
//! mill; the rest pass with a 4 mm end mill reads the stock that the
//! roughing pass leaves, at every cut depth. Read against an independent
//! simulation of the roughing pass. A roughing pass that stops above the
//! floor leaves the whole pocket. A stock that cannot be established (an
//! earlier operation with no current toolpath or suppressed, a tilted
//! plane) is refused by name. An earlier change makes the rest pass stale.
//! The mode is an undoable edit, a project-file key and an MCP parameter.
//!
//! Gate map (#2033): covers app::operation (rest_op, the Pocket arm of
//! generate, job's previous-operations stock, params' 0xD8 stamp byte),
//! app::rest (prior_state, prior_digest), app::project (rest_dto),
//! app::mcp (edit_operation), app::posting (verification_gate).
//!
//! Provenance: our own fixtures (pocket_support's block with a pocket);
//! no other CAM product's code, text or output consulted.

mod common;
mod pocket_support;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, RegenerateAll,
    ReorderOperation, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::links::Links;
use camcraft_app::machining2d::{PocketStrategy, RestParams, RestSource};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::rest_op::REST_PREVIOUS_TILTED;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::Columns;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, recognize};
use flecs_ecs::prelude::*;
use pocket_support::{block_with_pocket, rounded_rect, write_stl};
use serde_json::{Value, json};

const BIG: &str = "EM-1200-2FL";
const SMALL: &str = "EM-0400-2FL";
const TOP: f64 = 20.0;
const FLOOR: f64 = 16.0;
const A: f64 = 30.0;
const B: f64 = 20.0;
const CORNER: f64 = 3.0;

fn part() -> camcraft_app::geometry::Mesh {
    block_with_pocket(40.0, 30.0, &rounded_rect(A, B, CORNER, 16), FLOOR, TOP)
}

fn from_ops() -> RestParams {
    RestParams {
        source: RestSource::PreviousOperations,
        previous_tool_diameter: 0.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

fn pocket(stepover: f64, bottom: f64, rest: Option<RestParams>, tilt: Option<Tilt>) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(),
        rest,
        blend_radius: None,
        links: Links::PLANAR,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: Some(Height::fixed(TOP + 2.0)),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(bottom),
        stepdown: 2.0,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 2.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt,
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

fn rough(bottom: f64) -> OpParams {
    pocket(5.0, bottom, None, None)
}

fn rest_op(rest: Option<RestParams>) -> OpParams {
    pocket(1.5, FLOOR, rest, None)
}

/// The part, its stock, both tools; R1 roughs with the 12 mm tool down to
/// `rough_bottom`, P2 is the 4 mm rest pass in previous-operations mode.
/// Nothing is generated.
fn job(rough_bottom: f64) -> (World, History, SrcRef) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part()));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    for name in [BIG, SMALL] {
        let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    }
    let p = recognize::pockets_of(&world, "part").expect("recognised").into_iter().next().expect("the pocket");
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(p.floor) }]);
    h.execute(&world, CreateOperation::new(&world, "R1", BIG, src.clone(), rough(rough_bottom)).expect("rough"));
    h.execute(&world, CreateOperation::new(&world, "P2", SMALL, src.clone(), rest_op(Some(from_ops()))).expect("rest"));
    (world, h, src)
}

fn generate(world: &World, h: &mut History, name: &str) -> Toolpath {
    h.execute(world, GenerateOperation::new(world, name).unwrap_or_else(|e| panic!("{name}: {e}")));
    let e = model::find_by_name(world, name).expect("op");
    let c = model::toolpath_of(world, &e).unwrap_or_else(|| panic!("{name} generated no path"));
    let bad: Vec<_> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{name}: {bad:?}");
    c.path.clone()
}

fn refused(world: &World, name: &str) -> String {
    match GenerateOperation::new(world, name) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("{name} generated, a refusal was expected"),
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

fn rest_of(world: &World, name: &str) -> Option<RestParams> {
    let e = model::find_by_name(world, name).expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Pocket { rest, .. } => rest,
        other => panic!("not a Pocket: {other:?}"),
    }
}

/// Signed distance into the pocket (positive inside).
fn depth_in(p: Point2) -> f64 {
    let qx = p.x.abs() - (A - CORNER);
    let qy = p.y.abs() - (B - CORNER);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    CORNER - outside - qx.max(qy).min(0.0)
}

fn feed_len(p: &Toolpath) -> f64 {
    p.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y)).sum()
}

/// The columns at least `margin` inside the pocket that stand above the floor.
fn standing(cols: &Columns<'_>, margin: f64) -> Vec<Point2> {
    let mut out = Vec::new();
    for iy in 0..cols.ny() {
        for ix in 0..cols.nx() {
            let c = cols.center(ix, iy);
            if depth_in(c) > margin && cols.top_at(ix, iy) > FLOOR + 1e-6 {
                out.push(c);
            }
        }
    }
    out
}

#[test]
fn the_rest_pass_cuts_what_the_simulated_roughing_pass_left() {
    let (world, mut h, src) = job(FLOOR);
    let r = generate(&world, &mut h, "R1");
    let p = generate(&world, &mut h, "P2");
    assert!(
        p.notes.iter().any(|n| n.contains("reads the stock that the previous operations leave")),
        "the path says where its rest area comes from: {:?}",
        p.notes
    );
    assert!(!p.notes.iter().any(|n| n.contains("trusts the entered previous tool")), "{:?}", p.notes);
    // The independent reading: the stock grid, cut by the roughing pass.
    let mut stock = camcraft_app::stock_sim::stock_grid(&world, None, 2.0).expect("the stock grid");
    for m in &r.moves {
        stock.subtract(m, 6.0, 0.005);
    }
    let cell = stock.cell;
    let left = standing(&Columns::of_grid(&stock), 2.0 * cell).len();
    assert!(left > 0, "the 12 mm tool leaves the 3 mm corners");
    for m in &p.moves {
        stock.subtract(m, 2.0, 0.005);
    }
    let still = standing(&Columns::of_grid(&stock), 2.0 * cell);
    assert!(still.is_empty(), "{} of {left} standing column(s) were not cut, first {:?}", still.len(), still.first());
    // It is a rest pass: far less feed than the plain pocket of the same tool.
    h.execute(&world, CreateOperation::new(&world, "P3", SMALL, src, rest_op(None)).expect("plain"));
    let plain = generate(&world, &mut h, "P3");
    assert!(feed_len(&p) < 0.6 * feed_len(&plain), "rest {:.0} mm, plain {:.0} mm", feed_len(&p), feed_len(&plain));
}

#[test]
fn a_roughing_pass_that_stops_above_the_floor_leaves_the_whole_pocket() {
    let (world, mut h, _) = job(18.0);
    let r = generate(&world, &mut h, "R1");
    let p = generate(&world, &mut h, "P2");
    assert!(p.notes.iter().any(|n| n.contains("the whole pocket is cut")), "{:?}", p.notes);
    // The floor is cut everywhere, not only in the corners.
    let mut stock = camcraft_app::stock_sim::stock_grid(&world, None, 2.0).expect("the stock grid");
    for m in &r.moves {
        stock.subtract(m, 6.0, 0.005);
    }
    let cell = stock.cell;
    let left = standing(&Columns::of_grid(&stock), 2.0 * cell).len();
    for m in &p.moves {
        stock.subtract(m, 2.0, 0.005);
    }
    let still = standing(&Columns::of_grid(&stock), 2.0 * cell);
    assert!(left > 1000, "the shallow roughing pass leaves the lower floor: {left} column(s)");
    assert!(still.is_empty(), "{} of {left} standing column(s) were not cut, first {:?}", still.len(), still.first());
}

#[test]
fn a_stock_that_cannot_be_established_is_refused_by_name() {
    let (world, mut h, _) = job(FLOOR);
    // Missing: the roughing pass was never generated.
    let why = refused(&world, "P2");
    assert!(why.contains("operation 'R1' ahead has no toolpath"), "{why}");
    generate(&world, &mut h, "R1");
    generate(&world, &mut h, "P2");
    // Stale: the roughing pass was edited and not generated again.
    h.execute(&world, EditOperationParams::new(&world, "R1", pocket(4.0, FLOOR, None, None)).expect("edit"));
    assert_eq!(status(&world, "R1"), OpStatus::Stale);
    let why = refused(&world, "P2");
    assert!(why.contains("the toolpath of operation 'R1' ahead is stale"), "{why}");
    generate(&world, &mut h, "R1");
    generate(&world, &mut h, "P2");
    // Suppressed: its cut is not made, so it proves nothing.
    h.execute(&world, EditOperationParams::with_suppressed(&world, "R1", true).expect("suppress"));
    let why = refused(&world, "P2");
    assert!(why.contains("operation 'R1' ahead is suppressed"), "{why}");
    // The path on record was generated against the suppressed cut: the post refuses it.
    let gate = posting::verification_gate(&world).join("\n");
    assert!(gate.contains("P2 was generated against the cut of suppressed R1"), "{gate}");
}

#[test]
fn a_tilted_plane_is_refused_by_name_at_the_edit_and_at_generation() {
    let (world, mut h, _) = job(FLOOR);
    generate(&world, &mut h, "R1");
    let tilt = Tilt { basis_version: 1, a_deg: 20.0, b_deg: 0.0, origin: [0.0, 0.0, TOP] };
    let tilted = pocket(1.5, FLOOR, Some(from_ops()), Some(tilt));
    let why = match EditOperationParams::new(&world, "P2", tilted.clone()) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("the tilted edit was accepted"),
    };
    assert!(why.contains(REST_PREVIOUS_TILTED), "{why}");
    // A document that holds the pair anyway (written past the door) is
    // refused at generation: the frame is not guessed.
    let e = model::find_by_name(&world, "P2").expect("op");
    let mut op = model::operation_of(&world, &e).expect("operation");
    op.params = tilted;
    op.src = SrcRef::Entity("part".into());
    e.entity_view(&world).set(op);
    let why = refused(&world, "P2");
    assert!(why.contains(REST_PREVIOUS_TILTED), "{why}");
}

#[test]
fn an_earlier_change_makes_the_rest_pass_stale() {
    let (world, mut h, _) = job(FLOOR);
    generate(&world, &mut h, "R1");
    generate(&world, &mut h, "P2");
    assert_eq!(status(&world, "P2"), OpStatus::Ok);
    // The earlier tool changes.
    let big = model::find_by_name(&world, BIG).and_then(|e| model::tool_of(&world, &e)).expect("tool");
    h.execute(&world, ChangeTool::new(&world, BIG, camcraft_app::tools::Tool { diameter: 10.0, ..big }).expect("retool"));
    assert_eq!(status(&world, "P2"), OpStatus::Stale, "a changed earlier tool");
    h.undo(&world);
    assert_eq!(status(&world, "P2"), OpStatus::Ok);
    // The earlier depth changes and that operation is generated again.
    h.execute(&world, EditOperationParams::new(&world, "R1", rough(18.0)).expect("edit"));
    generate(&world, &mut h, "R1");
    assert_eq!(status(&world, "P2"), OpStatus::Stale, "a changed earlier depth");
    let e = model::find_by_name(&world, "P2").expect("op");
    let why = operation::stale_reason(&world, &e).unwrap_or_default();
    assert!(why.contains("'R1'"), "{why}");
    // One batch brings the program current, and the rest pass now cuts the whole pocket.
    h.execute(&world, RegenerateAll::new(&world));
    assert_eq!(status(&world, "P2"), OpStatus::Ok);
    let p = model::toolpath_of(&world, &e).expect("path").path.clone();
    assert!(p.notes.iter().any(|n| n.contains("the whole pocket is cut")), "{:?}", p.notes);
    // The order changes: the rest pass moves ahead of the roughing pass.
    h.execute(&world, ReorderOperation::new(&world, "P2", "R1", false).expect("reorder"));
    assert_eq!(status(&world, "P2"), OpStatus::Stale, "a changed order");
}

#[test]
fn the_mode_is_an_undoable_edit_and_a_project_file_key() {
    let (world, mut h, _) = job(FLOOR);
    let entered = RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, ..from_ops() };
    assert_eq!(rest_of(&world, "P2"), Some(from_ops()));
    h.execute(&world, EditOperationParams::new(&world, "P2", rest_op(Some(entered))).expect("edit"));
    assert_eq!(rest_of(&world, "P2"), Some(entered));
    h.undo(&world);
    assert_eq!(rest_of(&world, "P2"), Some(from_ops()));
    h.redo(&world);
    assert_eq!(rest_of(&world, "P2"), Some(entered));
    // The file states the mode; the entered-tool mode states no key, as before.
    let on = project::params_to_json(&rest_op(Some(from_ops())));
    assert_eq!(on.pointer("/pocket/rest/source"), Some(&json!("previous_operations")), "{on}");
    assert_eq!(project::params_from_json(&on).expect("round trip"), rest_op(Some(from_ops())));
    let old = project::params_to_json(&rest_op(Some(entered)));
    assert!(old.pointer("/pocket/rest/source").is_none(), "{old}");
    assert_eq!(project::params_from_json(&old).expect("round trip"), rest_op(Some(entered)));
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a reply");
    let r = &reply["result"];
    let err = r["isError"] == json!(true);
    (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
}

#[test]
fn mcp_sets_the_mode_undoes_it_and_the_project_keeps_it() {
    let dir = std::env::temp_dir();
    let stl = dir.join(format!("camcraft-2033-{}.stl", std::process::id()));
    write_stl(&part(), &stl);
    let mut s = Server::new();
    for (i, (name, args)) in [
        ("import_model", json!({ "path": stl.to_string_lossy(), "name": "part" })),
        ("add_tool", json!({ "catalog": SMALL })),
        ("add_operation", json!({ "kind": "pocket", "tool": SMALL, "source": "part", "name": "P" })),
    ]
    .into_iter()
    .enumerate()
    {
        let (err, body) = call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{name}: {body}");
    }
    let _ = std::fs::remove_file(&stl);
    // No previous tool is needed in this mode.
    let (err, body) = call(&mut s, 10, "edit_operation", json!({ "name": "P", "params": { "rest": { "source": "previous_operations" } } }));
    assert!(!err, "{body}");
    assert_eq!(body["params"].pointer("/pocket/rest/source"), Some(&json!("previous_operations")), "{body}");
    // A mode that does not exist is refused.
    let (err, body) = call(&mut s, 11, "edit_operation", json!({ "name": "P", "params": { "rest": { "source": "guess" } } }));
    assert!(err, "{body}");
    let file = dir.join(format!("camcraft-2033-{}.camproj", std::process::id()));
    let path = file.to_string_lossy().into_owned();
    let (err, body) = call(&mut s, 12, "save_project", json!({ "path": path }));
    assert!(!err, "{body}");
    let mut loaded = Server::new();
    let (err, body) = call(&mut loaded, 1, "open_project", json!({ "path": path }));
    let _ = std::fs::remove_file(&file);
    assert!(!err, "{body}");
    let (_, job) = call(&mut loaded, 2, "get_job", json!({}));
    assert!(job.to_string().contains("previous_operations"), "the reopened project keeps the mode: {job}");
    let (err, body) = call(&mut s, 13, "undo", json!({}));
    assert!(!err, "{body}");
    let (_, job) = call(&mut s, 14, "get_job", json!({}));
    assert!(!job.to_string().contains("previous_operations"), "undo removes the mode: {job}");
}

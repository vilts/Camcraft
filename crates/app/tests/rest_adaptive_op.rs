//! #1446: rest machining on the 2D Adaptive operation. A 60 x 40 pocket
//! with 3 mm corners, roughed by a 12 mm end mill; the 2D Adaptive with a
//! 4 mm end mill and rest machining on reads the stock that the roughing
//! pass leaves and cuts only that. Read against an independent simulation
//! of the roughing pass: the rest material is cut, and no plunge goes down
//! a column that the roughing pass left standing under the cutter. What
//! cannot be proved is refused by name: an entered previous tool, a
//! roughing pass that stops above the floor, an earlier operation with no
//! current toolpath, a tilt, the Trochoidal method, arc turns. The group is an undoable edit, a project-file key and an MCP
//! parameter.
//!
//! Gate map (#1446): covers app::operation (rest_op, the Adaptive2D arm
//! of generate, job's previous-operations stock, params' 0xD9 stamp
//! byte), app::project (params, params_impl, rest_dto), app::mcp
//! (edit_operation).
//!
//! Provenance: our own fixtures (pocket_support's block with a pocket);
//! no other CAM product's code, text or output consulted.

mod common;
mod pocket_support;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::links::Links;
use camcraft_app::machining2d::{
    AdaptiveMethod, AdaptiveTrochParams, CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride,
    PocketStrategy, RestParams, RestSource, REST_ADAPTIVE_ENTERED,
};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::rest_op::{
    REST_ADAPTIVE_ARC_TURNS, REST_ADAPTIVE_CORNER_RELIEF, REST_ADAPTIVE_TILTED, REST_ADAPTIVE_TROCHOIDAL,
};
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::Columns;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, project, recognize};
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

fn rough(bottom: f64) -> OpParams {
    OpParams::Pocket {
        rest: None,
        blend_radius: None,
        links: Links::PLANAR,
        stepover: 5.0,
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
        corner_relief: Default::default(),
        pass_start: Default::default(),
        tilt: None,
        picks: Default::default(),
    }
}

fn adaptive(rest: Option<RestParams>) -> OpParams {
    adaptive_with(rest, AdaptiveMethod::Spiral, false, Default::default(), None)
}

fn adaptive_with(
    rest: Option<RestParams>,
    method: AdaptiveMethod,
    blend_turns: bool,
    corner_relief: CornerRelief,
    tilt: Option<Tilt>,
) -> OpParams {
    OpParams::Adaptive2D {
        rest,
        blend_radius: None,
        blend_turns,
        links: Links::PLANAR,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: Some(Height::fixed(TOP + 2.0)),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 2.0,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        method,
        troch: AdaptiveTrochParams::default(),
        corner_relief,
        pass_start: Default::default(),
        tilt,
        picks: Default::default(),
    }
}

/// The part, its stock, both tools; R1 roughs with the 12 mm tool down to
/// `rough_bottom`, A2 is the 4 mm 2D Adaptive with rest machining on.
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
    h.execute(&world, CreateOperation::new(&world, "A2", SMALL, src.clone(), adaptive(Some(from_ops()))).expect("rest"));
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

fn edit_refused(world: &World, p: OpParams) -> String {
    match EditOperationParams::new(world, "A2", p) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("the edit was accepted, a refusal was expected"),
    }
}

fn rest_of(world: &World, name: &str) -> Option<RestParams> {
    let e = model::find_by_name(world, name).expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Adaptive2D { rest, .. } => rest,
        other => panic!("not a 2D Adaptive: {other:?}"),
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

/// The highest column top under a disc of radius `rad` at `q`.
fn top_under(cols: &Columns<'_>, q: Point2, rad: f64) -> f64 {
    let mut top = f64::NEG_INFINITY;
    for iy in 0..cols.ny() {
        for ix in 0..cols.nx() {
            let c = cols.center(ix, iy);
            if (c.x - q.x).hypot(c.y - q.y) <= rad {
                top = top.max(cols.top_at(ix, iy));
            }
        }
    }
    top
}

#[test]
fn the_rest_adaptive_pass_cuts_what_the_simulated_roughing_pass_left() {
    let (world, mut h, _) = job(FLOOR);
    let r = generate(&world, &mut h, "R1");
    let p = generate(&world, &mut h, "A2");
    assert!(
        p.notes.iter().any(|n| n.contains("reads the stock that the previous operations leave")),
        "the path says where its rest area comes from: {:?}",
        p.notes
    );
    // The independent reading: the stock grid, cut by the roughing pass.
    let mut stock = camcraft_app::stock_sim::stock_grid(&world, None, 2.0).expect("the stock grid");
    for m in &r.moves {
        stock.subtract(m, 6.0, 0.005);
    }
    let cell = stock.cell;
    let left = standing(&Columns::of_grid(&stock), 3.0 * cell).len();
    assert!(left > 0, "the 12 mm tool leaves the 3 mm corners");
    // No plunge goes down a column that the roughing pass left standing
    // under the whole cutter (this operation's own cuts are not credited).
    {
        let cols = Columns::of_grid(&stock);
        let mut plunges = 0;
        for m in p.moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
            plunges += 1;
            let under = top_under(&cols, Point2::new(m.to.x, m.to.y), 2.0);
            assert!(under <= m.to.z + 0.01, "a plunge to Z{} over stock standing to Z{under} under the cutter", m.to.z);
        }
        assert!(plunges > 0, "the rest pass enters by plunges");
        // No rapid below the top of the stock, but a straight lift.
        for m in p.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
            let lift = m.from.x == m.to.x && m.from.y == m.to.y && m.to.z > m.from.z;
            assert!(lift || m.from.z.min(m.to.z) >= TOP - 1e-9, "a rapid below the top of the stock: {m:?}");
        }
    }
    for m in &p.moves {
        stock.subtract(m, 2.0, 0.005);
    }
    let still = standing(&Columns::of_grid(&stock), 3.0 * cell);
    assert!(still.is_empty(), "{} of {left} standing column(s) were not cut, first {:?}", still.len(), still.first());
    // It is a rest pass: every feed stays at a corner of the pocket.
    for m in p.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let (dx, dy) = (A - m.to.x.abs(), B - m.to.y.abs());
        assert!(dx < 14.0 && dy < 14.0, "a feed at ({}, {}) is outside the corners", m.to.x, m.to.y);
    }
    assert!(feed_len(&p) > 0.0);
}

/// #1446 / #3355: the rest pass keeps its own order of the rest areas.
/// A pass start other than Auto does not change its moves, and the path
/// names the choice as not used (the Pocket's rule, #3354).
#[test]
fn a_pass_start_beside_rest_machining_is_named_as_not_used() {
    use camcraft_app::machining2d::{PASS_START_REST_NOTE, PassStart};
    let (world, mut h, _) = job(FLOOR);
    generate(&world, &mut h, "R1");
    let auto = generate(&world, &mut h, "A2");
    assert!(!auto.notes.iter().any(|n| n == PASS_START_REST_NOTE), "Auto has no note: {:?}", auto.notes);
    for side in [PassStart::Inside, PassStart::Outside] {
        let mut p = adaptive(Some(from_ops()));
        if let OpParams::Adaptive2D { pass_start, .. } = &mut p {
            *pass_start = side;
        }
        h.execute(&world, EditOperationParams::new(&world, "A2", p).unwrap_or_else(|e| panic!("{side:?}: {e}")));
        let got = generate(&world, &mut h, "A2");
        assert!(got.notes.iter().any(|n| n == PASS_START_REST_NOTE), "{side:?}: the rest note: {:?}", got.notes);
        assert_eq!(got.moves.len(), auto.moves.len(), "{side:?}: the rest pass keeps its own moves");
    }
}

#[test]
fn a_stock_that_does_not_prove_an_entry_is_refused_by_name() {
    // The roughing pass stops at Z18: the floor at Z16 stands whole, so
    // no plunge is proved clear there. The whole pocket is not guessed.
    let (world, mut h, _) = job(18.0);
    let why = refused(&world, "A2");
    assert!(why.contains("operation 'R1' ahead has no toolpath"), "{why}");
    generate(&world, &mut h, "R1");
    let why = refused(&world, "A2");
    assert!(why.contains("rest Adaptive has no entry with proved clearance"), "{why}");
    // Stale: the roughing pass was edited and not generated again.
    h.execute(&world, EditOperationParams::new(&world, "R1", rough(FLOOR)).expect("edit"));
    assert_eq!(operation::status_of(&world, &model::find_by_name(&world, "R1").expect("op")), OpStatus::Stale);
    let why = refused(&world, "A2");
    assert!(why.contains("the toolpath of operation 'R1' ahead is stale"), "{why}");
}

#[test]
fn what_the_rest_planner_does_not_read_is_refused_by_name_at_the_edit() {
    let (world, _h, _) = job(FLOOR);
    // An entered previous tool proves nothing.
    let entered = RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, ..from_ops() };
    let why = edit_refused(&world, adaptive(Some(entered)));
    assert!(why.contains(REST_ADAPTIVE_ENTERED), "{why}");
    let why = edit_refused(&world, adaptive(Some(RestParams { allowance: -0.1, ..from_ops() })));
    assert!(why.contains("rest allowance"), "{why}");
    let tilted = Tilt { basis_version: 1, a_deg: 20.0, b_deg: 0.0, origin: [0.0, 0.0, TOP] };
    let dogbone = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let ov_only = CornerRelief {
        overrides: vec![CornerReliefOverride { at: Point2::new(15.0, 10.0), choice: CornerReliefChoice::Dogbone }],
        ..Default::default()
    };
    let on = Some(from_ops());
    let off = CornerRelief::default();
    let cases = [
        (REST_ADAPTIVE_TROCHOIDAL, adaptive_with(on, AdaptiveMethod::Trochoidal, false, off.clone(), None)),
        (REST_ADAPTIVE_ARC_TURNS, adaptive_with(on, AdaptiveMethod::Spiral, true, off.clone(), None)),
        (REST_ADAPTIVE_CORNER_RELIEF, adaptive_with(on, AdaptiveMethod::Spiral, false, dogbone.clone(), None)),
        (REST_ADAPTIVE_CORNER_RELIEF, adaptive_with(on, AdaptiveMethod::Spiral, false, ov_only, None)),
        (REST_ADAPTIVE_TILTED, adaptive_with(on, AdaptiveMethod::Spiral, false, off, Some(tilted))),
    ];
    for (name, p) in cases {
        let why = edit_refused(&world, p);
        assert!(why.contains(name), "expected '{name}', got: {why}");
    }
    // The same options without rest machining stay legal or keep their own refusal.
    assert!(EditOperationParams::new(&world, "A2", adaptive(None)).is_ok());
    assert!(EditOperationParams::new(&world, "A2", adaptive_with(None, AdaptiveMethod::Spiral, false, dogbone, None)).is_ok());
}

#[test]
fn a_document_past_the_door_is_refused_at_generation() {
    let (world, mut h, _) = job(FLOOR);
    generate(&world, &mut h, "R1");
    let e = model::find_by_name(&world, "A2").expect("op");
    let entered = RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, ..from_ops() };
    let dogbone = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let off = CornerRelief::default();
    for (name, rest, method, relief) in [
        (REST_ADAPTIVE_ENTERED, entered, AdaptiveMethod::Spiral, off.clone()),
        (REST_ADAPTIVE_TROCHOIDAL, from_ops(), AdaptiveMethod::Trochoidal, off),
        (REST_ADAPTIVE_CORNER_RELIEF, from_ops(), AdaptiveMethod::Spiral, dogbone),
    ] {
        let mut op = model::operation_of(&world, &e).expect("operation");
        op.params = adaptive_with(Some(rest), method, false, relief, None);
        e.entity_view(&world).set(op);
        let why = refused(&world, "A2");
        assert!(why.contains(name), "expected '{name}', got: {why}");
    }
}

#[test]
fn the_group_is_an_undoable_edit_a_project_file_key_and_moves_the_stamp() {
    let (world, mut h, _) = job(FLOOR);
    generate(&world, &mut h, "R1");
    generate(&world, &mut h, "A2");
    let e = model::find_by_name(&world, "A2").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    assert_eq!(rest_of(&world, "A2"), Some(from_ops()));
    // Off is an edit: the path on record is stale, undo brings it back.
    h.execute(&world, EditOperationParams::new(&world, "A2", adaptive(None)).expect("edit"));
    assert_eq!(rest_of(&world, "A2"), None);
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "the rest group moves the stamp");
    h.undo(&world);
    assert_eq!(rest_of(&world, "A2"), Some(from_ops()));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    h.redo(&world);
    assert_eq!(rest_of(&world, "A2"), None);
    h.undo(&world);
    // The allowance moves the stamp too.
    let wide = RestParams { allowance: 0.3, ..from_ops() };
    h.execute(&world, EditOperationParams::new(&world, "A2", adaptive(Some(wide))).expect("edit"));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "the allowance moves the stamp");
    // The file states the group; without it the file has no key, as before.
    let on = project::params_to_json(&adaptive(Some(wide)));
    let rest = on.as_object().and_then(|o| o.values().next()).and_then(|v| v.get("rest")).cloned();
    assert_eq!(rest, Some(json!({ "source": "previous_operations", "previous_tool_diameter": 0.0, "previous_corner_radius": 0.0, "previous_stock_to_leave": 0.0, "allowance": 0.3 })), "{on}");
    assert_eq!(project::params_from_json(&on).expect("round trip"), adaptive(Some(wide)));
    let off = project::params_to_json(&adaptive(None));
    assert!(!off.to_string().contains("rest"), "{off}");
    assert_eq!(project::params_from_json(&off).expect("round trip"), adaptive(None));
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a reply");
    let r = &reply["result"];
    let err = r["isError"] == json!(true);
    (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
}

#[test]
fn mcp_sets_the_group_refuses_the_entered_tool_undoes_and_the_project_keeps_it() {
    let dir = std::env::temp_dir();
    let stl = dir.join(format!("camcraft-1446-{}.stl", std::process::id()));
    write_stl(&part(), &stl);
    let mut s = Server::new();
    for (i, (name, args)) in [
        ("import_model", json!({ "path": stl.to_string_lossy(), "name": "part" })),
        ("add_tool", json!({ "catalog": SMALL })),
        ("add_operation", json!({ "kind": "2D Adaptive", "tool": SMALL, "source": "part", "name": "A" })),
    ]
    .into_iter()
    .enumerate()
    {
        let (err, body) = call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{name}: {body}");
    }
    let _ = std::fs::remove_file(&stl);
    // An entered previous tool is refused by name, and nothing changes.
    let (err, body) = call(&mut s, 10, "edit_operation", json!({ "name": "A", "params": { "rest": { "previous_tool_diameter": 12 } } }));
    assert!(err && body.to_string().contains(REST_ADAPTIVE_ENTERED), "{body}");
    let (err, body) = call(&mut s, 11, "edit_operation", json!({ "name": "A", "params": { "rest": { "source": "previous_operations", "allowance": 0.2 } } }));
    assert!(!err, "{body}");
    assert!(body["params"].to_string().contains("previous_operations"), "{body}");
    let file = dir.join(format!("camcraft-1446-{}.camproj", std::process::id()));
    let path = file.to_string_lossy().into_owned();
    let (err, body) = call(&mut s, 12, "save_project", json!({ "path": path }));
    assert!(!err, "{body}");
    let mut loaded = Server::new();
    let (err, body) = call(&mut loaded, 1, "open_project", json!({ "path": path }));
    let _ = std::fs::remove_file(&file);
    assert!(!err, "{body}");
    let (_, job) = call(&mut loaded, 2, "get_job", json!({}));
    assert!(job.to_string().contains("previous_operations"), "the reopened project keeps the group: {job}");
    let (err, body) = call(&mut s, 13, "undo", json!({}));
    assert!(!err, "{body}");
    let (_, job) = call(&mut s, 14, "get_job", json!({}));
    assert!(!job.to_string().contains("previous_operations"), "undo removes the group: {job}");
}

//! #1489: rest machining on the 2D Pocket operation. A 60 x 40 pocket
//! with 3 mm corners, roughed by a 12 mm end mill, then a rest pass with
//! a 4 mm end mill that cuts only the four corners (golden CL). Read
//! against the stock simulation of the roughing pass: the rest pass runs
//! no pass over cleared floor and no rapid or plunge into uncut stock.
//! The group is an undoable parameter edit, an MCP parameter and a
//! project-file key that an older file reads as off; a previous tool that
//! is not larger, a strategy the rest pass does not run, and pre-drilled
//! entry beside it are refused by name at the edit.
//!
//! Gate map (#1489): covers app::operation (rest_op, the Pocket arm of
//! generate, params' 0xD7 stamp byte, sane), app::commands (gates'
//! rest_gate, EditOperationParams, EditOperationLinks), app::project
//! (rest_dto, params, params_impl), app::mcp (edit_operation),
//! motion::machining2d (rest2d).
//!
//! Provenance: our own fixtures (pocket_support's block with a pocket);
//! no other CAM product's code, text or output consulted.

mod common;
mod pocket_support;

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::links::Links;
use camcraft_app::machining2d::{PocketStrategy, RestParams};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::Columns;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, recognize};
use common::{gen_to_cltext, write_golden};
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, rounded_rect, write_stl};
use serde_json::{Value, json};

const BIG: &str = "EM-1200-2FL";
const SMALL: &str = "EM-0400-2FL";
const TOP: f64 = 20.0;
const FLOOR: f64 = 16.0;
/// Half sizes of the pocket (60 x 40) and its corner radius.
const A: f64 = 30.0;
const B: f64 = 20.0;
const CORNER: f64 = 3.0;

/// An 80 x 60 x 20 block with the 60 x 40 pocket, 4 mm deep.
fn part() -> camcraft_app::geometry::Mesh {
    block_with_pocket(40.0, 30.0, &rounded_rect(A, B, CORNER, 16), FLOOR, TOP)
}

fn prev() -> RestParams {
    RestParams { previous_tool_diameter: 12.0, previous_corner_radius: 0.0, previous_stock_to_leave: 0.0, allowance: 0.0 }
}

fn pocket(stepover: f64, entry: Entry3D, strategy: PocketStrategy, rest: Option<RestParams>) -> OpParams {
    OpParams::Pocket {
        rest,
        blend_radius: None,
        links: Links::PLANAR,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: Some(Height::fixed(TOP + 2.0)),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 2.0,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        strategy,
        entry,
        helix_radius: 2.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
    }
}

fn rough() -> OpParams {
    pocket(5.0, Entry3D::Helix, PocketStrategy::Waterline, None)
}

fn rest_op(rest: Option<RestParams>) -> OpParams {
    pocket(1.5, Entry3D::Plunge, PocketStrategy::Waterline, rest)
}

/// The part, a stock box round it, both tools; no operation yet.
fn world() -> (World, History, SrcRef) {
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

/// The job: R1 roughs with the 12 mm tool, P2 is the rest pass with
/// the 4 mm tool; both generated.
fn job() -> (World, History, Toolpath, Toolpath) {
    let (world, mut h, src) = world();
    h.execute(&world, CreateOperation::new(&world, "R1", BIG, src.clone(), rough()).expect("rough"));
    h.execute(&world, CreateOperation::new(&world, "P2", SMALL, src, rest_op(Some(prev()))).expect("rest"));
    let r = generate(&world, &mut h, "R1");
    let p = generate(&world, &mut h, "P2");
    (world, h, r, p)
}

fn rest_of(world: &World, name: &str) -> Option<RestParams> {
    let e = model::find_by_name(world, name).expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Pocket { rest, .. } => rest,
        other => panic!("not a Pocket: {other:?}"),
    }
}

fn refusal(r: Result<impl Sized, camcraft_app::commands::CommandError>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a refusal"),
    }
}

/// Distance of `q` from the nearer wall in X and in Y.
fn from_walls(q: Point2) -> (f64, f64) {
    (A - q.x.abs(), B - q.y.abs())
}

fn below_top(m: &Move) -> bool {
    m.from.z < TOP - 1e-9 || m.to.z < TOP - 1e-9
}

#[test]
fn the_rest_pass_cuts_only_the_corners_posts_and_matches_its_golden() {
    let (world, _h, _r, p) = job();
    assert!(p.notes.iter().any(|n| n.contains("rest machining trusts the entered previous tool (12 mm")), "{:?}", p.notes);
    let mut cutting = 0usize;
    for m in p.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        cutting += 1;
        for q in [m.from, m.to] {
            let (dx, dy) = from_walls(Point2::new(q.x, q.y));
            assert!(dx < 8.3 && dy < 8.3, "a cut at ({:.3}, {:.3}) is outside the corners", q.x, q.y);
        }
    }
    assert!(cutting > 8, "the corners are cut");
    let rep = posting::verification_report(&world);
    assert!(rep.problems.is_empty(), "{:?}", rep.problems);
    let out = std::env::temp_dir().join(format!("camcraft-1489-{}.nc", std::process::id()));
    posting::post_setup(&world, &out).expect("the rest pass posts");
    let _ = std::fs::remove_file(&out);
    write_golden("pocket_rest_corners.cl", &gen_to_cltext(&p));
}

/// Signed distance into the pocket's outline (positive inside).
fn depth_in(p: Point2) -> f64 {
    let qx = p.x.abs() - (A - CORNER);
    let qy = p.y.abs() - (B - CORNER);
    CORNER - qx.max(0.0).hypot(qy.max(0.0)) - qx.max(qy).min(0.0)
}

/// Points of the tool's disc about `c` inside the pocket, on a 0.1 mm
/// grid, read `EDGE` short of the rim: a column the rim only grazes (a
/// wall the tool runs along) is not material under the tool.
fn disc(c: Point2, r: f64) -> impl Iterator<Item = Point2> {
    let r = r - EDGE;
    let n = (r / 0.1).ceil() as i32;
    (-n..=n)
        .flat_map(move |i| (-n..=n).map(move |j| Point2::new(c.x + f64::from(i) * 0.1, c.y + f64::from(j) * 0.1)))
        .filter(move |q| (q.x - c.x).hypot(q.y - c.y) <= r && depth_in(*q) > EDGE)
}

/// How far short of the tool's rim (and of the pocket's walls) the
/// stock is read: two lattice cells of the simulation.
const EDGE: f64 = 0.2;

/// The highest stock top under the disc at `c`.
fn top_under(cols: &Columns<'_>, c: Point2, r: f64) -> f64 {
    disc(c, r).filter_map(|q| cols.top_over(q)).filter(|z| z.is_finite()).fold(f64::NEG_INFINITY, f64::max)
}

/// XY samples along a move's chords, 0.25 mm apart.
fn samples(m: &Move) -> Vec<Point2> {
    let mut out = Vec::new();
    for (a, b) in m.chords(0.005) {
        let n = ((b.x - a.x).hypot(b.y - a.y) / 0.25).ceil().max(1.0) as usize;
        out.extend((0..=n).map(|k| {
            let t = k as f64 / n as f64;
            Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
        }));
    }
    out
}

/// XY length of a move.
fn xy_len(m: &Move) -> f64 {
    m.chords(0.005).iter().map(|(a, b)| (b.x - a.x).hypot(b.y - a.y)).sum()
}

#[test]
fn read_against_the_simulated_roughing_pass() {
    let (world, _h, r, p) = job();
    let mut stock = camcraft_app::stock_sim::stock_grid(&world, None, 2.0).expect("the stock grid");
    for m in &r.moves {
        stock.subtract(m, 6.0, 0.005);
    }
    let roughed = stock.clone();
    let after_rough = Columns::of_grid(&roughed);
    let (mut rapids_in, mut plunges_in) = (Vec::new(), Vec::new());
    // Each pass (a run of feeds at one level): (engaged, length, air length).
    let mut runs: Vec<(bool, f64, f64)> = Vec::new();
    let mut open = false;
    for (k, m) in p.moves.iter().enumerate() {
        let now = Columns::of_grid(&stock);
        let flat = (m.from.z - m.to.z).abs() < 1e-9;
        if m.kind == MoveKind::Rapid {
            open = false;
            // Nothing stands under the tool below the rapid's low end.
            let low = m.from.z.min(m.to.z);
            if samples(m).into_iter().any(|s| top_under(&now, s, 2.0) > low + 0.01) {
                rapids_in.push(k);
            }
        } else if m.to.z < m.from.z - 1e-9 {
            open = false;
            // A plunge's axis goes down where this op or the roughing
            // pass already cleared; its rim meets no more than the one
            // level it descends (entry from cleared space is #1490).
            let at = Point2::new(m.to.x, m.to.y);
            let axis = now.top_over(at).unwrap_or(f64::NAN);
            let rim = top_under(&now, at, 2.0);
            if axis.is_nan() || axis > m.to.z + 0.01 || rim > m.from.z.min(TOP) + 0.01 {
                plunges_in.push((k, axis, rim, m.to.z));
            }
        } else if below_top(m) && flat {
            // (The ladder's first level runs at the top of the part, as
            // the plain pocket's does, over no stock: not read here.)
            // A pass meets stock the roughing pass left somewhere.
            let engaged = samples(m).iter().any(|s| top_under(&after_rough, *s, 2.0) > m.from.z + 0.01);
            let len = xy_len(m);
            if !open {
                runs.push((false, 0.0, 0.0));
                open = true;
            }
            if let Some(run) = runs.last_mut() {
                run.0 |= engaged;
                run.1 += len;
                if !engaged {
                    run.2 += len;
                }
            }
        }
        stock.subtract(m, 2.0, 0.005);
    }
    assert!(rapids_in.is_empty(), "rapids into uncut stock: {rapids_in:?}");
    assert!(plunges_in.is_empty(), "plunges into uncut stock: {plunges_in:?}");
    // One ring per corner per level, each cutting the corner without
    // feeding its inner leg over cleared floor (#1816).
    assert_eq!(runs.len(), 8, "{runs:?}");
    assert!(runs.iter().all(|r| r.0 && r.2 < 0.5), "a pass feeds over cleared floor: {runs:?}");
}

#[test]
fn the_group_is_an_undoable_edit_and_the_project_file_keeps_it() {
    let (world, mut h, src) = world();
    h.execute(&world, CreateOperation::new(&world, "P2", SMALL, src, rest_op(None)).expect("create"));
    assert_eq!(rest_of(&world, "P2"), None);
    let on = RestParams { previous_corner_radius: 1.0, previous_stock_to_leave: 0.2, allowance: 0.1, ..prev() };
    h.execute(&world, EditOperationParams::new(&world, "P2", rest_op(Some(on))).expect("edit"));
    assert_eq!(rest_of(&world, "P2"), Some(on));
    h.undo(&world);
    assert_eq!(rest_of(&world, "P2"), None);
    h.redo(&world);
    assert_eq!(rest_of(&world, "P2"), Some(on));
    // The file keeps the group; a pocket without it states no key.
    let e = model::find_by_name(&world, "P2").expect("op");
    let params = model::operation_of(&world, &e).expect("op").params;
    let text = project::params_to_json(&params);
    assert_eq!(text.pointer("/pocket/rest/previous_tool_diameter"), Some(&json!(12.0)), "{text}");
    assert_eq!(text.pointer("/pocket/rest/allowance"), Some(&json!(0.1)), "{text}");
    let back = project::params_from_json(&text).expect("round trip");
    assert_eq!(back, params);
    let off = project::params_to_json(&rest_op(None));
    assert!(off.pointer("/pocket/rest").is_none(), "{off}");
    // An older file (no key) reads as off.
    assert!(matches!(project::params_from_json(&off), Ok(OpParams::Pocket { rest: None, .. })));
}

#[test]
fn a_previous_tool_that_is_not_larger_is_refused_by_name_at_the_edit_and_the_retool() {
    let (world, mut h, src) = world();
    h.execute(&world, CreateOperation::new(&world, "P2", SMALL, src.clone(), rest_op(None)).expect("create"));
    for d in [4.0, 3.0] {
        let msg = refusal(EditOperationParams::new(&world, "P2", rest_op(Some(RestParams { previous_tool_diameter: d, ..prev() }))));
        assert!(msg.contains("needs a previous tool larger than this one"), "{msg}");
        assert!(msg.contains(&format!("the previous tool is {d} mm, this tool is 4 mm")), "{msg}");
    }
    // Made with the group: the same door.
    let msg = refusal(CreateOperation::new(&world, "P3", BIG, src, rest_op(Some(prev()))));
    assert!(msg.contains("needs a previous tool larger than this one"), "{msg}");
    // Re-tooled to the previous tool's own size: the same door.
    h.execute(&world, EditOperationParams::new(&world, "P2", rest_op(Some(prev()))).expect("edit"));
    let msg = refusal(EditOperationLinks::new(&world, "P2", Some(BIG), None));
    assert!(msg.contains("needs a previous tool larger than this one"), "{msg}");
}

#[test]
fn values_and_strategies_the_rest_pass_cannot_run_are_refused_by_name() {
    let (world, mut h, src) = world();
    h.execute(&world, CreateOperation::new(&world, "P2", SMALL, src, rest_op(None)).expect("create"));
    let cases = [
        (RestParams { previous_tool_diameter: 0.0, ..prev() }, "previous tool diameter must be positive"),
        (RestParams { previous_corner_radius: 6.5, ..prev() }, "corner radius"),
        (RestParams { previous_stock_to_leave: -0.1, ..prev() }, "previous stock to leave"),
        (RestParams { allowance: -0.1, ..prev() }, "rest allowance"),
        (RestParams { allowance: f64::NAN, ..prev() }, "must be finite"),
    ];
    for (rest, word) in cases {
        let msg = refusal(EditOperationParams::new(&world, "P2", rest_op(Some(rest))));
        assert!(msg.contains(word), "{rest:?}: {msg}");
        // The parameter door says it without a tool, and a file holding
        // it is not usable.
        let params = rest_op(Some(rest));
        assert!(params.named_refusal().is_some_and(|m| m.contains(word)), "{rest:?}: {:?}", params.named_refusal());
        assert!(!params.is_sane(), "{rest:?}");
    }
    assert!(rest_op(Some(prev())).is_sane() && rest_op(Some(prev())).named_refusal().is_none());
    let with = |strategy: PocketStrategy| pocket(1.5, Entry3D::Plunge, strategy, Some(prev()));
    assert!(EditOperationParams::new(&world, "P2", with(PocketStrategy::Zigzag)).is_ok(), "Zigzag rest machining is supported");
    // #1517: Adaptive is retired on 2D Pocket altogether.
    let msg = refusal(EditOperationParams::new(&world, "P2", with(PocketStrategy::Adaptive)));
    assert!(msg.contains("2D Adaptive"), "{msg}");
    assert!(
        camcraft_app::machining2d::rest_strategy(PocketStrategy::Adaptive)
            .is_err_and(|e| e.to_string().contains("not available with the Adaptive strategy"))
    );
    // Pre-drilled entry beside it: one of the two is turned off.
    let mut both = rest_op(Some(prev()));
    if let Some(l) = both.links_mut() {
        l.approach.predrilled = true;
    }
    let msg = refusal(EditOperationParams::new(&world, "P2", both));
    assert!(msg.contains(camcraft_app::operation::REST_PREDRILLED), "{msg}");
    // Nothing of it was stored.
    assert_eq!(rest_of(&world, "P2"), None);
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("a reply");
    let r = &reply["result"];
    let err = r["isError"] == json!(true);
    (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
}

#[test]
fn mcp_sets_refuses_undoes_and_saves_the_group() {
    let dir = std::env::temp_dir();
    let stl = dir.join(format!("camcraft-1489-{}.stl", std::process::id()));
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
    let (err, body) = call(&mut s, 10, "edit_operation", json!({ "name": "P", "params": { "rest": { "previous_tool_diameter": 3.0 } } }));
    assert!(err, "a previous tool smaller than this one is refused: {body}");
    assert!(body.to_string().contains("needs a previous tool larger than this one"), "{body}");
    let (err, body) = call(
        &mut s,
        11,
        "edit_operation",
        json!({ "name": "P", "params": { "rest": { "previous_tool_diameter": 12.0, "allowance": 0.1 } } }),
    );
    assert!(!err, "{body}");
    assert_eq!(body["params"].pointer("/pocket/rest/previous_tool_diameter"), Some(&json!(12.0)), "{body}");
    assert_eq!(body["params"].pointer("/pocket/rest/allowance"), Some(&json!(0.1)), "{body}");
    let file = dir.join(format!("camcraft-1489-{}.camproj", std::process::id()));
    let path = file.to_string_lossy().into_owned();
    let (err, body) = call(&mut s, 12, "save_project", json!({ "path": path }));
    assert!(!err, "{body}");
    let mut loaded = Server::new();
    let (err, body) = call(&mut loaded, 1, "open_project", json!({ "path": path }));
    let _ = std::fs::remove_file(&file);
    assert!(!err, "{body}");
    let (_, job) = call(&mut loaded, 2, "get_job", json!({}));
    assert!(job.to_string().contains("previous_tool_diameter"), "the reopened project keeps the group: {job}");
    // Off again by a null (RFC 7386), and undo brings it back.
    let (err, body) = call(&mut s, 13, "edit_operation", json!({ "name": "P", "params": { "rest": null } }));
    assert!(!err, "{body}");
    assert!(body["params"].pointer("/pocket/rest").is_none(), "{body}");
    let (err, body) = call(&mut s, 14, "undo", json!({}));
    assert!(!err, "{body}");
    let (_, job) = call(&mut s, 15, "get_job", json!({}));
    assert!(job.to_string().contains("previous_tool_diameter"), "undo restores the group: {job}");
}

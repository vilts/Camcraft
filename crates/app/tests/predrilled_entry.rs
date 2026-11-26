//! #1449 (first slice): a 2-D Pocket with pre-drilled entry plunges down
//! the hole a Drill operation ahead of it in the same setup made - only
//! where the stock the program ahead leaves PROVES the hole open under
//! the tool's disc plus clearance, to the level's depth. A deleted
//! drill, a drill narrower than the tool plus clearance, or a hole
//! shallower than a level keeps the asked helix there, said by name; the
//! posted program carries the straight plunge at the hole centre. The
//! flag is an ordinary parameter edit (undoable) and an MCP field.
//!
//! Gate map (#1449): covers app::operation (predrilled_entry, the Pocket
//! arm of generate, job's drills ahead, the stamp's 0xC7 byte, sane's
//! pocket-only refusal), app::project (approach DTO), app::mcp (edit).
//! Provenance: our own fixtures; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::links::{Approach, Links};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use serde_json::{Value, json};

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const TOP: f64 = 20.0;
/// The pocket's floor (a 20 x 20 closed pocket, 8 mm deep).
const FLOOR: f64 = 12.0;
/// Under the pocket's centre the part has a 6.2 mm square relief down to
/// here, so a drill tip below the pocket floor cuts only air of the part.
const RELIEF_FLOOR: f64 = 2.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// The flat ring at `z` between the squares of half widths `outer` and
/// `inner`, facing up.
fn ring(outer: f64, inner: f64, z: f64, f: &mut Vec<Triangle>) {
    let sq = |h: f64| [(-h, -h), (h, -h), (h, h), (-h, h)];
    let (o, i) = (sq(outer), sq(inner));
    let p = |(x, y): (f64, f64)| Point3::new(x, y, z);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), f);
    }
}

/// A 40 x 40 x [-10, 20] block with the closed pocket and its relief.
fn block() -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let walls = |lo: f64, hi: f64, half: f64, f: &mut Vec<Triangle>| {
        let c = cube(Point3::new(-half, -half, lo), Point3::new(half, half, hi));
        f.extend(c.facets[4..12].iter().map(flip));
    };
    let relief = 3.1;
    ring(20.0, 10.0, TOP, &mut f);
    walls(FLOOR, TOP, 10.0, &mut f);
    ring(10.0, relief, FLOOR, &mut f);
    walls(RELIEF_FLOOR, FLOOR, relief, &mut f);
    let c = cube(Point3::new(-relief, -relief, RELIEF_FLOOR), Point3::new(relief, relief, RELIEF_FLOOR + 1.0));
    f.extend(c.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn world() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", block()));
    let at = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 30.0 }, at).expect("stock"));
    for name in ["DR-0400", "DR-0600", "EM-0400-2FL"] {
        let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    }
    (world, h)
}

/// One hole at the pocket's centre, tip at `tip`.
fn drill(tip: f64, radius: f64) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: tip, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 15.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
        picks: Default::default(),
    }
}

fn pocket(predrilled: bool) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(), rest: None,
        blend_radius: None,
        stepover: 1.6,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(TOP + 15.0),
        retract_z: Some(Height::fixed(TOP + 2.0)),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 4.0,
        feed: 900.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
        links: Links { approach: Approach { predrilled, ..Approach::DEFAULT }, ..Links::PLANAR },
        corner_relief: Default::default(), picks: Default::default(),
    }
}

fn create(world: &World, h: &mut History, name: &str, tool: &str, p: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, SrcRef::Entity("block".into()), p).expect("create"));
}

fn generate(world: &World, h: &mut History, name: &str) -> Toolpath {
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    let tp = model::toolpath_of(world, &e).unwrap_or_else(|| panic!("{name} generated no path"));
    assert!(tp.warnings.is_empty(), "{name}: verify silent: {:?}", tp.warnings);
    tp.path.clone()
}

/// The job: the drill (when given) then the pocket; the pocket's path.
fn job(drill_op: Option<(f64, f64, &str)>, predrilled: bool) -> (World, History, Toolpath) {
    let (world, mut h) = world();
    if let Some((tip, r, tool)) = drill_op {
        create(&world, &mut h, "D1", tool, drill(tip, r));
        generate(&world, &mut h, "D1");
    }
    create(&world, &mut h, "P1", "EM-0400-2FL", pocket(predrilled));
    let t = generate(&world, &mut h, "P1");
    (world, h, t)
}

/// The levels (Z) a straight axial plunge at the pocket's centre
/// reaches: the entry down the hole, and the zigzag's re-entry there
/// onto the column its first half just cleared - each level once.
fn centre_plunges(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none() && m.to.z < m.from.z)
        .filter(|m| [m.from.x, m.from.y, m.to.x, m.to.y].iter().all(|v| v.abs() < 1e-9))
        .map(|m| m.to.z)
        .collect();
    zs.dedup();
    zs
}

/// The first descent onto `z` in the path: a helix half turn, or a
/// straight plunge at the centre.
fn first_entry_is_plunge(t: &Toolpath, z: f64) -> bool {
    t.moves
        .iter()
        .find(|m| m.kind != MoveKind::Rapid && m.to.z < m.from.z && (m.to.z - z).abs() < 1e-9)
        .is_some_and(|m| m.arc.is_none() && m.to.x.abs() < 1e-9 && m.to.y.abs() < 1e-9)
}

fn helix_moves(t: &Toolpath) -> usize {
    t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z - 1e-9).count()
}

fn noted(t: &Toolpath, s: &str) -> bool {
    t.notes.iter().any(|n| n.contains(s))
}

#[test]
fn a_drilled_hole_ahead_is_plunged_to_full_depth_and_posted() {
    // DR-0600 to Z 5: its point is full Ø6 by Z 6.8, far under the floor.
    let (world, _h, t) = job(Some((5.0, 3.0, "DR-0600")), true);
    // #2004: the stock top is TOP, so the ladder starts one stepdown
    // under it (16, then the floor 12); the air level at TOP is gone.
    assert_eq!(centre_plunges(&t), vec![16.0, FLOOR], "{:?}", t.notes);
    assert!([16.0, FLOOR].iter().all(|&z| first_entry_is_plunge(&t, z)));
    assert_eq!(helix_moves(&t), 0, "no helix where the hole serves: {:?}", t.notes);
    assert!(noted(&t, "down the pre-drilled hole at (0.000, 0.000)"), "{:?}", t.notes);
    let nc = std::env::temp_dir().join(format!("camcraft-predrilled-{}.nc", std::process::id()));
    let text = posting::post_script(&world, &nc, FANUC).map(|p| p.program).unwrap_or_else(|e| panic!("posts: {e}"));
    let _ = std::fs::remove_file(&nc);
    // The straight feed down the hole to the floor at X0 Y0, straight
    // from the rapid over it - no helix arc between.
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == "G01 X0. Y0. Z12. F200.").unwrap_or_else(|| panic!("no plunge to Z12 at X0 Y0\n{text}"));
    assert!(at > 0 && lines[at - 1].starts_with("G00 X0. Y0. Z"), "the rapid over the hole, then the plunge: {:?}", &lines[at.saturating_sub(2)..=at]);
}

#[test]
fn without_the_drill_the_helix_stands_and_says_why() {
    let (_w, _h, t) = job(None, true);
    assert!(centre_plunges(&t).is_empty());
    assert!(helix_moves(&t) > 0);
    assert!(noted(&t, "no drilling operation ahead"), "{:?}", t.notes);
}

#[test]
fn deleting_the_drill_takes_the_plunge_away() {
    let (world, mut h, t) = job(Some((5.0, 3.0, "DR-0600")), true);
    assert!(!centre_plunges(&t).is_empty());
    h.execute(&world, DeleteOperation::new(&world, "D1").expect("delete"));
    // The cached plunge is stale at once: the program ahead it was proved
    // against is gone, so nothing posts it until it is regenerated.
    let e = model::find_by_name(&world, "P1").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Stale);
    let t = generate(&world, &mut h, "P1");
    assert!(centre_plunges(&t).is_empty(), "plunged down a hole no longer drilled");
    assert!(helix_moves(&t) > 0);
}

#[test]
fn a_drill_no_wider_than_the_tool_plus_clearance_is_refused_by_name() {
    // DR-0400 into a Ø4 end mill: 4.0 < 4.2 needed.
    let (_w, _h, t) = job(Some((9.0, 2.0, "DR-0400")), true);
    assert!(centre_plunges(&t).is_empty());
    assert!(helix_moves(&t) > 0);
    assert!(noted(&t, "is 4.000 mm across - under the 4.200 mm the 4.000 mm tool needs"), "{:?}", t.notes);
}

#[test]
fn a_shallow_hole_serves_only_the_levels_it_reaches() {
    // Tip at Z 14: full Ø4.2 only above 14 + 2.1 x cot(59) = 15.26 -
    // the Z 16 level plunges, the floor level helixes, said with numbers.
    let (_w, _h, t) = job(Some((14.0, 3.0, "DR-0600")), true);
    // #2004: no level at TOP (the stock top); Z 16 is the first level.
    assert!(first_entry_is_plunge(&t, 16.0));
    assert!(
        !t.moves.iter().any(|m| m.kind != MoveKind::Rapid && m.to.z < m.from.z && (m.to.z - TOP).abs() < 1e-9),
        "no descent onto an air level at TOP"
    );
    assert!(!first_entry_is_plunge(&t, FLOOR), "the floor level is entered by its helix");
    assert!(helix_moves(&t) > 0);
    assert!(t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z).all(|m| m.from.z <= 16.0 + 1e-9));
    assert!(noted(&t, "above this level at Z 12.000"), "{:?}", t.notes);
}

#[test]
fn with_the_flag_off_the_drill_is_never_read() {
    let (_w, _h, t) = job(Some((5.0, 3.0, "DR-0600")), false);
    assert!(centre_plunges(&t).is_empty());
    assert!(!noted(&t, "pre-drilled"), "{:?}", t.notes);
}

#[test]
fn the_flag_is_an_undoable_edit_and_refused_off_the_pocket() {
    let (world, mut h) = world();
    create(&world, &mut h, "P1", "EM-0400-2FL", pocket(false));
    let read = |w: &World| {
        let e = model::find_by_name(w, "P1").expect("op");
        model::operation_of(w, &e).and_then(|o| o.params.links().map(|l| l.approach.predrilled))
    };
    h.execute(&world, EditOperationParams::new(&world, "P1", pocket(true)).expect("edit"));
    assert_eq!(read(&world), Some(true));
    h.undo(&world);
    assert_eq!(read(&world), Some(false));
    h.redo(&world);
    assert_eq!(read(&world), Some(true));
    // The file keeps it.
    let e = model::find_by_name(&world, "P1").expect("op");
    let p = model::operation_of(&world, &e).expect("op").params;
    let back = camcraft_app::project::params_from_json(&camcraft_app::project::params_to_json(&p)).expect("round trip");
    assert_eq!(back.links().map(|l| l.approach.predrilled), Some(true));
    // Another kind never silently drops it.
    let mut off = camcraft_app::operation::seed_params(kind_of("Adaptive2D"));
    if let Some(l) = off.links_mut() {
        l.approach.predrilled = true;
    }
    let err = CreateOperation::new(&world, "A1", "EM-0400-2FL", SrcRef::Entity("block".into()), off)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("pre-drilled entry is read by the 2D Pocket only"), "{err}");
}

fn kind_of(variant: &str) -> usize {
    (0..camcraft_app::operation::KindInfo::ALL.len())
        .find(|&i| format!("{:?}", camcraft_app::operation::seed_params(i).kind()) == variant)
        .unwrap_or_else(|| panic!("no kind {variant}"))
}

#[test]
fn mcp_sets_and_undoes_the_flag() {
    let mut s = Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: Value| -> (bool, Value) {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = s.handle(&msg).expect("a reply");
        let r = &reply["result"];
        let err = r["isError"] == json!(true);
        (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
    };
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    for (name, args) in [
        ("import_model", json!({ "path": model.to_string_lossy(), "name": "part" })),
        ("set_stock", json!({ "from_model": "part" })),
        ("add_tool", json!({ "catalog": "EM-0400-2FL" })),
        ("add_operation", json!({ "kind": "pocket", "tool": "EM-0400-2FL", "source": "part", "name": "P" })),
    ] {
        let (err, body) = call(name, args);
        assert!(!err, "{name}: {body}");
    }
    let (err, body) = call("edit_operation", json!({ "name": "P", "params": { "links": { "approach": { "predrilled": true } } } }));
    assert!(!err, "{body}");
    assert_eq!(body["params"].pointer("/pocket/links/approach/predrilled"), Some(&json!(true)), "{body}");
    let (err, body) = call("undo", json!({}));
    assert!(!err, "{body}");
    let (_, job) = call("get_job", json!({}));
    assert!(!job.to_string().contains("predrilled"), "undone: {job}");
    let (err, _) = call("add_operation", json!({ "kind": "adaptive", "tool": "EM-0400-2FL", "source": "part", "name": "A",
        "params": { "links": { "approach": { "predrilled": true } } } }));
    assert!(err, "an adaptive cannot carry pre-drilled entry yet");
}

//! #1591: 3D Adaptive's Method choice - Spiral (the default, the path
//! the kind always cut) or Trochoidal, which clears every level with
//! trochoidal loops inside the level's own area. A 34 x 16 pocket 6 mm
//! deep is roughed with a 6 mm end mill in two levels: the trochoidal
//! path stays in the pocket and over its floor, the stock sim finds no
//! gouge, it posts, and its CL is pinned. The choice is an undoable
//! parameter edit, an MCP parameter and a project-file word that older
//! files read as Spiral.
//!
//! Provenance: our own fixtures (pocket_support's block with a pocket).
//!
//! Gate map (#1591): covers app::operation (generate, params, kinds),
//! app::project (params, params_impl), app::mcp (edit, generate),
//! motion::machining3d (troch, ladder), motion::machining2d
//! (adaptive_troch_level).

mod common;
mod pocket_support;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{AdaptiveMethod, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use common::{gen_to_cltext, write_golden};
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, rounded_rect, write_stl};
use serde_json::{json, Value};

const TOOL: &str = "EM-0600-4FL";

/// 60 x 40 x 20 block, a sharp 34 x 16 pocket to z 14.
fn part() -> camcraft_app::geometry::Mesh {
    block_with_pocket(30.0, 20.0, &rounded_rect(17.0, 8.0, 0.0, 1), 14.0, 20.0)
}

fn rough(strategy: PocketStrategy, method: AdaptiveMethod) -> OpParams {
    OpParams::Pocket3D {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(25.0)),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
        method,
        picks: Default::default(),
    }
}

fn adaptive(method: AdaptiveMethod) -> OpParams {
    rough(PocketStrategy::Adaptive, method)
}

/// The part in a stock box of its own size, the 6 mm end mill and "R1"
/// on the whole model.
fn job(method: AdaptiveMethod) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part()));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Default::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "R1", TOOL, SrcRef::Entity("part".into()), adaptive(method)).expect("create"));
    (world, h)
}

fn method_of(world: &World) -> AdaptiveMethod {
    let e = model::find_by_name(world, "R1").expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Pocket3D { method, .. } => method,
        other => panic!("not a 3D Pocket: {other:?}"),
    }
}

#[test]
fn the_trochoidal_method_stays_in_the_pocket_posts_clean_and_matches_its_golden() {
    let (world, mut h) = job(AdaptiveMethod::Trochoidal);
    h.execute(&world, GenerateOperation::new(&world, "R1").expect("generates"));
    let e = model::find_by_name(&world, "R1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    println!("notes: {:?}", c.path.notes);
    // The cutter centre stays in the 34 x 16 pocket eroded by the 3 mm
    // radius at every level, and never goes under the pocket floor.
    let worst = c
        .path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| m.segments3(0.01))
        .flat_map(|(a, b)| [a, b])
        .map(|q| (q.x.abs() - 14.0).max(q.y.abs() - 5.0).max(14.0 - q.z))
        .fold(f64::MIN, f64::max);
    assert!(worst <= 2e-3, "the cutter leaves the pocket (or its floor) by {worst:.4} mm");
    assert!(c.path.has_arcs(), "the loops post as arcs");
    for z in [17.0, 14.0] {
        let arcs = c.path.moves.iter().filter(|m| m.arc.is_some() && m.from.z == z && m.to.z == z).count();
        assert!(arcs >= 8, "Z {z}: the level is cut by loops ({arcs} arcs)");
    }
    assert!(
        c.path.notes.iter().any(|n| n.starts_with("3D Adaptive (Trochoidal): 2 level(s) cut by ") && n.contains("engagement")),
        "{:?}",
        c.path.notes
    );
    assert!(!c.path.notes.iter().any(|n| n.contains("instead")), "no level falls back: {:?}", c.path.notes);
    let bad: Vec<_> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    // The stock sim judges the program against the model: no gouge.
    let rep = posting::verification_report(&world);
    assert!(rep.problems.is_empty(), "{:?}", rep.problems);
    let out = std::env::temp_dir().join(format!("camcraft-1591-{}.nc", std::process::id()));
    posting::post_setup(&world, &out).expect("the trochoidal 3D Adaptive posts");
    let _ = std::fs::remove_file(&out);
    write_golden("pocket3d_trochoidal_pocket.cl", &gen_to_cltext(&c.path));
    // The method is part of the operation's stamp under Adaptive: the
    // same operation on Spiral is another program.
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    let op = model::operation_of(&world, &e).expect("operation");
    let stamp = |p: OpParams| operation::current_stamp(&world, TOOL, &op.src, &p, "R1", None).expect("stamp");
    assert_eq!(op.stamp, Some(stamp(adaptive(AdaptiveMethod::Trochoidal))), "the stored stamp is the trochoidal one");
    assert_ne!(stamp(adaptive(AdaptiveMethod::Spiral)), stamp(adaptive(AdaptiveMethod::Trochoidal)), "the method moves the stamp");
    // A strategy that reads no method keeps one stamp for both words.
    assert_eq!(
        stamp(rough(PocketStrategy::OffsetRings, AdaptiveMethod::Spiral)),
        stamp(rough(PocketStrategy::OffsetRings, AdaptiveMethod::Trochoidal)),
        "Offset rings read no method"
    );
    h.execute(&world, EditOperationParams::new(&world, "R1", adaptive(AdaptiveMethod::Spiral)).expect("edit"));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "a method change makes the cached path stale");
}

#[test]
fn the_spiral_default_cuts_what_it_cut_and_another_strategy_reads_no_method() {
    // Spiral: the HSM plan, none of the method's words.
    let (world, mut h) = job(AdaptiveMethod::Spiral);
    h.execute(&world, GenerateOperation::new(&world, "R1").expect("generates"));
    let e = model::find_by_name(&world, "R1").expect("op");
    let spiral = model::toolpath_of(&world, &e).expect("cached path").path.clone();
    assert!(!spiral.notes.iter().any(|n| n.contains("Trochoidal")), "{:?}", spiral.notes);
    // Offset rings under either word: one path.
    let rings = |method| {
        let (world, mut h) = job(AdaptiveMethod::Spiral);
        h.execute(&world, EditOperationParams::new(&world, "R1", rough(PocketStrategy::OffsetRings, method)).expect("edit"));
        h.execute(&world, GenerateOperation::new(&world, "R1").expect("generates"));
        let e = model::find_by_name(&world, "R1").expect("op");
        model::toolpath_of(&world, &e).expect("cached path").path.clone()
    };
    let (a, b) = (rings(AdaptiveMethod::Spiral), rings(AdaptiveMethod::Trochoidal));
    assert_eq!(a.moves, b.moves, "Offset rings read no method");
    assert_eq!(a.notes, b.notes);
}

#[test]
fn the_method_is_an_undoable_parameter_edit() {
    let (world, mut h) = job(AdaptiveMethod::Spiral);
    assert_eq!(method_of(&world), AdaptiveMethod::Spiral);
    h.execute(&world, EditOperationParams::new(&world, "R1", adaptive(AdaptiveMethod::Trochoidal)).expect("edit"));
    assert_eq!(method_of(&world), AdaptiveMethod::Trochoidal);
    assert!(h.undo(&world));
    assert_eq!(method_of(&world), AdaptiveMethod::Spiral, "undo restores the method");
    assert!(h.redo(&world));
    assert_eq!(method_of(&world), AdaptiveMethod::Trochoidal, "redo applies it again");
}

#[test]
fn a_new_3d_pocket_is_spiral_and_an_older_file_reads_as_spiral() {
    let kind = operation::KindInfo::ALL.iter().position(|k| k.label == "3D Pocket").expect("the kind");
    let seeded = operation::seed_params(kind);
    assert!(matches!(seeded, OpParams::Pocket3D { method: AdaptiveMethod::Spiral, .. }), "{seeded:?}");
    // A file written before the word existed: no "method".
    let mut old = project::params_to_json(&adaptive(AdaptiveMethod::Spiral));
    let fields = old.get_mut("pocket3d").and_then(Value::as_object_mut).expect("the kind's grammar");
    assert_eq!(fields.remove("method"), Some(json!("spiral")), "the default is written as a word");
    let read = project::params_from_json(&old).expect("an older file reads");
    assert_eq!(read, adaptive(AdaptiveMethod::Spiral), "absent means Spiral, every other number as it was");
    // The trochoidal word survives the file.
    let saved = project::params_to_json(&adaptive(AdaptiveMethod::Trochoidal));
    assert_eq!(saved["pocket3d"]["method"], json!("trochoidal"));
    assert_eq!(project::params_from_json(&saved).expect("reads back"), adaptive(AdaptiveMethod::Trochoidal));
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

#[test]
fn mcp_sets_the_method_generates_verifies_and_refuses_an_unknown_word() {
    let dir = std::env::temp_dir().join(format!("camcraft-1591-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let stl = dir.join("part.stl");
    write_stl(&part(), &stl);
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }))));
    assert!(ok(&call(&mut s, 2, "set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 3, "add_tool", json!({ "catalog": TOOL }))));
    let add = call(&mut s, 4, "add_operation", json!({ "kind": "3D Pocket", "tool": TOOL, "name": "R1", "source": "part",
        "params": { "strategy": "adaptive", "top_z": { "reference": "fixed", "offset": 20.0 },
            "bottom_z": { "reference": "fixed", "offset": 14.0 }, "stepdown": 3.0 } }));
    assert!(ok(&add), "{add}");
    // The edit is one parameter.
    let edit = call(&mut s, 5, "edit_operation", json!({ "name": "R1", "params": { "method": "trochoidal" } }));
    assert!(ok(&edit), "{edit}");
    assert_eq!(edit["structuredContent"]["params"]["pocket3d"]["method"], json!("trochoidal"), "{edit}");
    let bad = call(&mut s, 6, "edit_operation", json!({ "name": "R1", "params": { "method": "zigzag" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown method is refused: {bad}");
    assert!(bad.to_string().contains("bad params patch"), "{bad}");
    let generated = call(&mut s, 7, "generate", json!({ "name": "R1" }));
    assert!(ok(&generated), "{generated}");
    assert!(generated.to_string().contains("3D Adaptive (Trochoidal)"), "the notes say the method: {generated}");
    let verify = call(&mut s, 8, "verify", json!({}));
    assert_eq!(verify["structuredContent"]["ok"], json!(true), "{verify}");
    // Back to the default by the same parameter, and undo brings the loops back.
    let back = call(&mut s, 9, "edit_operation", json!({ "name": "R1", "params": { "method": "spiral" } }));
    assert_eq!(back["structuredContent"]["params"]["pocket3d"]["method"], json!("spiral"), "{back}");
    assert!(ok(&call(&mut s, 10, "undo", json!({}))));
    let ops = call(&mut s, 11, "get_job", json!({})).to_string();
    assert!(ops.contains("trochoidal"), "undo restores the method: {ops}");
    let _ = std::fs::remove_dir_all(&dir);
}

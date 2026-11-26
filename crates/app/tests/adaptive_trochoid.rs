//! #1509: 2D Adaptive's Method choice - Spiral (the default, the path
//! the kind always cut) or Trochoidal, which loops inside the selected
//! region. A pocket floor face is picked (a slim twin of #1503's
//! screenshot: 34 x 16, a 6 mm end mill): the trochoidal path stays in
//! the face, the stock sim finds no gouge, it posts, and its CL is
//! pinned. The choice is an undoable parameter edit, an MCP parameter
//! and a project-file word that older files read as Spiral; a face too
//! narrow to loop in is refused by name, GUI command and MCP alike.
//!
//! Provenance: our own fixtures (pocket_support's block with a pocket).
//!
//! Gate map (#1509): covers app::operation (generate, params, kinds),
//! app::project (params, params_impl), app::mcp (edit, generate),
//! motion::machining2d (adaptive_troch, adaptive_method, trochoid_bore).

mod common;
mod pocket_support;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::links::Links;
use camcraft_app::machining2d::AdaptiveMethod;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, recognize};
use common::{gen_to_cltext, write_golden};
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, height_field, rounded_rect, write_stl};
use serde_json::{json, Value};

const TOOL: &str = "EM-0600-4FL";

/// 60 x 40 x 20 block, a sharp pocket `2a x 2b` to z 17.
fn part(a: f64, b: f64) -> camcraft_app::geometry::Mesh {
    block_with_pocket(30.0, 20.0, &rounded_rect(a, b, 0.0, 1), 17.0, 20.0)
}

fn adaptive(method: AdaptiveMethod) -> OpParams {
    OpParams::Adaptive2D {
        rest: None,
        blend_radius: None, blend_turns: false,
        links: Links::PLANAR,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(25.0)),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(17.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        method,
        troch: camcraft_app::machining2d::AdaptiveTrochParams::default(),
        tilt: None,
        corner_relief: Default::default(),
        pass_start: Default::default(),
        picks: Default::default(),
    }
}

fn job_mesh(mesh: camcraft_app::geometry::Mesh, dx: f64, dy: f64, method: AdaptiveMethod) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz: 20.0 }, Default::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    let p = recognize::pockets_of(&world, "part").expect("recognised").into_iter().next().expect("the pocket");
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(p.floor) }]);
    h.execute(&world, CreateOperation::new(&world, "A1", TOOL, src, adaptive(method)).expect("create"));
    (world, h)
}

/// The part, a stock box round it, the 6 mm end mill and "A1" on the
/// pocket's floor face.
fn job(a: f64, b: f64, method: AdaptiveMethod) -> (World, History) {
    job_mesh(part(a, b), 70.0, 50.0, method)
}

fn check_troch_job(mesh: camcraft_app::geometry::Mesh, dx: f64, dy: f64, golden: &str) {
    let (world, mut h) = job_mesh(mesh, dx, dy, AdaptiveMethod::Trochoidal);
    h.execute(&world, GenerateOperation::new(&world, "A1").expect("generates"));
    let e = model::find_by_name(&world, "A1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    let min_z = c
        .path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| [m.from.z, m.to.z])
        .fold(f64::INFINITY, f64::min);
    assert!(min_z >= 17.0 - 1e-6, "never under pocket floor: z={min_z:.4}");
    assert!(c.path.has_arcs(), "the loops post as arcs");
    assert!(c.path.notes.iter().any(|n| n.contains("2D Adaptive (Trochoidal)") && n.contains("engagement")), "{:?}", c.path.notes);
    let bad: Vec<_> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    let rep = posting::verification_report(&world);
    assert!(rep.problems.is_empty(), "{:?}", rep.problems);
    let out = std::env::temp_dir().join(format!("camcraft-1587-{golden}-{}.nc", std::process::id()));
    posting::post_setup(&world, &out).expect("the trochoidal 2D Adaptive posts");
    let _ = std::fs::remove_file(&out);
    write_golden(golden, &gen_to_cltext(&c.path));
}

fn method_of(world: &World) -> AdaptiveMethod {
    let e = model::find_by_name(world, "A1").expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Adaptive2D { method, .. } => method,
        other => panic!("not a 2D Adaptive: {other:?}"),
    }
}

#[test]
fn the_trochoidal_method_stays_in_the_picked_face_posts_clean_and_matches_its_golden() {
    let (world, mut h) = job(17.0, 8.0, AdaptiveMethod::Trochoidal);
    h.execute(&world, GenerateOperation::new(&world, "A1").expect("generates"));
    let e = model::find_by_name(&world, "A1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    // The cutter centre stays in the 34 x 16 face eroded by the 3 mm
    // radius, and never goes under the pocket floor.
    let worst = c
        .path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| m.segments3(0.01))
        .flat_map(|(a, b)| [a, b])
        .map(|q| (q.x.abs() - 14.0).max(q.y.abs() - 5.0).max(17.0 - q.z))
        .fold(f64::MIN, f64::max);
    assert!(worst <= 2e-3, "the cutter leaves the selected face (or its floor) by {worst:.4} mm");
    assert!(c.path.has_arcs(), "the loops post as arcs");
    assert!(c.path.notes.iter().any(|n| n.contains("2D Adaptive (Trochoidal)") && n.contains("engagement")), "{:?}", c.path.notes);
    let bad: Vec<_> = c.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(bad.is_empty(), "{bad:?}");
    // The stock sim judges the program against the model: no gouge.
    let rep = posting::verification_report(&world);
    assert!(rep.problems.is_empty(), "{:?}", rep.problems);
    let out = std::env::temp_dir().join(format!("camcraft-1509-{}.nc", std::process::id()));
    posting::post_setup(&world, &out).expect("the trochoidal 2D Adaptive posts");
    let _ = std::fs::remove_file(&out);
    write_golden("adaptive2d_trochoidal_pocket_face.cl", &gen_to_cltext(&c.path));
    // The method is part of the operation's stamp: the same operation
    // on Spiral is another program, so cached loops can never read as
    // current for it.
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
    let op = model::operation_of(&world, &e).expect("operation");
    let stamp = |m| operation::current_stamp(&world, TOOL, &op.src, &adaptive(m), "A1", None).expect("stamp");
    assert_eq!(op.stamp, Some(stamp(AdaptiveMethod::Trochoidal)), "the stored stamp is the trochoidal one");
    assert_ne!(stamp(AdaptiveMethod::Spiral), stamp(AdaptiveMethod::Trochoidal), "the method moves the stamp");
    h.execute(&world, EditOperationParams::new(&world, "A1", adaptive(AdaptiveMethod::Spiral)).expect("edit"));
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "a method change makes the cached path stale");
}

#[test]
fn wide_rectangular_pocket_posts_clean_and_matches_golden() {
    let mesh = block_with_pocket(40.0, 30.0, &rounded_rect(30.0, 20.0, 0.0, 1), 17.0, 20.0);
    check_troch_job(mesh, 90.0, 70.0, "adaptive2d_trochoidal_wide.cl");
}

#[test]
fn l_shaped_pocket_posts_clean_and_matches_golden() {
    let mesh = height_field(
        &[-40.0, -30.0, 0.0, 30.0, 40.0],
        &[-40.0, -30.0, 0.0, 30.0, 40.0],
        |i, j| if (1..=2).contains(&i) && (1..=2).contains(&j) && !(i == 2 && j == 2) { 17.0 } else { 20.0 },
    );
    check_troch_job(mesh, 90.0, 90.0, "adaptive2d_trochoidal_l_shape.cl");
}

#[test]
fn pocket_with_island_posts_clean_and_matches_golden() {
    let mesh = height_field(
        &[-40.0, -33.0, -6.0, 6.0, 33.0, 40.0],
        &[-30.0, -23.0, -6.0, 6.0, 23.0, 30.0],
        |i, j| if (1..=3).contains(&i) && (1..=3).contains(&j) && !(i == 2 && j == 2) { 17.0 } else { 20.0 },
    );
    check_troch_job(mesh, 90.0, 70.0, "adaptive2d_trochoidal_island.cl");
}

#[test]
fn a_face_too_narrow_to_loop_in_is_refused_by_name_and_nothing_posts() {
    // 34 x 6.2: the 6 mm tool fits, with 0.1 mm a side to loop in.
    let (world, _h) = job(17.0, 3.1, AdaptiveMethod::Trochoidal);
    let err = GenerateOperation::new(&world, "A1").map(|_| ()).expect_err("refused").to_string();
    assert!(err.contains("2D Adaptive (Trochoidal)") && err.contains("too narrow for a 6.000 mm tool to loop in"), "{err}");
    let e = model::find_by_name(&world, "A1").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "no path is cached for a refused region");
    let out = std::env::temp_dir().join(format!("camcraft-1509-narrow-{}.nc", std::process::id()));
    assert!(posting::post_setup(&world, &out).is_err(), "nothing posts");
    assert!(!out.exists());
}

#[test]
fn the_method_is_an_undoable_parameter_edit() {
    let (world, mut h) = job(17.0, 8.0, AdaptiveMethod::Spiral);
    assert_eq!(method_of(&world), AdaptiveMethod::Spiral);
    h.execute(&world, EditOperationParams::new(&world, "A1", adaptive(AdaptiveMethod::Trochoidal)).expect("edit"));
    assert_eq!(method_of(&world), AdaptiveMethod::Trochoidal);
    assert!(h.undo(&world));
    assert_eq!(method_of(&world), AdaptiveMethod::Spiral, "undo restores the method");
    assert!(h.redo(&world));
    assert_eq!(method_of(&world), AdaptiveMethod::Trochoidal, "redo applies it again");
}

#[test]
fn a_new_2d_adaptive_is_spiral_and_an_older_file_reads_as_spiral() {
    let kind = operation::KindInfo::ALL.iter().position(|k| k.label == "2D Adaptive").expect("the kind");
    let seeded = operation::seed_params(kind);
    assert!(matches!(seeded, OpParams::Adaptive2D { method: AdaptiveMethod::Spiral, .. }), "{seeded:?}");
    // A file written before the word existed: no "method".
    let mut old = project::params_to_json(&adaptive(AdaptiveMethod::Spiral));
    let fields = old.get_mut("adaptive2_d").and_then(Value::as_object_mut).expect("the kind's grammar");
    assert_eq!(fields.remove("method"), Some(json!("spiral")), "the default is written as a word");
    let read = project::params_from_json(&old).expect("an older file reads");
    assert_eq!(read, adaptive(AdaptiveMethod::Spiral), "absent means Spiral, every other number as it was");
    // The trochoidal word survives the file.
    let saved = project::params_to_json(&adaptive(AdaptiveMethod::Trochoidal));
    assert_eq!(saved["adaptive2_d"]["method"], json!("trochoidal"));
    assert_eq!(project::params_from_json(&saved).expect("reads back"), adaptive(AdaptiveMethod::Trochoidal));
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

#[test]
fn mcp_sets_the_method_generates_inside_the_face_and_refuses_an_unknown_word() {
    let dir = std::env::temp_dir().join(format!("camcraft-1509-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let stl = dir.join("part.stl");
    write_stl(&part(17.0, 8.0), &stl);
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }))));
    assert!(ok(&call(&mut s, 2, "set_stock", json!({ "box": { "dx": 70.0, "dy": 50.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 3, "add_tool", json!({ "catalog": TOOL }))));
    let floor = call(&mut s, 4, "detect_pockets", json!({ "model": "part" }))["structuredContent"]["features"][0]["floor"].clone();
    let add = call(&mut s, 5, "add_operation", json!({ "kind": "2D Adaptive", "tool": TOOL, "name": "A1", "source": "part",
        "params": { "top_z": { "reference": "fixed", "offset": 20.0 }, "bottom_z": { "reference": "fixed", "offset": 17.0 }, "stepdown": 3.0 } }));
    assert!(ok(&add), "{add}");
    let set = call(&mut s, 6, "set_operation_geometry", json!({ "operation": "A1", "picks": [{ "model": "part", "id": floor }] }));
    assert!(ok(&set), "{set}");
    // A new op says its method; the edit is one parameter.
    let edit = call(&mut s, 7, "edit_operation", json!({ "name": "A1", "params": { "method": "trochoidal" } }));
    assert!(ok(&edit), "{edit}");
    assert_eq!(edit["structuredContent"]["params"]["adaptive2_d"]["method"], json!("trochoidal"), "{edit}");
    let bad = call(&mut s, 8, "edit_operation", json!({ "name": "A1", "params": { "method": "zigzag" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown method is refused: {bad}");
    assert!(bad.to_string().contains("bad params patch"), "{bad}");
    let generated = call(&mut s, 9, "generate", json!({ "name": "A1" }));
    assert!(ok(&generated), "{generated}");
    let verify = call(&mut s, 10, "verify", json!({}));
    assert_eq!(verify["structuredContent"]["ok"], json!(true), "{verify}");
    // Back to the default by the same parameter, and undo brings the loops back.
    let back = call(&mut s, 11, "edit_operation", json!({ "name": "A1", "params": { "method": "spiral" } }));
    assert_eq!(back["structuredContent"]["params"]["adaptive2_d"]["method"], json!("spiral"), "{back}");
    assert!(ok(&call(&mut s, 12, "undo", json!({}))));
    let ops = call(&mut s, 13, "get_job", json!({})).to_string();
    assert!(ops.contains("trochoidal"), "undo restores the method: {ops}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mcp_edits_trochoidal_parameters_generates_refuses_limits_and_round_trips() {
    let dir = std::env::temp_dir().join(format!("camcraft-1588-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let stl = dir.join("part.stl");
    write_stl(&part(17.0, 8.0), &stl);
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "import_model", json!({ "path": stl.display().to_string() }))));
    assert!(ok(&call(&mut s, 2, "set_stock", json!({ "box": { "dx": 70.0, "dy": 50.0, "dz": 20.0 } }))));
    assert!(ok(&call(&mut s, 3, "add_tool", json!({ "catalog": TOOL }))));
    let floor = call(&mut s, 4, "detect_pockets", json!({ "model": "part" }))["structuredContent"]["features"][0]["floor"].clone();
    assert!(ok(&call(
        &mut s,
        5,
        "add_operation",
        json!({
            "kind": "2D Adaptive",
            "tool": TOOL,
            "name": "A1",
            "source": "part",
            "params": {
                "top_z": { "reference": "fixed", "offset": 20.0 },
                "bottom_z": { "reference": "fixed", "offset": 17.0 },
                "stepdown": 3.0
            }
        })
    )));
    assert!(ok(&call(&mut s, 6, "set_operation_geometry", json!({ "operation": "A1", "picks": [{ "model": "part", "id": floor }] }))));

    let edit = call(
        &mut s,
        7,
        "edit_operation",
        json!({
            "name": "A1",
            "params": {
                "method": "trochoidal",
                "max_engagement": 60.0,
                "return": "retract",
                "return_feed": 1800.0,
                "loop_width": 4.0
            }
        }),
    );
    assert!(ok(&edit), "edit_operation should accept the trochoidal parameters: {edit}");
    let p = &edit["structuredContent"]["params"]["adaptive2_d"];
    assert_eq!(p["max_engagement"], json!(60.0), "{p}");
    assert_eq!(p["return"], json!("retract"), "{p}");
    assert_eq!(p["return_feed"], json!(1800.0), "{p}");
    assert_eq!(p["loop_width"], json!(4.0), "{p}");

    let generated = call(&mut s, 8, "generate", json!({ "name": "A1" }));
    assert!(ok(&generated), "{generated}");
    let verify = call(&mut s, 9, "verify", json!({}));
    assert_eq!(verify["structuredContent"]["ok"], json!(true), "{verify}");

    let tall_lift = call(
        &mut s,
        10,
        "edit_operation",
        json!({ "name": "A1", "params": { "return": "microlift", "lift": 50.0 } }),
    );
    assert_eq!(tall_lift["isError"], json!(true), "tall microlift must be refused: {tall_lift}");
    assert!(tall_lift.to_string().contains("microlift"), "{tall_lift}");

    let good_lift = call(
        &mut s,
        11,
        "edit_operation",
        json!({
            "name": "A1",
            "params": { "max_engagement": 90.0, "return": "microlift", "lift": 0.4, "return_feed": null, "loop_width": null }
        }),
    );
    assert!(ok(&good_lift), "{good_lift}");
    let good_lift_gen = call(&mut s, 12, "generate", json!({ "name": "A1" }));
    assert!(ok(&good_lift_gen), "{good_lift_gen}");

    let tight = call(&mut s, 13, "edit_operation", json!({ "name": "A1", "params": { "max_engagement": 10.0 } }));
    assert!(ok(&tight), "{tight}");
    let refused = call(&mut s, 14, "generate", json!({ "name": "A1" }));
    assert_eq!(refused["isError"], json!(true), "unachievable engagement limit must be refused: {refused}");
    let err_text = refused.to_string();
    assert!(err_text.contains("2D Adaptive (Trochoidal)") && err_text.contains("engagement limit"), "{err_text}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn trochoidal_parameters_undo_redo_and_round_trip_with_older_files_defaulting() {
    let (world, mut h) = job(17.0, 8.0, AdaptiveMethod::Trochoidal);
    let base = adaptive(AdaptiveMethod::Trochoidal);
    let mut edited = base.clone();
    if let OpParams::Adaptive2D { troch, .. } = &mut edited {
        troch.max_engagement = 65.0;
        troch.ret = Some(camcraft_app::machining2d::TrochoidReturn::Microlift { height: 0.5 });
        troch.return_feed = Some(1500.0);
        troch.loop_width = Some(3.5);
    }
    h.execute(&world, EditOperationParams::new(&world, "A1", edited.clone()).expect("edit"));
    let e = model::find_by_name(&world, "A1").expect("op");
    assert_eq!(model::operation_of(&world, &e).expect("op").params, edited);
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").params, base, "undo restores defaults");
    assert!(h.redo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").params, edited, "redo applies trochoidal params");

    let saved = project::params_to_json(&edited);
    assert_eq!(project::params_from_json(&saved).expect("roundtrip"), edited);

    let mut legacy = project::params_to_json(&base);
    let obj = legacy.get_mut("adaptive2_d").and_then(Value::as_object_mut).expect("obj");
    for k in ["max_engagement", "return", "lift", "return_feed", "loop_width"] {
        obj.remove(k);
    }
    assert_eq!(project::params_from_json(&legacy).expect("older file"), base);
}

#[test]
fn trochoidal_doors_refuse_out_of_range_and_over_clearance_values_by_name() {
    let (world, _h) = job(17.0, 8.0, AdaptiveMethod::Trochoidal);
    let mut bad_eng = adaptive(AdaptiveMethod::Trochoidal);
    if let OpParams::Adaptive2D { troch, .. } = &mut bad_eng {
        troch.max_engagement = 181.0;
    }
    let err = EditOperationParams::new(&world, "A1", bad_eng).map(|_| ()).expect_err("bad max_engagement").to_string();
    assert!(err.contains("max engagement"), "{err}");

    let mut tall = adaptive(AdaptiveMethod::Trochoidal);
    if let OpParams::Adaptive2D { troch, .. } = &mut tall {
        // clearance is 30, top is 20 -> 10 mm room.
        troch.ret = Some(camcraft_app::machining2d::TrochoidReturn::Microlift { height: 10.5 });
    }
    let err = EditOperationParams::new(&world, "A1", tall).map(|_| ()).expect_err("tall microlift").to_string();
    assert!(err.contains("microlift") && err.contains("clearance plane") && err.contains("10.000"), "{err}");
}

#[test]
fn merged_arc_and_trochoidal_parameters_round_trip_and_keep_the_arc_refusal() {
    let (world, _h) = job(17.0, 8.0, AdaptiveMethod::Trochoidal);
    let e = model::find_by_name(&world, "A1").expect("op");
    let mut op = model::operation_of(&world, &e).expect("operation");
    if let OpParams::Adaptive2D { blend_turns, blend_radius, troch, .. } = &mut op.params {
        *blend_turns = true;
        *blend_radius = Some(0.75);
        troch.max_engagement = 65.0;
        troch.ret = Some(camcraft_app::machining2d::TrochoidReturn::Microlift { height: 0.4 });
        troch.return_feed = Some(1500.0);
        troch.loop_width = Some(3.5);
    }
    let saved = project::params_to_json(&op.params);
    let fields = &saved["adaptive2_d"];
    assert_eq!(fields["blend_turns"], json!(true));
    assert_eq!(fields["blend_radius"], json!(0.75));
    assert_eq!(fields["max_engagement"], json!(65.0));
    assert_eq!(fields["return"], json!("microlift"));
    assert_eq!(fields["lift"], json!(0.4));
    assert_eq!(fields["return_feed"], json!(1500.0));
    assert_eq!(fields["loop_width"], json!(3.5));
    assert_eq!(project::params_from_json(&saved).expect("both parameter sets read back"), op.params);
    let err = EditOperationParams::new(&world, "A1", op.params.clone()).map(|_| ()).expect_err("edit refuses arc turns").to_string();
    assert!(err.contains("arc turns") && err.contains("Trochoidal"), "{err}");
    // Bypass command validation: the generation arm must independently
    // refuse main's unsupported option rather than silently dropping it.
    let err = operation::generate(&world, &op, "A1").map(|_| ()).expect_err("generation refuses arc turns").to_string();
    assert!(err.contains("arc turns") && err.contains("Trochoidal"), "{err}");
}

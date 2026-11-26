//! #1445, #1479: the fine-boring cycle as a drilling operation's
//! parameter - the undoable command with its required shift, the shift's
//! door at command creation, the generated chain, the project file, the
//! MCP `edit_operation` door, and the post gate: Haas writes G76, a post
//! that does not declare the cycle refuses it by name, and a machine that
//! cannot stop its spindle oriented refuses it whatever the post.
//!
//! Gate map: covers app::commands (CreateOperation, EditOperationParams,
//! History), app::operation (OpParams::fine_bore_refusal, drill
//! generation), app::project (the cycle DTOs), app::posting (the post
//! gate), app::machine (check_document), app::mcp (edit_operation).
//!
//! Provenance: the published G76 fine boring canned cycle (Haas Mill
//! Operator's Manual, G76, words I and J) for the cycle's shape; our own
//! command and project-file laws for the rest. No vendor material.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::world::new_world;

const OP: &str = "FineBore1";
const TOOL: &str = "FB-2000";
/// The bore: dia. 20, through a 10 mm puck.
const HOLE_R: f64 = 10.0;

/// A dia. 60 x 10 mm puck with a through dia. 20 hole on its axis.
fn puck() -> Mesh {
    let meridian = [(HOLE_R, 0.0), (30.0, 0.0), (30.0, 10.0), (HOLE_R, 10.0), (HOLE_R, 0.0)];
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..32 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            facets.push(Triangle::new(a1, a2, b2));
            facets.push(Triangle::new(a1, b2, b1));
        }
    }
    Mesh::from_facets(facets)
}

/// A dia. 20 boring bar on a dia. `bar` shank.
fn bar(bar: f64) -> Tool {
    let mut t = catalog().into_iter().find(|(n, _)| *n == "BR-1000").map(|(_, t)| t).expect("catalog BR-1000");
    t.kind = ToolKind::Bore;
    t.diameter = 2.0 * HOLE_R;
    t.shank_d = bar;
    t
}

fn params(cycle: CycleChoice, dwell: f64) -> OpParams {
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell,
        feed: 200.0,
        plunge_feed: 60.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0),
            radius: HOLE_R,
            entry_z: 10.0,
            floor_z: 0.0,
            through: true,
            stepped: false,
        }],
        cross: Vec::new(),
        picks: Default::default(),
    }
}

fn fine(dx: f64, dy: f64) -> CycleChoice {
    CycleChoice::fine_bore(dx, dy).expect("a usable shift")
}

/// The puck in its stock with the bar on the rack, no machine mounted.
fn world_with(tool: Tool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = StockShape::Cylinder { diameter: 60.0, height: 10.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("puck", puck()));
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add bar"));
    (world, h)
}

fn cycle_of(world: &World) -> CycleChoice {
    let e = model::find_by_name(world, OP).expect("operation entity");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Drill { cycle, .. } => cycle,
        _ => panic!("a drill operation stays one"),
    }
}

fn cache(world: &World) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, OP).expect("operation entity");
    model::toolpath_of(world, &e).expect("generated toolpath")
}

/// The refusal text of a generation that must not make a path.
fn generation_refusal(world: &World, h: &mut History) -> String {
    match GenerateOperation::new(world, OP) {
        Err(e) => format!("{e:?}"),
        Ok(cmd) => {
            h.execute(world, cmd);
            let e = model::find_by_name(world, OP).expect("operation entity");
            format!("{:?}", model::operation_of(world, &e).expect("operation"))
        }
    }
}

#[test]
fn fine_bore_is_an_undoable_drill_parameter_that_generates_the_shift_and_round_trips() {
    let (world, mut h) = world_with(bar(16.0));
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src, params(CycleChoice::Bore, 0.0)).expect("create"));
    let edit = EditOperationParams::new(&world, OP, params(fine(0.3, -0.4), 0.5)).expect("edit to fine bore");
    h.execute(&world, edit);
    assert_eq!(cycle_of(&world), fine(0.3, -0.4));
    h.execute(&world, GenerateOperation::new(&world, OP).expect("generate fine bore"));

    let tp = cache(&world);
    assert!(tp.warnings.is_empty(), "the verifier accepts the shifted retract: {:?}", tp.warnings);
    // The `.cl` text names the cycle and reads it back.
    let cl = camcraft_app::cl::text::to_text(&tp.path).expect("cl text");
    assert!(cl.contains("; CYCLE FineBore "), "{cl}");
    assert_eq!(camcraft_app::cl::text::from_text(&cl).expect("cl reads back").cycles, tp.path.cycles);
    let t = tp.path;
    assert_eq!(t.cycles.len(), 1);
    let c = t.cycles[0];
    assert_eq!((c.kind, c.dwell), (CycleKind::FineBore, 0.5));
    let g = &t.moves[c.first_move..];
    assert_eq!((g[0].kind, g[0].feed), (MoveKind::Plunge, Some(60.0)), "feed in");
    assert_eq!((g[1].kind, g[1].to.x, g[1].to.y), (MoveKind::Rapid, 0.3, -0.4), "the shift");
    assert!((g[1].to.z - g[0].to.z).abs() < 1e-9, "the shift is at the bottom");
    assert_eq!((g[2].kind, g[2].to.x, g[2].to.y, g[2].to.z), (MoveKind::Rapid, 0.3, -0.4, 12.0), "rapid out, shifted");
    assert_eq!((g[3].kind, g[3].to.x, g[3].to.y, g[3].to.z), (MoveKind::Rapid, 0.0, 0.0, 12.0), "shift back at R");

    let file = std::env::temp_dir().join(format!("camcraft-fine-bore-{}.camproj", std::process::id()));
    project::save(&world, &file).expect("save");
    let saved = std::fs::read_to_string(&file).expect("read saved project");
    let v: Value = serde_json::from_str(&saved).expect("project json");
    let text = v.to_string();
    assert!(text.contains(r#""cycle":{"fine_bore":{"shift_x":0.3,"shift_y":-0.4}}"#), "params spelling: {saved}");
    assert!(saved.contains("\"kind\": \"fine_bore\""), "cached cycle spelling: {saved}");
    let loaded = new_world();
    project::load(&loaded, &file).expect("load");
    let _ = std::fs::remove_file(&file);
    assert_eq!(cycle_of(&loaded), fine(0.3, -0.4));
    assert_eq!(cache(&loaded).path.cycles[0].kind, CycleKind::FineBore);

    assert!(h.undo(&world), "undo generation");
    assert!(h.undo(&world), "undo the fine-bore edit");
    assert_eq!(cycle_of(&world), CycleChoice::Bore, "undo restores the plain bore");
    assert!(h.redo(&world), "redo the fine-bore edit");
    assert_eq!(cycle_of(&world), fine(0.3, -0.4));
}

#[test]
fn the_shift_is_in_the_operation_stamp() {
    let (world, mut h) = world_with(bar(16.0));
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src, params(fine(0.3, 0.0), 0.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, OP).expect("generate"));
    let e = model::find_by_name(&world, OP).expect("operation entity");
    let op = model::operation_of(&world, &e).expect("operation");
    let stamp = |cycle: CycleChoice| {
        camcraft_app::operation::current_stamp(&world, TOOL, &op.src, &params(cycle, 0.0), OP, None).expect("stamp")
    };
    assert_eq!(op.stamp, Some(stamp(fine(0.3, 0.0))), "the stored stamp is this shift's");
    // The same cycle with the shift the other way is a different program.
    assert_ne!(stamp(fine(-0.3, 0.0)), stamp(fine(0.3, 0.0)), "the shift along X moves the stamp");
    assert_ne!(stamp(fine(0.3, 0.1)), stamp(fine(0.3, 0.0)), "the shift along Y moves the stamp");
}

#[test]
fn a_fine_bore_with_no_usable_shift_is_refused_when_the_command_is_made() {
    let (world, mut h) = world_with(bar(16.0));
    let src = || SrcRef::Entity("puck".into());
    let made = |cycle: CycleChoice| {
        CreateOperation::new(&world, OP, TOOL, src(), params(cycle, 0.0)).map(|_| ()).map_err(|e| format!("{e:?}"))
    };
    let zero = made(CycleChoice::FineBore { shift_x_um: 0, shift_y_um: 0 }).unwrap_err();
    assert!(zero.contains("fine bore needs a shift"), "{zero}");
    let huge = made(CycleChoice::FineBore { shift_x_um: 5_001, shift_y_um: 0 }).unwrap_err();
    assert!(huge.contains("fine bore shift (5.001, 0) mm is not usable"), "{huge}");
    // Wider than the hole itself: a dia. 6 hole, a 4 mm shift.
    let mut small = params(fine(4.0, 0.0), 0.0);
    if let OpParams::Drill { holes, .. } = &mut small {
        holes[0].radius = 3.0;
    }
    let err = CreateOperation::new(&world, OP, TOOL, src(), small).map(|_| ()).map_err(|e| format!("{e:?}")).unwrap_err();
    assert!(err.contains("is not smaller than the radius 3 mm of the hole at (0, 0)"), "{err}");
    assert!(made(fine(0.3, 0.0)).is_ok(), "a usable shift makes the command");

    // The same door on an edit: the operation keeps its cycle.
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src(), params(CycleChoice::Bore, 0.0)).expect("create"));
    let zero = CycleChoice::FineBore { shift_x_um: 0, shift_y_um: 0 };
    let err = EditOperationParams::new(&world, OP, params(zero, 0.0)).map(|_| ()).map_err(|e| format!("{e:?}")).unwrap_err();
    assert!(err.contains("fine bore needs a shift"), "{err}");
    assert_eq!(cycle_of(&world), CycleChoice::Bore);
}

#[test]
fn a_shift_the_bar_has_no_room_for_is_refused_at_generation() {
    // A dia. 19.5 bar in the dia. 20 bore has 0.25 of room: 0.3 rubs.
    let (world, mut h) = world_with(bar(19.5));
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src, params(fine(0.3, 0.0), 0.0)).expect("create"));
    let err = generation_refusal(&world, &mut h);
    assert!(err.contains("fine bore shift 0.3 does not fit") && err.contains("bar dia. 19.5"), "{err}");
}

#[test]
fn a_fine_bore_is_held_to_the_shank_fit_not_to_the_flute() {
    // The hole is there already: a bar with a 5 mm edge zone bores the
    // 11 mm travel on its dia. 16 shank, as a bore and a ream do. A drill
    // cycle with the same tool is too deep for its flute.
    let mut short = bar(16.0);
    short.flute_len = 5.0;
    let (world, mut h) = world_with(short);
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src, params(fine(0.3, 0.0), 0.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, OP).expect("generate past the flute"));
    assert_eq!(cache(&world).path.cycles[0].kind, CycleKind::FineBore);
}

/// A fine-bore job on the Haas VF-2 (its file changed by `machine`) through
/// `post`, posted by the GUI's own path: the gate's answer, the machine
/// check's problems, and whether a file was written.
fn job(machine: impl Fn(&str) -> String, post: &str, tag: &str) -> (Result<posting::Posted, String>, Vec<String>, bool) {
    let (world, mut h) = world_with(bar(16.0));
    let text = machine(posting::builtin_machine_text("Haas VF-2").expect("the VF-2 file"));
    let m = camcraft_app::machine::parse_mch(&text).expect("the machine parses");
    h.execute(&world, LoadMachine::new(&world, format!("test-{tag}.mch"), Some(m)).expect("mount"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named(post.into())).expect("post"));
    let src = SrcRef::Entity("puck".into());
    h.execute(&world, CreateOperation::new(&world, OP, TOOL, src, params(fine(0.3, -0.4), 0.5)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, OP).expect("generate"));
    let m = camcraft_app::machine::of(&world).1.expect("a machine is mounted");
    let problems = camcraft_app::machine::check_document(&world, &m).map(|c| c.problems).unwrap_or_else(|e| vec![e]);
    let path = std::env::temp_dir().join(format!("camcraft_fine_bore_{tag}_{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let posted = posting::post_setup(&world, &path);
    let written = path.exists();
    let _ = std::fs::remove_file(&path);
    (posted, problems, written)
}

/// The shipped VF-2 file with `spindle.orientation` stated.
fn orientation(text: &str, word: &str) -> String {
    let key = r#""spindle": {"#;
    assert_eq!(text.matches(key).count(), 1, "one spindle in the VF-2 file");
    text.replace(key, &format!(r#"{key} "orientation": {word},"#))
}

#[test]
fn haas_posts_the_generated_fine_bore_as_g76() {
    for (tag, word) in [("unstated", None), ("stated", Some("true"))] {
        let (posted, problems, written) = job(|t| word.map_or(t.to_owned(), |w| orientation(t, w)), "Haas", tag);
        let p = posted.unwrap_or_else(|e| panic!("{tag}: {e}"));
        assert!(written, "{tag}");
        assert!(problems.is_empty(), "{tag}: {problems:?}");
        let g76: Vec<&str> = p.program.lines().filter(|l| l.split_whitespace().any(|w| w == "G76")).collect();
        assert_eq!(g76.len(), 1, "{tag}:\n{}", p.program);
        let has = |w: &str| g76[0].split_whitespace().any(|v| v == w);
        assert!(has("I0.3") && has("J-0.4") && has("P500") && has("F60."),"{tag}: {}", g76[0]);
    }
}

#[test]
fn a_fine_bore_on_a_machine_that_cannot_stop_oriented_is_refused_by_name() {
    let (posted, problems, written) = job(|t| orientation(t, "false"), "Haas", "no-orient");
    let err = match posted {
        Ok(p) => panic!("a machine with no oriented stop must refuse a fine bore:\n{}", p.program),
        Err(e) => e,
    };
    assert!(err.contains("oriented spindle stop") && err.contains("HAAS VF-2") && err.contains(OP), "{err}");
    assert!(!written, "a refused post writes nothing");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("FineBore1: 1 fine bore cycle(s) need an oriented spindle stop"), "{problems:?}");
}

#[test]
fn a_post_that_does_not_declare_fine_boring_refuses_it_and_writes_nothing() {
    for post in ["Fanuc", "LinuxCNC"] {
        let (posted, _, written) = job(|t| t.to_owned(), post, &post.to_lowercase());
        let err = match posted {
            Ok(p) => panic!("{post} must refuse a fine bore:\n{}", p.program),
            Err(e) => e,
        };
        assert!(err.contains("fine boring is not supported by this post"), "{post}: {err}");
        assert!(!written, "{post}: a refused post writes nothing");
    }
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

#[test]
fn mcp_edit_operation_sets_the_fine_bore_with_its_shift_and_refuses_one_without() {
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/Milling-test.step")
        .to_string_lossy()
        .into_owned();
    let mut s = Server::new();
    let steps = [
        ("import_model", json!({ "path": model, "name": "part" })),
        ("add_tool", json!({ "catalog": "DR-0600" })),
        ("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "part", "name": "OP" })),
    ];
    for (i, (name, args)) in steps.into_iter().enumerate() {
        let (err, body) = mcp_call(&mut s, i as i64 + 1, name, args);
        assert!(!err, "{name} refused: {body}");
    }
    let shift = json!({ "fine_bore": { "shift_x": 0.2, "shift_y": -0.1 } });
    let (err, body) = mcp_call(&mut s, 10, "edit_operation", json!({ "name": "OP", "params": { "cycle": shift } }));
    assert!(!err, "edit_operation refused a fine bore: {body}");
    assert_eq!(body["params"]["drill"]["cycle"], shift, "{body}");
    // The cycle word alone states no shift: refused, never a default.
    let (err, body) = mcp_call(&mut s, 11, "edit_operation", json!({ "name": "OP", "params": { "cycle": "fine_bore" } }));
    assert!(err, "a fine bore with no shift is refused: {body}");
    let zero = json!({ "fine_bore": { "shift_x": 0.0, "shift_y": 0.0 } });
    let (err, body) = mcp_call(&mut s, 12, "edit_operation", json!({ "name": "OP", "params": { "cycle": zero } }));
    assert!(err && body.to_string().contains("fine bore needs a shift"), "a zero shift is refused by name: {body}");
    let (err, body) = mcp_call(&mut s, 13, "undo", json!({}));
    assert!(!err, "undo: {body}");
    let (err, body) = mcp_call(&mut s, 14, "get_operation", json!({ "name": "OP" }));
    if !err {
        assert_ne!(body["params"]["drill"]["cycle"], shift, "undo took the fine bore back: {body}");
    }
}

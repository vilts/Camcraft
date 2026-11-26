//! #1890 through the operation layer, the project file and MCP: a 3D
//! Deburr `width` is the actual chamfer chord, and a file
//! from before v76 steps its width up by sqrt(2) (the 90-degree chord of
//! the setback it stated), and the break mode, profile and passes ride
//! the command history, the file and MCP.
//!
//! Provenance: our own tests of our own parameter model; the sizing law
//! is plane chamfer geometry (`finishing3d::deburr_size`).
//!
//! Gate map (#1890): covers app::project (params, apply - the v76
//! step), app::operation (generate, sane), app::mcp (edit).

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::finishing3d::{BreakMode, BreakProfile, DeburrBreak};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

/// The 90-degree chamfer mill the deburr suite rides.
const CONE: &str = "CM-0600-90";

/// A break of chord `width` on the brick's top rim, tips in z 6..10.
fn deburr(width: f64) -> OpParams {
    OpParams::Deburr {
        width,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(6.0),
        feed: 500.0,
        plunge_feed: 100.0,
        unreachable: camcraft_app::finishing3d::UnreachableEdges::SkipWithNote,
        sizing: DeburrBreak::default(),
        exclude_edges: Vec::new(),
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        picks: Default::default(),
    }
}

/// [`deburr`] of a 1 mm chord with the given sizing.
fn with(sizing: DeburrBreak) -> OpParams {
    let mut p = deburr(1.0);
    if let OpParams::Deburr { sizing: s, .. } = &mut p {
        *s = sizing;
    }
    p
}

/// A 40 x 20 x 10 brick that is its own stock, the cone, and a Deburr
/// "DB" of the brick (not generated).
fn brick_world(params: OpParams) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "DB", CONE, SrcRef::Entity("brick".into()), params).expect("create"));
    (world, h)
}

fn params_of(world: &World) -> OpParams {
    let e = model::find_by_name(world, "DB").expect("op");
    model::operation_of(world, &e).expect("operation").params
}

fn width_of(world: &World) -> f64 {
    match params_of(world) {
        OpParams::Deburr { width, .. } => width,
        ref other => panic!("deburr params expected, got {other:?}"),
    }
}

fn sizing_of(world: &World) -> DeburrBreak {
    match params_of(world) {
        OpParams::Deburr { sizing, .. } => sizing,
        ref other => panic!("deburr params expected, got {other:?}"),
    }
}

#[test]
fn main_v75_deburr_migrates_once_and_preserves_the_machine_controller() {
    let (world, mut h) = brick_world(deburr(1.0));
    h.execute(
        &world,
        LoadMachine::builtin(&world, "Fehlmann Picomax 55 (iTNC 530)").expect("machine"),
    );
    let saved: Value =
        serde_json::from_str(&project::save_text(&world).expect("save")).expect("JSON");
    const { assert!(project::PROJECT_VERSION >= 76, "v75 readers must refuse the new width meaning") };
    assert_eq!(saved["version"], project::PROJECT_VERSION, "saves the current version");
    let machine = &saved["machine"]["machine"];
    assert_eq!(
        machine["controller_identity"],
        json!({"model":"iTNC 530","nc_software":"340490-03"})
    );
    assert_eq!(machine["tool_change"], "manual-retract-top");
    for version in [74, 75] {
        let mut old = saved.clone();
        old["version"] = json!(version);
        // Main's v75 writer has the old width and no sizing keys.
        let mut removed = false;
        for entity in old["entities"].as_array_mut().expect("entities") {
            if let Some(params) = entity
                .pointer_mut("/operation/params/deburr")
                .and_then(Value::as_object_mut)
            {
                removed = true;
                for key in ["break_mode", "break_depth", "break_profile", "passes"] {
                    params.remove(key);
                }
            }
        }
        assert!(
            removed,
            "the legacy fixture must remove the new sizing keys"
        );
        let back = new_world();
        project::open(&back, &old.to_string()).expect("a main-written file opens");
        assert!(
            (width_of(&back) - std::f64::consts::SQRT_2).abs() < 1e-12,
            "v{version} width must migrate"
        );
        let resaved: Value =
            serde_json::from_str(&project::save_text(&back).expect("resave")).expect("JSON");
        assert_eq!(
            &resaved["machine"]["machine"], machine,
            "migration preserves the controller"
        );
        let again = new_world();
        project::open(&again, &resaved.to_string()).expect("v76 reopens");
        assert_eq!(
            width_of(&again),
            width_of(&back),
            "migration runs only once"
        );
    }
    let same = new_world();
    project::open(&same, &saved.to_string()).expect("a current file opens");
    assert_eq!(
        width_of(&same),
        1.0,
        "a current file already states the chord"
    );
}

#[test]
fn constant_depth_in_two_passes_generates_inside_its_declared_break() {
    // The brick's top rim is a 90-degree wedge: s = d / cos(45 deg).
    let sizing = DeburrBreak { mode: BreakMode::ConstantDepth, depth: 0.5, passes: 2, ..DeburrBreak::default() };
    let (world, mut h) = brick_world(with(sizing));
    h.execute(&world, GenerateOperation::new(&world, "DB").expect("the top rim rides"));
    let e = model::find_by_name(&world, "DB").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Ok);
    let t = model::toolpath_of(&world, &e).expect("cached").path;
    let s = 0.5 * std::f64::consts::SQRT_2;
    assert!(!t.breaks.is_empty() && t.breaks.iter().all(|b| (b.depth - s).abs() < 1e-9), "{:?}", t.breaks);
    let plunges: Vec<f64> =
        t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Plunge).map(|m| 10.0 - m.to.z).collect();
    assert!(plunges.iter().any(|d| (d - s / 2.0).abs() < 1e-9), "the outer pass: {plunges:?}");
    assert!(plunges.iter().any(|d| (d - s).abs() < 1e-9), "the final pass: {plunges:?}");
    assert!(plunges.iter().all(|d| *d <= s + 1e-9), "no pass cuts past the final break: {plunges:?}");
}

#[test]
fn the_record_bound_is_the_largest_setback_of_the_size_law() {
    // A chord of 1 is a setback of 1 / sqrt(2) on a 90-degree edge, but
    // up to 1 / (2 sin 15 deg) on the sharpest wedge Constant Width sizes.
    use camcraft_app::stock_sim::breaks;
    let w = breaks::declared_width(&deburr(1.0)).expect("a deburr declares a bound");
    assert!((w - 1.0 / (2.0 * 15f64.to_radians().sin())).abs() < 1e-12, "{w}");
    let depth = DeburrBreak { mode: BreakMode::ConstantDepth, depth: 0.5, ..DeburrBreak::default() };
    let d = breaks::declared_width(&with(depth)).expect("a deburr declares a bound");
    assert!((d - 0.5 / 75f64.to_radians().cos()).abs() < 1e-12, "{d}");
}

#[test]
fn a_round_profile_refuses_by_name_at_generate() {
    let (world, _h) = brick_world(with(DeburrBreak { profile: BreakProfile::Round, ..DeburrBreak::default() }));
    let Err(CommandError::Machining(msg)) = GenerateOperation::new(&world, "DB") else {
        panic!("a chamfer mill cuts no round break: expected a machining refusal");
    };
    assert!(msg.contains("Round break profile needs a ball-tipped cutter"), "{msg}");
}

#[test]
fn zero_passes_or_no_depth_is_refused_at_the_command() {
    let (world, _h) = brick_world(deburr(1.0));
    for bad in [DeburrBreak { passes: 0, ..DeburrBreak::default() }, DeburrBreak { depth: 0.0, ..DeburrBreak::default() }] {
        assert!(CreateOperation::new(&world, "X", CONE, SrcRef::Entity("brick".into()), with(bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn the_sizing_edit_undoes_and_round_trips_in_the_file() {
    let (world, mut h) = brick_world(deburr(1.0));
    let sizing = DeburrBreak { mode: BreakMode::ConstantDepth, depth: 0.3, profile: BreakProfile::Chamfer, passes: 4 };
    h.execute(&world, EditOperationParams::new(&world, "DB", with(sizing)).expect("edit"));
    assert_eq!(sizing_of(&world), sizing);
    let saved = project::save_text(&world).expect("save");
    for word in [r#""break_mode": "constant_depth""#, r#""break_depth": 0.3"#, r#""break_profile": "chamfer""#, r#""passes": 4"#] {
        assert!(saved.contains(word), "{word} in {saved}");
    }
    let back = new_world();
    project::open(&back, &saved).expect("reload");
    assert_eq!(sizing_of(&back), sizing);
    assert_eq!(project::save_text(&back).expect("resave"), saved, "the sizing round-trips byte-identically");
    h.undo(&world);
    assert_eq!(sizing_of(&world), DeburrBreak::default(), "undo restores the previous sizing");
    // A word that is not a mode is refused, not read as the default.
    let bad = saved.replace(r#""constant_depth""#, r#""constant_area""#);
    assert!(project::open(&new_world(), &bad).is_err(), "an unknown mode word is refused");
}

fn ok(server: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("request answered");
    assert_ne!(reply["result"]["isError"], json!(true), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

#[test]
fn mcp_sets_the_break_mode_profile_and_passes() {
    let mut s = Server::new();
    ok(&mut s, 1, "set_stock", json!({ "box": { "dx": 40.0, "dy": 20.0, "dz": 10.0 } }));
    ok(&mut s, 2, "add_tool", json!({ "catalog": CONE }));
    let params = json!({ "break_mode": "constant_depth", "break_depth": 0.4, "passes": 3 });
    ok(&mut s, 3, "add_operation", json!({ "kind": "Deburr", "name": "DB", "tool": CONE, "params": params }));
    let job = ok(&mut s, 4, "get_job", json!({}));
    let p = &job["operations"][0]["params"]["deburr"];
    assert_eq!(p["break_mode"], json!("constant_depth"), "{job}");
    assert_eq!(p["break_depth"], json!(0.4), "{job}");
    assert_eq!(p["passes"], json!(3), "{job}");
    let edited = ok(&mut s, 5, "edit_operation", json!({ "name": "DB", "params": { "break_profile": "round" } }));
    assert_eq!(edited["params"]["deburr"]["break_profile"], json!("round"), "{edited}");
}

//! #2133: out-of-range 3+2 tilt refusal names limits and A0 B180 remedy,
//! and the A0 B180 frame keeps plane X on machine X (the A rotary axis).
//! Provenance: our own tests over our own commands, MCP server and frame law.
//!
//! Gate map (M4.4): covers cl::frame, app::commands, app::operation, app::mcp,
//! app::posting, post::post (tormach.cps).

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, EditOperationParams, GenerateOperation, History,
    LoadMachine, SelectPost, SetStock, SetUnits,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::units::Unit;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const IN: f64 = 25.4;

fn slot_with_tilt(tilt: Option<Tilt>) -> OpParams {
    let above = |offset_in: f64| Height::of(HeightReference::TopOfStock, offset_in * IN);
    OpParams::Slot {
        a: Point2::new(-0.75 * IN, 0.0),
        b: Point2::new(-0.25 * IN, 0.0),
        width: 0.25 * IN,
        stock_to_leave: 0.0,
        clearance_z: above(0.5),
        retract_z: None,
        top_z: above(0.0),
        bottom_z: above(-0.015),
        stepdown: 0.015 * IN,
        stepover: 3.0,
        feed: 15.0 * IN,
        plunge_feed: 4.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: camcraft_app::links::Links::PLANAR,
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

fn bar_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(
        &world,
        LoadMachine::builtin(&world, "Tormach 770MX + microARC 4").expect("machine"),
    );
    h.execute(&world, SetUnits::new(&world, Unit::Inches).expect("inches"));
    h.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"),
    );
    let r = 0.375 * IN;
    let stick = 1.5 * IN;
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: stick, dy: 2.0 * r, dz: 2.0 * r },
        Location { pos: Point3::new(-stick / 2.0, 0.0, -r), ..Location::default() },
    )
    .expect("stock");
    h.execute(&world, stock);
    let (_, mut t) = catalog().into_iter().next().expect("catalog tool");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.flute_len = 0.75 * IN;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    h.execute(&world, AddTool::new(&world, "EM 1/4", t).expect("tool"));
    (world, h)
}

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, String) {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    let reply = server.handle(&req).expect("mcp reply");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let text = if is_err {
        result["content"][0]["text"].as_str().unwrap_or_default().to_owned()
    } else {
        result["structuredContent"].to_string()
    };
    (is_err, text)
}

fn assert_a_tilt_refusal(msg: &str) {
    assert!(
        !msg.contains("operation parameters are not usable"),
        "generic refusal hides the tilt cause: {msg}"
    );
    assert!(msg.contains("Tool axis A"), "must name Tool axis A: {msg}");
    assert!(msg.contains("-90..90"), "must give A limits: {msg}");
    assert!(msg.contains("-180..180"), "must give B limits: {msg}");
    assert!(
        msg.contains("A 0") && msg.contains("B 180"),
        "must explain the A 0 B 180 pose for the opposite face: {msg}"
    );
}

#[test]
fn out_of_range_tilt_refuses_by_name_with_a0_b180_remedy() {
    let (world, mut h) = bar_world();
    h.execute(
        &world,
        CreateOperation::new(&world, "S", "EM 1/4", SrcRef::Stock, slot_with_tilt(None))
            .expect("valid slot"),
    );

    for bad_a in [180.0, -180.0, 95.0] {
        let tilt = Tilt { basis_version: 1, a_deg: bad_a, b_deg: 0.0, origin: [0.0; 3] };
        let err = CreateOperation::new(&world, "BadA", "EM 1/4", SrcRef::Stock, slot_with_tilt(Some(tilt)))
            .err()
            .expect("out-of-range A must refuse at create");
        assert!(matches!(err, CommandError::Invalid(_)), "{err:?}");
        assert_a_tilt_refusal(&err.to_string());

        let err = EditOperationParams::new(&world, "S", slot_with_tilt(Some(tilt)))
            .err()
            .expect("out-of-range A must refuse at edit");
        assert!(matches!(err, CommandError::Invalid(_)), "{err:?}");
        assert_a_tilt_refusal(&err.to_string());
    }

    let bad_b = Tilt { basis_version: 1, a_deg: 0.0, b_deg: 200.0, origin: [0.0; 3] };
    let err = EditOperationParams::new(&world, "S", slot_with_tilt(Some(bad_b)))
        .err()
        .expect("out-of-range B must refuse");
    let msg = err.to_string();
    assert!(msg.contains("Tool axis B") && msg.contains("-180..180"), "{msg}");

    let nan_tilt = Tilt { basis_version: 1, a_deg: f64::NAN, b_deg: 0.0, origin: [0.0; 3] };
    let err = EditOperationParams::new(&world, "S", slot_with_tilt(Some(nan_tilt)))
        .err()
        .expect("non-finite tilt must refuse");
    assert!(err.to_string().contains("finite"), "{err}");

    let inf_origin = Tilt { basis_version: 1, a_deg: 0.0, b_deg: 0.0, origin: [0.0, f64::INFINITY, 0.0] };
    let err = EditOperationParams::new(&world, "S", slot_with_tilt(Some(inf_origin)))
        .err()
        .expect("non-finite origin must refuse");
    assert!(err.to_string().contains("Plane origin"), "{err}");

    // Same refusal over MCP add_operation and edit_operation.
    let mut server = Server::new();
    assert!(!mcp_call(&mut server, 1, "set_stock", json!({ "box": { "dx": 38.1, "dy": 19.05, "dz": 19.05 } })).0);
    assert!(!mcp_call(&mut server, 2, "add_tool", json!({ "catalog": "EM-0600-4FL" })).0);
    let (is_err, msg) = mcp_call(
        &mut server,
        3,
        "add_operation",
        json!({
            "kind": "Slot",
            "tool": "EM-0600-4FL",
            "name": "McpBad",
            "params": { "tilt": { "a_deg": 180.0, "b_deg": 0.0, "origin": [0.0, 0.0, 0.0] } }
        }),
    );
    assert!(is_err, "MCP add_operation must refuse A=180");
    assert_a_tilt_refusal(&msg);

    assert!(!mcp_call(&mut server, 4, "add_operation", json!({ "kind": "Slot", "tool": "EM-0600-4FL", "name": "McpGood" })).0);
    let (is_err, msg) = mcp_call(
        &mut server,
        5,
        "edit_operation",
        json!({
            "name": "McpGood",
            "params": { "tilt": { "a_deg": 180.0, "b_deg": 0.0, "origin": [0.0, 0.0, 0.0] } }
        }),
    );
    assert!(is_err, "MCP edit_operation must refuse A=180");
    assert_a_tilt_refusal(&msg);
}

#[test]
fn a0_b180_slot_cuts_along_machine_x_like_the_other_faces() {
    let (world, mut h) = bar_world();
    let p90 = slot_with_tilt(Some(Tilt { basis_version: 1, a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }));
    let p180 = slot_with_tilt(Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 180.0, origin: [0.0; 3] }));
    h.execute(&world, CreateOperation::new(&world, "Flat A90", "EM 1/4", SrcRef::Stock, p90).expect("A90"));
    h.execute(&world, GenerateOperation::new(&world, "Flat A90").expect("gen A90"));
    h.execute(&world, CreateOperation::new(&world, "Flat A180", "EM 1/4", SrcRef::Stock, p180).expect("A180"));
    h.execute(&world, GenerateOperation::new(&world, "Flat A180").expect("gen A180"));

    let tp = model::find_by_name(&world, "Flat A180")
        .and_then(|e| model::toolpath_of(&world, &e))
        .expect("cached A180 toolpath");
    let feeds: Vec<_> = tp.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty());
    for m in &feeds {
        assert!(m.from.y.abs() < 1e-6 && m.to.y.abs() < 1e-6, "cut must stay on Y=0, got {m:?}");
        assert!(
            (m.to.x - (-0.25 * IN)).abs() < 1e-6 && (m.from.x - (-0.75 * IN)).abs() < 1e-6,
            "cut must run X -0.75\" -> -0.25\", got {m:?}"
        );
    }

    let out = std::env::temp_dir().join(format!("camcraft_tilt_a180_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out).expect("posts on microARC 4");
    let _ = std::fs::remove_file(&out);
    let sections: Vec<&str> = posted.program.split("(OPERATION: ").skip(1).collect();
    assert_eq!(sections.len(), 2, "{}", posted.program);
    let cuts = |s: &str| -> Vec<String> {
        s.lines()
            .skip_while(|l| !l.starts_with("G0 A"))
            .skip(1)
            .filter(|l| l.starts_with("G1 "))
            .map(str::to_owned)
            .collect()
    };
    assert!(sections[1].contains("\nG0 A180\n"), "{}", sections[1]);
    assert_eq!(cuts(sections[0]), cuts(sections[1]), "A90 and A180 flats must match");
}

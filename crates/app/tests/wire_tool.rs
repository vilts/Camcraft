//! #2011: Wire EDM wire tool kind (`ToolKind::Wire`) and wire operations reading
//! wire diameter from the mounted wire tool, with machine/condition agreement
//! checks.
//! Provenance: our own tests over synthetic EDM fixtures; no vendor material consulted.
//! Gate map (#2011): covers tools::wire, app::operation (wire_op, tool_law, stamp,
//! stamp_tool, tool_gate), app::commands (tool, operation create/edit/generate), app::posting.

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, EditOperationLinks, GenerateOperation, History, ImportModel, LoadMachine,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    self, Height, OpParams, OpStatus, OpType, SrcRef, WireContourOp, WireSide, WireSkim, WireTabs,
};
use camcraft_app::posting;
use camcraft_app::tools::{self, Tool, ToolKind};
use camcraft_app::wire_tools::wire_tool;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const CPS_ISO: &str = "Generic ISO Wire EDM";

fn slug() -> Mesh {
    let pts = [(20.0, -10.0), (20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0)];
    let mut f = Vec::new();
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

fn wire_op(condition: &str) -> WireContourOp {
    WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: condition.into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-30.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
    }
}

fn doc_with_machine(mch: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(mch).expect("machine parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    (world, h)
}

fn top_y(world: &World, op_name: &str) -> f64 {
    let e = model::find_by_name(world, op_name).expect("op");
    let path = model::toolpath_of(world, &e).expect("path").path;
    path.moves
        .iter()
        .filter(|m| (m.from.x <= 0.0) != (m.to.x <= 0.0))
        .map(|m| m.from.y + (m.to.y - m.from.y) * (0.0 - m.from.x) / (m.to.x - m.from.x))
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn wire_contour_mounts_a_wire_tool_and_catalog_lists_one() {
    let info = OpType::WireContour.info();
    assert!(info.mounts_tool(), "Wire Contour must mount a wire tool from the library");
    assert!(!info.tool_kinds.is_empty(), "Wire Contour must list its wire tool kind");
    assert!(
        tools::catalog().iter().any(|(name, _)| name.starts_with("EDM-W")),
        "starter tool catalog must offer a wire EDM tool"
    );
}

#[test]
fn wire_tool_command_refuses_bad_rpm_diameter_corner_and_cutting_rows_by_name() {
    let (world, mut h) = doc_with_machine(FIXTURE);
    let valid = wire_tool(0.25);
    h.execute(&world, AddTool::new(&world, "W25", valid.clone()).expect("valid wire tool"));

    let spun = Tool { rpm: 100.0, ..valid.clone() };
    let err = AddTool::new(&world, "BadRpm", spun).err().expect("rpm refused").to_string();
    assert!(err.contains("rpm must be 0"), "{err}");

    for bad_d in [0.01, 1.5, f64::NAN] {
        let bad = Tool { diameter: bad_d, ..valid.clone() };
        assert!(AddTool::new(&world, "BadDia", bad).is_err(), "diameter {bad_d} must refuse");
    }

    let rounded = Tool { corner_r: 0.05, ..valid.clone() };
    let err = AddTool::new(&world, "BadCorner", rounded).err().expect("corner refused").to_string();
    assert!(err.contains("corner radius"), "{err}");

    let row = camcraft_app::tool_cutting_rows::CuttingRow::plain(Some(tools::WorkMaterial::MildSteel), 0.0, 50.0);
    let with_rows = Tool { cutting: vec![row], ..valid };
    let err = AddTool::new(&world, "BadRows", with_rows).err().expect("cutting rows refused").to_string();
    assert!(err.contains("EDM conditions"), "{err}");

    let err = CreateOperation::new(&world, "NoTool", "", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C110")))
        .err()
        .expect("empty tool refused")
        .to_string();
    assert!(err.contains("no entity named ''"), "{err}");

    let (_, mill) = tools::catalog().into_iter().find(|(_, t)| t.kind == ToolKind::EndMill).expect("end mill");
    h.execute(&world, AddTool::new(&world, "EM", mill).expect("em"));
    let err = CreateOperation::new(&world, "WrongTool", "EM", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C110")))
        .err()
        .expect("end mill refused")
        .to_string();
    assert!(err.contains("cuts with a wire"), "{err}");
}

#[test]
fn mounted_wire_diameter_drives_kerf_path_post_and_stamp_staleness() {
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
    v["edm"]["wire_diameters"] = serde_json::json!([0.20, 0.25]);
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "G1", "name": "gap only", "gap": 0.04, "role": "rough", "notes": "SYNTHETIC" }
    ]);
    let (world, mut h) = doc_with_machine(&v.to_string());
    h.execute(&world, AddTool::new(&world, "WireA", wire_tool(0.25)).expect("0.25 wire"));

    let op = OpParams::WireContour(wire_op("G1"));
    h.execute(&world, CreateOperation::new(&world, "Punch1", "WireA", SrcRef::Entity("slug".into()), op).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generate"));
    let y25 = top_y(&world, "Punch1");
    assert!((y25 - (10.0 + 0.125 + 0.04)).abs() < 1e-6, "0.25 wire kerf: {y25}");

    let cps = camcraft_app::posting::builtin_post_text(CPS_ISO).expect("cps");
    let out = std::env::temp_dir().join(format!("camcraft-2011-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps).expect("posts");
    let _ = std::fs::remove_file(&out);
    assert!(posted.program.contains("G1"), "NC posted cleanly");

    // Editing the mounted wire tool's diameter in-place makes the generated op Stale
    // without touching the operation directly, and regenerating shifts the path kerf.
    h.execute(&world, ChangeTool::new(&world, "WireA", wire_tool(0.20)).expect("0.20 edit"));
    let e = model::find_by_name(&world, "Punch1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "wire tool edit must stale op");
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("regenerate"));
    let y20 = top_y(&world, "Punch1");
    assert!((y20 - (10.0 + 0.10 + 0.04)).abs() < 1e-6, "0.20 wire kerf: {y20}");
    assert!(((y25 - y20) - 0.025).abs() < 1e-6, "radius delta 0.025 mm moved path");
}

#[test]
fn wire_tool_refuses_machine_wire_list_condition_and_spark_gap_mismatches() {
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
    v["edm"]["wire_diameters"] = serde_json::json!([0.16, 0.25]);
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "C25", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" },
        { "id": "C16", "gap": 0.03, "wire_diameter": 0.16, "role": "rough" },
        { "id": "BIG_GAP", "gap": 0.10, "role": "rough" }
    ]);
    let (world, mut h) = doc_with_machine(&v.to_string());
    for (n, d) in [("W25", 0.25), ("W20", 0.20), ("W16", 0.16), ("W2505", 0.2505), ("W252", 0.252)] {
        h.execute(&world, AddTool::new(&world, n, wire_tool(d)).expect(n));
    }

    let err = CreateOperation::new(&world, "BadMachineWire", "W20", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C25")))
        .err()
        .expect("0.20 not in edm.wire_diameters")
        .to_string();
    assert!(err.contains("edm.wire_diameters"), "{err}");

    let err = CreateOperation::new(&world, "BadCondWire", "W16", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C25")))
        .err()
        .expect("0.16 disagrees with C25 0.25")
        .to_string();
    assert!(err.contains("expects wire diameter 0.25 mm") && err.contains("W16"), "{err}");

    // Tolerance: 0.2505 within WIRE_MATCH_MM (0.001) of 0.25 is accepted; 0.252 is refused.
    assert!(CreateOperation::new(&world, "TolOk", "W2505", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C25"))).is_ok());
    assert!(CreateOperation::new(&world, "TolBad", "W252", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C25"))).is_err());

    // Skim & tab cut-off conditions must also match the mounted wire diameter.
    let mut bad_skim = wire_op("C25");
    bad_skim.skims = vec![WireSkim { condition: "C16".into(), reverse: false }];
    let err = CreateOperation::new(&world, "BadSkim", "W25", SrcRef::Entity("slug".into()), OpParams::WireContour(bad_skim))
        .err()
        .expect("skim condition wire mismatch")
        .to_string();
    assert!(err.contains("skim 1") && err.contains("0.16"), "{err}");

    let mut bad_tabs = wire_op("C25");
    bad_tabs.tabs = Some(WireTabs { count: Some(1), width: Some(1.0), at: vec![[0.0, 10.0]], cutoff_condition: "C16".into() });
    h.execute(&world, CreateOperation::new(&world, "BadTab", "W25", SrcRef::Entity("slug".into()), OpParams::WireContour(bad_tabs)).expect("create"));
    let err = GenerateOperation::new(&world, "BadTab")
        .err()
        .expect("cutoff condition wire mismatch")
        .to_string();
    assert!(err.contains("tab cut-off") && err.contains("0.16"), "{err}");

    // Gap >= wire radius must refuse even when condition omits wire_diameter.
    let err = CreateOperation::new(&world, "BadGap", "W16", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("BIG_GAP")))
        .err()
        .expect("gap 0.10 >= radius 0.08")
        .to_string();
    assert!(err.contains("spark gap") && err.contains("wire radius"), "{err}");

    // Retargeting an op to an incompatible wire tool refuses at EditOperationLinks.
    h.execute(&world, CreateOperation::new(&world, "Good", "W25", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C25"))).expect("good"));
    assert!(EditOperationLinks::new(&world, "Good", Some("W16"), None).is_err());
}

#[test]
fn legacy_wire_project_opens_without_a_tool_and_requires_recovery_before_posting() {
    use camcraft_app::project;

    let (world, mut h) = doc_with_machine(FIXTURE);
    h.execute(&world, AddTool::new(&world, "W25", wire_tool(0.25)).expect("wire"));
    h.execute(&world, CreateOperation::new(&world, "Legacy", "W25", SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op("C110"))).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Legacy").expect("generate"));
    // Reconstruct the pre-#2011 on-disk layout: same schema version,
    // condition-owned diameter, an empty tool reference and no tool entity.
    let mut old: serde_json::Value = serde_json::from_str(&project::save_text(&world).expect("save")).expect("json");
    let entities = old["entities"].as_array_mut().expect("entities");
    entities.retain(|e| e["name"] != "W25");
    let op = entities.iter_mut().find(|e| e["name"] == "Legacy").expect("legacy");
    op["operation"]["tool"] = serde_json::json!("");
    assert!(op["operation"]["cache"].is_object(), "fixture includes a cached path");
    let saved_machine = old["machine"].clone();
    assert!(saved_machine.is_object(), "fixture retains the machine condition table");

    let loaded = new_world();
    let notes = project::open_noted(&loaded, &old.to_string()).expect("previously saved wire project opens");
    assert!(notes.iter().any(|n| n.contains("Legacy") && n.contains("assign a wire tool")), "{notes:?}");
    let e = model::find_by_name(&loaded, "Legacy").expect("op retained");
    assert!(model::toolpath_of(&loaded, &e).is_none(), "legacy cache cannot authorize posting");
    let saved: serde_json::Value = serde_json::from_str(&project::save_text(&loaded).expect("save recovered")).expect("json");
    assert_eq!(saved["machine"], saved_machine, "condition-owned wire diameter is preserved exactly");
    let saved_op = saved["entities"].as_array().expect("entities").iter().find(|e| e["name"] == "Legacy").expect("op");
    assert!(saved_op["operation"]["stamp"].is_null(), "legacy generation stamp is discarded");
    assert!(GenerateOperation::new(&loaded, "Legacy").is_err(), "no implicit wire tool");
    let cps = camcraft_app::posting::shipped_posts().expect("the shipped posts read").iter().find(|(n, _)| *n == CPS_ISO).expect("post").1;
    let out = std::env::temp_dir().join(format!("camcraft-2140-legacy-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    assert!(posting::post_script(&loaded, &out, cps).is_err(), "legacy operation must not post");
    assert!(!out.exists(), "no NC file before recovery");

    let mut h = History::default();
    h.execute(&loaded, AddTool::new(&loaded, "Recovered", wire_tool(0.25)).expect("wire"));
    h.execute(&loaded, EditOperationLinks::new(&loaded, "Legacy", Some("Recovered"), None).expect("assign wire"));
    h.execute(&loaded, GenerateOperation::new(&loaded, "Legacy").expect("regenerate after recovery"));
    assert!(posting::post_script(&loaded, &out, cps).is_ok(), "explicit recovery posts");
    let _ = std::fs::remove_file(&out);
    project::open(&new_world(), &project::save_text(&loaded).expect("save")).expect("recovered project reopens");
}

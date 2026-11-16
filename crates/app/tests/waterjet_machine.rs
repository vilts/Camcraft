//! #1881: waterjet command, file and machine-check boundaries.
//! Provenance: our own synthetic fixtures from the issue's XY and cut-data requirements.
//! Gate map (#1881): covers app::machine, app::commands (tool), app::project,
//! app::tool_library, app::operation (kinds), app::cutting_rows.

use camcraft_app::cl::{Move, MoveKind, Toolpath as ClPath};
use camcraft_app::commands::{AddTool, History, ImportTools, SetWcsOrigin};
use camcraft_app::geometry::Point3;
use camcraft_app::machine::{self, AxisLimits, Kind, Machine};
use camcraft_app::model;
use camcraft_app::operation::{Family, Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, Operation, ToolDef, Toolpath};
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tool_library::{export_tool_library, parse_tool_library};
use camcraft_app::tools::{Tool, ToolKind, WorkMaterial};
use camcraft_app::world::new_world;
use camcraft_tools::waterjet::{PierceMethod, WaterjetCutData, waterjet_tool};
use flecs_ecs::prelude::{Entity, World};
use serde_json::{Value, json};

fn file() -> Value {
    json!({
        "general": {"model": "Waterjet test", "kind": "waterjet"},
        "kinematics": {"default": {"parts": [
            {"name": "X", "type": "linear", "travel": {"min": 0, "max": 460}},
            {"name": "Y", "type": "linear", "travel": {"min": -305, "max": 0}},
            {"name": "nozzle", "type": "head"}
        ]}},
        "controller": {"synced_configuration": {"parts": {
            "X": {"coordinate": 0, "max_normal_speed": 1000},
            "Y": {"coordinate": 1, "max_normal_speed": 700}
        }}}
    })
}

fn machine() -> Machine {
    machine::parse_mch(&file().to_string()).expect("valid waterjet")
}

fn recipe(thickness: f64) -> CuttingRow {
    CuttingRow {
        material: WorkMaterial::MildSteel, rpm: 0.0, feed: 600.0,
        waterjet: Some(WaterjetCutData {
            thickness, kerf: 0.8, pierce_time: 2.0, abrasive_delay: 0.3,
            re_pierce_time: 1.0, pierce_method: PierceMethod::Wiggle,
        }),
    }
}

fn tool() -> Tool {
    Tool { cutting: vec![recipe(5.0), recipe(10.0)], ..waterjet_tool(0.8) }
}

fn read_tool(world: &World) -> Tool {
    model::find_by_name(world, "Nozzle").and_then(|e| model::tool_of(world, &e)).expect("Nozzle")
}

#[test]
fn waterjet_validator_requires_xy_and_accepts_optional_z() {
    let base = file();
    assert!(machine::validate::validate_text(&base.to_string()).is_empty());
    for (index, name) in [(0, "X axis"), (1, "Y axis")] {
        let mut bad = base.clone();
        bad["kinematics"]["default"]["parts"].as_array_mut().unwrap().remove(index);
        let errors = machine::validate::validate_text(&bad.to_string());
        assert!(errors.iter().any(|e| e.key == name), "{errors:?}");
    }
    let mut xyz = base;
    xyz["kinematics"]["default"]["parts"].as_array_mut().unwrap()
        .push(json!({"name":"Z", "type":"linear", "travel":{"min":-20,"max":40}}));
    assert!(machine::validate::validate_text(&xyz.to_string()).is_empty());
    let m = machine::parse_mch(&xyz.to_string()).unwrap();
    assert_eq!(m.axis(2).and_then(|a| a.travel), Some((-20.0, 40.0)));
}

#[test]
fn waterjet_validator_names_spindle_live_tool_coolant_and_rotary() {
    for (key, value, expected) in [
        ("spindle", json!({"max_speed":2000}), "spindle.max_speed"),
        ("tool_station", json!({"live_tool":{}}), "live_tool"),
        ("tool_station", json!({"coolants":["Flood"]}), "coolants"),
    ] {
        let mut bad = file();
        bad["kinematics"]["default"]["parts"][2][key] = value;
        let errors = machine::validate::validate_text(&bad.to_string());
        assert!(errors.iter().any(|e| e.key == expected), "{errors:?}");
    }
    let mut bad = file();
    bad["tooling"] = json!({"coolants":["Mist"]});
    assert!(machine::validate::validate_text(&bad.to_string()).iter().any(|e| e.key == "tooling.coolants"));
    let mut bad = file();
    bad["kinematics"]["default"]["parts"].as_array_mut().unwrap()
        .push(json!({"name":"A", "type":"rotary"}));
    assert!(machine::validate::validate_text(&bad.to_string()).iter().any(|e| e.key == "A"));
}

#[test]
fn waterjet_add_tool_refuses_rpm_kerf_and_bad_recipe_by_name() {
    let w = new_world();
    assert!(AddTool::new(&w, "Nozzle", tool()).is_ok());
    for (bad, word) in [
        (Tool { rpm: 1000.0, ..tool() }, "rpm"),
        (Tool { diameter: 0.0, ..tool() }, "kerf"),
        (Tool { diameter: f64::NAN, ..tool() }, "kerf"),
        (Tool { corner_r: 0.1, ..tool() }, "corner_r"),
        (Tool { cutting: vec![CuttingRow { feed: 0.0, ..recipe(5.0) }], ..tool() }, "feed"),
    ] {
        let error = AddTool::new(&w, "Nozzle", bad).err().expect("refused").to_string();
        assert!(error.contains(word), "{word}: {error}");
    }
}

#[test]
fn waterjet_recipes_roundtrip_project_and_library_without_material_only_selection() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "Nozzle", tool()).unwrap());
    machine::set(&w, "synthetic-waterjet.mch", Some(&machine()));
    let original = read_tool(&w);
    assert_eq!(original.kind, ToolKind::Waterjet);
    assert!(camcraft_app::cutting_rows::row_in(&original, Some(WorkMaterial::MildSteel)).is_none());
    let data = project::tool_to_json(&original);
    assert_eq!(data["cutting_data"][0]["waterjet"]["pierce_method"], "wiggle");
    assert_eq!(project::tool_from_json(&data).unwrap(), original);
    let saved = project::save_text(&w).unwrap();
    let again = new_world();
    project::open(&again, &saved).unwrap();
    assert_eq!(read_tool(&again), original);
    assert_eq!(machine::document_of(&again), machine::document_of(&w));
    let library = export_tool_library(&w).unwrap();
    let imported = new_world();
    let command = ImportTools::new(&imported, parse_tool_library(&library).unwrap()).unwrap();
    History::default().execute(&imported, command);
    assert_eq!(read_tool(&imported), original);
    let mut bad = data;
    bad["cutting_data"][0]["waterjet"]["pierce_method"] = "unknown".into();
    assert!(project::tool_from_json(&bad).unwrap_err().contains("unknown"));
}

#[test]
fn only_waterjet_family_is_admitted_on_waterjet_machine() {
    for family in [Family::Mill, Family::Turn, Family::Plasma, Family::WireEdm, Family::Sinker, Family::Waterjet] {
        assert_eq!(family.admits(Kind::Waterjet), family == Family::Waterjet);
    }
    for kind in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::WireEdm, Kind::SinkerEdm, Kind::Waterjet] {
        assert_eq!(Family::Waterjet.admits(kind), kind == Kind::Waterjet);
    }
}

#[test]
fn waterjet_blank_draft_is_valid_and_no_builtin_post_claims_it() {
    let draft = machine::draft::MachineDraft::blank(Kind::Waterjet);
    let text = draft.text();
    assert!(machine::validate::validate_text(&text).is_empty(), "{text}");
    let m = machine::parse_mch(&text).unwrap();
    assert!(m.is_sane());
    assert_eq!(machine::kind(Some(&m)), Kind::Waterjet);
    assert!(m.axis(0).is_some() && m.axis(1).is_some() && m.axis(2).is_none());
    assert_eq!(m.spindle_max, None);
    assert!(m.coolants.is_empty());
    for (name, _) in camcraft_app::posting::BUILTINS {
        assert!(!camcraft_app::posting::post_serves(name, Kind::Waterjet), "{name}");
    }
}

/// No waterjet operation exists in this slice. Keep a deliberate milling-family
/// mismatch, then independently assert which additional cached-move errors appear.
fn cached(moves: Vec<Move>, tool: Tool) -> (World, Entity) {
    let w = new_world();
    let t = model::spawn(&w, "Nozzle");
    t.entity_view(&w).set(ToolDef { tool, number: None });
    let e = model::spawn(&w, "Cached");
    e.entity_view(&w).set(Operation {
        tool: "Nozzle".into(), src: SrcRef::Stock,
        params: OpParams::Face {
            holes: Default::default(), tilt: None, blend_turns: false,
            stepover: 1.0, clearance_z: Height::fixed(10.0), cut_z: Height::fixed(0.0),
            top_z: None, stepdown: None, feed: 600.0, plunge_feed: 100.0, climb: true,
        },
        stamp: None, setup: None, area: Default::default(), coolant: Default::default(),
        after: Default::default(), suppressed: false,
    }).set(Toolpath::new(ClPath { moves, ..Default::default() }, Vec::new()));
    (w, e)
}

fn feed(a: [f64; 3], b: [f64; 3], rate: f64) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2]), Some(rate))
}

#[test]
fn waterjet_machine_check_skips_missing_z_and_spindle_with_notes() {
    let (w, e) = cached(vec![feed([10.0,-10.0,-1000.0], [20.0,-20.0,1000.0], 600.0)], tool());
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert_eq!(result.problems.len(), 1, "{result:?}");
    assert!(result.problems[0].contains("only waterjet operations"));
    assert!(result.notes.iter().any(|n| n.contains("no Z axis") && n.contains("not judged")));
    assert!(result.notes.iter().any(|n| n.contains("no spindle") && n.contains("rpm")));
}

#[test]
fn waterjet_machine_check_judges_xy_travel_and_feed_and_optional_z() {
    let (w, e) = cached(vec![feed([10.0,-10.0,0.0], [461.0,-306.0,50.0], 1100.0)], tool());
    let mut m = machine();
    m.axes.push(AxisLimits { name: "Z".into(), travel: Some((-20.0,40.0)), feed: Some(1.0), ..Default::default() });
    let result = machine::check_ops(&w, &m, &[("Cached".into(), e)]).unwrap();
    for word in ["in X,", "in Y,", "in Z,", "Y tops out at 700"] {
        assert!(result.problems.iter().any(|p| p.contains(word)), "{word}: {result:?}");
    }
    assert!(!result.notes.iter().any(|n| n.contains("no Z axis")));
    assert!(!result.problems.iter().any(|p| p.contains("Z tops out")));
}

#[test]
fn waterjet_machine_check_catches_arc_bulges_and_only_limits_driven_axes() {
    let arc = feed([450.0,-90.0,0.0], [450.0,-110.0,0.0], 600.0)
        .with_arc(camcraft_app::geo2d::Point2::new(450.0,-100.0), false);
    let (w, e) = cached(vec![arc], tool());
    let mut m = machine();
    m.axes[0].travel = Some((0.0, 455.0));
    let result = machine::check_ops(&w, &m, &[("Cached".into(), e)]).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("460.000 in X")), "{result:?}");
    let (w, e) = cached(vec![feed([10.0,-10.0,0.0], [20.0,-10.0,0.0], 900.0)], tool());
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert_eq!(result.problems.len(), 1, "stationary Y does not limit an X move: {result:?}");
    let (w, e) = cached(vec![feed([10.0,-10.0,0.0], [20.0,-10.0,0.0], 1001.0)], tool());
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("X tops out at 1000")), "{result:?}");
}

#[test]
fn waterjet_arc_feed_checks_both_axes_even_when_endpoints_coincide() {
    let arc = feed([100.0,-80.0,0.0], [100.0,-80.0,0.0], 800.0)
        .with_arc(camcraft_app::geo2d::Point2::new(100.0,-100.0), true);
    let (w, e) = cached(vec![arc], tool());
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("Y tops out at 700")), "{result:?}");
}

#[test]
fn waterjet_machine_check_applies_wcs_and_axis_direction() {
    let (w, e) = cached(vec![feed([500.0,20.0,0.0], [510.0,30.0,0.0], 600.0)], tool());
    let ops = [("Cached".into(), e)];
    let before = machine::check_ops(&w, &machine(), &ops).unwrap();
    assert!(before.problems.iter().any(|p| p.contains("in X,")));
    let mut h = History::default();
    h.execute(&w, SetWcsOrigin::new(&w, Location { pos: Point3::new(500.0, 40.0, 0.0), ..Default::default() }).unwrap());
    let after = machine::check_ops(&w, &machine(), &ops).unwrap();
    assert_eq!(after.problems.len(), 1, "{after:?}");
    let mut inverted = file();
    inverted["kinematics"]["default"]["parts"][0]["direction"] = (-1).into();
    let inverted = machine::parse_mch(&inverted.to_string()).unwrap();
    let result = machine::check_ops(&w, &inverted, &ops).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("in X,")), "{result:?}");
}

#[test]
fn waterjet_machine_check_refuses_rotary_tilt_wrong_tool_and_bad_rpm() {
    let mut mv = feed([10.0,-10.0,0.0], [20.0,-20.0,0.0], 600.0);
    mv.rotary = Some(10.0);
    let (w, e) = cached(vec![mv], Tool { rpm: 1000.0, ..tool() });
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("rotary axis")), "{result:?}");
    assert!(result.problems.iter().any(|p| p.contains("rpm must be 0")), "{result:?}");
    mv.rotary = None;
    mv.axis = Some([1.0,0.0,0.0]);
    let (w, e) = cached(vec![mv], Tool { kind: ToolKind::Torch, cutting: Vec::new(), ..tool() });
    let result = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap();
    assert!(result.problems.iter().any(|p| p.contains("rotary axis")), "{result:?}");
    assert!(result.problems.iter().any(|p| p.contains("requires a waterjet tool")), "{result:?}");
}

#[test]
fn waterjet_machine_check_names_missing_cached_context() {
    let w = new_world();
    let e = model::spawn(&w, "Missing");
    let err = machine::check_ops(&w, &machine(), &[("Missing".into(), e)]).unwrap_err();
    assert!(err.contains("Missing: lost its operation"), "{err}");
    let (w, e) = cached(Vec::new(), tool());
    model::remove(&w, &model::find_by_name(&w, "Nozzle").unwrap());
    let err = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap_err();
    assert!(err.contains("tool Nozzle is not in the library"), "{err}");
    let (w, e) = cached(Vec::new(), tool());
    e.entity_view(&w).remove(w.component_id::<Toolpath>());
    let err = machine::check_ops(&w, &machine(), &[("Cached".into(), e)]).unwrap_err();
    assert!(err.contains("Cached: lost its toolpath"), "{err}");
}

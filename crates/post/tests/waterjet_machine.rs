//! #1881: waterjet machine files admit XY motion without requiring Z.
//! Provenance: our own synthetic machine fixture and issue requirements.
//! Gate map (#1881): covers post::machine (kind, parse, waterjet).

use camcraft_post::machine::{kind, parse_mch, waterjet_refusals, Coolant, LiveToolCodes, Machine};
use serde_json::{json, Value};

fn fixture() -> Value {
    json!({
        "general": { "model": "Waterjet test", "kind": "waterjet" },
        "kinematics": { "default": {
            "units": { "length": "mm", "angle": "degrees" },
            "parts": [
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": 0.0, "max": 460.0 } },
                { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -305.0, "max": 0.0 } },
                { "id": "nozzle", "name": "nozzle", "type": "head" }
            ]
        } }
    })
}

#[test]
fn waterjet_xy_without_z_parses_and_keeps_asymmetric_travel() {
    let text = fixture().to_string();
    let machine = parse_mch(&text).expect("an XY waterjet requires no Z axis");
    assert_eq!(kind(Some(&machine)).word(), "waterjet");
    assert!(machine.is_sane());
    assert_eq!(machine.axis(0).and_then(|a| a.travel), Some((0.0, 460.0)));
    assert_eq!(machine.axis(1).and_then(|a| a.travel), Some((-305.0, 0.0)));
    assert!(machine.axis(2).is_none());
}

#[test]
fn waterjet_optional_z_and_nested_xy_are_accepted() {
    let mut v = fixture();
    let parts = v["kinematics"]["default"]["parts"].take();
    v["kinematics"]["default"]["parts"] = json!([
        { "name": "Z", "type": "linear", "travel": { "min": -10.0, "max": 100.0 }, "parts": parts }
    ]);
    let m = parse_mch(&v.to_string()).expect("Z is optional, not forbidden");
    assert!(m.is_sane());
    assert_eq!(m.axis(2).and_then(|a| a.travel), Some((-10.0, 100.0)));
    assert!(waterjet_refusals(&v).is_empty());
    for id_only in [false, true] {
        let mut v = fixture();
        v["general"]["kind"] = "WATERJET".into();
        for index in 0..2 {
            let part = v["kinematics"]["default"]["parts"][index].as_object_mut().unwrap();
            if id_only {
                part.remove("name");
            } else {
                part.insert("name".into(), if index == 0 { "x" } else { "y" }.into());
            }
        }
        assert!(waterjet_refusals(&v).is_empty(), "id_only = {id_only}");
        let m = parse_mch(&v.to_string()).expect("lowercase names and IDs identify XY axes");
        assert!(m.is_sane());
        assert!(m.axis(0).is_some() && m.axis(1).is_some());
    }
}

#[test]
fn waterjet_file_refusals_name_each_forbidden_key() {
    for (key, head) in [
        ("spindle.max_speed", json!({"spindle": {"max_speed": 1000.0}})),
        ("spindle", json!({"spindle": {}})),
        ("live_tool", json!({"tool_station": {"live_tool": {}}})),
        ("coolants", json!({"tool_station": {"coolants": ["Flood"]}})),
    ] {
        let mut v = fixture();
        let target = v["kinematics"]["default"]["parts"][2].as_object_mut().unwrap();
        target.extend(head.as_object().unwrap().clone());
        assert!(waterjet_refusals(&v).iter().any(|(k, _)| k == key), "{key}");
        let error = parse_mch(&v.to_string()).expect_err(key);
        assert!(error.contains(key), "{error}");
    }
    let mut v = fixture();
    v["tooling"] = json!({"coolants": ["Mist"]});
    assert!(parse_mch(&v.to_string()).unwrap_err().contains("tooling.coolants"));
    let mut v = fixture();
    v["kinematics"]["default"]["parts"].as_array_mut().unwrap()
        .push(json!({"name": "A", "type": "rotary"}));
    assert!(parse_mch(&v.to_string()).unwrap_err().contains("A: a waterjet states no rotary axis"));
}

#[test]
fn waterjet_requires_both_linear_xy_axes_by_name() {
    for (index, name) in [(0, "X"), (1, "Y")] {
        let mut v = fixture();
        v["kinematics"]["default"]["parts"].as_array_mut().unwrap().remove(index);
        let error = parse_mch(&v.to_string()).expect_err("missing axis");
        assert!(error.contains(&format!("{name} axis")), "{error}");
        let mut v = fixture();
        v["kinematics"]["default"]["parts"][index]["type"] = "table".into();
        assert!(waterjet_refusals(&v).iter().any(|(k, _)| k == &format!("{name} axis")));
    }
}

#[test]
fn waterjet_saved_machine_refuses_spindle_coolant_rotary_and_missing_axes() {
    let base = parse_mch(&fixture().to_string()).unwrap();
    type Mutation = (&'static str, fn(&mut Machine));
    let mutations: &[Mutation] = &[
        ("spindle_min", |m| m.spindle_min = Some(1.0)),
        ("spindle_max", |m| m.spindle_max = Some(1000.0)),
        ("spindle_power", |m| m.spindle_power = Some(1.0)),
        ("spindle_ranges", |m| m.spindle_ranges = vec![(1.0, 1000.0)]),
        ("rigid_tapping", |m| m.rigid_tapping = Some(false)),
        ("spindles", |m| m.spindles = Some(1)),
        ("spindle_d", |m| m.spindle_d = Some(10.0)),
        ("spindle_len", |m| m.spindle_len = Some(10.0)),
        ("live_tool", |m| m.live_tool = Some(LiveToolCodes::default())),
        ("coolants", |m| m.coolants.push(Coolant { name: "Flood".into(), m_code: "M8".into() })),
        ("rotary", |m| { let mut a = m.axes[0].clone(); a.name = "A".into(); a.rotary = true; m.axes.push(a); }),
        ("missing X", |m| m.axes.retain(|a| a.name != "X")),
        ("missing Y", |m| m.axes.retain(|a| a.name != "Y")),
    ];
    for (name, mutate) in mutations {
        let mut machine = base.clone();
        mutate(&mut machine);
        assert!(!machine.waterjet_sane(), "{name}");
        assert!(!machine.is_sane(), "{name} reaches project door");
    }
    let ordinary = Machine { kind: Some("mill".into()), spindle_max: Some(1000.0), ..base };
    assert!(ordinary.waterjet_sane(), "waterjet rules leave mills alone");
}

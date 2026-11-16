//! Provenance: synthetic undersize/orbit fixtures from issue #1355 and the parent test's hand-built cavity.
//! Gate map: app::operation::sinker_op, app::operation::types::sinker, motion::sinker_undersize.

use super::*;

fn orbit_doc() -> (World, History) {
    let (world, mut h) = sinker_doc();
    let mut machine: serde_json::Value = serde_json::from_str(&sinker_machine()).expect("machine JSON");
    machine["edm"]["orbits"] = serde_json::json!([
        { "kind": "circular", "word": "CIRCLE R{radius}" },
        { "kind": "vector", "word": "VECTOR R{radius}" },
        { "kind": "planetary", "word": "PLANET R{radius}" }
    ]);
    machine["edm"]["max_orbit_radius"] = serde_json::json!(0.5);
    let machine = parse_mch(&machine.to_string()).expect("orbit machine");
    h.execute(&world, LoadMachine::new(&world, "orbit.mch".into(), Some(machine)).expect("mount"));
    h.execute(&world, AddTool::new(&world, "E-ORBIT-F", electrode_for(0.05)).expect("finish electrode"));
    (world, h)
}

fn orbited(pass: SinkerPass, condition: &str, kind: SinkerOrbitKind, radius: f64, vectors: u32) -> OpParams {
    let mut params = burn(pass, condition);
    if let OpParams::SinkerBurn(op) = &mut params {
        op.orbit = Some(SinkerOrbit { kind, radius, vectors, turns: 1 });
    }
    params
}

#[test]
fn orbited_rough_finish_pair_reaches_size_in_sim() {
    for kind in [SinkerOrbitKind::Circular, SinkerOrbitKind::Vector, SinkerOrbitKind::Planetary] {
        let (world, mut h) = orbit_doc();
        add(&world, &mut h, "Rough", ROUGH, orbited(SinkerPass::Rough, "R1", kind, 0.05, 4));
        generate(&world, &mut h, "Rough");
        assert!((feed_tip(&world, "Rough") - 14.25).abs() < 1e-9, "rough stands undersize minus lateral orbit");
        let sim = simulate_document(&world).expect("rough verdict");
        assert_eq!(sim.dev.gouge_cells, 0, "rough gouges nothing: {:?}", sim.dev);
        assert!(sim.dev.max_rest > 0.1, "rough leaves floor stock: {:?}", sim.dev);

        add(&world, &mut h, "Finish", "E-ORBIT-F", orbited(SinkerPass::Finish, "FB", kind, 0.03, 4));
        generate(&world, &mut h, "Finish");
        assert!((feed_tip(&world, "Finish") - 14.02).abs() < 1e-9, "lateral orbit does not deepen the floor");
        let sim = simulate_document(&world).expect("finish verdict");
        assert!(sim.dev.is_clean(), "{kind:?} finishes to size: {:?}", sim.dev);
        assert_eq!(sim.dev.gouge_cells, 0, "finish gouges nothing: {:?}", sim.dev);
        assert!(sim.dev.max_rest <= sim.dev.cell, "rest stays within resolution: {:?}", sim.dev);
    }
}

#[test]
fn orbit_that_would_overburn_the_cavity_is_refused_by_name() {
    for pass in [SinkerPass::Rough, SinkerPass::Finish] {
        let (world, mut h) = orbit_doc();
        let params = orbited(pass, "R1", SinkerOrbitKind::Circular, 0.21, 4);
        add(&world, &mut h, "Burn", ROUGH, params);
        let Err(err) = GenerateOperation::new(&world, "Burn") else { panic!("an overburn generated") };
        let message = err.to_string();
        assert!(message.contains("orbit") && message.contains("past size"), "{message}");
    }
}

#[test]
fn finish_orbit_without_full_axis_reach_is_refused_by_name() {
    for vectors in [1, 3] {
        let (world, mut h) = orbit_doc();
        let params = orbited(SinkerPass::Finish, "FB", SinkerOrbitKind::Vector, 0.03, vectors);
        add(&world, &mut h, "Finish", "E-ORBIT-F", params);
        let Err(err) = GenerateOperation::new(&world, "Finish") else { panic!("a partial-axis finish generated") };
        let message = err.to_string();
        assert!(message.contains("orbit") && message.contains("under size"), "{message}");
    }
}

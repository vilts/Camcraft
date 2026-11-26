//! Provenance: original tests over public Haas specification facts requested in #1976.
//! Gate map (M4.4): covers app::posting, app::machine, post::machine.

use camcraft_app::commands::{History, LoadMachine};
use camcraft_app::machine::{self, Kind, Machine, library, parse_mch};
use camcraft_app::post::machine::{mirror, script_props};
use flecs_ecs::prelude::*;

struct Spec {
    name: &'static str,
    travel: [f64; 3],
    rpm: f64,
    kw: f64,
    feed: f64,
    rapid: f64,
    slots: usize,
    mount: &'static str,
    holder: &'static str,
}

const SPECS: [Spec; 5] = [
    Spec {
        name: "Haas VF-2",
        travel: [762.0, 406.0, 508.0],
        rpm: 8100.0,
        kw: 22.4,
        feed: 16510.0,
        rapid: 25400.0,
        slots: 20,
        mount: "CAT40",
        holder: "CAT40 ER32",
    },
    Spec {
        name: "Haas VF-2SS",
        travel: [762.0, 406.0, 508.0],
        rpm: 12000.0,
        kw: 22.4,
        feed: 21158.0,
        rapid: 35560.0,
        slots: 30,
        mount: "CAT40",
        holder: "CAT40 ER32",
    },
    Spec {
        name: "Haas Mini Mill",
        travel: [406.0, 356.0, 381.0],
        rpm: 8000.0,
        kw: 5.6,
        feed: 10160.0,
        rapid: 10160.0,
        slots: 10,
        mount: "CAT40",
        holder: "CAT40 ER32",
    },
    Spec {
        name: "Haas DT-1",
        travel: [508.0, 406.0, 394.0],
        rpm: 10000.0,
        kw: 11.2,
        feed: 30480.0,
        rapid: 60960.0,
        slots: 20,
        mount: "BT30",
        holder: "BT30 ER32",
    },
    Spec {
        name: "Haas DM-1",
        travel: [508.0, 406.0, 394.0],
        rpm: 10000.0,
        kw: 11.2,
        feed: 30480.0,
        rapid: 60960.0,
        slots: 18,
        mount: "CAT40",
        holder: "CAT40 ER32",
    },
];

fn shipped(name: &str) -> Machine {
    let (_, text) = camcraft_app::posting::shipped_machines().expect("the shipped machines read")
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("{name} must appear in the machine picker"));
    let _: serde_json::Value = serde_json::from_str(text).expect("strict JSON");
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn every_haas_machine_lists_validates_and_mounts() {
    let entries = library::builtins();
    for s in &SPECS {
        let m = shipped(s.name);
        assert!(m.is_sane(), "{}", s.name);
        assert_eq!(machine::kind(Some(&m)), Kind::Mill);
        let e = entries
            .iter()
            .find(|e| e.name == s.name)
            .expect("picker entry");
        assert!(e.is_valid(), "{}: {:?}", s.name, e.refusals);
        assert!(e.posts.contains(&"Haas"), "{} offers the Haas post", s.name);
        let world = camcraft_app::world::new_world();
        let mut h = History::default();
        h.execute(
            &world,
            LoadMachine::builtin(&world, s.name).expect("mount built-in"),
        );
        assert_eq!(machine::document_of(&world).1, Some(m));
    }
}

#[test]
fn every_haas_machine_states_its_requested_configuration() {
    for s in &SPECS {
        let m = shipped(s.name);
        assert_eq!(m.vendor, "Camcraft");
        assert_eq!(m.model, s.name.to_uppercase());
        assert_eq!(m.axes.len(), 3, "{} has only X/Y/Z", s.name);
        assert_eq!(m.spindle_max, Some(s.rpm), "{} rpm", s.name);
        assert_eq!(m.spindle_power, Some(s.kw), "{} kW", s.name);
        assert_eq!(m.tool_count, Some(s.slots), "{} changer", s.name);
        assert_eq!(m.spindle_mount.as_deref(), Some(s.mount));
        assert_eq!(m.default_holder.as_deref(), Some(s.holder));
        assert_eq!(m.safe_retract_z, Some(0.0));
        assert_eq!(m.rigid_tapping, Some(true));
        assert_eq!(
            m.coolants
                .iter()
                .map(|c| (c.name.as_str(), c.m_code.as_str()))
                .collect::<Vec<_>>(),
            [("Flood", "M8")]
        );
        for (k, name) in ["X", "Y", "Z"].iter().enumerate() {
            let a = m.axis(k).expect("linear axis");
            assert_eq!(a.name, *name);
            assert!(!a.rotary);
            assert_eq!(
                a.travel,
                Some((-s.travel[k], s.travel[k])),
                "{} {name}",
                s.name
            );
            assert_eq!(a.feed, Some(s.feed), "{} {name} feed", s.name);
            assert_eq!(a.rapid, Some(s.rapid), "{} {name} rapid", s.name);
        }
        let p = script_props(&mirror(&m));
        assert_eq!(p["units"], "mm");
        assert_eq!(p["spindleMax"], s.rpm);
        assert_eq!(p["feedRateMax"], s.feed);
        assert_eq!(p["rapidRate"], s.rapid);
        assert_eq!(p["safeRetractZ"], 0.0);
        assert_eq!(p["rigidTapping"], true);
        assert_eq!(p["rtcp"], false);
    }
}

/// Synthetic cached straight moves isolate the machine checker from generators.
fn judged(m: &Machine, to: [f64; 3], feed: f64, rpm: f64) -> machine::MachineCheck {
    use camcraft_app::cl::{Move, MoveKind, Toolpath as ClPath};
    use camcraft_app::geometry::Point3;
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::{Name, Operation, ToolDef, Toolpath};
    let world = camcraft_app::world::new_world();
    let mut tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .expect("catalog tool")
        .1;
    tool.rpm = rpm;
    world.entity().set(Name("Tool".into())).set(ToolDef {
        tool,
        number: Some(1),
    });
    let op = world
        .entity()
        .set(Name("Limit pin".into()))
        .set(Operation {
            tool: "Tool".into(),
            src: SrcRef::Stock,
            stamp: None,
            setup: None,
            area: Default::default(),
            coolant: Default::default(),
            after: Default::default(),
            suppressed: false,
            params: OpParams::Face {
                holes: Default::default(),
                tilt: None,
                blend_turns: false, blend_radius: None,
                stepover: 1.0,
                clearance_z: Height::fixed(10.0),
                cut_z: Height::fixed(0.0),
                top_z: None,
                stepdown: None,
                feed,
                plunge_feed: feed,
                climb: true,
                picks: Default::default(),
            },
        })
        .set(Toolpath::new(
            ClPath::new(vec![Move::new(
                MoveKind::Feed,
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(to[0], to[1], to[2]),
                Some(feed),
            )]),
            vec![],
        ));
    machine::check_ops(&world, m, &[("Limit pin".into(), op.id())]).expect("machine check")
}

#[test]
fn haas_travel_feed_and_spindle_limits_are_inclusive_and_enforced() {
    for s in &SPECS {
        let m = shipped(s.name);
        for k in 0..3 {
            for sign in [-1.0, 1.0] {
                let mut to = [0.0; 3];
                to[k] = sign * s.travel[k];
                let at = judged(&m, to, s.feed, s.rpm);
                assert!(
                    at.is_ok(),
                    "{} axis {k} endpoint: {:?}",
                    s.name,
                    at.problems
                );
                to[k] += sign * 0.001;
                let past = judged(&m, to, s.feed, s.rpm);
                assert!(
                    past.problems.iter().any(|p| p.contains("stroke")),
                    "{}: {:?}",
                    s.name,
                    past.problems
                );
            }
            let mut to = [0.0; 3];
            to[k] = 1.0;
            let past = judged(&m, to, s.feed + 1.0, s.rpm);
            assert!(
                past.problems
                    .iter()
                    .any(|p| p.contains("feeds") && p.contains("tops out")),
                "{} axis {k}: {:?}",
                s.name,
                past.problems
            );
        }
        let past = judged(&m, [1.0, 0.0, 0.0], s.feed, s.rpm + 1.0);
        assert!(
            past.problems
                .iter()
                .any(|p| p.contains("rpm") && p.contains("above")),
            "{}: {:?}",
            s.name,
            past.problems
        );
    }
}

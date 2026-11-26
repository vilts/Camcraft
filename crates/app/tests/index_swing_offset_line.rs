//! #3127: the index-swing clause reads a work-turning rotary's line
//! through the WCS origin. A machine file that states a non-zero `pivot`
//! or `offset` for that rotary puts the line elsewhere: the clause
//! refuses the post by name, never judges the swing about the wrong
//! line. A zero pivot is judged as before.
//! Provenance: our own tests over our own commands and the shipped
//! microARC machine file (Tormach published spec numbers).
//!
//! Gate map (#3127): covers app::posting (index_swing, verification_gate).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SelectPost, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting::verification_gate;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// The shipped microARC with its A's `pivot` and `offset` as given.
fn microarc(pivot: Option<[f64; 3]>, offset: Option<[f64; 3]>) -> Machine {
    let text = camcraft_app::posting::builtin_machine_text("Tormach 770MX + microARC 4").expect("shipped");
    let mut m = parse_mch(text).expect("parses");
    let a = m.axes.iter_mut().find(|a| a.name == "A").expect("an A");
    a.pivot = pivot;
    a.offset = offset;
    m
}

/// The kit's 3/4" bar on `machine`, a 3" end mill and one flat at A90.
fn doc(machine: Machine) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::new(&world, "test".into(), Some(machine)), "machine"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    let at = Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() };
    h.execute(&world, ok(SetStock::new(&world, StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R }, at), "stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    t.holder_len = 3.0 * IN - t.flute_len - t.shank_len;
    h.execute(&world, ok(AddTool::new(&world, "EM 1/4", t), "tool"));
    let flat = OpParams::Slot {
        a: Point2::new(-0.75 * IN, 0.0),
        b: Point2::new(-0.25 * IN, 0.0),
        width: 0.25 * IN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: Some(Height::fixed(R + 0.1 * IN)),
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(R - 0.015 * IN),
        stepdown: 0.015 * IN,
        stepover: 0.1 * IN,
        feed: 15.0 * IN,
        plunge_feed: 4.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { basis_version: 1, a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }),
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    };
    h.execute(&world, ok(CreateOperation::new(&world, "FLAT", "EM 1/4", SrcRef::Stock, flat), "op"));
    h.execute(&world, ok(GenerateOperation::new(&world, "FLAT"), "generate"));
    world
}

fn off_line(world: &World) -> Vec<String> {
    verification_gate(world).into_iter().filter(|p| p.contains("pivot or offset")).collect()
}

#[test]
fn a_pivot_off_the_wcs_origin_refuses_the_index_swing_by_name() {
    let world = doc(microarc(Some([0.0, 0.0, 40.0]), None));
    let lines = off_line(&world);
    assert_eq!(lines.len(), 1, "{:?}", verification_gate(&world));
    let l = &lines[0];
    for text in ["FLAT:", "rotary A", "off the WCS origin", "index swing is not judged"] {
        assert!(l.contains(text), "{text}: {l}");
    }
}

#[test]
fn an_offset_off_the_wcs_origin_refuses_the_index_swing_by_name() {
    let world = doc(microarc(None, Some([0.0, 25.0, 0.0])));
    assert_eq!(off_line(&world).len(), 1, "{:?}", verification_gate(&world));
}

#[test]
fn a_zero_pivot_and_offset_judge_the_index_swing_as_before() {
    let world = doc(microarc(Some([0.0; 3]), Some([0.0; 3])));
    let gate = verification_gate(&world);
    assert!(off_line(&world).is_empty(), "{gate:?}");
    assert!(!gate.iter().any(|p| p.contains("index turns the work")), "the short tool clears: {gate:?}");
    let out = std::env::temp_dir().join(format!("camcraft_swing_zero_pivot_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "a zero pivot posts as before: {:?}", posted.err());
}

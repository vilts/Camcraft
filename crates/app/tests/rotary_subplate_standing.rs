//! #821 review fixes: a rotary subplate is judged from where it STANDS,
//! not only from its plate links - one put on a plate by position, or
//! left up after its plate went, still counts what is under it, and a
//! machine with no table refuses by name. A setup's stated
//! `nose_to_axis` never loosens the check: above the machine's own
//! figure, or above the machine's less the setup's own stack, it is
//! refused by name - also when a datum transfer carried it into a setup
//! with a taller stack.
//! Provenance: our own tests over our own commands; the microARC numbers
//! are the shipped machine file's (Tormach spec sheets); the subplate's
//! figures are generic, not the maker's.
//!
//! Gate map (M4.4): covers app::rotary_stack (where it stands,
//! check_stated), app::posting (index_swing, stated figure),
//! app::commands::setup (datum transfer of nose_to_axis).

use camcraft_app::accessory::{Accessory, subplate};
use camcraft_app::commands::{
    AccessoryPlacement, AddTool, CreateOperation, CreateSetup, DeleteFixture, GenerateOperation, History,
    InsertAccessory, InsertPlate, LoadMachine, PlaceOnPlate, PlatePlacement, SelectPost, SetFixtureFixed,
    SetOperationSetup, SetSetupNoseToAxis, SetStock,
};
use camcraft_app::fixture;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::plate::{self, MountAt};
use camcraft_app::posting::{MACHINES, index_swing::MARGIN_MM, verification_gate};
use camcraft_app::rotary_stack::{self, Rise, UNLINKED};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
const NOSE: f64 = 10.6 * IN;
const PLATE_T: f64 = 20.0;
const SUB_T: f64 = 0.75 * IN;

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn arc4() -> Machine {
    let text = MACHINES.iter().find(|(n, _)| *n == "Tormach 770MX + microARC 4").map(|(_, t)| *t).expect("shipped");
    parse_mch(text).expect("parses")
}

/// The rotary_subplate suite's document (20 mm to spare on the bare
/// table): the kit bar in the microARC, a 1/4" end mill, one flat at A90,
/// a declared setup - on machine `m`.
fn doc_on(m: Machine) -> (World, History) {
    let len = NOSE - R * std::f64::consts::SQRT_2 - MARGIN_MM - 20.0;
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::new(&world, "test".into(), Some(m)), "machine"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    let at = Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() };
    h.execute(&world, ok(SetStock::new(&world, StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R }, at), "stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    t.holder_len = len - t.flute_len - t.shank_len;
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
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] }),
        extend_start: 0.0,
        extend_end: 0.0,
    };
    h.execute(&world, ok(CreateOperation::new(&world, "FLAT", "EM 1/4", SrcRef::Stock, flat), "op"));
    h.execute(&world, ok(GenerateOperation::new(&world, "FLAT"), "generate"));
    h.execute(&world, ok(CreateSetup::new(&world, Some("Setup1"), None), "setup"));
    (world, h)
}

fn doc() -> (World, History) {
    doc_on(arc4())
}

fn table_top(world: &World, setup: Option<&str>) -> f64 {
    fixture::table_box(world, setup).expect("the microARC machine states a table").max.z
}

/// A fixed 300 x 300 mm dowel plate on the table of `setup`; its name.
fn plate_in(world: &World, h: &mut History, setup: Option<&str>) -> String {
    let p = plate::preset("300 x 300 mm").expect("preset").params();
    let at = Location { pos: Point3::new(-150.0, -150.0, table_top(world, setup) + PLATE_T), rot: IDENTITY_Q };
    let cmd = ok(InsertPlate::new(world, &p, setup, PlatePlacement::At(at)), "plate");
    let name = cmd.name().to_owned();
    h.execute(world, cmd);
    h.execute(world, ok(SetFixtureFixed::new(world, &name, true), "fixed"));
    name
}

fn subplate_params() -> Accessory {
    ok(subplate::generic().with_dim("bore_spacing", 100.0), "dims")
}

/// The subplate with its underside at height `z`, by position only.
fn subplate_at(world: &World, h: &mut History, setup: Option<&str>, z: f64) -> String {
    let at = Location { pos: Point3::new(-75.0, -25.0, z), rot: IDENTITY_Q };
    let cmd = ok(InsertAccessory::new(world, &subplate_params(), setup, AccessoryPlacement::At(at)), "subplate");
    let name = cmd.name().to_owned();
    h.execute(world, cmd);
    name
}

fn on_dowels() -> MountAt {
    MountAt::Dowels(["C2".into(), "C4".into()])
}

fn swing_lines(world: &World) -> Vec<String> {
    verification_gate(world)
        .into_iter()
        .filter(|p| p.contains("index turns the work") || p.contains("nose_to_axis") || p.contains("index swing"))
        .collect()
}

/// The clearance the 39.05 mm stack leaves the 20 mm-to-spare tool.
fn stacked_clearance() -> String {
    format!("a clearance of {:.3} mm", 20.0 + MARGIN_MM - (SUB_T + PLATE_T))
}

fn unlinked_stack(world: &World, s: &str) -> rotary_stack::Stack {
    match rotary_stack::rise(world, None) {
        Rise::Stack(st) => {
            assert!((st.thickness - (SUB_T + PLATE_T)).abs() < 1e-6, "{st:?}");
            assert_eq!(st.layers.len(), 2, "{st:?}");
            assert_eq!(st.layers[0], (s.to_owned(), SUB_T), "{st:?}");
            assert_eq!(st.layers[1].0, UNLINKED, "{st:?}");
            assert!((st.layers[1].1 - PLATE_T).abs() < 1e-6, "{st:?}");
            st
        }
        other => panic!("judged from where it stands: {other:?}"),
    }
}

#[test]
fn a_subplate_put_on_a_plate_by_position_counts_the_plate() {
    let (world, mut h) = doc();
    let _p = plate_in(&world, &mut h, None);
    let s = subplate_at(&world, &mut h, None, table_top(&world, None) + PLATE_T);
    let st = unlinked_stack(&world, &s);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "never the 19.05 mm of the subplate alone: {lines:?}");
    assert!(lines[0].contains(&stacked_clearance()), "{}", lines[0]);
    assert!(lines[0].contains(&st.describe()), "{}", lines[0]);
}

#[test]
fn a_subplate_left_up_after_its_plate_went_still_counts_the_height() {
    let (world, mut h) = doc();
    let p = plate_in(&world, &mut h, None);
    let s = subplate_at(&world, &mut h, None, table_top(&world, None));
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s, &p, on_dowels(), 0.0), "place"));
    assert_eq!(swing_lines(&world).len(), 1, "the linked stack is refused");
    h.execute(&world, ok(DeleteFixture::new(&world, &p), "delete the plate"));
    // Unlinked, but still 20 mm up: the rise does not drop back to 19.05.
    unlinked_stack(&world, &s);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&stacked_clearance()), "{}", lines[0]);
    assert!(h.undo(&world));
    match rotary_stack::rise(&world, None) {
        Rise::Stack(st) => assert_eq!(st.layers[1], (p, PLATE_T), "undo restores the link: {st:?}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_machine_with_no_table_refuses_a_subplate_by_name() {
    let mut m = arc4();
    for a in m.axes.iter_mut().filter(|a| a.name.eq_ignore_ascii_case("X")) {
        a.travel = None;
    }
    let (world, mut h) = doc_on(m);
    assert!(fixture::table_box(&world, None).is_none());
    let s = subplate_at(&world, &mut h, None, -200.0);
    let lines = swing_lines(&world);
    assert!(
        lines.iter().any(|l| l.contains(&format!(
            "the setup's machine states no table, so where rotary subplate '{s}' stands cannot be judged"
        ))),
        "{lines:?}"
    );
}

#[test]
fn a_stated_figure_never_loosens_the_check_past_the_machine_or_the_stack() {
    let (world, mut h) = doc();
    let p = plate_in(&world, &mut h, None);
    let s = subplate_at(&world, &mut h, None, table_top(&world, None));
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s, &p, on_dowels(), 0.0), "place"));
    // Above the machine's own bare-table figure: refused by name.
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE + 100.0)), "stated"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let want = format!("FLAT: the setup's stated nose_to_axis {:.3} mm exceeds the machine's own {NOSE:.3} mm", NOSE + 100.0);
    assert!(lines[0].starts_with(&want), "{}", lines[0]);
    assert!(lines[0].contains("the A90.000 index swing is not judged"), "{}", lines[0]);
    // Under the bare figure but above it less the 39.05 mm stack: refused.
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE - 10.0)), "stated"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let want = format!(
        "exceeds the machine's {NOSE:.3} mm less the {:.3} mm stack under the rotary ('{s}' 19.050 mm on '{p}' 20.000 mm)",
        SUB_T + PLATE_T
    );
    assert!(lines[0].contains(&want), "{}", lines[0]);
    // At or under it: the measured figure is judged, and tighter.
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE - 45.0)), "stated"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the A90.000 index turns the work"), "{}", lines[0]);
    assert!(lines[0].contains(&format!("(the setup's stated nose_to_axis {:.3} mm)", NOSE - 45.0)), "{}", lines[0]);
}

#[test]
fn a_transferred_stated_figure_is_rechecked_against_the_new_setup_s_stack() {
    let (world, mut h) = doc();
    // Setup1: the subplate alone on the table, measured exactly.
    let _s1 = subplate_at(&world, &mut h, Some("Setup1"), table_top(&world, Some("Setup1")));
    h.execute(&world, ok(SetSetupNoseToAxis::new(&world, "Setup1", Some(NOSE - SUB_T)), "stated"));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // Setup2 takes Setup1's datum - and its stated figure - but stands
    // its subplate on a plate: 39.05 mm, taller than the figure allows.
    h.execute(&world, ok(CreateSetup::new(&world, Some("Setup2"), Some("Setup1")), "transfer"));
    assert_eq!(rotary_stack::stated(&world, Some("Setup2")), Some(NOSE - SUB_T), "the figure rides the transfer");
    let p = plate_in(&world, &mut h, Some("Setup2"));
    let s2 = subplate_at(&world, &mut h, Some("Setup2"), table_top(&world, Some("Setup2")));
    h.execute(&world, ok(PlaceOnPlate::new(&world, &s2, &p, on_dowels(), 0.0), "place"));
    h.execute(&world, ok(SetOperationSetup::new(&world, "FLAT", Some("Setup2")), "move the op"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("stated nose_to_axis {:.3} mm exceeds the machine's", NOSE - SUB_T)), "{}", lines[0]);
    assert!(lines[0].contains(&format!("less the {:.3} mm stack", SUB_T + PLATE_T)), "{}", lines[0]);
}

//! M13.5 A axial-drilling tests at the app level: the lathe's own
//! hole, down the spindle axis from the turret. What "right" means
//! here: the Drill kind enters a plain-lathe document (the family
//! door's one per-kind exception) while every other mill kind still
//! refuses by name; a typed on-axis hole over a bored bar generates
//! Ok and silent under `Mode::Revolve`, with one cycle record at
//! X0 Y0 and the tip at the modeled floor, and the program posts
//! with the cycle vocabulary; the three doors refuse by name (a hole
//! off the axis, a hole past the bar's far end, a drill wider than
//! the bore); hole recognition sees the axial bore for the auto-fill;
//! and a drill where the model has no hole is convicted, never
//! silent. The doors' and the verdict's own laws live in
//! camcraft-motion's drilling `axial`; this file is the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the axial arm of
//! generate, holes_of), camcraft_app::commands (family gate,
//! Create/GenerateOperation), camcraft_app::machine, post::machine (the lathe kind),
//! camcraft_app::posting (the cycle words on a lathe document).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const BLANK: f64 = 10.0;
/// The modeled bore stands a hair wider than the 4 mm drill's radius,
/// and the revolve is cut 64 ways: the 64-gon's inscribed radius
/// (2.0475) clears the drill's 2.0, so the post gate's material sim
/// never reads a chord sliver of the tessellated wall as cut model.
const BORE: f64 = 2.05;
const SEGS: usize = 64;
const TOP: f64 = 20.0;
const FLOOR: f64 = 10.0;
const END: f64 = -20.0;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

/// The closed-solid revolve the turning suites share.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / SEGS as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..SEGS {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The bar with a centre hole: OD 10, a blind bore of radius 2 from
/// the top face at z = 20 down to a floor at z = 10, the bar's far
/// end at z = -20.
fn bored() -> Mesh {
    revolve(&[(0.0, END), (BLANK, END), (BLANK, TOP), (BORE, TOP), (BORE, FLOOR), (0.0, FLOOR)])
}

/// The plain bar: no hole for a drill to answer to.
fn plain() -> Mesh {
    revolve(&[(0.0, END), (BLANK, END), (BLANK, TOP), (0.0, TOP)])
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

fn lathe_world() -> (World, History) {
    lathe_world_with(bored())
}

/// A lathe document holding exactly ONE model named "bar" - the
/// program sim judges against every model in the document, so a
/// second bar on the same axis would stand in for the first's hole.
fn lathe_world_with(bar: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // The bar's own envelope, z END..TOP (issue #91: the old box
            // stood z 0..44, its top 24 mm over the hole's R plane - the
            // drill's rapid down to R crossed declared stock).
            camcraft_app::stock::StockShape::Box { dx: 24.0, dy: 24.0, dz: TOP - END },
            Location { pos: Point3::new(0.0, 0.0, END), ..Default::default() },
        )
        .expect("box stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, "DR-0400");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, history)
}

fn hole(x: f64, y: f64, radius: f64) -> Hole {
    Hole { xy: Point2::new(x, y), radius, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false }
}

fn axial_params(holes: Vec<Hole>, bottom_z: Option<Height>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes,
        cross: vec![],
    }
}

fn op(src: &str, params: OpParams) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity(src.into()),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, "DR-0400", SrcRef::Entity("bar".into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-axial-{}-{}.nc", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn an_axial_drill_runs_clean_on_the_lathe_and_posts_its_cycle() {
    let (world, mut history) = lathe_world();
    create(&world, &mut history, "AX1", axial_params(vec![hole(0.0, 0.0, BORE)], None));
    let c = cache(&world, "AX1").expect("cached");
    assert_eq!(status(&world, "AX1"), OpStatus::Ok, "the meridian verdict is silent: {:?}", c.warnings);
    assert!(c.warnings.is_empty(), "a matched axial drill reported: {:?}", c.warnings);
    let t = c.path;
    assert_eq!(t.cycles.len(), 1, "one cycle record for the one hole");
    assert!(t.moves.iter().all(|m| m.to.x.abs() < 1e-9 && m.to.y.abs() < 1e-9), "every move on the spindle axis");
    let tip = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).fold(f64::MAX, f64::min);
    assert!((tip - FLOOR).abs() < 1e-9, "the tip reaches the modeled floor: {tip}");
    assert!(t.moves.iter().all(|m| m.axis.is_none()), "the turret's drill never leans");

    let nc = tmp_nc("clean");
    posting::post_setup(&world, &nc).expect("a clean lathe hole posts");
    let text = std::fs::read_to_string(&nc).expect("written");
    let _ = std::fs::remove_file(&nc);
    assert!(
        ["G81", "G82", "G83", "G73"].iter().any(|w| text.contains(w)),
        "the cycle word posted (the mill dialect's, M13.5 limit): {text}"
    );
}

#[test]
fn the_family_door_admits_the_drill_on_a_lathe_and_no_other_mill_kind() {
    let (world, _h) = lathe_world();
    CreateOperation::new(&world, "AX", "DR-0400", SrcRef::Entity("bar".into()), axial_params(vec![hole(0.0, 0.0, BORE)], None))
        .expect("the lathe's own hole enters");
    let carried = operation::Carried {
        feed: 150.0,
        plunge: 80.0,
        climb: true,
        clearance: Height::fixed(30.0),
        stepover: 2.0,
        stepdown: 2.0,
        top: Height::fixed(TOP),
        bottom: Height::fixed(FLOOR),
        retract: Height::fixed(25.0),
        tilt: None,
    };
    let pocket = (operation::KindInfo::ALL[operation::OpType::Pocket.ordinal()].seed)(&carried);
    let e = match CreateOperation::new(&world, "PK", "EM-0400-2FL", SrcRef::Entity("bar".into()), pocket) {
        Ok(_) => panic!("a pocket is milling work; the lathe must refuse it"),
        Err(e) => e,
    };
    assert!(format!("{e}").contains("milling work"), "{e}");
}

#[test]
fn an_off_axis_hole_refuses_by_name() {
    let (world, _h) = lathe_world();
    let e = operation::generate(&world, &op("bar", axial_params(vec![hole(3.0, 0.0, BORE)], None)), "")
        .expect_err("the turret drills on the axis only");
    assert!(format!("{e}").contains("off the spindle axis"), "{e}");
    // #677: the refusal marks the off-axis hole's mouth.
    let at = e.location();
    assert!(at.is_some_and(|p| p.x == 3.0 && p.y == 0.0 && p.z.is_finite()), "marks the hole at (3, 0): {at:?}");
}

#[test]
fn a_hole_past_the_bar_end_refuses_by_name() {
    let (world, _h) = lathe_world();
    let e = operation::generate(
        &world,
        &op("bar", axial_params(vec![hole(0.0, 0.0, BORE)], Some(Height::fixed(END - 5.0)))),
        "",
    )
    .expect_err("the drill would leave through the chuck side");
    assert!(format!("{e}").contains("past the bar's far end"), "{e}");
    // #677: the refusal marks the hole's floor, past the bar's end.
    assert_eq!(e.location(), Some(Point3::new(0.0, 0.0, END - 5.0)), "{e}");
}

#[test]
fn a_drill_wider_than_the_bore_refuses_by_name() {
    let (world, _h) = lathe_world();
    let e = operation::generate(&world, &op("bar", axial_params(vec![hole(0.0, 0.0, 1.5)], None)), "")
        .expect_err("a 4 mm drill cannot make a 3 mm bore");
    assert!(format!("{e}").contains("wider drill"), "{e}");
}

#[test]
fn recognition_sees_the_axial_bore_for_the_auto_fill() {
    let (world, _h) = lathe_world();
    let found = operation::holes_of(&world, None, &SrcRef::Entity("bar".into())).expect("read");
    assert_eq!(found.len(), 1, "one hole on the axis: {found:?}");
    let h = found[0];
    assert!(h.xy.x.abs() < 0.05 && h.xy.y.abs() < 0.05, "on the axis: {:?}", h.xy);
    assert!((h.radius - BORE).abs() < 0.05, "the bore's radius: {}", h.radius);
    assert!((h.floor_z - FLOOR).abs() < 0.05 && !h.through, "blind to the modeled floor: {h:?}");
}

#[test]
fn a_drill_where_the_model_has_no_hole_is_convicted() {
    let (world, mut history) = lathe_world_with(plain());
    create(&world, &mut history, "AXP", axial_params(vec![hole(0.0, 0.0, BORE)], None));
    let c = cache(&world, "AXP").expect("cached");
    assert!(
        c.warnings.iter().any(|w| matches!(w, camcraft_app::verify::Warning::Gouge { .. }) && w.blocks_post()),
        "a hole the model does not have is convicted, never silent: {:?}",
        c.warnings
    );
}

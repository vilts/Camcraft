//! M5.2 stage E contour turning tests at the app level: the wiring -
//! a promised-face ride's whole life through the document. What
//! "right" means here: chamfering, taper and multi-pass on a turning
//! insert, on a mounted lathe, over a model that KEEPS the promise,
//! generate Ok with the envelope verdict silent; a promise the model
//! does not keep - a chamfer window on a square corner, a lying
//! angle, diameters the cone is not - refuses by name through the
//! generate command, never a ride invented from the numbers' air;
//! the multi-pass program runs ladder and spring rides as one
//! cached path; the wrong tool names itself at the generator's
//! door, an upward window at the create door, and both a parameter
//! edit and a nose re-grind ride the history Stale-and-back. The
//! kernels' own laws are pinned one crate down (camcraft-motion's
//! turning_contour suite); this file is the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the contour arms
//! of generate, the envelope verdict for windowed face rides, the
//! general tool digest's nose line, the seeds), camcraft_app::
//! commands (Create/GenerateOperation, ChangeTool,
//! EditOperationParams undo).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, EditOperationParams, GenerateOperation, History,
    ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Carried, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const NOSE: f64 = 0.8;
const TOL: f64 = 0.05;

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
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

fn rho(q: Point3) -> f64 {
    (q.x * q.x + q.y * q.y).sqrt()
}

fn feeds(t: &camcraft_app::cl::Toolpath) -> Vec<&camcraft_app::cl::Move> {
    t.moves
        .iter()
        .filter(|m| m.kind != camcraft_app::cl::MoveKind::Rapid)
        .collect()
}

/// The plunging rides of a program: feed moves that open with a
/// rapid - one per ladder rung and per spring ride.
fn blocks(t: &camcraft_app::cl::Toolpath) -> usize {
    t.moves
        .windows(2)
        .filter(|w| w[0].kind == camcraft_app::cl::MoveKind::Rapid && w[1].kind != camcraft_app::cl::MoveKind::Rapid)
        .count()
}

/// The closed-solid revolve the turning suites share.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// r 10 wall with a true 45-degree chamfer across the bottom
/// corner, (r 8, z -16) to (r 10, z -14): the motion suite's own
/// promised face.
fn chamfered() -> Mesh {
    revolve(&[(0.0, -16.0), (8.0, -16.0), (10.0, -14.0), (10.0, 40.0), (0.0, 40.0)])
}

/// The same bar with no chamfer at all: the promise the door must
/// refuse.
fn wall() -> Mesh {
    revolve(&[(0.0, -16.0), (10.0, -16.0), (10.0, 40.0), (0.0, 40.0)])
}

/// A full taper, dia 20 at z -16 to dia 16 at z 40.
fn tapered() -> Mesh {
    revolve(&[(0.0, -16.0), (10.0, -16.0), (8.0, 40.0), (0.0, 40.0)])
}

/// The stepped bar of the profile suite - wall r 6 to z 2, neck
/// r 5 to z 16 - the ladder's honest bar.
fn stepped() -> Mesh {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the contour kinds live in: the four bars in bar-sized
/// stock, the lathe mounted, the turning insert and the wrong
/// steels on the rack.
fn contour_world() -> (World, History) {
    contour_world_in(20.0, 56.0)
}

/// [`contour_world`] with bar stock `diameter` wide from Z -16 up
/// `height`: the stepped bar (r 6) is roughed from a bar its own size,
/// never the r 10 blank the other bars stand in (#103, #130).
fn contour_world_in(diameter: f64, height: f64) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // Bar-sized stock (#103): the widest bar's diameter (r 10)
            // over the bars' Z span - the turning kernels never cut
            // stock beyond the part (#130), so a wider blank is a
            // real crash.
            camcraft_app::stock::StockShape::Cylinder { diameter, height },
            Location { pos: camcraft_app::geometry::Point3::new(0.0, 0.0, -16.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "GT-0300");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", chamfered()));
    history.execute(&world, ImportModel::from_mesh("wall", wall()));
    history.execute(&world, ImportModel::from_mesh("taper", tapered()));
    history.execute(&world, ImportModel::from_mesh("stepped", stepped()));
    (world, history)
}

/// [`contour_world`] with the stock taken from the taper itself: a blank
/// already turned to the cone (#159). On the r 10 bar a taper ride alone
/// leaves the ring between its nose and the bar at the cone's small end,
/// and its radial retract there drives through it - a real crash, pinned
/// below; a finishing ride on a pre-turned cone has no such ring.
fn taper_blank_world() -> (World, History) {
    let (world, mut history) = contour_world();
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::FromModel { source: "taper".into() }, Location::default());
    history.execute(&world, cmd.expect("stock from the cone"));
    (world, history)
}

fn ch_params(angle_deg: f64) -> OpParams {
    OpParams::Chamfering {
        tolerance: TOL,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(-14.0),
        bottom_z: Height::fixed(-16.0),
        angle_deg,
        feed: 200.0,
        plunge_feed: 50.0,
        picks: Default::default(),
    }
}

fn tp_params(bottom_dia: f64, top_dia: f64) -> OpParams {
    OpParams::TaperTurning {
        tolerance: TOL,
        clearance_z: Height::fixed(48.0),
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(-16.0),
        bottom_dia,
        top_dia,
        feed: 200.0,
        plunge_feed: 50.0,
        picks: Default::default(),
    }
}

fn mp_params() -> OpParams {
    OpParams::MultiPassTurning {
        tolerance: TOL,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.5,
        stock_to_leave: 0.25,
        passes: 3,
        feed: 200.0,
        plunge_feed: 50.0,
        picks: Default::default(),
    }
}

fn create_on(
    world: &World,
    history: &mut History,
    name: &str,
    tool: &str,
    src: &str,
    params: OpParams,
) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

#[test]
fn a_chamfered_face_rides_clean_in_the_document() {
    // One promised face, one ride: the pass traces the MODEL's cone
    // (the authored 45 spends itself at the door), the envelope
    // verdict stands silent because the ride is tangent everywhere
    // it claims, and the rapids clear every CL point.
    let (world, mut history) = contour_world();
    create_on(&world, &mut history, "CH1", "TN-1600-0R8", "bar", ch_params(45.0));
    let c = cache(&world, "CH1").expect("cached");
    assert_eq!(status(&world, "CH1"), OpStatus::Ok, "the verdict is silent");
    assert!(c.warnings.is_empty(), "a clean face ride reported: {:?}", c.warnings);
    let t = c.path;
    assert_eq!(blocks(&t), 1, "a face ride is one plunging pass");
    let fs = feeds(&t);
    assert!(fs.len() >= 2, "a cone face chords to more than a point");
    for m in &fs {
        let (r, z) = (rho(m.to), m.to.z);
        assert!(
            (-16.0 - NOSE - TOL..=-14.0 + NOSE + TOL).contains(&z) && r >= 8.0 - TOL
                && r <= 10.0 + NOSE * 2f64.sqrt() + TOL,
            "the ride stands off the face it was promised: r {r} at z {z}"
        );
    }
    for m in t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Rapid) {
        assert!(rho(m.to) >= 10.0 + TOL - 1e-9 || m.to.z > 23.9, "a low rapid: {:?}", m.to);
    }
    assert!(t.is_chained(), "the chain holds");
}

#[test]
fn a_square_corner_refuses_the_chamfer_promise_by_name() {
    // The window promises a cone; the model is a cylinder to the
    // corner. The pass would chase a face that is not there, so
    // the door says what it needs - a MODELED chamfer - instead.
    let (world, mut history) = contour_world();
    history.execute(
        &world,
        CreateOperation::new(&world, "CW1", "TN-1600-0R8", SrcRef::Entity("wall".into()), ch_params(45.0))
            .expect("the door is numbers-only"),
    );
    let Err(err) = GenerateOperation::new(&world, "CW1") else {
        panic!("a square corner has no chamfer to ride");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("not a 45.0-degree chamfer") && msg.contains("MODELED chamfer"),
        "{msg}"
    );
}

#[test]
fn a_lying_angle_refuses_through_the_generate_command() {
    // The face IS there, but 30 degrees authored against a 45
    // model is a line the skin stands most of a millimetre off -
    // the door measures, not the ride.
    let (world, mut history) = contour_world();
    history.execute(
        &world,
        CreateOperation::new(&world, "CL1", "TN-1600-0R8", SrcRef::Entity("bar".into()), ch_params(30.0))
            .expect("a finite angle is a number"),
    );
    let Err(err) = GenerateOperation::new(&world, "CL1") else {
        panic!("30 is not 45");
    };
    assert!(err.to_string().contains("not a 30.0-degree chamfer"), "{err}");
}

#[test]
fn a_taper_rides_the_modeled_cone_and_a_square_bar_refuses() {
    // The two diameters state the cone; the ride walks the model's
    // offset bottom-up, a nose off the skin at both ends. The same
    // numbers over a cylinder are a promise the model does not
    // keep, and that refusal reads by name too.
    // On the Ø20 bar the ride ends at r 8.8 on Z 40 - its nose clears
    // r <= 9.6 there, the bar stands to r 10 - and the radial retract
    // out to r 10.85 drives through that ring: convicted by name.
    let (world, mut history) = contour_world();
    create_on(&world, &mut history, "TP0", "TN-1600-0R8", "taper", tp_params(20.0, 16.0));
    let c = cache(&world, "TP0").expect("cached");
    let last = c.warnings.iter().find_map(|w| match w {
        camcraft_app::verify::Warning::RapidInStock { at } => Some(*at),
        _ => None,
    });
    let m = last.and_then(|at| c.path.moves.get(at)).expect("the retract through the ring is named");
    assert!(m.kind == camcraft_app::cl::MoveKind::Rapid && rho(m.from) < 9.0 && rho(m.to) > 10.0, "the radial retract: {m:?}");
    let (world, mut history) = taper_blank_world();
    create_on(&world, &mut history, "TP1", "TN-1600-0R8", "taper", tp_params(20.0, 16.0));
    let c = cache(&world, "TP1").expect("cached");
    assert_eq!(status(&world, "TP1"), OpStatus::Ok);
    assert!(c.warnings.is_empty(), "the cone ride reported: {:?}", c.warnings);
    let fs = feeds(&c.path);
    assert!(fs[0].to.z < fs[fs.len() - 1].to.z, "the ride walks bottom-up");
    assert!((rho(fs[0].to) - (10.0 + NOSE)).abs() <= 2.0 * TOL + 1e-9, "start: {}", rho(fs[0].to));
    assert!((rho(fs[fs.len() - 1].to) - (8.0 + NOSE)).abs() <= 2.0 * TOL + 1e-9, "end: {}", rho(fs[fs.len() - 1].to));

    history.execute(
        &world,
        CreateOperation::new(&world, "TV1", "TN-1600-0R8", SrcRef::Entity("wall".into()), tp_params(20.0, 16.0))
            .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "TV1") else {
        panic!("the wall has no cone");
    };
    let msg = err.to_string();
    assert!(msg.contains("not the authored cone") && msg.contains("MODELED taper"), "{msg}");
}

#[test]
fn the_multipass_program_runs_ladder_and_springs_silent() {
    // One cached path: the roughing ladder down to the leave, then
    // three spring rides halving toward the profile - four plunging
    // blocks, each strictly inside the last, the last standing a
    // nose off the model. The envelope verdict measures the union
    // and says nothing: the skin is tangent, and nothing stands.
    let (world, mut history) = contour_world_in(12.0, 32.0);
    create_on(&world, &mut history, "MP1", "TN-1600-0R8", "stepped", mp_params());
    let c = cache(&world, "MP1").expect("cached");
    assert_eq!(status(&world, "MP1"), OpStatus::Ok);
    assert!(c.warnings.is_empty(), "the ladder program reported: {:?}", c.warnings);
    let t = c.path;
    assert!(blocks(&t) >= 4, "rung plus three springs: {}", blocks(&t));
    let fs = feeds(&t);
    assert!(t.is_chained());
    // The closing ride's last point stands off the neck's r 5 skin.
    assert!(
        rho(fs[fs.len() - 1].to) >= 5.0 + NOSE - 2.0 * TOL,
        "the finish floats: {}",
        rho(fs[fs.len() - 1].to)
    );
}

#[test]
fn contour_kinds_refuse_the_wrong_tool_by_name() {
    // The family door admits the KIND to a lathe; the tool door is
    // the generator's, and a contour ride's steel is the nose
    // circle - a blade edge or a fluted mill cannot trace it.
    let (world, mut history) = contour_world();
    common::create_past_the_tool_law(&world, &mut history, "CT1", "GT-0300", SrcRef::Entity("bar".into()), ch_params(45.0));
    let Err(err) = common::generate_past_the_tool_law(&world, "CT1") else {
        panic!("a blade edge cannot ride a cone face");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("needs a turning insert") && msg.contains("GT-0300") && msg.contains("grooving blade"),
        "{msg}"
    );
    common::create_past_the_tool_law(&world, &mut history, "CT2", "EM-0400-2FL", SrcRef::Entity("taper".into()), tp_params(20.0, 16.0));
    let Err(err) = common::generate_past_the_tool_law(&world, "CT2") else {
        panic!("an end mill cannot turn a taper");
    };
    assert!(err.to_string().contains("Taper Turning needs a turning insert"), "{err}");
}

#[test]
fn the_seeds_of_the_contour_kinds_stand() {
    // Registry end to end: pick each kind, accept its seed, and
    // the bar modeled to the seed's own numbers takes the whole
    // ride with the verdict silent - the defaults must be a
    // program someone can create by two clicks and run.
    let (world, mut history) = contour_world();
    let carried = |top: f64, bottom: f64, clearance: f64| Carried {
        feed: 200.0,
        plunge: 50.0,
        climb: true,
        clearance: Height::fixed(clearance),
        stepover: 0.0,
        stepdown: 0.0,
        top: Height::fixed(top),
        bottom: Height::fixed(bottom),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let seed = |k: OpType, c: Carried| (KindInfo::ALL[k as usize].seed)(&c);
    let s = seed(OpType::Chamfering, carried(-14.0, -16.0, 24.0));
    assert!(s.is_sane(), "the chamfer seed must pass its own door");
    create_on(&world, &mut history, "SD1", "TN-1600-0R8", "bar", s);
    let c = cache(&world, "SD1").expect("cached");
    assert_eq!(status(&world, "SD1"), OpStatus::Ok, "the chamfer seed's numbers stand");
    assert!(c.warnings.is_empty(), "the seeded face reported: {:?}", c.warnings);
    let s = seed(OpType::TaperTurning, carried(40.0, -16.0, 48.0));
    // The cone's finishing ride on a blank turned to it (#159).
    let (world, mut history) = taper_blank_world();
    create_on(&world, &mut history, "SD2", "TN-1600-0R8", "taper", s);
    let c = cache(&world, "SD2").expect("cached");
    assert_eq!(status(&world, "SD2"), OpStatus::Ok, "the taper seed states dia 20/16 and the bar keeps it");
    assert!(c.warnings.is_empty(), "the seeded cone reported: {:?}", c.warnings);
    let s = seed(OpType::MultiPassTurning, carried(16.0, -16.0, 30.0));
    // The stepped bar in a bar of its own size (#103).
    let (world, mut history) = contour_world_in(12.0, 32.0);
    create_on(&world, &mut history, "SD3", "TN-1600-0R8", "stepped", s);
    let c = cache(&world, "SD3").expect("cached");
    assert_eq!(status(&world, "SD3"), OpStatus::Ok);
    assert!(c.warnings.is_empty(), "the seeded ladder reported: {:?}", c.warnings);
}

#[test]
fn an_angle_edit_rides_the_history_both_ways() {
    // Everything-is-a-command: the pane's edit is
    // EditOperationParams, the cache marks itself Stale against
    // the new number - and undo lands the old ride exactly. The
    // lie (30 against a 45 face) only speaks at REgenerate; a
    // Stale op is an honest not-yet, never a silent wrong.
    let (world, mut history) = contour_world();
    create_on(&world, &mut history, "CH1", "TN-1600-0R8", "bar", ch_params(45.0));
    assert_eq!(status(&world, "CH1"), OpStatus::Ok);
    history.execute(
        &world,
        EditOperationParams::new(&world, "CH1", ch_params(30.0)).expect("a legal number"),
    );
    assert_eq!(status(&world, "CH1"), OpStatus::Stale, "the cached ride is the old cone's");
    assert!(history.undo(&world), "undo the edit");
    let e = model::find_by_name(&world, "CH1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Chamfering { angle_deg, .. } = &o.params else { unreachable!() };
    assert_eq!(*angle_deg, 45.0, "the face's number is back");
    assert_eq!(status(&world, "CH1"), OpStatus::Ok, "and the cached path matches it again");
}

#[test]
fn a_nose_regrind_marks_the_rides_stale_both_ways() {
    // The contour kinds ride the GENERAL tool digest - their steel
    // is just the nose circle - so re-grinding TN's radius marks
    // every contour op Stale, the honest read the profile kinds
    // already gave. Regenerate on the 0.9 nose (the door never
    // reads the nose, and the ball still stands tangent to the
    // face) and undo reads Stale again.
    let (world, mut history) = contour_world();
    create_on(&world, &mut history, "CH1", "TN-1600-0R8", "bar", ch_params(45.0));
    create_on(&world, &mut history, "TP1", "TN-1600-0R8", "taper", tp_params(20.0, 16.0));
    assert_eq!(status(&world, "CH1"), OpStatus::Ok);
    let mut worn = catalog_tool("TN-1600-0R8");
    worn.nose_radius = 0.9;
    history.execute(&world, ChangeTool::new(&world, "TN-1600-0R8", worn).expect("a sane insert"));
    assert_eq!(status(&world, "CH1"), OpStatus::Stale, "a different nose is a different skin");
    assert_eq!(status(&world, "TP1"), OpStatus::Stale);
    history.execute(&world, GenerateOperation::new(&world, "CH1").expect("regenerate"));
    assert_eq!(status(&world, "CH1"), OpStatus::Ok, "the 0.9 ball still rides the same cone");
    assert!(cache(&world, "CH1").expect("cached").warnings.is_empty(), "the regrind verdict spoke");
    assert!(history.undo(&world), "undo the grind");
    assert_eq!(status(&world, "TP1"), OpStatus::Stale, "and the cached 0.9 path reads stale now");
}

#[test]
fn an_upward_window_refuses_at_the_create_door() {
    // The ride reads stations downward - top below bottom is no
    // face and no window, and the door says it before any
    // geometry runs (AGENTS.md: validate early).
    let (world, _history) = contour_world();
    let params = OpParams::Chamfering {
        tolerance: TOL,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(-16.0),
        bottom_z: Height::fixed(-14.0),
        angle_deg: 45.0,
        feed: 200.0,
        plunge_feed: 50.0,
        picks: Default::default(),
    };
    let Err(err) =
        CreateOperation::new(&world, "CU1", "TN-1600-0R8", SrcRef::Entity("bar".into()), params)
    else {
        panic!("a window that cuts upward is no window");
    };
    assert!(err.to_string().contains("must cut downward"), "{err}");
}

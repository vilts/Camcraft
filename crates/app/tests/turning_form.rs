//! M5.2 stage E2 form-turning tests at the app level: the wiring -
//! a form op's whole life through the document. What "right" means
//! here: a crown gauge on a modeled valley, on a mounted lathe,
//! generates Ok with the crown verdict silent; the cached path is
//! the kernel's signature seen from the world - one straight
//! plunge per formed surface, tip at the floor the MODEL states,
//! rapids above the blank; the wrong tool on a form kind is
//! refused by name, and so is a smooth bar - a form op promises a
//! MODELED formed surface; re-grinding the gauge's crown convicts
//! through the TOOL digest (Stale both ways) and, when the new
//! crown cannot match the old valley, through the verdict
//! channel - never a silent Ok. The kernel's own laws are pinned one
//! crate down (camcraft-motion's turning_form suite); this file is
//! the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the form arm
//! of generate, the crown verdict wiring, the stamp's kind-scoped
//! width digest and the global nose_radius row), camcraft_app::
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
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const BLANK: f64 = 10.0;
const FLOOR: f64 = 8.0;
const CROWN: f64 = 6.0;

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

/// The formed bar, app-sized: wall r 10 with one crown-6 valley
/// centered at z 10 (floor r 8), on a bar long past the formed
/// surface so the axis fit reads a revolve, not a dome.
fn formed() -> Mesh {
    let s = |d: f64| CROWN - (CROWN * CROWN - d * d).sqrt();
    revolve(&[
        (0.0, -50.0),
        (BLANK, -50.0),
        (BLANK, 8.0),
        (FLOOR + s(2.0), 8.0),
        (FLOOR + s(1.0), 9.0),
        (FLOOR, 10.0),
        (FLOOR + s(1.0), 11.0),
        (FLOOR + s(2.0), 12.0),
        (BLANK, 12.0),
        (BLANK, 50.0),
        (0.0, 50.0),
    ])
}

/// The unformed bar the valley door must refuse.
fn wall() -> Mesh {
    revolve(&[(0.0, -50.0), (BLANK, -50.0), (BLANK, 50.0), (0.0, 50.0)])
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the form kind lives in: the formed bar AND a smooth
/// wall in bar-sized stock, the lathe mounted, the catalog crown
/// gauge (and the other lathe steel) on the rack.
fn form_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // Bar-sized stock (#103): the bar's own diameter over its
            // Z span - the turning kernels never cut stock beyond the
            // part (#130), so a wider blank is a real crash.
            camcraft_app::stock::StockShape::Cylinder { diameter: 20.0, height: 100.0 },
            Location { pos: Point3::new(0.0, 0.0, -50.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    add_tool(&world, &mut history, "FG-0600-04");
    add_tool(&world, &mut history, "GT-0300");
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", formed()));
    history.execute(&world, ImportModel::from_mesh("wall", wall()));
    (world, history)
}

fn form_params() -> OpParams {
    OpParams::FormTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(13.0),
        bottom_z: Height::fixed(7.0),
        feed: 40.0,
        plunge_feed: 20.0,
        picks: Default::default(),
    }
}

fn create_on(world: &World, history: &mut History, name: &str, tool: &str, src: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn create(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    create_on(world, history, name, tool, "bar", params);
}

#[test]
fn a_formed_op_runs_clean_in_the_document() {
    // The document's whole life: create, generate, and the crown
    // verdict comes back silent, so the op reads Ok with nothing
    // attached. The cached path from the world's side: one plunge,
    // straight in at the valley's own center, tip at the floor the
    // MODEL states - not a number anybody authored - and every
    // traverse above the blank.
    let (world, mut history) = form_world();
    create(&world, &mut history, "FM1", "FG-0600-04", form_params());
    let c = cache(&world, "FM1").expect("cached");
    assert_eq!(status(&world, "FM1"), OpStatus::Ok, "the crown verdict is silent");
    assert!(c.warnings.is_empty(), "a flush gauge reported: {:?}", c.warnings);
    let t = c.path;
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != camcraft_app::cl::MoveKind::Rapid).collect();
    assert_eq!(feeds.len(), 1, "one formed surface, one plunge");
    let (sa, ra) = (feeds[0].to.z, rho(feeds[0].to));
    assert!((sa - 10.0).abs() < 0.1, "the crown centers on its valley, not {sa}");
    assert!((ra - FLOOR).abs() < 0.05, "the tip reads the modeled floor, not {ra}");
    assert!(rho(feeds[0].from) > ra + 1.0, "in from the clearance stand, tip last");
    for m in t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Rapid) {
        assert!(rho(m.to) >= BLANK + 0.05 - 1e-9, "a low rapid: {:?}", m.to);
    }
    assert!(t.is_chained(), "the chain holds");
}

#[test]
fn a_form_kind_refuses_the_wrong_tool_by_name() {
    // The family door admits the KIND; the tool door is the
    // generator's, and the crown's arc IS the tool - so a
    // grooving blade (a straight edge, no crown at all) names
    // the op's tool and what that tool is.
    let (world, mut history) = form_world();
    common::create_past_the_tool_law(&world, &mut history, "FT1", "GT-0300", SrcRef::Entity("bar".into()), form_params());
    let Err(err) = common::generate_past_the_tool_law(&world, "FT1") else {
        panic!("a straight edge cannot stamp a crown");
    };
    let msg = err.to_string();
    assert!(msg.contains("needs a form gauge") && msg.contains("GT-0300"), "{msg}");
}

#[test]
fn a_smooth_bar_refuses_to_be_formed_by_name() {
    // The valley is the MODEL's: a wall has no formed surface to
    // promise, and the door says what form work needs - never a
    // crown invented from the parameters' air.
    let (world, mut history) = form_world();
    history.execute(
        &world,
        CreateOperation::new(&world, "FW1", "FG-0600-04", SrcRef::Entity("wall".into()), form_params())
            .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "FW1") else {
        panic!("a smooth bar has nothing to form");
    };
    let msg = err.to_string();
    assert!(msg.contains("nothing to form") && msg.contains("MODELED"), "{msg}");
}

#[test]
fn a_regrind_of_the_crown_convicts_both_digest_and_verdict() {
    // The killer of the mounted-tool law: the crown radius rides
    // the TOOL digest (the global nose_radius row), so re-grinding
    // the gauge to R 12 marks the op Stale on its own - and the
    // regenerated path, whose crown cannot match the R 6 valley,
    // convicts through the verdict channel, never a silent Ok.
    // Undoing the grind reads Stale again: the digest is a
    // question about NOW, never a remembered verdict.
    let (world, mut history) = form_world();
    create(&world, &mut history, "FM1", "FG-0600-04", form_params());
    assert_eq!(status(&world, "FM1"), OpStatus::Ok);
    let mut flatter = catalog_tool("FG-0600-04");
    flatter.nose_radius = 12.0; // re-ground, same name; still sane steel
    history.execute(&world, ChangeTool::new(&world, "FG-0600-04", flatter).expect("a sane gauge"));
    assert_eq!(status(&world, "FM1"), OpStatus::Stale, "a re-ground crown is a different cut");
    history.execute(&world, GenerateOperation::new(&world, "FM1").expect("regenerate"));
    assert_eq!(status(&world, "FM1"), OpStatus::Ok, "the verdict rides the cache, not the status");
    let warns = cache(&world, "FM1").expect("cached").warnings;
    assert!(
        warns.iter().any(|w| matches!(w, Warning::Envelope { rest_mm: r, .. } if *r > 0.0)),
        "a crown that stands off the valley judged silent: {warns:?}"
    );
    assert!(history.undo(&world), "undo the grind");
    assert_eq!(status(&world, "FM1"), OpStatus::Stale, "and the cached R 12 path reads stale now");
}

#[test]
fn a_narrower_band_still_rides_the_same_crown() {
    // The width half of the digest: the band is the gauge's, not
    // the op's - re-grind it narrower and the same crown stamps
    // the same floor, flush again, verdict silent both ways.
    let (world, mut history) = form_world();
    create(&world, &mut history, "FM1", "FG-0600-04", form_params());
    let mut narrow = catalog_tool("FG-0600-04");
    narrow.width = 3.0; // narrower band, same crown: is_sane holds
    history.execute(&world, ChangeTool::new(&world, "FG-0600-04", narrow).expect("a sane gauge"));
    assert_eq!(status(&world, "FM1"), OpStatus::Stale, "the cached plunge was the 4-wide band's");
    history.execute(&world, GenerateOperation::new(&world, "FM1").expect("regenerate"));
    assert_eq!(status(&world, "FM1"), OpStatus::Ok);
    assert!(cache(&world, "FM1").expect("cached").warnings.is_empty(), "the crown still fits");
}

#[test]
fn the_form_seed_stands() {
    // Registry end to end: pick the kind, accept the seed - and
    // the bar modeled to a crown-6, 4-wide valley takes the whole
    // form on those numbers, verdict silent.
    let (world, mut history) = form_world();
    let c = Carried {
        feed: 40.0,
        plunge: 20.0,
        climb: true,
        clearance: Height::fixed(24.0),
        stepover: 0.0,
        stepdown: 0.0,
        top: Height::fixed(13.0),
        bottom: Height::fixed(7.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let seed = (KindInfo::ALL[OpType::FormTurning as usize].seed)(&c);
    let OpParams::FormTurning { tolerance, feed, plunge_feed, .. } = &seed else {
        panic!("the seed must be form params");
    };
    assert_eq!((*tolerance, *feed, *plunge_feed), (0.05, 40.0, 20.0), "the seed states the gauge's numbers");
    create_on(&world, &mut history, "ST", "FG-0600-04", "bar", seed);
    let c = cache(&world, "ST").expect("cached");
    assert_eq!(status(&world, "ST"), OpStatus::Ok, "the seed's numbers stand");
    assert!(c.warnings.is_empty(), "the seeded form reported: {:?}", c.warnings);
}

#[test]
fn a_plunge_feed_edit_rides_the_history_both_ways() {
    // Everything-is-a-command: the parameter pane's edit is
    // EditOperationParams, the cache marks itself Stale against
    // the new number, and undo lands the old sink exactly.
    let (world, mut history) = form_world();
    create(&world, &mut history, "FM1", "FG-0600-04", form_params());
    let mut p = form_params();
    if let OpParams::FormTurning { plunge_feed, .. } = &mut p {
        *plunge_feed = 10.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "FM1", p).expect("a legal feed"));
    assert_eq!(status(&world, "FM1"), OpStatus::Stale, "the cached sink was the old feed's");
    assert!(history.undo(&world), "undo the edit");
    let e = model::find_by_name(&world, "FM1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::FormTurning { plunge_feed, .. } = &o.params else { unreachable!() };
    assert_eq!(*plunge_feed, 20.0, "the gauge's number is back");
    assert_eq!(status(&world, "FM1"), OpStatus::Ok, "and the cached path matches it again");
}

#[test]
fn an_upward_window_refuses_at_the_create_door() {
    // The blade trio's ordering law restated for the crown: top
    // below bottom is no window, and the door says it before any
    // geometry runs (AGENTS.md: validate early).
    let (world, _history) = form_world();
    let params = OpParams::FormTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(7.0),
        bottom_z: Height::fixed(13.0),
        feed: 40.0,
        plunge_feed: 20.0,
        picks: Default::default(),
    };
    let Err(err) =
        CreateOperation::new(&world, "FB1", "FG-0600-04", SrcRef::Entity("bar".into()), params)
    else {
        panic!("a window that cuts upward is no window");
    };
    let msg = err.to_string();
    assert!(msg.contains("must cut downward"), "{msg}");
}

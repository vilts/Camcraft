//! The threading infeed plan through the document (issue #416): a flank
//! or constant-area plan is an operation parameter like any other -
//! created, generated with the wave verdict silent, edited through the
//! history both ways (the op goes Stale against the new plan), saved
//! and loaded - and a plan the mounted insert cannot cut is refused by
//! name at the command door, never first at Generate. A default plan is
//! absent from the project file (no version step). The repeat of each
//! roughing depth (issue #418) rides the same doors: an edit undoes,
//! the key is written only when set. The kernel's own
//! laws are pinned one crate down (camcraft-motion's
//! turning_thread_infeed suite).
//!
//! Gate map (M4.4): covers camcraft_app::operation (the threading arm
//! of generate with the plan, the stamp's plan digest),
//! camcraft_app::commands (the threading gate on Create/Edit),
//! camcraft_app::project (the infeed_plan key).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const MAJOR: f64 = 6.0;
const PITCH: f64 = 2.0;
const DEPTH: f64 = 1.2;

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

/// Wall r 6, a 60-degree V every 2.0 from z 8 to z 14, 1.2 deep.
fn threaded() -> Mesh {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let root = MAJOR - DEPTH;
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        if c + PITCH <= 14.0 + 1e-9 {
            m.push((MAJOR, c + PITCH - half));
        }
        c += PITCH;
    }
    m.extend([(MAJOR, 14.0 + half), (MAJOR, 40.0), (0.0, 40.0)]);
    revolve(&m)
}

fn thread_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 56.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("insert");
    history.execute(&world, AddTool::new(&world, "TD-1732-60", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", threaded()));
    (world, history)
}

fn params(plan: ThreadInfeed) -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: plan,
        hand: camcraft_app::cl::ThreadHand::Right,
    }
}

fn flank(angle: f64) -> ThreadInfeed {
    ThreadInfeed { method: InfeedMethod::Flank, flank_angle: angle, ..ThreadInfeed::default() }
}

fn create(world: &World, history: &mut History, name: &str, plan: ThreadInfeed) {
    let cmd = CreateOperation::new(world, name, "TD-1732-60", SrcRef::Entity("bar".into()), params(plan));
    history.execute(world, cmd.expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("op");
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> camcraft_app::schema::Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("cached")
}

#[test]
fn a_flank_constant_area_plan_runs_clean_in_the_document() {
    let (world, mut history) = thread_world();
    let plan = ThreadInfeed {
        schedule: DepthSchedule::ConstantArea,
        min_depth: 0.1,
        finish_allowance: 0.05,
        spring_passes: 1,
        ..flank(27.5)
    };
    create(&world, &mut history, "TH1", plan);
    let c = cache(&world, "TH1");
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "{:?}", c.warnings);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    let t = c.path.threads.first().copied().expect("a flank plan carries its record");
    assert!((t.infeed_angle - 55.0).abs() < 1e-9 && t.spring_passes == 1, "{t:?}");
    assert!((t.min_depth - 0.1).abs() < 1e-9 && (t.first_depth - 0.4).abs() < 1e-9, "{t:?}");
}

#[test]
fn a_plan_the_insert_cannot_cut_is_refused_at_the_door_by_name() {
    let (world, mut history) = thread_world();
    let bad: [(ThreadInfeed, &str); 4] = [
        (flank(35.0), "half-angle"),
        (
            ThreadInfeed { schedule: DepthSchedule::ConstantArea, min_depth: 0.6, ..ThreadInfeed::default() },
            "first-pass depth",
        ),
        (ThreadInfeed { finish_allowance: DEPTH, ..ThreadInfeed::default() }, "finish allowance"),
        (ThreadInfeed { spring_passes: 120, ..ThreadInfeed::default() }, "spring passes"),
    ];
    for (plan, why) in bad {
        let Err(err) = CreateOperation::new(&world, "TX", "TD-1732-60", SrcRef::Entity("bar".into()), params(plan)) else {
            panic!("{why}: created");
        };
        assert!(err.to_string().contains(why), "{why}: {err}");
    }
    // The same door on an edit of a living op.
    create(&world, &mut history, "TH1", ThreadInfeed::default());
    let Err(err) = EditOperationParams::new(&world, "TH1", params(flank(31.0))) else {
        panic!("a 31-degree flank on a 60-degree insert was accepted");
    };
    assert!(err.to_string().contains("half-angle"), "{err}");
}

#[test]
fn a_plan_edit_rides_the_history_both_ways() {
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", ThreadInfeed::default());
    let before = cache(&world, "TH1").path;
    let cmd = EditOperationParams::new(&world, "TH1", params(flank(29.5))).expect("a legal plan");
    history.execute(&world, cmd);
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "the cached passes are the radial plan's");
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("regenerate"));
    assert_eq!(status(&world, "TH1"), OpStatus::Ok);
    assert_ne!(cache(&world, "TH1").path.moves, before.moves, "the flank plan moved the passes");
    assert!(history.undo(&world) && history.undo(&world), "undo generate and edit");
    let e = model::find_by_name(&world, "TH1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Threading { infeed_plan, .. } = &o.params else { unreachable!() };
    assert_eq!(*infeed_plan, ThreadInfeed::default(), "the radial plan is back");
    assert!(history.redo(&world));
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Threading { infeed_plan, .. } = &o.params else { unreachable!() };
    assert_eq!(*infeed_plan, flank(29.5));
}

#[test]
fn the_plan_rides_the_project_file_and_a_default_one_is_absent() {
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH0", ThreadInfeed::default());
    let dir = std::env::temp_dir();
    let a = dir.join(format!("camcraft-infeed-a-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &a).expect("saved");
    let text = std::fs::read_to_string(&a).expect("read");
    let _ = std::fs::remove_file(&a);
    assert!(!text.contains("infeed_plan"), "a default plan writes no key");
    let plan = ThreadInfeed {
        method: InfeedMethod::Alternating,
        flank_angle: 29.0,
        schedule: DepthSchedule::ConstantArea,
        min_depth: 0.08,
        finish_allowance: 0.03,
        spring_passes: 2,
        repeat_each_depth: true,
        dips: Default::default(),
    };
    create(&world, &mut history, "TH1", plan);
    // Fresh before the save, so a Stale after the load can only be the
    // round trip (NeoTech's 2026-11-13 refresh saw one; not reproduced).
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "TH1: fresh before the save");
    let b = dir.join(format!("camcraft-infeed-b-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &b).expect("saved");
    let text = std::fs::read_to_string(&b).expect("read");
    assert_eq!(text.matches("repeat_each_depth").count(), 2, "the plan and its record carry the flag, nothing else");
    let other = new_world();
    camcraft_app::project::load(&other, &b).expect("loaded");
    let _ = std::fs::remove_file(&b);
    for (name, want) in [("TH0", ThreadInfeed::default()), ("TH1", plan)] {
        let e = model::find_by_name(&other, name).expect("op");
        let o = model::operation_of(&other, &e).expect("operation");
        let OpParams::Threading { infeed_plan, .. } = &o.params else { unreachable!() };
        assert_eq!(*infeed_plan, want, "{name}");
        let saved = model::find_by_name(&world, name).and_then(|e| model::operation_of(&world, &e)).expect("saved op");
        assert_eq!(o.stamp, saved.stamp, "{name}: the stored stamp survives the round trip");
        // TH0's contribution to TH1's program-ahead digest: equal before
        // the save and after the load. It moves only when TH0's cached
        // path lands off the 1 um lattice the save snaps to (a main
        // defect, reported apart from this PR; the radial ladder here
        // lands on it on macOS, not on every platform's libm).
        assert_eq!(
            camcraft_app::rest::prior_digest(&other, name, o.setup.as_deref()),
            camcraft_app::rest::prior_digest(&world, name, saved.setup.as_deref()),
            "{name}: the program ahead digests as before the save"
        );
        let now = camcraft_app::operation::op_stamp(&other, &o, name);
        assert_eq!(now, camcraft_app::operation::op_stamp(&world, &saved, name), "{name}: the loaded inputs digest as before");
        assert_eq!(status(&other, name), OpStatus::Ok, "{name}: the loaded cache matches its plan");
    }
}

fn plan_of(world: &World, name: &str) -> ThreadInfeed {
    let e = model::find_by_name(world, name).expect("op");
    let o = model::operation_of(world, &e).expect("operation");
    let OpParams::Threading { infeed_plan, .. } = &o.params else { unreachable!() };
    *infeed_plan
}

fn plunge_count(world: &World, name: &str) -> usize {
    let path = cache(world, name).path;
    path.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed && m.arc.is_none()).count()
}

#[test]
fn the_repeat_toggle_rides_the_history_both_ways() {
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", ThreadInfeed::default());
    assert_eq!(plunge_count(&world, "TH1"), 3, "0.4, 0.8, 1.2");
    let on = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    history.execute(&world, EditOperationParams::new(&world, "TH1", params(on)).expect("a legal plan"));
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "the cached passes cut each depth once");
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("regenerate"));
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "{:?}", cache(&world, "TH1").warnings);
    assert!(cache(&world, "TH1").warnings.is_empty());
    assert_eq!(plunge_count(&world, "TH1"), 5, "0.4 twice, 0.8 twice, the root once");
    assert!(history.undo(&world) && history.undo(&world), "undo generate and edit");
    assert_eq!(plan_of(&world, "TH1"), ThreadInfeed::default(), "the repeat is off again");
    assert!(history.redo(&world));
    assert_eq!(plan_of(&world, "TH1"), on);
}

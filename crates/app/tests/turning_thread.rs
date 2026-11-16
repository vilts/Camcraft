//! M5.2 stage D threading tests at the app level: the wiring - a
//! threading op's whole life through the document. What "right" means
//! here: a V-form insert on a modeled thread, on a mounted lathe,
//! generates and reads Ok with the wave verdict (no sphere speaks it)
//! silent; the cached path carries the kernel's signature seen from
//! the world (three passes of plunge plus six half-turns, the last
//! rung at the root, rapids above the crests); the wrong tool on a
//! threading kind is refused by name, and so is a smooth bar - the
//! cut needs a MODELED thread to find its phase; a thread that drifts
//! against its own model convicts through the verdict channel, never
//! a silent Ok; a re-grind of the insert's V marks the op Stale
//! through the TOOL digest and reads fresh again after regenerate,
//! both ways in undo. The kernel's own laws are pinned one crate
//! down (camcraft-motion's turning_thread suite); this file is the
//! document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the threading arm
//! of generate, the wave verdict wiring, the stamp's kind-scoped
//! width+tip digest), camcraft_app::commands (Create/
//! GenerateOperation, ChangeTool, EditOperationParams undo),
//! camcraft_app::path_sig (the lattice-form signature, #1351).
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
const MAJOR: f64 = 6.0;
const PITCH: f64 = 2.0;
const DEPTH: f64 = 1.2;

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

/// The threaded bar, app-sized: the motion suite's own fixture -
/// wall r 6, a 60-degree V every 2.0 from z 8 to z 14 (valleys at
/// 8, 10, 12, 14), capped, and long past the thread so the axis fit
/// reads a revolve, not a dome. `depth` is the modeled thread's;
/// the seed test's bar carries the seed's number.
fn threaded(depth: f64) -> Mesh {
    let half = depth * (30.0_f64).to_radians().tan();
    let root = MAJOR - depth;
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
    m.push((MAJOR, 14.0 + half));
    m.push((MAJOR, 40.0));
    m.push((0.0, 40.0));
    revolve(&m)
}

/// The threadless bar the phase door must refuse.
fn wall() -> Mesh {
    revolve(&[(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 40.0), (0.0, 40.0)])
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the threading kind lives in: the threaded bar AND a
/// smooth wall in bar-sized stock, the lathe mounted, the catalog
/// threading insert (and the other lathe steel) on the rack.
fn thread_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // Bar-sized stock (#103): the bar's own diameter over its
            // Z span - the turning kernels never cut stock beyond the
            // part (#130), so a wider blank is a real crash.
            camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 56.0 },
            Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    add_tool(&world, &mut history, "TD-1732-60");
    add_tool(&world, &mut history, "GT-0300");
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", threaded(DEPTH)));
    history.execute(&world, ImportModel::from_mesh("wall", wall()));
    (world, history)
}

fn thread_params() -> OpParams {
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
        infeed_plan: Default::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
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
fn a_threaded_op_runs_clean_in_the_document() {
    // The document's whole life: create, generate, and the wave
    // verdict - the one no sphere can speak - comes back silent, so
    // the op reads Ok with nothing attached. The cached path from
    // the world's side: three passes of the 1.2-deep ladder at 0.4
    // infeed, each a plunge plus six half-turns; the last feed
    // stands at the root radius on the run's last center; every
    // traverse rides above the crests.
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", "TD-1732-60", thread_params());
    let c = cache(&world, "TH1").expect("cached");
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "the wave verdict is silent");
    assert!(c.warnings.is_empty(), "a flush thread reported: {:?}", c.warnings);
    let t = c.path;
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != camcraft_app::cl::MoveKind::Rapid).collect();
    assert_eq!(feeds.len(), 21, "three passes of plunge + six arcs: {}", feeds.len());
    assert_eq!(t.arc_count(), 18, "six half-turns per pass");
    assert!(feeds[0].arc.is_none(), "a pass opens with the radial plunge");
    assert!((rho(feeds[0].to) - (MAJOR - 0.4)).abs() < 1e-9, "the first rung is the first infeed");
    let last = feeds[feeds.len() - 1].to;
    assert!((rho(last) - (MAJOR - DEPTH)).abs() < 1e-9, "the last rung is the root: {:?}", last);
    // #629: toward the chuck - the run ends on the BOTTOM center.
    assert!((last.z - 8.0).abs() < 1e-6, "the run ends on the last center: {:?}", last);
    for m in t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Rapid) {
        assert!(rho(m.to) >= MAJOR + 0.05 - 1e-9 || m.to.z > 23.9, "a low rapid: {:?}", m.to);
    }
    assert!(t.is_chained(), "arcs and all, the chain holds");
}

#[test]
fn a_threading_kind_refuses_the_wrong_tool_by_name() {
    // The family door admits the KIND; the tool door is the
    // generator's, and the wave's profile IS the tool - so a
    // grooving blade (no flanks at all) names the op's tool and
    // what that tool is.
    let (world, mut history) = thread_world();
    common::create_past_the_tool_law(&world, &mut history, "TT1", "GT-0300", SrcRef::Entity("bar".into()), thread_params());
    let Err(err) = common::generate_past_the_tool_law(&world, "TT1") else {
        panic!("a rectangular kerf cannot cut a V");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("needs a threading insert") && msg.contains("GT-0300") && msg.contains("grooving blade"),
        "{msg}"
    );
}

#[test]
fn a_smooth_bar_refuses_to_be_threaded_by_name() {
    // The phase is the MODEL's: a wall with no valley train has no
    // phase to state, and the door says what threading needs -
    // never a lattice invented from the parameters' air.
    let (world, mut history) = thread_world();
    history.execute(
        &world,
        CreateOperation::new(&world, "TW1", "TD-1732-60", SrcRef::Entity("wall".into()), thread_params())
            .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "TW1") else {
        panic!("a smooth bar has no thread to find");
    };
    let msg = err.to_string();
    assert!(msg.contains("nothing to thread") && msg.contains("MODELED"), "{msg}");
}

#[test]
fn a_pitch_that_drifts_convicts_through_the_verdict() {
    // The generation door fit the numbers and passed - the lattice
    // reads its phase from the model's first valley - but the
    // turns walk off the valley train down the window, and the
    // wave verdict must say so: warnings ride the cache, and a
    // judge that cannot stay silent never dresses as Ok-without-
    // comment.
    let (world, mut history) = thread_world();
    let mut p = thread_params();
    if let OpParams::Threading { pitch, .. } = &mut p {
        *pitch = PITCH * 1.1;
    }
    create(&world, &mut history, "TD1", "TD-1732-60", p);
    assert_eq!(status(&world, "TD1"), OpStatus::Ok, "the stamp matches its inputs");
    let warns = cache(&world, "TD1").expect("cached").warnings;
    assert!(
        warns.iter().any(|w| matches!(w, Warning::Envelope { .. })),
        "a 10% pitch drift judged silent: {warns:?}"
    );
}

#[test]
fn the_threading_seed_stands() {
    // Registry end to end: pick the kind, accept the seed - and
    // the bar modeled to the seed's own depth takes the whole
    // thread on those numbers, verdict silent.
    let (world, mut history) = thread_world();
    history.execute(&world, ImportModel::from_mesh("bar8", threaded(1.08)));
    let c = Carried {
        feed: 120.0,
        plunge: 40.0,
        climb: true,
        clearance: Height::fixed(24.0),
        stepover: 0.0,
        stepdown: 0.0,
        top: Height::fixed(14.3),
        bottom: Height::fixed(7.7),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let seed = (KindInfo::ALL[OpType::Threading as usize].seed)(&c);
    let OpParams::Threading { pitch, depth, infeed, .. } = &seed else {
        panic!("the seed must be threading params");
    };
    assert_eq!((*pitch, *depth, *infeed), (2.0, 1.08, 0.4), "the seed states the wave's numbers");
    create_on(&world, &mut history, "ST", "TD-1732-60", "bar8", seed);
    let c = cache(&world, "ST").expect("cached");
    assert_eq!(status(&world, "ST"), OpStatus::Ok, "the seed's numbers stand");
    assert!(c.warnings.is_empty(), "the seeded thread reported: {:?}", c.warnings);
}

#[test]
fn a_regrind_of_the_form_marks_the_op_stale_both_ways() {
    // The insert's width and form ride the TOOL digest (M5.2 D):
    // re-grind the V in the tool list and the op cannot know its
    // cached wave from a different steel - regenerate (the same
    // 60-degree form still coincides, so the verdict keeps its
    // silence), and undoing the grind reads Stale again (the
    // digest is a question about NOW, never a remembered verdict).
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", "TD-1732-60", thread_params());
    assert_eq!(status(&world, "TH1"), OpStatus::Ok);
    let mut narrow = catalog_tool("TD-1732-60");
    narrow.width = 1.5; // re-ground, same name; still fits, still reaches
    history.execute(&world, ChangeTool::new(&world, "TD-1732-60", narrow).expect("a sane insert"));
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "a re-ground V is a different cut");
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("regenerate"));
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "the narrower steel still rides the same flanks");
    assert!(history.undo(&world), "undo the grind");
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "and the cached 1.5 path reads stale now");
}

#[test]
fn a_pitch_edit_rides_the_history_both_ways() {
    // Everything-is-a-command: the parameter pane's edit is
    // EditOperationParams, the cache marks itself Stale against
    // the new number, redo regenerates through the drift verdict
    // - and undo lands the old lattice exactly.
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", "TD-1732-60", thread_params());
    let mut p = thread_params();
    if let OpParams::Threading { pitch, .. } = &mut p {
        *pitch = PITCH * 1.1;
    }
    history.execute(&world, EditOperationParams::new(&world, "TH1", p).expect("a legal pitch"));
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "the cached lattice is the old pitch's");
    assert!(history.undo(&world), "undo the edit");
    let e = model::find_by_name(&world, "TH1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Threading { pitch, .. } = &o.params else { unreachable!() };
    assert_eq!(*pitch, PITCH, "the lattice's number is back");
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "and the cached path matches it again");
}

#[test]
fn the_thread_hand_edit_rides_the_history_both_ways() {
    // #629: the hand is a parameter like the pitch - EditOperationParams
    // writes it, the cache goes Stale, the regenerated helix winds the
    // other way (counter-clockwise falling: left-hand), and undo / redo
    // walk it back and forth.
    use camcraft_app::cl::ThreadHand;
    let hand_of = |world: &World| {
        let e = model::find_by_name(world, "TH1").expect("op");
        match model::operation_of(world, &e).expect("operation").params {
            OpParams::Threading { hand, .. } => hand,
            other => panic!("threading expected, got {other:?}"),
        }
    };
    let turns = |world: &World| -> Vec<Option<ThreadHand>> {
        let c = cache(world, "TH1").expect("cached");
        c.path.moves.iter().filter(|m| m.arc.is_some()).map(ThreadHand::of_move).collect()
    };
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH1", "TD-1732-60", thread_params());
    assert_eq!(hand_of(&world), ThreadHand::Right, "right-hand unless the drawing says LH");
    assert!(turns(&world).iter().all(|h| *h == Some(ThreadHand::Right)), "the seeded helix is right-hand");
    let mut p = thread_params();
    if let OpParams::Threading { hand, .. } = &mut p {
        *hand = ThreadHand::Left;
    }
    history.execute(&world, EditOperationParams::new(&world, "TH1", p).expect("a legal hand"));
    assert_eq!(hand_of(&world), ThreadHand::Left);
    assert_eq!(status(&world, "TH1"), OpStatus::Stale, "the cached helix winds the old way");
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("regenerate"));
    assert_eq!(status(&world, "TH1"), OpStatus::Ok, "the hand verdict is silent on its own helix");
    assert!(turns(&world).iter().all(|h| *h == Some(ThreadHand::Left)), "regenerated left-hand");
    assert!(history.undo(&world), "undo the regenerate");
    assert!(history.undo(&world), "undo the edit");
    assert_eq!(hand_of(&world), ThreadHand::Right, "the hand is back");
    assert_eq!(status(&world, "TH1"), OpStatus::Ok);
    assert!(turns(&world).iter().all(|h| *h == Some(ThreadHand::Right)));
    assert!(history.redo(&world), "redo the edit");
    assert_eq!(hand_of(&world), ThreadHand::Left);
}

#[test]
fn an_upward_window_refuses_at_the_create_door() {
    // The blade trio's ordering law restated for the wave: top
    // below bottom is no window of turns, and the door says it
    // before any geometry runs (AGENTS.md: validate early).
    let (world, _history) = thread_world();
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(7.7),
        bottom_z: Height::fixed(14.3),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
    };
    let Err(err) =
        CreateOperation::new(&world, "TB1", "TD-1732-60", SrcRef::Entity("bar".into()), params)
    else {
        panic!("a window that cuts upward is no window");
    };
    let msg = err.to_string();
    assert!(msg.contains("must cut downward"), "{msg}");
}

#[test]
fn a_chain_of_threading_ops_stays_ok_through_save_and_load() {
    // #1351: TH1's rest digest reads TH0's path signature. The file
    // stores moves on the 1 um lattice (TH0 generates y =
    // 6.050000000000001, the file holds 6.05), so the signature must
    // be the stored form's - else the reopened chain reads TH1 Stale.
    let (world, mut history) = thread_world();
    create(&world, &mut history, "TH0", "TD-1732-60", thread_params());
    create(&world, &mut history, "TH1", "TD-1732-60", thread_params());
    assert_eq!(status(&world, "TH0"), OpStatus::Ok);
    assert_eq!(status(&world, "TH1"), OpStatus::Ok);
    let file = std::env::temp_dir().join(format!("cc-1351-chain-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("saved");
    let other = new_world();
    let loaded = camcraft_app::project::load(&other, &file);
    let _ = std::fs::remove_file(&file);
    loaded.expect("loaded");
    for n in ["TH0", "TH1"] {
        assert_eq!(status(&other, n), OpStatus::Ok, "{n} after load");
        let (a, b) = (cache(&world, n).expect("generated"), cache(&other, n).expect("loaded"));
        assert_eq!(a.path_sig(), b.path_sig(), "{n}: the stored form signs as the generated one");
    }
    // The snap hides nothing a file could tell apart: a micron moves it.
    let mut moved = cache(&world, "TH0").expect("generated").path;
    let first = moved.moves.first_mut().expect("a move");
    first.to.y += 1e-3;
    let moved = camcraft_app::schema::Toolpath::new(moved, Vec::new());
    assert_ne!(moved.path_sig(), cache(&world, "TH0").expect("generated").path_sig(), "a micron moves the sign");
}

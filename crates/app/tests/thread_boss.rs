//! External threads on a boss, end to end (#985): a Thread operation set
//! to external reads a modelled boss, mills its thread after the stock
//! around it is machined away, verifies clean against the part and
//! posts helical G2/G3 with Z; a boss too thin, a thread past the
//! mill's reach, a boss standing in uncut stock and a wall inside the
//! crest ring's sweep are refused or convicted by name.
//!
//! Provenance: ISO 965-1 / DIN 13 external thread depth (0.6134 x
//! pitch: M8 x 1.25 root Ø6.466, M10 x 1.5 root Ø8.160) and public
//! handbook thread milling; our own fixtures.
//!
//! Gate map (M4.4): covers app::operation (thread_boss, thread_entry,
//! thread_op, material::detect_holes), camcraft_motion::drilling (recognize_bosses),
//! camcraft_motion::machining2d (thread_mill), app::commands (operation gates),
//! app::posting, app::project (params DTO).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod thread_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ThreadHand, Toolpath};
use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel};
use camcraft_app::drilling::Hole;
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::world::new_world;

use thread_support::boss::*;
use thread_support::{MULTI, SINGLE, assert_clean, blocking};

/// Create "Thread1" and generate it: the refusal's words, or empty.
fn try_thread(world: &World, history: &mut History, tool: &str, params: OpParams) -> String {
    let op = CreateOperation::new(world, "Thread1", tool, SrcRef::Entity("stud".into()), params);
    history.execute(world, op.expect("create the thread"));
    match GenerateOperation::new(world, "Thread1") {
        Ok(cmd) => {
            history.execute(world, cmd);
            String::new()
        }
        Err(e) => e.to_string(),
    }
}

fn path_of(world: &World) -> Toolpath {
    let e = model::find_by_name(world, "Thread1").expect("op");
    model::toolpath_of(world, &e).expect("cached path").path
}

/// The document's program through the Fanuc post on the built-in VMC.
fn fanuc(world: &World) -> String {
    let records = posting::stream_records(world).expect("the generated program dumps to records");
    let machine = serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).expect("built-in VMC");
    let out = engine::run(include_str!("../../../assets/posts/fanuc.cps"), &machine, &cnc::serialize(&records))
        .expect("the thread posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// The helix's half-turns: XY arcs about the boss's axis.
fn helix(path: &Toolpath) -> Vec<camcraft_app::cl::Move> {
    path.moves.iter().filter(|m| m.arc.is_some_and(|a| a.center.x.abs() < 1e-9 && a.center.y.abs() < 1e-9)).copied().collect()
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

fn thread_params(world: &World) -> OpParams {
    let e = model::find_by_name(world, "Thread1").expect("op");
    model::operation_of(world, &e).expect("operation").params
}

#[test]
fn an_external_m8_thread_verifies_clean_and_posts_helical_moves() {
    // M8 x 1.25 on a Ø8 boss, 10 mm long, with the catalog's Ø6 multi-form mill (ten rows): one helical
    // revolution per radial pass. Right-hand climb orbits a boss clockwise, winding down from the top.
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    assert_eq!(try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true)), "");
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
    assert_clean(&world);
    let path = path_of(&world);
    let turns = helix(&path);
    // Thread depth 0.6134 x 1.25 = 0.767 mm at 0.3 per pass: three passes, two half-turns each.
    assert_eq!(turns.len(), 6);
    assert!(turns.iter().all(|m| ThreadHand::of_move(m) == Some(ThreadHand::Right) && m.to.z < m.from.z));
    assert!(turns.iter().all(|m| m.arc.is_some_and(|a| !a.ccw)), "climb orbits a boss clockwise");
    let reach = turns.iter().map(|m| m.to.x.hypot(m.to.y)).fold(f64::INFINITY, f64::min);
    assert!((reach - 3.0 - (4.0 - 0.6134 * 1.25)).abs() < 2e-6, "the crests land on the design root circle: {reach}");
    assert!(path.moves.iter().all(|m| m.to.z >= SHOULDER - 1e-9), "the tip keeps the shoulder");
    // No move cuts below the boss's top by a rapid: the way down beside the boss is fed.
    assert!(path.moves.iter().all(|m| m.kind != camcraft_app::cl::MoveKind::Rapid || m.to.z >= TOP - 1e-9));
    common::write_golden("thread_mill_external_m8_multi_form.cl", &common::gen_to_cltext(&path));
    // The post: clockwise helical arcs are G02 blocks that carry a Z word.
    let nc = fanuc(&world);
    let helical = nc.lines().filter(|l| l.contains("G02") && l.contains('Z') && l.contains('I')).count();
    assert!(helical >= 6, "every half-turn posts as a G02 with Z:\n{nc}");
    assert!(!nc.contains("G84") && !nc.contains("G33"), "thread milling is plain interpolation");
    common::write_golden("nc/thread_mill_external_m8_multi_form.nc", &nc);
}

#[test]
fn a_single_form_mill_threads_an_m10_boss_one_turn_per_thread() {
    // M10 x 1.5 on a Ø10 boss with the catalog's single-form mill (0.5 to 1.5 mm): the pitch is stated.
    let (world, mut history) = stud_world(stud(5.0, Vec::new()));
    assert_eq!(try_thread(&world, &mut history, SINGLE, external(vec![boss(5.0)], Some(1.5), ThreadHand::Right, true)), "");
    assert_clean(&world);
    let path = path_of(&world);
    let turns = helix(&path);
    // Depth 0.6134 x 1.5 = 0.920 mm at 0.3 per pass: four passes; 10 mm less the lead: seven turns each.
    assert_eq!(turns.len(), 4 * 2 * 7, "{}", turns.len());
    assert!(turns.iter().all(|m| ((m.from.z - m.to.z) - 0.75).abs() < 1e-9), "half a pitch per half-turn, falling");
    let reach = turns.iter().map(|m| m.to.x.hypot(m.to.y)).fold(f64::INFINITY, f64::min);
    assert!((reach - 3.0 - (5.0 - 0.6134 * 1.5)).abs() < 2e-6, "{reach}");
    common::write_golden("thread_mill_external_m10_single_form.cl", &common::gen_to_cltext(&path));
}

#[test]
fn left_hand_and_conventional_external_threads_verify_clean_too() {
    for (hand, climb) in [(ThreadHand::Left, true), (ThreadHand::Left, false), (ThreadHand::Right, false)] {
        let (world, mut history) = stud_world(stud(5.0, Vec::new()));
        assert_eq!(try_thread(&world, &mut history, SINGLE, external(vec![boss(5.0)], Some(1.5), hand, climb)), "");
        assert_clean(&world);
        for m in helix(&path_of(&world)) {
            assert_eq!(ThreadHand::of_move(&m), Some(hand));
            assert_eq!(m.arc.map(|a| a.ccw), Some(!climb), "climb orbits a boss clockwise");
        }
        let nc = fanuc(&world);
        let word = if climb { "G02" } else { "G03" };
        assert!(nc.lines().any(|l| l.contains(word) && l.contains('Z')), "{hand:?} {climb}:\n{nc}");
    }
}

#[test]
fn detect_reads_the_bosses_of_an_external_thread_and_holes_of_an_internal_one() {
    let (world, _history) = stud_bare(stud(4.0, Vec::new()));
    let src = SrcRef::Entity("stud".into());
    let found = operation::detect_holes(&world, "Thread1", &external(Vec::new(), None, ThreadHand::Right, true), None, &src);
    let OpParams::Thread { holes, external: true, .. } = found.expect("the stud's bosses").params else {
        panic!("an external thread comes back external");
    };
    // The Ø8 boss, then the round base it stands on (a round wall is a boss too; the list is the author's to trim).
    assert_eq!(holes.len(), 2, "{holes:?}");
    assert!((holes[0].radius - 4.0).abs() < 1e-6 && holes[0].xy.x.abs() < 1e-6 && holes[0].xy.y.abs() < 1e-6, "{holes:?}");
    assert_eq!((holes[0].entry_z, holes[0].floor_z, holes[0].through, holes[0].stepped), (TOP, SHOULDER, false, false));
    assert!((holes[1].radius - 18.0).abs() < 1e-6 && holes[1].entry_z == SHOULDER, "{holes:?}");
    // The same model has no hole: an internal thread's detect says so, by name.
    let mut internal = external(Vec::new(), None, ThreadHand::Right, true);
    if let OpParams::Thread { external, .. } = &mut internal {
        *external = false;
    }
    let msg = operation::detect_holes(&world, "Thread1", &internal, None, &src).err().unwrap_or_default();
    assert!(msg.contains("no Z-axis holes"), "{msg}");
    // A model with no round upright wall has no boss.
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("block", camcraft_app::geometry::cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let block = SrcRef::Entity("block".into());
    let msg = operation::detect_holes(&world, "T", &external(Vec::new(), None, ThreadHand::Right, true), None, &block).err().unwrap_or_default();
    assert!(msg.contains("no upright round bosses"), "{msg}");
}

#[test]
fn a_boss_standing_in_uncut_stock_is_refused_by_name() {
    // Nothing ahead machined round the boss: the block is solid beside it.
    let (world, mut history) = stud_bare(stud(4.0, Vec::new()));
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("the boss at (0.000, 0.000) is not cleared: nothing ahead of the thread"), "{msg}");
    assert_eq!(status(&world, "Thread1"), OpStatus::Never, "no path is cached for a refused thread");
    assert!(!posting::verification_gate(&world).is_empty(), "nothing posts");
    // Roughed, but two millimetres short of the shoulder: the thread runs deeper than the air does.
    let (world, mut history) = stud_bare(stud(4.0, Vec::new()));
    clear_around(&world, &mut history, SHOULDER + 2.0, 0.0);
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("is not cleared") && msg.contains("stands at z 12.000") && msg.contains("down to z 10.000"), "{msg}");
    // Roughed with a millimetre left on the boss: more than a thread mill may take.
    let (world, mut history) = stud_bare(stud(4.0, Vec::new()));
    clear_around(&world, &mut history, SHOULDER, 1.0);
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("the boss at (0.000, 0.000) is not cleared") && msg.contains("stands at z 20.000"), "{msg}");
}

#[test]
fn a_hex_stud_is_no_round_boss_and_is_refused_by_name() {
    // A hex stud, corners on the M8's Ø8: its flats stand 0.54 mm inside the
    // drawn circle, where the crest law (the minor circle) and the
    // neighbour law (which passes the boss's own wall) would let the teeth
    // cut them unjudged. Refused, never threaded.
    let (world, mut history) = stud_world(stud_of(6, 4.0, Vec::new()));
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("the boss at (0.000, 0.000) is not round - external thread milling needs a round boss"), "{msg}");
    assert_eq!(status(&world, "Thread1"), OpStatus::Never, "no path is cached for a refused thread");
    assert!(!posting::verification_gate(&world).is_empty(), "nothing posts");
    // The same boss drawn round (96 flats) threads clean.
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    assert_eq!(try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true)), "");
}

#[test]
fn moving_the_roughing_after_the_thread_marks_it_stale_and_refuses_it() {
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    assert_eq!(try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true)), "");
    let reorder = camcraft_app::commands::ReorderOperation::new(&world, "Rough1", "Thread1", true).expect("reorder");
    history.execute(&world, reorder);
    assert_eq!(status(&world, "Thread1"), OpStatus::Stale, "the program ahead of the thread changed");
    let msg = GenerateOperation::new(&world, "Thread1").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("is not cleared"), "{msg}");
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
}

#[test]
fn impossible_bosses_are_refused_when_the_operation_is_made() {
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    let src = || SrcRef::Entity("stud".into());
    let refusal = |tool: &str, params: OpParams| match CreateOperation::new(&world, "Thread1", tool, src(), params) {
        Ok(_) => panic!("the door was meant to refuse"),
        Err(e) => e.to_string(),
    };
    // A Ø1.4 pin under a 0.767 mm deep thread has no core.
    let msg = refusal(MULTI, external(vec![boss(0.7)], None, ThreadHand::Right, true));
    assert!(msg.contains("the 1.4 mm boss at (0, 0) is thinner than the 0.76675 mm thread is deep"), "{msg}");
    // 15 mm of thread under the multi-form mill's 12.5 mm of teeth: its shank would meet the boss.
    let tall = Hole { entry_z: SHOULDER + 15.0, ..boss(4.0) };
    let msg = refusal(MULTI, external(vec![tall], None, ThreadHand::Right, true));
    assert!(msg.contains("is 15 mm long") && msg.contains("reaches only 12.5 mm"), "{msg}");
    // The infeed is judged against the EXTERNAL depth: 0.75 passes on a boss, not in a hole (0.677).
    let mut deep = external(vec![boss(4.0)], None, ThreadHand::Right, true);
    if let OpParams::Thread { infeed, .. } = &mut deep {
        *infeed = 0.75;
    }
    assert!(CreateOperation::new(&world, "Deep", MULTI, src(), deep.clone()).is_ok());
    if let OpParams::Thread { infeed, .. } = &mut deep {
        *infeed = 0.8;
    }
    assert!(refusal(MULTI, deep).contains("more than the"));
    // A stated bottom is resolved at generate, which asks the reach again.
    let mut low = external(vec![Hole { entry_z: SHOULDER + 15.0, ..boss(4.0) }], None, ThreadHand::Right, true);
    if let OpParams::Thread { bottom_z, top_z, .. } = &mut low {
        *bottom_z = Some(Height::fixed(SHOULDER + 1.0));
        // #1212: the R plane on the taller boss's top, so the door's R law stands aside for the reach.
        *top_z = Height::fixed(SHOULDER + 15.0);
    }
    let msg = try_thread(&world, &mut history, MULTI, low);
    assert!(msg.contains("is 14 mm long") && msg.contains("reaches only 12.5 mm"), "{msg}");
}

#[test]
fn a_wall_inside_the_crest_rings_sweep_is_a_gouge_that_blocks_the_post() {
    // A rib 9.6 mm from the axis: the first pass's crests reach 9.74 - 0.14 mm into it, under the grid's
    // cell and far under the thread depth the root law grants at a wall. The roughing here follows a
    // model without the rib (it mills the rib away), so the air law has nothing to say and the neighbour
    // law is what convicts the thread; the same rib 0.2 mm further out is air.
    for (x1, gouged) in [(-9.6, true), (-9.8, false)] {
        let (world, mut history) = stud_bare(stud(4.0, rib(-12.0, x1, 2.0, 16.0)));
        history.execute(&world, ImportModel::from_mesh("core", stud(4.0, Vec::new())));
        let op = CreateOperation::new(&world, "Rough1", ENDMILL, SrcRef::Entity("core".into()), clearing(SHOULDER, 0.0));
        history.execute(&world, op.expect("create the roughing"));
        history.execute(&world, GenerateOperation::new(&world, "Rough1").expect("generate the roughing"));
        assert_eq!(try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true)), "");
        let warns = blocking(&world);
        assert_eq!(!warns.is_empty(), gouged, "rib at {x1}: {warns:?}");
        if gouged {
            assert_eq!(warns, ["move 4 cuts past the finish surface", "move 5 cuts past the finish surface"], "the first pass only");
            let out = std::env::temp_dir().join(format!("camcraft_985_rib_{}.nc", std::process::id()));
            let refused = posting::post_setup(&world, &out).err().unwrap_or_default();
            assert!(refused.contains("post blocked"), "{refused}");
            assert!(!out.exists(), "a refused post writes nothing");
        }
    }
}

#[test]
fn a_part_wall_the_roughing_left_standing_in_the_sweep_is_refused() {
    // The same rib under a roughing that respects it: the contour stops above the rib, so stock stands
    // where the mill would feed down - the air law refuses the thread before a move is cached.
    let (world, mut history) = stud_world(stud(4.0, rib(-12.0, -9.0, 2.0, 16.0)));
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(4.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("the boss at (0.000, 0.000) is not cleared") && msg.contains("stands at z 16.000"), "{msg}");
}

#[test]
fn a_hole_list_read_as_bosses_never_passes_silently() {
    // The flag flipped on a list that holds no boss: "a boss" of 2 mm radius inside the real Ø8 one. The
    // mill would orbit inside the part - the stock stands there, so the thread is refused by name.
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    let msg = try_thread(&world, &mut history, MULTI, external(vec![boss(2.0)], None, ThreadHand::Right, true));
    assert!(msg.contains("is not cleared"), "{msg}");
}

#[test]
fn the_external_flag_edits_undoes_and_round_trips_the_project_file() {
    let (world, mut history) = stud_world(stud(4.0, Vec::new()));
    let params = external(vec![boss(4.0)], None, ThreadHand::Left, false);
    assert_eq!(try_thread(&world, &mut history, MULTI, params.clone()), "");
    // Written only when set: every internal thread's file text is what it was.
    let dto = camcraft_app::project::params_to_json(&params);
    assert_eq!(dto["thread"]["external"], serde_json::json!(true), "{dto}");
    let mut internal = params.clone();
    if let OpParams::Thread { external, holes, .. } = &mut internal {
        *external = false;
        holes.clear();
    }
    let dto = camcraft_app::project::params_to_json(&internal);
    assert!(dto["thread"].get("external").is_none(), "{dto}");
    // The edit is one undoable command, and it marks the cached path stale.
    history.execute(&world, EditOperationParams::new(&world, "Thread1", internal.clone()).expect("an internal thread with no holes yet"));
    assert_eq!(thread_params(&world), internal);
    assert_eq!(status(&world, "Thread1"), OpStatus::Stale);
    assert!(history.undo(&world));
    assert_eq!(thread_params(&world), params);
    assert_eq!(status(&world, "Thread1"), OpStatus::Ok);
    let path = std::env::temp_dir().join(format!("camcraft_985_boss_{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let loaded = new_world();
    camcraft_app::project::load(&loaded, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(thread_params(&loaded), params);
}

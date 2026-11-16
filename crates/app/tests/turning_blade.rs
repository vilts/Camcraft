//! M5.2 stage C blade tests at the app level: the wiring - a blade
//! kind's whole life through the document. What "right" means here:
//! a grooving op on a mounted lathe generates, verifies through the
//! revolved-blade verdict (no sphere speaks it) and reads Ok; the
//! cached path carries the kernel's signature seen from the world
//! (radial plunges, one station each, landing on the model's own
//! floor, rapids above the skin); the wrong tool on a blade kind is
//! refused by name at generation; a re-ground blade (width edited on
//! the tool entity) marks every op that rides it Stale - through
//! the TOOL digest, not the params one - and reads fresh again after
//! regenerate, in both directions of undo. The kernels' own laws
//! are pinned one crate down (camcraft-motion's turning_blade
//! suite); this file is the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the blade arms
//! of generate/heights, the revolved verdict wiring, the stamp's
//! kind-scoped width digest), camcraft_app::commands (Create/
//! GenerateOperation, ChangeTool undo).
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

/// The grooved bar, app-sized: wall r 6 from z -16 to 28 with a
/// 3.2-wide groove (floor r 4, stations -6.8 .. -3.6) - wide enough
/// for both catalog blades to tile it, the numbers the staleness
/// test's re-grind rides on, and long enough that its end caps
/// cannot tip the axis fit's eigen balance past the dome guard
/// (a stubby capped cylinder reads isotropic there - the motion
/// fixtures' revolve builder gives its bars the same room).
fn bar_mesh() -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in [
        (0.0, -16.0),
        (6.0, -16.0),
        (6.0, -6.8),
        (4.0, -6.8),
        (4.0, -3.6),
        (6.0, -3.6),
        (6.0, 28.0),
        (0.0, 28.0),
    ]
    .windows(2)
    {
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

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the blade kinds live in: bar in bar-sized stock, the
/// lathe mounted, both catalog blades on the rack.
fn blade_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // Bar-sized stock (#103): the bar's own diameter over its
            // Z span - the turning kernels never cut stock beyond the
            // part (#130), so a wider blank is a real crash.
            camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 44.0 },
            Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, "GT-0200");
    add_tool(&world, &mut history, "GT-0300");
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", bar_mesh()));
    (world, history)
}

fn groove_finish_params() -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        feed: 150.0,
        plunge_feed: 60.0,
    }
}

fn part_params(plane_z: f64) -> OpParams {
    OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(28.0),
        bottom_z: Height::fixed(plane_z),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
    }
}

fn create(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

#[test]
fn a_groove_finish_runs_clean_in_the_document() {
    // The document's whole life: create, generate, and the revolved
    // verdict - the one no sphere can speak - comes back silent, so
    // the op reads Ok. The cached path from the world's side: two
    // radial plunges (the 3.2 window tiles exactly two 2.0 kerfs),
    // each landing on the model's own floor at r 4, every traverse
    // above the skin.
    let (world, mut history) = blade_world();
    create(&world, &mut history, "G1", "GT-0200", groove_finish_params());
    assert_eq!(status(&world, "G1"), OpStatus::Ok, "the meridian verdict is silent");
    let t = cache(&world, "G1").expect("cached").path;
    let mut feeds = 0;
    for m in &t.moves {
        if m.kind == camcraft_app::cl::MoveKind::Feed {
            assert!(
                (m.from.z - m.to.z).abs() < 1e-9 && rho(m.to) <= rho(m.from) + 1e-9,
                "a blade feed holds its station and sinks inward: {m:?}"
            );
            assert!(
                (m.to.x * m.to.x + m.to.y * m.to.y).sqrt() < 4.0 + 1e-9
                    && (m.to.x * m.to.x + m.to.y * m.to.y + m.to.z * m.to.z).sqrt() > 3.9,
                "the plunge lands on the floor: {:?}",
                m.to
            );
            assert!((m.to.z + 5.8).abs() < 1e-9 || (m.to.z + 4.6).abs() < 1e-9, "on a center: {:?}", m.to);
            feeds += 1;
        } else {
            assert!(rho(m.to) > 6.0 - 1e-9 || m.to.z > 19.9, "a traverse clears the bar: {:?}", m.to);
        }
    }
    assert_eq!(feeds, 2, "a 3.2 window eats two 2.0-wide kerfs");
}

#[test]
fn a_part_op_severs_the_bar_in_the_document() {
    // Parting at z -10: one center, a rung ladder to the axis - the
    // LAST feed stands on it (the kerf's through cut, snapped exact,
    // not a hair off), and the verdict reads the separation silent.
    let (world, mut history) = blade_world();
    create(&world, &mut history, "P1", "GT-0300", part_params(-10.0));
    assert_eq!(status(&world, "P1"), OpStatus::Ok, "the kerf is a cut, not a verdict");
    let t = cache(&world, "P1").expect("cached").path;
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed).collect();
    assert_eq!(feeds.len(), 12, "the 6 of column at 0.5 rungs");
    for m in &feeds {
        assert!((m.to.z + 10.0).abs() < 1e-9 && (m.from.z + 10.0).abs() < 1e-9, "one center: {m:?}");
    }
    assert!(rho(feeds[0].to) > 5.4, "the first rung steps the column: {:?}", feeds[0].to);
    assert!(rho(feeds[11].to) < 1e-12, "the last rung stands ON the axis: {:?}", feeds[11].to);
}

#[test]
fn a_blade_kind_refuses_the_wrong_tool_by_name() {
    // The family door admits the KIND; the tool door is the
    // generator's, and the kerf's width IS the tool - so a turning
    // insert on a Part names the op, the tool and what the tool is.
    let (world, mut history) = blade_world();
    common::create_past_the_tool_law(&world, &mut history, "T1", "TN-1600-0R8", SrcRef::Entity("bar".into()), part_params(-10.0));
    let Err(err) = common::generate_past_the_tool_law(&world, "T1") else {
        panic!("a round nose cannot cut a band");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("needs a grooving blade") && msg.contains("TN-1600-0R8")
            && msg.contains("turning insert"),
        "{msg}"
    );
}

#[test]
fn an_empty_window_refuses_by_name() {
    // top == bottom is a window with no downward span: the create
    // door says so by those words (validate early - the kernel's
    // own "holds no length of the bar" is the same law's second
    // line, pinned in the motion suite), never an empty path
    // dressed as success.
    let (world, _history) = blade_world();
    let params = OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-5.0),
        bottom_z: Height::fixed(-5.0),
        feed: 150.0,
        plunge_feed: 60.0,
    };
    let Err(err) =
        CreateOperation::new(&world, "G1", "GT-0200", SrcRef::Entity("bar".into()), params)
    else {
        panic!("a window with no downward span is no window");
    };
    let msg = err.to_string();
    assert!(msg.contains("must cut downward"), "{msg}");
}

#[test]
fn the_blade_seeds_stand() {
    // Registry end to end: pick the kind, accept the seed, press
    // Generate - rough, finish and part all path on the document
    // bar with the seeds' own numbers.
    let (world, mut history) = blade_world();
    let c = Carried {
        feed: 150.0,
        plunge: 80.0,
        climb: true,
        clearance: Height::fixed(20.0),
        stepover: 3.0,
        stepdown: 5.0, // the mill's fold: the seeds must NOT carry it
        top: Height::fixed(-3.6),
        bottom: Height::fixed(-6.8),
        retract: Height::fixed(18.0),
        tilt: None,
    };
    let rough = (KindInfo::ALL[OpType::GrooveRough as usize].seed)(&c);
    create(&world, &mut history, "SR", "GT-0200", rough);
    assert_eq!(status(&world, "SR"), OpStatus::Ok, "the rough seed's numbers stand");
    let fin = (KindInfo::ALL[OpType::GrooveFinish as usize].seed)(&c);
    create(&world, &mut history, "SF", "GT-0200", fin);
    assert_eq!(status(&world, "SF"), OpStatus::Ok, "the finish seed's numbers stand");
    let c2 = Carried {
        bottom: Height::fixed(-10.0),
        top: Height::fixed(28.0),
        ..c
    };
    let part = (KindInfo::ALL[OpType::Part as usize].seed)(&c2);
    create(&world, &mut history, "SP", "GT-0300", part);
    assert_eq!(status(&world, "SP"), OpStatus::Ok, "the part seed's numbers stand");
}

#[test]
fn a_reground_blade_marks_every_op_stale_both_ways() {
    // The width rides the TOOL digest (M5.2 C): re-grind the blade
    // in the tool list and the op cannot know its cached path from
    // a different steel - regenerate, and undoing the grind reads
    // Stale again (the digest is a question about NOW, never a
    // remembered verdict).
    let (world, mut history) = blade_world();
    create(&world, &mut history, "G1", "GT-0200", groove_finish_params());
    assert_eq!(status(&world, "G1"), OpStatus::Ok);
    let mut wide = catalog_tool("GT-0200");
    wide.width = 3.0; // re-ground, same name
    history.execute(&world, ChangeTool::new(&world, "GT-0200", wide).expect("a sane blade"));
    assert_eq!(status(&world, "G1"), OpStatus::Stale, "a re-ground blade is a different cut");
    history.execute(&world, GenerateOperation::new(&world, "G1").expect("regenerate"));
    assert_eq!(status(&world, "G1"), OpStatus::Ok, "the 3.0 blade tiles the 3.2 window too");
    assert!(history.undo(&world), "undo the grind");
    assert_eq!(status(&world, "G1"), OpStatus::Stale, "and the cached 3.0 path reads stale now");
}

// ---------------------------------------------------------------- M5.2 G.2
// The bar feeder's signals are authored numbers the POST reads: they
// ride the stamp, the file, and the program - and never a toolpath.

#[test]
fn the_feeder_signals_reach_the_posted_program() {
    // End to end in a live document: flags on -> the program says
    // load, advance, unload around the section's own lines; the
    // same world with silent flags writes no feeder lines at all
    // (the byte-identity the goldens keep watching).
    let (world, mut history) = blade_world();
    let mut p = part_params(-10.0);
    if let OpParams::Part { bar_load, bar_advance, bar_unload, bar_transfer, .. } = &mut p {
        *bar_load = true;
        *bar_advance = 25.0;
        *bar_unload = true;
        *bar_transfer = true;
    }
    create(&world, &mut history, "P1", "GT-0300", p.clone());
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-feeder-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    camcraft_app::posting::post_setup(&world, &path).expect("the sever posts");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(
        text.contains("(PART LOAD ADVANCE 25)")
            && text.contains("(PART TRANSFER 1->2)")
            && text.contains("(PART UNLOAD)"),
        "the events must voice, in the choreography's order: {text}"
    );
    let at = |needle: &str| text.find(needle).expect("present");
    assert!(
        at("(PART LOAD") < at("(PART TRANSFER") && at("(PART TRANSFER") < at("(PART UNLOAD)"),
        "load, cut, hand off, unload - the program tells the story in order"
    );
    // The stamp: the flags change what posts, so they dirty the
    // cache exactly like an authored feed would.
    let mut quiet = p.clone();
    if let OpParams::Part { bar_load, bar_unload, bar_advance, bar_transfer, .. } = &mut quiet {
        *bar_load = false;
        *bar_unload = false;
        *bar_advance = 0.0;
        *bar_transfer = false;
    }
    history.execute(
        &world,
        EditOperationParams::new(&world, "P1", quiet).expect("edit"),
    );
    assert_eq!(
        status(&world, "P1"),
        OpStatus::Stale,
        "the signals changed the program, so they changed the stamp"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_handoff_needs_the_steel_it_asks_for() {
    // The program gate's new clause (M5.3 C): stated spindles check,
    // unstated check nothing - the transfer flag alone never blocks
    // a post on a machine that said nothing about its hands, and a
    // one-spindle lathe hears the refusal by the op's name, the
    // machine's own, and the number it stated.
    let (world, mut history) = blade_world();
    let mut p = part_params(-10.0);
    if let OpParams::Part { bar_transfer, .. } = &mut p {
        *bar_transfer = true;
    }
    create(&world, &mut history, "P1", "GT-0300", p);
    let with = |n: Option<usize>| {
        let m = Machine { spindles: n, ..machine("lathe") };
        camcraft_app::machine::check_document(&world, &m)
            .expect("a fresh program checks")
            .problems
    };
    let one = with(Some(1));
    assert!(
        one.iter().any(|x| x.starts_with("P1:")
            && x.contains("second spindle")
            && x.contains("TestBench lathe")
            && x.contains("1 spindle")),
        "the refusal names op, machine and count: {one:?}"
    );
    assert!(
        with(Some(2)).iter().all(|x| !x.contains("second spindle")),
        "two hands, no complaint"
    );
    assert!(
        with(None).iter().all(|x| !x.contains("second spindle")),
        "unstated checks nothing - the same floor the tilt refusal stands on"
    );
}

#[test]
fn a_negative_bar_advance_is_refused_at_the_door() {
    // Validate early, name the door: an unmeasurable advance is
    // created never, not posted wrong.
    let (world, _h) = blade_world();
    let mut p = part_params(-10.0);
    if let OpParams::Part { bar_advance, .. } = &mut p {
        *bar_advance = -5.0;
    }
    let Err(err) = CreateOperation::new(&world, "P1", "GT-0300", SrcRef::Entity("bar".into()), p)
    else {
        panic!("a negative advance cannot be authored");
    };
    assert!(format!("{err:?}").contains("not usable"), "the door must speak: {err:?}");
}

// ---------------------------------------------------------------- M5.3 D
// The measurement ask: authored on the Part op's parameters, voiced
// as one PROBE comment on the stream - and JUDGED AT THE POST, on
// the composition. The probe touches the PART, and the part is what
// every blade of the program cut TOGETHER: a rough holding steel
// back for a finish leaves nothing on the part once the finish
// answers for it, and leaves exactly that floor when it never does.
// The fixtures ride that law, not a lattice's arithmetic: a
// GrooveRough with leave 0.25 stands 0.25 mm off the model floor by
// construction (its floor is `b + leave`), which its own op-level
// bar forgives (tolerance + leave) and a 0.05 ask does not.

fn groove_rough_params(leave: f64) -> OpParams {
    OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        stepdown: 0.5,
        stock_to_leave: leave,
        feed: 150.0,
        plunge_feed: 60.0,
    }
}

/// A grooved bar roughed (G1), optionally finished (G2), parted
/// (P1) with a probe ask on the parting - the program in the order
/// its lifecycle reads. The rough's leave is the deviation; the
/// finish's presence is whether the program answers for it.
fn probe_program(budget: f64, with_finish: bool) -> (World, History) {
    let (world, mut history) = blade_world();
    create(&world, &mut history, "G1", "GT-0200", groove_rough_params(0.25));
    if with_finish {
        create(&world, &mut history, "G2", "GT-0200", groove_finish_params());
    }
    let mut p = part_params(-10.0);
    if let OpParams::Part { bar_load, bar_unload, bar_transfer, inspect_tol, .. } = &mut p {
        *bar_load = true;
        *bar_unload = true;
        *bar_transfer = true;
        *inspect_tol = budget;
    }
    create(&world, &mut history, "P1", "GT-0300", p);
    (world, history)
}

fn post_txt(world: &World, tag: &str) -> Result<String, String> {
    let path = std::env::temp_dir().join(format!("camcraft-probe-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let r = camcraft_app::posting::post_setup(world, &path);
    let out = match &r {
        Ok(_) => std::fs::read_to_string(&path).expect("written"),
        Err(e) => e.clone(),
    };
    let _ = std::fs::remove_file(&path);
    r.map(|_| out)
}

#[test]
fn a_left_floor_under_the_probe_budget_refuses_the_post() {
    // The verdict as a decision, and composed: the parting op's own
    // cut is clean, but the rough's left floor stands under the
    // ask - the part as machined CANNOT measure to it, and no
    // per-op rule could have said so. The refusal names the ask,
    // the asker, the guilty leg and the act (re-run); a clean
    // program's numbers appear nowhere in it.
    let (world, _h) = probe_program(0.05, false);
    let err = post_txt(&world, "refuse").expect_err("left steel under the ask blocks the post");
    assert!(err.contains("post blocked"), "{err}");
    assert!(
        err.contains("probe ask") && err.contains("P1") && err.contains("G1") && err.contains("re-run"),
        "the refusal names ask, asker, culprit and act: {err}"
    );
    assert!(err.contains("left standing"), "the floor clause, not a phantom gouge: {err}");
}

#[test]
fn a_finish_that_answers_for_the_leave_posts() {
    // The composition's honesty, in the other direction: the same
    // 0.25 mm the rough stood on the part is now cut away by G2,
    // and the union's floor is the model's - silence, and a posted
    // program. A per-op verdict could not pass this test: it would
    // convict the rough for a promise the program kept. This is
    // also the choreography check: LOAD, cut, PROBE, TRANSFER,
    // UNLOAD, in the posted order.
    let (world, _h) = probe_program(0.05, true);
    let text = post_txt(&world, "finish").expect("the answered promise posts");
    assert!(text.contains("(PROBE TOL"), "the ask voices: {text}");
    assert!(
        !text.contains("probe ask") && !text.contains("left standing"),
        "the composition measures silence, so the program says nothing of it:\n{text}"
    );
    let at = |needle: &str| text.find(needle).expect("present");
    assert!(
        at("(PART LOAD") < at("(PROBE TOL")
            && at("(PROBE TOL") < at("(PART TRANSFER")
            && at("(PART TRANSFER") < at("(PART UNLOAD)"),
        "load, cut, measure, hand off, unload - in order"
    );
}

#[test]
fn a_budget_over_the_deviation_posts_the_ask_and_says_nothing() {
    // The ask stands, the verdict does not: a bar of r6 by 44 long
    // cannot hide a 50 mm floor, so the program voices PROBE and
    // the gate has nothing to say. The budget edit still STALES the
    // op - the number changed the posted program even though no
    // geometry reads it - the stamp's own law, unchanged by where
    // the verdict lives.
    let (world, mut h) = probe_program(50.0, false);
    let text = post_txt(&world, "loose").expect("a meetable budget posts");
    assert!(text.contains("(PROBE TOL"), "the ask voices: {text}");
    assert!(!text.contains("probe ask"), "no verdict text rides the program: {text}");
    let mut p = part_params(-10.0);
    if let OpParams::Part { bar_load, bar_unload, bar_transfer, inspect_tol, .. } = &mut p {
        *bar_load = true;
        *bar_unload = true;
        *bar_transfer = true;
        *inspect_tol = 1.0;
    }
    h.execute(&world, EditOperationParams::new(&world, "P1", p).expect("edit"));
    assert_eq!(
        status(&world, "P1"),
        OpStatus::Stale,
        "the budget changed the program, so it changed the stamp"
    );
}

#[test]
fn a_clean_parting_measures_zero_under_the_ask() {
    // The vacuity guard, aimed the right way: a parting cut's own
    // band stands inside its cutoff kerf - what stands there was
    // meant to go - so a program that leaves steel nowhere measures
    // 0 mm deviation and posts at ANY ask. (The v2 law made this
    // the whole verdict and it meant nothing; under the composition
    // it means the program is clean, which is only worth saying
    // because the test above proves the same gate convicts a
    // program that is not.)
    let (world, mut history) = blade_world();
    let mut p = part_params(-10.0);
    if let OpParams::Part { inspect_tol, .. } = &mut p {
        *inspect_tol = 0.05;
    }
    create(&world, &mut history, "P1", "GT-0300", p);
    post_txt(&world, "solo").expect("a clean part leaves nothing for a probe to find");
}

#[test]
fn a_negative_probe_budget_is_refused_at_the_door() {
    // The advance's law again, in the measuring direction: a budget
    // that cannot be measured to was never authored.
    let (world, _h) = blade_world();
    let mut p = part_params(-10.0);
    if let OpParams::Part { inspect_tol, .. } = &mut p {
        *inspect_tol = -0.05;
    }
    let Err(err) = CreateOperation::new(&world, "P1", "GT-0300", SrcRef::Entity("bar".into()), p)
    else {
        panic!("a negative budget cannot be authored");
    };
    assert!(format!("{err:?}").contains("not usable"), "the door must speak: {err:?}");
}

//! M2.4 C acceptance: the two names that end the table. Bore is the
//! drill's hole list spoken as a ride - radial sweep, one ring per
//! level per hole, the cutter-center law rc = radius - (tool_r +
//! leave) keeping every feed move inside the finished wall. Thread
//! is the helix about that same list, timed by the TOOL's pitch (the
//! tap cycle's law) and deepened by the ISO 724 ridge constant. Both
//! rows seat inside their sections' runs (Thread at 10, Bore at 12),
//! so the ladder takes a two-boundary step - ribbon.rs's v44 rung
//! test says it in pins; this file says it in G-code.
//!
//! Provenance: catalog naming per Autodesk's published HSMWorks user
//! documentation (behavior only); the boring sweep and the helical
//! thread are machine-tool handbook geometry written clean-room
//! (ISO 724's thread depth 0.5413 x pitch; the ring fit is the
//! pocket family's chord tolerance law on one circle).
//!
//! Gate map (M4.4): covers app::commands, app::geometry, motion::machining2d,
//! app::model, app::operation, app::project, app::schema, geometry::geometry::stock,
//! tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, tool: Tool, name: &str) {
    let cmd = AddTool::new(world, name, tool);
    history.execute(world, cmd.expect("tool"));
}

/// A multi-form thread mill (#894): the pitch is the tool's own.
fn thread_mill(diameter: f64, pitch: f64) -> Tool {
    Tool { material_group: "steel".into(), ..camcraft_app::thread_mill::thread_mill(diameter, pitch, 0.0, 25.0) }
}

/// #992: an op made and generated before `tag` - name, (tool name, tool), params.
type Ahead<'a> = (&'a str, (&'a str, Option<Tool>), OpParams);
fn job_tool(tag: &str, tool: Tool, tool_name: &str, params: OpParams, ahead: &[Ahead]) -> (World, History, camcraft_app::cl::Toolpath) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
            Location::default(),
        )
        .expect("box stock"),
    );
    add_tool(&world, &mut history, tool, tool_name);
    for (name, (tn, t), p) in ahead.iter().map(|(n, t, p)| (*n, t.clone(), p.clone())).chain([(tag, (tool_name, None), params)]) {
        if let Some(t) = t {
            add_tool(&world, &mut history, t, tn);
        }
        history.execute(&world, CreateOperation::new(&world, name, tn, SrcRef::Stock, p).expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    let e = model::find_by_name(&world, tag).expect("op exists");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "{tag} reads Stale against its own lone world");
    let path = model::toolpath_of(&world, &e).expect("cached").path;
    (world, history, path)
}

fn door_world(tool: Tool, name: &str) -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
            .expect("box stock"),
    );
    add_tool(&world, &mut history, tool, name);
    world
}

fn refuse(world: &World, tool_name: &str, params: OpParams) -> String {
    let op = Operation {
        tool: tool_name.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    match operation::generate(world, &op, "") {
        Ok((path, _)) => panic!("a door was meant to refuse, got {} moves", path.moves.len()),
        Err(e) => e.to_string(),
    }
}

fn hole(x: f64, y: f64, radius: f64, floor_z: f64, through: bool) -> Hole {
    Hole {
        xy: Point2::new(x, y),
        radius,
        entry_z: 20.0,
        floor_z,
        through,
        stepped: false,
    }
}

fn bore(holes: Vec<Hole>) -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes,
    }
}

fn thread(holes: Vec<Hole>) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        infeed: 0.1,
        feed: 400.0,
        plunge_feed: 150.0,
        climb: true,
        holes,
        pitch: None,
        hand: camcraft_app::cl::ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
    }
}

#[test]
fn registry_rows_seat_the_pair_in_their_runs() {
    // The seating law (M2.4 C): the partition's contiguity rule and
    // the family-at-the-tail rule together forbid a tail-append, so
    // the pair seats IN its section's run - Thread closes the 2D
    // mill's run at 10, Bore joins Drilling behind the drill at 12 -
    // and every tag still equals its row index across the grown
    // table.
    for (i, row) in KindInfo::ALL.iter().enumerate() {
        assert_eq!(row.tag as usize, i, "row {}: tag says {}", i, row.tag);
    }
    assert_eq!((OpType::Thread.ordinal(), OpType::Bore.ordinal()), (10, 12));
    assert_eq!(KindInfo::ALL[10].label, "Thread");
    assert_eq!(KindInfo::ALL[12].label, "Bore");
    assert_eq!(KindInfo::ALL[10].section, camcraft_app::operation::Section::Mill2D);
    assert_eq!(KindInfo::ALL[12].section, camcraft_app::operation::Section::Drilling);
    // Order-within-section: the drill leads its run, the boring ride
    // closes it; the 2D mill's run keeps its head at Face and gains
    // the thread mill at its tail.
    let drilling = camcraft_app::operation::Section::Drilling.kinds();
    assert_eq!(KindInfo::ALL[drilling[0]].ty, OpType::Drill);
    // #108 seats the probe behind the boring ride: Probe closes the
    // run now, Bore stands just before it.
    assert_eq!(KindInfo::ALL[*drilling.last().unwrap()].ty, OpType::Probe);
    assert_eq!(KindInfo::ALL[drilling[drilling.len() - 2]].ty, OpType::Bore);
}

#[test]
fn bore_rides_the_cutter_center_law() {
    // Ø20 holes, an 8-wide cutter: the center circle is rc = 10 - 4 =
    // 6 about each hole, and NO feed move stands further from the
    // hole center than that - the never-gouge door of this kernel.
    let holes = vec![hole(15.0, 15.0, 10.0, 5.0, false), hole(45.0, 15.0, 10.0, 5.0, false)];
    let (_w, _h, path) = job_tool("BOR", catalog_tool("EM-0800-4FL"), "EM-0800-4FL", bore(holes.clone()), &[]);
    assert!(path.all_finite());
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty());
    for m in &feeds {
        let inside = holes.iter().any(|p| {
            (m.to.x - p.xy.x).hypot(m.to.y - p.xy.y) <= p.radius - 4.0 + 1e-9
        });
        assert!(inside, "a boring move outside every cutter-center disk: {m:?}");
    }
    // The ladder speaks on Z: `cut_levels`' law restated - the same
    // rungs the pocket walks from the stock top, so the boring ride
    // starts AT the hole's entry (its first ring squares the mouth)
    // and closes on each hole's own floor.
    let mut zs: Vec<_> = feeds.iter().map(|m| m.to.z).collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(zs, vec![20.0, 15.0, 10.0, 5.0], "rungs of 5, entry plane to floor");
    // Two holes, so the ladder runs twice - and the ride between them
    // is never a feed through air at depth: rapids link the islands.
    let hops = path.moves.iter().filter(|m| m.kind == MoveKind::Rapid).count();
    assert!(hops >= 3, "between holes the family hops: {hops} rapids");
}

#[test]
fn a_stepped_hole_is_declined_by_name() {
    // A counterbore fold is a second wall this ride cannot see -
    // refusal, not silent partial work (the Prime Directive's shape).
    let mut stepped = hole(15.0, 15.0, 10.0, 5.0, false);
    stepped.stepped = true;
    let msg = refuse(
        &door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL"),
        "EM-0800-4FL",
        bore(vec![stepped]),
    );
    assert!(msg.contains("counterbore fold"), "{msg}");
}

#[test]
fn a_cutter_with_no_circle_inside_is_refused() {
    // rc = radius - (tool_r + leave) must stand positive: an 8-wide
    // cutter in a 7 mm hole has no rim circle to ride, and the door
    // says so before the path is ever built (never an empty path).
    let msg = refuse(
        &door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL"),
        "EM-0800-4FL",
        bore(vec![hole(15.0, 15.0, 3.5, 5.0, false)]),
    );
    assert!(msg.contains("no circle left inside"), "{msg}");
}

#[test]
fn thread_helices_are_timed_by_the_tools_pitch() {
    // R10 wall, a 6-wide thread mill, pitch 1.5: the finished circle
    // is rho = R + H - r with H = 0.5413 x 1.5; every feed move off
    // the axis stands between the tangent circle (R - r) and that
    // circle - never past the major diameter.
    let pitch = 1.5;
    let h = 0.5413 * pitch;
    let (r, big) = (3.0, 10.0);
    let holes = vec![hole(15.0, 15.0, big, 6.0, false)];
    // #992: a Ø20 drill opens the hole first, its point at the stock's bottom (clear of the thread's floor).
    let drill = Tool { diameter: 20.0, shank_d: 20.0, flute_len: 30.0, ..catalog_tool("DR-0600") };
    let OpParams::Thread { clearance_z, top_z, .. } = thread(vec![]) else { unreachable!() };
    let holes_dr = vec![hole(15.0, 15.0, big, 0.0, false)];
    let dr = OpParams::Drill { clearance_z, retract_z: None, top_z, bottom_z: None, cycle: camcraft_app::drilling::CycleChoice::Drill,
        peck: 0.0, dwell: 0.0, feed: 100.0, plunge_feed: 80.0, holes: holes_dr, cross: vec![] };
    let (_w, _h, path) = job_tool("THD", thread_mill(6.0, pitch), "TM-0600", thread(holes.clone()), &[("DR", ("DR-20", Some(drill)), dr)]);
    assert!(path.all_finite());
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(feeds.len() > 30, "a threaded wall is a long helix, got {}", feeds.len());
    let rho0 = big - r;
    let rho1 = big + h - r + 1e-9;
    for m in &feeds {
        let rho = (m.to.x - holes[0].xy.x).hypot(m.to.y - holes[0].xy.y);
        // The axis is legitimate air - the plunge runs at the center
        // because the whole disc fits inside the hole; it is the
        // OFF-axis moves that must stay on the thread's rings.
        if rho < 1e-6 {
            continue;
        }
        assert!((rho0 - 1e-9..=rho1).contains(&rho), "off the thread envelope: {m:?}");
    }
    // #894: right-hand climb winds UP from the floor, half a pitch
    // per half-turn (counter-clockwise rising is a right-hand helix).
    let up = feeds.windows(2).any(|w2| (w2[1].to.z - w2[0].to.z - pitch * 0.5).abs() < 1e-9);
    assert!(up, "the helix rises half a pitch per half-turn");
    let far = feeds.iter().map(|m| (m.to.x - 15.0).hypot(m.to.y - 15.0)).fold(0.0, f64::max);
    assert!((far - (big + h - r)).abs() < 2e-6, "the last pass lands on the major circle: {far}");
}

#[test]
fn thread_refuses_a_tool_that_is_no_thread_mill_by_name() {
    // #894: an operation that still names another kind of cutter (an
    // old file, a retyped tool) is refused at generate by the tool's
    // own name - never a helix timed against a pitch it does not have.
    let msg = refuse(
        &door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL"),
        "EM-0800-4FL",
        thread(vec![hole(15.0, 15.0, 10.0, 6.0, false)]),
    );
    assert!(msg.contains("needs a thread mill") && msg.contains("flat end mill"), "{msg}");
}

#[test]
fn the_pair_marks_stale_on_what_it_reads() {
    // The stamp law made honest for the new names (M2.4 C): the
    // boring ride churns on the hole's own numbers it times against
    // - and the thread mill churns on the TOOL's pitch, the number
    // its helix is timed by. A pitch-only tool edit that left the op
    // Ok would be the silent-wrong answer the Prime Directive names.
    let w = door_world(catalog_tool("EM-0800-4FL"), "EM-0800-4FL");
    let src = SrcRef::Stock;
    let holes = vec![hole(15.0, 15.0, 10.0, 5.0, false)];
    let st = |p: &OpParams| operation::current_stamp(&w, "EM-0800-4FL", &src, p, "X", None);
    let before = st(&bore(holes.clone())).expect("bore stamps");
    assert_eq!(before, st(&bore(holes.clone())).expect("again"));
    let moved = {
        let mut h2 = holes.clone();
        h2[0].floor_z = 4.0;
        st(&bore(h2)).expect("bore stamps again")
    };
    assert_ne!(before, moved, "a hole floor edit must mark the boring ride Stale");
    // The thread digest: same op, tool re-stated with a new pitch.
    let w2 = door_world(thread_mill(6.0, 1.5), "TM");
    let st2 = |p: &OpParams| operation::current_stamp(&w2, "TM", &SrcRef::Stock, p, "X", None);
    let p15 = st2(&thread(holes.clone())).expect("thread stamps");
    let world3 = door_world(thread_mill(6.0, 1.75), "TM");
    let p175 = operation::current_stamp(&world3, "TM", &SrcRef::Stock, &thread(holes), "X", None).expect("stamps");
    assert_ne!(p15, p175, "a pitch-only tool edit must mark the thread mill Stale");
}

#[test]
fn the_new_rows_round_trip_through_the_project_file() {
    // The wire law, tail-appended: each variant carries its own tag
    // and every hole's whole story lands back exactly - including
    // the Option heights, which v45 says by key or not at all.
    for (key, params, tool) in [
        (
            "\"bore\"",
            bore(vec![hole(15.0, 15.0, 10.0, 5.0, false)]),
            ("EM-0800-4FL", catalog_tool("EM-0800-4FL")),
        ),
        ("\"thread\"", thread(vec![hole(15.0, 15.0, 10.0, 6.0, false)]), ("TM", thread_mill(6.0, 1.5))),
    ] {
        let path = std::env::temp_dir().join(format!(
            "camcraft_m24c_rt_{}_{}.camproj",
            std::process::id(),
            &key[1..key.len() - 1]
        ));
        let world = new_world();
        let mut history = History::default();
        history.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
                .expect("box stock"),
        );
        add_tool(&world, &mut history, tool.1, tool.0);
        history.execute(
            &world,
            CreateOperation::new(&world, "OP", tool.0, SrcRef::Stock, params.clone()).expect("create"),
        );
        project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains(key), "the params carry their tag {key}");
        // The rows this suite added entered the DTO grammar at v45;
        // every later bump's file must still carry them, so the
        // claim is "v45-era grammar or newer", never the exact head
        // (a hard pin here rotted twice in M2.5 A/B, whose gates
        // this suite never rode).
        let stated = text
            .split_once("\"version\"")
            .and_then(|(_, s)| {
                let digits: String =
                    s.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
                digits.parse::<u32>().ok()
            })
            .expect("the file states a numeric version");
        assert!(
            (45..=project::PROJECT_VERSION).contains(&stated),
            "the file must state v45-era grammar up to this build's head ({})",
            project::PROJECT_VERSION
        );
        let world2 = new_world();
        project::load(&world2, &path).expect("load");
        let e = model::find_by_name(&world2, "OP").expect("op");
        let loaded = model::operation_of(&world2, &e).expect("operation component");
        assert_eq!(loaded.params, params, "round trip must land every number of {key}");
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn through_holes_drop_the_exit_allowance_below_the_floor() {
    // The cycles' exit law in the ride's own words: a through hole
    // runs its ladder one allowance PAST the floor, so the rim
    // circle is cut whole on the way out - and a blind hole never
    // does. THROUGH_CLEAR is the drilling module's, read same.
    let clear = camcraft_app::drilling::THROUGH_CLEAR;
    let holes = vec![hole(15.0, 15.0, 10.0, 5.0, true)];
    let (_w, _h, path) = job_tool("BORT", catalog_tool("EM-0800-4FL"), "EM-0800-4FL", bore(holes), &[]);
    assert!(path.all_finite());
    let lowest = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!(
        (lowest - (5.0 - clear)).abs() < 1e-6,
        "a through hole's ladder bottoms at floor - allowance: {lowest}"
    );
}

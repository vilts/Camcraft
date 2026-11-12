//! The Fanuc LATHE post golden law (M13.5): `assets/posts/fanuc-lathe.cps`
//! posts the turning documents' own streams - facing, profile rough
//! and finish, groove finish and parting, the axial drill - to
//! byte-identical G-code, and refuses by name what a 2-axis lathe
//! cannot express (a move off every meridian, a posed move, a cycle
//! off the axis). The fixtures are the proven ones of the turning
//! suites; the goldens under `golden/nc/fanuc-lathe/` lock the
//! translation (diameter X, G18/G97/G99, G83 face drilling, G32
//! threading) - the gate's verdicts are locked by the posting suites.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post`
//!
//! The refusals the script names (off-meridian, posed, off-axis cycle,
//! stray XY arc, no spindle) ride synthetic streams in
//! `lathe_post_refusals.rs`.
//!
//! Gate map (M4.4): covers app::posting (stream_records, post_serves,
//! default_post_for, the registry), post::post (engine on the lathe
//! dialect), app::commands (SelectPost by name on a lathe document).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteOperation, EditOperationParams, GenerateOperation, History, ImportModel,
    LoadMachine, ReorderOperation, SelectPost, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Kind, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");

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

/// The closed-solid revolve the turning suites share, `segs` ways.
fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
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

/// A lathe that states what the post reads: a 12000 rpm ceiling (the
/// G50 clamp; the catalog drill runs 10000) and flood coolant (the
/// M8/M9 pair).
fn lathe() -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

/// The bar in stock of its own size (#103): a cylinder of the
/// meridian's widest diameter over its Z span - the turning kernels
/// never cut stock beyond the part (#130), so a wider blank is a real
/// crash the rapid law convicts.
fn bar_world(meridian: &[(f64, f64)], tools: &[&str]) -> (World, History) {
    let r = meridian.iter().map(|p| p.0).fold(0.0f64, f64::max);
    let z0 = meridian.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let z1 = meridian.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, z0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 2.0 * r, height: z1 - z0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(lathe()));
    history.execute(&world, cmd.expect("lathe"));
    for t in tools {
        add_tool(&world, &mut history, t);
    }
    history.execute(&world, ImportModel::from_mesh("bar", revolve(meridian, 16)));
    (world, history)
}

/// Bar in box stock with the box's base at `at`, the lathe mounted, the
/// named tools on the rack.
fn lathe_world_at(bar: Mesh, stock: (f64, f64, f64), at: Location, tools: &[&str]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: stock.0, dy: stock.1, dz: stock.2 },
        at,
    );
    history.execute(&world, cmd.expect("box stock"));
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(lathe()));
    history.execute(&world, cmd.expect("lathe"));
    for t in tools {
        add_tool(&world, &mut history, t);
    }
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, history)
}

fn create(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = camcraft_app::model::find_by_name(world, name).expect("op");
    assert_eq!(
        camcraft_app::operation::status_of(world, &e),
        camcraft_app::operation::OpStatus::Ok,
        "{name} generates"
    );
}

/// The document's own stream through the lathe script, gate-free:
/// the translation under test. The engine's warning list must be
/// exactly the stream's own WARN records (a fixture's verifier notes,
/// which the gate judges elsewhere) - the SCRIPT refused nothing, or
/// the refusal is a post bug, not a generator one.
fn program_of(world: &World) -> String {
    let records = posting::stream_records(world).expect("every op generated");
    let out = engine::run(LATHE_CPS, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs");
    let warns: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(out.warnings, warns, "the lathe post refused a turning stream by name");
    out.program
}

// ------------------------------------------------------------ goldens

#[test]
fn facing_posts_to_the_lathe_golden() {
    // The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
    let bar = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
    let (world, mut history) = bar_world(&bar, &["TN-1600-0R8"]);
    let params = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    create(&world, &mut history, "F1", "TN-1600-0R8", params);
    let text = program_of(&world);
    assert!(text.contains("G18") && text.contains("G99") && text.contains("G97 S3000 M03"), "{text}");
    assert!(text.contains("G50 S12000"), "the machine's ceiling clamps the spindle: {text}");
    assert!(text.contains("T0101"), "station and offset: {text}");
    // #630: the nose CENTRE ends on the axis, so the posted control
    // point (orientation 2, r 0.8) stands r past it - X-1.6 diameter -
    // and the last pass posts the facing plane itself, Z14, not 14 + r.
    assert!(text.contains("X-1.6 Z14 "), "the pass ends with the nose centre on the axis: {text}");
    assert!(!text.contains("Z14.8"), "a face at Z14 posts Z14, not the nose centre: {text}");
    assert!(text.contains("ORIENTATION 2"), "the tool note names the orientation: {text}");
    assert!(!text.contains("Y"), "a lathe program has no Y word: {text}");
    common::write_golden("nc/fanuc-lathe/facing.nc", &text);
}

/// The stage-B flanged bar: neck r3.5 so the rough ladder has rungs.
const FLANGED: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)];

fn flanged_world() -> (World, History) {
    bar_world(&FLANGED, &["TN-1600-0R8"])
}

/// The flanged bar's rough (down to `rough_bottom`) and its finish.
fn rough_finish(rough_bottom: f64) -> (OpParams, OpParams) {
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(rough_bottom),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let finish = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        feed: 120.0,
        plunge_feed: 60.0,
    };
    (rough, finish)
}

#[test]
fn a_rough_moved_after_deleted_or_made_shallower_stales_its_finish() {
    // #109 (PR #153 review): S1's retracts are credited by R1 turned
    // ahead of it (#159), so R1 is an input of S1's verdict. Moving R1
    // after S1, deleting it, or re-cutting it shallower must mark S1
    // Stale and refuse the post - never ship the credited retract.
    for case in ["reorder", "delete", "shallower"] {
        let (world, mut history) = flanged_world();
        let (rough, finish) = rough_finish(-16.0);
        create(&world, &mut history, "R1", "TN-1600-0R8", rough);
        create(&world, &mut history, "S1", "TN-1600-0R8", finish);
        match case {
            "reorder" => {
                let cmd = ReorderOperation::new(&world, "R1", "S1", true);
                history.execute(&world, cmd.expect("R1 after S1"));
            }
            "delete" => history.execute(&world, DeleteOperation::new(&world, "R1").expect("delete R1")),
            _ => {
                let cmd = EditOperationParams::new(&world, "R1", rough_finish(10.0).0);
                history.execute(&world, cmd.expect("a shallower rough"));
                history.execute(&world, GenerateOperation::new(&world, "R1").expect("regenerate R1"));
            }
        }
        let e = camcraft_app::model::find_by_name(&world, "S1").expect("op");
        assert_eq!(
            camcraft_app::operation::status_of(&world, &e),
            camcraft_app::operation::OpStatus::Stale,
            "{case}: the rough ahead of S1 moved"
        );
        let out = std::env::temp_dir().join(format!("camcraft-rough-moved-{case}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&out);
        match posting::post_setup(&world, &out) {
            Ok(_) => panic!("{case}: a finish credited by a moved rough posted"),
            Err(e) => assert!(e.contains("regenerate"), "{case}: refused, but not as stale: {e}"),
        }
        assert!(!out.exists(), "a refused post writes nothing");
    }
}

#[test]
fn profile_rough_and_finish_post_to_the_lathe_golden() {
    let (world, mut history) = flanged_world();
    let (rough, finish) = rough_finish(-16.0);
    // Program order reads names (ops created without an order sort by
    // key): R1 then S1, the rough ahead of the finish - the program the
    // gate accepts. Finish ahead of rough is a real rapid in stock,
    // pinned by `a_finish_posted_ahead_of_its_rough_is_convicted_by_name`.
    create(&world, &mut history, "R1", "TN-1600-0R8", rough);
    create(&world, &mut history, "S1", "TN-1600-0R8", finish);
    let text = program_of(&world);
    assert_eq!(text.matches("G97 S3000 M03").count(), 2, "one spindle start per section: {text}");
    assert!(text.contains("F0.05"), "150 mm/min at 3000 rpm is 0.05 mm/rev: {text}");
    assert!(text.contains("F0.04"), "120 mm/min at 3000 rpm is 0.04 mm/rev: {text}");
    common::write_golden("nc/fanuc-lathe/profile_rough_finish.nc", &text);
}

#[test]
fn a_finish_posted_ahead_of_its_rough_is_convicted_by_name() {
    // #159: ops created without an order post by name - P1 ahead of R1.
    // The finish then rides the raw Ø12 bar, and its radial retract at
    // Z 16 (r 4.3 -> 6.85) drives the nose through the neck's uncut
    // r 5.1..6 band: a rapid in stock, named on P1, and the post refuses.
    let (world, mut history) = flanged_world();
    let (rough, finish) = rough_finish(-16.0);
    // P1 is made first: R1 runs behind it, so its stamp digests P1
    // (#109) - made the other way round, R1 would read Stale and the
    // post would refuse it as stale before P1's rapid is reached.
    create(&world, &mut history, "P1", "TN-1600-0R8", finish);
    create(&world, &mut history, "R1", "TN-1600-0R8", rough);
    let e = camcraft_app::model::find_by_name(&world, "P1").expect("op");
    let warns = camcraft_app::model::toolpath_of(&world, &e).expect("cache").warnings;
    assert!(
        warns.iter().any(|w| matches!(w, camcraft_app::verify::Warning::RapidInStock { .. })),
        "the finish ahead of its rough went unnamed: {warns:?}"
    );
    let out = std::env::temp_dir().join(format!("camcraft-finish-first-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a finish ahead of its rough posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains("P1: rapid move"), "refused, but not by the rapid law on P1: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn groove_and_part_post_to_the_lathe_golden() {
    // The stage-C grooved bar: wall r6, a 3.2-wide groove to r4.
    let bar = [(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)];
    let (world, mut history) = bar_world(&bar, &["GT-0200", "GT-0300"]);
    let groove = OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        feed: 150.0,
        plunge_feed: 60.0,
    };
    let part = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(28.0),
        bottom_z: Height::fixed(-10.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 60.0,
        bar_load: true,
        bar_advance: 30.0,
        bar_unload: true,
        bar_transfer: false,
        inspect_tol: 0.0,
    };
    create(&world, &mut history, "G1", "GT-0200", groove);
    create(&world, &mut history, "P1", "GT-0300", part);
    let text = program_of(&world);
    assert!(text.contains("T0101") && text.contains("T0202"), "two stations: {text}");
    assert!(text.contains("(PART LOAD ADVANCE 30)") && text.contains("(PART UNLOAD)"), "{text}");
    // #641: the parting plane Z-10 is the kerf's centre; the 3 mm blade
    // posts its default left (-Z) corner, half a width below it.
    assert!(text.contains("Z-11.5 ") && !text.contains("Z-10 "), "the kerf's left corner: {text}");
    common::write_golden("nc/fanuc-lathe/groove_part.nc", &text);
}

#[test]
fn an_axial_drill_posts_through_the_document_with_the_lathe_post_chosen() {
    // The in-app wiring end to end: the lathe post chosen BY NAME
    // (the picker's law), the gate run, the program written with the
    // face cycle vocabulary - and the same text locked as a golden.
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
    // The bar's own envelope, z -20..20 (issue #91: the old box stood
    // z 0..44, its top 24 mm over the R plane - the rapid down to R
    // crossed declared stock).
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), ..Location::default() };
    let (world, mut history) = lathe_world_at(bar, (24.0, 24.0, 40.0), at, &["DR-0400"]);
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    let params = OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    };
    create(&world, &mut history, "AX1", "DR-0400", params);
    history.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("a registry name"),
    );
    assert!(posting::post_label(&world).contains("Fanuc Lathe"), "{}", posting::post_label(&world));
    let path = std::env::temp_dir().join(format!("camcraft-lathe-post-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("a clean lathe hole posts through the gate");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("G83 Z10 "), "the face drilling cycle to the floor: {text}");
    assert!(text.contains("G00 X0 Z"), "the positioning rapid ahead of the cycle: {text}");
    assert!(text.contains("G80"), "the cycle cancels: {text}");
    assert!(!text.contains("G81") && !text.contains("G98"), "no mill cycle words: {text}");
    assert_eq!(text, program_of(&world), "the gated program is the stream's own translation");
    common::write_golden("nc/fanuc-lathe/axial_drill.nc", &text);
}

// ------------------------------------------------------------ registry

#[test]
fn the_registry_lists_the_lathe_post_by_kind() {
    assert!(posting::BUILTINS.iter().any(|(n, _)| *n == "Fanuc Lathe"));
    assert!(posting::post_serves("Fanuc Lathe", Kind::Lathe));
    assert!(posting::post_serves("Fanuc Lathe", Kind::MillTurn));
    assert!(!posting::post_serves("Fanuc Lathe", Kind::Mill));
    assert!(posting::post_serves("Fanuc", Kind::Mill));
    assert!(posting::post_serves("Fanuc", Kind::MillTurn));
    assert!(!posting::post_serves("Fanuc", Kind::Lathe));
    assert_eq!(posting::default_post_for(Kind::Lathe), PostRef::Named("Fanuc Lathe".into()));
    assert_eq!(posting::default_post_for(Kind::Mill), PostRef::Builtin);
    assert_eq!(posting::default_post_for(Kind::MillTurn), PostRef::Builtin);
    // The picker's seam: a named choice the registry knows is accepted.
    let world = new_world();
    assert!(SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).is_ok());
}

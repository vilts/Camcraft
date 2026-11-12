//! The workholding rpm limit through a MILLING post (#1174, third
//! review): on a lathe or mill-turn a work-spindle section - a turning
//! kind, an on-centre drill - only cuts if the work turns, so it is
//! judged against the chuck whatever the post calls itself. Before the
//! fix a mill-turn's facing op at 3000 rpm under a 2000 rpm chuck posted
//! `S3000 M3` through `fanuc.cps` and `haas.cps` (main refused it). A
//! real milling section on a mill-turn through a milling post drives the
//! tool spindle and stays unjudged. The walk below covers every machine
//! kind x post dialect x section the limit can meet (C sections are
//! pinned in `lathe_workholding_live`).
//!
//! Gate map (M4.4): covers app::setup_workholding (work_speed's
//! milling-post arms), app::posting (post_script).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine, SetSetupWorkholdingRpm,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{seed_params, Height, KindInfo, OpParams, OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const HAAS: &str = include_str!("../../../assets/posts/haas.cps");
const CHUCK: f64 = 2000.0;
/// The finished bar r6, z -16..14, in a stock to z 16 (facing takes the 2 mm).
const BAR: [(f64, f64); 4] = [(0.0, -16.0), (6.0, -16.0), (6.0, 14.0), (0.0, 14.0)];

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

fn machine(kind: &str) -> Option<Machine> {
    (kind != "none").then(|| Machine {
        model: format!("TestBench {kind}"),
        kind: Some(kind.into()),
        spindle_max: Some(12_000.0),
        ..Default::default()
    })
}

/// Which section the op cuts: a turning facing pass (the work spindle)
/// or a mill Face pass (the tool spindle).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Section {
    Work,
    Tool,
}

/// The bar on machine `kind` ("none" mounts nothing), Setup1 stating a
/// 2000 rpm chuck, op K of `section` with its tool at `rpm` - generated.
/// `None` when the kind's door refuses the op on that machine.
fn doc(kind: &str, section: Section, rpm: f64) -> Option<World> {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 12.0, height: 32.0 }, at).expect("stock"));
    if let Some(m) = machine(kind) {
        h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    }
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 24)));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, SetSetupWorkholdingRpm::new(&world, "Setup1", Some(CHUCK)).expect("chuck limit"));
    let (tool, params) = match section {
        Section::Work => (
            "TN-1600-0R8",
            OpParams::Facing {
                tolerance: 0.05,
                clearance_z: Height::fixed(25.0),
                top_z: Height::fixed(16.0),
                bottom_z: Height::fixed(14.0),
                stepdown: 0.5,
                feed: 150.0,
                plunge_feed: 80.0,
            },
        ),
        Section::Tool => {
            let i = KindInfo::ALL.iter().position(|k| k.ty == OpType::Face).expect("mill Face");
            ("EM-1000-4FL", seed_params(i))
        }
    };
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, tool, Tool { rpm, ..t }).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "K", tool, SrcRef::Entity("bar".into()), params).ok()?);
    h.execute(&world, GenerateOperation::new(&world, "K").ok()?);
    Some(world)
}

fn post(world: &World, cps: &str, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-1174c-{}-{tag}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps).map(|p| p.program);
    let _ = std::fs::remove_file(&out);
    posted
}

#[test]
fn a_mill_turn_facing_op_through_a_milling_post_is_held_to_the_chuck() {
    for (name, cps) in [("fanuc", FANUC), ("haas", HAAS)] {
        let over = doc("mill-turn", Section::Work, 3000.0).expect("facing on a mill-turn");
        let why = post(&over, cps, name).err().unwrap_or_default();
        assert!(
            why.contains("K: turns the work at 3000 rpm (G97)") && why.contains("workholding maximum of 2000 rpm"),
            "{name}: facing at 3000 under a 2000 chuck is refused by name: {why}"
        );
        let at = doc("mill-turn", Section::Work, CHUCK).expect("facing on a mill-turn");
        let posted = post(&at, cps, name);
        assert!(posted.is_ok(), "{name}: exactly at the limit posts: {:?}", posted.err());
    }
}

#[test]
fn a_mill_turn_milling_section_through_a_milling_post_is_not_judged() {
    for (name, cps) in [("fanuc", FANUC), ("haas", HAAS)] {
        let w = doc("mill-turn", Section::Tool, 8000.0).expect("mill Face on a mill-turn");
        let program = post(&w, cps, name).unwrap_or_else(|e| panic!("{name}: the milling section posts: {e}"));
        assert!(program.contains("S8000"), "{name}: the tool spindle at the cutter's rpm: {program}");
    }
}

/// The verdict a cell must give: judged by the chuck limit (refused by
/// name over it), or not judged (anything else - posted, or refused for
/// another named reason).
fn judged(kind: &str, dialect: &str, section: Section) -> bool {
    let chucked = kind == "lathe" || kind == "mill-turn";
    match (dialect, section) {
        // A turning post opens a work section on the work spindle; the
        // lathe door (#1190) refuses a milling section by name first.
        ("turning", Section::Work) => true,
        ("turning", Section::Tool) => false,
        // An undeclared script (#1199): the stricter reading.
        ("undeclared", _) => true,
        // A milling post: a work section on a chucked machine cuts only
        // if the work turns; a milling section turns the tool - except
        // on a plain lathe, whose one spindle holds the chuck.
        ("milling", Section::Work) => chucked,
        ("milling", Section::Tool) => kind == "lathe",
        _ => unreachable!("{dialect}"),
    }
}

#[test]
fn every_machine_dialect_and_section_cell_gives_its_verdict() {
    let undeclared = LATHE.replacen("capabilities = 2;", "", 1);
    let mut rows = Vec::new();
    for kind in ["none", "mill", "mill-turn", "lathe"] {
        for (dialect, cps) in [("turning", LATHE), ("milling", FANUC), ("undeclared", undeclared.as_str())] {
            for section in [Section::Work, Section::Tool] {
                let rpm = if section == Section::Work { 3000.0 } else { 8000.0 };
                let Some(w) = doc(kind, section, rpm) else {
                    rows.push(format!("{kind:9} {dialect:10} {section:?}: not creatable on this machine"));
                    continue;
                };
                let got = post(&w, cps, &format!("{kind}-{dialect}-{section:?}"));
                let by_limit = got.as_ref().err().is_some_and(|e| e.contains("workholding maximum"));
                let want = judged(kind, dialect, section);
                rows.push(format!("{kind:9} {dialect:10} {section:?}: {}", if by_limit { "judged" } else { "not judged" }));
                assert_eq!(by_limit, want, "{kind} / {dialect} / {section:?}: {got:?}");
                if let Ok(program) = &got {
                    assert!(!(section == Section::Work && program.contains("G97 S3000")), "work spindle over the chuck: {program}");
                }
            }
        }
    }
    println!("{}", rows.join("\n"));
}

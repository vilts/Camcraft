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
/// `Err` with the creation refusal when the kind's door refuses the op on that machine.
fn doc(kind: &str, section: Section, rpm: f64) -> Result<World, String> {
    built(kind, None, section, rpm)
}

/// `doc`, then the machine is swapped to `remount` once the op exists: the
/// only way a section the lathe door refuses at creation reaches a post
/// on a lathe (the op was made on a mill, the machine changed after).
fn built(kind: &str, remount: Option<&str>, section: Section, rpm: f64) -> Result<World, String> {
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
    let create = CreateOperation::new(&world, "K", tool, SrcRef::Entity("bar".into()), params).map_err(|e| e.to_string())?;
    h.execute(&world, create);
    let generate = GenerateOperation::new(&world, "K").map_err(|e| e.to_string())?;
    h.execute(&world, generate);
    if let Some(k) = remount {
        h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), machine(k)).expect("remount"));
    }
    Ok(world)
}

fn post(world: &World, cps: &str, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-1174c-{}-{tag}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps).map(|p| p.program);
    let _ = std::fs::remove_file(&out);
    posted
}

/// The arm `work_speed` judges by the chuck (`(ToolSpindle, Refused)` on a
/// plain lathe): a milling section the lathe door refuses at creation,
/// made on a mill, then the machine becomes a plain lathe. Reached through
/// `program_problems_by` with a milling voice: over the chuck it is named;
/// at the limit it is clean. The post itself refuses the same section
/// first by the family clause (posting_family), so that refusal is pinned too.
#[test]
fn a_milling_section_on_a_plain_lathe_is_judged_by_the_chuck() {
    use camcraft_app::post::dialect::WorkVoice;
    use camcraft_app::setup_workholding::program_problems_by;
    let milling = |_: Option<&str>| WorkVoice::ToolSpindle;
    let over = built("mill", Some("lathe"), Section::Tool, 8000.0).expect("mill Face made on a mill");
    let problems = program_problems_by(&over, &milling);
    assert!(
        problems.iter().any(|p| p.contains("K: turns the work at 8000 rpm (G97)") && p.contains("2000 rpm")),
        "a milling section on a plain lathe is judged at 8000 over a 2000 chuck: {problems:?}"
    );
    for (name, cps) in [("fanuc", FANUC), ("haas", HAAS)] {
        let why = post(&over, cps, name).err().unwrap_or_default();
        assert!(why.contains("a Face operation cannot run on its setup's"), "{name}: the post refuses it first by family: {why}");
    }
    let at = built("mill", Some("lathe"), Section::Tool, CHUCK).expect("mill Face made on a mill");
    assert!(program_problems_by(&at, &milling).is_empty(), "exactly at the chuck limit is clean");
}

#[test]
fn a_mill_turn_facing_op_through_a_milling_post_is_refused_and_held_to_the_chuck() {
    // #1232 (maintainer, 2026-11-12: "Refuse it"): a work-spindle
    // section needs a turning post on a mill-turn as on a lathe - so
    // the facing op is refused by name even at the chuck limit, and
    // over it the chuck limit is still named beside that refusal.
    for (name, cps) in [("fanuc", FANUC), ("haas", HAAS)] {
        let over = doc("mill-turn", Section::Work, 3000.0).expect("facing on a mill-turn");
        let why = post(&over, cps, name).err().unwrap_or_default();
        assert!(
            why.contains("K: turns the work at 3000 rpm (G97)") && why.contains("workholding maximum of 2000 rpm"),
            "{name}: facing at 3000 under a 2000 chuck is refused by name: {why}"
        );
        assert!(why.contains("is not a turning post") && why.contains("(K)"), "{name}: {why}");
        let at = doc("mill-turn", Section::Work, CHUCK).expect("facing on a mill-turn");
        let why = post(&at, cps, name).expect_err("a turning section through a milling post");
        assert!(why.contains("is not a turning post") && why.contains("(K)"), "{name}: refused by name: {why}");
        assert!(!why.contains("workholding maximum"), "{name}: at the limit the chuck is not the reason: {why}");
        // Through the turning post the same section posts at the limit.
        assert!(post(&at, LATHE, name).is_ok(), "{name}: a turning post serves it");
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
        // An undeclared script (#1199): on a machine with a work spindle
        // the door refuses every section but a C one by name first (a
        // turning section too: only a turning post may post it, and an
        // unstated script is not known to be one). Elsewhere a work
        // section is not creatable and a milling section is not judged.
        ("undeclared", _) => !chucked,
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
                // The creation refusals: turning work on a mill, milling
                // work on a lathe (the lathe door, #1190). Each is pinned
                // by its message; every other cell must be created.
                let refused = matches!((kind, section), ("none" | "mill", Section::Work) | ("lathe", Section::Tool));
                let w = match doc(kind, section, rpm) {
                    Ok(w) => {
                        assert!(!refused, "{kind} / {dialect} / {section:?}: must be refused at creation, was created");
                        w
                    }
                    Err(why) => {
                        assert!(refused, "{kind} / {dialect} / {section:?}: refused but should be creatable: {why}");
                        let (what, mount) = match section {
                            Section::Work => ("a Facing operation is turning work", "mount a lathe (or a mill-turn) first"),
                            Section::Tool => ("a Face operation is milling work", "mount a mill (or a mill-turn) first"),
                        };
                        assert!(
                            why.contains(what) && why.contains(mount),
                            "{kind} / {dialect} / {section:?}: refused for another reason: {why}"
                        );
                        rows.push(format!("{kind:9} {dialect:10} {section:?}: REFUSED at creation: {why}"));
                        continue;
                    }
                };
                let got = post(&w, cps, &format!("{kind}-{dialect}-{section:?}"));
                let by_limit = got.as_ref().err().is_some_and(|e| e.contains("workholding maximum"));
                let want = judged(kind, dialect, section);
                rows.push(format!("{kind:9} {dialect:10} {section:?}: {}", if by_limit { "judged" } else { "not judged" }));
                assert_eq!(by_limit, want, "{kind} / {dialect} / {section:?}: {got:?}");
                // What the undeclared cells on a chucked machine guard:
                // the door's refusal, naming the missing manifest line.
                if dialect == "undeclared" && (kind == "lathe" || kind == "mill-turn") {
                    let why = got.as_ref().err().map_or("", String::as_str);
                    assert!(why.contains("no `capabilities` line"), "{kind} / {section:?}: the door refuses: {got:?}");
                    assert!(why.contains("`capabilities = 2;`"), "{kind} / {section:?}: names the fix: {why}");
                }
                if let Ok(program) = &got {
                    assert!(!(section == Section::Work && program.contains("G97 S3000")), "work spindle over the chuck: {program}");
                }
            }
        }
    }
    println!("{}", rows.join("\n"));
}

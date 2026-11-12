//! The 3D Pocket under HSM Adaptive through the app (M11.4 owed
//! "Pocket3D port of the same planner"): the user's own part
//! (`testdata/models/Milling-test.step`, a D-shaped plate with a blind
//! bore) roughed through the commands with `strategy: Adaptive` - every
//! level planned by the 2D HSM planner inside the 3-D ladder's level
//! law - verifies clean against the part solid; a level the planner
//! refuses is said in the path's notes with its Z (the op's status
//! summary carries them), never silently swapped; the stamp tells
//! Adaptive from the rings (an old Adaptive op goes Stale once, an
//! OffsetRings op keeps its digest); and the default's output is
//! `golden_cl`'s business, unchanged.
//!
//! Gate map (M11.4): covers app::operation (params digest, generate's
//! Pocket3D arm), app::commands, sim::verify.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::Links;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::model;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";

fn pocket_op(strategy: PocketStrategy) -> OpParams {
    OpParams::Pocket3D {
        links: Links::ROUGH3D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
    }
}

/// The user's part in a 70 x 70 block from Z -7, the 10 mm tool, one op
/// generated through the commands: its path and its warnings.
fn user_job(params: OpParams) -> (World, Toolpath, Vec<String>) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Entity(name), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let warns = tp.warnings.iter().map(|w| format!("{w:?}")).collect();
    (world, tp.path.clone(), warns)
}

#[test]
#[ignore = "heavy lane: HSM Adaptive over every level of the user's STEP bore, verified against the part solid, ~220 s debug (release tier); the planner laws' debug twin is motion pocket3d_hsm::an_open_level_holds_the_planner_laws"]
fn the_users_bore_roughs_adaptive_level_by_level_and_verifies_clean() {
    let (_, path, warns) = user_job(pocket_op(PocketStrategy::Adaptive));
    assert!(warns.is_empty(), "every level verifies clean: {warns:?}\nnotes {:?}", path.notes);
    let planar = path.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).count();
    assert!(planar > 0, "the ladder cuts");
    // The HSM plan's arcs ride the levels (or the level says why not).
    let arcs = path.moves.iter().filter(|m| m.arc.is_some() && m.from.z == m.to.z).count();
    let fell: Vec<&String> = path.notes.iter().filter(|n| n.contains("Offset rings cut the level at Z")).collect();
    assert!(arcs > 0 || !fell.is_empty(), "{:?}", path.notes);
    // Never a silent switch: a rings level names its Z in the notes,
    // and the op's status line carries the notes.
    for n in &fell {
        assert!(n.contains("Z ") && n.starts_with("adaptive: the HSM plan"), "{n}");
        let line = path.status_summary("Op1", 0, camcraft_app::units::Unit::Millimeters);
        assert!(line.contains("Offset rings"), "{line}");
    }
    // Issue #850: the growth planner's budgets never refuse a level of
    // the bore (its finely tessellated outline is heavy geometry over a
    // short walk); only the levels whose plan would cross itself go to
    // the rings. A budget that cut a level would change the path silently
    // under the assertions above.
    assert!(!path.notes.iter().any(|n| n.contains("planning budget") || n.contains("too detailed")), "{:?}", path.notes);
    // No level falls back (issue #980 part 2). Since #850 the top level
    // (Z 20) fell back as "crosses itself", and since PR #839 (the 0.005
    // mm STEP chord) the Z 10 level too - both for one reason, not the
    // mesh: the entry orbit's closing vertex stood an ulp off its start
    // (`adaptive::rings::circle`), and the plan's crossing law read the
    // spiral leaving it as crossing the orbit's first chord. Which levels
    // the float noise hit was luck of the outline; #839 only reshuffled
    // it. The orbit now closes exactly on its start and every level keeps
    // its HSM plan.
    assert!(fell.is_empty(), "{fell:?}");
    // The path as the fix plans it - every level HSM (Z 20 back on its 58
    // microlift returns, so far more moves than its rings), its fitted
    // arcs back. Before: main f490458a 6500 / 6464 / 222 (Z 20 and Z 10
    // rings); 575f02d8^1 (before #839) 5530 / 5497 / 600 (Z 20 rings).
    // The planner's output differs slightly between platforms (floating
    // point differs in the last bits; making it bit-stable is #1010), so
    // the triple is exact PER MEASURED PLATFORM - a real change still
    // trips it on the release platform - and a band only where nobody has
    // measured yet. A change to the planner or the mesh that moves these
    // re-measures them and says why.
    println!("moves {} planar {planar} arcs {arcs} notes {:?}", path.moves.len(), path.notes);
    let got = (path.moves.len(), planar, arcs);
    assert!(bore_path_matches(got), "the bore's path as the fix plans it: {got:?} {:?}", path.notes);
}

/// The bore's (moves, planar feeds, arcs) on this platform (issue #980).
fn bore_path_matches(got: (usize, usize, usize)) -> bool {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        // Measured on issue/980-bore-hsm-plan, three release runs, identical.
        got == (16197, 15458, 1581)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        // Measured by the contributor's reviewer at 608126a5 on Apple
        // silicon (https://github.com/NeoTech/Camcraft/pull/1032).
        got == (16160, 15432, 1571)
    } else {
        // Not measured here yet: measure this platform and add its exact
        // triple above. Until then a band covering both measured ones.
        let (moves, planar, arcs) = got;
        (16100..=16260).contains(&moves) && (15380..=15520).contains(&planar) && (1560..=1600).contains(&arcs)
    }
}

#[test]
fn the_stamp_tells_the_hsm_plan_from_the_rings() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let stamp = |s: PocketStrategy| operation::current_stamp(&world, TOOL, &SrcRef::Stock, &pocket_op(s), "X", None);
    let hsm = stamp(PocketStrategy::Adaptive);
    assert!(hsm.is_some());
    assert_ne!(hsm, stamp(PocketStrategy::OffsetRings), "the HSM plan is a new generation of the word");
    assert_ne!(hsm, stamp(PocketStrategy::Waterline));
}

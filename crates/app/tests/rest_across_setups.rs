//! #318: rest machining across setups. A rest pass in a later setup the
//! machined body is carried into (#317) reads what the earlier setups cut:
//! it skips the material Setup1 removed, its stamp moves when anything
//! ahead in Setup1 (or the carry itself) changes, a suppressed Setup1 cut
//! refuses it by name at the post, and a carry the lattice refuses refuses
//! the rest reading by name instead of falling back to a fresh block.
//! Provenance: our own fixtures (remachining.rs's plate with a pocket)
//! over plain re-clamp practice; no vendor material consulted.
//!
//! Gate map (#318): covers app::rest (prior_state, prior_digest,
//! prior_stale, suppressed_cuts_ahead, carry_refusal, rest::across),
//! app::operation (job, stamp), app::posting_suppressed,
//! app::commands (SetSetupRelative, AlignPartToSetup, EditOperationParams,
//! SetSetupStockSource - #320's pick decides the chain).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History,
    ImportModel, SetSetupRelative, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::rest::REST_CARRY_REFUSAL;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A 40x40x20 plate with a 20x20 pocket sunk to z=10.
fn plate_with_pocket() -> Mesh {
    let mut facets = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    facets.extend(outer.facets[0..2].iter().cloned());
    facets.extend(outer.facets[4..12].iter().cloned());
    let inner = cube(Point3::new(-10.0, -10.0, 10.0), Point3::new(10.0, 10.0, 20.0));
    facets.extend(inner.facets[0..2].iter().cloned());
    facets.extend(inner.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    for k in 0..4 {
        let (a, b, c, d) = (o[k], o[(k + 1) % 4], i[(k + 1) % 4], i[k]);
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(facets)
}

fn pocket(bottom_z: f64, stepover: f64, rest_rough: bool) -> OpParams {
    OpParams::Pocket3D {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::ROUGH3D,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom_z),
        stepdown: 5.0,
        feed: 1500.0,
        plunge_feed: 400.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough,
        optimal_load: camcraft_app::load::load_of_degrees(60.0),
        feed_opt: false,
        shell: false,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        picks: Default::default(),
    }
}

fn add(world: &World, h: &mut History, name: &str, tool: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Entity("plate".into()), params);
    h.execute(world, cmd.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// How Setup2 is entered from Setup1.
#[derive(Clone, Copy, PartialEq)]
enum Second {
    /// No Setup2: both ops in Setup1 (the single-setup reference).
    None,
    /// Setup2 from Setup1's datum, not linked: the M3.2 A clip.
    Clip,
    /// Setup2 placed "Same as" Setup1: the body is carried.
    SameAs,
}

/// The plate in the stock, Setup1 roughing with the 8 mm tool, then the
/// 4 mm rest pass - in Setup1 or in Setup2 as `second` says.
fn program(second: Second) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    for t in ["EM-0800-4FL", "EM-0400-2FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        h.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("box stock"));
    h.execute(&world, ImportModel::from_mesh("plate", plate_with_pocket()));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add(&world, &mut h, "Rough1", "EM-0800-4FL", pocket(12.0, 4.0, false));
    if second != Second::None {
        h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    }
    if second == Second::SameAs {
        h.execute(&world, SetSetupRelative::link(&world, "Setup2", "Setup1", [0.0; 3]).expect("same as"));
    }
    add(&world, &mut h, "Rest2", "EM-0400-2FL", pocket(10.0, 2.0, true));
    (world, h)
}

fn path(world: &World, name: &str) -> Vec<camcraft_app::cl::Move> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e)).expect("cached").path.moves
}

fn setup_of(world: &World, name: &str) -> Option<String> {
    model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).and_then(|o| o.setup)
}

/// The cutting length below the stock top (Z < 20): what the pass spends
/// in material it believes stands.
fn cut_length(world: &World, name: &str) -> f64 {
    path(world, name)
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.from.z.max(m.to.z) < 20.0 - 1e-6)
        .map(|m| ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2) + (m.to.z - m.from.z).powi(2)).sqrt())
        .sum()
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

#[test]
fn a_second_setup_rest_pass_skips_what_the_first_setup_cleared() {
    let (carried, _h) = program(Second::SameAs);
    assert_eq!(setup_of(&carried, "Rest2").as_deref(), Some("Setup2"));
    assert_eq!(setup_of(&carried, "Rough1").as_deref(), Some("Setup1"));
    let (one, _h1) = program(Second::None);
    let (clip, _h2) = program(Second::Clip);
    // Carried "Same as" Setup1: Setup2's rest pass is the very pass the
    // same op would get in Setup1 - it reads Rough1's cut.
    assert_eq!(path(&carried, "Rest2"), path(&one, "Rest2"), "the carried rest equals the same-setup rest");
    // Entered by the clip (not linked) it starts from its own block, as
    // before #318, and re-cuts the pocket Rough1 cleared.
    let (c, f) = (cut_length(&carried, "Rest2"), cut_length(&clip, "Rest2"));
    assert!(c + 1.0 < f, "the carried rest cuts less: {c} vs fresh-block {f}");
    assert!(posting::verification_report(&carried).problems.is_empty(), "{:?}", posting::verification_report(&carried).problems);
}

#[test]
fn a_change_ahead_in_the_first_setup_stales_the_second_setups_rest_and_undo_restores_it() {
    let (world, mut h) = program(Second::SameAs);
    assert_eq!(status(&world, "Rest2"), OpStatus::Ok);

    // Re-rough Setup1 to another depth: Rest2's rest digest moves.
    let edit = EditOperationParams::new(&world, "Rough1", pocket(14.0, 4.0, false)).expect("edit");
    h.execute(&world, edit);
    h.execute(&world, GenerateOperation::new(&world, "Rough1").expect("regenerate"));
    assert_eq!(status(&world, "Rough1"), OpStatus::Ok);
    assert_eq!(status(&world, "Rest2"), OpStatus::Stale, "Setup1's new path stales Setup2's rest");
    let why = operation::stale_reason(&world, &model::find_by_name(&world, "Rest2").expect("Rest2"));
    assert!(why.as_ref().is_some_and(|w| w.contains("'Rough1'") && w.contains("setup")), "{why:?}");
    assert!(h.undo(&world) && h.undo(&world));
    assert_eq!(status(&world, "Rest2"), OpStatus::Ok, "undo restores the path Rest2 was generated against");

    // Unlinking Setup2 ends the carry: Rest2 now reads its own block.
    h.execute(&world, SetSetupRelative::unlink(&world, "Setup2").expect("unlink"));
    assert_eq!(status(&world, "Rest2"), OpStatus::Stale, "the carry itself is an input");
    assert!(h.undo(&world));
    assert_eq!(status(&world, "Rest2"), OpStatus::Ok);
}

#[test]
fn a_suppressed_first_setup_cut_refuses_the_second_setups_rest_by_name() {
    let (world, mut h) = program(Second::SameAs);
    assert!(posting::verification_report(&world).problems.is_empty());
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Rough1", true).expect("suppress"));
    let want = "operation Rest2 was generated against the cut of suppressed Rough1; unsuppress Rough1 or suppress Rest2 too";
    let problems = posting::verification_report(&world).problems;
    assert!(problems.iter().any(|p| p == want), "{problems:?}");
}

#[test]
fn a_clip_entered_setup_does_not_read_the_first_setups_cuts() {
    // Not carried: suppressing Rough1 leaves Setup2's rest posting, and
    // the rest stamp does not read Setup1 (the pre-#318 law, unchanged).
    let (world, mut h) = program(Second::Clip);
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Rough1", true).expect("suppress"));
    let problems = posting::verification_report(&world).problems;
    assert!(!problems.iter().any(|p| p.contains("generated against")), "{problems:?}");
    assert_eq!(status(&world, "Rest2"), OpStatus::Ok);
}

#[test]
fn a_carry_the_lattice_refuses_refuses_the_rest_reading_by_name() {
    // Face Setup1, then turn Setup2 30 degrees about Z: the body cannot be
    // carried exactly, so a Setup2 op that reads the program ahead is
    // refused by name - never generated against a fresh block.
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    h.execute(&world, ImportModel::from_mesh("plate", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let face = |name: &str| {
        let p = OpParams::Face {
            holes: Default::default(),
            tilt: None,
            blend_turns: false, blend_radius: None,
            stepover: 3.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(16.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        };
        CreateOperation::new(&world, name, "EM-0400-2FL", SrcRef::Entity("plate".into()), p).expect("create")
    };
    h.execute(&world, face("Face1"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup2", [0.0, 0.0, 30.0]).expect("30 degrees"));
    h.execute(&world, face("Face2"));
    let why = camcraft_app::rest::carry_refusal(&world, "Face2", Some("Setup2")).expect("refused");
    assert!(why.starts_with(REST_CARRY_REFUSAL) && why.contains("from setup 'Setup1' into setup 'Setup2'"), "{why}");
    let err = GenerateOperation::new(&world, "Face2").err().expect("generation refused").to_string();
    assert!(err.contains("Face2") && err.contains("from setup 'Setup1' into setup 'Setup2'"), "{err}");
    assert!(err.contains(REST_CARRY_REFUSAL), "{err}");
}

/// #320: pick Setup2's stock source and regenerate its rest pass.
fn pick(world: &World, h: &mut History, source: camcraft_app::setup_stock::StockSource) {
    let cmd = camcraft_app::commands::SetSetupStockSource::new(world, "Setup2", source).expect("pick");
    h.execute(world, cmd);
    h.execute(world, GenerateOperation::new(world, "Rest2").expect("regenerate"));
}

/// Face1 in Setup1, then Setup2 from its datum holding the plate turned
/// about Z by `deg`, with Face2 created there (not generated).
fn faced_then_turned(deg: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    h.execute(&world, ImportModel::from_mesh("plate", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let face = |name: &str| {
        let p = OpParams::Face {
            holes: Default::default(),
            tilt: None,
            blend_turns: false, blend_radius: None,
            stepover: 3.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(16.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        };
        CreateOperation::new(&world, name, "EM-0400-2FL", SrcRef::Entity("plate".into()), p).expect("create")
    };
    h.execute(&world, face("Face1"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup2", [0.0, 0.0, deg]).expect("turn"));
    h.execute(&world, face("Face2"));
    (world, h)
}

#[test]
fn a_turned_setup_picked_its_own_stock_does_not_read_the_first_setups_cuts() {
    use camcraft_app::commands::SetSetupStockSource;
    use camcraft_app::setup_stock::StockSource;
    // Turned 30 degrees: unpicked, the chain must carry and refuses.
    // "Its own stock" is a new block: the chain stops at Setup2, so there
    // is nothing to carry and nothing to refuse.
    let (world, mut h) = faced_then_turned(30.0);
    assert!(camcraft_app::rest::carry_refusal(&world, "Face2", Some("Setup2")).is_some());
    h.execute(&world, SetSetupStockSource::new(&world, "Setup2", StockSource::Own).expect("own"));
    assert_eq!(camcraft_app::rest::carry_refusal(&world, "Face2", Some("Setup2")), None, "a new block reads no carry");
    h.execute(&world, GenerateOperation::new(&world, "Face2").expect("generated on its own block"));

    // Turned 180 degrees (a carry the lattice holds): unpicked, Face2
    // reads Face1's cut, so suppressing Face1 refuses the post by name;
    // picked "Its own stock", it does not read Setup1 at all.
    let suppressed_reader = |world: &World| {
        posting::verification_report(world).problems.iter().any(|p| p.contains("generated against the cut of suppressed Face1"))
    };
    let (world, mut h) = faced_then_turned(180.0);
    h.execute(&world, GenerateOperation::new(&world, "Face2").expect("generate"));
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress"));
    assert!(suppressed_reader(&world), "carried: Face2 read Face1's cut");
    assert!(h.undo(&world));
    h.execute(&world, SetSetupStockSource::new(&world, "Setup2", StockSource::Own).expect("own"));
    h.execute(&world, GenerateOperation::new(&world, "Face2").expect("regenerate"));
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress"));
    assert!(!suppressed_reader(&world), "a new block never read Face1's cut");
}

#[test]
fn a_same_held_setup_picked_what_setup1_leaves_reads_its_cuts() {
    use camcraft_app::setup_stock::StockSource;
    // Not linked, same hold: unpicked it is the clip (own block); the
    // explicit pick carries the body, in rest as in the sim.
    let (world, mut h) = program(Second::Clip);
    let (one, _h1) = program(Second::None);
    pick(&world, &mut h, StockSource::FromPrevious);
    assert_eq!(path(&world, "Rest2"), path(&one, "Rest2"), "the picked carry equals the same-setup rest");
    assert_eq!(status(&world, "Rest2"), OpStatus::Ok);
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Rough1", true).expect("suppress"));
    let want = "operation Rest2 was generated against the cut of suppressed Rough1; unsuppress Rough1 or suppress Rest2 too";
    let problems = posting::verification_report(&world).problems;
    assert!(problems.iter().any(|p| p == want), "{problems:?}");
}

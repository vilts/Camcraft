//! #320: a setup's stock source - its own stock, or what the setup
//! before it leaves behind. Unpicked, it reads #317's implicit rule, so
//! an untouched document simulates bit for bit as before. A pick changes
//! where the sim starts, marks the setup's operations Stale, undoes, and
//! round-trips; a carry the sim refuses cannot be picked; a new block
//! after a flip still reports a gouge cut before it; the setup sheet
//! states the source.
//! Provenance: our own fixtures (a faced block) over plain re-clamp
//! practice; no vendor material consulted.
//!
//! Gate map (#320): covers app::setup_stock, app::commands
//! (SetSetupStockSource), app::stock_sim, sim::stock_sim (simulate_document, carry,
//! incoming_body), app::operation (stamp), app::project (stock_source),
//! app::setup_sheet (STOCK source).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetSetupRelative, SetSetupStockSource, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::setup_stock::{self, StockSource};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{CARRY_TURN_REFUSAL, DEV_TOL, ProgramSim, incoming_body, simulate_document};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A 60 x 60 x 16 part in a stock Z -2..18, Setup1 faced to `cut1`, and
/// Setup2 from Setup1's datum (holding the part alike until turned).
fn document(cut1: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add_face(&world, &mut h, "Face1", cut1);
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    (world, h)
}

fn add_face(world: &World, h: &mut History, name: &str, cut: f64) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face(cut)).expect("create");
    h.execute(world, op);
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn pick(world: &World, h: &mut History, source: StockSource) {
    h.execute(world, SetSetupStockSource::new(world, "Setup2", source).expect("pick"));
}

/// Regenerate Setup2's operation after a pick moved its stamp (a stale
/// operation is refused by the program sim).
fn regen(world: &World, h: &mut History) {
    if camcraft_app::model::find_by_name(world, "Face2").is_some() {
        h.execute(world, GenerateOperation::new(world, "Face2").expect("regenerate"));
    }
}

fn bits(s: &ProgramSim) -> Vec<u64> {
    s.dev.dev.iter().map(|v| v.to_bits()).collect()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn fresh(world: &World, op: &str) -> bool {
    let e = camcraft_app::model::find_by_name(world, op).expect("op");
    camcraft_app::operation::status_of(world, &e) == camcraft_app::operation::OpStatus::Ok
}

#[test]
fn unpicked_setups_read_the_317_rule_and_simulate_unchanged() {
    // Same hold, unlinked: its own stock (the clip).
    let (world, mut h) = document(16.0);
    add_face(&world, &mut h, "Face2", 16.0);
    assert_eq!(setup_stock::stored(&world, "Setup2"), None);
    assert_eq!(setup_stock::effective(&world, "Setup2"), Some(StockSource::Own));
    assert_eq!(setup_stock::effective(&world, "Setup1"), None, "the first setup has no choice");
    let implicit = simulate_document(&world).expect("judged");
    // Picking what Setup1 leaves and undoing it lands back bit for bit.
    pick(&world, &mut h, StockSource::FromPrevious);
    h.undo(&world);
    assert_eq!(setup_stock::stored(&world, "Setup2"), None, "undo restores 'never picked'");
    assert!(fresh(&world, "Face2"), "undo restores the stamp");
    assert_eq!(bits(&simulate_document(&world).expect("judged")), bits(&implicit));

    // Linked "Same as", or flipped: what Setup1 leaves behind.
    let link = SetSetupRelative::link(&world, "Setup2", "Setup1", [0.0; 3]).expect("link");
    h.execute(&world, link);
    regen(&world, &mut h);
    assert_eq!(setup_stock::effective(&world, "Setup2"), Some(StockSource::FromPrevious));
    let linked = simulate_document(&world).expect("carried");
    pick(&world, &mut h, StockSource::Own);
    pick(&world, &mut h, StockSource::FromPrevious);
    regen(&world, &mut h);
    assert_eq!(bits(&simulate_document(&world).expect("carried")), bits(&linked), "explicit equals implicit");

    let (world, mut h) = document(16.0);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    assert_eq!(setup_stock::effective(&world, "Setup2"), Some(StockSource::FromPrevious));
    assert_eq!(setup_stock::describe(&world, "Setup2").as_deref(), Some("What Setup1 leaves behind"));
}

#[test]
fn a_new_block_after_a_flip_starts_from_its_own_stock_and_undoes() {
    // The verdict reads each column's top against the part, so the two
    // starts must differ from above: Setup2's own block stands 4 mm
    // taller (top 22) than the 18 mm Setup1 leaves (#415), and Face2
    // stops at 19, above the part - 2 mm rest on the carried body (it
    // cuts air), 3 mm on the new block. Faced down to the part, both
    // starts finish to the same exact top and no verdict could tell.
    let (world, mut h) = document(16.0);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate Setup2"));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 24.0 }, at).expect("Setup2's stock"));
    add_face(&world, &mut h, "Face2", 19.0);
    let carried = simulate_document(&world).expect("carried");
    let before = incoming_body(&world, "Setup2").expect("carried body");
    assert!(near(before.volume(), 60.0 * 60.0 * 18.0, 1e-6), "{}", before.volume());
    assert!(near(carried.dev.max_rest, 2.0, 1e-6), "the carried top 18 over the part: {}", carried.dev.max_rest);
    assert!(fresh(&world, "Face2"));

    pick(&world, &mut h, StockSource::Own);
    assert!(!fresh(&world, "Face2"), "the pick is a moved input of the setup's operations");
    assert!(fresh(&world, "Face1"), "the first setup's operations do not move");
    regen(&world, &mut h);
    let block = incoming_body(&world, "Setup2").expect("a new block");
    let stock2 = camcraft_app::stock_sim::stock_grid(&world, Some("Setup2"), 2.0).expect("Setup2's stock");
    let (lo, hi) = block.bounds().expect("material");
    assert!(near(hi.z, stock2.stock_z, 1e-9), "Setup2's own block top: {lo:?} {hi:?}");
    assert!(near(block.volume(), 60.0 * 60.0 * 24.0, 1e-6), "the whole block: {}", block.volume());
    let new_block = simulate_document(&world).expect("judged on a new block");
    assert!(near(new_block.dev.max_rest, 3.0, 1e-6), "Face2 at 19 over the part: {}", new_block.dev.max_rest);
    assert_ne!(bits(&new_block), bits(&carried), "the sim starts elsewhere");

    h.undo(&world); // the regeneration
    h.undo(&world); // the pick
    assert_eq!(setup_stock::stored(&world, "Setup2"), None);
    assert!(fresh(&world, "Face2"), "undo restores the stamp");
    assert_eq!(bits(&simulate_document(&world).expect("carried")), bits(&carried));
    h.redo(&world);
    assert_eq!(setup_stock::stored(&world, "Setup2"), Some(StockSource::Own));
    assert!(!fresh(&world, "Face2"), "redo moves the stamp again");
}

#[test]
fn a_gouge_before_a_new_block_still_reads_in_the_verdict() {
    // Setup1 faces 1 mm into the part, Setup2 flips onto a new block.
    let (world, mut h) = document(15.0);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    add_face(&world, &mut h, "Face2", 16.0);
    pick(&world, &mut h, StockSource::Own);
    regen(&world, &mut h);
    let sim = simulate_document(&world).expect("judged");
    assert!(near(sim.dev.max_gouge, 1.0, 1e-6), "the left block's gouge folds in: {}", sim.dev.max_gouge);
    assert!(sim.dev.gouge_cells > 0 && sim.dev.max_gouge > DEV_TOL);
}

#[test]
fn a_refused_carry_and_the_first_setup_cannot_be_picked() {
    let (world, mut h) = document(16.0);
    let skew = AlignPartToSetup::angles(&world, "Setup2", [0.0, 0.0, 30.0]).expect("30 degrees about Z");
    h.execute(&world, skew);
    let why = setup_stock::carry_refusal(&world, "Setup2").expect("refused");
    assert!(why.contains("from setup 'Setup1' into setup 'Setup2'") && why.contains(CARRY_TURN_REFUSAL), "{why}");
    let err = SetSetupStockSource::new(&world, "Setup2", StockSource::FromPrevious).err().expect("refused");
    assert!(err.to_string().contains(CARRY_TURN_REFUSAL), "{err}");
    // Its own stock is a new block, so the program simulates again.
    let op = CreateOperation::new(&world, "Face2", TOOL, SrcRef::Entity("part".into()), face(16.0)).expect("create");
    h.execute(&world, op);
    // Unpicked, the carry is refused by generation (#318's rest chain)
    // and by the program sim (#317), both by name.
    let err = GenerateOperation::new(&world, "Face2").err().expect("refused").to_string();
    assert!(err.contains(CARRY_TURN_REFUSAL), "{err}");
    assert!(simulate_document(&world).is_err(), "unpicked: the #317 carry, refused");
    pick(&world, &mut h, StockSource::Own);
    regen(&world, &mut h);
    simulate_document(&world).expect("a new block is judged");

    let err = SetSetupStockSource::new(&world, "Setup1", StockSource::Own).err().expect("first");
    assert!(err.to_string().contains("first setup"), "{err}");
    assert!(setup_stock::carry_refusal(&world, "Setup1").is_some_and(|w| w.contains("first setup")));
    let err = SetSetupStockSource::new(&world, "Setup2", StockSource::Own).err().expect("no change");
    assert!(err.to_string().contains("already"), "{err}");
    assert!(SetSetupStockSource::new(&world, "Nope", StockSource::Own).is_err());
    assert!(SetSetupStockSource::new(&world, "part", StockSource::Own).is_err(), "a model is no setup");
}

#[test]
fn the_pick_round_trips_and_older_or_broken_files_read_by_name() {
    let (world, mut h) = document(16.0);
    pick(&world, &mut h, StockSource::FromPrevious);
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-stock-source-{}.ccam", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"stock_source\": \"from_previous\""), "{first}");
    // The stock source arrived at v57 (#320); pin the floor so a later
    // rung does not break this round trip (cf. #344).
    const _: () = assert!(project::PROJECT_VERSION >= 57, "stock sources need project v57 or later");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(setup_stock::stored(&world2, "Setup2"), Some(StockSource::FromPrevious));
    project::save(&world2, &path).expect("resave");
    assert_eq!(first, std::fs::read_to_string(&path).expect("reread"), "byte-identical round trip");
    let _ = std::fs::remove_file(&path);

    let old = r#"{"version": 56, "entities": [
        {"name": "Setup1", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Stock", "wcs": "G54"}},
        {"name": "Setup2", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Setup2/Stock", "wcs": "Setup2/WCS"}}]}"#;
    let w = new_world();
    project::open(&w, old).expect("a v56 file reads");
    assert_eq!(setup_stock::stored(&w, "Setup2"), None, "absent: never picked");
    let now = old.replace("\"version\": 56", "\"version\": 57");
    let bad = now.replace("\"wcs\": \"Setup2/WCS\"", "\"wcs\": \"Setup2/WCS\", \"stock_source\": \"borrowed\"");
    let err = project::open(&new_world(), &bad).expect_err("an unknown word");
    assert!(err.to_string().contains("'borrowed', which is no stock source"), "{err}");
}

#[test]
fn the_setup_sheet_states_the_stock_source() {
    let (world, mut h) = document(16.0);
    add_face(&world, &mut h, "Face2", 16.0);
    let sheet = |w: &World, s: &str| camcraft_app::setup_sheet::render(w, Some(s)).expect("sheet");
    assert!(sheet(&world, "Setup2").contains("  source   Its own stock\n"));
    assert!(!sheet(&world, "Setup1").contains("  source "), "the first setup prints no source");
    pick(&world, &mut h, StockSource::FromPrevious);
    regen(&world, &mut h);
    let text = sheet(&world, "Setup2");
    assert!(text.contains("  source   What Setup1 leaves behind\n"), "{text}");
    assert!(text.contains("max (30.000, 30.000, 16.000)  volume 64800.000 mm3"), "{text}");
}

/// #866: a plate held the same way in both setups carries, even when
/// both hold it turned side-on - the carry reads the turn BETWEEN the
/// setups, never either setup's own turn from as modelled.
#[test]
fn both_setups_turned_alike_carry_through_the_identity() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", cube(Point3::new(-40.0, -30.0, 0.0), Point3::new(40.0, 30.0, 8.0))));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup1", [-90.0, 0.0, 0.0]).expect("stand it on its edge"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2 from Setup1's datum"));
    assert_eq!(setup_stock::carry_refusal(&world, "Setup2"), None, "the same hold carries");
    h.execute(&world, SetSetupStockSource::new(&world, "Setup2", StockSource::FromPrevious).expect("pickable"));
}

/// #866: a genuinely side-on turn between the setups is refused, and the
/// refusal says which turn, between which setups, in the row's words.
#[test]
fn a_side_on_turn_between_setups_is_refused_naming_the_turn() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", cube(Point3::new(-40.0, -30.0, 0.0), Point3::new(40.0, 30.0, 8.0))));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup2", [-90.0, 0.0, 0.0]).expect("stand it on its edge"));
    let why = setup_stock::carry_refusal(&world, "Setup2").expect("refused");
    assert!(why.contains("from setup 'Setup1' into setup 'Setup2'"), "{why}");
    assert!(why.contains("'Setup2' holds the part turned X -90°, Y 0°, Z 0° from how 'Setup1' holds it"), "{why}");
    assert!(why.contains(CARRY_TURN_REFUSAL), "the named refusal stays: {why}");
    assert!(SetSetupStockSource::new(&world, "Setup2", StockSource::FromPrevious).is_err());
}

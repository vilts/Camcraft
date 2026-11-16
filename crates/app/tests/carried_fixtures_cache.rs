//! #321: carried stock - workholding, the lathe second op, the cache. A
//! 10x10x5 part sits in a 100 mm block Setup1 faces; Setup2 holds the body
//! Setup1 left ("Same as") and carries a jaw fixture beside the part. The
//! jaw is refused at the post where Setup1 left material standing in it,
//! and clear where Setup1 faced that material away (even though Setup2's
//! declared block still reaches through it); a carry on a lathe or
//! mill-turn is refused by name (generation and the rest chain name it,
//! the sim and the post refuse the path it staled); the carried body is not
//! recomputed on an unrelated edit, and is on an upstream one.
//! Provenance: our own fixtures (carried_verify.rs's faced block and
//! contour) over plain re-clamp practice; no vendor material consulted.
//!
//! Gate map (#321): covers app::fixture_carried (carried_fixture_problems,
//! penetration), app::rest::carried_cache (body, computes, clear),
//! app::rest::across (carried_body, entered_body), app::stock_sim, sim::stock_sim::carry
//! (CARRY_MACHINE_REFUSAL), app::posting (the workholding clause).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History, ImportModel,
    LoadMachine, RenameOperation, SetSetupRelative, SetStock, TagFixture,
};
use camcraft_app::fixture_carried::{FIXTURE_IN_CARRIED, carried_fixture_problems};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::rest::{carried_body_computes, carry_refusal};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::machine::{Kind, parse_mch};
use camcraft_app::stock_sim::{CARRY_MACHINE_REFUSAL, carry_machine_refusal, simulate_document};
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";

fn contour(feed: f64) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(12.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn block(world: &World, w: f64, z0: f64, dz: f64) -> SetStock {
    let at = Location { pos: Point3::new(5.0, 5.0, z0), ..Default::default() };
    SetStock::new(world, StockShape::Box { dx: w, dy: w, dz }, at).expect("stock")
}

/// The part in Setup1's 100 mm block Z -5..10 faced to `cut1`; Setup2
/// "Same as" Setup1 declaring a `w2`-wide block Z -5..(-5 + `dz2`), a
/// jaw (x 20..30, y 0..10, z 5..8) tagged to it, and Contour2.
fn program(cut1: f64, w2: f64, dz2: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    h.execute(&world, block(&world, 100.0, -5.0, 15.0));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face(cut1)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, SetSetupRelative::link(&world, "Setup2", "Setup1", [0.0; 3]).expect("same as"));
    h.execute(&world, block(&world, w2, -5.0, dz2));
    h.execute(&world, ImportModel::from_mesh("Jaw", cube(Point3::new(20.0, 0.0, 5.0), Point3::new(30.0, 10.0, 8.0))));
    h.execute(&world, TagFixture::new(&world, "Jaw", Some("Setup2")).expect("tag jaw"));
    let op = CreateOperation::new(&world, "Contour2", TOOL, SrcRef::Entity("part".into()), contour(800.0)).expect("create");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("generate"));
    (world, h)
}

fn jaw_lines(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|p| p.contains(FIXTURE_IN_CARRIED)).collect()
}

#[test]
fn a_jaw_modelled_into_material_setup1_left_refuses_the_post() {
    // Setup1 faces only to 9: material stands to Z 9 under the jaw (Z 5..8),
    // all of it outside Setup2's small declared block - only the carried
    // stock says the jaw is modelled into material.
    let (world, _h) = program(9.0, 12.0, 10.0);
    let p = carried_fixture_problems(&world);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(
        p[0].contains("setup 'Setup2'") && p[0].contains("fixture 'Jaw'") && p[0].contains("setup 'Setup1'") && p[0].contains("3.00 mm deep"),
        "{}",
        p[0]
    );
    assert_eq!(jaw_lines(&world), p, "the post gate reads the same clause");
    let dir = std::env::temp_dir().join(format!("cc-321-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let posted = posting::post_setup(&world, &dir.join("jaw.nc"));
    let _ = std::fs::remove_dir_all(&dir);
    match posted {
        Ok(_) => panic!("a jaw inside the carried stock posted"),
        Err(e) => assert!(e.contains(FIXTURE_IN_CARRIED), "{e}"),
    }
}

#[test]
fn a_jaw_where_setup1_faced_the_material_away_is_clear() {
    // Setup1 faces to 5: the jaw sits ON the face it left (contact, not
    // penetration). Setup2's declared block (to Z 10) still reaches through
    // the jaw - the carried body, not the declared block, is read.
    let (world, _h) = program(5.0, 100.0, 15.0);
    assert!(carried_fixture_problems(&world).is_empty(), "{:?}", carried_fixture_problems(&world));
    assert!(jaw_lines(&world).is_empty());
}

#[test]
fn the_carried_body_is_not_recomputed_on_an_unrelated_edit() {
    let (world, mut h) = program(9.0, 100.0, 10.0);
    let n0 = carried_body_computes();
    assert!(n0 >= 1, "Contour2's generation read the carried body");
    // Edits that do not feed the carry: the consumer's own feed, a label
    // upstream, the post gate reading the same entry - all hits.
    h.execute(&world, EditOperationParams::new(&world, "Contour2", contour(600.0)).expect("edit"));
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("regenerate"));
    h.execute(&world, RenameOperation::new(&world, "Face1", "Top face").expect("rename"));
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("regenerate"));
    let _ = posting::verification_report(&world);
    assert_eq!(carried_body_computes(), n0, "an unrelated edit recomputed the carried body");
    // An upstream edit moves the key: recomputed, and never served stale -
    // Setup1 now faces to 5, so the dive beside the part meets air.
    h.execute(&world, EditOperationParams::new(&world, "Face1", face(5.0)).expect("edit face"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate face"));
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("regenerate"));
    assert!(carried_body_computes() > n0, "an upstream edit served the cached body");
    let cached = model::find_by_name(&world, "Contour2").and_then(|e| model::toolpath_of(&world, &e)).expect("cached");
    assert!(cached.path.moves.iter().any(|m| m.kind == MoveKind::Rapid), "the contour has rapids");
    assert!(
        !cached.warnings.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "the body Setup1 faced to 5 was judged: {:?}",
        cached.warnings
    );
}

#[test]
fn a_new_document_starts_with_no_carried_body_cached() {
    let (world, mut h) = program(9.0, 100.0, 10.0);
    drop(new_world());
    let n0 = carried_body_computes();
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("regenerate"));
    assert!(carried_body_computes() > n0, "the cache outlived its document");
}

#[test]
fn a_carry_on_a_lathe_or_mill_turn_is_refused_by_name_on_every_path() {
    for (machine, name) in [("Lathe", "lathe"), ("Mill-turn", "mill-turn")] {
        let (world, mut h) = program(9.0, 100.0, 10.0);
        h.execute(&world, LoadMachine::builtin(&world, machine).expect("load machine"));
        let generated = GenerateOperation::new(&world, "Contour2").map(|_| ()).expect_err("refused").to_string();
        assert!(
            generated.contains(CARRY_MACHINE_REFUSAL)
                && generated.contains(&format!("on a {name} machine"))
                && generated.contains("from setup 'Setup1' into setup 'Setup2'"),
            "{machine}: {generated}"
        );
        // The rest chain names it too (the stamp digests that refusal, so
        // the path cut on the mill goes stale).
        let rest = carry_refusal(&world, "Contour2", Some("Setup2")).expect("the rest chain refuses");
        assert!(rest.contains(CARRY_MACHINE_REFUSAL), "{machine}: {rest}");
        // The sim never judges it: the op cut on the mill is stale by name.
        let sim = simulate_document(&world).map(|_| ()).expect_err("the program sim never judges it");
        assert!(sim.contains("Contour2") && sim.contains("regenerate"), "{machine}: {sim}");
        let gate = posting::verification_report(&world).problems;
        assert!(gate.iter().any(|p| p.contains("Contour2")), "{machine}: the carried program posted: {gate:?}");
    }
}

#[test]
fn a_carry_onto_a_sinker_edm_names_the_sinker_edm_and_a_mill_still_carries() {
    let (world, mut h) = program(9.0, 100.0, 10.0);
    // The mill carries: nothing is refused on it.
    assert!(carry_refusal(&world, "Contour2", Some("Setup2")).is_none(), "a mill carries");
    let m = parse_mch(include_str!("edm/sinker-edm.mch")).expect("parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    let rest = carry_refusal(&world, "Contour2", Some("Setup2")).expect("the sinker refuses");
    assert!(rest.contains(&carry_machine_refusal(Kind::SinkerEdm)) && rest.contains("sinker EDM"), "{rest}");
    assert!(!rest.contains("lathe"), "{rest}");
}

#[test]
fn a_fixture_narrower_than_a_cell_is_judged_by_the_column_under_it() {
    // A 0.1 mm dowel pin (no column centre inside its footprint) standing
    // Z 2..8 in material Setup1 left to Z 5: 3 mm deep, never skipped.
    let (world, mut h) = program(5.0, 100.0, 15.0);
    h.execute(&world, ImportModel::from_mesh("Pin", cube(Point3::new(40.0, 5.0, 2.0), Point3::new(40.1, 5.1, 8.0))));
    h.execute(&world, TagFixture::new(&world, "Pin", Some("Setup2")).expect("tag pin"));
    let p = carried_fixture_problems(&world);
    assert!(p.len() == 1 && p[0].contains("fixture 'Pin'") && p[0].contains("3.00 mm deep"), "{p:?}");
}

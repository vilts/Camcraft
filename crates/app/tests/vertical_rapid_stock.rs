//! #96 at the app level: a mill op's vertical rapid down into declared
//! stock is a `RapidInStock` at generation and the post gate refuses
//! it by name; the same op over stock standing to its top is silent.
//! The turned family's axial rapids keep the reading they had before
//! (the column cannot see the meridian - `operation::turning_rapids`);
//! the turning_bore, turning_contour and lathe_post suites pin that.
//!
//! Provenance: our own fixtures (the posting suite's 10 x 10 x 5 box
//! under a contour), specced from the Prime Directive and the shop rule
//! "never traverse through uncut stock"; no vendor material.
//!
//! Gate map (#96): covers camcraft_app::operation (generate's verify
//! wiring, turning_rapids, job's prior plan), camcraft_app::rest
//! (PriorPlan), camcraft_app::posting (the gate),
//! camcraft_sim::verify (rapids, prior), camcraft_app::operation::stamp
//! (#109: the program ahead in the stamp).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, RegenerateAll, ReorderOperation, SetStock,
};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

/// A contour of the 5 mm box: top 5, clearance 10, retract 8 - the
/// plunge-height rapid lands at Z 7.
fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
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
        blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

/// A slot beside the box (y -8..-2): top 5, clearance 10, retract 8 -
/// its plunge-height rapid lands at Z 7. The rapid law's fixture since
/// #1559: a contour now enters over the stock standing above its top
/// (`carried_stock_entry.rs`), so it no longer dives into the block; a
/// slot is not carried and still does.
fn slot() -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(-6.0, -5.0),
        b: camcraft_app::geo2d::Point2::new(16.0, -5.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Some(Height::fixed(8.0)),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// The box beside the slot in a block `stock_dz` tall (Z 0..dz).
fn world_with(stock_dz: f64) -> (World, Vec<Warning>, Vec<camcraft_app::cl::Move>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: stock_dz }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Slot1", "EM-0600-4FL", SrcRef::Stock, slot());
    history.execute(&world, cmd.expect("create slot"));
    history.execute(&world, GenerateOperation::new(&world, "Slot1").expect("generate"));
    let e = model::find_by_name(&world, "Slot1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    (world, cache.warnings.clone(), cache.path.moves.clone())
}

#[test]
fn a_vertical_rapid_into_a_taller_block_is_named_and_blocks_the_post() {
    // The 10 mm block stands 5 mm over the slot's top: the rapid
    // straight down from clearance to the plunge height (Z 7) beside
    // the box crosses 3 mm of uncut stock.
    let (world, warns, moves) = world_with(10.0);
    let vertical_down: Vec<usize> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.kind == MoveKind::Rapid && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < 10.0 - 1e-6 && m.to.z < m.from.z
        })
        .map(|(i, _)| i)
        .collect();
    assert!(!vertical_down.is_empty(), "the fixture must rapid down under the block's top: {moves:?}");
    // The first dive meets the raw block and is named; a later level's
    // re-entry down the first plunge's own kerf is acquitted by the
    // cuts before it (cut order), so it may stay silent.
    assert!(warns.contains(&Warning::RapidInStock { at: vertical_down[0] }), "the first dive went unnamed: {warns:?}");
    assert!(
        warns.iter().all(|w| matches!(w, Warning::RapidInStock { at } if vertical_down.contains(at))),
        "only the dives are convicted: {warns:?}"
    );
    let out = tmp_nc("dive");
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a program with a rapid into the block posted"),
        Err(e) => assert!(
            e.contains("post blocked") && e.contains(&format!("rapid move {} ", vertical_down[0])),
            "refused, but not by the rapid law: {e}"
        ),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn the_same_slot_over_stock_to_its_top_is_silent() {
    let (world, warns, _) = world_with(5.0);
    assert!(warns.is_empty(), "{warns:?}");
    let out = tmp_nc("clean");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

/// A face of the whole stock block down to `cut` (from its top, one
/// pass).
fn face(cut: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(20.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// The 10 mm block, a face of it to `face_cut`, then the slot -
/// both generated in program order (the names sort the face first,
/// `setup::operation_order`): the slot's verdict and the post's.
fn face_then_slot(face_cut: f64, tag: &str) -> (Vec<Warning>, Vec<Warning>, Result<(), String>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Op1Face", "EM-0600-4FL", SrcRef::Stock, face(face_cut));
    history.execute(&world, cmd.expect("create face"));
    let cmd = CreateOperation::new(&world, "Op2Slot", "EM-0600-4FL", SrcRef::Stock, slot());
    history.execute(&world, cmd.expect("create slot"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    history.execute(&world, GenerateOperation::new(&world, "Op2Slot").expect("generate slot"));
    let warns = |name: &str| {
        let e = model::find_by_name(&world, name).expect("op");
        model::toolpath_of(&world, &e).expect("cache").warnings
    };
    let out = tmp_nc(tag);
    let posted = posting::post_setup(&world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    (warns("Op1Face"), warns("Op2Slot"), posted)
}

#[test]
fn a_dive_into_stock_an_earlier_face_removed_posts() {
    // Review of #105: the face takes the 10 mm block down to Z 5, so
    // the slot's rapid 10 -> 7 beside the box falls through air the
    // program ahead of it already cut. The verdict reads the setup's
    // prior stock, not the raw block.
    let (face_w, contour_w, posted) = face_then_slot(5.0, "faced");
    assert!(face_w.is_empty(), "the face itself is clean: {face_w:?}");
    assert!(contour_w.is_empty(), "the faced block acquits the dive: {contour_w:?}");
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn a_dive_into_stock_no_earlier_op_removed_is_still_convicted() {
    // The face stops at Z 8: the dive to Z 7 still crosses 1 mm the
    // program never cut - named, and the post refuses it.
    let (_, contour_w, posted) = face_then_slot(8.0, "half-faced");
    assert!(
        contour_w.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "the dive under the face's floor went unnamed: {contour_w:?}"
    );
    match posted {
        Ok(()) => panic!("a dive under the face's floor posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains("rapid move"), "refused, but not by the rapid law: {e}"),
    }
}

/// The 10 mm block faced to Z 5, then the contour whose dive the face
/// acquits - both generated, both Ok.
fn faced_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Op1Face", "EM-0600-4FL", SrcRef::Stock, face(5.0));
    history.execute(&world, cmd.expect("create face"));
    let cmd = CreateOperation::new(&world, "Op2Contour", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    history.execute(&world, GenerateOperation::new(&world, "Op2Contour").expect("generate contour"));
    (world, history)
}

fn status_of(world: &World, name: &str) -> OpStatus {
    camcraft_app::operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

#[test]
fn one_regenerate_all_brings_a_dependent_pair_current() {
    // #145: the face's stepover moves, so its regenerated path moves and
    // the contour behind it (whose verdict reads it, #109) goes Stale
    // only once the face is re-cut. One Regenerate All must re-cut both,
    // in order, as one undo unit - never leave the contour for a second
    // click.
    let (world, mut history) = faced_world();
    let mut p = face(5.0);
    if let OpParams::Face { stepover, .. } = &mut p {
        *stepover = 3.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", p).expect("a legal stepover"));
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Stale);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok, "the face's cache has not moved yet");
    let batch = RegenerateAll::new(&world);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok, "building the batch writes nothing");
    assert_eq!(batch.regenerated(), 2, "the face, then the contour behind it");
    assert!(batch.skipped().is_empty(), "{:?}", batch.skipped());
    history.execute(&world, batch);
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Ok);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok);
    let out = tmp_nc("regen-all");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
    assert!(history.undo(&world), "one undo unit");
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Stale);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok, "the contour's old cache is back, against the old face");
}

#[test]
fn a_face_regenerated_shallower_marks_the_acquitted_contour_stale() {
    // #109: the contour's dive is acquitted by the face ahead of it, so
    // the face is an input of the contour's verdict. The face re-cut to
    // Z 8 leaves 1 mm the dive now crosses: the contour reads Stale and
    // the post refuses it, never posting the cached acquittal.
    let (world, mut history) = faced_world();
    let status = |name: &str| status_of(&world, name);
    assert_eq!(status("Op2Contour"), OpStatus::Ok, "the faced block acquits the dive");
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", face(8.0)).expect("a legal cut"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("regenerate face"));
    assert_eq!(status("Op1Face"), OpStatus::Ok, "the shallower face is fresh");
    assert_eq!(status("Op2Contour"), OpStatus::Stale, "the face ahead moved");
    let out = tmp_nc("refaced");
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a contour acquitted by a face that no longer cuts there posted"),
        Err(e) => assert!(e.contains("Op2Contour") && e.contains("regenerate"), "refused, but not as stale: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn the_program_ahead_digest_moves_only_with_the_path() {
    // #153 review B1: the digest reads the signature stored when the
    // cache was built - an unchanged regeneration keeps it (and the
    // contour Ok), a moved path moves it.
    let (world, mut history) = faced_world();
    let digest = |w: &World| camcraft_app::rest::prior_digest(w, "Op2Contour", None);
    let before = digest(&world).expect("the face is ahead");
    let face_e = model::find_by_name(&world, "Op1Face").expect("face");
    let cached = model::toolpath_of(&world, &face_e).expect("cache");
    let rebuilt = camcraft_app::schema::Toolpath::new(cached.path.clone(), Vec::new());
    assert_eq!(rebuilt.path_sig(), cached.path_sig(), "the signature is the path's, not the warnings'");
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("regenerate face"));
    assert_eq!(digest(&world), Some(before), "the same path, the same digest");
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok);
    let mut p = face(5.0);
    if let OpParams::Face { stepover, .. } = &mut p {
        *stepover = 3.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", p).expect("a legal stepover"));
    assert_eq!(digest(&world), Some(before), "an unregenerated edit moves no path");
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("regenerate face"));
    assert_ne!(digest(&world), Some(before), "a moved path moves the digest");
    let moved = model::toolpath_of(&world, &face_e).expect("cache");
    assert_ne!(moved.path_sig(), cached.path_sig());
}

#[test]
fn a_reorder_marks_the_dependent_stale() {
    // #109: the program ahead is part of the input - moving the
    // contour before the face changes both ops' programs ahead.
    let (world, mut history) = faced_world();
    history.execute(&world, ReorderOperation::new(&world, "Op2Contour", "Op1Face", false).expect("reorder"));
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Stale, "the face no longer runs ahead of it");
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Stale, "the contour now runs ahead of it");
    let out = tmp_nc("reordered");
    assert!(posting::post_setup(&world, &out).is_err(), "a reordered program posts only regenerated");
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_posed_op_keeps_its_stamp_when_the_program_ahead_moves() {
    // #109's inert-data law: a 3+2-posed op reads no prior (verify's
    // acquittal and the credits run only without a frame), so it
    // digests none - a re-cut face ahead leaves its stamp unchanged,
    // while the unposed contour's moves.
    let (world, mut history) = faced_world();
    let mut posed = face(5.0);
    if let OpParams::Face { tilt, .. } = &mut posed {
        *tilt = Some(camcraft_app::operation::Tilt { basis_version: 1, a_deg: 90.0, b_deg: 0.0, origin: [0.0; 3] });
    }
    let cmd = CreateOperation::new(&world, "Op3Posed", "EM-0600-4FL", SrcRef::Stock, posed);
    history.execute(&world, cmd.expect("create posed face"));
    let stamp = |name: &str| {
        let op = model::operation_of(&world, &model::find_by_name(&world, name).expect("op")).expect("op");
        camcraft_app::operation::op_stamp(&world, &op, name)
    };
    let (posed_before, contour_before) = (stamp("Op3Posed"), stamp("Op2Contour"));
    assert!(posed_before.is_some() && contour_before.is_some());
    let mut p = face(5.0);
    if let OpParams::Face { stepover, .. } = &mut p {
        *stepover = 3.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", p).expect("a legal stepover"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("regenerate face"));
    assert_eq!(stamp("Op3Posed"), posed_before, "a posed op reads no program ahead");
    assert_ne!(stamp("Op2Contour"), contour_before, "the unposed contour does");
}

#[test]
fn a_refused_op_ahead_does_not_hold_back_the_ops_behind_it() {
    // #153 re-review: the batch defers a dependent behind an op ahead
    // staged in the same round; when that op's generator then refuses,
    // the dependent must still be regenerated in a later round, and
    // only the refusal is named - one broken op never blocks the job.
    let (world, mut history) = faced_world();
    // A staged face from the stock's top: when the block grows past its
    // clearance (Z 20), the face reads Stale and its generator refuses
    // the plane.
    let mut staged = face(5.0);
    if let OpParams::Face { top_z, stepdown, .. } = &mut staged {
        *top_z = Some(Height { reference: camcraft_app::operation::HeightReference::TopOfStock, offset: 0.0 });
        *stepdown = Some(5.0);
    }
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", staged).expect("a legal staged face"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    history.execute(&world, GenerateOperation::new(&world, "Op2Contour").expect("generate contour"));
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok);
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 30.0 }, Default::default());
    history.execute(&world, cmd.expect("taller stock"));
    let mut c = contour();
    if let OpParams::Contour { feed, .. } = &mut c {
        *feed = 600.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "Op2Contour", c).expect("a legal feed"));
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Stale);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Stale);
    let batch = RegenerateAll::new(&world);
    let skipped: Vec<String> = batch.skipped().iter().map(|(n, w)| format!("{n}: {w}")).collect();
    assert_eq!(batch.regenerated(), 1, "the contour, behind the refusal: {skipped:?}");
    assert_eq!(skipped.len(), 1, "only the face is named: {skipped:?}");
    assert!(skipped[0].starts_with("Op1Face: ") && !skipped[0].contains("still stale"), "the refusal itself: {skipped:?}");
    history.execute(&world, batch);
    assert_eq!(status_of(&world, "Op2Contour"), OpStatus::Ok, "regenerated and judged");
    assert_eq!(status_of(&world, "Op1Face"), OpStatus::Stale, "the refused face keeps its tag");
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-vrapid-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

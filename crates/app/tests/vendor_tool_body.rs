//! Issue #361 at the app level: a holder file becomes a tool body
//! through one door. A STEP lathe holder modelled away from the turning
//! tool frame (an offset, L-shaped holder - not a body of revolution) is
//! placed by numbers and judged on every move; the same file mirrored
//! along the spindle stands clear. A STEP that could only be meshed
//! coarser than the importer promises is refused as a body by name. The
//! editor's and MCP's lathe placement preview is the command's own.
//!
//! Gate map (M4.4): covers camcraft_app::tool_body_file (the body-file
//! door), camcraft_app::tool_body_lathe_view (lathe placement reads),
//! commands::tool_body (the file constructors), tool_body_view::load_aligned.
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures - `testdata/models/lathe_holder_offset.step`
//! is an extruded L we wrote; no vendor file consulted.

mod common;

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CommandError, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetToolBody};
use camcraft_app::geometry::import::step;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::PostRef;
use camcraft_app::tool_body::{self, BodyFrame};
use camcraft_app::tool_body_lathe_view::{self, FrameChoice};
use camcraft_app::tools::ToolKind;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting, tool_body_file, tool_body_view};
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// How the fixture sits in the turning tool frame: its shank runs along
/// model +Y (radial), the spindle is model +X, and the tip stands at
/// model (5, 10, 3) - 2 mm off the head's corner. Placed, the head
/// spans x 2..32, z -26..-1 and the shank x 32..120, z -21..-1, both
/// y -10..10: nothing of it is on the spindle's far side of the tip.
const PLACED: LathePlacement = LathePlacement { radial: [0.0, 1.0, 0.0], along: [1.0, 0.0, 0.0], tip: [5.0, 10.0, 3.0] };

/// The same file with the spindle direction reversed: the head now
/// trails the tip toward the free end (z 1..26).
const MIRRORED: LathePlacement = LathePlacement { radial: [0.0, 1.0, 0.0], along: [-1.0, 0.0, 0.0], tip: [5.0, 10.0, 3.0] };

/// A lathe document holding a stepped part (r 12 below z 0 - the
/// shoulder - r 6 above to z 20) and the insert on the rack.
fn od_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    add_tool(&world, &mut history, INSERT);
    let part = revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", part));
    (world, history)
}

/// The finish ride down the r 6 neck to 2 above the shoulder.
fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn regen(world: &World, history: &mut History, name: &str) {
    history.execute(world, GenerateOperation::new(world, name).expect("regenerate"));
}

fn holder_hits(warns: &[Warning]) -> Vec<(usize, bool, f64)> {
    warns
        .iter()
        .filter_map(|w| match *w {
            Warning::HolderHit { at, part, short_mm } => Some((at, part, short_mm)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_step_lathe_holder_placed_off_its_model_frame_is_judged_and_its_mirror_stands_clear() {
    let (world, mut history) = od_world();
    let path = testdata("lathe_holder_offset.step");
    history.execute(&world, CreateOperation::new(&world, "F1", INSERT, SrcRef::Entity("step".into()), finish()).expect("create"));
    // #1232: the setup's post must serve turning; the default milling post is refused by name.
    posting::post_set(&world, &PostRef::Named("Fanuc Lathe".into()));
    regen(&world, &mut history, "F1");
    let bare = cache(&world, "F1").expect("cached");
    assert!(holder_hits(&bare.warnings).is_empty(), "{:?}", bare.warnings);

    // The head reaches 26 below the tip from 2 outside it: at the ride's
    // foot (2 above the shoulder) it stands at r 8.8 over the r 12 part.
    history.execute(&world, SetToolBody::lathe_from_file(&world, INSERT, &path, PLACED).expect("the STEP holder lands"));
    let e = model::find_by_name(&world, INSERT).expect("the tool");
    let body = tool_body::body_of(&world, &e).expect("a body");
    assert_eq!(body.source, "lathe_holder_offset.step");
    assert!(matches!(body.frame, BodyFrame::Lathe { place, zone: None, .. } if place == PLACED), "{:?}", body.frame);
    let (lo, hi) = tool_body_lathe_view::extent(&body).expect("an extent");
    for (got, want) in lo.iter().chain(hi.iter()).zip([2.0, -10.0, -26.0, 120.0, 10.0, -1.0]) {
        assert!((got - want).abs() < 1e-6, "extent {lo:?}..{hi:?}");
    }
    regen(&world, &mut history, "F1");
    let held = cache(&world, "F1").expect("cached");
    assert_eq!(held.path.moves, bare.path.moves, "the body never moves the path");
    let hits = holder_hits(&held.warnings);
    assert!(!hits.is_empty() && hits.iter().all(|h| h.1), "the shoulder rub went unnamed: {:?}", held.warnings);
    let worst = hits.iter().map(|h| h.2).fold(0.0, f64::max);
    assert!((worst - (12.0 - 8.8)).abs() < 0.02, "shortfall {worst}");
    let out = std::env::temp_dir().join(format!("camcraft-361-step-holder-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let refused = posting::post_setup(&world, &out).expect_err("a holder rub never posts");
    assert!(!out.exists(), "a refused post writes nothing");
    assert!(refused.contains("post blocked") && refused.contains("rubs the part"), "{refused}");

    // Not a body of revolution: the same file with the spindle reversed
    // trails the head toward the free end, clear of the shoulder.
    history.execute(&world, SetToolBody::lathe_from_file(&world, INSERT, &path, MIRRORED).expect("the mirrored holder lands"));
    regen(&world, &mut history, "F1");
    let clear = cache(&world, "F1").expect("cached");
    assert!(holder_hits(&clear.warnings).is_empty(), "the mirrored holder is clear: {:?}", clear.warnings);

    // One undo step each (a regeneration is a step of its own): the
    // mirrored body, then the first, then none.
    history.undo(&world);
    history.undo(&world);
    assert!(matches!(tool_body::body_of(&world, &e).map(|b| b.frame), Some(BodyFrame::Lathe { place, .. }) if place == PLACED));
    history.undo(&world);
    history.undo(&world);
    assert!(tool_body::body_of(&world, &e).is_none(), "undo takes the body off again");
}

#[test]
fn a_step_meshed_coarser_than_promised_is_refused_as_a_tool_body_by_name() {
    let path = testdata("Milling-test.step");
    // At the importer's own budget the file is a body file like any other.
    let full = tool_body_file::read(&path).expect("reads at full fidelity");
    assert_eq!(full.source, "Milling-test.step");
    assert!(!full.facets.is_empty());

    // Forced over budget (#864's own lever), every door refuses it.
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    add_tool(&world, &mut history, INSERT);
    let named = |e: &str| e.contains("Milling-test.step is too detailed to import at full fidelity") && e.contains("would read smaller than the holder is");
    step::with_facet_budget(2_000, || {
        let e = tool_body_file::read(&path).expect_err("the door refuses");
        assert!(named(&e), "{e}");
        let e = tool_body_view::load_aligned(&path).err().expect("the editor's read refuses");
        assert!(named(&e), "{e}");
        match SetToolBody::from_file(&world, "EM-0600-4FL", &path, [0.0, 0.0, 1.0], 60.0) {
            Err(CommandError::Invalid(e)) => assert!(named(&e), "{e}"),
            other => panic!("the spindle door let it through: {other:?}"),
        }
        match SetToolBody::from_step_auto(&world, "EM-0600-4FL", &path) {
            Err(CommandError::Invalid(e)) => assert!(named(&e), "{e}"),
            other => panic!("the auto-align door let it through: {other:?}"),
        }
        match SetToolBody::lathe_from_file(&world, INSERT, &path, PLACED) {
            Err(CommandError::Invalid(e)) => assert!(named(&e), "{e}"),
            other => panic!("the lathe door let it through: {other:?}"),
        }
    });
    for tool in ["EM-0600-4FL", INSERT] {
        let e = model::find_by_name(&world, tool).expect("the tool");
        assert!(tool_body::body_of(&world, &e).is_none(), "nothing landed on {tool}");
    }
    // A missing file stays the importer's own error.
    assert!(matches!(SetToolBody::from_file(&world, "EM-0600-4FL", &testdata("no-such.step"), [0.0, 0.0, 1.0], 60.0), Err(CommandError::Import(_))));
}

#[test]
fn the_lathe_placement_preview_is_the_commands_own_reading() {
    let file = tool_body_file::read(&testdata("lathe_holder_offset.step")).expect("reads");
    let zone = tool_body_lathe_view::zone_of_reaches([0.0, 6.0, 4.0], [5.0, 6.0, 1.0]);
    assert_eq!(zone, InsertZone { lo: [-0.0, -6.0, -4.0], hi: [5.0, 6.0, 1.0] });
    assert_eq!(tool_body_lathe_view::reaches_of_zone(&zone), ([0.0, 6.0, 4.0], [5.0, 6.0, 1.0]));
    let seen = tool_body_lathe_view::preview(&file.facets, &PLACED, Some(zone)).expect("a sane placement previews");

    let (world, mut history) = od_world();
    let spec = camcraft_app::commands::LatheBodySpec { source: file.source.clone(), facets: file.facets.clone(), place: PLACED, zone: Some(zone) };
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(spec)).expect("lands"));
    let e = model::find_by_name(&world, INSERT).expect("the tool");
    let landed = tool_body::body_of(&world, &e).expect("a body");
    assert_eq!(seen.frame, landed.frame, "the preview's bands are the landed body's");
    assert_eq!(seen.facets, landed.facets);

    // A landed body re-edits with AS_PLACED: the same facets again.
    let again = tool_body_lathe_view::preview(&landed.facets, &tool_body_lathe_view::AS_PLACED, Some(zone)).expect("re-edits");
    let BodyFrame::Lathe { clearance: a, .. } = &again.frame else { panic!("a lathe frame") };
    let BodyFrame::Lathe { clearance: b, .. } = &landed.frame else { panic!("a lathe frame") };
    assert_eq!(a, b, "re-applying a landed body changes no judged number");

    // The preview refuses what the command refuses, by the same name.
    let skew = LathePlacement { along: [1.0, 0.2, 0.0], ..PLACED };
    let e = tool_body_lathe_view::preview(&file.facets, &skew, None).expect_err("not square");
    assert!(e.contains("not square"), "{e}");
    let far = LathePlacement { tip: [5.0, -200.0, 3.0], ..PLACED };
    let e = tool_body_lathe_view::preview(&file.facets, &far, None).expect_err("tip away");
    assert!(e.contains("the tip stands more than 50.8 mm from the lathe body"), "{e}");

    // Which frame a kind's body takes - the split the fit laws refuse by.
    assert_eq!(tool_body_lathe_view::frame_choice(ToolKind::Turning), FrameChoice::Lathe);
    assert_eq!(tool_body_lathe_view::frame_choice(ToolKind::Groove), FrameChoice::Lathe);
    assert_eq!(tool_body_lathe_view::frame_choice(ToolKind::Bore), FrameChoice::Either);
    assert_eq!(tool_body_lathe_view::frame_choice(ToolKind::EndMill), FrameChoice::Spindle);
}

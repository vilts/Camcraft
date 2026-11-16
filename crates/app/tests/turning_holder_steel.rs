//! Issue #1037 at the app level: a lathe holder body judged against the
//! setup's workholding, not only the part and stock. An offset holder
//! whose leg reaches a chuck jaw is a blocking finding naming the move,
//! at generation and again at the post gate; the same holder short of
//! the jaw passes and posts; a jaw declared standing steel is read in
//! the machine frame (beside the tool's plane it is clear, where the
//! same jaw spinning is struck); and a fixture with no readable geometry
//! is named in a note.
//!
//! Gate map (M4.4): covers camcraft_app::operation::turning_holder_steel
//! (the law), fixture::unreadable_for, fixture_struck::op_hits (the
//! post gate's reading of it).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures, specced from the turning kernel's
//! design notes; no vendor material consulted.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    CreateOperation, GenerateOperation, History, ImportModel, LatheBodySpec, LoadMachine, SelectPost, SetFixtureFixed, SetToolBody,
    TagFixture,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef, STEEL_UNREAD_NOTE};
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_tools::lathe_holder::LathePlacement;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";

/// A closed axis-aligned box over `[lo, hi]`, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// An offset holder in the turning tool frame: a head over the tip (x
/// 2..24, from 1 below it to 10 above) and a leg standing out past the
/// part's r 12 shoulder (x 16..24), its foot `reach` below the tip -
/// toward the chuck. The leg never meets the part: at the finish's tip
/// radius (6.8) it stands from r 22.8.
fn offset_holder(reach: f64) -> LatheBodySpec {
    let mut facets = cuboid([2.0, -6.0, -1.0], [24.0, 6.0, 10.0]);
    facets.extend(cuboid([16.0, -6.0, -reach], [24.0, 6.0, -1.0]));
    let place = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
    LatheBodySpec { source: "offset.step".into(), facets, place, zone: None }
}

/// A lathe document: the stepped part (r 12 below z 0 to -16, r 6 above
/// to 20), the insert on the rack, and a chuck jaw named "jaw" over
/// `jaw` gripping the r 12 end from z -10 down, tagged as a fixture.
fn chucked(jaw: ([f64; 3], [f64; 3])) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    // A post that serves a lathe: main refuses one that does not
    // (`posting_serves`), which would mask this suite's own verdicts.
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, INSERT);
    let part = revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", part));
    history.execute(&world, ImportModel::from_mesh("jaw", Mesh::from_facets(cuboid(jaw.0, jaw.1))));
    history.execute(&world, TagFixture::new(&world, "jaw", None).expect("tag the jaw"));
    (world, history)
}

/// The jaw in the tool's own plane (the kernels walk +Y), r 12 to 30
/// over z -30 .. -10.
const IN_PLANE: ([f64; 3], [f64; 3]) = ([-5.0, 12.0, -30.0], [5.0, 30.0, -10.0]);

/// The finish ride down the r 6 neck to 2 above the shoulder.
fn finish(world: &World, history: &mut History) {
    let params = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let op = CreateOperation::new(world, "F1", INSERT, SrcRef::Entity("step".into()), params).expect("create");
    history.execute(world, op);
    history.execute(world, GenerateOperation::new(world, "F1").expect("generate"));
}

fn steel_hits(world: &World) -> Vec<usize> {
    let c = cache(world, "F1").expect("cached");
    c.warnings.iter().filter_map(|w| match *w {
        Warning::FixtureHit { at, fixture_part: FixturePart::Holder } => Some(at),
        _ => None,
    }).collect()
}

fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("camcraft-holder-1037-{tag}-{}.nc", std::process::id()));
    let r = posting::post_setup(world, &out).map(|_| ());
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert_eq!(wrote, r.is_ok(), "a refused post writes nothing: {r:?}");
    r
}

#[test]
fn an_offset_holder_reaching_a_chuck_jaw_blocks_naming_the_move() {
    let (world, mut history) = chucked(IN_PLANE);
    // The leg's foot 15 below the tip: at the ride's foot (z 2) it
    // stands at z -13, 3 into the jaw, r 22.8 .. 30.8 over its r 12 .. 30.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(15.0))).expect("a sane holder"));
    finish(&world, &mut history);
    let c = cache(&world, "F1").expect("cached");
    assert!(!c.warnings.iter().any(|w| matches!(w, Warning::HolderHit { .. })), "the leg never meets the part: {:?}", c.warnings);
    let hits = steel_hits(&world);
    assert!(!hits.is_empty(), "the holder in the chuck jaw went unnamed: {:?}", c.warnings);
    let e = post(&world, "jaw").expect_err("a holder in the chuck posted");
    assert!(e.contains("post blocked") && e.contains(&format!("holder at move {} strikes a fixture", hits[0])), "{e}");
    // The post gate reads the steel as it stands now, by the same law:
    // the live fixture walk names the strike too.
    let live = posting::fixture_findings(&world).expect("a fixture walk");
    assert!(live.unjudged.is_empty(), "{:?}", live.unjudged);
    let live: Vec<usize> = live.hits.iter().filter_map(|(_, w)| match *w {
        Warning::FixtureHit { at, fixture_part: FixturePart::Holder } => Some(at),
        _ => None,
    }).collect();
    assert_eq!(live, hits, "the gate and generation read one law");
}

#[test]
fn the_same_holder_short_of_the_jaw_passes_and_posts() {
    let (world, mut history) = chucked(IN_PLANE);
    // The foot 10 below the tip: z -8 at the ride's foot, 2 above the jaw.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(10.0))).expect("a sane holder"));
    finish(&world, &mut history);
    let c = cache(&world, "F1").expect("cached");
    assert!(steel_hits(&world).is_empty(), "{:?}", c.warnings);
    assert!(!c.path.notes.iter().any(|n| n.starts_with(STEEL_UNREAD_NOTE)), "{:?}", c.path.notes);
    post(&world, "clear").expect("the clear holder posts");
}

#[test]
fn a_standing_jaw_is_read_in_the_machine_frame_and_a_spinning_one_about_the_axis() {
    // The jaw beside the tool's plane (x 20 .. 30): spinning with the
    // part it sweeps r 20 .. 30.4, where the leg rides - struck.
    let beside = ([20.0, -5.0, -30.0], [30.0, 5.0, -10.0]);
    let (world, mut history) = chucked(beside);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(15.0))).expect("a sane holder"));
    finish(&world, &mut history);
    assert!(!steel_hits(&world).is_empty(), "the spinning jaw went unnamed");
    // Declared standing steel, it stays beside the plane, past the
    // leg's |y| 6: clear.
    history.execute(&world, SetFixtureFixed::new(&world, "jaw", true).expect("fixed"));
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("regenerate"));
    assert!(steel_hits(&world).is_empty(), "the standing jaw is beside the plane");
    // In the plane, standing or spinning, it is struck.
    let (world, mut history) = chucked(IN_PLANE);
    history.execute(&world, SetFixtureFixed::new(&world, "jaw", true).expect("fixed"));
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(15.0))).expect("a sane holder"));
    finish(&world, &mut history);
    assert!(!steel_hits(&world).is_empty(), "the standing jaw in the plane went unnamed");
}

#[test]
fn a_fixture_the_law_cannot_read_is_named_on_the_op() {
    let (world, mut history) = chucked(IN_PLANE);
    // A fixture with no geometry: no box, so no law can read it.
    history.execute(&world, ImportModel::from_mesh("ghost", Mesh::from_facets(Vec::new())));
    history.execute(&world, TagFixture::new(&world, "ghost", None).expect("tag the ghost"));
    // The bare insert's op says its holder went unjudged, nothing more.
    finish(&world, &mut history);
    let notes = cache(&world, "F1").expect("cached").path.notes;
    assert!(!notes.iter().any(|n| n.starts_with(STEEL_UNREAD_NOTE)), "{notes:?}");
    // With a body, the ghost is named.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(offset_holder(10.0))).expect("a sane holder"));
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("regenerate"));
    let notes = cache(&world, "F1").expect("cached").path.notes;
    assert!(notes.iter().any(|n| n.starts_with(STEEL_UNREAD_NOTE) && n.contains("'ghost'") && !n.contains("'jaw'")), "{notes:?}");
}


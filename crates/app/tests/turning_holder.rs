//! Issue #369 at the app level: a lathe holder body, placed in the
//! turning tool frame, judged on every move against the revolved part
//! and the stock standing at each instant. An offset OD holder that
//! reaches the shoulder of a stepped part, and a boring bar too fat for
//! its bore, are HolderHits that block the post; a holder clear of the
//! work passes, a holder trailing where a rough cut passes, and a turning
//! tool with no body is unchanged but for its "holder not judged" note.
//!
//! Gate map (M4.4): covers camcraft_app::operation::turning_holder and
//! turning_holder_scene (the holder law), camcraft_app::tool_body_lathe
//! (placement, fits), commands::tool_body (SetToolBody::lathe),
//! project::tool_body_wire (the lathe body on file).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures, specced from the turning kernel's
//! design notes; no vendor material consulted.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    CommandError, CreateOperation, GenerateOperation, History, ImportModel, LatheBodySpec, LoadMachine, SetStock, SetToolBody,
};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tool_body_lathe::HOLDER_NOTE;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting, project, tool_body};
use camcraft_tools::lathe_holder::LathePlacement;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";

/// A closed axis-aligned box over `[lo, hi]`, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// The mesh modelled in the turning tool frame itself.
const AS_MODELLED: LathePlacement = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };

fn body(source: &str, facets: Vec<Triangle>) -> LatheBodySpec {
    LatheBodySpec { source: source.into(), facets, place: AS_MODELLED, zone: None }
}

/// An OD holder shank standing radially out from the tip: x 2..40,
/// y -6..6, its foot `reach` below the tip along the spindle (toward the
/// chuck) and 10 above it.
fn od_holder(reach: f64) -> LatheBodySpec {
    body("od.step", cuboid([2.0, -6.0, -reach], [40.0, 6.0, 10.0]))
}

/// The stepped part: r 12 below z 0 (the shoulder), r 6 above to z 20.
fn stepped() -> camcraft_app::geometry::Mesh {
    revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)])
}

/// A lathe document holding the stepped part, the insert on the rack,
/// and a bar of Ø24 from z -16 to 20 when `bar`.
fn od_world(bar: bool) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    if bar {
        let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
        let shape = camcraft_app::stock::StockShape::Cylinder { diameter: 24.0, height: 36.0 };
        history.execute(&world, SetStock::new(&world, shape, at).expect("bar stock"));
    }
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, INSERT);
    history.execute(&world, ImportModel::from_mesh("step", stepped()));
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

fn op(world: &World, history: &mut History, name: &str, tool: &str, src: &str, params: OpParams) {
    history.execute(world, CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), params).expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn regen(world: &World, history: &mut History, name: &str) {
    history.execute(world, GenerateOperation::new(world, name).expect("regenerate"));
}

fn holder_hits(warns: &[Warning]) -> Vec<(usize, bool, f64)> {
    warns.iter().filter_map(|w| match *w {
        Warning::HolderHit { at, part, short_mm } => Some((at, part, short_mm)),
        _ => None,
    }).collect()
}

fn refused_post(world: &World, tag: &str) -> String {
    let out = std::env::temp_dir().join(format!("camcraft-holder-369-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let r = posting::post_setup(world, &out);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(!wrote, "a refused post writes nothing");
    match r {
        Ok(_) => panic!("a holder rub posted"),
        Err(e) => e,
    }
}

#[test]
fn an_offset_od_holder_reaching_the_shoulder_is_convicted_and_the_bare_insert_is_unchanged() {
    let (world, mut history) = od_world(false);
    op(&world, &mut history, "F1", INSERT, "step", finish());
    let bare = cache(&world, "F1").expect("cached");
    assert!(holder_hits(&bare.warnings).is_empty(), "{:?}", bare.warnings);
    assert!(bare.path.notes.iter().any(|n| n == HOLDER_NOTE), "the bare insert says its holder went unjudged: {:?}", bare.path.notes);

    // The holder's foot reaches 8 below the tip: at the ride's foot (2
    // above the shoulder) it stands at r 8.8 over the r 12 shoulder.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(od_holder(8.0))).expect("a sane holder"));
    regen(&world, &mut history, "F1");
    let held = cache(&world, "F1").expect("cached");
    assert_eq!(held.path.moves, bare.path.moves, "the body never moves the path");
    assert!(!held.path.notes.iter().any(|n| n == HOLDER_NOTE), "a judged holder carries no note: {:?}", held.path.notes);
    let hits = holder_hits(&held.warnings);
    assert!(!hits.is_empty() && hits.iter().all(|h| h.1), "the shoulder rub went unnamed: {:?}", held.warnings);
    // The shortfall is the radial depth into the shoulder: the foot at
    // r 8.8 (nose centre 6.8 + 2) under the r 12 skin.
    let worst = hits.iter().map(|h| h.2).fold(0.0, f64::max);
    assert!((worst - (12.0 - 8.8)).abs() < 0.02, "shortfall {worst}");
    let first = hits[0].0;
    let e = refused_post(&world, "od");
    assert!(e.contains("post blocked") && e.contains(&format!("holder body at move {first} rubs the part")), "{e}");
    assert!(e.contains("mm short of clearance"), "{e}");

    // Cleared, the op is the bare one again, note and all.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, None).expect("clear"));
    regen(&world, &mut history, "F1");
    let again = cache(&world, "F1").expect("cached");
    assert_eq!((again.path.moves, again.warnings, again.path.notes), (bare.path.moves, bare.warnings, bare.path.notes));
}

#[test]
fn a_holder_clear_of_the_work_passes_and_posts() {
    let (world, mut history) = od_world(false);
    // The same shank standing only above the tip's station: it never
    // reaches the shoulder.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(body("od.step", cuboid([2.0, -6.0, 0.5], [40.0, 6.0, 10.0])))).expect("a sane holder"));
    op(&world, &mut history, "F1", INSERT, "step", finish());
    let c = cache(&world, "F1").expect("cached");
    assert!(holder_hits(&c.warnings).is_empty(), "{:?}", c.warnings);
    let out = std::env::temp_dir().join(format!("camcraft-holder-369-clear-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn uncut_stock_under_the_holder_is_named_and_a_rough_ahead_clears_it() {
    // In a Ø24 bar, a finish alone leaves the shell outside its nose
    // band standing: the shank 2 above the tip rides through it.
    let (world, mut history) = od_world(true);
    let clear = || body("od.step", cuboid([2.0, -6.0, 0.5], [40.0, 6.0, 10.0]));
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(clear())).expect("a sane holder"));
    op(&world, &mut history, "F2", INSERT, "step", finish());
    let c = cache(&world, "F2").expect("cached");
    let hits = holder_hits(&c.warnings);
    assert!(!hits.is_empty() && hits.iter().all(|h| !h.1), "the stock rub went unnamed (or named the part): {:?}", c.warnings);
    // A rough ahead of the finish eats the shell down to the leave: the
    // same holder then trails through air.
    let (world, mut history) = od_world(true);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(clear())).expect("a sane holder"));
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    op(&world, &mut history, "R1", INSERT, "step", rough);
    let r = cache(&world, "R1").expect("cached");
    assert!(holder_hits(&r.warnings).is_empty(), "the rough's holder trails its own cut: {:?}", r.warnings);
    op(&world, &mut history, "F2", INSERT, "step", finish());
    let c = cache(&world, "F2").expect("cached");
    assert!(holder_hits(&c.warnings).is_empty(), "the holder rode where the rough cut: {:?}", c.warnings);
}

#[test]
fn a_boring_bar_too_fat_for_its_bore_is_convicted_and_a_slim_one_passes() {
    let (world, mut history) = bore_world();
    pilot(&world, &mut history);
    create(&world, &mut history, "BR1", bore_params());
    assert!(cache(&world, "BR1").expect("cached").warnings.is_empty(), "the bare bar bores clean");
    // A slim bar: x -4..-0.5 toward the axis, y ±1.5, up the spindle
    // from 1 above the nose - at the ride's r 5.6 it reaches r 5.32,
    // inside the Ø11.3 pilot.
    history.execute(&world, SetToolBody::lathe(&world, "BR-1000", Some(body("slim.stp", cuboid([-4.0, -1.5, 1.0], [-0.5, 1.5, 80.0])))).expect("a sane bar"));
    regen(&world, &mut history, "BR1");
    let c = cache(&world, "BR1").expect("cached");
    assert!(holder_hits(&c.warnings).is_empty(), "the slim bar fits: {:?}", c.warnings);
    // A Ø12 bar on the insert's edge: across the axis its far side and
    // its ±6 flats stand past the r 6 wall, rapids and feeds alike.
    history.execute(&world, SetToolBody::lathe(&world, "BR-1000", Some(body("fat.stp", cuboid([-12.0, -6.0, 1.0], [-0.4, 6.0, 80.0])))).expect("a sane bar"));
    regen(&world, &mut history, "BR1");
    let c = cache(&world, "BR1").expect("cached");
    let hits = holder_hits(&c.warnings);
    assert!(hits.iter().any(|h| h.1), "the bar in the wall went unnamed: {:?}", c.warnings);
    // On the centred approach (nose r 4.8) the bar reaches
    // hypot(12 - 4.8, 6) = 9.37 from the axis: ~3.4 into the r ~5.9-6
    // faceted wall.
    let worst = hits.iter().map(|h| h.2).fold(0.0, f64::max);
    assert!(worst > 3.2 && worst < 3.6, "shortfall {worst}");
    let kinds: Vec<_> = hits.iter().map(|h| c.path.moves[h.0].kind).collect();
    assert!(kinds.contains(&camcraft_app::cl::MoveKind::Rapid), "the approach rapid is judged too: {kinds:?}");
    assert!(kinds.iter().any(|k| *k != camcraft_app::cl::MoveKind::Rapid), "and the ride: {kinds:?}");
    let e = refused_post(&world, "bore");
    assert!(e.contains("post blocked") && e.contains("holder body at move"), "{e}");
}

#[test]
fn a_grooving_blade_holder_is_judged_by_its_band_and_a_fat_one_rubs_the_groove_walls() {
    // turning_blade's grooved bar (r 6, a groove floored at r 4 over z
    // -6.8 .. -3.6) in a Ø20 bar; the 2 mm blade's own band, 8 mm tall,
    // is its footprint: a holder 0.05 inside that band each side, from 5
    // above the tip, rides the kerf the blade cut, while one 6 mm wide
    // reaching 2 above the tip rubs the groove's walls.
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let shape = camcraft_app::stock::StockShape::Cylinder { diameter: 20.0, height: 44.0 };
    history.execute(&world, SetStock::new(&world, shape, at).expect("bar stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, "GT-0200");
    let grooved = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)]);
    history.execute(&world, ImportModel::from_mesh("bar", grooved));
    let params = OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(35.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        stepdown: 1.0,
        stock_to_leave: 0.0,
        feed: 150.0,
        plunge_feed: 60.0,
    };
    history.execute(&world, SetToolBody::lathe(&world, "GT-0200", Some(body("blade.stp", cuboid([5.0, -0.9, -0.95], [40.0, 0.9, 0.95])))).expect("a blade holder"));
    op(&world, &mut history, "G1", "GT-0200", "bar", params);
    let c = cache(&world, "G1").expect("cached");
    assert!(holder_hits(&c.warnings).is_empty(), "the holder in the blade's band rides its kerf: {:?}", c.warnings);
    history.execute(&world, SetToolBody::lathe(&world, "GT-0200", Some(body("fat.stp", cuboid([2.0, -3.0, -3.0], [40.0, 3.0, 3.0])))).expect("a fat holder"));
    regen(&world, &mut history, "G1");
    let c = cache(&world, "G1").expect("cached");
    assert!(holder_hits(&c.warnings).iter().any(|h| h.1), "the fat holder in the groove walls went unnamed: {:?}", c.warnings);
}

#[test]
fn degenerate_lathe_bodies_and_wrong_tools_refuse_by_name() {
    let (world, mut history) = od_world(false);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let refuse = |tool: &str, spec: LatheBodySpec, says: &str| match SetToolBody::lathe(&world, tool, Some(spec)) {
        Err(CommandError::Invalid(m)) => assert!(m.contains(says), "{m}"),
        Ok(_) => panic!("accepted a body that should say '{says}'"),
        Err(e) => panic!("{e:?}"),
    };
    let base = || od_holder(0.0);
    refuse(INSERT, LatheBodySpec { place: LathePlacement { radial: [0.0; 3], ..AS_MODELLED }, ..base() }, "finite, non-zero");
    refuse(INSERT, LatheBodySpec { place: LathePlacement { along: [1.0, 0.0, 1.0], ..AS_MODELLED }, ..base() }, "not square");
    refuse(INSERT, LatheBodySpec { place: LathePlacement { tip: [f64::NAN, 0.0, 0.0], ..AS_MODELLED }, ..base() }, "tip point");
    refuse(INSERT, LatheBodySpec { place: LathePlacement { tip: [-200.0, 0.0, 0.0], ..AS_MODELLED }, ..base() }, "tip stands more than");
    refuse(INSERT, LatheBodySpec { facets: Vec::new(), ..base() }, "no facets");
    let flat = vec![Triangle::new(Point3::new(2.0, 0.0, 0.0), Point3::new(40.0, 0.0, 0.0), Point3::new(2.0, 6.0, 0.0))];
    refuse(INSERT, LatheBodySpec { facets: flat, ..base() }, "no length along the spindle");
    let line = vec![Triangle::new(Point3::new(2.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.0), Point3::new(4.0, 0.0, 2.0))];
    refuse(INSERT, LatheBodySpec { facets: line, ..base() }, "degenerate");
    refuse("EM-0400-2FL", base(), "no lathe steel");
    assert!(matches!(SetToolBody::lathe(&world, "nope", Some(base())), Err(CommandError::NotFound(_))));
    // A placement off the modelled frame lands the same body: a model
    // with its shank along -Y and the spindle along +X, tip at (5, 5, 5).
    let turned: Vec<Triangle> = od_holder(8.0)
        .facets
        .iter()
        .map(|t| {
            let m = |p: Point3| Point3::new(5.0 + p.z, 5.0 - p.x, 5.0 - p.y);
            Triangle::new(m(t.a), m(t.b), m(t.c))
        })
        .collect();
    let place = LathePlacement { radial: [0.0, -1.0, 0.0], along: [1.0, 0.0, 0.0], tip: [5.0, 5.0, 5.0] };
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(LatheBodySpec { source: "turned.step".into(), facets: turned, place, zone: None })).expect("placed"));
    let a = tool_body::body_of(&world, &model::find_by_name(&world, INSERT).expect("tool")).expect("landed");
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(od_holder(8.0))).expect("as modelled"));
    let b = tool_body::body_of(&world, &model::find_by_name(&world, INSERT).expect("tool")).expect("landed");
    let bands = |x: &camcraft_app::schema::ToolBody| match &x.frame {
        tool_body::BodyFrame::Lathe { clearance, .. } => clearance.bands().to_vec(),
        tool_body::BodyFrame::Spindle { .. } => Vec::new(),
    };
    let (ba, bb) = (bands(&a), bands(&b));
    assert!(!ba.is_empty() && ba.len() == bb.len(), "{} vs {}", ba.len(), bb.len());
    for (p, q) in ba.iter().zip(&bb) {
        assert!((p.x_lo - q.x_lo).abs() < 1e-9 && (p.x_hi - q.x_hi).abs() < 1e-9 && (p.y_hi - q.y_hi).abs() < 1e-9, "{p:?} vs {q:?}");
    }
    // Undo walks back to the turned placement.
    history.undo(&world);
    assert_eq!(tool_body::body_of(&world, &model::find_by_name(&world, INSERT).expect("tool")).map(|x| x.source).as_deref(), Some("turned.step"));
}

#[test]
fn a_lathe_body_on_a_bar_refuses_a_milling_op_by_name() {
    // The boring bar serves the drilling cycles too, where no law reads
    // a lathe-frame body: the op refuses rather than cut it unjudged.
    let (world, mut history) = bore_world();
    history.execute(&world, SetToolBody::lathe(&world, "BR-1000", Some(body("bar.stp", cuboid([-4.0, -1.5, 1.0], [-0.5, 1.5, 80.0])))).expect("a sane bar"));
    let hole = camcraft_app::drilling::Hole { xy: camcraft_app::geo2d::Point2::new(0.0, 0.0), radius: 5.0, entry_z: 50.0, floor_z: -16.0, through: false, stepped: false };
    let params = OpParams::Drill {
        clearance_z: Height::fixed(60.0),
        retract_z: None,
        top_z: Height::fixed(50.0),
        bottom_z: None,
        cycle: camcraft_app::drilling::CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    };
    history.execute(&world, CreateOperation::new(&world, "D1", "BR-1000", SrcRef::Entity("bar".into()), params).expect("create"));
    match GenerateOperation::new(&world, "D1") {
        Err(e) => assert!(format!("{e:?}").contains("lathe-frame body"), "{e:?}"),
        Ok(_) => panic!("a milling op cut with a lathe body unjudged"),
    }
}

#[test]
fn the_lathe_body_round_trips_through_the_project_file() {
    let (world, mut history) = od_world(false);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(od_holder(8.0))).expect("a sane holder"));
    let path = std::env::temp_dir().join(format!("camcraft_lathe_body_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = camcraft_app::world::new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "save-load-save is byte-identical");
    assert!(first.contains("\"lathe\"") && !first.contains("\"axis\""), "a lathe body writes its own placement only");
    let tool = |w: &World| tool_body::body_of(w, &model::find_by_name(w, INSERT).expect("tool"));
    assert_eq!(tool(&world2), tool(&world), "facets, placement and the re-derived bands all agree");
}

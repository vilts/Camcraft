//! #319: verification in a later setup judges the carried body. A 10x10x5
//! part sits in a block Setup1 faces; Setup2 holds the body Setup1 left
//! ("Same as", or flipped) but declares a stock whose top is the part's.
//! Setup2's contour dives from its retract plane beside the part: where
//! Setup1 left material standing above the declared top the dive is
//! refused, where Setup1 faced it away it is allowed, a declared stock
//! larger than the carried block is judged present, the mismatch is said
//! by name, and the post agrees.
//! Provenance: our own fixtures (vertical_rapid_stock.rs's contour over a
//! box, setup_carry.rs's faced block) over plain re-clamp practice; no
//! vendor material consulted.
//!
//! Gate map (#319): covers app::operation (carried_stock, job, stamp),
//! app::rest::across (carried_stock_block, carried_stock_body,
//! carried_digest), app::stock_sim, sim::stock_sim (simulate_document's carry refusal),
//! app::posting (the gate's cached warnings).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History, ImportModel,
    SetSetupRelative, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{self, CARRIED_OUTSIDE_DECLARED, CARRIED_OUTSIDE_NOTED, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{CARRY_TURN_REFUSAL, simulate_document};
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";

/// A contour of the part: top 5, clearance 12, retract 8 - its dive from
/// the retract plane lands below Z 8, beside the part.
fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(12.0),
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

/// A box stock `w` x `w` centred on the part, Z `z0` up `dz`.
fn block(world: &World, w: f64, z0: f64, dz: f64) -> SetStock {
    let at = Location { pos: Point3::new(5.0, 5.0, z0), ..Default::default() };
    SetStock::new(world, StockShape::Box { dx: w, dy: w, dz }, at).expect("stock")
}

/// How Setup2 holds the body Setup1 leaves.
#[derive(Clone, Copy, PartialEq)]
enum Hold {
    SameAs,
    FlipX,
    /// Same hold, not linked: the clip, nothing carried.
    Clip,
}

/// The part (0..10)^2 x [0, 5] in Setup1's `w1`-wide block Z -5..10,
/// faced to `cut1`; Setup2 held as `hold`, declaring a `w2`-wide block
/// Z -5..5 (top at the part's), and Contour2 round the part.
fn program(w1: f64, cut1: f64, hold: Hold, w2: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    h.execute(&world, block(&world, w1, -5.0, 15.0));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face(cut1)).expect("create");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    match hold {
        Hold::SameAs => h.execute(&world, SetSetupRelative::link(&world, "Setup2", "Setup1", [0.0; 3]).expect("same as")),
        Hold::FlipX => h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip")),
        Hold::Clip => {}
    }
    h.execute(&world, block(&world, w2, -5.0, 10.0));
    let op = CreateOperation::new(&world, "Contour2", TOOL, SrcRef::Entity("part".into()), contour()).expect("create");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("generate"));
    (world, h)
}

fn cached(world: &World, name: &str) -> camcraft_app::schema::Toolpath {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e)).expect("cached")
}

/// Contour2's vertical rapid dives below Z 8 (the retract plane).
fn dives(world: &World) -> Vec<usize> {
    let moves = cached(world, "Contour2").path.moves;
    let out: Vec<usize> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.kind == MoveKind::Rapid
                && m.to.z < m.from.z
                && m.to.z < 8.0 - 1e-6
                && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) < 1e-6
        })
        .map(|(i, _)| i)
        .collect();
    assert!(!out.is_empty(), "the fixture must dive below the retract plane: {moves:?}");
    out
}

fn convicted(world: &World) -> Vec<usize> {
    cached(world, "Contour2")
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::RapidInStock { at } => Some(*at),
            _ => None,
        })
        .collect()
}

fn noted(world: &World) -> bool {
    let notes = cached(world, "Contour2").path.notes;
    notes.iter().any(|n| n.starts_with(CARRIED_OUTSIDE_DECLARED) && n.contains("'Setup1'") && n.contains("'Setup2'"))
}

#[test]
fn a_dive_clear_of_the_declared_block_into_carried_material_is_refused() {
    // Setup1 faces only to 9: the carried body stands to Z 9 round the
    // part, 4 mm above Setup2's declared top - the dive is clear of the
    // declared block and inside the real one.
    let (world, _h) = program(100.0, 9.0, Hold::SameAs, 100.0);
    let (d, c) = (dives(&world), convicted(&world));
    assert!(c.contains(&d[0]), "dives {d:?}, convicted {c:?}");
    assert!(noted(&world), "{:?}", cached(&world, "Contour2").path.notes);
    let dir = std::env::temp_dir().join(format!("cc-319-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join("refused.nc");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_dir_all(&dir);
    match posted {
        Ok(_) => panic!("the carried-stock dive posted"),
        Err(e) => assert!(
            e.contains("post blocked") && e.contains("operation Contour2: ") && e.contains(CARRIED_OUTSIDE_DECLARED),
            "{e}"
        ),
    }
}

#[test]
fn a_dive_into_material_setup1_removed_is_allowed() {
    // Setup1 faces the block down to the part's top: the dive meets air,
    // as the declared stock says too - no note, no conviction.
    let (world, _h) = program(100.0, 5.0, Hold::SameAs, 100.0);
    dives(&world);
    assert!(convicted(&world).is_empty(), "{:?}", cached(&world, "Contour2").warnings);
    assert!(!noted(&world));
}

#[test]
fn a_flipped_carry_judges_the_dive_against_the_flipped_body() {
    // Flipped about the part's centre (Z 2.5): Setup1's unfaced bottom
    // (-5) stands on top at Z 10 - the dive meets it.
    let (world, _h) = program(100.0, 9.0, Hold::FlipX, 100.0);
    let (d, c) = (dives(&world), convicted(&world));
    assert!(c.contains(&d[0]), "dives {d:?}, convicted {c:?}");
    assert!(noted(&world));
}

#[test]
fn declared_stock_beyond_the_carried_block_is_judged_present() {
    // Setup1 holds only a 12 mm block (x, y -1..11), faced to the part's
    // top; Setup2 declares 100 mm standing to Z 10. The dive beside the
    // part (past the carried block) lands in declared material the
    // carried grid does not cover: it is never read as air.
    let (world, mut h) = program(12.0, 5.0, Hold::SameAs, 100.0);
    h.execute(&world, block(&world, 100.0, -5.0, 15.0));
    h.execute(&world, GenerateOperation::new(&world, "Contour2").expect("regenerate"));
    let (d, c) = (dives(&world), convicted(&world));
    assert!(c.contains(&d[0]), "dives {d:?}, convicted {c:?}");
}

#[test]
fn a_turn_off_the_quarter_with_only_a_non_rest_op_after_it_is_refused_by_name() {
    // Contour2 (no rest reading) was generated while Setup2 held the part
    // like Setup1; the part is then turned 30 degrees in Setup2. Its
    // regeneration is refused by name, never cut against the declared
    // block, and the program sim never judges the turned program.
    let (world, mut h) = program(100.0, 5.0, Hold::SameAs, 100.0);
    h.execute(&world, SetSetupRelative::unlink(&world, "Setup2").expect("unlink"));
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup2", [0.0, 0.0, 30.0]).expect("30 degrees"));
    let generated = GenerateOperation::new(&world, "Contour2").err().expect("refused").to_string();
    assert!(
        generated.contains("Contour2")
            && generated.contains(CARRY_TURN_REFUSAL)
            && generated.contains("from setup 'Setup1' into setup 'Setup2'"),
        "{generated}"
    );
    assert!(simulate_document(&world).is_err(), "the turned program is never judged");
    assert!(!posting::verification_report(&world).problems.is_empty(), "nor posted");
}

#[test]
fn an_overhang_no_feed_move_reaches_is_noted_and_posts() {
    // Setup1 holds a 140 mm block faced to the part's top; Setup2 declares
    // 100 mm. The carried rim stands 20 mm beside the declared stock, far
    // from the contour round the part: noted by name, never refused.
    let (world, _h) = program(140.0, 5.0, Hold::SameAs, 100.0);
    let notes = cached(&world, "Contour2").path.notes;
    assert!(!noted(&world), "{notes:?}");
    assert!(notes.iter().any(|n| n.starts_with(CARRIED_OUTSIDE_NOTED) && n.contains("'Setup2'")), "{notes:?}");
    let problems = posting::verification_report(&world).problems;
    assert!(!problems.iter().any(|p| p.contains(CARRIED_OUTSIDE_DECLARED)), "{problems:?}");
}

/// A posed (3+2, untilted angles) face in the active setup, created only:
/// its stamp is read, never cut.
fn posed(world: &World, h: &mut History, name: &str) {
    let mut p = face(5.0);
    if let OpParams::Face { tilt, .. } = &mut p {
        *tilt = Some(Tilt { a_deg: 0.0, b_deg: 0.0, origin: [0.0; 3] });
    }
    h.execute(world, CreateOperation::new(world, name, TOOL, SrcRef::Stock, p).expect("create posed"));
}

fn stamp(world: &World, name: &str) -> Option<operation::GenStamp> {
    let e = model::find_by_name(world, name).expect("op");
    let op = model::operation_of(world, &e).expect("operation");
    operation::current_stamp(world, &op.tool, &op.src, &op.params, name, op.setup.as_deref())
}

/// Re-face Setup1 one millimetre deeper and regenerate it.
fn reface(world: &World, h: &mut History) {
    h.execute(world, EditOperationParams::new(world, "Face1", face(8.0)).expect("edit"));
    h.execute(world, GenerateOperation::new(world, "Face1").expect("regenerate"));
}

#[test]
fn a_setup1_change_restamps_the_carried_setups_ops_whose_stock_grew() {
    // Setup2's job stock grew to the carried body (Z 9 over a declared 5):
    // re-facing Setup1 moves both a non-rest op's and a posed op's stamp.
    let (world, mut h) = program(100.0, 9.0, Hold::SameAs, 100.0);
    posed(&world, &mut h, "Posed2");
    assert!(camcraft_app::rest::carried_digest(&world, "Posed2", Some("Setup2")).is_some());
    let before = (stamp(&world, "Contour2"), stamp(&world, "Posed2"));
    reface(&world, &mut h);
    let after = (stamp(&world, "Contour2"), stamp(&world, "Posed2"));
    assert_ne!(before.0, after.0, "the non-rest op reads the carried body");
    assert_ne!(before.1, after.1, "the posed op judges the stock the carried body spans");
    assert_eq!(operation::status_of(&world, &model::find_by_name(&world, "Contour2").expect("op")), OpStatus::Stale);
}

#[test]
fn single_setup_and_clip_entered_stamps_do_not_read_the_carry() {
    // Clip-entered Setup2: nothing is carried, so re-facing Setup1 moves
    // neither Setup2 stamp, and no carried term exists.
    let (world, mut h) = program(100.0, 9.0, Hold::Clip, 100.0);
    posed(&world, &mut h, "Posed2");
    assert!(camcraft_app::rest::carried_digest(&world, "Contour2", Some("Setup2")).is_none());
    assert!(camcraft_app::rest::carried_digest(&world, "Posed2", Some("Setup2")).is_none());
    let before = (stamp(&world, "Contour2"), stamp(&world, "Posed2"));
    reface(&world, &mut h);
    assert_eq!(before, (stamp(&world, "Contour2"), stamp(&world, "Posed2")));
    assert_eq!(operation::status_of(&world, &model::find_by_name(&world, "Contour2").expect("op")), OpStatus::Ok);
    // One setup: a posed op after Face1 never digests the program ahead,
    // so its stamp is the pre-#319 one whatever Face1 cuts.
    let (one, mut h1) = program(100.0, 9.0, Hold::Clip, 100.0);
    h1.execute(&one, camcraft_app::commands::ActivateSetup::new(&one, Some("Setup1")).expect("activate Setup1"));
    posed(&one, &mut h1, "Posed1");
    assert!(camcraft_app::rest::carried_digest(&one, "Posed1", Some("Setup1")).is_none());
    let before = stamp(&one, "Posed1");
    reface(&one, &mut h1);
    assert_eq!(before, stamp(&one, "Posed1"));
}

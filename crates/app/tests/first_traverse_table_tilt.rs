//! #3100: the first traverse of a 3+2 plane on a machine whose table
//! tilts the work. The post sends Z, then X/Y at that height, then the
//! move with the rotary words (`fanuc.cps` `xyFirst`, `onRapid`). So the
//! X/Y traverse runs with each work-turning rotary where the previous
//! program left it, and the posed law (#3084) reads the work through
//! every such pose.
//!
//! - One rotary A on the table (about X, through the WCS origin): an A 30
//!   face plane over a 60 x 60 x 20 block. At the plane's own pose the
//!   block's top stands 15 over the plane origin; a full turn about A
//!   lifts the top edge (x, 30, 20) to radius sqrt(1300), 18.735 over it.
//!   A clearance 16 over the origin is held by name; 20 passes.
//! - A and B on the table (a trunnion): a B 30 face plane reads the block
//!   on its sphere about the WCS origin; 30 over the origin passes.
//! - A post that declares `rotariesBeforeTraverse` (Tormach PathPilot:
//!   A turns at the safe Z before the X/Y traverse) is read at the
//!   plane's own pose; a user's own script with the declaration is taken
//!   at its word with a note, and without it is held by name.
//!
//! Provenance: our own machine files and fixtures; the shop rule "never
//! traverse through uncut stock or steel" (machine-tool handbooks) and
//! plain rotation geometry. No vendor material.
//!
//! Gate map (#3100): covers post::dialect (turns_rotaries_first), app::operation (first_traverse_posed,
//! generate), sim::verify (first_traverse_posed).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SelectPost, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{Point3, IDENTITY_Q};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

/// The plane origin: the block's top centre (Z 20).
const ORIGIN_Z: f64 = 20.0;

/// Which rotaries turn the work. The head never tilts.
#[derive(Clone, Copy)]
enum Rig {
    /// A about X on the table.
    TableA,
    /// A about X on the table, B about Y on A.
    Trunnion,
}

fn machine_text(rig: Rig) -> String {
    let rot = |id: &str, name: &str, axis: &str, inner: &str| {
        format!(r#"{{ "id": "{id}", "name": "{name}", "type": "rotary", "axis": {axis}, "travel": {{ "min": -120.0, "max": 120.0 }}{inner} }}"#)
    };
    let head = r#"{ "id": "hed", "name": "head", "type": "head", "spindle": { "min_speed": 50.0, "max_speed": 15000.0 } }"#;
    let (table, b) = match rig {
        Rig::TableA => (rot("ra", "A", "[1.0, 0.0, 0.0]", ""), ""),
        Rig::Trunnion => (rot("ra", "A", "[1.0, 0.0, 0.0]", &format!(r#", "parts": [ {} ]"#, rot("rb", "B", "[0.0, 1.0, 0.0]", ""))), r#", "B": { "coordinate": 4 }"#),
    };
    format!(
        r#"{{
  "general": {{ "model": "TEST TABLE TILT", "vendor": "Camcraft", "kind": "mill" }},
  "kinematics": {{ "default": {{
    "units": {{ "length": "mm", "angle": "degrees" }},
    "parts": [
      {{ "id": "base", "name": "table", "type": "table", "parts": [
        {{ "id": "crl", "name": "X", "type": "linear", "travel": {{ "min": -200.0, "max": 200.0 }}, "parts": [
          {{ "id": "sld", "name": "Y", "type": "linear", "travel": {{ "min": -200.0, "max": 200.0 }} }} ] }},
        {{ "id": "col", "name": "Z", "type": "linear", "travel": {{ "min": 0.0, "max": 300.0 }}, "safe_retract": 0.0, "parts": [ {head} ] }},
        {table}
      ] }}
    ] }} }},
  "controller": {{ "synced_configuration": {{ "parts": {{
    "X": {{ "coordinate": 0 }}, "Y": {{ "coordinate": 1 }}, "Z": {{ "coordinate": 2 }},
    "A": {{ "coordinate": 3 }}{b}
  }} }} }},
  "tooling": {{ "number_of_tools": 24 }}
}}"#
    )
}

/// A face plane at A `a_deg` / B `b_deg` through the top centre of a
/// 60 x 60 x 20 block (x, y +-30, z 0..20), its clearance `over` the
/// plane origin; the WCS origin on the rotary lines; the default post.
fn face_on(rig: Rig, a_deg: f64, b_deg: f64, over: f64) -> (World, OpStatus, Vec<Warning>) {
    let (world, status, warns, _) = face_posted(rig, a_deg, b_deg, over, None);
    (world, status, warns)
}

/// [`face_on`] with the setup's post `post` (`None`: the default), and
/// the path's notes.
fn face_posted(rig: Rig, a_deg: f64, b_deg: f64, over: f64, post: Option<PostRef>) -> (World, OpStatus, Vec<Warning>, Vec<String>) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let m = parse_mch(&machine_text(rig)).unwrap_or_else(|e| panic!("test machine: {e}"));
    h.execute(&world, LoadMachine::new(&world, "test-table-tilt.mch".into(), Some(m)).expect("machine"));
    if let Some(p) = post {
        h.execute(&world, SelectPost::new(&world, p).expect("post"));
    }
    h.execute(&world, SetWcsOrigin::new(&world, Location { pos: Point3::ZERO, rot: IDENTITY_Q }).expect("G54 on the lines"));
    let at = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", t).expect("tool"));
    let p = OpParams::Face {
        holes: Default::default(),
        tilt: Some(Tilt { basis_version: 1, a_deg, b_deg, origin: [0.0, 0.0, ORIGIN_Z] }),
        blend_turns: false,
        blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(ORIGIN_Z + over),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, p).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let status = camcraft_app::operation::status_of(&world, &e);
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    let (warns, notes) = (cached.warnings.clone(), cached.path.notes.clone());
    (world, status, warns, notes)
}

fn low(warns: &[Warning]) -> Option<(f64, f64, bool)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseLow { height_mm, top_mm, fixture, .. } => Some((height_mm, top_mm, fixture)),
        _ => None,
    })
}

#[test]
fn a_3_2_plane_under_the_work_a_turning_table_can_leave_turned_up_is_refused_by_name() {
    // The top edge (x, +-30, 20) on its circle about A: radius sqrt(1300);
    // read along the A 30 axis from the plane origin (0, 0, 20), the line
    // stands 20 cos 30 under it.
    let want = 1300f64.sqrt() - ORIGIN_Z * 30f64.to_radians().cos();
    assert!((want - 18.735).abs() < 1e-3, "{want}");
    let (world, status, warns) = face_on(Rig::TableA, 30.0, 0.0, 16.0);
    assert_eq!(status, OpStatus::Ok);
    let (height, top, fixture) = low(&warns).unwrap_or_else(|| panic!("clearance 16: no first-traverse verdict in {warns:?}"));
    // #2991: the heights in the user's frame (local + origin Z 20): the
    // clearance as typed (36), the edge at 38.735, and `tilted`.
    assert!((height - (ORIGIN_Z + 16.0)).abs() < 1e-6, "the post traverses at the clearance the user set: {height}");
    assert!((top - (want + ORIGIN_Z)).abs() < 1e-6 && !fixture, "the stock edge turned up: {top}, want {}", want + ORIGIN_Z);
    assert!(warns.iter().any(|w| matches!(w, Warning::FirstTraverseLow { tilted: true, .. })), "a 3+2 verdict is tilted: {warns:?}");
    let n = warns.iter().filter(|w| matches!(w, Warning::FirstTraverseLow { .. })).count();
    assert_eq!(n, 1, "one verdict names the highest top: {warns:?}");
    // The verdict blocks the post. (This rig states no nose_to_axis, so
    // the index-swing clause refuses the post first, by its own name.)
    assert!(warns.iter().any(|w| matches!(w, Warning::FirstTraverseLow { .. }) && w.blocks_post()), "{warns:?}");
    let err = posting::post_setup_unwritten(&world).expect_err("the program must not post");
    assert!(err.contains("Face1"), "{err}");
}

#[test]
fn a_3_2_plane_over_every_pose_of_a_turning_table_passes() {
    let (_, status, warns) = face_on(Rig::TableA, 30.0, 0.0, 20.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "clearance 20 stands over the turned stock: {warns:?}");
    // The trunnion: the corner (30, 30, 0) or (30, 30, 20) on its sphere,
    // sqrt(2200) from the origin, 29.584 over the plane along B 30.
    let (_, status, warns) = face_on(Rig::Trunnion, 0.0, 30.0, 30.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "clearance 30 stands over the sphere: {warns:?}");
}

#[test]
fn a_post_that_turns_the_table_first_reads_the_plane_at_its_own_pose() {
    // Tormach PathPilot declares rotariesBeforeTraverse: it turns A at the
    // safe Z before the X/Y traverse (posting_rotaries_first.rs). The
    // same plane, 16 over the origin, is judged at its own pose (top 15)
    // and has no first-traverse verdict; a shipped post gives no note.
    let tormach = Some(PostRef::Named("Tormach PathPilot".into()));
    let (_, status, warns, notes) = face_posted(Rig::TableA, 30.0, 0.0, 16.0, tormach);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "{warns:?}");
    assert!(!notes.iter().any(|n| n.contains("rotariesBeforeTraverse")), "{notes:?}");
}

#[test]
fn a_users_own_post_is_taken_at_its_declared_order_with_a_note() {
    // A user's script with the declaration: taken at its word, said on the
    // path. The same script without it reads every pose: held by name.
    let text = posting::builtin_post_text("Tormach PathPilot").expect("tormach.cps");
    let dir = std::env::temp_dir().join(format!("camcraft_3100_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let declared = dir.join("declared.cps");
    std::fs::write(&declared, text).expect("write the declared script");
    let plain = dir.join("plain.cps");
    std::fs::write(&plain, text.replace("rotariesBeforeTraverse = true;", "")).expect("write the plain script");
    let file = |p: &std::path::Path| Some(PostRef::File(p.to_string_lossy().into_owned()));
    let (_, status, warns, notes) = face_posted(Rig::TableA, 30.0, 0.0, 16.0, file(&declared));
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "{warns:?}");
    assert!(notes.iter().any(|n| n.contains("declares rotariesBeforeTraverse") && n.contains("taken at its word")), "{notes:?}");
    let (_, _, warns, notes) = face_posted(Rig::TableA, 30.0, 0.0, 16.0, file(&plain));
    let _ = std::fs::remove_dir_all(&dir);
    let (_, top, _) = low(&warns).unwrap_or_else(|| panic!("the plain script reads every pose: {warns:?}"));
    // #2991: in the user's frame, as the refused case above.
    assert!((top - (1300f64.sqrt() - ORIGIN_Z * 30f64.to_radians().cos() + ORIGIN_Z)).abs() < 1e-6, "{top}");
    assert!(!notes.iter().any(|n| n.contains("rotariesBeforeTraverse")), "{notes:?}");
}

#[test]
fn a_switch_to_a_users_declaring_post_reads_stale_and_regenerates_with_the_note() {
    // Generated under Tormach PathPilot (no note), then the setup takes a
    // user's copy of the same script: the cache must not stay fresh, or
    // the post would run without the note that says the order was taken
    // at its word.
    let tormach = Some(PostRef::Named("Tormach PathPilot".into()));
    let (world, status, _, notes) = face_posted(Rig::TableA, 30.0, 0.0, 16.0, tormach);
    assert_eq!(status, OpStatus::Ok);
    assert!(!notes.iter().any(|n| n.contains("rotariesBeforeTraverse")), "{notes:?}");
    let text = posting::builtin_post_text("Tormach PathPilot").expect("tormach.cps");
    let path = std::env::temp_dir().join(format!("camcraft_3100_switch_{}.cps", std::process::id()));
    std::fs::write(&path, text).expect("write the user's script");
    let mut h = History::default();
    h.execute(&world, SelectPost::new(&world, PostRef::File(path.to_string_lossy().into_owned())).expect("post"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&world, &e), OpStatus::Stale, "the cache judged under another post");
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let status = camcraft_app::operation::status_of(&world, &e);
    let notes = model::toolpath_of(&world, &e).expect("cached path").path.notes.clone();
    let _ = std::fs::remove_file(&path);
    assert_eq!(status, OpStatus::Ok);
    assert!(notes.iter().any(|n| n.contains("declares rotariesBeforeTraverse")), "{notes:?}");
}

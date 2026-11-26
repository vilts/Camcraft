//! #315: a rotated work offset must never post silently. The post
//! stream carries the offset's translation only (OriginX/Y/Z), so a
//! G54 turned 90 deg about Z would post world-axis coordinates - a
//! program turned against the part. The gate refuses it by name; an
//! unrotated offset (identity within the gate's tolerance) posts
//! byte-identical to before.
//! Provenance: our own fixtures over the translation-only G54 law.
//!
//! #327: a non-finite (or zero-norm) offset placement is refused by name
//! at command creation, at project load and in the post gate - never
//! posted as "unrotated" - and the setup sheet states a rotation.
//!
//! Gate map (#315, #327): covers app::posting (verification_report,
//! wcs_context, stream_records, wcs_rotation), app::commands (SetWcsOrigin,
//! CreateSetup), app::project (save, open, validate), app::setup_sheet.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::*;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupWorkOffset, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, quat_from_euler};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::{model, project, setup_sheet};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// A 60 x 60 x 20 box faced at Z 19.5, generated, G54 not yet placed.
fn faced_block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, h)
}

fn place_g54(world: &World, h: &mut History, rot: [f64; 4]) {
    let loc = Location { pos: Point3::new(10.0, 5.0, 1.0), rot };
    h.execute(world, SetWcsOrigin::new(world, loc).expect("place G54"));
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-rotwcs-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn yaw90() -> [f64; 4] {
    quat_from_euler(0.0, 0.0, 90f64.to_radians())
}

fn roll30() -> [f64; 4] {
    quat_from_euler(30f64.to_radians(), 0.0, 0.0)
}

#[test]
fn the_post_stream_carries_z_offset_rotation() {
    // #760: a pure Z-rotated G54 carries its WcsYaw and Bake mode in the stream.
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    let square = posting::stream_records(&world).expect("stream");
    place_g54(&world, &mut h, yaw90());
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let turned = posting::stream_records(&world).expect("stream");
    let text = camcraft_app::post::cnc::serialize(&turned);
    assert!(text.contains("PARAM WcsYaw 90.000000"), "{text}");
    assert!(text.contains("PARAM WcsRotationMode bake"), "{text}");
    assert_ne!(format!("{square:?}"), format!("{turned:?}"));
}

#[test]
fn a_tilted_work_offset_is_refused_by_name() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, roll30());
    let path = tmp_nc("refused");
    let err = posting::post_setup(&world, &path).expect_err("a tilted G54 must not post");
    assert!(!path.exists(), "a refusal leaves no file");
    assert!(
        err.contains("the WCS (G54) is rotated 30.000 deg about X")
            && err.contains("posting a tilted work offset is not supported")
            && err.contains("align WCS Z to machine +Z (use Part alignment on the Setup page)")
            && err.contains("(operations: Face1)"),
        "named refusal, got: {err}"
    );
    let report = posting::verification_report(&world);
    assert_eq!(report.problems.len(), 2, "both live-source and offset refusals: {:?}", report.problems);
    assert_eq!(
        report.problems.iter().filter(|problem| {
            problem.contains("the WCS (G54) is rotated 30.000 deg about X")
                && problem.contains("(operations: Face1)")
        }).count(),
        1,
        "one named offset refusal: {:?}", report.problems
    );
    assert_eq!(
        report.problems.iter().filter(|problem| {
            problem.contains("Face1: the parts beside its source cannot be read again")
                && problem.contains("the WCS (G54) has its Z axis tilted 30.000 deg")
                && problem.contains("regenerate it")
        }).count(),
        1,
        "main's live-source check still refuses the tilted WCS: {:?}", report.problems
    );
}

#[test]
fn a_setups_rotated_offset_names_the_setup() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    place_g54(&world, &mut h, quat_from_euler(30f64.to_radians(), 0.0, 0.0));
    let err = posting::post_setup(&world, &tmp_nc("setup")).expect_err("refused");
    assert!(
        err.contains("setup Setup1: its WCS (G54) is rotated 30.000 deg about X"),
        "names the setup and the axis, got: {err}"
    );
}

#[test]
fn an_unrotated_offset_posts_exactly_as_before() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    let path = tmp_nc("identity");
    posting::post_setup(&world, &path).expect("an identity G54 posts");
    let exact = std::fs::read_to_string(&path).expect("written");
    // Float noise from a normalized round trip is still the identity.
    let noisy = quat_from_euler(1e-13, -1e-13, 1e-13);
    assert_ne!(noisy, IDENTITY_Q, "the fixture really is off-identity by noise");
    place_g54(&world, &mut h, noisy);
    posting::post_setup(&world, &path).expect("noise-level rotation posts");
    let again = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(exact, again, "byte-identical program");
    // Undo back to a tilted offset refuses again; undo to identity posts.
    place_g54(&world, &mut h, roll30());
    assert!(posting::post_setup(&world, &path).is_err());
    assert!(h.undo(&world));
    posting::post_setup(&world, &path).expect("undo restores the postable offset");
    let _ = std::fs::remove_file(&path);
}

/// Write a placement straight onto G54, past the command and the
/// transform chokepoint - the only way a NaN rotation can reach the
/// world, which is exactly what the post gate must still refuse.
fn poke_g54(world: &World, loc: Location) {
    let e = model::find_by_name(world, "G54").expect("G54 placed");
    e.entity_view(world).set(loc);
}

#[test]
fn set_wcs_origin_refuses_a_non_finite_placement_by_its_label() {
    // #342: the one WCS of the active setup, labelled G55 - the refusal
    // names it by its label, "WCS (G55)".
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(1)).expect("label G55"));
    let cases = [
        ([f64::NAN, 0.0, 0.0, 1.0], Point3::new(0.0, 0.0, 0.0), "rotation must be finite"),
        ([0.0, f64::INFINITY, 0.0, 1.0], Point3::new(0.0, 0.0, 0.0), "rotation must be finite"),
        ([0.0, 0.0, 0.0, 0.0], Point3::new(0.0, 0.0, 0.0), "rotation is degenerate"),
        (IDENTITY_Q, Point3::new(f64::NAN, 0.0, 0.0), "origin must be finite"),
        (IDENTITY_Q, Point3::new(0.0, 0.0, f64::NEG_INFINITY), "origin must be finite"),
    ];
    for (rot, pos, why) in cases {
        let err = SetWcsOrigin::new(&world, Location { pos, rot })
            .err()
            .expect("a non-finite placement must be refused");
        let msg = err.to_string();
        assert!(msg.contains("WCS (G55)") && msg.contains(why), "{rot:?} {pos:?}: {msg}");
    }
}

#[test]
fn a_non_finite_offset_rotation_is_refused_by_the_post_gate() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    for rot in [[f64::NAN, 0.0, 0.0, 1.0], [0.0, 0.0, f64::INFINITY, 1.0], [0.0; 4]] {
        poke_g54(&world, Location { pos: Point3::new(10.0, 5.0, 1.0), rot });
        let path = tmp_nc("nan-rot");
        let err = posting::post_setup(&world, &path).expect_err("a NaN rotation must not post");
        assert!(!path.exists(), "a refusal leaves no file");
        assert!(
            err.contains("the WCS (G54) has a non-finite placement (origin or rotation)")
                && err.contains("(operations: Face1)"),
            "{rot:?}: named refusal, got: {err}"
        );
    }
}

#[test]
fn a_non_finite_offset_origin_is_refused_by_the_post_gate() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    poke_g54(&world, Location { pos: Point3::new(f64::INFINITY, 5.0, 1.0), rot: IDENTITY_Q });
    let report = posting::verification_report(&world);
    assert!(
        report.problems.iter().any(|p| p.contains("the WCS (G54) has a non-finite placement")),
        "named refusal, got: {:?}",
        report.problems
    );
}

#[test]
fn project_load_refuses_a_degenerate_offset_rotation() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, yaw90());
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-rotwcs-load-{}.json", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let entities = doc["entities"].as_array_mut().expect("entities");
    let g54 = entities.iter_mut().find(|e| e["name"] == "G54").expect("G54 row");
    g54["rotation"] = serde_json::json!([0.0, 0.0, 0.0, 0.0]);
    let bad = serde_json::to_string(&doc).expect("json out");
    let fresh = new_world();
    let err = project::open(&fresh, &bad).expect_err("a zero-norm offset rotation must not load");
    assert!(
        err.to_string().contains("WCS 'G54' has a non-finite or degenerate placement"),
        "named refusal, got: {err}"
    );
    // The unedited file still loads: the check is the offset's, not a
    // blanket refusal of rotated offsets.
    project::open(&new_world(), &text).expect("the saved rotated offset loads");
}

#[test]
fn the_setup_sheet_states_the_offset_rotation() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    let square = setup_sheet::render(&world, None).expect("sheet");
    assert!(!square.contains("rotated"), "an unrotated offset adds no line:\n{square}");
    place_g54(&world, &mut h, yaw90());
    let turned = setup_sheet::render(&world, None).expect("sheet");
    assert!(
        turned.contains("rotated 90.000 deg about Z (baked into XY)"),
        "the rotation is on the sheet:\n{turned}"
    );
    poke_g54(&world, Location { pos: Point3::new(10.0, 5.0, 1.0), rot: [f64::NAN, 0.0, 0.0, 1.0] });
    let nan = setup_sheet::render(&world, None).expect("sheet");
    assert!(nan.contains("placement is not finite - the post refuses this offset"), "{nan}");
    assert!(!nan.contains("NaN"), "no Debug floats:\n{nan}");
}

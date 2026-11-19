//! #1649: what generation and the post do with a flipped WCS axis. The
//! cut does not change; the post mirrors the program on a flipped X or Y
//! (signs, G2/G3, the compensation side), and what it does not carry (a
//! flipped Z, a flipped and rotated WCS, a post not verified for a mirror)
//! is refused by name. Probing in a flipped setup is refused by name (the
//! maintainer's decision of 2026-11-18). Clearing the flip posts the same
//! program as before.
//! The travel check and the setup sheet follow the flip.
//! Provenance: our own fixtures over our own WCS model.
//!
//! Gate map (#1649): covers app::posting (verification_gate, post_setup,
//! post_setup_unwritten, stream_records), app::setup_wcs_flip,
//! app::operation (collect_job), app::machine (check_document),
//! app::setup_sheet, app::machine (work_envelope).
//! Lane shape (M4.4): all tests here run in seconds.

mod wcs_flip_support;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, CreateSetup, GenerateOperation, SetStock};
use camcraft_app::operation::SrcRef;
use camcraft_app::stock::StockShape;
use camcraft_app::{model, posting, setup_wcs_flip};
use wcs_flip_support::{TOOL, add_probe, face, faced_block, flip};

/// The G, X, Y, Z, I, J words of each program line that has one, in order.
fn words(program: &str) -> Vec<Vec<(char, f64)>> {
    program
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('(') && !l.starts_with('%') && !l.starts_with('O'))
        .map(|l| {
            l.split_whitespace()
                .filter_map(|w| {
                    let mut ch = w.chars();
                    let c = ch.next()?;
                    let v = ch.as_str().parse::<f64>().ok()?;
                    "GXYZIJ".contains(c).then_some((c, v))
                })
                .collect::<Vec<_>>()
        })
        .filter(|w: &Vec<(char, f64)>| !w.is_empty())
        .collect()
}

/// The plain program's words as a controller with `flip` must read them:
/// X/I and Y/J negate on a flipped axis, and G2 <-> G3 when exactly one
/// of X and Y is flipped (the fixture's arcs are all in the XY plane).
fn mirrored(plain: &[Vec<(char, f64)>], flip: [bool; 2]) -> Vec<Vec<(char, f64)>> {
    let neg = |v: f64| if v == 0.0 { 0.0 } else { -v };
    plain
        .iter()
        .map(|line| {
            line.iter()
                .map(|&(c, v)| match c {
                    'X' | 'I' if flip[0] => (c, neg(v)),
                    'Y' | 'J' if flip[1] => (c, neg(v)),
                    'G' if flip[0] != flip[1] && v == 2.0 => (c, 3.0),
                    'G' if flip[0] != flip[1] && v == 3.0 => (c, 2.0),
                    _ => (c, v),
                })
                .collect()
        })
        .collect()
}

fn program(world: &World) -> String {
    posting::post_setup_unwritten(world).unwrap_or_else(|e| panic!("must post: {e}")).program
}

#[test]
fn a_flipped_x_or_y_posts_the_mirrored_program_and_clearing_it_posts_as_before() {
    let (world, mut h) = faced_block();
    let before = posting::stream_records(&world).expect("stream");
    let plain = program(&world);
    let base = words(&plain);
    let arcs = |w: &[Vec<(char, f64)>], g: f64| w.iter().filter(|l| l.contains(&('G', g))).count();
    assert!(arcs(&base, 2.0) + arcs(&base, 3.0) > 0, "the fixture posts arcs:\n{plain}");
    assert!(base.iter().flatten().any(|&(c, v)| c == 'X' && v != 0.0), "and X words:\n{plain}");
    for (f, note) in [
        ([true, false, false], "(WCS X FLIPPED - PROGRAM MIRRORED)"),
        ([false, true, false], "(WCS Y FLIPPED - PROGRAM MIRRORED)"),
        ([true, true, false], "(WCS X AND Y FLIPPED - PROGRAM MIRRORED)"),
    ] {
        flip(&world, &mut h, None, f);
        assert_eq!(posting::verification_gate(&world), Vec::<String>::new(), "{f:?}: the flip is carried");
        let got = program(&world);
        assert!(got.contains(note), "{f:?}: the program names the flip:\n{got}");
        assert_eq!(words(&got), mirrored(&base, [f[0], f[1]]), "{f:?}: every word mirrors:\n{got}");
        assert_ne!(words(&got), base, "{f:?}: and the program is not the plain one");
    }
    flip(&world, &mut h, None, [false; 3]);
    let after = posting::stream_records(&world).expect("stream");
    assert_eq!(format!("{before:?}"), format!("{after:?}"), "the same program as before the flip");
    assert_eq!(program(&world), plain);
}

#[test]
fn a_flipped_z_refuses_the_post_by_name() {
    let (world, mut h) = faced_block();
    assert!(posting::verification_gate(&world).is_empty(), "the fixture posts");
    flip(&world, &mut h, None, [false, false, true]);
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|p| p.contains("the WCS (G54) has its Z axis flipped")
            && p.contains("a flipped Z is not carried into the program yet")
            && p.contains("(operations: Face1)")),
        "named refusal, got: {gate:?}"
    );
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-wcsflip-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(posting::post_setup(&world, &path).is_err(), "a flipped Z must not post");
    assert!(!path.exists(), "a refusal leaves no file");
    flip(&world, &mut h, None, [true, true, true]);
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p.contains("its X, Y and Z axes flipped")), "{gate:?}");
    flip(&world, &mut h, None, [false; 3]);
    assert!(posting::verification_gate(&world).is_empty(), "a cleared flip posts again");
}

#[test]
fn a_flipped_and_rotated_wcs_and_an_unverified_post_refuse_by_name() {
    use camcraft_app::commands::{SelectPost, SetWcsOrigin};
    use camcraft_app::schema::{Location, PostRef};
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [true, false, false]);
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Mazak".into())).expect("Mazak"));
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|p| p
            .contains("has its X axis flipped, and its post (Mazak (built-in)) is not verified for a mirrored program")),
        "{gate:?}"
    );
    assert!(posting::post_setup_unwritten(&world).is_err(), "an unverified post must not post a flipped WCS");
    assert!(h.undo(&world));
    assert!(posting::verification_gate(&world).is_empty(), "the builtin post carries the flip");

    let yaw = camcraft_app::geometry::quat_from_euler(0.0, 0.0, 30f64.to_radians());
    let at = Location { pos: camcraft_app::geometry::Point3::new(0.0, 0.0, 0.0), rot: yaw };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("rotate G54"));
    assert_eq!(setup_wcs_flip::of_setup(&world, None), [true, false, false], "the flip rides the move");
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|p| p.contains("has its X axis flipped and is also rotated")
            && p.contains("a flipped and rotated WCS is not supported yet")),
        "{gate:?}"
    );
    assert!(posting::post_setup_unwritten(&world).is_err(), "a flipped and rotated WCS must not post");
}

#[test]
fn generation_runs_in_a_flipped_setup_and_the_toolpath_does_not_change() {
    let (world, mut h) = faced_block();
    let op = model::find_by_name(&world, "Face1").expect("Face1");
    let before = format!("{:?}", model::toolpath_of(&world, &op).expect("toolpath").path.moves);
    for f in [[true, false, false], [true, true, false], [false, false, true]] {
        flip(&world, &mut h, None, f);
        h.execute(&world, GenerateOperation::new(&world, "Face1").unwrap_or_else(|e| panic!("{f:?} generates: {e}")));
        let after = format!("{:?}", model::toolpath_of(&world, &op).expect("toolpath").path.moves);
        assert_eq!(before, after, "{f:?}: the cut is the same; only the post mirrors");
    }
}

#[test]
fn a_setups_flip_names_the_setup_and_leaves_the_other_setup_alone() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    flip(&world, &mut h, Some("Setup2"), [false, false, true]);
    assert_eq!(setup_wcs_flip::of_setup(&world, Some("Setup1")), [false; 3]);
    assert!(posting::verification_gate(&world).is_empty(), "no posted op runs in Setup2");
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("Setup2 stock"));
    h.execute(&world, CreateOperation::new(&world, "Face2", TOOL, SrcRef::Stock, face()).expect("Face2"));
    h.execute(&world, GenerateOperation::new(&world, "Face2").expect("a flipped setup generates"));
    let gate = posting::verification_gate(&world);
    let hit = gate.iter().find(|p| p.contains("flipped")).unwrap_or_else(|| panic!("named refusal, got {gate:?}"));
    assert!(hit.contains("setup 'Setup2': its WCS") && hit.contains("has its Z axis flipped"), "{hit}");
    assert!(hit.contains("(operations: Face2)") && !hit.contains("Face1"), "{hit}");
}

#[test]
fn the_travel_check_reads_the_posted_sign_of_a_flipped_axis() {
    use camcraft_app::machine::{AxisLimits, Machine, check_document};
    let (world, mut h) = faced_block();
    let machine = |x: (f64, f64)| Machine {
        model: "FlipMill".into(),
        axes: vec![
            AxisLimits { name: "X".into(), travel: Some(x), ..Default::default() },
            AxisLimits { name: "Y".into(), travel: Some((-500.0, 500.0)), ..Default::default() },
            AxisLimits { name: "Z".into(), travel: Some((-500.0, 500.0)), ..Default::default() },
        ],
        ..Default::default()
    };
    // The stock is centred on the world origin (X -30..30, the facing
    // runs past it). With the work origin at X -40 the plain program stays
    // on the + side and the flipped one on the - side.
    use camcraft_app::commands::SetWcsOrigin;
    use camcraft_app::schema::Location;
    let at = Location {
        pos: camcraft_app::geometry::Point3::new(-40.0, 0.0, 0.0),
        rot: camcraft_app::geometry::quat_from_euler(0.0, 0.0, 0.0),
    };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("move G54"));
    let (plus, minus) = (machine((0.0, 500.0)), machine((-500.0, 0.0)));
    let c = check_document(&world, &plus).expect("check");
    assert!(c.is_ok(), "plain fits the + stroke: {:?}", c.problems);
    assert!(!check_document(&world, &minus).expect("check").is_ok(), "plain overruns the - stroke");
    flip(&world, &mut h, None, [true, false, false]);
    let c = check_document(&world, &plus).expect("check");
    assert!(c.problems.iter().any(|p| p.contains("in X") && p.contains("reaches -")), "{:?}", c.problems);
    let c = check_document(&world, &minus).expect("check");
    assert!(c.is_ok(), "the mirrored program fits the - stroke: {:?}", c.problems);
}

#[test]
fn the_setup_sheet_names_the_flipped_axes() {
    let (world, mut h) = faced_block();
    let plain = camcraft_app::setup_sheet::render(&world, None).expect("sheet");
    assert!(!plain.contains("flipped"), "{plain}");
    flip(&world, &mut h, None, [true, false, false]);
    let sheet = camcraft_app::setup_sheet::render(&world, None).expect("sheet");
    assert!(sheet.contains("X axis flipped - the program is mirrored on it"), "{sheet}");
    flip(&world, &mut h, None, [true, true, false]);
    let sheet = camcraft_app::setup_sheet::render(&world, None).expect("sheet");
    assert!(sheet.contains("X and Y axes flipped - the program is mirrored on them"), "{sheet}");
}

#[test]
fn the_work_envelope_mirrors_about_the_origin_on_a_flipped_axis() {
    use camcraft_app::machine::{self, AxisLimits, Machine, envelope::EnvelopeShape};
    let (world, mut h) = faced_block();
    let ax = |name: &str, t: (f64, f64)| AxisLimits { name: name.into(), travel: Some(t), ..Default::default() };
    let m = Machine {
        model: "FlipMill".into(),
        axes: vec![ax("X", (-100.0, 200.0)), ax("Y", (-50.0, 80.0)), ax("Z", (-30.0, 0.0))],
        ..Default::default()
    };
    machine::set(&world, "flip.mch", Some(&m));
    let boxed = |w: &World| match machine::work_envelope(w) {
        Ok(EnvelopeShape::Box { min, max }) => (min, max),
        other => panic!("a mill box: {other:?}"),
    };
    assert_eq!(boxed(&world), ([-100.0, -50.0, -30.0], [200.0, 80.0, 0.0]));
    flip(&world, &mut h, None, [true, false, false]);
    assert_eq!(boxed(&world), ([-200.0, -50.0, -30.0], [100.0, 80.0, 0.0]), "X+ runs to the world's -X");
    flip(&world, &mut h, None, [true, true, false]);
    assert_eq!(boxed(&world), ([-200.0, -80.0, -30.0], [100.0, 50.0, 0.0]));
}

/// #1649, the maintainer's decision of 2026-11-18: a probe in a flipped
/// setup is refused before the post, by name, with the flipped axis; the
/// facing beside it is not named. The MCP flip tool reports it too.
#[test]
fn probing_in_a_flipped_setup_refuses_the_post_by_name() {
    let (world, mut h) = faced_block();
    add_probe(&world, &mut h);
    assert!(posting::verification_gate(&world).is_empty(), "the fixture posts");
    for (f, named) in [([true, false, false], "has its X axis flipped"), ([true, true, false], "has its X and Y axes flipped")] {
        flip(&world, &mut h, None, f);
        let gate = posting::verification_gate(&world);
        let hit = gate.iter().find(|p| p.contains("probing")).unwrap_or_else(|| panic!("{f:?}: named refusal, got {gate:?}"));
        assert!(hit.contains(&format!("the WCS (G54) {named}; probing does not post in a flipped WCS")), "{hit}");
        assert!(hit.contains("(operations: Probe1)") && !hit.contains("Face1"), "{hit}");
        assert!(posting::post_setup_unwritten(&world).is_err(), "{f:?}: a probe in a flipped setup must not post");
    }
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-wcsflip-probe-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(posting::post_setup(&world, &path).is_err(), "a probe in a flipped setup must not post");
    assert!(!path.exists(), "a refusal leaves no file");
    let refused = setup_wcs_flip::setup_post_problems(&world, None);
    assert!(refused.iter().any(|p| p.contains("probing does not post")), "{refused:?}");
}

/// The same probe with no flip posts as before #1649: it sets G54, and a
/// flip set and cleared again leaves the program as it was.
#[test]
fn probing_in_an_unflipped_setup_posts_unchanged() {
    let (world, mut h) = faced_block();
    add_probe(&world, &mut h);
    let before = program(&world);
    assert!(before.contains("G10 L2 P1"), "the probe sets G54:
{before}");
    assert!(!before.contains("FLIPPED"), "{before}");
    flip(&world, &mut h, None, [true, false, false]);
    flip(&world, &mut h, None, [false; 3]);
    assert!(posting::verification_gate(&world).is_empty());
    assert_eq!(program(&world), before);
}

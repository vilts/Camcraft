//! Combined WCS XY flips and Z rotations reach the app's post door (#2476).
//! Provenance: our own ECS fixtures and frame-composition examples.
//! Gate map (#2476): covers app::setup_wcs_flip, app::posting,
//! app::machine (check_document), app::setup_sheet.

mod wcs_flip_support;

use camcraft_app::commands::{GenerateOperation, History, LoadMachine, SetPostOptions, SetWcsOrigin};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::machine::{AxisLimits, Machine, check_document};
use camcraft_app::schema::{Location, PostOptions, WcsRotationOutput};
use camcraft_app::{posting, setup_sheet};
use wcs_flip_support::{faced_block, flip};

fn mode(world: &flecs_ecs::prelude::World, h: &mut History, wcs_rotation: WcsRotationOutput) {
    let next = PostOptions { wcs_rotation, ..Default::default() };
    if camcraft_app::setup_post_options::document(world) != next {
        h.execute(world, SetPostOptions::new(world, next).expect("set rotation mode"));
    }
}

fn machine(x: (f64, f64), y: (f64, f64)) -> Machine {
    let axis = |name: &str, travel| AxisLimits { name: name.into(), travel: Some(travel), ..Default::default() };
    Machine {
        model: "FlipRotationMill".into(),
        axes: vec![axis("X", x), axis("Y", y), axis("Z", (-500.0, 500.0))],
        coordinate_rotation: Some(true),
        ..Default::default()
    }
}

#[test]
fn flipped_yaw_posts_in_bake_and_control_and_preserves_the_viewer_path() {
    let (world, mut h) = faced_block();
    let at = Location { pos: Point3::new(10.0, 5.0, 20.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("yaw G54"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate in yaw setup"));
    h.execute(&world, LoadMachine::new(&world, "rotation.mch".into(), Some(machine((-500.0, 500.0), (-500.0, 500.0)))).expect("rotation machine"));
    for f in [[true, false, false], [false, true, false], [true, true, false]] {
        flip(&world, &mut h, None, f);
        mode(&world, &mut h, WcsRotationOutput::Bake);
        assert!(posting::verification_gate(&world).is_empty(), "{f:?}: {:?}", posting::verification_gate(&world));
        let bake = posting::post_setup_unwritten(&world).expect("flipped yaw Bake posts");
        assert!(!bake.program.contains("G68"), "{}", bake.program);
        let sheet = setup_sheet::render(&world, None).expect("Bake setup sheet");
        assert!(sheet.contains("rotated 30.000 deg about Z (baked into XY)"), "{sheet}");
        assert!(sheet.contains("flipped - the program is mirrored"), "{sheet}");
        mode(&world, &mut h, WcsRotationOutput::Control);
        let control = posting::post_setup_unwritten(&world).expect("flipped yaw Control posts");
        let angle = if f[0] != f[1] { "R-30." } else { "R30." };
        assert!(control.program.contains(&format!("G68 X0. Y0. {angle}")), "{}", control.program);
        assert!(control.program.contains("G69"), "{}", control.program);
        let bake_points: Vec<_> = bake.line_pos.iter().flatten().copied().collect();
        let control_points: Vec<_> = control.line_pos.iter().flatten().copied().collect();
        assert!(!bake_points.is_empty());
        assert_eq!(control_points, bake_points, "the viewer stays in Setup coordinates");
        let sheet = setup_sheet::render(&world, None).expect("setup sheet");
        assert!(sheet.contains("rotated 30.000 deg about Z (control rotation G68 / ROT)"), "{sheet}");
        assert!(sheet.contains("flipped - the program is mirrored"), "{sheet}");
    }
}

#[test]
fn flipped_yaw_travel_checks_the_mirrored_table_path_in_both_modes() {
    let (world, mut h) = faced_block();
    let at = Location { pos: Point3::new(-100.0, -100.0, 0.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("yaw G54"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    for output_mode in [WcsRotationOutput::Bake, WcsRotationOutput::Control] {
        mode(&world, &mut h, output_mode);
        for f in [[true, false, false], [false, true, false], [true, true, false]] {
            flip(&world, &mut h, None, f);
            let stroke = |on| if on { (-500.0, 0.0) } else { (0.0, 500.0) };
            let fits = machine(stroke(f[0]), stroke(f[1]));
            h.execute(&world, LoadMachine::new(&world, "rotation.mch".into(), Some(fits.clone())).expect("machine"));
            assert!(posting::verification_gate(&world).is_empty(), "combined setup must reach the travel check: {:?}", posting::verification_gate(&world));
            let check = check_document(&world, &fits).expect("travel check");
            assert!(check.is_ok(), "{f:?} {output_mode:?}: {:?}", check.problems);
            let wrong = machine(stroke(!f[0]), stroke(!f[1]));
            let check = check_document(&world, &wrong).expect("wrong stroke check");
            assert!(!check.is_ok(), "{f:?} {output_mode:?}: the opposite stroke must refuse");
        }
    }
}

#[test]
fn a_flipped_non_z_rotation_still_refuses_by_name() {
    let (world, mut h) = faced_block();
    flip(&world, &mut h, None, [true, false, false]);
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: quat_from_euler(15f64.to_radians(), 0.0, 0.0) };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("tilted WCS"));
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p.contains("only a finite Z rotation is supported with flipped axes")), "{gate:?}");
    assert!(posting::post_setup_unwritten(&world).is_err(), "a tilt must stay refused");
}

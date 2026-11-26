//! The machining mesh's parity laws for the STEP switch (#917): positions
//! snapped off their last-bit noise and never moved past the 1e-9 mm
//! window, a solid whose faces all point inward turned outward and said
//! (one shell only: a solid with a void is refused),
//! and the fillet fixture meshed at the app chord (adaptive refinement of
//! the deviation bound).
//! Provenance: our own test design; fixtures are the repo's own.
//!
//! Gate map (#917): covers camcraft_kernel::mesh_weld,
//! camcraft_kernel::machining_mesh, camcraft_kernel::mesh_deviation.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::{MeshRefusal, machining_mesh};
use camcraft_kernel::mesh_weld::{SNAP_WINDOW_MM, shells, snap, turn_outward};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepSolid, import_step};
use solidcore::kernel::CancelToken;

/// The app's chord cap (#689), mm.
const TOL: f64 = 0.005;

fn text(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_default()
}

fn solid_of(file: &str, text: &str) -> StepSolid {
    let mut k = SolidcoreKernel::default();
    let mut import = match import_step(&mut k, text.as_bytes(), &CancelToken::new()) {
        Ok(i) => i,
        Err(e) => panic!("{file}: {e}"),
    };
    assert_eq!(import.solids.len(), 1, "{file}: one solid expected");
    import.solids.remove(0)
}

#[test]
fn the_snap_takes_the_last_bit_noise_off_a_grid_value() {
    for (noisy, exact) in [
        (20.000000000000004, 20.0_f64),
        (19.999999999999996, 20.0),
        (-5.000000000000001, -5.0),
        (14.999999999999998, 15.0),
        (0.1 + 0.2, 0.3),
        (-1e-17, 0.0),
        (12.5 + 1e-10, 12.5),
    ] {
        assert_eq!(snap(noisy).to_bits(), exact.to_bits(), "{noisy:e} -> {exact}");
    }
    // A grid value stays itself, and the result is +0.0, never -0.0.
    for exact in [0.0_f64, 1.0, 0.001, 123.456, -98.765] {
        assert_eq!(snap(exact).to_bits(), exact.to_bits(), "{exact}");
    }
    assert_eq!(snap(-0.0).to_bits(), 0.0_f64.to_bits());
}

#[test]
fn a_coordinate_outside_the_window_is_never_moved() {
    for c in [
        12.5 + 1e-6,
        12.5 - 1e-6,
        20.0 + 2.0 * SNAP_WINDOW_MM,
        12.499_999_9,
        std::f64::consts::PI,
        12.507_3,
        f64::NAN,
        f64::INFINITY,
        1e300,
    ] {
        assert_eq!(snap(c).to_bits(), c.to_bits(), "{c:e} must stay");
    }
}

#[test]
fn the_bore_mesh_stands_on_its_exact_levels() {
    let s = solid_of("Milling-test.step", &text("Milling-test.step"));
    let m = machining_mesh(&s, TOL, &CancelToken::new()).unwrap_or_else(|e| panic!("{e}"));
    let zs: Vec<f64> = m.positions.iter().map(|p| p[2]).collect();
    let top = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let bottom = zs.iter().copied().fold(f64::INFINITY, f64::min);
    assert_eq!((top, bottom), (20.0, -5.0), "no last-bit noise on the levels");
    for z in [0.0, 20.0] {
        assert!(zs.contains(&z), "a vertex stands exactly on Z {z}");
    }
    assert_eq!(m.report.open_sides, 0);
}

/// box.step with every face and bound sense turned over: the same closed
/// box, all faces pointing inward.
fn inward(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.contains("FACE_SURFACE(") || l.contains("FACE_BOUND(") {
                l.replace(".T.)", ".X.)")
                    .replace(".F.)", ".T.)")
                    .replace(".X.)", ".F.)")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_inward_solid_is_turned_outward_and_said() {
    let plain = solid_of("box.step", &text("box.step"));
    let good = machining_mesh(&plain, TOL, &CancelToken::new()).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !good.report.notes.iter().any(|n| n.contains("inward")),
        "an outward box says nothing: {:?}",
        good.report.notes
    );
    assert!(!good.report.turned_outward, "an outward box is not turned");
    let turned = inward(&text("box.step"));
    assert_ne!(turned, text("box.step"), "the senses were turned");
    let s = solid_of("box.step inward", &turned);
    let m = machining_mesh(&s, TOL, &CancelToken::new())
        .unwrap_or_else(|e| panic!("an inward closed box is the same box: {e}"));
    assert!(m.report.volume > 0.0);
    assert!((m.report.volume - good.report.volume).abs() <= good.report.volume * 1e-9);
    // #917: the mesh says the turn as a flag; the STEP import gathers the
    // flags of all solids into one note (#1617 wording), so the kernel
    // adds no note of its own.
    assert!(m.report.turned_outward, "the turn is said: {:?}", m.report.notes);
    assert!(
        !m.report.notes.iter().any(|n| n.contains("inward")),
        "said once, by the import, not twice: {:?}",
        m.report.notes
    );
}

#[test]
fn the_fillet_fixture_meshes_at_the_app_chord() {
    let s = solid_of("fillet_corner.step", &text("fillet_corner.step"));
    let m = machining_mesh(&s, TOL, &CancelToken::new())
        .unwrap_or_else(|e| panic!("fillet_corner.step at {TOL} mm: {e}"));
    assert!(m.report.deviation.largest() <= TOL, "{:?}", m.report.deviation);
    assert_eq!(m.report.open_sides, 0);
    assert!(m.report.volume > 0.0);
}

/// box.step's solid with a void: the unit box, and inside it the same box
/// at half size (0.25..0.75), renumbered by 1000, used as a void through
/// an `ORIENTED_CLOSED_SHELL` `.F.`. `outer_inward` turns the outer
/// shell's faces over ([`inward`] on the outer lines only).
fn with_void(outer_inward: bool) -> String {
    let base = text("box.step");
    let renumber = |l: &str| -> String {
        let mut out = String::new();
        let mut rest = l;
        while let Some(i) = rest.find('#') {
            out.push_str(&rest[..i]);
            let tail = &rest[i + 1..];
            let n = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
            let id: u32 = tail[..n].parse().unwrap_or(0);
            out.push_str(&format!("#{}", id + 1000));
            rest = &tail[n..];
        }
        out.push_str(rest);
        out
    };
    let half = |l: &str| -> String {
        if let Some(i) = l.find("CARTESIAN_POINT('', (") {
            let start = i + "CARTESIAN_POINT('', (".len();
            let end = l[start..].find(')').map_or(l.len(), |e| start + e);
            let xs: Vec<String> = l[start..end]
                .split(',')
                .map(|v| format!("{:?}", 0.25 + 0.5 * v.trim().parse::<f64>().unwrap_or(0.0)))
                .collect();
            format!("{}{}{}", &l[..start], xs.join(", "), &l[end..])
        } else {
            l.replace(", 1.0);", ", 0.5);")
        }
    };
    let solid = |l: &str| {
        l.split(" = ")
            .next()
            .and_then(|id| id.trim_start_matches('#').parse::<u32>().ok())
            .is_some_and(|id| id >= 17)
    };
    let mut out = Vec::new();
    for l in base.lines() {
        if l.starts_with("#16 = MANIFOLD_SOLID_BREP") {
            out.push("#16 = BREP_WITH_VOIDS('', #17, (#2000));".to_owned());
            out.push("#2000 = ORIENTED_CLOSED_SHELL('', *, #1017, .F.);".to_owned());
        } else if l == "ENDSEC;" && out.iter().any(|o: &String| o.starts_with("#165 ")) {
            for inner in base.lines().filter(|l| solid(l)) {
                out.push(half(&renumber(inner)));
            }
            out.push(l.to_owned());
        } else if outer_inward && solid(l) {
            out.push(inward(l));
        } else {
            out.push(l.to_owned());
        }
    }
    out.join("\n")
}

/// Review of #2843: a solid with a void whose outer shell points inward
/// is refused by name, never turned whole (which would make the void
/// solid). solidcore's reader refuses it when it measures the solid; if a
/// later reader lets it through, the mesher's `Volume` refusal must hold.
/// The same solid with its outer shell outward still meshes.
#[test]
fn a_solid_with_a_void_and_its_outer_shell_inward_is_refused() {
    let good = solid_of("box.step with a void", &with_void(false));
    let m = machining_mesh(&good, TOL, &CancelToken::new())
        .unwrap_or_else(|e| panic!("a box with a void meshes: {e}"));
    assert!((m.report.volume - 0.875).abs() < 1e-9, "volume {}", m.report.volume);
    let mut k = SolidcoreKernel::default();
    let said = match import_step(&mut k, with_void(true).as_bytes(), &CancelToken::new()) {
        Err(e) => e.to_string(),
        Ok(mut import) => {
            assert_eq!(import.solids.len(), 1);
            let bad = import.solids.remove(0);
            let err = machining_mesh(&bad, TOL, &CancelToken::new())
                .expect_err("a void solid with its outer shell inward is never turned");
            assert!(matches!(err, MeshRefusal::Volume(v) if v < 0.0), "{err}");
            err.to_string()
        }
    };
    assert!(
        said.contains("#16") && said.contains("not a positive")
            || said.contains("encloses volume") && said.contains("not a positive one"),
        "refused by name: {said}"
    );
}

/// Review of #2843: the turn itself. Two cube shells (a box and a void)
/// with a negative sum stay as they are; one inward cube is turned.
#[test]
fn only_one_shell_is_ever_turned_outward() {
    // A cube's 12 triangles over corners 0..8 (bit 0 = x, 1 = y, 2 = z),
    // wound outward.
    let cube = |o: u32| -> Vec<[u32; 3]> {
        [
            [0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4],
            [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5],
        ]
        .iter()
        .map(|t| t.map(|i| i + o))
        .collect()
    };
    assert_eq!(shells(&cube(0)), 1);
    let mut two: Vec<[u32; 3]> = cube(0);
    two.extend(cube(8));
    assert_eq!(shells(&two), 2);
    let before = two.clone();
    assert_eq!(turn_outward(&mut two, -1.125), None, "a void solid is never turned");
    assert_eq!(two, before, "nothing changed");
    let mut one = cube(0);
    assert_eq!(turn_outward(&mut one, -1.0), Some(1.0));
    assert_eq!(one.first(), Some(&[0, 3, 2]), "turned over");
    assert_eq!(turn_outward(&mut one, 1.0), None, "an outward cube stays");
    assert_eq!(turn_outward(&mut one, f64::NAN), None);
}

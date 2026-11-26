//! #917 at the kernel: the exact volumes of two fixtures whose pcurves the
//! kernel makes, pinned against a closed form from the fixture's own
//! dimensions and against solidcore's own closed mesh. Not against truck:
//! truck reads both as open meshes (121 and 1090 open edges), so the bench
//! (step_agreement.rs) can only say that they disagree.
//! Provenance: our own fixtures; the volumes are elementary solid geometry
//! (a cube less its edge rounds, a cylinder less two blind radial holes).
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (import_step)
//! and camcraft_kernel::solidcore_trims (KernelTrims).

mod step_util;

use std::f64::consts::PI;
use std::path::PathBuf;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use step_util::{CASE_TIMEOUT, bounded_cancel, solidcore_mesh};

/// The exact integrator against a closed form: both are exact, so only the
/// integrator's quadrature and the file's rounded coordinates stand between.
const EXACT_REL: f64 = 1e-5;
/// The bench's tolerance of a mesh against the exact solid (step_agreement).
const MESH_REL: f64 = 1e-3;

/// One solid's exact volume and box.
type Exact = (f64, [f64; 3], [f64; 3]);

/// The bytes of the fixture `file`.
fn fixture(file: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(file);
    let bytes = std::fs::read(&path).unwrap_or_default();
    assert!(!bytes.is_empty(), "fixture {file} is missing");
    bytes
}

/// The one solid of `file` and the file's bytes.
fn exact(file: &'static str) -> (Exact, Vec<u8>) {
    let bytes = fixture(file);
    (exact_of(file, bytes.clone()), bytes)
}

/// The one solid that `bytes` (named `file`) reads as.
fn exact_of(file: &'static str, bytes: Vec<u8>) -> Exact {
    let b = bytes;
    let read = bounded_cancel(file, CASE_TIMEOUT, move |stop| {
        let mut k = SolidcoreKernel::default();
        import_step(&mut k, &b, stop).map(|i| {
            i.solids
                .iter()
                .map(|s| (s.measure.volume, s.measure.min, s.measure.max))
                .collect::<Vec<Exact>>()
        })
    });
    let solids = match read {
        Ok(s) => s,
        Err(e) => panic!("{file} must read through the kernel pcurves: {e}"),
    };
    let [one] = solids.as_slice() else {
        panic!("{file}: one solid, got {solids:?}")
    };
    *one
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-12)
}

/// solidcore's own mesh of the file is closed and within the bench
/// tolerance of the exact volume.
fn mesh_agrees(file: &str, bytes: &[u8], v: f64) {
    let m = solidcore_mesh(bytes).unwrap_or_else(|e| panic!("{file}: no mesh: {e}"));
    assert_eq!(
        m.mesh.open_edges, 0,
        "{file}: the mesh is not closed: {m:?}"
    );
    assert!(
        m.refused_faces.is_empty(),
        "{file}: faces left out: {:?}",
        m.refused_faces
    );
    assert!(
        rel(m.mesh.volume, v) <= MESH_REL,
        "{file}: mesh {} against exact {v}",
        m.mesh.volume
    );
}

#[test]
fn fillet_corner_reads_at_its_closed_form_volume() {
    let want = fillet_corner_volume();
    let ((v, min, max), bytes) = exact("fillet_corner.step");
    assert!(
        rel(v, want) <= EXACT_REL,
        "volume {v} mm3, closed form {want}"
    );
    for i in 0..3 {
        let ok = (min[i] + 15.0).abs() <= 1e-6 && (max[i] - 15.0).abs() <= 1e-6;
        assert!(ok, "box {min:?}..{max:?}");
    }
    mesh_agrees("fillet_corner.step", &bytes, v);
}

/// The fixture fillet_corner.step: a 30 mm cube on the origin, the three
/// edges at (-15,-15,-15) rounded R = 3.125 and a spherical corner blend.
/// Each round takes (1 - pi/4) r^2 along its edge outside the corner cube of
/// side r, length (L - r); the corner cube keeps the sphere's octant,
/// pi r^3 / 6, so it loses r^3 (1 - pi/6). In the file's units.
fn fillet_corner_volume() -> f64 {
    let (l, r) = (30.0_f64, 3.125_f64);
    l.powi(3) - 3.0 * (1.0 - PI / 4.0) * r * r * (l - r) - r.powi(3) * (1.0 - PI / 6.0)
}

/// fillet_corner.step with its length unit changed from the millimetre to
/// the inch (a conversion-based unit of 25.4 mm, ISO 10303-41). Every
/// number of the file stays; the part is 25.4 times larger.
#[test]
fn fillet_corner_in_inches_reads_at_its_closed_form_volume_in_mm() {
    let text = String::from_utf8(fixture("fillet_corner.step")).expect("ASCII fixture");
    let mm = "#9=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));";
    assert!(text.contains(mm), "fillet_corner.step: the unit line moved");
    let inch = "#9=(CONVERSION_BASED_UNIT('INCH',#9001) LENGTH_UNIT() NAMED_UNIT(#9002));
                #9001=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#9003);
                #9002=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);
                #9003=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));";
    let bytes = text.replace(mm, inch).into_bytes();
    let (v, min, max) = exact_of("fillet_corner.step in inches", bytes);
    let want = fillet_corner_volume() * 25.4_f64.powi(3);
    assert!(
        rel(v, want) <= EXACT_REL,
        "volume {v} mm3, closed form {want}"
    );
    let h = 15.0 * 25.4;
    for i in 0..3 {
        let ok = (min[i] + h).abs() <= 1e-4 && (max[i] - h).abs() <= 1e-4;
        assert!(ok, "box {min:?}..{max:?}");
    }
}

/// One blind radial hole of radius `a` in a cylinder of radius `big`, its
/// flat bottom at distance `h` from the cylinder's axis: the integral over
/// the hole's disc (y across the bar) of the depth sqrt(big^2 - y^2) - h,
/// by the midpoint rule on 20000 strips (error far under 1e-9 relative).
fn blind_radial_hole(big: f64, a: f64, h: f64) -> f64 {
    let n = 20_000_u32;
    let dy = 2.0 * a / f64::from(n);
    (0..n)
        .map(|i| {
            let y = -a + (f64::from(i) + 0.5) * dy;
            2.0 * (a * a - y * y).sqrt() * ((big * big - y * y).sqrt() - h) * dy
        })
        .sum()
}

#[test]
fn bar_cross_holes_reads_at_its_closed_form_volume() {
    // The fixture: a bar R 25 along X from x = 0 to 40, with two blind
    // radial holes R 2.5 at x = 20 along +Z and -Z, their flat bottoms at
    // z = 16.970684123352356 and z = -16.967928079409667 (the file's own
    // plane points). Volume = pi R^2 L less the two holes.
    let (big, len, a) = (25.0_f64, 40.0_f64, 2.5_f64);
    let holes = blind_radial_hole(big, a, 16.970684123352356)
        + blind_radial_hole(big, a, 16.967928079409667);
    let want = PI * big * big * len - holes;
    let ((v, min, max), bytes) = exact("bar_cross_holes.step");
    assert!(
        rel(v, want) <= EXACT_REL,
        "volume {v} mm3, closed form {want}"
    );
    let (lo, hi) = ([0.0, -big, -big], [len, big, big]);
    for i in 0..3 {
        let ok = (min[i] - lo[i]).abs() <= 1e-6 && (max[i] - hi[i]).abs() <= 1e-6;
        assert!(ok, "box {min:?}..{max:?}");
    }
    mesh_agrees("bar_cross_holes.step", &bytes, v);
}

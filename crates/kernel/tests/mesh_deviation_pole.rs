//! #3034: a triangle fan around the pole of a spline face measures by its
//! true sag. At a pole every u names the same point, so the planner can
//! give the apex node any u (the q02 mould's ellipsoid boss had its apex at
//! u = 0 and u = 1 while the ring around it stood at u = 0.5). Before the
//! fix the measure interpolated u across half the period inside such a
//! triangle, projected from that far hint, and judged a good mesh 0.29 mm
//! off its surface: the import of a closed body was refused.
//!
//! Fixture: our own geometry, built here - a rational quadratic hemisphere
//! (Piegl & Tiller, "The NURBS Book", 2nd ed., sec. 7.5: a circle swept by
//! a quarter circle) with its pole at v = 1, and a fan of triangles from a
//! ring just under the pole to the apex. No user file.
//!
//! Gate map (M12.1): covers camcraft_kernel::mesh_deviation,
//! camcraft_kernel::mesh_pole.

use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use std::path::PathBuf;

use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::mesh_deviation;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use solidcore::eval::plan::EvalSurface;
use solidcore::kernel::CancelToken;
use solidcore_record::Vec3;
use solidcore_record::brep::Brep;
use solidcore_tess::{FacePlan, TessPlan};

const R: f64 = 10.0;
const TOL: f64 = 0.005;
/// The ring's v, as in the q02 plan (0.9857 there).
const RING_V: f64 = 0.985;
/// Ring nodes around the full circle.
const RING: usize = 256;

/// The hemisphere of radius `R` about the origin: u around the circle (9
/// control points), v from the equator (0) to the pole (1).
fn hemisphere() -> EvalSurface {
    let circle: Vec<([f64; 2], f64)> = (0..9)
        .map(|i| {
            let a = i as f64 * TAU / 8.0;
            if i % 2 == 0 { ([a.cos(), a.sin()], 1.0) } else { ([a.cos() / FRAC_1_SQRT_2, a.sin() / FRAC_1_SQRT_2], FRAC_1_SQRT_2) }
        })
        .collect();
    // (radius, height, weight) of the quarter circle.
    let quarter = [(R, 0.0, 1.0), (R, R, FRAC_1_SQRT_2), (0.0, R, 1.0)];
    let mut ctrl = Vec::new();
    for (r, z, wv) in quarter {
        for ([cx, cy], wu) in &circle {
            let w = wu * wv;
            ctrl.push([w * r * cx, w * r * cy, w * z, w]);
        }
    }
    let u = vec![0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0];
    EvalSurface::nurbs([2, 2], [u, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]], [9, 3], ctrl)
}

/// A record with no faces: every face of the plan is measured by
/// projection onto its plan surface (the spline path of the q02 boss).
fn no_analytic() -> Brep {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    let bytes = std::fs::read(&path).expect("box.step");
    let mut k = SolidcoreKernel::default();
    let mut import = import_step(&mut k, &bytes, &CancelToken::new()).expect("box.step imports");
    let mut record = import.solids.remove(0).record;
    record.faces.clear();
    record
}

/// The fan: nodes 0 and 1 are the apex at u = 0 and u = 1 (fixed, as the
/// planner gives them), then the ring. A triangle of the first half of the
/// ring meets the apex at u = 0, one of the second half at u = 1.
fn fan() -> TessPlan {
    let mut uv = vec![[0.0, 1.0], [1.0, 1.0]];
    uv.extend((0..=RING).map(|k| [k as f64 / RING as f64, RING_V]));
    let triangles = (0..RING)
        .map(|k| {
            let apex = if 2 * k < RING { 0 } else { 1 };
            [(k + 2) as u32, (k + 3) as u32, apex]
        })
        .collect();
    TessPlan {
        surfaces: vec![hemisphere()],
        faces: vec![FacePlan {
            face: 0,
            surface: Some(0),
            uv,
            fixed: vec![Vec3::new(0.0, 0.0, R), Vec3::new(0.0, 0.0, R)],
            normals: Vec::new(),
            flip: false,
            triangles,
            poles: Vec::new(),
        }],
        edges: Vec::new(),
        edge_faces: Vec::new(),
    }
}

/// The q02 case end to end at the kernel: a closed solid whose spline face
/// ends in a pole at a vertex meshes inside the tolerance, closed.
/// `sphere_quarter_pole.step` is our own file (see its header): a quarter
/// sphere of radius 25 whose dome is a rational B-spline that turns 180
/// degrees in u, with its v = 1 edge collapsed to the apex. The planner
/// gives the apex one u, so the fan's far triangles span half a turn.
#[test]
fn a_solid_with_a_spline_pole_meshes_inside_the_tolerance() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/sphere_quarter_pole.step");
    let bytes = std::fs::read(&path).expect("sphere_quarter_pole.step");
    let mut k = SolidcoreKernel::default();
    let mut import = import_step(&mut k, &bytes, &CancelToken::new()).expect("the quarter sphere reads");
    assert_eq!(import.solids.len(), 1, "one solid");
    let s = import.solids.remove(0);
    let m = machining_mesh(&s, TOL, &CancelToken::new())
        .unwrap_or_else(|e| panic!("a quarter sphere with a spline pole must mesh: {e}"));
    assert_eq!(m.report.open_sides, 0, "{:?}", m.report);
    assert!(m.report.deviation.largest() <= TOL, "{:?}", m.report.deviation);
    // Signed volume (divergence theorem): the quarter sphere's pi R^3 / 3,
    // wound outward, inside the tolerance times the dome's area (1963 mm^2).
    let p = |i: u32| m.positions[i as usize];
    let volume: f64 = m
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(p);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        })
        .sum();
    let exact = std::f64::consts::PI * 25.0_f64.powi(3) / 3.0;
    assert!((volume - exact).abs() < 10.0, "volume {volume} vs {exact}");
}

#[test]
fn a_fan_at_a_spline_pole_measures_by_its_true_sag() {
    let plan = fan();
    let s = &plan.surfaces[0];
    // The fixture is what it says: the apex at either u is the pole, and
    // the ring stands a small circle under it.
    for u in [0.0, 0.3, 1.0] {
        let (p, _, _) = s.derivs(u, 1.0).expect("the pole evaluates");
        assert!((p.x.abs() + p.y.abs() + (p.z - R).abs()) < 1e-9, "u = {u}: {p:?}");
    }
    let (ring, _, _) = s.derivs(0.0, RING_V).expect("the ring evaluates");
    let rho = ring.x.hypot(ring.y);
    assert!(rho > 0.05 && rho < 1.0, "ring radius {rho}");
    // The fan's true sag: a ring chord's sag and the sag of a side running
    // to the apex across that small arc, both far under the tolerance.
    let d = mesh_deviation::measure_within(&plan, &no_analytic(), TOL, &CancelToken::new())
        .expect("the fan measures");
    assert!(d.largest() <= TOL, "a good fan at a pole is judged {} mm off: {d:?}", d.largest());
}

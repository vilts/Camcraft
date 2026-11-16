//! Pinning tests for #1029: projected spline/revolved deviation, the
//! spline_revolved.step fixture, and continuous between-sample sag bounds.
//! Provenance: our own test design; fixtures are the repo's own.
//!
//! Gate map (M12.1): covers camcraft_kernel::machining_mesh,
//! camcraft_kernel::mesh_bound, camcraft_kernel::mesh_bound_blossom,
//! camcraft_kernel::mesh_bound_spline, camcraft_kernel::mesh_deviation.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::mesh_deviation;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepSolid, import_step};
use solidcore::Vec3 as EvalVec3;
use solidcore::eval::plan::EvalSurface;
use solidcore::kernel::CancelToken;
use solidcore_record::Vec3;
use solidcore_record::brep::Surface;
use solidcore_tess::{FacePlan, MeshTolerance, TessPlan};

const TOL: f64 = 0.005;

fn solid(file: &str) -> StepSolid {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(file);
    let bytes = std::fs::read(&path).unwrap_or_default();
    let mut k = SolidcoreKernel::default();
    let mut import = import_step(&mut k, &bytes, &CancelToken::new())
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(import.solids.len(), 1, "{file}: one solid expected");
    import.solids.remove(0)
}

fn plane_frame(surface: &Surface) -> ([f64; 3], [f64; 3], [f64; 3]) {
    match surface {
        Surface::Plane { origin, u_axis, v_axis } => (*origin, *u_axis, *v_axis),
        Surface::PlaneThrough { origin, u_point, v_point } => (
            *origin,
            [0, 1, 2].map(|i| u_point[i] - origin[i]),
            [0, 1, 2].map(|i| v_point[i] - origin[i]),
        ),
        other => panic!("expected planar box face, got {other:?}"),
    }
}

fn plane_eval(origin: [f64; 3], du: [f64; 3], dv: [f64; 3], u: f64, v: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| origin[i] + du[i] * u + dv[i] * v)
}

/// Pin (Item 2): clean-room STEP fixture with a RationalSpline top dome and a
/// PlacedRevolution outer wall imports cleanly and meshes within tolerance.
#[test]
fn spline_and_revolved_step_fixture_meshes_within_tolerance() {
    let s = solid("spline_revolved.step");
    let has_spline = s.record.faces.iter().any(|f| matches!(f.surface, Surface::Spline { .. } | Surface::RationalSpline { .. }));
    let has_rev = s.record.faces.iter().any(|f| matches!(f.surface, Surface::PlacedRevolution { .. }));
    assert!(has_spline, "fixture must contain a Spline or RationalSpline face");
    assert!(has_rev, "fixture must contain a PlacedRevolution face");

    let m = machining_mesh(&s, TOL, &CancelToken::new())
        .expect("spline_revolved.step must mesh within 0.005 mm");
    assert_eq!(m.report.open_sides, 0);
    assert!(m.report.deviation.largest() <= TOL, "{:?}", m.report.deviation);
    assert_eq!(m.report.faces, 6);

    let m01 = machining_mesh(&s, 0.01, &CancelToken::new())
        .expect("spline_revolved.step at 0.01 mm must pass under projection");
    assert!(m01.report.deviation.parametric > 0.01, "{:?}", m01.report.deviation);
    assert!(m01.report.deviation.largest() <= 0.01, "{:?}", m01.report.deviation);
}

/// Pin (Item 2): non-uniform parameter speed on a flat B-spline face leaves the
/// mesh lying exactly on the surface (0 projected error) while linear (u, v)
/// interpolation over-reads parametric distance past tolerance.
#[test]
fn non_uniform_spline_parametric_overread_passes_under_projection() {
    let mut s = solid("box.step");
    let face = &mut s.record.faces[0];
    let (origin, du, dv) = plane_frame(&face.surface);
    let [[u0, u1], [v0, v1]] = face.domain;
    let u_mid = u0 + 0.15 * (u1 - u0);
    let p = |u: f64, v: f64| plane_eval(origin, du, dv, u, v);
    face.surface = Surface::Spline {
        u_knots: vec![u0, u0, u0, u1, u1, u1],
        v_knots: vec![v0, v0, v1, v1],
        points: vec![vec![p(u0, v0), p(u0, v1)], vec![p(u_mid, v0), p(u_mid, v1)], vec![p(u1, v0), p(u1, v1)]],
    };
    let (plan, _) = solidcore_tess::plan(&s.record, &MeshTolerance::chord(TOL), 1.8)
        .expect("non-uniform planar spline must plan");
    let d = mesh_deviation::measure(&plan, &s.record, &CancelToken::new())
        .expect("measure must succeed");
    assert!(d.parametric > 0.1, "must provoke parametric over-read > 0.1 mm, got {}", d.parametric);
    assert!(d.largest() <= TOL, "projected distance must pass <= {TOL}, got {d:?}");
    let m = machining_mesh(&s, TOL, &CancelToken::new())
        .expect("non-uniform exact spline face must not false-refuse");
    assert!(m.report.deviation.largest() <= TOL);
}

/// Pin (Item 3): a curved sphere triangle whose 15 barycentric samples (i/4, j/4)
/// all lie inside tolerance, while the surface peaks past tolerance between those
/// samples, MUST be refused (not falsely passed by the 15-point lower bound).
#[test]
fn between_sample_sag_past_tolerance_is_refused() {
    let r_sph = 48.5_f64;
    let z_tri = r_sph - 0.0054_f64;
    let verts = [0_usize, 1, 2].map(|k| {
        let th = k as f64 * std::f64::consts::TAU / 3.0;
        Vec3::new(th.cos(), th.sin(), z_tri)
    });
    let mut sphere_record = solid("box.step").record;
    sphere_record.faces.truncate(1);
    sphere_record.faces[0].surface = Surface::Sphere {
        frame: solidcore_record::brep::Frame { origin: [0.0; 3], x_axis: [1.0, 0.0, 0.0], y_axis: [0.0, 1.0, 0.0] },
        radius: r_sph,
    };
    let sph_plan = TessPlan {
        surfaces: vec![EvalSurface::Plane {
            origin: EvalVec3::new(0.0, 0.0, z_tri),
            du: EvalVec3::new(1.0, 0.0, 0.0),
            dv: EvalVec3::new(0.0, 1.0, 0.0),
        }],
        faces: vec![FacePlan {
            face: 0,
            surface: Some(0),
            uv: verts.map(|p| [p.x, p.y]).to_vec(),
            fixed: verts.to_vec(),
            normals: Vec::new(),
            flip: false,
            triangles: vec![[0, 1, 2]],
            poles: Vec::new(),
        }],
        edges: Vec::new(),
        edge_faces: Vec::new(),
    };
    let dsph = mesh_deviation::measure(&sph_plan, &sphere_record, &CancelToken::new())
        .expect("sphere test case must measure cleanly");
    assert!(dsph.largest() > TOL, "0.0054 mm sag at centroid must exceed {TOL}, got {dsph:?}");
}

/// Pin (Item 3): a cubic B-spline bump whose 15 barycentric samples (i/4, j/4)
/// all have z = 0 while u = 0.125 peaks > 0.01 mm MUST exceed TOL.
#[test]
fn between_sample_spline_sag_past_tolerance_is_refused() {
    let u_knots = vec![0.0, 0.0, 0.0, 0.0, 0.125, 0.25, 0.25, 1.0, 1.0, 1.0, 1.0];
    let v_knots = vec![0.0, 0.0, 1.0, 1.0];
    let n_u = 7;
    let greville: Vec<f64> = (0..n_u).map(|i| (u_knots[i + 1] + u_knots[i + 2] + u_knots[i + 3]) / 3.0).collect();
    let ctrl_pts: Vec<Vec<[f64; 3]>> = (0..n_u)
        .map(|i| {
            let z = if i == 2 { 0.08 } else { 0.0 };
            vec![[greville[i], 0.0, z], [greville[i], 1.0, z]]
        })
        .collect();
    let ctrl_ref = &ctrl_pts;
    let eval_ctrl: Vec<[f64; 4]> = (0..2)
        .flat_map(|j| (0..n_u).map(move |i| [ctrl_ref[i][j][0], ctrl_ref[i][j][1], ctrl_ref[i][j][2], 1.0]))
        .collect();
    let mut spline_record = solid("box.step").record;
    spline_record.faces.truncate(1);
    spline_record.faces[0].surface = Surface::Spline { u_knots: u_knots.clone(), v_knots: v_knots.clone(), points: ctrl_pts };
    spline_record.faces[0].domain = [[0.0, 1.0], [0.0, 1.0]];
    let spl_plan = TessPlan {
        surfaces: vec![EvalSurface::nurbs([3, 1], [u_knots, v_knots], [n_u, 2], eval_ctrl)],
        faces: vec![FacePlan {
            face: 0,
            surface: Some(0),
            uv: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            fixed: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)],
            normals: Vec::new(),
            flip: false,
            triangles: vec![[0, 1, 2]],
            poles: Vec::new(),
        }],
        edges: Vec::new(),
        edge_faces: Vec::new(),
    };
    let dspl = mesh_deviation::measure(&spl_plan, &spline_record, &CancelToken::new())
        .expect("spline test case must measure cleanly");
    assert!(dspl.largest() > TOL, "spline bump > 0.01 mm at u=0.125 must exceed {TOL}, got {dspl:?}");
}

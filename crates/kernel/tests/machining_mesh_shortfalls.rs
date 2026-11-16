//! Crafted B-rep provocations for every solidcore-tess PlanStats shortfall
//! through `machining_mesh` (M12.1, #1029).
//! Provenance: our own test design; mutates `box.step` in memory.
//!
//! Gate map (M12.1): covers camcraft_kernel::machining_mesh,
//! camcraft_kernel::mesh_bound, camcraft_kernel::mesh_bound_blossom,
//! camcraft_kernel::mesh_bound_spline, camcraft_kernel::mesh_deviation.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::{MeshRefusal, machining_mesh};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepSolid, import_step};
use solidcore::kernel::CancelToken;
use solidcore_record::brep::{Curve, Curve2, Edge, EdgeUseRef, Face, Loop, OrientedEdge, Surface, Vertex};

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

fn plane_normal(du: [f64; 3], dv: [f64; 3]) -> [f64; 3] {
    let n = [du[1] * dv[2] - du[2] * dv[1], du[2] * dv[0] - du[0] * dv[2], du[0] * dv[1] - du[1] * dv[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    [n[0] / len, n[1] / len, n[2] / len]
}

fn ortho_unit(dir: [f64; 3]) -> [f64; 3] {
    let ref_v = if dir[0].abs() <= dir[1].abs() && dir[0].abs() <= dir[2].abs() {
        [1.0, 0.0, 0.0]
    } else if dir[1].abs() <= dir[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    plane_normal(dir, ref_v)
}

fn clamped_knots(degree: usize, n_ctrl: usize, t0: f64, t1: f64) -> Vec<f64> {
    assert!(n_ctrl > degree);
    let n_interior = n_ctrl - degree - 1;
    let mut k = vec![t0; degree + 1];
    for i in 1..=n_interior {
        k.push(t0 + (t1 - t0) * i as f64 / (n_interior + 1) as f64);
    }
    k.extend(std::iter::repeat_n(t1, degree + 1));
    k
}

fn build_spline_surface(face: &Face, grid_n: usize, degree: usize, mut offset_at: impl FnMut(usize, usize, f64, f64) -> f64) -> Surface {
    let (origin, du, dv) = plane_frame(&face.surface);
    let nrm = plane_normal(du, dv);
    let [[u0, u1], [v0, v1]] = face.domain;
    let u_knots = clamped_knots(degree, grid_n, u0, u1);
    let v_knots = clamped_knots(degree, grid_n, v0, v1);
    let greville = |knots: &[f64], i: usize| knots[i + 1..=i + degree].iter().sum::<f64>() / degree as f64;
    let points = (0..grid_n)
        .map(|i| {
            let u = greville(&u_knots, i);
            let su = (u - u0) / (u1 - u0);
            (0..grid_n)
                .map(|j| {
                    let v = greville(&v_knots, j);
                    let sv = (v - v0) / (v1 - v0);
                    let mut p = plane_eval(origin, du, dv, u, v);
                    let off = if i > 0 && i + 1 < grid_n && j > 0 && j + 1 < grid_n {
                        offset_at(i, j, su, sv)
                    } else {
                        0.0
                    };
                    for c in 0..3 {
                        p[c] += off * nrm[c];
                    }
                    p
                })
                .collect()
        })
        .collect();
    Surface::Spline { u_knots, v_knots, points }
}

/// Degree-1 B-spline with knots clustered at `2^-k` (`k = 1..=26`), alternating
/// `0` and `amp`: only ~29 control points while triggering `MAX_DEPTH` in ~100 nodes.
fn depth_capped_spline_curve(a: [f64; 3], b: [f64; 3], amp: f64, offset_dir: [f64; 3]) -> Curve {
    let levels = 26usize;
    let mut ts = Vec::with_capacity(levels + 2);
    ts.push(0.0);
    for k in (1..=levels).rev() {
        ts.push(0.5_f64.powi(k as i32));
    }
    ts.push(1.0);
    let mut knots = Vec::with_capacity(ts.len() + 2);
    knots.push(0.0);
    knots.extend_from_slice(&ts);
    knots.push(1.0);
    let points = ts
        .iter()
        .enumerate()
        .map(|(idx, &t)| {
            let off = if idx > 0 && idx + 1 < ts.len() && (levels + 1 - idx) % 2 == 1 { amp } else { 0.0 };
            [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t + off * offset_dir[c])
        })
        .collect();
    Curve::Spline { knots, points }
}

/// Real planner provocations: crafted B-rep mutations of box.step reach every
/// PlanStats refusal variant (and capped within==true notes) through `machining_mesh`.
#[test]
fn provoke_face_left_out_edge_not_sampled_and_lenient() {
    // 1. FaceLeftOut: shift one pcurve along u past accept -> face 0 unmeshed.
    {
        let mut s = solid("box.step");
        let f0 = &mut s.record.faces[0];
        let Loop::Edges { edges } = &mut f0.loops[0] else { panic!("expected edge loop") };
        match &mut edges[0].pcurve {
            Curve2::Spline { points, .. } => points.iter_mut().for_each(|p| p[0] += 0.5),
            Curve2::RationalSpline { homogeneous, .. } => homogeneous.iter_mut().for_each(|p| p[0] += 0.5 * p[2]),
        }
        f0.domain[0][1] += 0.5;
        let res = machining_mesh(&s, TOL, &CancelToken::new());
        assert!(matches!(res, Err(MeshRefusal::FaceLeftOut { face: 0, .. })), "got {res:?}");
    }
    // 2. EdgeNotSampled: free edge with inverted interval [1.0, 0.0].
    {
        let mut s = solid("box.step");
        let mut bad_edge = s.record.edges[0].clone();
        bad_edge.uses.clear();
        bad_edge.interval = [1.0, 0.0];
        let ei = s.record.edges.len();
        s.record.edges.push(bad_edge);
        let res = machining_mesh(&s, TOL, &CancelToken::new());
        assert!(matches!(res, Err(MeshRefusal::EdgeNotSampled { edge, .. }) if edge == ei), "got {res:?}");
    }
    // 3. Lenient: non-dyadic self-crossing inner loop on face 0 -> lenient CDT fill.
    {
        let mut s = solid("box.step");
        let (origin, du, dv) = plane_frame(&s.record.faces[0].surface);
        let [[u0, u1], [v0, v1]] = s.record.faces[0].domain;
        let uv_at = |su: f64, sv: f64| [u0 + (u1 - u0) * su, v0 + (v1 - v0) * sv];
        let pts_uv = [uv_at(0.2, 0.2), uv_at(0.8, 0.6), uv_at(0.8, 0.2), uv_at(0.2, 0.7)];
        let pts_3d: Vec<[f64; 3]> = pts_uv.iter().map(|&[u, v]| plane_eval(origin, du, dv, u, v)).collect();
        let base_v = s.record.vertices.len() as u32;
        for &point in &pts_3d {
            s.record.vertices.push(Vertex { point, tolerance: 1e-6 });
        }
        let loop_idx = s.record.faces[0].loops.len() as u32;
        let mut hole = Vec::new();
        for k in 0..4 {
            let next = (k + 1) % 4;
            let edge_idx = s.record.edges.len() as u32;
            s.record.edges.push(Edge {
                curve: Curve::Line { start: pts_3d[k], end: pts_3d[next] },
                nurbs: None,
                interval: [0.0, 1.0],
                vertices: [base_v + k as u32, base_v + next as u32],
                tolerance: 1e-6,
                uses: vec![EdgeUseRef { face: 0, r#loop: loop_idx, position: k as u32, upper: false }],
            });
            hole.push(OrientedEdge {
                edge: edge_idx,
                forward: true,
                pcurve: Curve2::Spline { knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![pts_uv[k], pts_uv[next]] },
            });
        }
        s.record.faces[0].loops.push(Loop::Edges { edges: hole });
        let res = machining_mesh(&s, 0.0001, &CancelToken::new());
        assert!(matches!(res, Err(MeshRefusal::Lenient { face: 0, .. })), "got {res:?}");
    }
}

#[test]
fn provoke_face_capped_refusal() {
    // 4a. FaceCapped (!within): high-frequency high-amplitude wavy spline exceeds MAX_FACE_GRID & MAX_REFINE.
    let mut s = solid("box.step");
    s.record.faces[0].surface = build_spline_surface(&s.record.faces[0], 60, 3, |i, j, _, _| {
        if (i + j) % 2 == 0 { 0.5 } else { -0.5 }
    });
    let res = machining_mesh(&s, 0.001, &CancelToken::new());
    assert!(matches!(res, Err(MeshRefusal::FaceCapped { face: 0, sag, tol }) if sag > tol), "got {res:?}");
}

#[test]
#[ignore = "release heavy lane: capped-note spline fixture exceeds 60 seconds in debug"]
fn provoke_face_capped_note() {
    // 4b. FaceCapped (within == true): localized cross-band spline hits GridThinned, Refine brings sag <= tol -> note.
    let mut s = solid("box.step");
    s.record.faces[0].surface = build_spline_surface(&s.record.faces[0], 70, 3, |i, j, su, sv| {
        let fade_u = (std::f64::consts::PI * su).sin().powi(2);
        let fade_v = (std::f64::consts::PI * sv).sin().powi(2);
        let env_u = (-((su - 0.5) / 0.012).powi(2)).exp();
        let env_v = (-((sv - 0.5) / 0.012).powi(2)).exp();
        let sign_u = if i % 2 == 0 { 1.0 } else { -1.0 };
        let sign_v = if j % 2 == 0 { 1.0 } else { -1.0 };
        0.015 * (fade_u * env_v * sign_u + fade_v * env_u * sign_v) * 0.5
    });
    let m = machining_mesh(&s, 0.000055, &CancelToken::new())
        .expect("capped GridThinned face within tolerance should succeed with note");
    assert!(m.report.notes.iter().any(|n| n.contains("a planner bound limited it")), "got {:?}", m.report.notes);
}

#[test]
fn provoke_edge_capped_refusal_and_note() {
    // 5a. EdgeCapped (!within): dyadic-clustered spline hits MAX_DEPTH (24) with sag > tol.
    {
        let mut s = solid("box.step");
        let e0 = &mut s.record.edges[0];
        let Curve::Line { start, end } = e0.curve else { panic!("expected line edge 0") };
        let dir = [0, 1, 2].map(|c| end[c] - start[c]);
        let amp = 0.2;
        e0.curve = depth_capped_spline_curve(start, end, amp, ortho_unit(dir));
        e0.nurbs = None;
        e0.interval = [0.0, 1.0];
        e0.tolerance = amp * 2.0 + 0.05;
        let res = machining_mesh(&s, TOL, &CancelToken::new());
        assert!(matches!(res, Err(MeshRefusal::EdgeCapped { edge: 0, sag, tol }) if sag > tol), "got {res:?}");
    }
    // 5b. EdgeCapped (within == true): dyadic quarter-turn spline hits MAX_DEPTH with sag <= tol -> note.
    {
        let mut s = solid("box.step");
        let amp = 0.28;
        let (_, face0_du, face0_dv) = plane_frame(&s.record.faces[0].surface);
        let e0 = &mut s.record.edges[0];
        let Curve::Line { start, end } = e0.curve else { panic!("expected line edge 0") };
        let dir = [0, 1, 2].map(|c| end[c] - start[c]);
        let mut off = plane_normal(plane_normal(face0_du, face0_dv), dir);
        let mid_to_center = [0, 1, 2].map(|c| 0.5 - 0.5 * (start[c] + end[c]));
        if (0..3).map(|c| off[c] * mid_to_center[c]).sum::<f64>() < 0.0 {
            off = [-off[0], -off[1], -off[2]];
        }
        e0.curve = depth_capped_spline_curve(start, end, amp, off);
        e0.nurbs = None;
        e0.interval = [0.0, 1.0];
        e0.tolerance = amp * 2.0 + 0.05;
        let m = machining_mesh(&s, 0.30, &CancelToken::new())
            .expect("capped EdgeCap within tolerance should succeed with note");
        assert!(m.report.notes.iter().any(|n| n.contains("sampling stopped at")), "got {:?}", m.report.notes);
    }
}

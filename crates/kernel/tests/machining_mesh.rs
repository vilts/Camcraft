//! The machining mesh's laws (M12.1, #940), default lane: a STEP solid
//! meshed by solidcore-tess at a chord in mm is handed out only whole and
//! measured inside its tolerance; every planner shortfall, a bad tolerance,
//! a time-out and a measured deviation past the tolerance are named
//! refusals. The per-fixture deviation table is the heavy bench
//! (tests/mesh_deviation_bench.rs).
//! Provenance: our own test design; fixtures are the repo's own.
//!
//! Gate map (M12.1): covers camcraft_kernel::machining_mesh,
//! camcraft_kernel::mesh_bound, camcraft_kernel::mesh_bound_blossom,
//! camcraft_kernel::mesh_bound_spline, camcraft_kernel::mesh_deviation.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::{MeshRefusal, judge_stats, machining_mesh};
use camcraft_kernel::mesh_deviation::{self, analytic_distance};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepSolid, import_step};
use solidcore::kernel::CancelToken;
use solidcore_record::Vec3;
use solidcore_tess::{
    Cap, CappedEdge, EdgeCap, MeshTolerance, OverTolerance, PlanStats, RefusedEdge, RefusedFace,
};

/// The app's chord cap (#689), mm.
const TOL: f64 = 0.005;

#[test]
fn solidcraft_sample_meshes_within_the_app_chord() {
    let bytes = include_bytes!("../../../testdata/models/solidcraft_for_beginners.sc");
    let imported = camcraft_kernel::solidcraft::import_solidcraft(
        &mut SolidcoreKernel::default(),
        bytes,
        &CancelToken::new(),
    )
    .expect("the Solidcraft sample must import as a checked solid");
    let mesh = machining_mesh(&imported.solid, TOL, &CancelToken::new())
        .expect("the Solidcraft sample must mesh at the app chord");
    println!("Solidcraft sample: {:?}", mesh.report);
    assert_eq!(mesh.report.tolerance_mm, TOL);
    assert_eq!(mesh.report.open_sides, 0);
    assert!(mesh.report.deviation.largest() <= TOL);
    assert!(mesh.report.deviation.samples > 0);
    assert!(mesh.report.volume > 0.0);
    assert_eq!(mesh.report.faces, imported.solid.record.faces.len());
    assert!(
        mesh.report
            .notes
            .iter()
            .any(|note| note.contains("replanned"))
    );
    for face in 0..imported.solid.record.faces.len() {
        assert!(
            mesh.tri_face.contains(&(face as u32)),
            "missing face {face}"
        );
    }
}

#[test]
fn solidcraft_sample_still_refuses_an_out_of_tolerance_mesh() {
    let bytes = include_bytes!("../../../testdata/models/solidcraft_for_beginners.sc");
    let imported = camcraft_kernel::solidcraft::import_solidcraft(
        &mut SolidcoreKernel::default(),
        bytes,
        &CancelToken::new(),
    )
    .expect("the Solidcraft sample must import as a checked solid");
    let rec = &imported.solid.record;
    // At 0.1 mm, solidcore-tess plans a coarse mesh whose continuous projected
    // deviation exceeds 0.1 mm (and far exceeds TOL).
    let (coarse_plan, _) = solidcore_tess::plan(rec, &MeshTolerance::chord(0.1), 150.0)
        .expect("coarse plan must build");
    let coarse_01 = mesh_deviation::measure_within(&coarse_plan, rec, 0.1, &CancelToken::new())
        .expect("coarse plan must measure");
    assert!(coarse_01.largest() > 0.1, "{coarse_01:?}");
    let coarse_tol = mesh_deviation::measure_within(&coarse_plan, rec, TOL, &CancelToken::new())
        .expect("coarse plan must measure against TOL");
    assert!(coarse_tol.largest() > TOL, "{coarse_tol:?}");
    // Under projection (#1029), replanning at 0.05 mm brings it within 0.1 mm
    // even though face 36 parametric over-reads > 0.13 mm.
    let ok = machining_mesh(&imported.solid, 0.1, &CancelToken::new())
        .expect("0.1 mm request passes via projection after 0.05 mm replan");
    assert!(ok.report.deviation.parametric > 0.1 && ok.report.deviation.largest() <= 0.1);
    // An un-reducible > 0.1 mm spline bump on box.step still refuses with Deviation.
    let mut bad = solid("box.step");
    let solidcore_record::brep::Surface::Plane { origin, u_axis: du, v_axis: dv } = bad.record.faces[0].surface else {
        panic!("box.step face 0 is not a plane")
    };
    let n = [du[1] * dv[2] - du[2] * dv[1], du[2] * dv[0] - du[0] * dv[2], du[0] * dv[1] - du[1] * dv[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.125, 0.25, 0.25, 1.0, 1.0, 1.0, 1.0];
    let g: Vec<f64> = (0..7).map(|i| (knots[i + 1] + knots[i + 2] + knots[i + 3]) / 3.0).collect();
    let points = (0..7)
        .map(|i| (0..7).map(|j| {
            let h = if i == 2 && j == 2 { 1.5 / len } else { 0.0 };
            [0, 1, 2].map(|c| origin[c] + du[c] * g[i] + dv[c] * g[j] + n[c] * h)
        }).collect())
        .collect();
    bad.record.faces[0].surface = solidcore_record::brep::Surface::Spline { u_knots: knots.clone(), v_knots: knots, points };
    bad.record.faces[0].nurbs = None;
    bad.record.faces[0].domain = [[0.0, 1.0], [0.0, 1.0]];
    match machining_mesh(&bad, 0.1, &CancelToken::new()) {
        Err(MeshRefusal::Deviation { measured, tol, face }) => {
            assert!(measured > tol && tol == 0.1 && face == Some(0));
        }
        other => panic!("an un-reducible > 0.1 mm deviation must remain refused: {other:?}"),
    }
}

fn solid(file: &str) -> StepSolid {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(file);
    let bytes = std::fs::read(&path).unwrap_or_default();
    let mut k = SolidcoreKernel::default();
    let mut import = match import_step(&mut k, &bytes, &CancelToken::new()) {
        Ok(i) => i,
        Err(e) => panic!("{file}: {e}"),
    };
    assert_eq!(import.solids.len(), 1, "{file}: one solid expected");
    import.solids.remove(0)
}

#[test]
fn machining_mesh_is_watertight_and_within_tolerance() {
    for (file, faces) in [("box.step", 6), ("Milling-test.step", 0)] {
        let s = solid(file);
        let m = match machining_mesh(&s, TOL, &CancelToken::new()) {
            Ok(m) => m,
            Err(e) => panic!("{file}: {e}"),
        };
        let r = &m.report;
        println!("{file}: {r:?}");
        assert_eq!(r.open_sides, 0, "{file}");
        assert!(r.deviation.largest() <= TOL, "{file}: {:?}", r.deviation);
        assert!(r.deviation.samples > 0, "{file}");
        assert_eq!(m.triangles.len(), r.triangles);
        assert_eq!(m.tri_face.len(), r.triangles);
        if faces > 0 {
            assert_eq!(r.faces, faces, "{file}");
        }
        // The enclosed volume is the exact solid's, less the facets' sag.
        let rel = (r.volume - s.measure.volume).abs() / s.measure.volume;
        assert!(
            rel < 1e-3,
            "{file}: volume {} vs {}",
            r.volume,
            s.measure.volume
        );
    }
}

#[test]
fn a_bad_tolerance_or_a_spent_time_bound_is_refused_by_name() {
    let s = solid("box.step");
    for tol in [f64::NAN, f64::INFINITY, 0.0, -0.005, 1e-9] {
        match machining_mesh(&s, tol, &CancelToken::new()) {
            Err(MeshRefusal::Tolerance { .. }) => {}
            other => panic!("tolerance {tol}: {other:?}"),
        }
    }
    let stop = CancelToken::new();
    stop.cancel();
    assert!(matches!(
        machining_mesh(&s, TOL, &stop),
        Err(MeshRefusal::TimedOut(_))
    ));
}

/// Every PlanStats entry has its decided outcome (decision file, point 2).
#[test]
fn every_planner_shortfall_is_a_refusal_or_a_note() {
    let face = |reason: &str| RefusedFace {
        face: 3,
        reason: reason.into(),
    };
    let edge = |reason: &str| RefusedEdge {
        edge: 7,
        reason: reason.into(),
    };
    let over = |within| OverTolerance {
        face: 2,
        sag: 0.009,
        unmeasured: 0,
        tol: TOL,
        within,
        caps: vec![Cap::Refine],
    };
    let capped = |within| CappedEdge {
        edge: 5,
        cap: EdgeCap::Nodes,
        sag: if within { 0.001 } else { 0.02 },
        excess: 4.0,
        tol: TOL,
        within,
    };
    let none = PlanStats::default();
    assert_eq!(judge_stats(&none), Ok(vec![]));

    let refused = [
        (
            PlanStats {
                unmeshed: vec![face("no surface")],
                ..none.clone()
            },
            "FaceLeftOut",
        ),
        (
            PlanStats {
                lenient: vec![face("pcurve off its edge")],
                ..none.clone()
            },
            "Lenient",
        ),
        (
            PlanStats {
                unsampled_edges: vec![edge("no curve")],
                ..none.clone()
            },
            "EdgeNotSampled",
        ),
        (
            PlanStats {
                over_tolerance: vec![over(false)],
                ..none.clone()
            },
            "FaceCapped",
        ),
        (
            PlanStats {
                edges_capped: vec![capped(false)],
                ..none.clone()
            },
            "EdgeCapped",
        ),
    ];
    for (stats, name) in refused {
        let got = judge_stats(&stats);
        assert!(
            format!("{got:?}").starts_with(&format!("Err({name}")),
            "{name}: {got:?}"
        );
    }
    // Within tolerance after all, or only finer: notes, not refusals.
    let noted = PlanStats {
        over_tolerance: vec![over(true)],
        edges_capped: vec![capped(true)],
        kept_strict: vec![edge("coarser sampling failed")],
        ..none
    };
    match judge_stats(&noted) {
        Ok(notes) => assert_eq!(notes.len(), 3, "{notes:?}"),
        Err(e) => panic!("{e}"),
    }
}

/// The deviation measure sees a moved edge node (parametric) and a surface
/// put in the wrong place (analytic, independent of the plan's surface).
#[test]
fn the_deviation_measure_sees_a_moved_node_and_a_wrong_surface() {
    let s = solid("box.step");
    let (plan, _) = match solidcore_tess::plan(&s.record, &MeshTolerance::chord(TOL), 1.8) {
        Ok(p) => p,
        Err(e) => panic!("{e}"),
    };
    let stop = CancelToken::new();
    let clean = match mesh_deviation::measure(&plan, &s.record, &stop) {
        Ok(d) => d,
        Err(e) => panic!("{e:?}"),
    };
    assert!(clean.largest() < 1e-9, "{clean:?}");
    assert_eq!(clean.faces_analytic, 6);

    // One shared edge node 0.1 mm off.
    let mut moved = plan.clone();
    let Some(p) = moved.faces.iter_mut().find_map(|f| f.fixed.first_mut()) else {
        panic!("no edge node")
    };
    *p += Vec3::new(0.0, 0.0, 0.1);
    let d = mesh_deviation::measure(&moved, &s.record, &stop);
    assert!(
        matches!(&d, Ok(d) if d.parametric > 0.05),
        "moved node not seen: {d:?}"
    );

    // The record's own surface 0.1 mm off where the plan put the face: the
    // plan's triangles agree with the plan's surface (parametric 0), the
    // analytic measure, read from the record, sees the gap.
    let mut wrong = s.record.clone();
    let Some(solidcore_record::brep::Surface::Plane {
        origin,
        u_axis,
        v_axis,
    }) = wrong.faces.first_mut().map(|f| &mut f.surface)
    else {
        panic!("box.step face 0 is not a plane")
    };
    let n = [
        u_axis[1] * v_axis[2] - u_axis[2] * v_axis[1],
        u_axis[2] * v_axis[0] - u_axis[0] * v_axis[2],
        u_axis[0] * v_axis[1] - u_axis[1] * v_axis[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    for i in 0..3 {
        origin[i] += n[i] * 0.1 / len;
    }
    match mesh_deviation::measure(&plan, &wrong, &stop) {
        Ok(d) => {
            assert!(d.parametric < 1e-9 && d.analytic > 0.09, "{d:?}");
            assert!(d.largest() > 0.09 && d.worst_face == Some(0), "{d:?}");
        }
        Err(e) => panic!("{e:?}"),
    }
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(mesh_deviation::measure(&plan, &s.record, &cancelled).is_err());
}

#[test]
fn analytic_distances_are_the_closed_forms() {
    use solidcore_record::brep::{Frame, Surface};
    let frame = Frame {
        origin: [0.0; 3],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
    };
    let close = |s: &Surface, p: [f64; 3], want: f64| {
        let got = analytic_distance(s, p);
        assert!(
            got.is_some_and(|d| (d - want).abs() < 1e-12),
            "{s:?} at {p:?}: {got:?}, want {want}"
        );
    };
    close(
        &Surface::Plane {
            origin: [0.0, 0.0, 1.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
        },
        [3.0, 4.0, 3.5],
        2.5,
    );
    close(
        &Surface::Cylinder {
            frame: frame.clone(),
            radius: 2.0,
        },
        [3.0, 4.0, 9.0],
        3.0,
    );
    close(
        &Surface::Sphere {
            frame: frame.clone(),
            radius: 2.0,
        },
        [0.0, 3.0, 4.0],
        3.0,
    );
    close(
        &Surface::Torus {
            frame: frame.clone(),
            major_radius: 5.0,
            minor_radius: 1.0,
            tube_start: 0.0,
        },
        [5.0, 0.0, 3.0],
        2.0,
    );
    // A 45-degree cone of radius 1 at z = 0, widening up: (3, 0, 0) is 2
    // out radially, sqrt(2) off the slant line; past the apex (z = -1) the
    // apex is the nearest point.
    let cone = Surface::Cone {
        frame,
        radius_at_origin: 1.0,
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    close(&cone, [3.0, 0.0, 0.0], std::f64::consts::SQRT_2);
    close(&cone, [0.0, 0.0, -3.0], 2.0);
}

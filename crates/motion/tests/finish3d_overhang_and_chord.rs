//! #1840: 3D finish generator safety and precision pins exposed when the
//! as-imported sample's verify gate ran on S1 (Finish3D, BN-0600, tol 0.05):
//!
//! 1. `scan::facet_polygon`: horizontal/axis-aligned tying support maxima were
//!    dropped by `s > b` and shallow vertical-wall edges compressed many theta
//!    samples between two XY scan directions, biting up to 0.076 mm into walls.
//! 2. `rings::emit_rings`: only the cycle's entry vertex checked `column_clear`,
//!    so rings starting in open air walked straight underneath overhanging canopies
//!    and plunged/cut through solid material.
//! 3. `raster::raster_pass_spaced`: `eps_chord = tolerance * 0.25` (0.0125 mm at
//!    default 0.05 tolerance) plus midpoint-only checks allowed convex ridges to
//!    sag > `DEV_TOL` (0.01 mm) in `stock_sim::deviate`.
//!
//! Provenance: our own analytic/extruded solid fixtures matching the failure modes.
//! Gate map (#1840): covers camcraft_motion::finishing3d (scan, rings, raster).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{
    parallel, waterline, Finish3DParams, FinishPattern, GapLink,
};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{dist_seg_tri, Backend, SimOp, Stock, DEV_TOL};

const R: f64 = 3.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn params(pattern: FinishPattern, clearance_z: f64, top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        pattern,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.05,
        clearance_z,
        retract_z: None,
        gap_link: GapLink::Jump,
        top_z,
        bottom_z,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// Closed prism from a CCW (x, z) boundary extruded across `y0..y1`.
fn extrude_xz_ccw(poly: &[(f64, f64)], y0: f64, y1: f64) -> Vec<Triangle> {
    let mut tris = Vec::new();
    let n = poly.len();
    for i in 1..(n - 1) {
        let (x0, z0) = poly[0];
        let (xa, za) = poly[i];
        let (xb, zb) = poly[i + 1];
        // Front (-y) face: i x k = -j outward when (x, z) is CCW.
        tris.push(Triangle::new(p3(x0, y0, z0), p3(xa, y0, za), p3(xb, y0, zb)));
        // Back (+y) face.
        tris.push(Triangle::new(p3(x0, y1, z0), p3(xb, y1, zb), p3(xa, y1, za)));
    }
    for i in 0..n {
        let (ax, az) = poly[i];
        let (bx, bz) = poly[(i + 1) % n];
        let a0 = p3(ax, y0, az);
        let a1 = p3(ax, y1, az);
        let b1 = p3(bx, y1, bz);
        let b0 = p3(bx, y0, bz);
        tris.push(Triangle::new(a0, a1, b1));
        tris.push(Triangle::new(a0, b1, b0));
    }
    tris
}

fn box_tris(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Vec<Triangle> {
    extrude_xz_ccw(&[(x0, z0), (x1, z0), (x1, z1), (x0, z1)], y0, y1)
}

fn simulate_and_gouge(
    facets: &[Triangle],
    path: &Toolpath,
    box_min: (f64, f64),
    box_max: (f64, f64),
    stock_top: f64,
) -> (usize, f64) {
    let mut stock = Stock::block(
        Backend::Column,
        box_min.0,
        box_min.1,
        box_max.0,
        box_max.1,
        stock_top,
        0.08,
    )
    .expect("test stock");
    let sim = SimOp {
        moves: &path.moves,
        radius: R,
        ball: true,
        cone: None,
        corner: None,
    };
    stock.simulate(&[sim]).expect("simulate");
    let dev = stock.deviate(facets, DEV_TOL).expect("deviate");
    (dev.gouge_cells, dev.max_gouge)
}

#[test]
fn shallow_wall_edge_slice_keeps_tie_endpoints_and_ellipse_chord_within_gouge_tol() {
    // Reproduces sample_ap214.step Band 4's vertical wall triangle at y = -3.175:
    // shallow slope from (-17.42, 1.77) -> (-17.26, 2.94) -> (-32.11, 3.99)
    // leaves a long nearly horizontal edge intersecting cz = 3.074 (z_tip ~ 0.074).
    let poly = [
        (-34.0, 0.0),
        (-16.0, 0.0),
        (-17.26, 1.77),
        (-17.26, 2.94),
        (-32.11, 3.99),
        (-34.0, 3.99),
    ];
    let facets = extrude_xz_ccw(&poly, -3.175, -1.5);
    // top_z = 1.17 -> next stepdown (~1.0954 mm) hits z_tip ~ 0.0746 (cz ~ 3.0746).
    let path = waterline(&facets, &params(FinishPattern::Waterline, 12.0, 1.17, 0.05))
        .expect("waterline generates");
    assert!(
        !path.notes.iter().any(|n| n.contains("plunge column")),
        "shallow wall edge slice should cut without plunge-column refusals: {:?}",
        path.notes
    );
    let (gouge_cells, max_gouge) =
        simulate_and_gouge(&facets, &path, (-38.0, -8.0), (-12.0, 3.0), 6.0);
    assert_eq!(
        gouge_cells, 0,
        "shallow vertical wall sliced inside DEV_TOL: max_gouge={max_gouge:.4} mm"
    );
}

fn move_point(m: &camcraft_motion::cl::Move, u: f64) -> Point3 {
    let z = m.from.z + (m.to.z - m.from.z) * u;
    if let Some(arc) = &m.arc {
        let (cx, cy) = (arc.center.x, arc.center.y);
        let rad = (m.from.x - cx).hypot(m.from.y - cy);
        let a0 = (m.from.y - cy).atan2(m.from.x - cx);
        let a1 = (m.to.y - cy).atan2(m.to.x - cx);
        let mut da = a1 - a0;
        if arc.ccw {
            while da <= -1e-9 {
                da += std::f64::consts::TAU;
            }
        } else {
            while da >= 1e-9 {
                da -= std::f64::consts::TAU;
            }
        }
        let ang = a0 + da * u;
        Point3::new(cx + rad * ang.cos(), cy + rad * ang.sin(), z)
    } else {
        Point3::new(
            m.from.x + (m.to.x - m.from.x) * u,
            m.from.y + (m.to.y - m.from.y) * u,
            z,
        )
    }
}

#[test]
fn waterline_and_steep_rings_trim_segments_under_overhang_instead_of_cutting_through() {
    // Lower pedestal [-8, 8] x [-8, 8] x [0, 8] with an overhanging bridge roof
    // [-2, 2] x [-9, 8] x [12, 16] attached via [-2, 2] x [2, 8] x [8, 12].
    // Rings at z_tip in [3, 6] are open-air on the flanks (|x| > 5) so entry
    // is clear, but walk directly beneath the overhang at |x| <= 5 on y ~ -11.
    let mut facets = box_tris(-8.0, -8.0, 0.0, 8.0, 8.0, 8.0);
    facets.extend(box_tris(-2.0, 2.0, 8.0, 2.0, 8.0, 12.0));
    let overhang = box_tris(-2.0, -9.0, 12.0, 2.0, 8.0, 16.0);
    facets.extend(overhang.iter().copied());

    for pattern in [FinishPattern::Waterline, FinishPattern::SteepShallow] {
        let p = params(pattern, 22.0, 7.0, 3.0);
        let path = match pattern {
            FinishPattern::Waterline => waterline(&facets, &p),
            _ => parallel(&facets, &p),
        }
        .expect("path generates");
        assert!(
            path.moves.iter().any(|m| m.kind == MoveKind::Feed),
            "{pattern:?} should still cut open-air flanks"
        );
        // Check that no cut move places the vertical tool column through the overhang.
        for (idx, m) in path.moves.iter().enumerate() {
            if !matches!(m.kind, MoveKind::Feed | MoveKind::Plunge) {
                continue;
            }
            for step in 0..=8 {
                let u = f64::from(step) / 8.0;
                let tip = move_point(m, u);
                let a = Point3::new(tip.x, tip.y, tip.z + R);
                let b = Point3::new(tip.x, tip.y, 22.0 + R);
                for tri in &overhang {
                    let d = dist_seg_tri(a, b, tri);
                    assert!(
                        d >= R - DEV_TOL,
                        "{pattern:?} move {idx} ({m:?}) cut into overhang: dist={d:.4} < {:.4} at ({:.3}, {:.3}, z_tip {:.3})",
                        R - DEV_TOL,
                        tip.x,
                        tip.y,
                        tip.z,
                    );
                }
            }
        }
        assert!(
            path.notes.iter().any(|n| n.contains("plunge column")),
            "{pattern:?} should report trimmed overhang portions: {:?}",
            path.notes
        );
    }
}

#[test]
fn parallel_raster_over_convex_cylinder_keeps_sagitta_within_stock_sim_dev_tol() {
    // Convex cylinder ridge R = 9.66 mm rising from a wider base (-32.36..32.36,
    // matching sample_ap214.step's upper lug geometry and cruise phase at tol 0.05).
    let r_cyl = 9.66;
    let n = 35usize;
    let mut cyl = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let th = std::f64::consts::PI * (i as f64) / (n as f64);
        cyl.push((r_cyl * th.cos(), r_cyl * th.sin()));
    }
    let mut facets = box_tris(-32.36, -1.6, -4.0, 32.36, 1.6, 0.0);
    facets.extend(extrude_xz_ccw(&cyl, -1.6, 1.6));
    let path = parallel(&facets, &params(FinishPattern::Parallel, 20.0, r_cyl, 7.0))
        .expect("parallel generates");
    let (gouge_cells, max_gouge) =
        simulate_and_gouge(&facets, &path, (-14.0, -5.0), (14.0, 5.0), 12.0);
    assert_eq!(
        gouge_cells, 0,
        "convex cylinder chord sagitta exceeded DEV_TOL ({DEV_TOL}): cells={gouge_cells}, max_gouge={max_gouge:.6} mm"
    );
}

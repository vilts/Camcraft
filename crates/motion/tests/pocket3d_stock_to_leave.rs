//! #3669: 3D Pocket applies `stock_to_leave` axially and radially (tops, flanks, and floors).
//! Gate map (#3669): covers camcraft_motion::machining3d (ladder, floor_levels, entry, shell, rest).
//! Provenance: clean-room 50 x 50 x 8 plate + spherical dome (`3d/rough-finish`), flat-top boss,
//! and through-pocket fixtures with exact 3D tool-envelope distance measurements.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{
    Entry3D, Pocket3DParams, open_sides, pocket3d, pocket3d_offset, pocket3d_with_rest, stock_band,
};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const HALF: f64 = 25.0;
const PLATE: f64 = 8.0;
const DOME_A: f64 = 18.0;
const DOME_H: f64 = 10.0;
const SPHERE_R: f64 = (DOME_A * DOME_A + DOME_H * DOME_H) / (2.0 * DOME_H);
const SPHERE_ZC: f64 = PLATE + DOME_H - SPHERE_R;
const TOOL_D: f64 = 6.0;
const TOOL_R: f64 = TOOL_D * 0.5;
const LEAVE: f64 = 0.5;
const ALLOW_TOL: f64 = 0.01;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn dome_height(x: f64, y: f64) -> f64 {
    let r2 = x * x + y * y;
    if r2 >= DOME_A * DOME_A {
        PLATE
    } else {
        PLATE + (SPHERE_R * SPHERE_R - r2).sqrt() - (SPHERE_R - DOME_H)
    }
}

/// Exact watertight mesh of the `3d/rough-finish` 50 x 50 x 8 plate with spherical dome (apex Z18).
fn rough_finish_mesh(n: usize) -> Vec<Triangle> {
    let node = |i: usize| -HALF + 2.0 * HALF * (i as f64) / (n as f64);
    let mut top = vec![vec![p3(0.0, 0.0, 0.0); n + 1]; n + 1];
    let mut bot = vec![vec![p3(0.0, 0.0, 0.0); n + 1]; n + 1];
    for i in 0..=n {
        for j in 0..=n {
            let (x, y) = (node(i), node(j));
            let z = (dome_height(x, y) * 1e6).round() / 1e6;
            top[i][j] = p3(x, y, z);
            bot[i][j] = p3(x, y, 0.0);
        }
    }
    let mut tris = Vec::new();
    for i in 0..n {
        for j in 0..n {
            tris.push(Triangle::new(top[i][j], top[i + 1][j], top[i + 1][j + 1]));
            tris.push(Triangle::new(top[i][j], top[i + 1][j + 1], top[i][j + 1]));
        }
    }
    let mut ring = Vec::with_capacity(4 * n);
    for i in 0..n { ring.push((i, 0)); }
    for j in 0..n { ring.push((n, j)); }
    for i in (1..=n).rev() { ring.push((i, n)); }
    for j in (1..=n).rev() { ring.push((0, j)); }
    let centre = p3(0.0, 0.0, 0.0);
    for idx in 0..ring.len() {
        let (a, b) = (ring[idx], ring[(idx + 1) % ring.len()]);
        let (t0, t1, b0, b1) = (top[a.0][a.1], top[b.0][b.1], bot[a.0][a.1], bot[b.0][b.1]);
        tris.push(Triangle::new(b0, b1, t1));
        tris.push(Triangle::new(b0, t1, t0));
        tris.push(Triangle::new(centre, b1, b0));
    }
    tris
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| p3([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a, b, c, d| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()]).unwrap()
}

fn stock_50_footprint(facets: &[Triangle], strategy: PocketStrategy, shell: bool, rest: bool) -> Region {
    let sil = camcraft_motion::geometry::slice::project(facets, 64).expect("silhouette");
    let stock = rect(-HALF, -HALF, HALF, HALF);
    let air = strategy == PocketStrategy::Adaptive && !shell && !rest;
    open_sides(&sil, TOOL_R, &Region::empty(), Some(&stock), stock_band(TOOL_R, air)).expect("footprint")
}

fn rough_params(strategy: PocketStrategy, entry: Entry3D) -> Pocket3DParams {
    let mut links = Links::ROUGH3D;
    links.approach.stock_top = Some(22.0);
    Pocket3DParams {
        pass_start: Default::default(),
        links,
        tool_diameter: TOOL_D,
        stepover: 3.0,
        stock_to_leave: LEAVE,
        clearance_z: 47.0,
        top_z: 22.0,
        bottom_z: PLATE,
        stepdown: 2.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        retract_z: None,
        entry,
        helix_radius: 1.2,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

fn verify_clean(facets: &[Triangle], region: &Region, path: &Toolpath, top_z: f64) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: TOOL_R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: TOOL_R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    verify(
        path,
        &VerifyContext {
            region,
            stock: None,
            model: Some(facets),
            tool: &tool,
            cutter: None,
            mode: Mode::Area,
            stock_top_z: top_z,
            clearance_z: 47.0,
            stock_to_leave: LEAVE,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

/// 3D distance from point `p` to upright flat-end cylinder `(cx, cy, z >= cz)` of radius `r`.
fn dist_point_to_cylinder(p: Point3, c: Point3, r: f64) -> f64 {
    let dr = ((p.x - c.x).hypot(p.y - c.y) - r).max(0.0);
    let dz = (c.z - p.z).max(0.0);
    dr.hypot(dz)
}

struct TriBox {
    tri: Triangle,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    z1: f64,
}

fn min_envelope_distance_mesh(path: &Toolpath, facets: &[Triangle], r: f64) -> f64 {
    let boxes: Vec<TriBox> = facets
        .iter()
        .map(|&tri| TriBox {
            tri,
            x0: tri.a.x.min(tri.b.x).min(tri.c.x),
            x1: tri.a.x.max(tri.b.x).max(tri.c.x),
            y0: tri.a.y.min(tri.b.y).min(tri.c.y),
            y1: tri.a.y.max(tri.b.y).max(tri.c.y),
            z1: tri.a.z.max(tri.b.z).max(tri.c.z),
        })
        .collect();
    let mut best = f64::INFINITY;
    for m in &path.moves {
        for (a, b) in m.segments3(0.005) {
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt();
            let steps = (len / 0.15).ceil().clamp(1.0, 2000.0) as usize;
            for k in 0..=steps {
                let s = k as f64 / steps as f64;
                let c = p3(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                for tb in &boxes {
                    let bx = (c.x - tb.x1).max(tb.x0 - c.x).max(0.0);
                    let by = (c.y - tb.y1).max(tb.y0 - c.y).max(0.0);
                    let lb = (bx.hypot(by) - r).max(0.0).hypot((c.z - tb.z1).max(0.0));
                    if lb >= best {
                        continue;
                    }
                    const G: usize = 6;
                    for u in 0..=G {
                        for v in 0..=(G - u) {
                            let w = G - u - v;
                            let fu = u as f64 / G as f64;
                            let fv = v as f64 / G as f64;
                            let fw = w as f64 / G as f64;
                            let q = p3(
                                tb.tri.a.x * fu + tb.tri.b.x * fv + tb.tri.c.x * fw,
                                tb.tri.a.y * fu + tb.tri.b.y * fv + tb.tri.c.y * fw,
                                tb.tri.a.z * fu + tb.tri.b.z * fv + tb.tri.c.z * fw,
                            );
                            best = best.min(dist_point_to_cylinder(q, c, r));
                        }
                    }
                }
            }
        }
    }
    best
}

fn analytic_plate_dome_dist(c: Point3, r: f64) -> f64 {
    let dx = (c.x.abs() - HALF).max(0.0);
    let dy = (c.y.abs() - HALF).max(0.0);
    let plate_dr = (dx.hypot(dy) - r).max(0.0);
    let plate_dz = (c.z - PLATE).max(0.0);
    let d_plate = if c.z <= PLATE && plate_dr == 0.0 { 0.0 } else { plate_dr.hypot(plate_dz) };

    let rho_t = (c.x.hypot(c.y) - r).max(0.0);
    let dz_c = c.z - SPHERE_ZC;
    let d_dome = if dz_c > 0.0 && rho_t * (PLATE - SPHERE_ZC) <= dz_c * DOME_A {
        (rho_t.hypot(dz_c) - SPHERE_R).max(0.0)
    } else {
        (rho_t - DOME_A).max(0.0).hypot((c.z - PLATE).max(0.0))
    };
    d_plate.min(d_dome)
}

fn min_envelope_distance_analytic(path: &Toolpath, r: f64) -> f64 {
    let mut best = f64::INFINITY;
    for m in &path.moves {
        for (a, b) in m.segments3(0.005) {
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt();
            let steps = (len / 0.1).ceil().clamp(1.0, 2000.0) as usize;
            for k in 0..=steps {
                let s = k as f64 / steps as f64;
                let c = p3(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                best = best.min(analytic_plate_dome_dist(c, r));
            }
        }
    }
    best
}

#[test]
fn rough_finish_dome_and_plate_keep_axial_and_radial_allowance() {
    let facets = rough_finish_mesh(40);
    let region = stock_50_footprint(&facets, PocketStrategy::Auto, false, false);
    let p = rough_params(PocketStrategy::Auto, Entry3D::Helix);
    let path = pocket3d(&facets, &region, &p).expect("rough-finish 3D pocket");

    let min_mesh = min_envelope_distance_mesh(&path, &facets, TOOL_R);
    let min_analytic = min_envelope_distance_analytic(&path, TOOL_R);
    assert!(
        min_mesh >= LEAVE - ALLOW_TOL,
        "mesh tool-envelope distance {min_mesh:.4} mm fell below allowance {LEAVE} - {ALLOW_TOL}"
    );
    assert!(
        min_analytic >= LEAVE - ALLOW_TOL,
        "analytic tool-envelope distance {min_analytic:.4} mm fell below allowance {LEAVE} - {ALLOW_TOL}"
    );

    // Z18 level must stay radially clear of the dome crown (must not pass near (0, 0) at Z18).
    let z18_near_apex = path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).any(|m| {
        m.segments3(0.01).into_iter().any(|(a, b)| {
            a.z.min(b.z) <= 18.0 + 1e-6 && a.x.hypot(a.y).min(b.x.hypot(b.y)) < TOOL_R + LEAVE
        })
    });
    assert!(!z18_near_apex, "Z18 cut passed over the dome apex");

    // Plate floor must be cut at Z8.5 (8.0 + 0.5) and never down to Z8.0.
    let has_floor_allowance = path.moves.iter().any(|m| {
        m.kind == MoveKind::Feed && (m.from.z - 8.5).abs() < 1e-6 && (m.to.z - 8.5).abs() < 1e-6
    });
    assert!(has_floor_allowance, "expected plate floor cut at Z8.5");
    let min_z = path.moves.iter().map(|m| m.from.z.min(m.to.z)).fold(f64::INFINITY, f64::min);
    assert!(min_z >= 8.5 - 1e-6, "tool descended to Z {min_z:.4} below floor allowance Z8.5");
    assert_eq!(verify_clean(&facets, &region, &path, 22.0), Vec::new());
}

#[test]
fn dome_allowance_holds_across_strategies_shell_and_rest() {
    let facets = rough_finish_mesh(24);
    for (strategy, shell, rest) in [
        (PocketStrategy::Waterline, false, false),
        (PocketStrategy::OffsetRings, false, false),
        (PocketStrategy::Adaptive, false, false),
        (PocketStrategy::OffsetRings, true, false),
        (PocketStrategy::Zigzag, false, true),
    ] {
        let region = stock_50_footprint(&facets, strategy, shell, rest);
        let p = rough_params(strategy, Entry3D::Plunge);
        let path = if rest {
            let (tp, rep) = pocket3d_with_rest(&facets, &region, &p).expect("rest pocket");
            assert_eq!(rep.gouge_cells, 0, "{strategy:?}: rest gouged");
            tp
        } else if shell {
            pocket3d_offset(&facets, &region, &p).expect("offset shell")
        } else {
            pocket3d(&facets, &region, &p).expect("plain pocket")
        };
        let min_mesh = min_envelope_distance_mesh(&path, &facets, TOOL_R);
        assert!(
            min_mesh >= LEAVE - ALLOW_TOL,
            "{strategy:?} shell={shell} rest={rest}: min distance {min_mesh:.4} < {}",
            LEAVE - ALLOW_TOL
        );
        assert_eq!(verify_clean(&facets, &region, &path, 22.0), Vec::new(), "{strategy:?}");
    }
}

#[test]
fn flat_top_boss_and_floor_both_keep_axial_allowance() {
    // 50 x 50 x 8 plate with a centered 16 x 16 flat-topped boss reaching Z18.
    let mut facets = box_facets([-HALF, -HALF, 0.0], [HALF, HALF, PLATE]);
    facets.extend(box_facets([-8.0, -8.0, PLATE], [8.0, 8.0, 18.0]));
    let region = stock_50_footprint(&facets, PocketStrategy::Zigzag, false, false);
    let p = rough_params(PocketStrategy::Zigzag, Entry3D::Plunge);
    let path = pocket3d(&facets, &region, &p).expect("boss pocket");

    let min_mesh = min_envelope_distance_mesh(&path, &facets, TOOL_R);
    assert!(min_mesh >= LEAVE - ALLOW_TOL, "boss min distance {min_mesh:.4} mm");
    // Flat top at Z18 gets Z18.5; flat plate at Z8 gets Z8.5.
    for want_z in [18.5, 8.5] {
        assert!(
            path.moves.iter().any(|m| m.kind == MoveKind::Feed && (m.to.z - want_z).abs() < 1e-6),
            "missing allowance level Z {want_z}"
        );
    }
    assert_eq!(verify_clean(&facets, &region, &path, 22.0), Vec::new());
}

#[test]
fn through_pocket_without_floor_cuts_to_bottom_z_and_keeps_wall_allowance() {
    // 40 x 40 block with a 20 x 20 through hole; no horizontal floor exists at bottom_z.
    let mut facets = Vec::new();
    facets.extend(box_facets([-20.0, -20.0, 0.0], [-10.0, 20.0, 10.0]));
    facets.extend(box_facets([10.0, -20.0, 0.0], [20.0, 20.0, 10.0]));
    facets.extend(box_facets([-10.0, -20.0, 0.0], [10.0, -10.0, 10.0]));
    facets.extend(box_facets([-10.0, 10.0, 0.0], [10.0, 20.0, 10.0]));
    let region = rect(-10.0, -10.0, 10.0, 10.0);
    let p = Pocket3DParams {
        links: Links::ROUGH3D,
        top_z: 10.0,
        bottom_z: 0.0,
        stepdown: 5.0,
        clearance_z: 15.0,
        ..rough_params(PocketStrategy::Zigzag, Entry3D::Plunge)
    };
    let path = pocket3d(&facets, &region, &p).expect("through pocket");
    let min_z = path.moves.iter().map(|m| m.from.z.min(m.to.z)).fold(f64::INFINITY, f64::min);
    assert!((min_z - 0.0).abs() < 1e-6, "through pocket must reach bottom_z 0.0, got {min_z}");
    let min_mesh = min_envelope_distance_mesh(&path, &facets, TOOL_R);
    assert!(min_mesh >= LEAVE - ALLOW_TOL, "wall allowance {min_mesh:.4} mm");
}

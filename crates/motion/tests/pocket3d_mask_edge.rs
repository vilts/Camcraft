//! Issue #1749 (reopened): a 3D Pocket that runs after an op which
//! already cleared the stock past the part's open edge (an outside
//! profile) still clears its notch out to that edge. The re-machining
//! mask's boundary is air that the earlier op cut, not a wall: before
//! the fix the ladder eroded the MASKED target by the radius, so the
//! cutter stayed a radius off the mask boundary and a band one radius
//! wide stood along the open edge at the full stock height. That is
//! the ridge that held a user's soft jaws out of Setup 2.
//!
//! Provenance: our own design (the M3.1 D mask law, read with the
//! "tool centre on boundary" containment of public CAM documentation);
//! no vendor material consulted.
//!
//! Gate map (#1749): covers camcraft_motion::machining3d (ladder, open_side).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{open_sides, pocket3d_offset_prev, pocket3d_prev, Entry3D, Pocket3DParams};
use camcraft_motion::stock_sim::{Columns, Dexel};

/// The cutter radius.
const R: f64 = 4.0;
/// The notch floor (the slab's top) and the wall's top.
const FLOOR: f64 = -10.0;
const TOP: f64 = 0.0;
/// The stock top the mask stands at where nothing has cut yet.
const STOCK_TOP: f64 = 1.0;
/// The part's half width: the silhouette is [-20, 20]^2.
const H: f64 = 20.0;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A solid axis box as twelve outward triangles.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::with_capacity(12);
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

/// A slab to the floor, and a wall over its -X half up to TOP: the +X
/// half is a notch open on its +X, +Y and -Y sides. Its inner wall at
/// X 0 meets the open +Y edge at (0, 20).
fn notched_part() -> Vec<Triangle> {
    let mut f = box_facets([-H, -H, -20.0], [H, H, FLOOR]);
    f.extend(box_facets([-H, -H, FLOOR], [0.0, H, TOP]));
    f
}

/// The whole-model footprint the app hands the ladder: the silhouette
/// grown past its open sides (`open_sides`, the #1749 law).
fn footprint(f: &[Triangle]) -> Region {
    let silhouette = camcraft_motion::geometry::slice::project(f, 64).expect("silhouette");
    open_sides(&silhouette, R, &Region::empty(), None).expect("open sides")
}

/// What the op ahead left: an outside profile cleared every column
/// outside the silhouette to the floor; inside it, the stock stands.
fn mask() -> Dexel {
    let mut d = Dexel::block(-30.0, -30.0, 30.0, 30.0, STOCK_TOP, 0.5).expect("dexel");
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let x = d.x0 + (ix as f64 + 0.5) * d.cell;
            let y = d.y0 + (iy as f64 + 0.5) * d.cell;
            if x.abs() > H || y.abs() > H {
                d.set(iy * d.nx + ix, FLOOR);
            }
        }
    }
    d
}

fn params() -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: STOCK_TOP,
        bottom_z: FLOOR,
        stepdown: 2.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 0.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

/// Every feed's tip, sampled at most 0.25 mm apart.
fn feed_samples(t: &Toolpath) -> Vec<Point3> {
    let mut out = Vec::new();
    for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let len = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2) + (m.to.z - m.from.z).powi(2)).sqrt();
        let n = ((len / 0.25).ceil() as usize).clamp(1, 4000);
        for k in 0..=n {
            let s = k as f64 / n as f64;
            let q = m.arc.and_then(|a| a.point_at(m.from, m.to, s)).unwrap_or_else(|| {
                p(m.from.x + (m.to.x - m.from.x) * s, m.from.y + (m.to.y - m.from.y) * s, m.from.z + (m.to.z - m.from.z) * s)
            });
            out.push(q);
        }
    }
    out
}

/// The stock the path leaves over (x, y): the lowest tip whose flat-end
/// disc covers the column, or the stock top where none does.
fn carried(s: &[Point3], x: f64, y: f64) -> f64 {
    s.iter().filter(|q| (q.x - x).hypot(q.y - y) <= R - 1e-6).map(|q| q.z).fold(STOCK_TOP, f64::min)
}

/// No tip goes under the floor, and no disc enters the wall below its top.
fn assert_no_gouge(s: &[Point3]) {
    for q in s {
        assert!(q.z >= FLOOR - 1e-6, "tip at ({:.3}, {:.3}, {:.3}) is under the floor", q.x, q.y, q.z);
        if q.z < TOP - 1e-6 {
            let dx = (-H - q.x).max(q.x).max(0.0);
            let dy = (-H - q.y).max(q.y - H).max(0.0);
            let d = dx.hypot(dy);
            assert!(d >= R - 0.01, "tip at ({:.3}, {:.3}, {:.3}) is {d:.3} from the wall", q.x, q.y, q.z);
        }
    }
}

/// The open edge, from the wall to the notch's far corner, 0.05 mm
/// inside the part: the stock left there is at the floor.
fn assert_open_edge_cleared(s: &[Point3], what: &str) {
    for k in 0..=40 {
        let x = 0.05 + f64::from(k) * (H - 0.1) / 40.0;
        for y in [H - 0.05, -(H - 0.05)] {
            let left = carried(s, x, y);
            assert!(left <= FLOOR + 1e-6, "{what}: stock stands at Z {left:.3} over ({x:.3}, {y:.3}); the floor is {FLOOR}");
        }
    }
}

/// The reopened reading: after an op that cleared past the open edge,
/// the masked ladder still reaches the open edge beside the wall. Before
/// the fix the stock stood at Z 1.000 over (0.05, 19.95).
#[test]
fn a_masked_pocket_still_clears_the_open_edge_beside_a_wall() {
    let f = notched_part();
    let region = footprint(&f);
    let grid = mask();
    let cols = Columns::of_grid(&grid);
    for (what, path) in [
        ("plain ladder", pocket3d_prev(&f, &region, &params(), Some(&cols))),
        ("offset shell", pocket3d_offset_prev(&f, &region, &params(), Some(&cols))),
    ] {
        let path = path.unwrap_or_else(|e| panic!("{what}: the notch generates: {e}"));
        let s = feed_samples(&path);
        assert_no_gouge(&s);
        assert_open_edge_cleared(&s, what);
    }
}

/// The mask still only removes cuts: no cutting move lies where its disc
/// meets no standing column (all the stock past the cleared ring is
/// air), and a fully machined mask still has nothing to cut.
#[test]
fn the_mask_still_keeps_the_cutter_off_cleared_ground() {
    let f = notched_part();
    let region = footprint(&f);
    let grid = mask();
    let path = pocket3d_prev(&f, &region, &params(), Some(&Columns::of_grid(&grid))).expect("the notch generates");
    for q in feed_samples(&path) {
        let dx = (q.x.abs() - H).max(0.0);
        let dy = (q.y.abs() - H).max(0.0);
        assert!(dx.hypot(dy) <= R + 0.01, "a cut at ({:.3}, {:.3}, {:.3}) touches no standing stock", q.x, q.y, q.z);
    }
    let done = Dexel::block(-30.0, -30.0, 30.0, 30.0, FLOOR, 0.5).expect("dexel");
    assert!(pocket3d_prev(&f, &region, &params(), Some(&Columns::of_grid(&done))).is_err(), "nothing standing, nothing to cut");
}

/// Rounded-corner polyline extruded between `z0` and `z1`, star-shaped about `center`.
fn extrude_star(ring: &[(f64, f64)], center: (f64, f64), z0: f64, z1: f64) -> Vec<Triangle> {
    let n = ring.len();
    let mut out = Vec::with_capacity(n * 4);
    let c0 = p(center.0, center.1, z0);
    let c1 = p(center.0, center.1, z1);
    for i in 0..n {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % n];
        out.push(Triangle::new(c1, p(x0, y0, z1), p(x1, y1, z1)));
        out.push(Triangle::new(c0, p(x1, y1, z0), p(x0, y0, z0)));
        out.push(Triangle::new(p(x0, y0, z0), p(x1, y1, z0), p(x1, y1, z1)));
        out.push(Triangle::new(p(x0, y0, z0), p(x1, y1, z1), p(x0, y0, z1)));
    }
    out
}

/// CCW quarter-arc from `a0` to `a1` radians around `(cx, cy)` with radius `rad`.
fn arc_pts(cx: f64, cy: f64, rad: f64, a0: f64, a1: f64, steps: usize) -> Vec<(f64, f64)> {
    (0..=steps)
        .map(|k| {
            let a = a0 + (a1 - a0) * (k as f64) / (steps as f64);
            (cx + rad * a.cos(), cy + rad * a.sin())
        })
        .collect()
}

/// 30 mm cube ([-15, 15]^2) with R3.125 outer corner fillets, notched in ++
/// above Z 19.826 up to Z 34.826, with R3.125 outer corner fillets on the walls.
fn filleted_cube_notch() -> Vec<Triangle> {
    let f = 3.125;
    let h = 15.0;
    let mut base = Vec::new();
    base.extend(arc_pts(h - f, h - f, f, 0.0, std::f64::consts::FRAC_PI_2, 8));
    base.extend(arc_pts(-(h - f), h - f, f, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 8));
    base.extend(arc_pts(-(h - f), -(h - f), f, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2, 8));
    base.extend(arc_pts(h - f, -(h - f), f, 3.0 * std::f64::consts::FRAC_PI_2, std::f64::consts::TAU, 8));
    let mut wall = Vec::new();
    wall.extend(arc_pts(-f, h - f, f, 0.0, std::f64::consts::FRAC_PI_2, 8));
    wall.extend(arc_pts(-(h - f), h - f, f, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 8));
    wall.extend(arc_pts(-(h - f), -(h - f), f, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2, 8));
    wall.extend(arc_pts(h - f, -(h - f), f, 3.0 * std::f64::consts::FRAC_PI_2, std::f64::consts::TAU, 8));
    wall.extend(arc_pts(h - f, -f, f, 0.0, std::f64::consts::FRAC_PI_2, 8));
    wall.push((0.0, 0.0));
    let mut tris = extrude_star(&base, (0.0, 0.0), 4.826, 19.826);
    tris.extend(extrude_star(&wall, (-7.5, -7.5), 19.826, 34.826));
    tris
}

/// Reproduces the user's sc_logo Setup 1 notch geometry and mask:
/// 30 mm cube with R3.125 fillets, 35.08 mm stock, 6.35 mm flat endmill,
/// outside-profile mask leaving 0.254 mm outside the silhouette, and
/// checks Soft jaw (9) (X 0.05..3.17, Y 3.18..15) and Soft jaw (8) (X 3.18..15).
#[test]
fn filleted_notch_after_outside_profile_clears_open_edge_corner_for_jaws() {
    let f = filleted_cube_notch();
    let r = 3.175;
    let h = 15.0;
    let floor = 19.826;
    let stock_top = 34.826;
    let silhouette = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
    let stock_rect = Region::new(vec![
        Loop::new(vec![
            Point2::new(-17.54, -17.54),
            Point2::new(17.54, -17.54),
            Point2::new(17.54, 17.54),
            Point2::new(-17.54, 17.54),
        ])
        .expect("stock loop"),
    ])
    .expect("stock region");
    let region = open_sides(&silhouette, r, &Region::empty(), Some(&stock_rect)).expect("open sides");

    // Outside-profile mask: cleared outside silhouette + 0.254 mm down to Z 3.81
    // at auto_cell(3.175, 35.08, 35.08) = 0.396875 mm.
    let cell = 3.175 * 0.125;
    let mut grid = Dexel::block(-17.54, -17.54, 17.54, 17.54, stock_top, cell).expect("dexel");
    let keep = silhouette.offset(0.254).expect("keep");
    for iy in 0..grid.ny {
        for ix in 0..grid.nx {
            let x = grid.x0 + (ix as f64 + 0.5) * grid.cell;
            let y = grid.y0 + (iy as f64 + 0.5) * grid.cell;
            if !keep.contains(Point2::new(x, y)) {
                grid.set(iy * grid.nx + ix, 3.81);
            }
        }
    }
    let cols = Columns::of_grid(&grid);
    let p3 = Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 2.0 * r,
        stepover: 2.032,
        stock_to_leave: 0.0,
        clearance_z: 60.0,
        top_z: stock_top,
        bottom_z: floor,
        stepdown: 0.15,
        feed: 762.0,
        plunge_feed: 457.2,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.524,
        optimal_load: 0.12,
    };
    let path = pocket3d_offset_prev(&f, &region, &p3, Some(&cols)).expect("filleted notch generates");
    let s = feed_samples(&path);
    let carried_at = |x: f64, y: f64| {
        s.iter()
            .filter(|q| (q.x - x).hypot(q.y - y) <= r - 1e-6)
            .map(|q| q.z)
            .fold(stock_top, f64::min)
    };
    // Jaw (9) zone: X 0.05..3.17, Y 3.18..15.0 (Z 23.4+)
    // Jaw (8) zone: X 3.18..15.0, Y 3.18..15.0, plus symmetric +X open edge.
    for k in 0..=20 {
        let a = 3.18 + f64::from(k) * (h - 0.05 - 3.18) / 20.0;
        for b in [0.05, 1.5, 3.17, 7.5, 14.95] {
            if b == 14.95 && a > h - 3.125 {
                continue; // outside the R3.125 outer corner of the cube
            }
            for (x, y) in [(b, a), (a, b)] {
                let z_left = carried_at(x, y);
                assert!(
                    z_left <= floor + 1e-3,
                    "ridge left at ({x:.3}, {y:.3}): Z={z_left:.3} > floor {floor}"
                );
            }
        }
    }
}

/// A duplicate roughing op with positive `stock_to_leave` leaves that
/// collar on every wall; the mask must ignore that collar and refuse
/// rather than re-cutting around the walls. A subsequent pass with a
/// smaller `stock_to_leave` still cuts the excess allowance down.
#[test]
fn a_redundant_duplicate_with_stock_to_leave_refuses() {
    let f = notched_part();
    let region = footprint(&f);
    let mut p3 = params();
    p3.stock_to_leave = 1.5;
    let first = pocket3d_prev(&f, &region, &p3, None).expect("first pass generates");
    let mut grid = Dexel::block(-30.0, -30.0, 30.0, 30.0, STOCK_TOP, 0.25).expect("dexel");
    for m in &first.moves {
        grid.subtract(m, R, 0.005);
    }
    let cols = Columns::of_grid(&grid);
    let err = pocket3d_prev(&f, &region, &p3, Some(&cols))
        .expect_err("redundant duplicate with stock_to_leave must refuse");
    assert!(err.to_string().contains("nothing to cut"), "{err}");

    let mut finish = params();
    finish.stock_to_leave = 0.2;
    let pass = pocket3d_prev(&f, &region, &finish, Some(&cols))
        .expect("smaller stock_to_leave pass must still cut the roughing collar");
    assert!(pass.moves.iter().any(|m| m.kind == MoveKind::Feed), "finish cuts remaining collar");
}

fn floor_fillet_part() -> (Vec<Triangle>, Region) {
    let mut tris = box_facets([-15.0, -15.0, 4.826], [15.0, 15.0, 19.826]);
    let mut prof: Vec<(f64, f64)> = vec![(-15.0, 19.826), (-15.0, 34.826), (0.0, 34.826), (0.0, 22.951)];
    for k in 1..=8 {
        let a = std::f64::consts::PI + (std::f64::consts::FRAC_PI_2 * k as f64) / 8.0;
        prof.push((3.125 + 3.125 * a.cos(), 22.951 + 3.125 * a.sin()));
    }
    let n = prof.len();
    for k in 0..n {
        let (x0, z0) = prof[k];
        let (x1, z1) = prof[(k + 1) % n];
        tris.push(Triangle::new(p(-7.5, -15.0, 25.0), p(x1, -15.0, z1), p(x0, -15.0, z0)));
        tris.push(Triangle::new(p(-7.5, 15.0, 25.0), p(x0, 15.0, z0), p(x1, 15.0, z1)));
        tris.push(Triangle::new(p(x0, -15.0, z0), p(x1, -15.0, z1), p(x1, 15.0, z1)));
        tris.push(Triangle::new(p(x0, -15.0, z0), p(x1, 15.0, z1), p(x0, 15.0, z0)));
    }
    let sil = camcraft_motion::geometry::slice::project(&tris, 64).expect("floor fillet sil");
    let reg = open_sides(&sil, 3.175, &Region::empty(), None).expect("floor fillet open sides");
    (tris, reg)
}

#[test]
fn adaptive_small_stepdown_microlift_and_helix_stay_crossing_free() {
    use camcraft_motion::links::Links;
    use camcraft_motion::machining2d::{PocketParams, pocket};
    use camcraft_motion::verify::crossed_pairs;
    use std::time::Instant;

    let inner = Region::new(vec![
        Loop::new(vec![
            Point2::new(-15.0, -8.0),
            Point2::new(15.0, -8.0),
            Point2::new(15.0, 8.0),
            Point2::new(-15.0, 8.0),
        ])
        .expect("rect loop"),
    ])
    .expect("rect");
    let reg = inner.offset(8.0).expect("rounded stadium");
    let p2 = PocketParams {
        blend_radius: None,
        tool_diameter: 10.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: -0.30,
        bottom_z: -0.60,
        stepdown: 0.15,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: 0.45,
        blend_turns: false,
        links: Links::ROUGH3D,
    };
    let t0 = Instant::now();
    let path_ulp = pocket(&reg, &p2).expect("ulp path");
    let c_ulp = crossed_pairs(&path_ulp, 0.01);
    eprintln!("p_ulp ({:.2}s): moves={} notes={:?} crossings={}", t0.elapsed().as_secs_f64(), path_ulp.moves.len(), path_ulp.notes, c_ulp.len());
    assert!(c_ulp.is_empty(), "adaptive 0.15 ULP crossings: {c_ulp:?}");
}

#[test]
fn offset_shell_fillet_helix_and_floor_never_cross() {
    use camcraft_motion::links::Links;
    use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};
    use std::time::Instant;

    let (tris, reg) = floor_fillet_part();
    let mut dex = Dexel::block(-25.0, -25.0, 25.0, 25.0, 35.08, 0.5).expect("dex");
    for ix in 0..100 {
        let x = -24.75 + (ix as f64) * 0.5;
        for iy in 0..100 {
            let y = -24.75 + (iy as f64) * 0.5;
            if x.abs() > 16.0 || y.abs() > 16.0 {
                dex.subtract(
                    &camcraft_motion::cl::Move {
                        kind: MoveKind::Feed,
                        from: Point3::new(x, y, 3.81),
                        to: Point3::new(x, y, 3.81),
                        feed: Some(500.0),
                        arc: None,
                        axis: None,
                        rotary: None,
                    },
                    0.5,
                    0.01,
                );
            }
        }
    }
    let cols = Columns::of_grid(&dex);
    let p3 = Pocket3DParams {
        links: Links::ROUGH3D,
        tool_diameter: 6.35,
        stepover: 2.032,
        stock_to_leave: 0.0,
        clearance_z: 60.0,
        top_z: 20.70,
        bottom_z: 19.826,
        stepdown: 0.15,
        feed: 762.0,
        plunge_feed: 457.2,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.524,
        optimal_load: 0.12,
    };
    let t0 = Instant::now();
    let path = pocket3d_offset_prev(&tris, &reg, &p3, Some(&cols)).expect("ff shell");
    let tool = ToolGeom {
        radius: 3.175,
        nose_radius: 0.0,
        flute_len: 14.478,
        shank_radius: 3.175,
        shank_len: 20.0,
        holder_radius: 12.0,
        holder_len: 30.0,
    };
    let cx = VerifyContext {
        region: &reg,
        stock: None,
        model: Some(&tris),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 35.08,
        clearance_z: 60.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    let warns = verify(&path, &cx);
    let crossings: Vec<_> = warns.iter().filter(|w| matches!(w, Warning::Crossing { .. })).collect();
    eprintln!("ff_shell ({:.2}s): total_warns={} crossings={}", t0.elapsed().as_secs_f64(), warns.len(), crossings.len());
    assert!(crossings.is_empty(), "offset shell crossings: {crossings:?}");
}

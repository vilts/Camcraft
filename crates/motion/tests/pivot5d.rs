//! The pivot law over the STAMP-based 5-axis kinds (M13 follow-up):
//! every posed ball must stand where its 3-axis plan put it. The
//! stream locates the TOOL TIP and the post keeps the tilt modal, so
//! the ball a posted program cuts with is `tip + r·a` under the pose
//! the machine HOLDS (the last stated one) - and that sphere must be
//! the planned one, `plan_tip + r·up`, within tolerance, on every cut
//! move, for every kind: contour, parallel, flow, swarf (plain and
//! advanced), multi-axis rough, and the drive-curve contour.
//!
//! The measurement half (`measure`) is the table the fix was specced
//! from: per kind and fixture, the worst placement error of a posted
//! ball centre off the planned centre polyline, the worst air gap of
//! a ball whose plan touched the part, and the worst penetration.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (stamp, curve,
//! retip), camcraft_motion::finishing3d, camcraft_motion::machining3d.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{self, Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d};
use camcraft_motion::multiaxis::{
    AxisControl, CurveParams, contour5d, contour_curve, flow3d, flow5d, multiaxis_rough, parallel5d,
    ruling, swarf5d, swarf5d_advanced,
};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 3.0;
const TOL: f64 = 0.05;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn finish(pattern: FinishPattern, top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern,
        stepover: Some(2.0),
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 40.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
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

fn control() -> AxisControl {
    AxisControl {
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: Some(60.0),
    }
}

fn unclamped() -> AxisControl {
    AxisControl {
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: None,
    }
}

/// A closed solid of revolution about Z from a meridian (rho, z).
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The standing bar: a 10-radius cylinder, 20 tall, capped.
fn bar() -> Vec<Triangle> {
    revolve(64, &[(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)])
}

/// A spherical cap at the origin (the multiaxis suite's dome).
fn dome() -> Vec<Triangle> {
    let (radius, polar_max, n_lat, n_lon) = (15.0, 0.9, 8usize, 16usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    let mut prev = first;
    for k in 2..=n_lat {
        let cur = ring(k);
        for j in 0..n_lon {
            facets.push(Triangle::new(prev[j], cur[j], cur[(j + 1) % n_lon]));
            facets.push(Triangle::new(prev[j], cur[(j + 1) % n_lon], prev[(j + 1) % n_lon]));
        }
        prev = cur;
    }
    facets
}

/// A circular tube about X (the swarf suite's blade).
fn tube() -> Vec<Triangle> {
    let (n_circ, n_x, radius, x_len) = (32usize, 8usize, 12.0, 30.0);
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(-x_len + 2.0 * x_len * i as f64 / n_x as f64, radius * th.cos(), radius * th.sin())
    };
    let mut t = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            t.push(Triangle::new(a, b, c));
            t.push(Triangle::new(a, c, d));
        }
    }
    t
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p3([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1])
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn rough_params() -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 2.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn sub(a: Point3, b: Point3) -> Point3 {
    p3(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn len(a: Point3) -> f64 {
    dot(a, a).sqrt()
}

fn dist_pt_seg(p: Point3, a: Point3, b: Point3) -> f64 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    let t = if l2 > 0.0 { (dot(sub(p, a), d) / l2).clamp(0.0, 1.0) } else { 0.0 };
    len(sub(p, p3(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t)))
}

/// Point-to-triangle distance: the face where the projection lands
/// inside, the three edges everywhere else (textbook, Ericson 5.1).
fn dist_pt_tri(p: Point3, t: &Triangle) -> f64 {
    let mut d = dist_pt_seg(p, t.a, t.b).min(dist_pt_seg(p, t.b, t.c)).min(dist_pt_seg(p, t.c, t.a));
    let (e0, e1) = (sub(t.b, t.a), sub(t.c, t.a));
    let n = e0.cross(e1);
    let nn = dot(n, n);
    if nn > 1e-24 {
        let w = sub(p, t.a);
        let u = dot(w.cross(e1), n) / nn;
        let v = dot(e0.cross(w), n) / nn;
        if u >= -1e-12 && v >= -1e-12 && u + v <= 1.0 + 1e-12 {
            d = d.min(dot(w, n).abs() / nn.sqrt());
        }
    }
    d
}

/// Distance from `p` to the mesh (brute force: these fixtures are
/// small and this is a measurement, not a law).
fn dist_mesh(p: Point3, facets: &[Triangle]) -> f64 {
    facets.iter().map(|t| dist_pt_tri(p, t)).fold(f64::INFINITY, f64::min)
}

/// The ball centres a posted program cuts with: every cut move's
/// chord ends (`segments3`), lifted `r` along the pose the machine
/// HOLDS there - the move's own stated axis, or the last one stated
/// (the post keeps the tilt modal; nothing stated yet is upright).
fn posted_centres(path: &Toolpath, r: f64, keep: fn(MoveKind) -> bool) -> Vec<Point3> {
    let mut held = [0.0, 0.0, 1.0];
    let mut out = Vec::new();
    let mut rapid = false;
    for m in &path.moves {
        let was = held;
        if let Some(a) = m.axis {
            held = a;
        }
        let prev_rapid = std::mem::replace(&mut rapid, m.kind == MoveKind::Rapid);
        if m.kind == MoveKind::Rapid || !keep(m.kind) {
            continue;
        }
        for (j, (a, b)) in m.segments3(1e-3).into_iter().enumerate() {
            // A plunge's start off a rapid (the plan's own entry from
            // clearance, or a swarf air link's re-entry - stamp_link)
            // is air by the clearance law, not a ball of the plan; the
            // plunge's end is, and is measured.
            let entry = m.kind == MoveKind::Plunge && prev_rapid;
            if j == 0 && !entry {
                out.push(p3(a.x + r * was[0], a.y + r * was[1], a.z + r * was[2]));
            }
            out.push(p3(b.x + r * held[0], b.y + r * held[1], b.z + r * held[2]));
        }
    }
    out
}

/// The planned centre polyline of a vertical ball's ladder: tips + r·up.
fn planned_segments(plan: &Toolpath, r: f64) -> Vec<(Point3, Point3)> {
    let mut v = Vec::new();
    for m in &plan.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        for (a, b) in m.segments3(1e-3) {
            v.push((p3(a.x, a.y, a.z + r), p3(b.x, b.y, b.z + r)));
        }
    }
    v
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// Worst distance of a posted ball centre off the planned centres.
    placement: f64,
    /// Worst air gap (`dist - r`) at a posted centre whose nearest
    /// planned centre touched the part (planned gap <= TOL).
    air: f64,
    /// Worst penetration (`r - dist`) at any posted centre, >= 0.
    pen: f64,
    /// Cut stations measured.
    n: usize,
}

fn measure(plan_segs: &[(Point3, Point3)], posted: &Toolpath, facets: &[Triangle], r: f64) -> Row {
    measure_kinds(plan_segs, posted, facets, r, |_| true)
}

fn measure_kinds(
    plan_segs: &[(Point3, Point3)],
    posted: &Toolpath,
    facets: &[Triangle],
    r: f64,
    keep: fn(MoveKind) -> bool,
) -> Row {
    let cs = posted_centres(posted, r, keep);
    let mut row = Row { placement: 0.0, air: 0.0, pen: 0.0, n: cs.len() };
    for c in &cs {
        let (mut best, mut near) = (f64::INFINITY, *c);
        for (a, b) in plan_segs {
            let d = dist_pt_seg(*c, *a, *b);
            if d < best {
                best = d;
                let ab = sub(*b, *a);
                let l2 = dot(ab, ab);
                let t = if l2 > 0.0 { (dot(sub(*c, *a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
                near = p3(a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t);
            }
        }
        row.placement = row.placement.max(best);
        let g = dist_mesh(*c, facets) - r;
        row.pen = row.pen.max(-g);
        if dist_mesh(near, facets) - r <= TOL {
            row.air = row.air.max(g);
        }
    }
    row
}

/// `PIVOT5D_MEASURE_ONLY=1` prints the whole table without judging
/// it (the before/after measurement the fix was specced from).
fn measure_only() -> bool {
    std::env::var_os("PIVOT5D_MEASURE_ONLY").is_some()
}

fn report(name: &str, row: Row) {
    eprintln!(
        "PIVOT5D | {name:<28} | n {:>5} | placement {:>7.3} mm | air-at-contact {:>7.3} mm | penetration {:>7.3} mm",
        row.n, row.placement, row.air, row.pen
    );
}

/// The law every kind answers to: the posted balls ARE the planned
/// balls (placement within the chord budget), no planned contact goes
/// to air, nothing buries.
fn assert_pivot(name: &str, row: Row) {
    report(name, row);
    if measure_only() {
        return;
    }
    assert!(row.n > 0, "{name}: no cut stations measured");
    assert!(row.placement <= 2e-3, "{name}: a posted ball stands {:.4} mm off its plan", row.placement);
    assert!(row.air <= TOL + 2e-3, "{name}: a planned contact posts {:.4} mm of air", row.air);
}

#[test]
fn contour5d_balls_are_the_waterline_balls() {
    for (fx, facets, top, bot) in [("bar", bar(), 19.0, 2.0), ("dome", dome(), 14.0, 9.8)] {
        for (cn, c) in [("lead15/60", control()), ("unclamped", unclamped())] {
            let p = finish(FinishPattern::Waterline, top, bot);
            let plan = finishing3d::waterline(&facets, &p).unwrap();
            let got = contour5d(&facets, &p, &c, None).unwrap();
            let row = measure(&planned_segments(&plan, R), &got, &facets, R);
            assert_pivot(&format!("contour5d {fx} {cn}"), row);
            // Penetration rides the planned ladder's own: the waterline
            // grants the facet chords their tolerance, nothing more.
            let base = measure(&planned_segments(&plan, R), &plan, &facets, R);
            report(&format!("  (plan itself) {fx}"), base);
            assert!(measure_only() || row.pen <= base.pen + 2e-3, "contour5d {fx}: posed buries {:.4} vs plan {:.4}", row.pen, base.pen);
        }
    }
}

#[test]
fn parallel5d_balls_are_the_raster_balls() {
    for (fx, facets, top, bot) in [("dome", dome(), 15.0, 9.8), ("bar", bar(), 20.0, 2.0)] {
        let p = finish(FinishPattern::Parallel, top, bot);
        let plan = finishing3d::parallel(&facets, &p).unwrap();
        let got = parallel5d(&facets, &p, &control(), None).unwrap();
        let row = measure(&planned_segments(&plan, R), &got, &facets, R);
        assert_pivot(&format!("parallel5d {fx}"), row);
        let base = measure(&planned_segments(&plan, R), &plan, &facets, R);
        report(&format!("  (plan itself) {fx}"), base);
        assert!(measure_only() || row.pen <= base.pen + 2e-3, "parallel5d {fx}: posed buries {:.4} vs plan {:.4}", row.pen, base.pen);
    }
}

#[test]
fn flow5d_balls_are_the_flowline_balls() {
    let facets = dome();
    let p = finish(FinishPattern::Parallel, 15.0, 9.8);
    let plan = flow3d(&facets, &p).unwrap();
    let got = flow5d(&facets, &p, &control(), None).unwrap();
    assert_pivot("flow5d dome", measure(&planned_segments(&plan, R), &got, &facets, R));
}

#[test]
fn swarf_balls_are_the_swarf_raster_balls() {
    let blade = tube();
    let p = finish(FinishPattern::Parallel, 11.0, -11.0);
    let f = ruling::analyze(&blade).unwrap();
    let plan = finishing3d::swarf(&blade, &p, f.ux, f.uy).unwrap();
    let got = swarf5d(&blade, &p, &control()).unwrap();
    assert_pivot("swarf tube", measure(&planned_segments(&plan, R), &got, &blade, R));
    let plan = finishing3d::swarf_multi(&blade, &p, f.ux, f.uy, 2).unwrap();
    let got = swarf5d_advanced(&blade, &p, &control(), 2).unwrap();
    report("  (plan itself) adv swarf", measure(&planned_segments(&plan, R), &plan, &blade, R));
    assert_pivot("advanced swarf tube", measure(&planned_segments(&plan, R), &got, &blade, R));
}

#[test]
fn multiaxis_rough_balls_are_the_ladder_balls() {
    let f = slab_with_boss();
    let region = camcraft_motion::geometry::slice::project(&f, 64).unwrap();
    let plan = pocket3d(&f, &region, &rough_params()).unwrap();
    let got = multiaxis_rough(&f, &region, &rough_params(), &control(), None).unwrap();
    let row = measure(&planned_segments(&plan, R), &got, &f, R);
    report("multiaxis rough boss", row);
    assert!(measure_only() || row.placement <= 2e-3, "rough: a posted ball stands {:.4} mm off its plan", row.placement);
}

/// The capped blade of the app's swarf suite (16 x 6, caps fanned).
fn capped_blade() -> Vec<Triangle> {
    let (radius, x_len, n_circ, n_x) = (12.0, 24.0, 16usize, 6usize);
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(-x_len + 2.0 * x_len * i as f64 / n_x as f64, radius * th.cos(), radius * th.sin())
    };
    let mut t = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            t.push(Triangle::new(a, b, c));
            t.push(Triangle::new(a, c, d));
        }
    }
    for j in 0..n_circ {
        t.push(Triangle::new(p3(x_len, 0.0, 0.0), v(j, n_x), v(j + 1, n_x)));
        t.push(Triangle::new(p3(-x_len, 0.0, 0.0), v(j + 1, 0), v(j, 0)));
    }
    t
}

/// The envelope scan (the verifier's own law (f), posed lift and all)
/// reads every posed kind exactly as it reads its upright plan: the
/// pose moved no ball, so the measured gouge and rest are the plan's.
#[test]
#[ignore = "verdict lane: envelope scans of posed paths; release seconds - run with -- --ignored"]
fn the_envelope_reads_every_posed_kind_as_its_plan() {
    use camcraft_sim::stock_sim::envelope_scan;
    let mut p = finish(FinishPattern::Parallel, 11.0, -11.0);
    p.tolerance = 0.1;
    let blade = capped_blade();
    let f = ruling::analyze(&blade).unwrap();
    type Case = (&'static str, Vec<Triangle>, Toolpath, Toolpath);
    let cases: Vec<Case> = vec![
        ("swarf blade", blade.clone(), finishing3d::swarf(&blade, &p, f.ux, f.uy).unwrap(), swarf5d(&blade, &p, &control()).unwrap()),
        (
            "adv swarf blade",
            blade.clone(),
            finishing3d::swarf_multi(&blade, &p, f.ux, f.uy, 2).unwrap(),
            swarf5d_advanced(&blade, &p, &control(), 2).unwrap(),
        ),
        (
            "contour5d bar",
            bar(),
            finishing3d::waterline(&bar(), &finish(FinishPattern::Waterline, 19.0, 2.0)).unwrap(),
            contour5d(&bar(), &finish(FinishPattern::Waterline, 19.0, 2.0), &control(), None).unwrap(),
        ),
        (
            "parallel5d dome",
            dome(),
            finishing3d::parallel(&dome(), &finish(FinishPattern::Parallel, 15.0, 9.8)).unwrap(),
            parallel5d(&dome(), &finish(FinishPattern::Parallel, 15.0, 9.8), &control(), None).unwrap(),
        ),
    ];
    for (name, facets, plan, got) in cases {
        let a = envelope_scan(&facets, &plan.moves, R, 0.1).unwrap();
        let b = envelope_scan(&facets, &got.moves, R, 0.1).unwrap();
        eprintln!(
            "PIVOT5D | scan {name:<20} | plan gouge {:.4} rest {:.4} | posed gouge {:.4} rest {:.4}",
            a.gouge_mm, a.rest_mm, b.gouge_mm, b.rest_mm
        );
        assert!(b.gouge_mm <= a.gouge_mm + 0.011, "{name}: the pose bought a gouge");
        assert!(b.rest_mm <= a.rest_mm + 0.011, "{name}: the pose bought rest");
    }
}

/// The drive-curve ring round a box's top rim (four convex edges,
/// four convex corners): every station's ball touches the part
/// (gap 0), and no chord between stations sags into an edge or cuts
/// a corner - the straight centre chord between two corner rests cut
/// 0.55 mm into the edge before the walk was densified.
#[test]
fn contour_curve_rides_a_convex_rim_without_cutting_it() {
    let facets = box_facets([0.0, 0.0, 0.0], [10.0, 10.0, 5.0]);
    let cp = CurveParams {
        tool_diameter: 2.0 * R,
        clearance_z: 25.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let ring = vec![p3(0.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), p3(10.0, 10.0, 5.0), p3(0.0, 10.0, 5.0), p3(0.0, 0.0, 5.0)];
    let got = contour_curve(&[ring], &facets, &cp, &control(), None).unwrap();
    let cs = posted_centres(&got, R, |k| k == MoveKind::Feed);
    let (mut air, mut pen) = (0.0f64, 0.0f64);
    for w in cs.windows(2) {
        for c in [w[0], p3(0.5 * (w[0].x + w[1].x), 0.5 * (w[0].y + w[1].y), 0.5 * (w[0].z + w[1].z))] {
            let g = dist_mesh(c, &facets) - R;
            air = air.max(g);
            pen = pen.max(-g);
        }
    }
    eprintln!("PIVOT5D | curve convex rim ring       | n {:>5} | air {air:.4} mm | penetration (chords) {pen:.4} mm", cs.len());
    assert!(air <= 1e-6 + 0.003, "a rim station floats {air} mm");
    assert!(pen <= 0.003, "a rim chord cuts {pen} mm into the part");
}

/// The drive-curve contour plans BALL CENTRES touching the chain; the
/// plan is read off the chain directly: on the box's CONVEX top-front
/// edge the ball rides the edge line, centre a radius out along the
/// bisector, `(y, z) = (-r/√2, 15 + r/√2)` (the old plane law stood it
/// at `(-r, 15 + r)`, in air); on the flat top `z = 15 + r`.
const H: f64 = R * std::f64::consts::FRAC_1_SQRT_2;

#[test]
fn contour_curve_balls_are_the_station_balls() {
    let facets = box_facets([0.0, 0.0, 0.0], [40.0, 40.0, 15.0]);
    let cp = CurveParams {
        tool_diameter: 2.0 * R,
        clearance_z: 25.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    type Case = (&'static str, Vec<Point3>, Vec<(Point3, Point3)>);
    let cases: [Case; 2] = [
        (
            "curve top-front edge",
            vec![p3(5.0, 0.0, 15.0), p3(20.0, 0.0, 15.0), p3(35.0, 0.0, 15.0)],
            vec![(p3(5.0, -H, 15.0 + H), p3(35.0, -H, 15.0 + H))],
        ),
        (
            "curve flat top",
            vec![p3(5.0, 20.0, 15.0), p3(35.0, 20.0, 15.0)],
            vec![(p3(5.0, 20.0, 15.0 + R), p3(35.0, 20.0, 15.0 + R))],
        ),
    ];
    for (name, chain, plan) in cases {
        let got = contour_curve(&[chain], &facets, &cp, &control(), None).unwrap();
        // The curve's stations only: the plunge from the hop plane is
        // air by design, so measure the feeds.
        assert_pivot(name, measure_kinds(&plan, &got, &facets, R, |k| k == MoveKind::Feed));
    }
}

/// #3414: the approach never turns the rotaries while it descends.
/// The first pose posts as its own zero-travel rapid (the swing) on
/// the hop plane BEFORE the plunge, and the plunge holds the pose the
/// swing already stated. The law over the whole stream: a pose change
/// never rides the move that crosses from air to material - every cut
/// move that follows a rapid holds the pose that rapid travelled
/// under. The leans between two cut moves keep the modal law: the
/// tool tilts while it is already in the cut, never on the way in.
fn no_pose_change_rides_the_air_to_cut_move(name: &str, path: &Toolpath) {
    let mut prev_axis: Option<[f64; 3]> = None;
    let mut prev_kind = MoveKind::Rapid;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid && prev_kind == MoveKind::Rapid {
            assert_eq!(
                m.axis, prev_axis,
                "{name}: move {i} turns the rotaries while it reaches the part"
            );
        }
        prev_axis = m.axis;
        prev_kind = m.kind;
    }
}

fn is_upright(a: [f64; 3]) -> bool {
    a[0].abs() < 1e-9 && a[1].abs() < 1e-9
}

#[test]
fn the_approach_rotates_before_it_descends() {
    let facets = box_facets([0.0, 0.0, 0.0], [40.0, 40.0, 15.0]);
    let cp = CurveParams {
        tool_diameter: 2.0 * R,
        clearance_z: 25.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let chain = vec![p3(5.0, 0.0, 15.0), p3(20.0, 0.0, 15.0), p3(35.0, 0.0, 15.0)];
    let got = contour_curve(&[chain], &facets, &cp, &control(), None).unwrap();
    no_pose_change_rides_the_air_to_cut_move("curve edge", &got);
    // The swing itself: a zero-travel rapid that states the first
    // lean, ahead of every cut move.
    let swing = got
        .moves
        .iter()
        .position(|m| m.kind == MoveKind::Rapid && m.from == m.to && m.axis.is_some_and(|a| !is_upright(a)))
        .unwrap_or_else(|| panic!("no zero-travel rotary swing before the first descend"));
    let first_cut = got.moves.iter().position(|m| m.kind != MoveKind::Rapid).unwrap();
    assert!(swing < first_cut, "the swing posted after the first cut move");
    // The plunge descends under the pose the swing stated.
    let plunge = &got.moves[first_cut];
    assert_eq!(plunge.kind, MoveKind::Plunge, "the approach descend is a plunge");
    assert_eq!(plunge.axis, got.moves[swing].axis, "the plunge does not hold the pose the swing stated");
    // The same law over the STAMP-based kinds.
    let dome = dome();
    let fp = finish(FinishPattern::Parallel, 15.0, 9.8);
    no_pose_change_rides_the_air_to_cut_move("parallel5d", &parallel5d(&dome, &fp, &control(), None).unwrap());
    no_pose_change_rides_the_air_to_cut_move("flow5d", &flow5d(&dome, &fp, &control(), None).unwrap());
    let blade = capped_blade();
    let sp = finish(FinishPattern::Parallel, 11.0, -11.0);
    no_pose_change_rides_the_air_to_cut_move("swarf5d", &swarf5d(&blade, &sp, &control()).unwrap());
    // The rough ladder over the slab with a boss: its walls lean and
    // its floors cut upright, so a floor approach that follows a
    // link rapid under a lean must swing back to upright at a
    // standstill first - the upright arms of the stamp (#3414).
    let slab = slab_with_boss();
    let region = camcraft_motion::geometry::slice::project(&slab, 64).unwrap();
    no_pose_change_rides_the_air_to_cut_move(
        "multiaxis_rough",
        &multiaxis_rough(&slab, &region, &rough_params(), &control(), None).unwrap(),
    );
    // The waterline rings of the dome complete the stamped kinds.
    no_pose_change_rides_the_air_to_cut_move(
        "contour5d",
        &contour5d(&dome, &finish(FinishPattern::Waterline, 14.0, 9.8), &control(), None).unwrap(),
    );
}

//! M5.1 tail flowline law tests: what the seeded-tracer kernel
//! promises (every ground point within half a stepover of a traced
//! line, the march's honesty at line ends, the refusal set) and
//! what the 5-axis field glue composes (an axis on every cut move,
//! the min-curvature directions steering the lines WITH a blade,
//! plain ground degrading to the fallback's straight passes).
//! M2.5 E adds the OTHER door on the same field: `flow3d` - the
//! flowline finish asked of a 3-axis machine, tested here as the
//! law that its posed twin is the same ground cut upright.
//!
//! The envelope VERDICTS - the measured gouge/rest of a flow path
//! over the dome - ride the ignored lane, as every envelope test in
//! this codebase does.
//!
//! Gate map (M4.4): covers camcraft_motion::finishing3d (the flow
//! kernel) and camcraft_motion::multiaxis (the field, the stamp
//! composition).
//! Lane shape (M4.4): the law tests run in seconds; the envelope
//! verdict is #[ignore]d - run it with
//! `cargo test --release -p camcraft-app -- --ignored`.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{flow, scallop_stepover, Finish3DParams, FinishPattern};
use camcraft_motion::geo2d::Point2;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::multiaxis::{flow3d, flow5d, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 3.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn finish(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.05,
        clearance_z: 30.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        // M8.2 B: the ring entry keeps its plunge unless asked.
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// A grid of quads over the graph z = f(x,y).
fn graph_grid(n: usize, span: f64, f: impl Fn(f64, f64) -> f64) -> Vec<Triangle> {
    let g = |i: usize, j: usize| {
        let (x, y) = (-span + 2.0 * span * i as f64 / n as f64, -span + 2.0 * span * j as f64 / n as f64);
        p3(x, y, f(x, y))
    };
    let mut v = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b, c, d) = (g(i, j), g(i + 1, j), g(i + 1, j + 1), g(i, j + 1));
            v.push(Triangle::new(a, b, c));
            v.push(Triangle::new(a, c, d));
        }
    }
    v
}

/// A circular tube about the X axis: every facet's least-curving
/// direction is x-hat, so the field must run the lines WITH it.
fn tube_about(n_circ: usize, n_x: usize, radius: f64, x_len: f64) -> Vec<Triangle> {
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

/// A vertical cylinder wall about Z: every facet steep, the rings'
/// whole model (the flow stage must add nothing and refuse nothing).
fn wall(n_circ: usize, n_z: usize, radius: f64, height: f64) -> Vec<Triangle> {
    let v = |j: usize, k: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(radius * th.cos(), radius * th.sin(), height * k as f64 / n_z as f64)
    };
    let mut t = Vec::new();
    for j in 0..n_circ {
        for k in 0..n_z {
            let (a, b, c, d) = (v(j, k), v(j + 1, k), v(j + 1, k + 1), v(j, k + 1));
            t.push(Triangle::new(a, b, c));
            t.push(Triangle::new(a, c, d));
        }
    }
    t
}

fn straight(_: Point2) -> (f64, f64) {
    (1.0, 0.0)
}

/// The swirl field: circles about the origin. The kernel never sees
/// a closed loop from one seed (the half-line walks its circle and
/// the budget ends it), so this is the family's most curved test.
fn swirl(q: Point2) -> (f64, f64) {
    (-q.y, q.x)
}

/// The cut chords of a path: every non-rapid move's XY segment.
fn chords(t: &camcraft_motion::cl::Toolpath) -> Vec<(Point2, Point2)> {
    t.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y)))
        .filter(|(a, b)| (b.x - a.x).hypot(b.y - a.y) > 1e-9)
        .collect()
}

fn dist2_seg(q: Point2, a: Point2, b: Point2) -> f64 {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let ee = ex * ex + ey * ey;
    let t = if ee > 0.0 {
        (((q.x - a.x) * ex + (q.y - a.y) * ey) / ee).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (q.x - a.x - t * ex).hypot(q.y - a.y - t * ey)
}

#[test]
fn flow_covers_straight_ground_to_the_half_step_law() {
    let plate = graph_grid(16, 40.0, |_, _| 0.0);
    let p = finish(2.0, -0.5);
    let step = scallop_stepover(R, p.tolerance);
    let t = flow(&plate, &p, &straight).expect("flow over plain ground");
    assert!(t.all_finite());
    let cs = chords(&t);
    assert!(cs.len() > 40, "only {} cut chords over a plate", cs.len());
    // Every interior point within half a step (plus the march's
    // lattice quantum) of a traced chord: the cusp law's measure.
    let n = 30;
    for i in 0..=n {
        for j in 0..=n {
            let q = Point2::new(-38.0 + 76.0 * i as f64 / n as f64, -38.0 + 76.0 * j as f64 / n as f64);
            let d = cs.iter().map(|(a, b)| dist2_seg(q, *a, *b)).fold(f64::INFINITY, f64::min);
            assert!(d <= 0.6 * step, "({:?}) stands {d:.3} from every line (law {:.3})", q, 0.5 * step);
        }
    }
}

#[test]
fn flow_lines_turn_with_a_turning_field() {
    let plate = graph_grid(16, 40.0, |_, _| 0.0);
    let p = finish(2.0, -0.5);
    let step = scallop_stepover(R, p.tolerance);
    let t = flow(&plate, &p, &swirl).expect("flow over a swirl field");
    assert!(t.all_finite());
    let cs = chords(&t);
    // The lines really curved: chords running WITH x (a swirl line
    // crosses x-hat at the axes) and chords running ACROSS it both
    // appear - a straight-line family could not do both.
    let (mut along, mut across) = (0, 0);
    for (a, b) in &cs {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = (dx * dx + dy * dy).sqrt();
        let ax = dx.abs() / l;
        if ax > 0.9 {
            along += 1;
        } else if ax < 0.3 {
            across += 1;
        }
    }
    assert!(along > 20 && across > 20, "the swirl never turned the path: {along} along, {across} across");
    // Coverage still holds, a little looser where the circles'
    // radial spacing quantizes: the CLAIM + half-seed bound.
    let n = 20;
    for i in 0..=n {
        for j in 0..=n {
            let q = Point2::new(-36.0 + 72.0 * i as f64 / n as f64, -36.0 + 72.0 * j as f64 / n as f64);
            if q.x.hypot(q.y) > 36.0 {
                continue;
            }
            let d = cs.iter().map(|(a, b)| dist2_seg(q, *a, *b)).fold(f64::INFINITY, f64::min);
            assert!(d <= 1.1 * step, "swirl left ({:?}) {d:.3} uncovered", q);
        }
    }
}

/// The radial field: lines through the origin, both ways. The
/// DIVERGING family - the convergence law's dual, and the one the
/// straight-line tests cannot see: rays fan out as they run, so
/// the seed lattice alone decides how many of them survive, and
/// the gaps between them grow with radius.
fn radial(q: Point2) -> (f64, f64) {
    let l = q.x.hypot(q.y);
    if l < 1e-12 {
        (1.0, 0.0)
    } else {
        (q.x / l, q.y / l)
    }
}

#[test]
fn flow_keeps_its_cusp_over_a_diverging_field() {
    let plate = graph_grid(16, 40.0, |_, _| 0.0);
    let p = finish(2.0, -0.5);
    let step = scallop_stepover(R, p.tolerance);
    let t = flow(&plate, &p, &radial).expect("flow over a radial field");
    assert!(t.all_finite());
    let cs = chords(&t);
    assert!(cs.len() > 40, "only {} chords over a radial field", cs.len());
    let mut worst = 0.0f64;
    let n = 24;
    for i in 0..=n {
        for j in 0..=n {
            let q = Point2::new(-38.0 + 76.0 * i as f64 / n as f64, -38.0 + 76.0 * j as f64 / n as f64);
            let d = cs.iter().map(|(a, b)| dist2_seg(q, *a, *b)).fold(f64::INFINITY, f64::min);
            worst = worst.max(d);
        }
    }
    assert!(
        worst <= 0.6 * step,
        "the diverging family left {worst:.3} uncovered - past the raster's promise ({:.3})",
        0.5 * step
    );
}

#[test]
fn a_stepover_asking_for_hours_refuses_by_name() {
    let plate = graph_grid(4, 100.0, |_, _| 0.0);
    let mut p = finish(2.0, -0.5);
    p.stepover = Some(0.005);
    match flow(&plate, &p, &straight).err() {
        Some(GenError::Invalid(s)) => assert!(s.contains("flow needs"), "got {s}"),
        other => panic!("the seed cap spoke {other:?}, not a named refusal"),
    }
}

#[test]
fn a_wall_only_model_is_left_to_the_rings() {
    let w = wall(32, 8, 15.0, 40.0);
    let t = flow(&w, &finish(38.0, 2.0), &straight).expect("rings carry an all-steep model");
    assert!(t.all_finite());
    assert!(t.moves.iter().any(|m| m.kind != MoveKind::Rapid));
}

#[test]
fn degenerate_ground_refuses_or_stays_whole_never_crashes() {
    assert!(flow(&[], &finish(2.0, 0.0), &straight).err().is_some());
    let sliver = vec![Triangle::new(p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(2.0, 0.0, 0.0))];
    if let Ok(t) = flow(&sliver, &finish(2.0, -0.5), &straight) {
        // Tracing a sliver is allowed; half-tracing it is not. The
        // refusal branch is the other honest answer - either way the
        // law is that a degenerate model never panics.
        assert!(t.all_finite());
    }
}

#[test]
fn flow5d_states_an_axis_on_every_cut_move() {
    let plate = graph_grid(12, 40.0, |_, _| 0.0);
    let c = AxisControl {
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: None,
    };
    let t = flow5d(&plate, &finish(2.0, -0.5), &c, None).expect("flow over plain ground poses");
    assert!(!t.is_prismatic(), "a lead angle spoke no axis anywhere");
    let mut n = 0;
    // #3300: the moves before the first lean state the upright Z
    // column (the post's known start); the lead law reads from there.
    let first_lean = t.moves.iter().position(|m| m.axis.is_some_and(|a| a[2] < 1.0 - 1e-9)).unwrap_or(0);
    for m in t.moves[first_lean..].iter().filter(|m| m.kind != MoveKind::Rapid) {
        let Some(a) = m.axis else { continue };
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        assert!((l - 1.0).abs() < 1e-9, "axis {a:?} is not unit");
        assert!(a[2] < 1.0 - 1e-9, "the 15-degree lead posed a vertical axis");
        n += 1;
    }
    assert!(n > 20, "only {n} posed moves over a plate");
}

#[test]
fn flow5d_runs_with_the_blade_it_reads() {
    let blade = tube_about(32, 8, 12.0, 30.0);
    let t = flow5d(&blade, &finish(11.0, -11.0), &AxisControl::default(), None).expect("a tube flows");
    assert!(t.all_finite());
    // The tube's least-curving direction is x-hat everywhere it is
    // shallow: the body's chords must lie along it.
    let cs = chords(&t);
    let (mut along, mut total) = (0, 0);
    for (a, b) in &cs {
        if a.x.abs() > 26.0 || b.x.abs() > 26.0 {
            continue; // rim rows and their links
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-6 {
            continue;
        }
        total += 1;
        if dy.abs() / l < 0.1 {
            along += 1;
        }
    }
    assert!(total > 40, "only {total} body chords over a whole tube");
    assert!(along * 10 > total * 9, "{along}/{total} body chords left the blade's direction");
}

#[test]
fn two_runs_of_the_field_speak_the_same_path() {
    // HashMap iteration order must never reach the geometry: the
    // field's answers are bucket-vector walks and angle votes only.
    let blade = tube_about(16, 6, 12.0, 30.0);
    let p = finish(11.0, -11.0);
    let a = flow5d(&blade, &p, &AxisControl::default(), None).unwrap();
    let b = flow5d(&blade, &p, &AxisControl::default(), None).unwrap();
    assert_eq!(a.moves.len(), b.moves.len());
    for (x, y) in a.moves.iter().zip(&b.moves) {
        assert_eq!((x.from.x, x.from.y, x.from.z, x.to.x, x.to.y, x.to.z), (y.from.x, y.from.y, y.from.z, y.to.x, y.to.y, y.to.z));
        assert_eq!(x.axis.map(|v| v.map(lattice_bits)), y.axis.map(|v| v.map(lattice_bits)));
    }
}

fn lattice_bits(v: f64) -> i64 {
    camcraft_motion::cl::lattice(v).to_bits() as i64
}

#[test]
#[ignore = "verdict lane: envelope scan over a flow path on the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn dome_flow_is_envelope_clean() {
    use camcraft_motion::stock_sim::envelope_scan;
    let facets = sphere_cap(20.0, std::f64::consts::FRAC_PI_4, 12, 96);
    let t = flow5d(&facets, &finish(18.0, 20.0 * std::f64::consts::FRAC_1_SQRT_2 - 0.5), &AxisControl::default(), None)
        .expect("dome flow");
    let rep = envelope_scan(&facets, &t.moves, R, 0.05).expect("fixture-sized grid");
    let gate = 0.05 + rep.cell * rep.cell / (2.0 * R) + 1e-9;
    assert!(rep.gouge_mm <= gate, "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate, "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    // M13.9: the ridge flank law's spacing scallop answers to the gate
    // itself - the 0.156 it read before the march's skip-phase stride
    // ladder was runs the flow lines dropped between fold gaps.
    assert!(rep.scallop_mm <= gate, "scallop {} mm at {:?} over the gate {gate}", rep.scallop_mm, rep.scallop_at);
}

/// The spherical cap of the stage-B finishing tests, restated here
/// so this binary stands on its own.
fn sphere_cap(radius: f64, polar_max: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
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
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let zb = radius * polar_max.cos();
    let center = p3(0.0, 0.0, zb);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    facets
}

// ---------- M2.5 E: the flow pattern's 3-axis door ----------
//
// `flow3d` is `flow5d`'s field law minus the pose, and it is what
// `FinishPattern::Flow` runs. The laws below are the door's own:
// the ground is the twin's ground, the lines curve with the grain
// the field reads, and an unclaimed flat still cuts straight.

/// The catalog's own ball: R6 with the standard budget.
fn ball(p: &mut Finish3DParams, q: FinishPattern) -> Finish3DParams {
    p.pattern = q;
    *p
}

#[test]
fn the_flow_pattern_walks_the_same_ground_as_the_five_axis_kind() {
    // Two truths, one code: same field, same kernel - the stamp is
    // the only difference, so the cut BALLS are the same (the pivot
    // law: each posed tip is its upright twin's centre backed off r
    // along the pose; leaning arcs ride as chords ON the same arc)
    // and the 3-axis path states no axis anywhere.
    let blade = tube_about(16, 6, 12.0, 30.0);
    let p = ball(&mut finish(11.0, -11.0), FinishPattern::Flow);
    let r = p.tool_diameter * 0.5;
    let three = flow3d(&blade, &p).expect("a tube flows in XYZ");
    let five = flow5d(&blade, &p, &AxisControl::default(), None).expect("and posed");
    let plan: Vec<(Point3, Point3)> = three
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| m.segments3(1e-4))
        .map(|(a, b)| (Point3::new(a.x, a.y, a.z + r), Point3::new(b.x, b.y, b.z + r)))
        .collect();
    let mut held = [0.0, 0.0, 1.0];
    for m in &five.moves {
        if let Some(a) = m.axis {
            held = a;
        }
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let c = Point3::new(m.to.x + r * held[0], m.to.y + r * held[1], m.to.z + r * held[2]);
        let off = plan
            .iter()
            .map(|(a, b)| {
                let d = *b - *a;
                let l2 = d.dot(d);
                let t = if l2 > 0.0 { ((c - *a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let e = c - Point3::new(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t);
                e.dot(e).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(off < 1e-3, "the two doors walked different metal: {c:?} is {off} off");
    }
    assert!(
        three.moves.iter().all(|m| m.axis.is_none()),
        "flow3d posed an axis on a 3-axis path"
    );
}

#[test]
fn flow3d_lines_turn_with_the_grain_they_read() {
    // The tube's least-curving direction is x-hat wherever the
    // ground is shallow: the body's chords lie along it, curved
    // into the circular rows by the march's grounding, never run
    // straight ACROSS the blade as a fixed-direction raster would.
    let blade = tube_about(32, 8, 12.0, 30.0);
    let p = ball(&mut finish(11.0, -11.0), FinishPattern::Flow);
    let t = flow3d(&blade, &p).expect("a tube flows");
    assert!(t.all_finite());
    let cs = chords(&t);
    let (mut along, mut total) = (0, 0);
    for (a, b) in &cs {
        if a.x.abs() > 26.0 || b.x.abs() > 26.0 {
            continue; // rim rows and their links
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-6 {
            continue;
        }
        total += 1;
        if dy.abs() / l < 0.1 {
            along += 1;
        }
    }
    assert!(total > 40, "only {total} body chords over a whole tube");
    assert!(along * 10 > total * 9, "{along}/{total} body chords left the blade's direction");
}

#[test]
fn flow3d_over_plain_ground_cuts_and_never_stalls() {
    // Flat ground claims nothing; the fallback walks +x, so the
    // pass exists everywhere - the same degradation the 5-axis
    // kind promises, now without any pose to hide it in.
    let plate = graph_grid(12, 40.0, |_, _| 0.0);
    let p = ball(&mut finish(2.0, -0.5), FinishPattern::Flow);
    let t = flow3d(&plate, &p).expect("plain ground flows");
    assert!(chords(&t).len() > 20, "flow over plain ground stalled");
}

#[test]
fn two_runs_of_the_flow_pattern_speak_the_same_path() {
    // The field's HashMap walk must never reach the geometry.
    let blade = tube_about(16, 6, 12.0, 30.0);
    let p = ball(&mut finish(11.0, -11.0), FinishPattern::Flow);
    let a = flow3d(&blade, &p).unwrap();
    let b = flow3d(&blade, &p).unwrap();
    assert_eq!(a.moves.len(), b.moves.len());
    for (x, y) in a.moves.iter().zip(&b.moves) {
        assert_eq!(
            (x.from.x, x.from.y, x.from.z, x.to.x, x.to.y, x.to.z),
            (y.from.x, y.from.y, y.from.z, y.to.x, y.to.y, y.to.z)
        );
    }
}

#[test]
#[ignore = "verdict lane: envelope scan over the 3-axis flow path on the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn dome_flow3d_is_envelope_clean() {
    use camcraft_motion::stock_sim::envelope_scan;
    let facets = sphere_cap(20.0, std::f64::consts::FRAC_PI_4, 12, 96);
    let p = ball(&mut finish(18.0, 20.0 * std::f64::consts::FRAC_1_SQRT_2 - 0.5), FinishPattern::Flow);
    let t = flow3d(&facets, &p).expect("dome flow, upright");
    let rep = envelope_scan(&facets, &t.moves, R, 0.05).expect("fixture-sized grid");
    let gate = 0.05 + rep.cell * rep.cell / (2.0 * R) + 1e-9;
    assert!(rep.gouge_mm <= gate, "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate, "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    // M13.9: the ridge flank law's spacing scallop answers to the gate
    // itself - the 0.156 it read before the march's skip-phase stride
    // ladder was runs the flow lines dropped between fold gaps.
    assert!(rep.scallop_mm <= gate, "scallop {} mm at {:?} over the gate {gate}", rep.scallop_mm, rep.scallop_at);
}

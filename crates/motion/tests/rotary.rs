//! M5.1 stage E rotary law tests: what the axis fit says about the
//! archetypes (a standing lathe bar, the same bar laid down, a
//! steep cone that tips the eigen test, a sphere that answers every
//! axis alike, freeform and flat ground that answer none), and what
//! the two generators compose to: the contour's positions are the
//! waterline's, bit for bit, under the radial pose; the parallel's
//! passes run along the fitted axis on a laid bar.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (axis fit,
//! rotary composition) on this crate's own fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{self, Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::axis::fit;
use camcraft_motion::multiaxis::{rotary_contour, rotary_parallel, AxisControl};

const TAU: f64 = std::f64::consts::TAU;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn finish(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0,
        tolerance: 0.05,
        clearance_z: top_z + 5.0,
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

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

/// A closed lathe solid: the meridian polyline (rho, z), ascending
/// in z, swept about Z at n_circ turns - cylinders, cones, facing
/// flats and pointed ends all ride one list of stations. Every
/// triangle is wound outward, the way a turned solid's facets are.
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
                // Split along a DIAGONAL (a1-b2), not along the side
                // a2-b2: a wall patch is not planar, and joining the
                // triangles at a side edge is a bowtie - two crossed
                // diagonal slits. The fit never notices (normals are
                // per-truth), but any silhouette law slicing this
                // mesh reads it as open.
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The stepped shaft: a solid rod, flat foot, shoulder, cone,
/// barrel, chamfer, flat crown - the shapes a lathe actually leaves.
fn lathe_bar(n_circ: usize) -> Vec<Triangle> {
    revolve(
        n_circ,
        &[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 6.0),
            (1.2, 8.5),
            (1.2, 18.0),
            (2.5, 21.0),
            (2.5, 28.0),
            (0.0, 28.0),
        ],
    )
}

/// The same bar laid on its side: the axis becomes +x, the meridian
/// stations run along x, the radius lives in y-z.
fn laid_bar(n_circ: usize) -> Vec<Triangle> {
    lathe_bar(n_circ)
        .into_iter()
        .map(|f| Triangle::new(laid(f.a), laid(f.b), laid(f.c)))
        .collect()
}

fn laid(q: Point3) -> Point3 {
    p3(q.z, q.x, q.y)
}

fn unit_angle(a: Point3, b: Point3) -> f64 {
    let d = (a.x * b.x + a.y * b.y + a.z * b.z).clamp(-1.0, 1.0);
    d.acos()
}

// ---- the fit law -------------------------------------------------

#[test]
fn a_standing_lathe_bar_yields_its_axis() {
    let ax = fit(&lathe_bar(16)).expect("a turned bar is turned");
    assert!((ax.dir.x.abs() < 1e-9 && ax.dir.y.abs() < 1e-9 && (ax.dir.z - 1.0).abs() < 1e-9), "{:?}", ax.dir);
    assert!(ax.origin.x.abs() < 1e-6 && ax.origin.y.abs() < 1e-6, "{:?}", ax.origin);
}

#[test]
fn a_laid_bar_yields_the_same_axis_laid() {
    let ax = fit(&laid_bar(16)).expect("the same bar, on its side");
    assert!((ax.dir.y.abs() < 1e-9 && ax.dir.z.abs() < 1e-9 && (ax.dir.x - 1.0).abs() < 1e-9), "{:?}", ax.dir);
    assert!(ax.origin.x.abs() < 1e-6 && ax.origin.z.abs() < 1e-6, "{:?}", ax.origin);
}

#[test]
fn a_steep_cone_still_yields_its_axis() {
    // Past 45 degrees the normals' second moment tips to the OTHER
    // eigendirection - the two-candidate law exists for this cone.
    let cone = revolve(24, &[(0.0, 0.0), (3.0, 0.0), (0.3, 3.0), (0.0, 3.0)]);
    let ax = fit(&cone).expect("a steep revolve is a revolve");
    assert!((ax.dir.x * ax.dir.x + ax.dir.y * ax.dir.y).sqrt() < 1e-6, "{:?}", ax.dir);
}

#[test]
fn a_sphere_refuses_as_a_dome_by_name() {
    // Every latitude and longitude: normals concurrent at the
    // center, so every axis fits equally.
    let (r, nlat, nlon) = (5.0, 12, 24);
    let pt = |i: usize, j: usize| {
        let (ph, th) = (std::f64::consts::PI * (i as f64 / nlat as f64 - 0.5), TAU * j as f64 / nlon as f64);
        p3(r * ph.cos() * th.cos(), r * ph.cos() * th.sin(), r * ph.sin())
    };
    let mut s = Vec::new();
    for i in 0..nlat {
        for j in 0..nlon {
            s.push(Triangle::new(pt(i, j), pt(i + 1, j), pt(i + 1, j + 1)));
            s.push(Triangle::new(pt(i, j), pt(i + 1, j + 1), pt(i, j + 1)));
        }
    }
    let e = fit(&s).expect_err("a dome has no ONE axis");
    assert!(e.to_string().contains("equally"), "{e}");
}

#[test]
fn freeform_and_flat_ground_share_no_axis() {
    // A wadding of two waves: no meridian anywhere.
    let (n, span) = (9usize, 12.0f64);
    let g = |i: usize, j: usize| {
        let (x, y) = (-span + 2.0 * span * i as f64 / n as f64, -span + 2.0 * span * j as f64 / n as f64);
        p3(x, y, 2.0 * (x * 0.4).sin() + 0.1 * y * y)
    };
    let mut blob = Vec::new();
    for i in 0..n {
        for j in 0..n {
            blob.push(Triangle::new(g(i, j), g(i + 1, j), g(i + 1, j + 1)));
            blob.push(Triangle::new(g(i, j), g(i + 1, j + 1), g(i, j + 1)));
        }
    }
    let e = fit(&blob).expect_err("a freeform is not turned");
    assert!(e.to_string().contains("axis"), "{e}");
    // Plain ground: normals parallel, none answering the fit.
    let plate = revolve(16, &[(0.0, 5.0), (4.0, 5.0)]);
    assert!(fit(&plate).is_err(), "a flat facing alone names no axis");
}

#[test]
fn a_mesh_too_coarse_to_read_is_refused() {
    // 8 turns: the faceted zone's normals miss the axis by more
    // than the sagitta budget - refuse as unreadable, don't lie.
    let coarse = revolve(8, &[(0.0, 0.0), (2.0, 0.0), (2.0, 20.0), (0.0, 20.0)]);
    assert!(fit(&coarse).is_err());
}

// ---- the contour law ---------------------------------------------

/// The ball center a posed tip stands under: `r` up the pose (the
/// stream's tip contract), upright where the move states none.
fn center(m: &camcraft_motion::cl::Move, r: f64) -> Point3 {
    let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
    p3(m.to.x + r * a[0], m.to.y + r * a[1], m.to.z + r * a[2])
}

/// Distance from `c` to the nearest ball-center chord of a vertical
/// ball's ladder (its tips lifted by `r`).
fn off_ladder(want: &camcraft_motion::cl::Toolpath, c: Point3, r: f64) -> f64 {
    let mut best = f64::INFINITY;
    for m in &want.moves {
        for (a, b) in m.segments3(1e-4) {
            let (a, b) = (p3(a.x, a.y, a.z + r), p3(b.x, b.y, b.z + r));
            let (ex, ey, ez) = (b.x - a.x, b.y - a.y, b.z - a.z);
            let l2 = ex * ex + ey * ey + ez * ez;
            let t = if l2 > 0.0 {
                (((c.x - a.x) * ex + (c.y - a.y) * ey + (c.z - a.z) * ez) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = ((c.x - a.x - t * ex).powi(2) + (c.y - a.y - t * ey).powi(2) + (c.z - a.z - t * ez).powi(2)).sqrt();
            best = best.min(d);
        }
    }
    best
}

#[test]
fn the_rotary_contour_is_the_waterline_posed_radially() {
    // M13 pivot law: the rings' BALLS are the waterline's balls - the
    // pose turns about the ball center, never about the tip (which
    // would carry the ball one radius off every wall) - so every
    // emitted tip, lifted r along its own pose, lands on the plain
    // ladder's center path, and the pose is the radial OF THAT CENTER.
    let facets = lathe_bar(16);
    let p = finish(28.0, 0.0);
    let r = p.tool_diameter * 0.5;
    let want = finishing3d::waterline(&facets, &p).expect("the plain rings run");
    let got = rotary_contour(&facets, &p, &control(0.0, 0.0, None)).expect("the same rings, turned");
    assert!(got.moves.len() >= want.moves.len(), "the exit dropped moves");
    let mut cuts = 0usize;
    for a in &got.moves {
        let c = center(a, r);
        // Chord ends sit ON the plain ladder's arcs; the yardstick chords
        // them at 1e-4 of sagitta.
        assert!(off_ladder(&want, c, r) < 2e-4, "the ball left the planned ladder at {:?}", a.to);
        let rho = (c.x * c.x + c.y * c.y).sqrt();
        match a.axis {
            Some(ax) if rho > 1e-6 => {
                let av = p3(ax[0], ax[1], ax[2]);
                assert!((av.z).abs() < 1e-9, "the pose left the radial plane: {av:?}");
                assert!(unit_angle(av, p3(c.x, c.y, 0.0)) < 1e-9, "the pose is not the center's radial");
                if a.kind != MoveKind::Rapid {
                    cuts += 1;
                }
            }
            Some(_) => {}
            None => assert_eq!(a.kind, MoveKind::Rapid, "a cut move about the axis poses"),
        }
    }
    assert!(cuts > 20, "rings of posed cuts, got {cuts}");
}

#[test]
fn the_contour_lean_tilts_off_the_radial_by_the_stated_angle() {
    let facets = lathe_bar(16);
    let p = finish(28.0, 0.0);
    let got = rotary_contour(&facets, &p, &control(15.0, 0.0, None)).expect("a leaning rotary cut");
    let mut body = 0usize;
    for m in &got.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        // The lean is measured off the radial of the BALL CENTER the
        // tip stands under (the M13 pivot law).
        let c = center(m, p.tool_diameter * 0.5);
        let l = (c.x * c.x + c.y * c.y).sqrt();
        if l < 0.5 {
            continue; // near the axis the ring's radial is the travel itself
        }
        let a = m.axis.expect("a cut move poses");
        let av = p3(a[0], a[1], a[2]);
        let n = p3(c.x / l, c.y / l, 0.0);
        assert!((unit_angle(av, n) - 15f64.to_radians()).abs() < 1e-9, "the lead law broke at {:?}", m.to);
        body += 1;
    }
    assert!(body > 20, "a whole shaft's rings carry plenty of posed moves, got {body}");
}

// ---- the parallel law ---------------------------------------------

#[test]
fn the_rotary_parallel_runs_the_laid_bar_axially() {
    let facets = laid_bar(16);
    let p = Finish3DParams {
        clearance_z: 10.0,
        ..finish(3.0, -3.0)
    };
    let got = rotary_parallel(&facets, &p, &control(0.0, 0.0, None)).expect("a laid bar takes passes");
    let mut body = 0usize;
    for m in &got.moves {
        if m.kind == MoveKind::Rapid {
            // Rapids state the radial the head holds (the post's tilt
            // is modal - M13): never a lean along the bar.
            if let Some(a) = m.axis {
                assert!(a[0].abs() < 1e-9, "a rapid leaned along the axis: {a:?}");
            }
            continue;
        }
        let a = m.axis.expect("every pass move stands radial to the bar");
        assert!(a[0].abs() < 1e-9, "the pose leaned along the axis: {a:?}");
        body += 1;
    }
    assert!(body > 20, "passes over a whole bar, got {body}");
}

#[test]
fn a_standing_bar_refuses_parallel_by_name() {
    let p = finish(28.0, 0.0);
    let c = control(0.0, 0.0, None);
    let e = rotary_parallel(&lathe_bar(16), &p, &c).expect_err("a standing bar has no axial passes here");
    assert!(e.to_string().contains("Rotary Contour"), "{e}");
}

// ---- the held-end law (#2343) -------------------------------------

/// A plain round bar of radius 3 from station `x0` to `x1`, laid along x.
fn laid_stock(x0: f64, x1: f64) -> Vec<Triangle> {
    revolve(16, &[(0.0, x0), (3.0, x0), (3.0, x1), (0.0, x1)])
        .into_iter()
        .map(|f| Triangle::new(laid(f.a), laid(f.b), laid(f.c)))
        .collect()
}

/// The lowest and highest station a cutting ball center stands at.
fn cut_span(path: &camcraft_motion::cl::Toolpath) -> (f64, f64) {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .flat_map(|m| [m.from.x, center(m, 1.0).x])
        .fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(x), h.max(x)))
}

#[test]
fn the_parallel_leaves_the_end_face_where_the_stock_goes_on() {
    use camcraft_motion::multiaxis::rotary_parallel_in;
    let (facets, c) = (laid_bar(16), control(0.0, 0.0, None));
    let p = Finish3DParams {
        clearance_z: 10.0,
        ..finish(3.0, -3.0)
    };
    // The plain raster goes a ball radius past each end (stations 0 and
    // 28) and down the end faces.
    let plain = rotary_parallel(&facets, &p, &c).expect("the plain raster");
    let (lo, hi) = cut_span(&plain);
    assert!(lo < -0.9 && hi > 28.9, "the plain raster passes each end: {lo}..{hi}");
    assert!(plain.notes.iter().all(|n| !n.contains("end face")), "{:?}", plain.notes);
    // The bar goes on 20 mm past station 0, into the chuck: the ball
    // center stays on the model's side of that plane; the free end is as before.
    let held = rotary_parallel_in(&facets, Some(&laid_stock(-20.0, 28.0)), &p, &c).expect("a held bar");
    let (lo, hi) = cut_span(&held);
    assert!(lo >= -1e-6, "the ball center passed the held end's plane: station {lo}");
    assert!(lo < 1e-6, "the turned surface is finished to its end: {lo}");
    assert!(hi > 28.9, "the free end face is still cut: {hi}");
    let note = "Rotary Parallel: the stock goes on 20.000 mm past the end of the model at (0.000, 0.000, 0.000) - that end face is left";
    assert!(held.notes.iter().any(|n| n.starts_with(note)), "{:?}", held.notes);
    // No link feeds across the stretch that was left: each feed that
    // ends below the retract radius stays in the window too.
    for m in held.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(center(m, 1.0).x >= -1e-6, "a feed or plunge stands past the plane: {m:?}");
    }
    // Under the sloped-entry law no ramp starts past the held end's
    // plane: an infeed there stays a radial plunge.
    let sloped = Finish3DParams { entry_ramp: true, ..p };
    let ramped = rotary_parallel_in(&facets, Some(&laid_stock(-20.0, 28.0)), &sloped, &c).expect("sloped entries");
    assert!(ramped.notes.iter().any(|n| n.contains("ramp in along the pass")), "{:?}", ramped.notes);
    for m in ramped.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(m.from.x >= -1e-6 && m.to.x >= -1e-6, "a ramp runs in from past the plane: {m:?}");
    }
    // Stock that ends with the model, or goes on by no more than the
    // tool's diameter (a facing allowance), changes nothing.
    for stock in [laid_stock(0.0, 28.0), laid_stock(-2.0, 30.0)] {
        let same = rotary_parallel_in(&facets, Some(&stock), &p, &c).expect("free ends");
        assert_eq!(same.moves, plain.moves);
        assert_eq!(same.notes, plain.notes);
    }
}

//! #904: the turning band kinds' meridian verdicts on a model imported
//! coarser than the importer promises. The scan reads the silhouette off
//! the facets, and a facet of a convex meridian stands inside the true
//! surface by its chord error - so the gouge clause gives the model's
//! excess back out of its tolerance while a positive tolerance is left
//! (`turning::gouge_tol_on`, the `_excess` twins of the blade, thread and
//! form verdicts, and the composed program scan). Past that it reads
//! untightened, and the program gate refuses the operation instead. The
//! rest clause keeps the operation's tolerance.
//!
//! What "right" means here: a floor cut deeper than the tightened
//! tolerance but inside the plain one passes at full fidelity and is
//! convicted on the coarse model; a flush cut is never convicted by the
//! tightening; a real gouge is convicted at every excess; an excess of 0
//! is the plain verdict exactly.
//!
//! Provenance: our own fixtures (the blade, thread and form suites'
//! revolved bars) over our own laws; no vendor material.
//!
//! Gate map (#904): covers camcraft_motion::turning (revolve, blade_law,
//! thread_law, form).
//! Lane shape (M4.4): all tests here run in well under a second.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::{
    blade_program_warnings, blade_program_warnings_excess, blade_warnings, blade_warnings_excess, form,
    form_warnings, form_warnings_excess, gouge_tol_on, groove_finish, thread, thread_warnings,
    thread_warnings_excess, BladeLeg, FormParams, GrooveFinishParams, ThreadParams,
};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
/// The operations' tolerance, and a coarse model's excess that leaves
/// 0.043 of it: a 0.012 mm mesh against the importer's 0.005 mm promise.
const TOL: f64 = 0.05;
const EXCESS: f64 = 0.007;
/// A cut this far below the model: inside `TOL`, past `TOL - EXCESS`.
const BETWEEN: f64 = 0.046;
const WIDTH: f64 = 2.0;

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// The blade suite's grooved wall: r 6, a groove 2.4 wide with its
/// floor at r 4 over stations -6.0 .. -3.6.
fn grooved() -> Vec<Triangle> {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.0), (4.0, -6.0), (4.0, -3.6), (6.0, -3.6), (6.0, 40.0), (0.0, 40.0)])
}

/// The thread suite's bar: a 60-degree V `depth` deep every 2.0 from
/// z 8 to z 14 on an r 6 wall.
fn threaded(depth: f64) -> Vec<Triangle> {
    let half = depth * 30.0_f64.to_radians().tan();
    let mut m = vec![(0.0, -16.0), (6.0, -16.0), (6.0, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((6.0 - depth, c));
        m.push((6.0, c + half));
        if c + 2.0 <= 14.0 + 1e-9 {
            m.push((6.0, c + 2.0 - half));
        }
        c += 2.0;
    }
    m.extend([(6.0, 14.0 + half), (6.0, 40.0), (0.0, 40.0)]);
    revolve(&m)
}

/// The form suite's bar: r 10 with one crown-6 valley at station 10.
fn valley() -> Vec<Triangle> {
    let s6 = |d: f64| 6.0 - (36.0 - d * d).sqrt();
    let c = 10.0;
    revolve(&[
        (0.0, -50.0),
        (10.0, -50.0),
        (10.0, c - 2.0),
        (8.0 + s6(2.0), c - 2.0),
        (8.0 + s6(1.0), c - 1.0),
        (8.0, c),
        (8.0 + s6(1.0), c + 1.0),
        (8.0 + s6(2.0), c + 2.0),
        (10.0, c + 2.0),
        (10.0, 50.0),
        (0.0, 50.0),
    ])
}

/// The path with every feed's end `d` nearer the axis (the bars stand
/// on world Z): what a generator fault would post.
fn sunk(path: &Toolpath, d: f64) -> Toolpath {
    let mut t = path.clone();
    for m in t.moves.iter_mut().filter(|m| m.kind == MoveKind::Feed) {
        let rho = (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
        let k = (rho - d) / rho;
        m.to = Point3::new(m.to.x * k, m.to.y * k, m.to.z);
    }
    t
}

fn gouge_of(warns: &[Warning]) -> f64 {
    warns
        .iter()
        .map(|w| match w {
            Warning::Envelope { gouge_mm, .. } => *gouge_mm,
            other => panic!("the meridian verdict spoke {other:?}, not the envelope"),
        })
        .fold(0.0, f64::max)
}

fn groove_path() -> Toolpath {
    let p = GrooveFinishParams {
        tolerance: TOL,
        clearance_z: 44.0,
        top_z: -3.6,
        bottom_z: -6.0,
        feed: 0.0,
        plunge_feed: 60.0,
        blade_width: WIDTH,
    };
    groove_finish(&grooved(), &p).expect("the groove finishes")
}

#[test]
fn the_tightened_tolerance_is_the_excess_floor() {
    assert_eq!(gouge_tol_on(TOL, 0.0), TOL, "full fidelity: the tolerance itself");
    assert!((gouge_tol_on(TOL, EXCESS) - (TOL - EXCESS)).abs() < 1e-12);
    // Nothing positive left: the clause reads untightened (the gate refuses).
    for past in [TOL, 0.2, f64::INFINITY] {
        assert_eq!(gouge_tol_on(TOL, past), TOL, "excess {past}");
    }
    assert_eq!(gouge_tol_on(TOL, f64::NAN), TOL);
    assert_eq!(gouge_tol_on(TOL, -1.0), TOL);
}

#[test]
fn a_blade_floor_between_the_two_tolerances_is_convicted_only_on_the_coarse_model() {
    let (bar, flush) = (grooved(), groove_path());
    let deep = sunk(&flush, BETWEEN);
    // Full fidelity: 0.046 mm under a 0.05 mm tolerance is silent, and
    // an excess of 0 is that verdict exactly.
    assert!(blade_warnings(&bar, &deep, WIDTH, TOL, 0.0).is_empty());
    assert_eq!(blade_warnings_excess(&bar, &deep, WIDTH, TOL, 0.0, 0.0), blade_warnings(&bar, &deep, WIDTH, TOL, 0.0));
    // The coarse model: the same floor is past the 0.043 mm left.
    let g = gouge_of(&blade_warnings_excess(&bar, &deep, WIDTH, TOL, 0.0, EXCESS));
    assert!((g - BETWEEN).abs() < 1e-6, "the gouge is measured, not invented: {g}");
    // A flush groove is never convicted by the tightening - at any
    // excess, including one that uses the whole tolerance.
    for excess in [0.0, EXCESS, 0.049, TOL, 0.5] {
        assert!(blade_warnings_excess(&bar, &flush, WIDTH, TOL, 0.0, excess).is_empty(), "excess {excess}");
    }
    // Past the bound the clause reads untightened; a real gouge is
    // convicted at every excess.
    for excess in [TOL, 0.5] {
        assert!(blade_warnings_excess(&bar, &deep, WIDTH, TOL, 0.0, excess).is_empty(), "excess {excess}");
    }
    let real = sunk(&flush, 0.3);
    for excess in [0.0, EXCESS, TOL, 0.5] {
        assert!(gouge_of(&blade_warnings_excess(&bar, &real, WIDTH, TOL, 0.0, excess)) > 0.29, "excess {excess}");
    }
}

#[test]
fn the_rest_clause_keeps_the_operations_tolerance() {
    // A floor left 0.046 mm standing: inside the tolerance, and the
    // model's chord error cannot make standing steel a gouge.
    let (bar, flush) = (grooved(), groove_path());
    let shy = sunk(&flush, -BETWEEN);
    assert!(blade_warnings_excess(&bar, &shy, WIDTH, TOL, 0.0, EXCESS).is_empty());
    // ...while 0.06 mm standing rests on either model, by the same number.
    let left = sunk(&flush, -0.06);
    assert_eq!(blade_warnings_excess(&bar, &left, WIDTH, TOL, 0.0, EXCESS), blade_warnings(&bar, &left, WIDTH, TOL, 0.0));
    assert!(!blade_warnings(&bar, &left, WIDTH, TOL, 0.0).is_empty());
}

#[test]
fn the_composed_program_scan_tightens_the_same_way() {
    let (bar, flush) = (grooved(), groove_path());
    let deep = sunk(&flush, BETWEEN);
    let legs = |p| [BladeLeg { path: p, width: WIDTH }];
    assert!(blade_program_warnings(&bar, &legs(&deep), TOL).is_empty());
    assert_eq!(blade_program_warnings_excess(&bar, &legs(&deep), TOL, 0.0), blade_program_warnings(&bar, &legs(&deep), TOL));
    assert!(gouge_of(&blade_program_warnings_excess(&bar, &legs(&deep), TOL, EXCESS)) > 0.045);
    assert!(blade_program_warnings_excess(&bar, &legs(&flush), TOL, EXCESS).is_empty());
    assert!(blade_program_warnings_excess(&bar, &legs(&deep), TOL, TOL).is_empty(), "past the bound: untightened");
}

#[test]
fn a_thread_cut_between_the_two_tolerances_is_convicted_only_on_the_coarse_model() {
    // The model's V is 1.2 deep; the numbers ask for 0.046 more, on
    // steel ground for that depth (a parallel-flank overcut).
    let model = threaded(1.2);
    let params = |depth: f64| ThreadParams {
        tolerance: TOL,
        clearance_z: 24.0,
        top_z: 14.3,
        bottom_z: 7.7,
        pitch: 2.0,
        depth,
        infeed: 0.4,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: 2.0 * depth * 30.0_f64.to_radians().tan(),
        form_angle: 60.0,
        hand: camcraft_motion::cl::ThreadHand::Right,
    };
    let run = |depth: f64| {
        let p = params(depth);
        (thread(&model, &p).expect("the thread cuts"), p.insert_width)
    };
    let (deep, w) = run(1.2 + BETWEEN);
    assert!(thread_warnings(&model, &deep, w, 60.0, TOL).is_empty(), "inside the plain tolerance");
    assert_eq!(thread_warnings_excess(&model, &deep, w, 60.0, TOL, 0.0), thread_warnings(&model, &deep, w, 60.0, TOL));
    let g = gouge_of(&thread_warnings_excess(&model, &deep, w, 60.0, TOL, EXCESS));
    assert!(g > TOL - EXCESS && g <= TOL, "convicted by the tightened clause: {g}");
    assert!(thread_warnings_excess(&model, &deep, w, 60.0, TOL, TOL).is_empty(), "past the bound: untightened");
    // The matched thread is silent on the coarse model too.
    let (flush, w) = run(1.2);
    for excess in [0.0, EXCESS, 0.049, TOL] {
        assert!(thread_warnings_excess(&model, &flush, w, 60.0, TOL, excess).is_empty(), "excess {excess}");
    }
    // A 0.3 overcut gouges at every excess.
    let (real, w) = run(1.5);
    for excess in [0.0, EXCESS, TOL] {
        assert!(gouge_of(&thread_warnings_excess(&model, &real, w, 60.0, TOL, excess)) >= 0.25, "excess {excess}");
    }
}

#[test]
fn a_crown_sunk_between_the_two_tolerances_is_convicted_only_on_the_coarse_model() {
    let model = valley();
    let p = FormParams {
        tolerance: TOL,
        clearance_z: 60.0,
        top_z: 50.0,
        bottom_z: -50.0,
        feed: 40.0,
        plunge_feed: 20.0,
        band_width: 4.0,
        crown_radius: 6.0,
    };
    let flush = form(&model, &p).expect("a modeled valley is form work");
    let deep = sunk(&flush, BETWEEN);
    assert!(form_warnings(&model, &deep, 4.0, 6.0, TOL).is_empty(), "inside the plain tolerance");
    assert_eq!(form_warnings_excess(&model, &deep, 4.0, 6.0, TOL, 0.0), form_warnings(&model, &deep, 4.0, 6.0, TOL));
    let g = gouge_of(&form_warnings_excess(&model, &deep, 4.0, 6.0, TOL, EXCESS));
    assert!(g > TOL - EXCESS && g <= TOL, "convicted by the tightened clause: {g}");
    for excess in [0.0, EXCESS, 0.049, TOL] {
        assert!(form_warnings_excess(&model, &flush, 4.0, 6.0, TOL, excess).is_empty(), "excess {excess}");
    }
    assert!(form_warnings_excess(&model, &deep, 4.0, 6.0, TOL, TOL).is_empty(), "past the bound: untightened");
    let real = sunk(&flush, 0.3);
    for excess in [0.0, EXCESS, TOL] {
        assert!(gouge_of(&form_warnings_excess(&model, &real, 4.0, 6.0, TOL, excess)) > 0.29, "excess {excess}");
    }
}

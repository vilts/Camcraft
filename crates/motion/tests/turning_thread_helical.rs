//! Issue #3023: a turned shaft with a HELICAL modelled thread (the
//! issue's part: M16x1.5 x 16.5 thread, Ø24 body, Ø30 head, 70 long)
//! is refused by Threading with a message that names the helical
//! thread - not "every axis fits this model equally", not "the mesh is
//! open". A revolved V thread and a smooth bar are not helical.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread kernel,
//! helical_refusal) and camcraft_motion::multiaxis::axis_helix.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::ThreadHand;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::axis_helix::helix;
use camcraft_motion::turning::{thread, ThreadParams};

const TAU: f64 = std::f64::consts::TAU;
const MAJOR: f64 = 8.0;
const DEPTH: f64 = 0.92;
const PITCH: f64 = 1.5;
const TURNS: usize = 11;
const N_CIRC: usize = 96;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed revolve of the meridian `(r, z)` points.
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
            if ra > 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            }
            if rb > 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The shaft above the thread: relief, Ø24 body, Ø30 head, capped.
fn shaft_body(n_circ: usize) -> Vec<Triangle> {
    let top = PITCH * TURNS as f64 + PITCH;
    revolve(
        n_circ,
        &[(MAJOR, top), (MAJOR, 17.0), (12.0, 17.0), (12.0, 53.0), (15.0, 53.0), (15.0, 70.0), (0.0, 70.0)],
    )
}

/// The helical V thread: a sharp 60-degree-ish V, crest r `MAJOR`,
/// root `MAJOR - DEPTH`, swept as a helix of `pitch` (negative: left
/// hand) from z 0, with a cap at the free end.
fn helical(pitch: f64, n_circ: usize) -> Vec<Triangle> {
    let rows = 2 * TURNS;
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        let r = if i.is_multiple_of(2) { MAJOR } else { MAJOR - DEPTH };
        let z = i as f64 * PITCH * 0.5 + pitch * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for i in 0..rows {
        for j in 0..n_circ {
            t.push(Triangle::new(v(j, i), v(j + 1, i), v(j + 1, i + 1)));
            t.push(Triangle::new(v(j, i), v(j + 1, i + 1), v(j, i + 1)));
        }
    }
    t.extend(revolve(n_circ, &[(0.0, 0.0), (MAJOR, 0.0)]));
    t.extend(shaft_body(n_circ));
    t
}

/// The same shaft with the thread modelled as revolved V rings.
fn revolved_v() -> Vec<Triangle> {
    let mut m = vec![(0.0, 0.0)];
    for i in 0..=2 * TURNS {
        let r = if i.is_multiple_of(2) { MAJOR } else { MAJOR - DEPTH };
        m.push((r, i as f64 * PITCH * 0.5));
    }
    let mut t = revolve(N_CIRC, &m);
    t.extend(shaft_body(N_CIRC));
    t
}

/// The same shaft with no thread: a smooth major diameter.
fn smooth() -> Vec<Triangle> {
    let mut t = revolve(N_CIRC, &[(0.0, 0.0), (MAJOR, 0.0)]);
    t.extend(shaft_body(N_CIRC));
    t
}

fn params() -> ThreadParams {
    ThreadParams {
        tolerance: 0.01,
        clearance_z: 80.0,
        top_z: 16.0,
        bottom_z: 1.0,
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.2,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: 1.2,
        form_angle: 60.0,
        hand: ThreadHand::Right,
    }
}

fn refusal(facets: &[Triangle]) -> String {
    match thread(facets, &params()) {
        Ok(_) => panic!("a helical thread must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_helical_thread_is_refused_by_name() {
    let msg = refusal(&helical(PITCH, N_CIRC));
    assert!(msg.contains("HELICAL"), "{msg}");
    assert!(msg.contains("about a 1.5") && msg.contains("right-hand"), "{msg}");
    assert!(msg.contains("revolved V"), "{msg}");
}

#[test]
fn a_left_hand_helical_thread_says_left_hand() {
    let msg = refusal(&helical(-PITCH, N_CIRC));
    assert!(msg.contains("HELICAL") && msg.contains("about a 1.5") && msg.contains("left-hand"), "{msg}");
}

#[test]
fn a_coarse_helical_thread_is_refused_by_name() {
    let msg = refusal(&helical(PITCH, 48));
    assert!(msg.contains("HELICAL") && msg.contains("about a 1.5"), "{msg}");
}

#[test]
fn an_open_helical_mesh_is_refused_by_name() {
    // The issue's STEP route: the helical faces tessellate with gaps.
    // The refusal reads the facets and does not need a closed shell.
    let mut t = helical(PITCH, N_CIRC);
    let n = t.len();
    t.retain({
        let mut i = 0usize;
        move |_| {
            i += 1;
            !i.is_multiple_of(37) || i > n / 3
        }
    });
    let msg = refusal(&t);
    assert!(msg.contains("HELICAL"), "{msg}");
}

#[test]
fn a_revolved_v_thread_and_a_smooth_bar_are_not_helical() {
    assert_eq!(helix(&revolved_v()), None);
    assert_eq!(helix(&smooth()), None);
    let hx = helix(&helical(PITCH, N_CIRC)).expect("the helix is found");
    assert!((hx.pitch - PITCH).abs() < 0.02 * PITCH, "{hx:?}");
    assert!(hx.axis.dir.z > 0.999, "{hx:?}");
}

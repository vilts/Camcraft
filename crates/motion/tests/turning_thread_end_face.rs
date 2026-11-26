//! Free-end threading law tests (issue #2879): a thread at the free
//! end of a bar may overhang the end face with its last V band and may
//! start in free air some pitches in front of the end face so spindle
//! and feed synchronise. Free air past the bar's end face is never
//! rest; real gouges, shallow cuts, missed turns, and a free-air pass
//! whose flank clips a solid end face still convict.
//!
//! Provenance: single-point threading lead-in in free air in front of
//! the workpiece end face is standard lathe handbook practice; our own
//! meridian domain law in `revolve::lattice` and lattice extent in
//! `thread`. No vendor material consulted.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread kernel
//! `k_hi`, thread_warnings, blade_warnings, revolve::lattice and
//! revolve::scan domain law).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, ThreadHand};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    blade_warnings, groove_finish, thread, thread_warnings, GrooveFinishParams, ThreadParams,
};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
const MAJOR: f64 = 5.0;
const PITCH: f64 = 1.5;
const DEPTH: f64 = 0.92;
const ROOT: f64 = MAJOR - DEPTH;
const INSERT_W: f64 = 1.2;
const END_FACE: f64 = 78.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Closed-solid revolve shared with the turning suites.
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

/// Bar with a 10 mm thread diameter, 0.92 mm deep 60-degree V grooves
/// every 1.5 mm ending at `last_groove`, and the free end face at Z78.
/// If the last V's right flank crosses Z78, it is clipped cleanly at
/// the end face; if it finishes before Z78, the crest continues to Z78.
fn free_end_bar(last_groove: f64) -> Vec<Triangle> {
    let half_v = DEPTH * (30.0_f64).to_radians().tan();
    let cot30 = 1.0 / (30.0_f64).to_radians().tan();
    let first_groove = last_groove - 8.0 * PITCH;
    let mut m = vec![(0.0, 0.0), (MAJOR, 0.0), (MAJOR, first_groove - half_v)];
    for k in 0..=8 {
        let c = first_groove + (k as f64) * PITCH;
        m.push((ROOT, c));
        let right = c + half_v;
        if right <= END_FACE {
            m.push((MAJOR, right));
            if k < 8 {
                m.push((MAJOR, c + PITCH - half_v));
            } else if right < END_FACE - 1e-9 {
                m.push((MAJOR, END_FACE));
            }
        } else {
            let r_end = ROOT + (END_FACE - c) * cot30;
            m.push((r_end, END_FACE));
        }
    }
    m.push((0.0, END_FACE));
    revolve(16, &m)
}

fn params(top_z: f64, bottom_z: f64) -> ThreadParams {
    ThreadParams {
        tolerance: TOL,
        clearance_z: 85.0,
        top_z,
        bottom_z,
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.25,
        feed: 120.0,
        plunge_feed: 40.0,
        insert_width: INSERT_W,
        form_angle: 60.0,
        hand: ThreadHand::Right,
    }
}

#[test]
fn free_end_groove_band_past_end_face_is_not_rest() {
    // Issue #2879 exact repro: last groove at Z77.5, end face at Z78,
    // 1.2 mm wide 60-degree insert (band Z76.9..Z78.1 overhangs by 0.1 mm).
    let bar = free_end_bar(77.5);
    let path = thread(&bar, &params(78.0, 65.0)).expect("the thread cuts");
    let warns = thread_warnings(&bar, &path, INSERT_W, 60.0, TOL);
    assert!(warns.is_empty(), "end air past Z78 reported as warning: {warns:?}");
}

#[test]
fn free_air_lead_in_starts_in_front_of_end_face_and_judges_clean() {
    // Issue #2879: setting window top to Z81 places lead-in lattice points
    // at Z79 and Z80.5 in free air in front of the Z78 end face.
    let bar = free_end_bar(77.5);
    let path = thread(&bar, &params(81.0, 65.0)).expect("the thread cuts");
    let f = Frame::about(&bar).expect("plumb");
    let first_feed = path
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Feed && m.arc.is_none())
        .expect("a pass opens with a radial plunge");
    let (s_start, _) = f.sr(first_feed.to);
    assert!(
        (s_start - 80.5).abs() < 1e-6,
        "pass should start at Z80.5 in free air, got {s_start}"
    );
    let cycle = path.threads.first().expect("thread cycle record");
    assert!(
        (cycle.start.z - 80.5).abs() < 1e-6,
        "cycle record should start at Z80.5, got {}",
        cycle.start.z
    );
    let warns = thread_warnings(&bar, &path, INSERT_W, 60.0, TOL);
    assert!(warns.is_empty(), "free-air lead-in reported as warning: {warns:?}");

    // Same for the #2394 geometry (last groove Z77.4, end face Z78, top Z81):
    // lead-in lattice points stand at Z78.9 and Z80.4.
    let bar2 = free_end_bar(77.4);
    let path2 = thread(&bar2, &params(81.0, 65.0)).expect("the thread cuts");
    let f2 = Frame::about(&bar2).expect("plumb");
    let first_feed2 = path2
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Feed && m.arc.is_none())
        .expect("plunge");
    let (s_start2, _) = f2.sr(first_feed2.to);
    assert!(
        (s_start2 - 80.4).abs() < 1e-6,
        "pass should start at Z80.4 in free air, got {s_start2}"
    );
    let warns2 = thread_warnings(&bar2, &path2, INSERT_W, 60.0, TOL);
    assert!(warns2.is_empty(), "free-air lead-in on Z77.4 bar reported: {warns2:?}");
}

#[test]
fn free_air_start_still_convicts_real_gouge_and_real_rest_on_part() {
    let bar = free_end_bar(77.5);

    // Real gouge on the part (0.15 mm too deep, insert resized to fit the 1.5 pitch).
    let mut deep = params(81.0, 65.0);
    deep.depth = DEPTH + 0.15;
    deep.insert_width = 2.0 * deep.depth * (30.0_f64).to_radians().tan();
    let t_deep = thread(&bar, &deep).expect("overcut threads");
    match thread_warnings(&bar, &t_deep, deep.insert_width, 60.0, TOL).as_slice() {
        [Warning::Envelope { gouge_mm, .. }] => {
            assert!(*gouge_mm >= 0.12, "overcut understated: {gouge_mm}");
        }
        other => panic!("deep thread with free-air start must convict as gouge, got {other:?}"),
    }

    // Real rest on the part (0.4 mm too shallow).
    let mut shallow = params(81.0, 65.0);
    shallow.depth = DEPTH - 0.4;
    let t_shallow = thread(&bar, &shallow).expect("shallow threads");
    match thread_warnings(&bar, &t_shallow, INSERT_W, 60.0, TOL).as_slice() {
        [Warning::Envelope { gouge_mm, rest_mm, .. }] => {
            assert!(*rest_mm >= 0.35 && *gouge_mm <= TOL + 1e-6, "shallow cut misread: {rest_mm}");
        }
        other => panic!("shallow thread with free-air start must convict as rest, got {other:?}"),
    }

    // Missed turn inside the part (pitch doubled).
    let mut skip = params(81.0, 65.0);
    skip.pitch = PITCH * 2.0;
    let t_skip = thread(&bar, &skip).expect("skip threads");
    let skip_warns = thread_warnings(&bar, &t_skip, INSERT_W, 60.0, TOL);
    assert!(!skip_warns.is_empty(), "missed turn with free-air start judged silent");

    // A free-air lattice point whose left flank actually enters and gouges
    // a solid end face (last groove Z76.8, next lattice point Z78.3 with
    // band Z77.7..Z78.9 cutting 0.40 mm into the solid Z78 end face).
    let clipped_bar = free_end_bar(76.8);
    let t_clip = thread(&clipped_bar, &params(81.0, 64.0)).expect("cuts");
    match thread_warnings(&clipped_bar, &t_clip, INSERT_W, 60.0, TOL).as_slice() {
        [Warning::Envelope { gouge_mm, .. }] => {
            assert!(*gouge_mm >= 0.35, "end-face flank gouge understated: {gouge_mm}");
        }
        other => panic!("free-air pass whose flank clips solid end face must gouge, got {other:?}"),
    }
}

#[test]
fn groove_at_free_end_face_spilling_into_end_air_is_not_rest() {
    // A groove cut to the bar's free end spills half a blade width into
    // end air (blade::centers' documented promise); end air is not rest.
    let bar = revolve(
        16,
        &[
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 36.0),
            (4.0, 36.0),
            (4.0, 40.0),
            (0.0, 40.0),
        ],
    );
    let path = groove_finish(
        &bar,
        &GrooveFinishParams {
            tolerance: TOL,
            clearance_z: 44.0,
            top_z: 40.0,
            bottom_z: 36.0,
            feed: 0.0,
            plunge_feed: 60.0,
            blade_width: 2.0,
        },
    )
    .expect("end groove finishes");
    let warns = blade_warnings(&bar, &path, 2.0, TOL, 0.0);
    assert!(warns.is_empty(), "end-face groove air reported as warning: {warns:?}");
}

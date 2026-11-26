//! The guide laws (review of PR #3630): a closed guide keeps its
//! closing span, the band's width is read at every knot of both
//! guides, and finite but extreme coordinates end in a refusal, never
//! a hang.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::axis::Axis;
use camcraft_motion::multiaxis::{rotary_flow, AxisControl};

use crate::fixture::*;

/// A closed ring of `n` corners at radius `r` and height `z`, its
/// first point repeated at the end (a picked closed edge loop).
fn ring(n: usize, r: f64, z: f64) -> Vec<Point3> {
    (0..=n).map(|k| gp(r, TAU * (k % n) as f64 / n as f64, z)).collect()
}

#[test]
fn a_closed_guide_pair_flows_the_whole_lap() {
    // The review's reproduction: two closed 12-sided rings on an R8
    // tube at Z0 and Z4. The closing span was dropped: the first pass
    // covered 330 degrees and left the last 30-degree strip uncut.
    let facets = prism(12, R_ROOT, 0.0, 4.0);
    let chains = vec![ring(12, R_ROOT, 0.0), ring(12, R_ROOT, 4.0)];
    let tool = ball(2.0);
    let path = rotary_flow(
        &chains,
        &facets,
        &stated_axis(),
        &params(&tool, Some(0.9)),
        &AxisControl::default(),
    )
    .unwrap();
    no_gouge(&path, &facets, 1.0);
    no_swept_gouge(&path, &facets, 1.0);
    let centers = first_pass_centers(&path, 14.0, 1.0);
    let span = wound(&centers).abs();
    assert!(
        (span - TAU).abs() < 0.05,
        "the first pass winds {:.1} degrees, not the closed guide's full lap",
        span.to_degrees()
    );
}

#[test]
fn the_band_width_is_read_at_every_guide_knot() {
    // The review's reproduction: guide A runs straight up an R8 prism
    // at azimuth 0; guide B steps out to 5 * d at a knot that falls
    // between two of the 33 fixed samples. The width there alone is
    // 8 * 5 * d = 3.49 mm: at a 0.9 mm stepover, 5 passes at least.
    let d = TAU / 72.0;
    let z = [0.0, 156.25, 312.5, 10_000.0];
    let facets = prism(72, R_ROOT, 0.0, 10_000.0);
    let a: Vec<Point3> = z.iter().map(|&v| gp(R_ROOT, 0.0, v)).collect();
    let b: Vec<Point3> = z
        .iter()
        .zip([d, 5.0 * d, d, d])
        .map(|(&v, th)| gp(R_ROOT, th, v))
        .collect();
    let tool = ball(2.0);
    let path = rotary_flow(
        &[a, b],
        &facets,
        &stated_axis(),
        &params(&tool, Some(0.9)),
        &AxisControl::default(),
    )
    .unwrap();
    let peak = R_ROOT * 5.0 * d;
    let want = (peak / 0.9).ceil() as usize + 1;
    let plunges = path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert!(
        plunges >= want,
        "the band divided into {plunges} passes; its {peak:.3} mm peak needs {want}: {:?}",
        path.notes
    );
}

#[test]
fn extreme_finite_coordinates_end_in_a_refusal_not_a_hang() {
    // The review's reproduction: at a radius of 1e16 a quarter-radius
    // march step is lost in the float spacing, so the push-out
    // repeated forever. Every input is finite.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let x = 1e16;
        let facets = vec![Triangle::new(p3(x, -1.0, 0.0), p3(x, 1.0, 0.0), p3(x, 0.0, 1.0))];
        let chains = vec![
            vec![p3(x, -0.5, 0.0), p3(x, 0.0, 0.5)],
            vec![p3(x, 0.5, 0.0), p3(x, 0.0, 0.6)],
        ];
        let axis = Axis {
            origin: Point3::ZERO,
            dir: p3(0.0, 0.0, 1.0),
        };
        let tool = ball(2.0);
        let mut q = params(&tool, Some(0.9));
        q.clearance = x + 20.0;
        let got = rotary_flow(&chains, &facets, &axis, &q, &AxisControl::default());
        let _ = tx.send(got.map(|p| p.moves.len()).map_err(|e| format!("{e}")));
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Err(_) => panic!("the kernel did not end within 60 s on finite, extreme coordinates"),
        Ok(Ok(n)) => panic!("the kernel posted {n} moves at a 1e16 mm radius"),
        Ok(Err(msg)) => assert!(
            msg.contains("float spacing"),
            "the refusal does not name the precision limit: {msg}"
        ),
    }
}

//! #2970 Rotary Parallel coverage law: on a plain bar laid along the
//! rotary axis, the passes go all the way round - every sector of the
//! circle holds a cutting move on the turned surface, at a coarse and
//! at a fine stepover. Angles are about the bar's axis (+x) in the
//! world frame: 90 degrees is the top (+z), 270 the bottom (-z).
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary
//! parallel) on this crate's own fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{Finish3DParams, FinishPattern, GapLink};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::{rotary_parallel, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
/// The bar: radius and length along +x.
const R: f64 = 3.0;
const LEN: f64 = 20.0;
/// The ball's radius (a 2 mm ball nose).
const BALL: f64 = 1.0;
const SECTOR_DEG: f64 = 30.0;

/// A closed round bar laid along +x, wound outward, `n` facets round.
fn bar(n: usize) -> Vec<Triangle> {
    let v = |j: usize, r: f64, x: f64| {
        let th = TAU * j as f64 / n as f64;
        Point3::new(x, r * th.cos(), r * th.sin())
    };
    let mut t = Vec::new();
    for j in 0..n {
        // Foot cap at x = 0 (normal -x), wall, crown cap at x = LEN (+x).
        t.push(Triangle::new(v(0, 0.0, 0.0), v(j + 1, R, 0.0), v(j, R, 0.0)));
        let (a1, a2, b1, b2) = (v(j, R, 0.0), v(j + 1, R, 0.0), v(j, R, LEN), v(j + 1, R, LEN));
        t.push(Triangle::new(a1, a2, b2));
        t.push(Triangle::new(a1, b2, b1));
        t.push(Triangle::new(v(0, 0.0, LEN), v(j, R, LEN), v(j + 1, R, LEN)));
    }
    t
}

fn params(stepover: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern: FinishPattern::Parallel,
        stepover: Some(stepover),
        tool_diameter: 2.0 * BALL,
        tolerance: 0.05,
        clearance_z: R + 6.0,
        retract_z: None,
        top_z: R,
        bottom_z: -R,
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

/// The angle (degrees, 0..360) about +x of each cutting ball center
/// that stands on the turned surface: away from the end faces, at one
/// ball radius off the wall within the tolerance.
fn cut_angles(path: &Toolpath) -> Vec<f64> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .filter_map(|m| {
            let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
            let c = Point3::new(m.to.x + BALL * a[0], m.to.y + BALL * a[1], m.to.z + BALL * a[2]);
            let rho = (c.y * c.y + c.z * c.z).sqrt();
            let on_wall = c.x > 1.0 && c.x < LEN - 1.0 && (rho - (R + BALL)).abs() < 0.1;
            on_wall.then(|| c.z.atan2(c.y).to_degrees().rem_euclid(360.0))
        })
        .collect()
}

/// The sectors (start angle, degrees) that hold no cutting move, and
/// the widest angular gap between neighbouring cutting moves.
fn coverage(angles: &[f64]) -> (Vec<f64>, f64) {
    let n = (360.0 / SECTOR_DEG) as usize;
    let mut hit = vec![false; n];
    for a in angles {
        let k = ((a / SECTOR_DEG) as usize).min(n - 1);
        hit[k] = true;
    }
    let empty = (0..n).filter(|k| !hit[*k]).map(|k| k as f64 * SECTOR_DEG).collect();
    let mut s: Vec<f64> = angles.to_vec();
    s.sort_by(f64::total_cmp);
    let mut gap = 360.0f64;
    if let (Some(first), Some(last)) = (s.first(), s.last()) {
        gap = s.windows(2).map(|w| w[1] - w[0]).fold(first + 360.0 - last, f64::max);
    }
    (empty, gap)
}

/// The passes (distinct angles, 0.1 degree apart or more) whose angle
/// lies within `half` degrees of `at`, as signed offsets from it, each
/// with the number of cutting moves that stand on it.
fn passes_near(angles: &[f64], at: f64, half: f64) -> Vec<(f64, usize)> {
    let mut d: Vec<f64> = angles
        .iter()
        .map(|a| (a - at + 540.0).rem_euclid(360.0) - 180.0)
        .filter(|d| d.abs() <= half)
        .collect();
    d.sort_by(f64::total_cmp);
    let mut out: Vec<(f64, usize)> = Vec::new();
    for x in d {
        match out.last_mut() {
            Some((a, n)) if x - *a < 0.1 => *n += 1,
            _ => out.push((x, 1)),
        }
    }
    out
}

fn widest_step(p: &[(f64, usize)]) -> f64 {
    p.windows(2).map(|w| w[1].0 - w[0].0).fold(0.0, f64::max)
}

fn check(stepover: f64) {
    let got = rotary_parallel(&bar(48), &params(stepover), &AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None })
        .expect("a laid bar takes passes");
    let angles = cut_angles(&got);
    let (empty, gap) = coverage(&angles);
    eprintln!(
        "stepover {stepover}: {} cutting moves on the wall, widest gap {gap:.1} deg, empty sectors {empty:?}, notes {:?}",
        angles.len(),
        got.notes
    );
    assert!(got.notes.iter().any(|n| n.starts_with("Rotary Parallel: the passes go all the way round")), "{:?}", got.notes);
    assert!(
        empty.is_empty(),
        "stepover {stepover}: the passes leave sectors {empty:?} uncut (widest gap {gap:.1} deg; 270 is the bottom)"
    );
    // The two seams (0 and 180 degrees, where the half turned below
    // meets the half planned from above) step no wider than the rings
    // on the flank above them, and no ring there is cut twice: each
    // pass at a seam carries fewer moves than two flank rings do (the
    // median flank pass is one ring; a raster line carries many more).
    let flank = passes_near(&angles, 150.0, 20.0);
    let mut counts: Vec<usize> = flank.iter().map(|p| p.1).collect();
    counts.sort_unstable();
    let ring_moves = counts.get(counts.len() / 2).copied().unwrap_or(0);
    for seam in [0.0, 180.0] {
        let at = passes_near(&angles, seam, 20.0);
        eprintln!("seam {seam}: passes {at:?}; flank {flank:?}");
        assert!(
            widest_step(&at) <= widest_step(&flank) + 0.5,
            "seam {seam}: a step of {:.1} deg, the flank's widest is {:.1}",
            widest_step(&at),
            widest_step(&flank)
        );
        for (d, n) in &at {
            assert!(*n < 2 * ring_moves, "seam {seam}: the pass at {d:.1} deg has {n} moves, a flank ring {ring_moves} - cut twice");
        }
    }
    // No cutting move is emitted twice.
    let feeds: Vec<_> = got.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    for (i, m) in feeds.iter().enumerate() {
        assert!(!feeds[..i].iter().any(|n| n.from == m.from && n.to == m.to), "a cutting move twice: {m:?}");
    }
}

#[test]
fn the_rotary_parallel_goes_all_the_way_round_a_laid_bar() {
    check(0.8);
}

#[test]
fn the_rotary_parallel_goes_all_the_way_round_at_a_fine_stepover() {
    check(0.25);
}



/// The raster's and the rings' notes (what they refused, reported not
/// cut) reach the Rotary Parallel path: the ladder run deep under the
/// bar refuses a ring entry, and the path says so.
#[test]
fn the_rotary_parallel_keeps_the_ladder_notes() {
    let p = Finish3DParams { bottom_z: -R - 2.0 * BALL, ..params(0.8) };
    let got = rotary_parallel(&bar(48), &p, &AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None })
        .expect("a laid bar takes passes");
    assert!(got.notes.iter().any(|n| n.starts_with("rings: ")), "{:?}", got.notes);
}

/// A Bottom over the axis keeps the cut to the upper arc, as before
/// #2970: no ball touches the bar under that height, and the path does
/// not say it goes all the way round. This is how a machine whose
/// rotary travel cannot turn the part a full turn keeps to what it
/// reaches.
#[test]
fn a_bottom_over_the_axis_keeps_the_cut_to_the_upper_arc() {
    let p = Finish3DParams { bottom_z: 1.0, ..params(0.8) };
    let got = rotary_parallel(&bar(48), &p, &AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None })
        .expect("a laid bar takes passes");
    let cuts: Vec<_> = got.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(cuts.len() > 20, "the upper arc is cut: {}", cuts.len());
    for m in cuts {
        let rho = (m.to.y * m.to.y + m.to.z * m.to.z).sqrt();
        if rho < R + 0.1 {
            // A tip on the bar is the contact: never under the Bottom.
            assert!(m.to.z >= 1.0 - 0.05 - 1e-9, "a contact under the Bottom: {m:?}");
        }
    }
    assert!(got.notes.iter().all(|n| !n.contains("all the way round")), "{:?}", got.notes);
}

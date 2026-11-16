//! Pins for the thread mill's pass control (#988): the constant-chip-area schedule, the finishing and spring passes, the fit allowance, and the climb sense under an M4 spindle with a left-hand-cut mill - and each refusal by name.
//!
//! Provenance: the laws are the handbook statements
//! [`super::thread_passes`] cites; the checks are our own.

use crate::cl::{ThreadHand, Toolpath};
use crate::geo2d::Point2;

use super::thread_mill::{thread_mill, ThreadHole, ThreadParams, EXTERNAL_THREAD_DEPTH_PER_PITCH, THREAD_DEPTH_PER_PITCH};
use super::thread_passes::{PassSchedule, ThreadPasses};

const R: f64 = 5.0;

fn params(external: bool, passes: ThreadPasses) -> ThreadParams {
    ThreadParams {
        holes: vec![ThreadHole { xy: Point2::new(10.0, 20.0), radius: R, top_z: 0.0, floor_z: -6.0, stepped: false, through: false }],
        pitch: 1.5,
        tool_diameter: 6.0,
        rows: 1,
        infeed: 0.3,
        external,
        hand: ThreadHand::Right,
        climb: true,
        passes,
        clearance_z: 10.0,
        r_z: None,
        retract_z: None,
        feed: 400.0,
        plunge_feed: 120.0,
    }
}

fn depth(external: bool) -> f64 {
    1.5 * if external { EXTERNAL_THREAD_DEPTH_PER_PITCH } else { THREAD_DEPTH_PER_PITCH }
}

/// The crest depth of every helix arc, in path order.
fn crest_depths(p: &ThreadParams, path: &Toolpath) -> Vec<f64> {
    let c = p.holes[0].xy;
    path.moves
        .iter()
        .filter(|m| m.arc.is_some_and(|a| (a.center.x - c.x).abs() < 1e-9 && (a.center.y - c.y).abs() < 1e-9))
        .map(|m| {
            let rho = (m.to.x - c.x).hypot(m.to.y - c.y);
            if p.external { R + 3.0 - rho } else { rho - (R - 3.0) }
        })
        .collect()
}

/// The distinct pass depths of a ride, in order (each pass is a run of
/// helix arcs at one radius; a spring pass repeats its radius).
fn pass_depths(p: &ThreadParams) -> Vec<f64> {
    let path = thread_mill(p).expect("a valid ride");
    let all = crest_depths(p, &path);
    let per_pass = all.len() / p.passes.depths(depth(p.external), p.infeed).len();
    all.chunks(per_pass).map(|c| c[0]).collect()
}

fn err_of(p: &ThreadParams) -> String {
    format!("{:?}", thread_mill(p).expect_err("refused"))
}

#[test]
fn constant_area_passes_decrease_and_each_takes_the_same_chip_area() {
    for external in [false, true] {
        let p = params(external, ThreadPasses { schedule: PassSchedule::ConstantArea, ..ThreadPasses::default() });
        let h = depth(external);
        let got = pass_depths(&p);
        let n = (h / 0.3).powi(2).ceil();
        assert_eq!(got.len() as f64, n, "{got:?}");
        assert!(got[0] <= 0.3 + 1e-6, "the first, largest pass is at most the infeed: {got:?}");
        // The V form's area grows with the square of its depth: equal
        // differences of the squares are equal chip areas.
        let area = got[0].powi(2);
        let mut last = 0.0_f64;
        for (i, d) in got.iter().enumerate() {
            assert!((d.powi(2) - last.powi(2) - area).abs() < 1e-5, "pass {i}: {got:?}");
            if i > 1 {
                assert!(d - last < got[i - 1] - got[i - 2] + 1e-9, "steps decrease: {got:?}");
            }
            last = *d;
        }
        assert!((last - h).abs() < 2e-6, "the last pass lands on the thread depth: {last} vs {h}");
    }
}

#[test]
fn a_finishing_pass_and_spring_passes_land_on_the_final_radius() {
    let passes = ThreadPasses { finish: 0.05, spring: 2, ..ThreadPasses::default() };
    let p = params(false, passes);
    let h = depth(false);
    let got = pass_depths(&p);
    let rough = h - 0.05;
    let n = (rough / 0.3).ceil() as usize;
    assert_eq!(got.len(), n + 3, "roughing, one finish, two spring: {got:?}");
    assert!((got[n - 1] - rough).abs() < 2e-6, "{got:?}");
    for d in &got[n..] {
        assert!((d - h).abs() < 2e-6, "finish and spring on the thread depth: {got:?}");
    }
    // The default ride is the #894 ride, untouched.
    assert_eq!(ThreadPasses::default().depths(h, 0.3).len(), (h / 0.3).ceil() as usize);
}

#[test]
fn an_allowance_leaves_the_stated_depth_uncut_and_is_refused_past_its_tolerance() {
    for external in [false, true] {
        let h = depth(external);
        let p = params(external, ThreadPasses { allowance: 0.04, ..ThreadPasses::default() });
        let got = pass_depths(&p);
        assert!(got.iter().all(|d| *d <= h - 0.04 + 2e-6), "{got:?}");
        assert!((got.last().copied().unwrap_or(0.0) - (h - 0.04)).abs() < 2e-6, "{got:?}");
        // Cutting deeper than the standard depth is past the major / design-root diameter.
        let deeper = params(external, ThreadPasses { allowance: -0.01, ..ThreadPasses::default() });
        assert!(err_of(&deeper).contains("past the thread's standard depth"), "{}", err_of(&deeper));
        let too_much = params(external, ThreadPasses { allowance: h * 0.3, ..ThreadPasses::default() });
        assert!(err_of(&too_much).contains("would not gauge"), "{}", err_of(&too_much));
    }
}

#[test]
fn an_m4_spindle_with_a_left_hand_cut_mill_flips_the_orbit_and_keeps_the_hand() {
    for external in [false, true] {
        for hand in ThreadHand::ALL {
            let mut m3 = params(external, ThreadPasses::default());
            m3.hand = hand;
            let mut m4 = m3.clone();
            m4.passes.spindle_reverse = true;
            m4.passes.cutter_left = true;
            assert_eq!(m4.ccw(), !m3.ccw(), "climb flips with the spindle");
            let path = thread_mill(&m4).expect("an M4 ride");
            let c = m4.holes[0].xy;
            let helix: Vec<_> = path
                .moves
                .iter()
                .filter(|m| m.arc.is_some_and(|a| (a.center.x - c.x).abs() < 1e-9 && (a.center.y - c.y).abs() < 1e-9))
                .collect();
            assert!(!helix.is_empty());
            for m in helix {
                assert_eq!(ThreadHand::of_move(m), Some(hand), "{external} {hand:?}: the thread's hand is the drawing's");
                assert_eq!(m.arc.map(|a| a.ccw), Some(m4.ccw()));
            }
        }
    }
}

#[test]
fn a_cutter_hand_against_the_spindle_and_other_nonsense_are_refused_by_name() {
    let rh_m4 = params(false, ThreadPasses { spindle_reverse: true, ..ThreadPasses::default() });
    assert!(err_of(&rh_m4).contains("right-hand-cut thread mill under an M4"), "{}", err_of(&rh_m4));
    let lh_m3 = params(false, ThreadPasses { cutter_left: true, ..ThreadPasses::default() });
    assert!(err_of(&lh_m3).contains("left-hand-cut thread mill under an M3"), "{}", err_of(&lh_m3));
    let springy = params(false, ThreadPasses { spring: 5, ..ThreadPasses::default() });
    assert!(err_of(&springy).contains("spring passes"), "{}", err_of(&springy));
    let all_finish = params(false, ThreadPasses { finish: depth(false), ..ThreadPasses::default() });
    assert!(err_of(&all_finish).contains("leaves nothing to rough"), "{}", err_of(&all_finish));
    let mut fine = params(false, ThreadPasses { schedule: PassSchedule::ConstantArea, ..ThreadPasses::default() });
    fine.infeed = 0.05;
    assert!(err_of(&fine).contains("raise the infeed"), "{}", err_of(&fine));
    let nan = params(false, ThreadPasses { finish: f64::NAN, ..ThreadPasses::default() });
    assert!(err_of(&nan).contains("finite"), "{}", err_of(&nan));
    // A negative finish would rough past depth - allowance: the last
    // roughing pass overshoots the thread depth, a gouge.
    let under = params(false, ThreadPasses { finish: -0.05, ..ThreadPasses::default() });
    assert!(err_of(&under).contains("finishing pass depth cannot be negative"), "{}", err_of(&under));
}

//! Issue #1591: where a trochoidal level's bore starts. A disc inside a
//! level this run cleared starts on that level's plane; a disc no level
//! holds starts one plunge height over the stock top; and a refused
//! level posts nothing and is said with its Z. Our own regression; no
//! vendor material consulted.

use super::Run;
use crate::cl::{MoveKind, PathBuilder};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::{AdaptiveMethod, PocketStrategy};
use crate::machining3d::Pocket3DParams;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(l) = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) else {
        panic!("fixture loop")
    };
    let Ok(r) = Region::new(vec![l]) else { panic!("fixture region") };
    r
}

fn params() -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        tool_diameter: 6.0,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: Some(5.0),
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        links: crate::links::Links::ROUGH3D,
    }
}

/// The lowest Z a rapid of the level at -6 ends on, with `cleared` as
/// what the run proved above it.
fn lowest_rapid(cleared: Vec<(f64, Region)>) -> f64 {
    let p = params();
    let mut run = Run::new(AdaptiveMethod::Trochoidal, &p, true);
    run.cleared = cleared;
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 10.0));
    // The 34 x 16 pocket's cutter-centre area (the D6's 28 x 10).
    let posted = match run.level(&mut b, &rect(3.0, 3.0, 31.0, 13.0), &p, -6.0, 5.0) {
        Ok(posted) => posted,
        Err(e) => panic!("the level plans: {e}"),
    };
    assert!(posted, "the level is the method's: {:?}", b.notes());
    b.moves().iter().filter(|m| m.kind == MoveKind::Rapid).map(|m| m.to.z).fold(f64::INFINITY, f64::min)
}

#[test]
fn a_bore_starts_on_a_plane_only_where_its_disc_is_proved_cleared() {
    // Nothing proved: one plunge height over the stock top.
    assert_eq!(lowest_rapid(Vec::new()), 2.0);
    // The level above swept the whole pocket: its plane.
    assert_eq!(lowest_rapid(vec![(-3.0, rect(0.0, 0.0, 34.0, 16.0))]), -3.0);
    // The level above cleared elsewhere only: the stock top again, for
    // every bore and every clean-up plunge.
    assert_eq!(lowest_rapid(vec![(-3.0, rect(100.0, 100.0, 134.0, 116.0))]), 2.0);
    // The deepest level that holds the disc is the one read.
    let both = vec![(-1.5, rect(0.0, 0.0, 34.0, 16.0)), (-3.0, rect(0.0, 0.0, 34.0, 16.0))];
    assert_eq!(lowest_rapid(both), -3.0);
}

#[test]
fn a_refused_level_posts_nothing_and_is_said_with_its_z() {
    let p = params();
    let mut run = Run::new(AdaptiveMethod::Trochoidal, &p, true);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 10.0));
    // A 0.2 mm wide centre area: no room to loop in.
    let posted = run.level(&mut b, &rect(3.0, 3.0, 31.0, 3.2), &p, -3.0, 5.0).unwrap_or(true);
    assert!(!posted, "the caller's plan cuts the level");
    assert_eq!(b.moves_len(), 0, "nothing of the attempt is posted");
    let notes = b.notes().to_vec();
    assert!(
        notes.len() == 1
            && notes[0].starts_with("3D Adaptive (Trochoidal): ")
            && notes[0].ends_with(" - the Spiral plan cuts the level at Z -3.000 instead"),
        "{notes:?}"
    );
}

#[test]
fn the_method_is_off_for_spiral_another_strategy_and_a_run_that_is_not_plain() {
    let p = params();
    assert!(Run::new(AdaptiveMethod::Trochoidal, &p, true).on());
    assert!(!Run::new(AdaptiveMethod::Spiral, &p, true).on());
    assert!(!Run::new(AdaptiveMethod::Trochoidal, &p, false).on(), "a rest pass, a shell, the wrap");
    let rings = Pocket3DParams { strategy: PocketStrategy::OffsetRings, ..p };
    assert!(!Run::new(AdaptiveMethod::Trochoidal, &rings, true).on());
    // Off, a level is the caller's and nothing is said.
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 10.0));
    let mut off = Run::new(AdaptiveMethod::Spiral, &p, true);
    assert!(matches!(off.level(&mut b, &rect(3.0, 3.0, 31.0, 13.0), &p, -3.0, 5.0), Ok(false)));
    assert!(b.moves_len() == 0 && b.notes().is_empty());
}

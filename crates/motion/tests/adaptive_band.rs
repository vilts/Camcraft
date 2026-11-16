//! Issue #13: 2D Adaptive on an extended Outside band - the annulus a
//! Containment Outside plus an extension leaves around a 60 x 50 part
//! (the user's "Milling-test" underside) - must plan in seconds or be
//! refused by name, never grind for minutes.
//!
//! Gate map: covers camcraft_motion::machining2d (adaptive, pocket_hsm).
//! Provenance: our own regression of the user's report; no vendor
//! material consulted.

use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, pocket};

const D: f64 = 10.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    let pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { panic!("fixture loop") };
    l
}

/// The band an Outside containment with an extension leaves round the
/// 60 x 50 part: the outline grown by `w` (rounded corners, as the
/// boundary offset grows it) less the part.
fn band(w: f64) -> Region {
    let Ok(part) = Region::new(vec![rect(0.0, 0.0, 60.0, 50.0)]) else { panic!("part") };
    let Ok(outer) = part.offset(w) else { panic!("outer") };
    let Ok(r) = outer.difference(&part) else { panic!("band") };
    r
}

fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        retract_z: Some(21.0),
        top_z: 20.0,
        bottom_z: -5.0,
        stepdown: 25.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

/// The user's band (14 mm: the tool's reach plus the 4 mm extension)
/// and a wider one: planned or refused by name in seconds, never ground
/// for minutes, and always cut.
#[test]
fn adaptive_on_an_annular_band_finishes_in_seconds() {
    for w in [14.0, 20.0] {
        let reg = band(w);
        let t0 = std::time::Instant::now();
        let r = pocket(&reg, &params());
        let secs = t0.elapsed().as_secs_f64();
        eprintln!("band {w}: {secs:.2} s, {:?}", r.as_ref().map(|p| p.notes.clone()));
        let limit = if cfg!(debug_assertions) { 30.0 } else { 5.0 };
        assert!(secs < limit, "the {w} mm band planned for {secs:.1} s (over {limit} s)");
        let Ok(p) = r else { panic!("the {w} mm band is cut: {r:?}") };
        assert!(!p.moves.is_empty(), "the {w} mm band is cut");
        if let Some(n) = p.notes.iter().find(|n| n.contains("Offset rings cut") && n.contains("refused")) {
            assert!(n.contains("mm wide") || n.contains("budget"), "a refusal names the channel or the budget: {n}");
        }
    }
}

/// The user's band itself is a channel no single spiral should grow
/// through: refused up front, by name.
#[test]
fn the_users_band_is_refused_by_name_up_front() {
    let Ok(p) = pocket(&band(14.0), &params()) else { panic!("the band is cut") };
    assert!(
        p.notes.iter().any(|n| n.contains("channel") && n.contains("mm wide") && n.contains("Offset rings cut")),
        "{:?}",
        p.notes
    );
}

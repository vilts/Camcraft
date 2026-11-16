//! #1303: the tilted reach reading (laws (c) and (c3)) dips a body band
//! by its rim's exact R·sin θ, not by its whole radius - so the verdict
//! changes continuously with the tilt instead of jumping just above the
//! upright tolerance of #938. Pinned both ways on the `beside_wall`
//! plate of `tool_body_bands` (the review reproduction of PR #973): a
//! small commanded tilt of a holder standing clear over the part no
//! longer convicts, and a tilt that really brings the rim into the part
//! still does.
//!
//! Provenance: our own fixtures; the expected verdicts are worked from
//! the band's geometry (lowest rim point = foot height - R·sin θ) in the
//! comments, no vendor material.
//!
//! Gate map (M4.4): covers sim::verify::reach, sim::verify::sweep, sim::verify::tilt_band.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("loop")
}

/// The plate of `tool_body_bands::beside_wall`: a part island x 40..60
/// (y 10..90, top z 0) in a 100 mm plate. Reach warnings only.
fn beside_wall(moves: Vec<Move>, g: &ToolGeom) -> Vec<Warning> {
    let region = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0), rect(40.0, 10.0, 60.0, 90.0)]).expect("region");
    let stock = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0)]).expect("stock");
    let cx = VerifyContext {
        region: &region,
        stock: Some(&stock),
        model: None,
        tool: g,
        cutter: None,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(&Toolpath::new(moves), &cx)
        .into_iter()
        .filter(|w| matches!(w, Warning::ShankHit { .. } | Warning::SweepHit { .. }))
        .collect()
}

/// The issue's tool: an r 12 holder over a Ø8 cutter - flutes to 8, an
/// r 4 shank to 14, the holder 14..44 above the tip.
fn r12_holder() -> ToolGeom {
    ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 4.0, shank_len: 6.0, holder_radius: 12.0, holder_len: 30.0 }
}

/// A unit axis tilted `rad` off +Z toward +X - toward the wall.
fn toward_wall(rad: f64) -> [f64; 3] {
    [rad.sin(), 0.0, rad.cos()]
}

/// Approach and plunge to `tip_z` at (31, 50), 9 mm off the wall at x
/// 40, every cut move held at `tilt` radians toward the wall (`None`:
/// no axis stated). Upright the r 12 holder overhangs the island to x
/// 43, its bottom at `tip_z + 14`.
fn plunge(tilt: Option<f64>, tip_z: f64) -> Vec<Move> {
    let posed = |m: Move| match tilt {
        Some(t) => m.with_axis(toward_wall(t)),
        None => m,
    };
    vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 10.0), Point3::new(31.0, 50.0, 10.0), None),
        posed(Move::new(MoveKind::Feed, Point3::new(31.0, 50.0, 10.0), Point3::new(31.0, 50.0, 5.0), Some(500.0))),
        posed(Move::new(MoveKind::Plunge, Point3::new(31.0, 50.0, 5.0), Point3::new(31.0, 50.0, tip_z), Some(80.0))),
    ]
}

const PART_HIT: Warning = Warning::ShankHit { at: 2, part: true };

/// (b) The reproduction: tip at z -9, the holder's bottom 5 mm over the
/// island's top. Clean upright and at the upright tolerance (1e-9 rad) -
/// and now just above it too. Before #1303 the last three convicted
/// `ShankHit { part: true }`: the tilted reading dropped the bottom by
/// the whole 12 mm. The rim really dips 12·sin θ: 2.4e-8 mm at 2e-9
/// rad, 0.002 mm at 0.01 degrees, 0.21 mm at 1 degree.
#[test]
fn a_small_tilt_of_a_holder_standing_clear_over_the_part_no_longer_convicts() {
    let g = r12_holder();
    assert_eq!(beside_wall(plunge(None, -9.0), &g), Vec::new(), "upright");
    for rad in [1e-9, 2e-9, 0.01f64.to_radians(), 1f64.to_radians()] {
        let w = beside_wall(plunge(Some(rad), -9.0), &g);
        assert_eq!(w, Vec::new(), "{rad} rad: the holder stands 5 mm clear of the part");
    }
}

/// (a) A real tilt that brings the rim into the part still convicts.
/// Tip at z -13.9: the holder's bottom 0.1 mm over the island, its low
/// rim (toward the wall, x ~43) over the part. The rim's lowest point
/// stands at `14·cos θ - 12·sin θ - 13.9`: +0.016 mm at 0.4 degrees
/// (clear), -0.026 mm at 0.6 degrees - in the part - and deeper from
/// there on, a large tilt included.
#[test]
fn a_tilt_that_brings_the_rim_into_the_part_still_convicts() {
    let g = r12_holder();
    assert_eq!(beside_wall(plunge(None, -13.9), &g), Vec::new(), "upright, 0.1 mm clear");
    assert_eq!(beside_wall(plunge(Some(0.4f64.to_radians()), -13.9), &g), Vec::new(), "0.4 degrees: 0.016 mm clear");
    for deg in [0.6f64, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0] {
        let low = 14.0 * deg.to_radians().cos() - 12.0 * deg.to_radians().sin() - 13.9;
        assert!(low < 0.0, "the fixture: the rim is under the top at {deg} degrees");
        let w = beside_wall(plunge(Some(deg.to_radians()), -13.9), &g);
        assert!(w.contains(&PART_HIT), "{deg} degrees: the rim stands {low} mm into the part: {w:?}");
    }
}

/// (c) Continuity: across the old threshold (1e-9 rad) and on up, the
/// verdict is monotone in the tilt - clean, then convicted, never back -
/// and it turns where the rim reaches the part (between 0.4 and 0.6
/// degrees here), not at the tolerance.
#[test]
fn the_verdict_is_monotone_in_the_tilt_across_the_upright_tolerance() {
    let g = r12_holder();
    let rads = [
        0.0, 1e-12, 5e-10, 1e-9, 1.01e-9, 2e-9, 1e-8, 1e-6, 1e-4, 0.01f64.to_radians(), 0.1f64.to_radians(),
        0.4f64.to_radians(), 0.6f64.to_radians(), 1f64.to_radians(), 3f64.to_radians(), 10f64.to_radians(),
        20f64.to_radians(), 30f64.to_radians(), 45f64.to_radians(), 60f64.to_radians(),
    ];
    let mut convicted = false;
    for rad in rads {
        let hit = beside_wall(plunge(Some(rad), -13.9), &g).contains(&PART_HIT);
        assert!(hit || !convicted, "the verdict falls back to clean at {rad} rad");
        assert_eq!(hit, rad >= 0.6f64.to_radians(), "{rad} rad");
        convicted = hit;
    }
}

/// Law (c3), the same column mid-turn: a level feed at z -13.9 toward
/// the wall that ends upright after a move held at `tilt`. Its end pose
/// is clean for law (c) (the holder 0.1 mm over the part); between, the
/// pose runs from the tilt back to upright.
fn turning_feed(tilt: f64) -> Vec<Move> {
    let lean = toward_wall(tilt);
    vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 10.0), Point3::new(29.0, 50.0, 10.0), None),
        Move::new(MoveKind::Feed, Point3::new(29.0, 50.0, 10.0), Point3::new(29.0, 50.0, 5.0), Some(500.0)).with_axis(lean),
        Move::new(MoveKind::Plunge, Point3::new(29.0, 50.0, 5.0), Point3::new(29.0, 50.0, -13.9), Some(80.0)).with_axis(lean),
        Move::new(MoveKind::Feed, Point3::new(29.0, 50.0, -13.9), Point3::new(31.0, 50.0, -13.9), Some(500.0))
            .with_axis([0.0, 0.0, 1.0]),
    ]
}

#[test]
fn the_mid_turn_sweep_dips_by_the_rim_too() {
    let g = r12_holder();
    // 0.01 degrees: no pose of the turn dips the rim more than 0.002 mm.
    // Before #1303 every tilted pose of the sweep stood 12 mm lower.
    assert_eq!(beside_wall(turning_feed(0.01f64.to_radians()), &g), Vec::new());
    // 1 degree: the turn starts with the rim 0.11 mm into the part.
    let w = beside_wall(turning_feed(1f64.to_radians()), &g);
    assert!(w.contains(&Warning::SweepHit { at: 3, part: true }), "the swing's rim in the part: {w:?}");
}

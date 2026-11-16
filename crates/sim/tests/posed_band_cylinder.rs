//! #1305 through the public laws: the posed fixture sweep reads a
//! leaning band as the flat-ended cylinder it is, so a holder standing
//! just over a clamp is no longer convicted by a capsule cap reaching a
//! whole radius under it. Where the sweep alone decides - fixed steel
//! under a rotary, judged in the machine frame - the verdict now reads
//! the true band; a rim that really reaches steel still convicts.
//! (In the path frame law (c2)'s own column enclosure still drops a
//! leaning band by its radius - #1303's half of the same cliff.)
//! Provenance: our own fixtures; the law is the Prime Directive's.
//!
//! Gate map (#1305): covers camcraft_sim::verify (posed_steel,
//! workholding), camcraft_sim::fixture_mesh.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{
    bands_vs_fixtures, bands_vs_workholding, rapids_vs_workholding, FixturePart, RotaryAxis, ToolGeom, Warning, Workholding,
};

/// A r 15 holder whose bottom stands 30 mm over the tip.
fn wide_holder() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 15.0,
        holder_len: 30.0,
    }
}

/// One 2 mm move along X at the origin, the pose leaning `deg` off +Z
/// toward +Y (about a rotary on world X).
fn run(kind: MoveKind, deg: f64) -> Toolpath {
    let r = deg.to_radians();
    let m = Move::new(kind, Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), Some(500.0));
    Toolpath::new(vec![m.with_axis([0.0, r.sin(), r.cos()])])
}

/// A clamp beside the cut, inside the holder's rim, its top at `top`.
fn clamp(top: f64) -> [FixtureBox; 1] {
    [FixtureBox { min: Point3::new(-4.0, 6.0, -10.0), max: Point3::new(4.0, 14.0, top) }]
}

fn rotary_x() -> RotaryAxis {
    RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] }
}

/// The table's clamp does not turn with the part: the machine stands
/// the leaning stack upright over it, the holder's flat bottom 0.5 mm
/// above its top at every lean. The capsule's cap reached 15 mm under.
#[test]
fn a_fixed_clamp_half_a_millimetre_under_the_holder_is_clear_at_any_lean() {
    let tool = wide_holder();
    let steel = clamp(29.5);
    let wh = Workholding { turning: &[], fixed: &steel, axis: Some(rotary_x()) };
    for deg in [0.01, 0.5, 5.0, 40.0, -40.0] {
        let cut = bands_vs_workholding(&run(MoveKind::Feed, deg), &tool, &wh, None, 0.01);
        assert!(cut.is_empty(), "feed leaning {deg} deg: {cut:?}");
        let rapid = rapids_vs_workholding(&run(MoveKind::Rapid, deg), &tool, &wh, None);
        assert!(rapid.is_empty(), "rapid leaning {deg} deg: {rapid:?}");
    }
}

/// The same clamp half a millimetre INTO the holder's bottom convicts,
/// on a cut by the holder's name and on a rapid as the tool body.
#[test]
fn a_fixed_clamp_reaching_into_the_holder_convicts_at_any_lean() {
    let tool = wide_holder();
    let steel = clamp(30.5);
    let wh = Workholding { turning: &[], fixed: &steel, axis: Some(rotary_x()) };
    for deg in [0.01, 0.5, 5.0, 40.0, -40.0] {
        let cut = bands_vs_workholding(&run(MoveKind::Feed, deg), &tool, &wh, None, 0.01);
        assert_eq!(cut, vec![Warning::FixtureHit { at: 0, fixture_part: FixturePart::Holder }], "feed leaning {deg} deg");
        let rapid = rapids_vs_workholding(&run(MoveKind::Rapid, deg), &tool, &wh, None);
        assert_eq!(rapid, vec![Warning::FixtureHit { at: 0, fixture_part: FixturePart::Rapid }], "rapid leaning {deg} deg");
    }
}

/// In the path frame a lean whose rim really dips into the clamp
/// (15 sin 5 deg = 1.3 mm of a 0.5 mm gap) convicts as before.
#[test]
fn a_lean_whose_rim_reaches_the_clamp_convicts_in_the_path_frame() {
    let tool = wide_holder();
    let hits = bands_vs_fixtures(&run(MoveKind::Feed, 5.0), &tool, &clamp(29.5), None, 0.01);
    assert_eq!(hits, vec![Warning::FixtureHit { at: 0, fixture_part: FixturePart::Holder }]);
    let upright = Toolpath::new(vec![Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), Some(500.0))]);
    assert!(bands_vs_fixtures(&upright, &tool, &clamp(29.5), None, 0.01).is_empty());
}

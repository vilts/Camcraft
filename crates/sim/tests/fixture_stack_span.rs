//! The fixture laws (c2)/(d2) read the whole tool stack over its true
//! z span (#424, #425). An upright rapid carries its shank, holder and
//! spindle housing past steel, not only its flute disc; a ramp lifts
//! the whole stack with its high end, so the holder's top there is
//! judged, not the window pinned at the move's deepest z. The strict-dip
//! silences hold for every band: a retract off a mounting face and flush
//! contact stay silent, penetration reports.
//!
//! Gate map (M4.4): covers sim::verify::laws, sim::verify::stack_span,
//! sim::verify::workholding, sim::fixture.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use camcraft_sim::cl::{ArcMove, ArcPlane, Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{
    bands_vs_fixtures, bands_vs_workholding, rapids_vs_fixtures, rapids_vs_fixtures_held, rapids_vs_fixtures_spindle,
    rapids_vs_workholding,
    FixturePart, RotaryAxis, ToolGeom, Warning, Workholding,
};

/// Flute 0..20 r3, shank 20..50 r5, holder 50..90 r20 above the tip:
/// the #424 repro's holder (r20, starting 50 above the tip).
fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 5.0,
        shank_len: 30.0,
        holder_radius: 20.0,
        holder_len: 40.0,
    }
}

/// [`tool`] with a fat shank (r8), for the traverse that names it.
fn fat_shank() -> ToolGeom {
    ToolGeom { shank_radius: 8.0, ..tool() }
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn bx(min: Point3, max: Point3) -> FixtureBox {
    FixtureBox { min, max }
}

fn rapid(a: Point3, b: Point3) -> Toolpath {
    Toolpath::new(vec![Move::new(MoveKind::Rapid, a, b, None)])
}

fn feed(a: Point3, b: Point3) -> Toolpath {
    Toolpath::new(vec![Move::new(MoveKind::Feed, a, b, Some(500.0))])
}

fn hit(part: FixturePart) -> Vec<Warning> {
    vec![Warning::FixtureHit { at: 0, fixture_part: part }]
}

/// The #424 repro's bolt: 15 off the spindle axis, standing to z 60.
fn bolt() -> FixtureBox {
    bx(p(14.0, -1.0, -20.0), p(16.0, 1.0, 60.0))
}

#[test]
fn an_upright_retract_beside_a_tall_bolt_names_the_holder() {
    // Vertical retract z 0 -> 50: the disc (r3), flute column and shank
    // (r5) stay clear of a bolt 15 off the axis; the holder (r20) stands
    // over it from z 50 up - and the bolt reaches z 60.
    let path = rapid(p(0.0, 0.0, 0.0), p(0.0, 0.0, 50.0));
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[bolt()]), hit(FixturePart::Holder));
    assert_eq!(
        rapids_vs_fixtures_spindle(&path, &tool(), &[bolt()], Some((30.0, 40.0))),
        hit(FixturePart::Holder),
        "the worst band struck is named, the housing above it notwithstanding"
    );
    // Through the workholding split: turning, fixed with no frame, and
    // fixed under a rotary (an upright move is the part frame's).
    let b = [bolt()];
    let axis = RotaryAxis { origin: p(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] };
    for wh in [
        Workholding { turning: &b, fixed: &[], axis: None },
        Workholding { turning: &[], fixed: &b, axis: None },
        Workholding { turning: &[], fixed: &b, axis: Some(axis) },
    ] {
        assert_eq!(rapids_vs_workholding(&path, &tool(), &wh, None), hit(FixturePart::Holder));
    }
    // The bolt cut down to the holder's floor at the retract's foot is
    // flush contact: silent. A thousandth more is a crash.
    let flush = bx(p(14.0, -1.0, -20.0), p(16.0, 1.0, 50.0));
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[flush]), Vec::new());
    let proud = bx(p(14.0, -1.0, -20.0), p(16.0, 1.0, 50.001));
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[proud]), hit(FixturePart::Holder));
}

#[test]
fn a_low_traverse_past_a_tall_bolt_names_the_shank() {
    // A rapid traverse at z 2 passing a bolt 8 off its line: the disc
    // (r3) and flute column miss it, the r8 shank (z 22..52) does not.
    let path = rapid(p(-50.0, 0.0, 2.0), p(50.0, 0.0, 2.0));
    let b = bx(p(-1.0, 7.0, -20.0), p(1.0, 9.0, 60.0));
    assert_eq!(rapids_vs_fixtures(&path, &fat_shank(), &[b]), hit(FixturePart::Shank));
    // A column 26 off the line: only the spindle housing (r30, from
    // z 92) reaches it - named when the machine states it, silent
    // when it does not (the unstated-limit law).
    let col = bx(p(-1.0, 25.0, 0.0), p(1.0, 27.0, 120.0));
    assert_eq!(rapids_vs_fixtures(&path, &fat_shank(), &[col]), Vec::new());
    assert_eq!(
        rapids_vs_fixtures_spindle(&path, &fat_shank(), &[col], Some((30.0, 40.0))),
        hit(FixturePart::Spindle)
    );
}

#[test]
fn a_rapid_under_an_overhanging_clamp_arm_hits_with_its_flute() {
    // The tip at z 5 runs under an arm at z 15..25: the disc is clear,
    // the flute column (z 5..25) is not - the tool body on a rapid.
    let path = rapid(p(-50.0, 0.0, 5.0), p(50.0, 0.0, 5.0));
    let arm = bx(p(-5.0, -5.0, 15.0), p(5.0, 5.0, 25.0));
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[arm]), hit(FixturePart::Rapid));
}

#[test]
fn the_strict_dip_silences_hold_for_the_whole_stack() {
    // A plate under the part, its top the mounting face at z 0, wide
    // enough that every band of the stack stands over it.
    let plate = bx(p(-200.0, -200.0, -30.0), p(200.0, 200.0, 0.0));
    let spindle = Some((30.0, 40.0));
    // A retract off exactly the mounting face: silent.
    let off = rapid(p(0.0, 0.0, 0.0), p(0.0, 0.0, 50.0));
    assert_eq!(rapids_vs_fixtures_spindle(&off, &tool(), &[plate], spindle), Vec::new());
    // A plunge landing exactly on it, a traverse riding it: silent.
    let on = rapid(p(0.0, 0.0, 50.0), p(0.0, 0.0, 0.0));
    assert_eq!(rapids_vs_fixtures_spindle(&on, &tool(), &[plate], spindle), Vec::new());
    let ride = rapid(p(-50.0, 0.0, 0.0), p(50.0, 0.0, 0.0));
    assert_eq!(rapids_vs_fixtures_spindle(&ride, &tool(), &[plate], spindle), Vec::new());
    // Dipping into the steel by a hundredth: the disc's verdict, as ever.
    let dip = rapid(p(0.0, 0.0, -0.01), p(0.0, 0.0, 50.0));
    assert_eq!(rapids_vs_fixtures_spindle(&dip, &tool(), &[plate], spindle), hit(FixturePart::Rapid));
    // A feed riding the face and a ramp leaving it: silent too (c2).
    let ramp = feed(p(0.0, 0.0, 0.0), p(40.0, 0.0, 40.0));
    assert_eq!(bands_vs_fixtures(&ramp, &tool(), &[plate], spindle, 0.01), Vec::new());
}

#[test]
fn a_ramp_judges_the_holder_top_at_its_high_end() {
    // The #425 repro: a ramp z 0 -> 40, a clamp arm at z 100..120 over
    // the high end. The holder spans 50..90 at the low end and 90..130
    // at the high end - its top is in the arm. The shank (20..90 over
    // the move) and the flute stay under it.
    let path = feed(p(0.0, 0.0, 0.0), p(40.0, 0.0, 40.0));
    let arm = bx(p(35.0, -5.0, 100.0), p(45.0, 5.0, 120.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[arm], None, 0.01), hit(FixturePart::Holder));
    let a = [arm];
    let wh = Workholding { turning: &a, fixed: &[], axis: None };
    assert_eq!(bands_vs_workholding(&path, &tool(), &wh, None, 0.01), hit(FixturePart::Holder));
    // Run it the other way (descending): the same move, the same span.
    let down = feed(p(40.0, 0.0, 40.0), p(0.0, 0.0, 0.0));
    assert_eq!(bands_vs_fixtures(&down, &tool(), &[arm], None, 0.01), hit(FixturePart::Holder));
    // The arm above the holder's highest top (130) is clear.
    let high = bx(p(35.0, -5.0, 130.0), p(45.0, 5.0, 150.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[high], None, 0.01), Vec::new());
}

#[test]
fn a_helical_ramp_reads_each_chord_at_its_own_height() {
    // Half a turn ccw about the origin, r10, climbing z 0 -> 20; an arm
    // over the end at z 95..105: the holder's top there is 110, its top
    // at the deepest end 90 - the old pinned window missed it.
    let mut m = Move::new(MoveKind::Feed, p(10.0, 0.0, 0.0), p(-10.0, 0.0, 20.0), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 0.0), ccw: true, plane: ArcPlane::Xy });
    let path = Toolpath::new(vec![m]);
    let arm = bx(p(-12.0, -2.0, 95.0), p(-8.0, 2.0, 105.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[arm], None, 0.01), hit(FixturePart::Holder));
}

#[test]
fn level_moves_keep_their_verdicts() {
    // A level feed at z 0: the shank window is 20..50 exactly - a box
    // floored at 50 is flush (silent), one dipping to 49.999 is struck.
    let path = feed(p(0.0, 0.0, 0.0), p(40.0, 0.0, 0.0));
    let flush = bx(p(10.0, -1.0, 50.0), p(12.0, 1.0, 60.0));
    let dip = bx(p(10.0, -1.0, 49.999), p(12.0, 1.0, 60.0));
    // The flush box is the holder's (50..90, r20) - named as such.
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[flush], None, 0.01), hit(FixturePart::Holder));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[dip], None, 0.01), hit(FixturePart::Shank));
    // A box over the holder's top (90) is clear on a level move.
    let over = bx(p(10.0, -1.0, 90.0), p(12.0, 1.0, 100.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[over], None, 0.01), Vec::new());
    // A level rapid at z 0 past the same box: clear above the stack.
    assert_eq!(rapids_vs_fixtures(&rapid(p(0.0, 0.0, 0.0), p(40.0, 0.0, 0.0)), &tool(), &[over]), Vec::new());
}

/// The #424 x #368 seam: an upright rapid judges a modelled holder
/// body's own bands (`HolderStack`), not the tool's holder cylinder. A
/// slim holder cylinder (r5) clears the bolt 15 off the axis; a body
/// whose collar is r20 over 50..90 does not - and the rapid must say so.
#[test]
fn an_upright_rapid_reads_a_modelled_holder_bodys_bands() {
    use camcraft_tools::holder_profile::{HolderBand, HolderStack};
    let slim = ToolGeom { holder_radius: 5.0, ..tool() };
    let path = rapid(p(0.0, 0.0, 0.0), p(0.0, 0.0, 50.0));
    assert_eq!(rapids_vs_fixtures(&path, &slim, &[bolt()]), Vec::new(), "the r5 cylinder clears the bolt");
    let body = HolderStack { bands: vec![HolderBand { above: 50.0, len: 40.0, r: 20.0 }], top: 90.0, neck: Vec::new() };
    assert_eq!(
        rapids_vs_fixtures_held(&path, &slim, &body, &[bolt()], None),
        hit(FixturePart::Holder),
        "the body's r20 collar reaches the bolt"
    );
}


//! #835 review: the band stack a body is judged by is a checked profile
//! (contiguous from the shank's top, or the enclosing cylinder stands),
//! the raised-foot gap is judged at the shank carried up to it - pinned
//! on both sides - and a swing through the part is never hidden behind
//! a stock-only shank conviction on the same move.
//!
//! Gate map (M4.4): covers sim::verify::reach, sim::verify::sweep, sim::verify::body_bands, sim::verify::probe, app::tool_body.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_body;
use camcraft_app::verify::{verify, BodyBands, Cutter, Mode, ToolGeom, VerifyContext, Warning};
use camcraft_tools::holder_profile::{HolderBand, HolderStack};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("loop")
}

fn cx<'a>(region: &'a Region, stock: &'a Region, g: &'a ToolGeom, cutter: Option<Cutter>) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: Some(stock),
        model: None,
        tool: g,
        cutter,
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
    }
}

fn reach_warnings(w: Vec<Warning>) -> Vec<Warning> {
    w.into_iter().filter(|w| matches!(w, Warning::ShankHit { .. } | Warning::SweepHit { .. })).collect()
}

/// Plunge to `tip_z` beside a 20 mm part island whose wall stands `gap`
/// off the tool's axis.
fn plunge_beside_island(gap: f64, tip_z: f64, g: &ToolGeom, cutter: Option<Cutter>) -> Vec<Warning> {
    let region = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0), rect(40.0, 40.0, 60.0, 60.0)]).expect("region");
    let stock = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0)]).expect("stock");
    let x = 40.0 - gap;
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 5.0), Point3::new(x, 50.0, 5.0), None),
        Move::new(MoveKind::Plunge, Point3::new(x, 50.0, 5.0), Point3::new(x, 50.0, tip_z), Some(80.0)),
    ]);
    reach_warnings(verify(&t, &cx(&region, &stock, g, cutter)))
}

// ---- contiguity is checked, not assumed -----------------------------

/// Flute 8, shank to 14, the enclosing holder r 20 to 49.
fn geom() -> ToolGeom {
    ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 4.0, shank_len: 6.0, holder_radius: 20.0, holder_len: 35.0 }
}

#[test]
fn a_gapped_stack_is_refused_and_the_cylinder_stands() {
    let g = geom();
    // 14..30, nothing 30..34, 34..49.
    let gapped = HolderStack {
        bands: vec![HolderBand { above: 14.0, len: 16.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    assert_eq!(BodyBands::of(&g, &gapped), None);
    assert_eq!(Cutter::flat(&g).with_body(&g, &gapped).holder_bands(), vec![(14.0, 20.0, 35.0)]);
    // The same stack closed up is carried.
    let closed = HolderStack {
        bands: vec![HolderBand { above: 14.0, len: 20.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    assert!(BodyBands::of(&g, &closed).is_some());
}

#[test]
fn a_late_starting_stack_is_refused_and_the_cylinder_stands() {
    let g = geom();
    // Nothing 14..20: the first band starts past the shank's top.
    let late = HolderStack {
        bands: vec![HolderBand { above: 20.0, len: 14.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    assert_eq!(BodyBands::of(&g, &late), None);
    assert_eq!(Cutter::flat(&g).with_body(&g, &late).holder_bands(), vec![(14.0, 20.0, 35.0)]);
    // Overlapping bands (running back down the axis) are no profile either.
    let overlap = HolderStack {
        bands: vec![HolderBand { above: 14.0, len: 22.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    assert_eq!(BodyBands::of(&g, &overlap), None);
}

// ---- the raised foot: the gap is the shank carried up ---------------

/// A drawn body r `body_r` from 24 to 50 above the tip, its foot 10 mm
/// above the shank's top (14): the stack and the record the laws read.
fn raised_foot(shank_r: f64, body_r: f64) -> (ToolGeom, HolderStack) {
    let profile = SpunProfile::new(vec![
        SpunPoint::line(24.0, 0.0),
        SpunPoint::line(24.0, body_r),
        SpunPoint::line(50.0, body_r),
        SpunPoint::line(50.0, 0.0),
    ])
    .expect("a sane profile")
    .holder_profile();
    // The judged record: `HolderProfile::enclose`'s holder, r body_r from
    // the shank's top to the body's top.
    let g = ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: shank_r, shank_len: 6.0, holder_radius: body_r, holder_len: 36.0 };
    let stack = tool_body::stack(&g, Some(&profile));
    assert_eq!(stack.bands[0], HolderBand { above: 14.0, len: 10.0, r: shank_r }, "the gap is the shank carried up");
    (g, stack)
}

#[test]
fn a_raised_foot_over_a_slim_shank_is_judged_looser_than_the_cylinder() {
    // Holder wider than the shank: the cylinder judged the gap at r 20,
    // the stack at the shank's r 5. Wall 10 off, tip 20 deep: the gap
    // (bottom 6 under the top) stands clear at r 5, the body's foot
    // stands 4 above the top - only the cylinder hit.
    let (g, stack) = raised_foot(5.0, 20.0);
    let body = Cutter::flat(&g).with_body(&g, &stack);
    assert_eq!(body.holder_bands(), vec![(14.0, 5.0, 10.0), (24.0, 20.0, 26.0)]);
    assert_eq!(plunge_beside_island(10.0, -20.0, &g, None), vec![Warning::ShankHit { at: 1, part: true }]);
    assert_eq!(plunge_beside_island(10.0, -20.0, &g, Some(body)), Vec::new());
    // The body itself in the wall is still judged.
    assert_eq!(plunge_beside_island(10.0, -30.0, &g, Some(body)), vec![Warning::ShankHit { at: 1, part: true }]);
}

#[test]
fn a_raised_foot_over_a_fat_shank_is_judged_stricter_than_the_cylinder() {
    // Shank wider than the body: the cylinder judged the gap at the
    // body's r 6, the stack at the shank's r 8. Wall 7 off: the shank
    // band convicts either way; the holder group now convicts too.
    let (g, stack) = raised_foot(8.0, 6.0);
    let body = Cutter::flat(&g).with_body(&g, &stack);
    assert_eq!(body.holder_bands(), vec![(14.0, 8.0, 10.0), (24.0, 6.0, 26.0)]);
    assert_eq!(plunge_beside_island(7.0, -20.0, &g, None), vec![Warning::ShankHit { at: 1, part: true }]);
    assert_eq!(
        plunge_beside_island(7.0, -20.0, &g, Some(body)),
        vec![Warning::ShankHit { at: 1, part: true }, Warning::ShankHit { at: 1, part: true }]
    );
}

// ---- (c3) is not hidden behind a stock-only (c) --------------------

#[test]
fn a_swing_through_the_part_is_not_hidden_behind_a_stock_shank_hit() {
    // The region runs 10 mm past the stock on -X, so the stock's wall at
    // x = 0 is uncut stock (inside the region, not part); the island wall
    // at x = 17 is part. The plunge at x = 12 ends leaning 30 degrees
    // toward -X: its shank column (centre x 6.5, r 8.5) reaches the stock
    // wall only (law (c): stock). Turning there from home, the still-
    // upright shank (r 7 about x 12) grazes the island (law (c3): part).
    // Both must be reported.
    let region = Region::new(vec![rect(-10.0, 0.0, 40.0, 30.0), rect(17.0, 5.0, 27.0, 25.0)]).expect("region");
    let stock = Region::new(vec![rect(0.0, 0.0, 40.0, 30.0)]).expect("stock");
    let g = ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 7.0, shank_len: 6.0, holder_radius: 0.0, holder_len: 0.0 };
    let tilt = |deg: f64| [deg.to_radians().sin(), 0.0, deg.to_radians().cos()];
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 5.0), Point3::new(12.0, 15.0, 5.0), None),
        Move::new(MoveKind::Plunge, Point3::new(12.0, 15.0, 5.0), Point3::new(12.0, 15.0, -20.0), Some(80.0)).with_axis(tilt(-30.0)),
    ]);
    let w = reach_warnings(verify(&t, &cx(&region, &stock, &g, None)));
    assert!(w.contains(&Warning::ShankHit { at: 1, part: false }), "law (c) sees the stock wall: {w:?}");
    assert!(w.contains(&Warning::SweepHit { at: 1, part: true }), "the swing through the island is reported: {w:?}");
    assert!(!w.contains(&Warning::SweepHit { at: 1, part: false }), "a stock sweep on the same move is the same event: {w:?}");
}

// ---- #903: a leaning band's disc is not the cutter's kerf -----------

/// The stepped body of `tool_body_judged`: an r 6 nose 14..34 above the
/// tip, an r 20 flange 34..49, on a Ø8 end mill with an r 4 shank.
fn stepped_body() -> (ToolGeom, Cutter) {
    let g = ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 4.0, shank_len: 6.0, holder_radius: 20.0, holder_len: 35.0 };
    let stack = HolderStack {
        bands: vec![HolderBand { above: 14.0, len: 20.0, r: 6.0 }, HolderBand { above: 34.0, len: 15.0, r: 20.0 }],
        top: 49.0,
        neck: Vec::new(),
    };
    let body = Cutter::flat(&g).with_body(&g, &stack);
    assert!(body.body.is_some(), "the stepped stack is carried");
    (g, body)
}

/// A plain slim holder, no body: r 6 x 20 over the r 4 shank.
fn slim_holder() -> ToolGeom {
    ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: 4.0, shank_len: 6.0, holder_radius: 6.0, holder_len: 20.0 }
}

fn deg_toward_x(deg: f64) -> [f64; 3] {
    [deg.to_radians().sin(), 0.0, deg.to_radians().cos()]
}

/// The review's reproduction: a part island x 40..60 (y 10..90, top z 0)
/// in a 100 mm plate, tip at (31, 50) - its flutes stay clear of the
/// wall at every pose used here (x <= 39 leaning 30 degrees toward it).
fn beside_wall(moves: Vec<Move>, g: &ToolGeom, cutter: Option<Cutter>) -> Vec<Warning> {
    let region = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0), rect(40.0, 10.0, 60.0, 90.0)]).expect("region");
    let stock = Region::new(vec![rect(0.0, 0.0, 100.0, 100.0)]).expect("stock");
    reach_warnings(verify(&Toolpath::new(moves), &cx(&region, &stock, g, cutter)))
}

/// Law (c): lean 30 degrees toward the wall and plunge to z -9. The shank
/// 12 mm up the axis stands at x 40.5, z -0.6 - in the part.
fn leaning_plunge() -> Vec<Move> {
    let lean = deg_toward_x(30.0);
    vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 10.0), Point3::new(31.0, 50.0, 10.0), None),
        Move::new(MoveKind::Feed, Point3::new(31.0, 50.0, 10.0), Point3::new(31.0, 50.0, 5.0), Some(500.0)).with_axis(lean),
        Move::new(MoveKind::Plunge, Point3::new(31.0, 50.0, 5.0), Point3::new(31.0, 50.0, -9.0), Some(80.0)).with_axis(lean),
    ]
}

#[test]
fn a_leaning_stepped_body_in_the_wall_is_convicted_by_law_c() {
    let (g, body) = stepped_body();
    let w = beside_wall(leaning_plunge(), &g, Some(body));
    assert!(w.contains(&Warning::ShankHit { at: 2, part: true }), "the leaning nose/shank in the part: {w:?}");
    // The enclosing cylinder convicts it too - the body is never looser here.
    let w = beside_wall(leaning_plunge(), &g, None);
    assert!(w.contains(&Warning::ShankHit { at: 2, part: true }), "{w:?}");
}

#[test]
fn a_leaning_slim_holder_in_the_wall_is_convicted_by_law_c() {
    let g = slim_holder();
    let w = beside_wall(leaning_plunge(), &g, None);
    assert!(w.contains(&Warning::ShankHit { at: 2, part: true }), "the leaning slim holder in the part: {w:?}");
}

/// Law (c3): the plunge to z -12 turns from 30 degrees toward the wall
/// to 30 degrees away. The end pose leans away - law (c) sees no part -
/// but mid-turn, still leaning toward the wall at z -8.6, the r 6 nose
/// (or holder) stands in the part. Shallow enough that the stepped
/// body's r 20 flange stays above the top at every such state, so the
/// nose alone must be judged.
fn turning_plunge() -> Vec<Move> {
    vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, -20.0, 10.0), Point3::new(31.0, 50.0, 10.0), None),
        Move::new(MoveKind::Feed, Point3::new(31.0, 50.0, 10.0), Point3::new(31.0, 50.0, 5.0), Some(500.0))
            .with_axis(deg_toward_x(30.0)),
        Move::new(MoveKind::Plunge, Point3::new(31.0, 50.0, 5.0), Point3::new(31.0, 50.0, -12.0), Some(80.0))
            .with_axis(deg_toward_x(-30.0)),
    ]
}

#[test]
fn a_leaning_stepped_body_swinging_through_the_wall_is_convicted_by_law_c3() {
    let (g, body) = stepped_body();
    let w = beside_wall(turning_plunge(), &g, Some(body));
    // Leaning away, the r 20 flange's column reaches the plate's own
    // stock edge at x 0 (stock, not part) - the part is (c3)'s finding.
    assert!(!w.contains(&Warning::ShankHit { at: 2, part: true }), "the end pose leans away from the part: {w:?}");
    assert!(w.contains(&Warning::SweepHit { at: 2, part: true }), "the swing through the part: {w:?}");
}

#[test]
fn a_leaning_slim_holder_swinging_through_the_wall_is_convicted_by_law_c3() {
    let g = slim_holder();
    let w = beside_wall(turning_plunge(), &g, None);
    assert!(!w.iter().any(|w| matches!(w, Warning::ShankHit { at: 2, .. })), "the end pose leans away: {w:?}");
    assert!(w.contains(&Warning::SweepHit { at: 2, part: true }), "the swing through the part: {w:?}");
}

#[test]
fn upright_verdicts_are_unchanged_by_the_tip_centred_kerf() {
    // Upright the band's disc and the kerf share the tip: the annulus
    // law is the one it always was. (gap, tip z) -> verdict.
    let hit = vec![Warning::ShankHit { at: 1, part: true }];
    let (g, body) = stepped_body();
    let slim = slim_holder();
    for (gap, tip_z, want_body, want_slim) in [
        (5.0, -20.0, hit.clone(), hit.clone()), // r 6 nose / holder, bottom at -6
        (7.0, -20.0, Vec::new(), Vec::new()), // clear of r 6; the flange's bottom (14) above the top
        (5.0, -5.0, Vec::new(), Vec::new()), // the r 6 bottom above the top (z 9)
        (15.0, -40.0, hit.clone(), Vec::new()), // the flange (r 20, bottom -6) in the wall
    ] {
        assert_eq!(plunge_beside_island(gap, tip_z, &g, Some(body)), want_body, "body: gap {gap}, tip {tip_z}");
        assert_eq!(plunge_beside_island(gap, tip_z, &slim, None), want_slim, "slim: gap {gap}, tip {tip_z}");
    }
}

// ---- #903 re-review: a shank no wider than the cutter, leaning -------

/// A long-reach Ø8 end mill, no body: shank r `shank_r` from 8 to 38
/// above the tip, holder r 20 from 38 to 58.
fn long_reach(shank_r: f64) -> ToolGeom {
    ToolGeom { radius: 4.0, nose_radius: 0.0, flute_len: 8.0, shank_radius: shank_r, shank_len: 30.0, holder_radius: 20.0, holder_len: 20.0 }
}

#[test]
fn a_leaning_shank_no_wider_than_the_cutter_is_convicted_by_law_c() {
    // 30 degrees toward the wall at x 40, tip (31, 50, -9): the shank 12
    // mm up the axis stands at x ~40.5, z ~-0.6 - in the island. The
    // holder's bottom stays above the top: only the shank can tell.
    for shank_r in [4.0, 3.5] {
        let w = beside_wall(leaning_plunge(), &long_reach(shank_r), None);
        assert!(w.contains(&Warning::ShankHit { at: 2, part: true }), "shank r {shank_r}: {w:?}");
    }
}

#[test]
fn a_leaning_shank_no_wider_than_the_cutter_swinging_through_the_wall_is_convicted_by_law_c3() {
    for shank_r in [4.0, 3.5] {
        let w = beside_wall(turning_plunge(), &long_reach(shank_r), None);
        assert!(!w.contains(&Warning::ShankHit { at: 2, part: true }), "the end pose leans away: shank r {shank_r}: {w:?}");
        assert!(w.contains(&Warning::SweepHit { at: 2, part: true }), "the swing through the part: shank r {shank_r}: {w:?}");
    }
}

#[test]
fn an_upright_shank_no_wider_than_the_cutter_reads_unchanged() {
    // Upright, a shank within the cutter's radius stands in the flutes'
    // kerf and is not judged; only the r 20 holder can convict.
    let hit = vec![Warning::ShankHit { at: 1, part: true }];
    for shank_r in [4.0, 3.5] {
        let g = long_reach(shank_r);
        for (gap, tip_z, want) in [
            (5.0, -20.0, Vec::new()), // holder bottom at 18: above the top
            (5.0, -36.0, Vec::new()), // holder bottom at 2: above the top
            (15.0, -40.0, hit.clone()), // holder (r 20, bottom -2) in the wall
        ] {
            assert_eq!(plunge_beside_island(gap, tip_z, &g, None), want, "shank r {shank_r}: gap {gap}, tip {tip_z}");
        }
    }
}

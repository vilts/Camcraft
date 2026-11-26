//! M13.11 facing rows honouring holes (the M1.5 leftover): a row is
//! clipped a tool radius out of every bridged hole, its pieces are cut,
//! and the row crosses the hole through the clearance plane; an air
//! hole is cut straight across only when asked, an island never; a
//! hole-free region faces identically under every law, and `Through`
//! is the M1.3 band serpentine to the byte; a strip narrower than the
//! tool beside a hole refuses by name; staged facing lifts per level.
//!
//! Gate map (M13.11): covers camcraft_motion::machining2d (face, face_holes).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{face, face_over, FaceHoles, FaceParams};

fn sq(c: (f64, f64), hx: f64, hy: f64) -> Loop {
    Loop::new(vec![
        Point2::new(c.0 - hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 + hy),
        Point2::new(c.0 - hx, c.1 + hy),
    ])
    .unwrap()
}

/// A 60x40 plate about the origin, with the given 10x10 holes.
fn plate(holes: &[(f64, f64)]) -> Region {
    let mut loops = vec![sq((0.0, 0.0), 30.0, 20.0)];
    loops.extend(holes.iter().map(|&c| sq(c, 5.0, 5.0)));
    Region::new(loops).unwrap()
}

fn p(holes: FaceHoles) -> FaceParams {
    FaceParams {
        tool_diameter: 6.0,
        stepover: 3.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        holes,
        stock_top: None,
    }
}

fn feeds(t: &Toolpath) -> Vec<&Move> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect()
}

fn plunges(t: &Toolpath) -> usize {
    t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count()
}

/// Does the XY segment of `m` come within `r` of the hole square at `c`?
/// Read with the region's own arc tolerance (5 um): the grown hole's
/// rounded corners are chorded inside the true circle by that much.
fn feed_reaches_hole(m: &Move, c: (f64, f64), r: f64) -> bool {
    let n = 64;
    let r = r - 0.01;
    (0..=n).any(|k| {
        let t = k as f64 / n as f64;
        let (x, y) = (m.from.x + (m.to.x - m.from.x) * t, m.from.y + (m.to.y - m.from.y) * t);
        let dx = (x - c.0).abs() - 5.0;
        let dy = (y - c.1).abs() - 5.0;
        dx.max(0.0).hypot(dy.max(0.0)) < r
    })
}

#[test]
fn rows_over_a_through_hole_are_clipped_and_lifted_at_clearance() {
    let region = plate(&[(0.0, 0.0)]);
    let t = face(&region, &p(FaceHoles::Lift)).expect("faced");
    assert!(t.all_finite());
    // No cut comes within a tool radius of the hole.
    for m in feeds(&t) {
        assert!(!feed_reaches_hole(m, (0.0, 0.0), 3.0), "a row cut into the hole: {m:?}");
    }
    // The rows across the hole's band (|y| < 8) were split and re-entered:
    // one extra plunge per split row, every link riding the clearance plane.
    let split = plunges(&t) - 1;
    assert!((5..=7).contains(&split), "five rows cross the hole's grown band, got {split} links");
    let mut lifted = 0;
    for w in t.moves.windows(3) {
        if w[0].kind == MoveKind::Rapid && w[1].kind == MoveKind::Rapid && w[2].kind == MoveKind::Plunge {
            assert_eq!(w[0].to.z, 5.0, "up to clearance");
            assert_eq!(w[1].to.z, 5.0, "across at clearance");
            assert_eq!(w[2].to.z, -2.0, "back down to the cut");
            lifted += 1;
        }
    }
    assert_eq!(lifted, split, "every link is up, across, down");
    assert!(t.notes.iter().any(|n| n.contains("lifted over")), "{:?}", t.notes);
}

#[test]
fn a_hole_free_region_faces_identically_under_every_law() {
    let region = plate(&[]);
    let lift = face(&region, &p(FaceHoles::Lift)).expect("lift");
    let air = face(&region, &p(FaceHoles::CutAir)).expect("cut air");
    let through = face(&region, &p(FaceHoles::Through)).expect("through");
    assert_eq!(lift.moves, through.moves);
    assert_eq!(air.moves, through.moves);
    assert_eq!(plunges(&lift), 1);
    assert!(lift.notes.is_empty(), "{:?}", lift.notes);
}

#[test]
fn through_is_the_band_serpentine_hole_or_no_hole() {
    let holed = face(&plate(&[(0.0, 0.0)]), &p(FaceHoles::Through)).expect("through");
    let plain = face(&plate(&[]), &p(FaceHoles::Through)).expect("plain");
    assert_eq!(holed.moves, plain.moves, "the M1.3 rows, holes ignored");
    assert!(holed.notes.is_empty());
}

#[test]
fn cut_air_runs_across_an_air_hole_and_lifts_over_an_island() {
    let region = plate(&[(0.0, 0.0)]);
    let lift = face(&region, &p(FaceHoles::Lift)).expect("lift");
    let through = face(&region, &p(FaceHoles::Through)).expect("through");
    // Nothing stands in the hole: it is air, and the row runs across it.
    let air = face_over(&region, &p(FaceHoles::CutAir), 0.0, Some(&Region::empty())).expect("air");
    assert_eq!(air.moves, through.moves);
    // The hole is a boss: an island, lifted over exactly as Lift does.
    let boss = Region::new(vec![sq((0.0, 0.0), 5.0, 5.0)]).unwrap();
    let island = face_over(&region, &p(FaceHoles::CutAir), 0.0, Some(&boss)).expect("island");
    assert_eq!(island.moves, lift.moves);
    // No outline at all: every hole is lifted over, and the path says why.
    let blind = face(&region, &p(FaceHoles::CutAir)).expect("blind");
    assert_eq!(blind.moves, lift.moves);
    assert!(blind.notes.iter().any(|n| n.contains("no material outline")), "{:?}", blind.notes);
}

#[test]
fn a_strip_narrower_than_the_tool_between_two_holes_is_refused_by_name() {
    // Two holes 4 mm apart: the 6 mm tool cannot stand between them, and
    // the strip would stay unfaced - refused, never faced short.
    let region = plate(&[(-7.0, 0.0), (7.0, 0.0)]);
    let err = face(&region, &p(FaceHoles::Lift)).expect_err("refused").to_string();
    assert!(err.contains("4.00 mm apart") && err.contains("no wider than the 6.000 mm tool"), "{err}");
    // Far enough apart, both are bridged and the strip between is faced.
    let ok = face(&plate(&[(-12.0, 0.0), (12.0, 0.0)]), &p(FaceHoles::Lift)).expect("two holes bridged");
    assert!(plunges(&ok) > 1);
    // The legacy law never refuses: it never looked at the holes.
    assert!(face(&region, &p(FaceHoles::Through)).is_ok());
}

#[test]
fn the_narrow_strip_refusal_points_at_the_strip() {
    // #261 part 2: the refusal names its spot - the middle of the 4 mm
    // strip (x = 0, between the holes' walls at -2 and 2), on the row
    // its sentence quotes, at the cut height.
    let err = face(&plate(&[(-7.0, 0.0), (7.0, 0.0)]), &p(FaceHoles::Lift)).expect_err("refused");
    let at = err.location().expect("a located refusal");
    assert!(at.x.abs() < 1e-9, "the strip's middle: {at:?}");
    assert_eq!(at.z, -2.0, "the cut height");
    assert!(at.y.abs() <= 5.0, "a row within the holes' band (its edge included): {at:?}");
    assert!(err.to_string().contains(&format!("on the row at Y={:.2}", at.y)), "the row the sentence names: {err}");
}

#[test]
fn staged_facing_lifts_over_the_hole_at_every_level() {
    let region = plate(&[(0.0, 0.0)]);
    let single = face(&region, &p(FaceHoles::Lift)).expect("single");
    let mut staged = p(FaceHoles::Lift);
    staged.top_z = Some(0.0);
    staged.stepdown = Some(1.0);
    let t = face(&region, &staged).expect("staged");
    // Two levels (-1, -2; #2004: no air level at Top Z 0), each with
    // the single level's plunges.
    assert_eq!(plunges(&t), 2 * plunges(&single));
    assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z == 0.0), "no air level at Top Z");
    for z in [-1.0, -2.0] {
        assert!(t.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z == z), "level {z}");
    }
    for m in feeds(&t) {
        assert!(!feed_reaches_hole(m, (0.0, 0.0), 3.0), "{m:?}");
    }
}

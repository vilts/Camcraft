//! Issue #3679: law (d3) judges a feed plunge against the floor that the
//! cuts before it left under the FULL tool disc - a dive whose rim
//! stands on stock that no level above cut convicts when its cutter
//! reaches deeper below that stock than one stepdown of the operation.
//! A dive exactly one stepdown deep, a dive in the kerf of the level
//! above (the reporter's workaround, 2.7 mm), and a pecked dive stay
//! clean. `plunge_stepdown: None` keeps the law silent.
//!
//! Provenance: our own fixtures, specced from the reporter's program of
//! #3453 (18 mm sheet, stepdown 4.5, four tabs) and the shop rule "one
//! stepdown of material per plunge"; no vendor material.
//!
//! Gate map (#3679): covers camcraft_sim::verify (plunge_depth, rapids, rest).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// The reporter's stepdown (#3453).
const STEP: f64 = 4.5;

fn rect() -> Region {
    let l = Loop::new(vec![
        Point2::new(-40.0, -40.0),
        Point2::new(40.0, -40.0),
        Point2::new(40.0, 40.0),
        Point2::new(-40.0, 40.0),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

fn at(kind: MoveKind, a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
    Move {
        rotary: None,
        kind,
        from: Point3::new(a.0, a.1, a.2),
        to: Point3::new(b.0, b.1, b.2),
        feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
        arc: None,
        axis: None,
    }
}

/// A closed square of flat feeds at `z` about `(cx, cy)`, half-width `h`.
fn square(m: &mut Vec<Move>, cx: f64, cy: f64, h: f64, z: f64) {
    let (lo, hi) = ((cx - h, cy - h), (cx + h, cy + h));
    let corners = [lo, (hi.0, lo.1), hi, (lo.0, hi.1)];
    for k in 0..4 {
        let a = corners[k];
        let b = corners[(k + 1) % 4];
        m.push(at(MoveKind::Feed, (a.0, a.1, z), (b.0, b.1, z)));
    }
}

/// `moves` on a flat tool of radius 3 over a stock box whose top is Z 0.
fn judge(moves: Vec<Move>, stepdown: Option<f64>) -> Vec<Warning> {
    let p = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let region = rect();
    let tool = ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 3.0,
        holder_len: 1.0,
    };
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        tool: &tool,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 12.0,
        gouge_tol: 0.01,
        z_tol: 0.05,
        plunge_stepdown: stepdown,
        ..Default::default()
    };
    verify(&p, &cx)
}

/// The reporter's program shape (#3453, #3678): levels cut in squares
/// far from the dive (no chord of theirs reaches the dive's disc), then
/// a rapid to the dive station and one 18 mm feed plunge beside stock
/// that no level above cut, then the levels the dive entered for.
fn tabbed_program() -> Vec<Move> {
    let mut m = Vec::new();
    // The far levels: entered one stepdown at a time, cut as squares
    // about (30, 30) - their chords stay more than 20 mm from the dive.
    m.push(at(MoveKind::Rapid, (0.0, 0.0, 25.0), (30.0, 30.0, 5.0)));
    m.push(at(MoveKind::Plunge, (30.0, 30.0, 5.0), (30.0, 30.0, -4.5)));
    square(&mut m, 30.0, 30.0, 5.0, -4.5);
    m.push(at(MoveKind::Rapid, (30.0, 30.0, -4.5), (30.0, 30.0, 5.0)));
    m.push(at(MoveKind::Plunge, (30.0, 30.0, 5.0), (30.0, 30.0, -9.0)));
    square(&mut m, 30.0, 30.0, 5.0, -9.0);
    // The tab window dive: one 18 mm feed plunge at a station no level
    // above cut (#3678's open case).
    m.push(at(MoveKind::Rapid, (30.0, 30.0, -9.0), (30.0, 30.0, 12.0)));
    m.push(at(MoveKind::Rapid, (30.0, 30.0, 12.0), (0.0, 0.0, 12.0)));
    m.push(at(MoveKind::Plunge, (0.0, 0.0, 12.0), (0.0, 0.0, -18.0)));
    // The levels the dive entered for: the bottom level after it, and
    // a mid-level re-entry through the dive's own kerf.
    square(&mut m, 0.0, 0.0, 5.0, -18.0);
    m.push(at(MoveKind::Rapid, (0.0, 0.0, -18.0), (0.0, 0.0, 12.0)));
    m.push(at(MoveKind::Plunge, (0.0, 0.0, 12.0), (0.0, 0.0, -13.5)));
    square(&mut m, 0.0, 0.0, 5.0, -13.5);
    m
}

/// The dive index of [`tabbed_program`].
const DIVE: usize = 14;

#[test]
fn a_feed_plunge_deeper_than_one_stepdown_below_uncut_stock_is_named() {
    let w = judge(tabbed_program(), Some(STEP));
    assert_eq!(w, vec![Warning::PlungeTooDeep { at: DIVE, mm: 18.0, limit_mm: STEP }], "{w:?}");
}

#[test]
fn no_stated_stepdown_keeps_every_verdict_as_it_was() {
    let w = judge(tabbed_program(), None);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_dive_exactly_one_stepdown_below_the_uncut_top_is_clean() {
    let w = judge(
        vec![
            at(MoveKind::Rapid, (0.0, 0.0, 25.0), (0.0, 0.0, 5.0)),
            at(MoveKind::Plunge, (0.0, 0.0, 5.0), (0.0, 0.0, -4.5)),
        ],
        Some(STEP),
    );
    assert!(w.is_empty(), "{w:?}");
}

/// A ring at Z -9 whose pass swept the dive's whole disc (the square
/// about the origin, half-width 2, encloses the disc of radius 3 with
/// every sample within reach of its edges), entered one stepdown at a
/// time. The dive below it is judged against ITS floor.
fn ring_above_dive() -> Vec<Move> {
    let mut m = Vec::new();
    m.push(at(MoveKind::Rapid, (0.0, 0.0, 25.0), (-2.0, -2.0, 5.0)));
    m.push(at(MoveKind::Plunge, (-2.0, -2.0, 5.0), (-2.0, -2.0, -4.5)));
    square(&mut m, 0.0, 0.0, 2.0, -4.5);
    m.push(at(MoveKind::Rapid, (-2.0, -2.0, -4.5), (-2.0, -2.0, 5.0)));
    m.push(at(MoveKind::Plunge, (-2.0, -2.0, 5.0), (-2.0, -2.0, -9.0)));
    square(&mut m, 0.0, 0.0, 2.0, -9.0);
    m
}

#[test]
fn a_ring_entry_dive_below_the_floor_the_levels_left_is_clean() {
    // The reporter's workaround shape (#3678): each dive 2.7 mm below
    // the floor the level above left under the whole disc.
    let mut m = ring_above_dive();
    m.push(at(MoveKind::Rapid, (-2.0, -2.0, -9.0), (0.0, 0.0, 12.0)));
    m.push(at(MoveKind::Plunge, (0.0, 0.0, 12.0), (0.0, 0.0, -11.7)));
    let w = judge(m, Some(STEP));
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_dive_just_past_one_stepdown_below_the_floor_is_named() {
    let mut m = ring_above_dive();
    m.push(at(MoveKind::Rapid, (-2.0, -2.0, -9.0), (0.0, 0.0, 12.0)));
    m.push(at(MoveKind::Plunge, (0.0, 0.0, 12.0), (0.0, 0.0, -13.6)));
    let w = judge(m, Some(STEP));
    match w.as_slice() {
        [Warning::PlungeTooDeep { at: 13, mm, limit_mm }] => {
            assert!((mm - 4.6).abs() < 1e-9, "the depth below the floor reads 4.6, not {mm}");
            assert!((*limit_mm - STEP).abs() < 1e-12);
        }
        other => panic!("one finding at the dive, not {other:?}"),
    }
}

#[test]
fn a_pecked_dive_of_one_stepdown_per_peck_is_clean() {
    let w = judge(
        vec![
            at(MoveKind::Rapid, (0.0, 0.0, 25.0), (0.0, 0.0, 5.0)),
            at(MoveKind::Plunge, (0.0, 0.0, 5.0), (0.0, 0.0, -4.5)),
            at(MoveKind::Rapid, (0.0, 0.0, -4.5), (0.0, 0.0, 5.0)),
            at(MoveKind::Rapid, (0.0, 0.0, 5.0), (0.0, 0.0, -4.4)),
            at(MoveKind::Plunge, (0.0, 0.0, -4.4), (0.0, 0.0, -9.0)),
            at(MoveKind::Rapid, (0.0, 0.0, -9.0), (0.0, 0.0, 5.0)),
            at(MoveKind::Rapid, (0.0, 0.0, 5.0), (0.0, 0.0, -8.9)),
            at(MoveKind::Plunge, (0.0, 0.0, -8.9), (0.0, 0.0, -13.5)),
            at(MoveKind::Rapid, (0.0, 0.0, -13.5), (0.0, 0.0, 5.0)),
            at(MoveKind::Rapid, (0.0, 0.0, 5.0), (0.0, 0.0, -13.4)),
            at(MoveKind::Plunge, (0.0, 0.0, -13.4), (0.0, 0.0, -18.0)),
        ],
        Some(STEP),
    );
    assert!(w.is_empty(), "{w:?}");
}

/// A feed that starts inside stock - a rapid broke the surface above
/// its start - answers for its own descent only: the material above
/// the start is the rapid law's to name (it does, and it holds the
/// post). The bite is the lesser of the start and the floor.
#[test]
fn a_feed_that_a_rapid_already_stood_inside_stock_answers_for_its_own_bite_only() {
    let w = judge(
        vec![
            at(MoveKind::Rapid, (0.0, 0.0, 25.0), (0.0, 0.0, -2.0)),
            at(MoveKind::Plunge, (0.0, 0.0, -2.0), (0.0, 0.0, -4.4)),
        ],
        Some(STEP),
    );
    // The rapid's own crime is named - the fixture is the buried start.
    assert!(w.contains(&Warning::RapidInStock { at: 0 }), "the rapid that broke the surface went unnamed: {w:?}");
    // The feed's bite is 2.4 mm, one stepdown of its own take.
    assert!(!w.iter().any(|x| matches!(x, Warning::PlungeTooDeep { .. })), "{w:?}");
}

#[test]
fn a_feed_plunge_buried_deeper_than_one_stepdown_is_named_for_its_own_bite() {
    let w = judge(
        vec![
            at(MoveKind::Rapid, (0.0, 0.0, 25.0), (0.0, 0.0, -2.0)),
            at(MoveKind::Plunge, (0.0, 0.0, -2.0), (0.0, 0.0, -8.0)),
        ],
        Some(STEP),
    );
    let bites: Vec<(usize, f64, f64)> = w
        .iter()
        .filter_map(|x| match x {
            Warning::PlungeTooDeep { at, mm, limit_mm } => Some((*at, *mm, *limit_mm)),
            _ => None,
        })
        .collect();
    match bites.as_slice() {
        [(1, mm, limit_mm)] => {
            // The bite is 6.0 mm (Z -2 to Z -8), not the 8 mm column.
            assert!((mm - 6.0).abs() < 1e-9, "the bite reads 6.0 mm, not {mm}");
            assert!((limit_mm - STEP).abs() < 1e-12);
        }
        other => panic!("one finding for the feed's own bite, not {other:?} in {w:?}"),
    }
}

#[test]
fn a_non_finite_stepdown_fails_closed_on_a_dive_in_stock() {
    let w = judge(
        vec![
            at(MoveKind::Rapid, (0.0, 0.0, 25.0), (0.0, 0.0, 5.0)),
            at(MoveKind::Plunge, (0.0, 0.0, 5.0), (0.0, 0.0, -18.0)),
        ],
        Some(f64::NAN),
    );
    // A limit that cannot be read is no acquittal: the dive is named.
    match w.as_slice() {
        [Warning::PlungeTooDeep { at: 1, mm, .. }] => assert!((mm - 18.0).abs() < 1e-9, "{mm}"),
        other => panic!("the unreadable limit convicts, not {other:?}"),
    }
}

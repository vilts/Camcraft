//! #1595: the mark law, unit by unit - the budget's arithmetic, and the
//! solid law read against the part lowered by it. Our own fixtures.

use super::*;
use crate::cl::{Move, MoveKind};
use crate::geometry::cube;
use crate::tools::ToolGeom;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 6 mm flat end mill, 20 mm of flute.
fn cutter() -> verify::Cutter {
    verify::Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 3.0,
        shank_len: 30.0,
        holder_radius: 15.0,
        holder_len: 40.0,
    })
}

/// A 40 x 40 x 10 block with a 10 x 40 x 5 boss standing on it along
/// x 30..40.
fn stepped() -> Vec<Triangle> {
    let mut f = cube(p(0.0, 0.0, 0.0), p(40.0, 40.0, 10.0)).facets.to_vec();
    f.extend(cube(p(30.0, 0.0, 10.0), p(40.0, 40.0, 15.0)).facets.iter().copied());
    f
}

/// One feed along y at `x`, the tip at `z`.
fn line(x: f64, z: f64) -> Toolpath {
    Toolpath::new(vec![Move::new(MoveKind::Feed, p(x, 10.0, z), p(x, 30.0, z), Some(500.0))])
}

fn hits(path: &Toolpath, budget: f64) -> Vec<Warning> {
    let mut w = Vec::new();
    mark_into(&mut w, path, &stepped(), &cutter(), budget, 0.0);
    w
}

#[test]
fn the_budget_is_the_lesser_of_the_stated_depth_and_the_cutter_radius() {
    assert_eq!(budget(8.0, 7.7, 3.0), 8.0 - 7.7);
    assert_eq!(budget(8.0, 1.0, 3.0), 3.0, "never deeper than half the kerf is wide");
    assert_eq!(budget(8.0, 5.0, 3.0), 3.0);
    assert_eq!(budget(7.0, 8.0, 3.0), 0.0, "a Bottom over the Top states no depth");
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(budget(bad, 1.0, 3.0), 0.0);
        assert_eq!(budget(8.0, bad, 3.0), 0.0);
        assert_eq!(budget(8.0, 1.0, bad), 0.0);
    }
    assert_eq!(budget(8.0, 1.0, -3.0), 0.0);
}

#[test]
fn only_engrave_and_trace_are_mark_kinds() {
    use crate::operation::{seed_params, OpType};
    for t in [OpType::Engrave, OpType::Trace] {
        assert!(is_mark_kind(&seed_params(t.ordinal())), "{t:?}");
    }
    for t in [OpType::Chamfer2D, OpType::Pocket, OpType::Contour, OpType::Slot, OpType::Circular, OpType::Drill] {
        assert!(!is_mark_kind(&seed_params(t.ordinal())), "{t:?}");
    }
}

#[test]
fn a_cut_within_the_budget_under_the_surface_is_a_mark() {
    assert_eq!(hits(&line(15.0, 9.7), 0.3), Vec::new());
    assert_eq!(hits(&line(15.0, 7.0), 3.0), Vec::new());
}

#[test]
fn a_cut_past_the_budget_is_a_gouge_on_its_move() {
    assert_eq!(hits(&line(15.0, 9.6), 0.3), vec![Warning::Gouge { at: 0 }]);
    assert_eq!(hits(&line(15.0, 9.7), 0.0), vec![Warning::Gouge { at: 0 }], "no budget, no entry");
    assert_eq!(hits(&line(15.0, 9.7), f64::NAN), vec![Warning::Gouge { at: 0 }], "an unread budget is none");
}

#[test]
fn a_wall_beside_the_line_is_the_part_not_the_mark() {
    // 0.3 mm into the low face, the centre on the boss's foot (x 30):
    // the flank stands 3 mm inside a 5 mm wall.
    assert_eq!(hits(&line(30.0, 9.7), 0.3), vec![Warning::Gouge { at: 0 }]);
    // A step no higher than the budget is within the mark.
    assert_eq!(hits(&line(30.0, 9.7), 5.3), Vec::new());
    // Clear of the wall by the radius: the same cut is a mark.
    assert_eq!(hits(&line(26.9, 9.7), 0.3), Vec::new());
}

#[test]
fn a_conviction_another_law_already_made_is_not_repeated() {
    let mut w = vec![Warning::Gouge { at: 0 }];
    let found = mark_into(&mut w, &line(15.0, 9.0), &stepped(), &cutter(), 0.3, 0.0);
    assert_eq!(found, Marked { deep: 1, through: 0, stack: 0 }, "found, and not said twice");
    assert_eq!(w, vec![Warning::Gouge { at: 0 }]);
}

/// A 40 x 40 x 2 plate, Z 0..2 (the review's thin-plate fixture).
fn plate() -> Vec<Triangle> {
    cube(p(0.0, 0.0, 0.0), p(40.0, 40.0, 2.0)).facets.to_vec()
}

fn found_on(part: &[Triangle], path: &Toolpath, c: &verify::Cutter, budget: f64) -> (Marked, Vec<Warning>) {
    let mut w = Vec::new();
    let found = mark_into(&mut w, path, part, c, budget, 0.0);
    (found, w)
}

#[test]
fn a_mark_is_bounded_by_the_part_under_the_cutter() {
    // The review's cut: Top 2, Bottom -0.9, a 2.9 mm budget on a 2 mm
    // plate. The lowered plate (Z -2.9..-0.9) is clear of the cutter;
    // the floor on its underside is not.
    let (found, w) = found_on(&plate(), &line(20.0, -0.9), &cutter(), 2.9);
    assert_eq!(found, Marked { deep: 0, through: 1, stack: 0 });
    assert_eq!(w, vec![Warning::Gouge { at: 0 }], "a through-cut is a gouge on its move");
    // Along the plate's edge, half the cutter over air: the same.
    assert_eq!(found_on(&plate(), &line(0.0, -0.9), &cutter(), 2.9).0.through, 1);
    // Down to the floor and no further: 1.8 deep leaves MARK_FLOOR.
    assert_eq!(found_on(&plate(), &line(20.0, MARK_FLOOR), &cutter(), 2.9), (Marked::default(), Vec::new()));
    assert_eq!(found_on(&plate(), &line(20.0, 1.7), &cutter(), 0.3), (Marked::default(), Vec::new()), "a shallow mark on a thin plate");
    // 0.1 mm of plate left is a breakthrough, whatever the budget says.
    assert_eq!(found_on(&plate(), &line(20.0, 0.1), &cutter(), 2.9).0, Marked { deep: 0, through: 1, stack: 0 });
    // Past the budget AND through: both readings speak, one warning.
    let (found, w) = found_on(&plate(), &line(20.0, -0.9), &cutter(), 0.3);
    assert_eq!((found, w.len()), (Marked { deep: 1, through: 1, stack: 0 }, 1));
}

#[test]
fn a_mark_over_a_void_may_not_break_into_it() {
    // A bridge: two feet and a 2 mm deck (Z 6..8) over a tunnel along Y.
    let mut part = cube(p(0.0, 0.0, 0.0), p(10.0, 40.0, 6.0)).facets.to_vec();
    part.extend(cube(p(30.0, 0.0, 0.0), p(40.0, 40.0, 6.0)).facets.iter().copied());
    part.extend(cube(p(0.0, 0.0, 6.0), p(40.0, 40.0, 8.0)).facets.iter().copied());
    // 1.9 mm into the deck over the tunnel: 0.1 mm of roof left.
    assert_eq!(found_on(&part, &line(20.0, 6.1), &cutter(), 3.0).0.through, 1);
    // 1.5 mm in leaves 0.5 mm of roof: a mark.
    assert_eq!(found_on(&part, &line(20.0, 6.5), &cutter(), 3.0), (Marked::default(), Vec::new()));
}

/// The 6 mm end mill with only 2 mm of flute under its shank.
fn stubby() -> verify::Cutter {
    verify::Cutter { flute_len: 2.0, ..cutter() }
}

#[test]
fn the_shank_is_judged_against_the_part_as_it_stands() {
    // 3 mm into the low face on a 3 mm budget, 2 mm of flute: the edge
    // is within its mark, the shank stands 1 mm inside the part. Lowered
    // by the budget the part would have excused it.
    let (found, w) = found_on(&stepped(), &line(15.0, 7.0), &stubby(), 3.0);
    assert_eq!(found, Marked { deep: 0, through: 0, stack: 1 });
    assert_eq!(w, vec![Warning::ShankHit { at: 0, part: true }]);
    // Within the flute the same tool marks clean.
    assert_eq!(found_on(&stepped(), &line(15.0, 8.5), &stubby(), 3.0), (Marked::default(), Vec::new()));
    // The boss beside the line, its top (Z 15) within the budget of the
    // shank's foot: 0.3 mm into the low face clear of the wall by the
    // cutter radius, a 5 mm shank radius reaching 2 mm over the boss
    // at Z 11.7 - inside it.
    let fat = verify::Cutter { shank_radius: 5.0, ..stubby() };
    let (found, w) = found_on(&stepped(), &line(27.0, 9.7), &fat, 0.3);
    assert_eq!((found.stack, w), (1, vec![Warning::ShankHit { at: 0, part: true }]));
}

#[test]
fn a_rapid_has_no_mark_to_make() {
    // A rapid 0.2 mm under the surface, inside a 0.3 mm budget.
    let rapid = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(15.0, 10.0, 9.8), p(15.0, 30.0, 9.8), None)]);
    let (found, w) = found_on(&stepped(), &rapid, &cutter(), 0.3);
    assert_eq!((found, w), (Marked { deep: 0, through: 0, stack: 1 }, vec![Warning::RapidInStock { at: 0 }]));
    // On the surface it touches nothing.
    let over = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(15.0, 10.0, 10.0), p(15.0, 30.0, 10.0), None)]);
    assert_eq!(found_on(&stepped(), &over, &cutter(), 0.3), (Marked::default(), Vec::new()));
}

#[test]
fn only_the_retract_up_the_marks_own_kerf_is_a_rapid_under_the_surface() {
    let feed = Move::new(MoveKind::Feed, p(15.0, 10.0, 9.7), p(15.0, 30.0, 9.7), Some(500.0));
    let rapid = |a: Point3, b: Point3| Move::new(MoveKind::Rapid, a, b, None);
    // Straight up from where the cut ended: the kerf it made.
    let lift = Toolpath::new(vec![feed, rapid(p(15.0, 30.0, 9.7), p(15.0, 30.0, 25.0))]);
    assert_eq!(found_on(&stepped(), &lift, &cutter(), 0.3), (Marked::default(), Vec::new()));
    // Sideways out of the kerf, down into the part, or up from a place
    // no cut left the tool: each is a rapid in the part.
    let hit = |at: usize| (Marked { deep: 0, through: 0, stack: 1 }, vec![Warning::RapidInStock { at }]);
    let side = Toolpath::new(vec![feed, rapid(p(15.0, 30.0, 9.7), p(20.0, 30.0, 9.7))]);
    assert_eq!(found_on(&stepped(), &side, &cutter(), 0.3), hit(1));
    let down = Toolpath::new(vec![feed, rapid(p(15.0, 30.0, 9.7), p(15.0, 30.0, 9.5))]);
    assert_eq!(found_on(&stepped(), &down, &cutter(), 0.3), hit(1));
    let elsewhere = Toolpath::new(vec![feed, rapid(p(15.0, 20.0, 9.2), p(15.0, 20.0, 25.0))]);
    assert_eq!(found_on(&stepped(), &elsewhere, &cutter(), 0.3), hit(1));
}

#[test]
fn the_retract_out_of_the_kerf_is_judged_past_where_the_cut_left_the_tool() {
    // Review of PR #1615: an open line's end under an overhang of the
    // part at Z 105..110 - over the holder's top (Z 99.7) while the
    // line is cut, in the holder's way as it retracts to Z 130.
    let feed = Move::new(MoveKind::Feed, p(15.0, 10.0, 9.7), p(15.0, 30.0, 9.7), Some(500.0));
    let lift = Toolpath::new(vec![feed, Move::new(MoveKind::Rapid, p(15.0, 30.0, 9.7), p(15.0, 30.0, 130.0), None)]);
    let mut roofed = stepped();
    roofed.extend(cube(p(5.0, 22.0, 105.0), p(25.0, 38.0, 110.0)).facets.iter().copied());
    assert_eq!(found_on(&roofed, &lift, &cutter(), 0.3), (Marked { deep: 0, through: 0, stack: 1 }, vec![Warning::RapidInStock { at: 1 }]));
    // No overhang: the same retract is the kerf's own, and clean.
    assert_eq!(found_on(&stepped(), &lift, &cutter(), 0.3), (Marked::default(), Vec::new()));
}

#[test]
fn the_notes_name_the_budget_and_the_floor_each_for_its_own_finding() {
    let u = Unit::default();
    assert!(notes("Trace", Marked::default(), 3.0, u).is_empty());
    assert!(notes("Trace", Marked { stack: 2, ..Marked::default() }, 3.0, u).is_empty(), "a shank hit says itself");
    assert_eq!(notes("Trace", Marked { deep: 1, ..Marked::default() }, 3.0, u), vec![note("Trace", 3.0, u)]);
    let through = notes("Trace", Marked { through: 1, ..Marked::default() }, 3.0, u);
    assert_eq!(through, vec![through_note("Trace", u)]);
    assert!(through[0].starts_with("Trace cuts through the part") && through[0].contains("0.200 mm"), "{through:?}");
    assert_eq!(notes("Trace", Marked { deep: 1, through: 1, stack: 0 }, 3.0, u).len(), 2);
}

#[test]
fn the_note_names_the_budget_in_the_documents_unit() {
    let n = note("Engrave", 3.0, Unit::default());
    assert!(n.starts_with("Engrave cuts deeper than a mark") && n.contains("3.000 mm") && n.contains("model the groove"), "{n}");
}

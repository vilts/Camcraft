//! #1595, review of PR #1615 (pullrequestreview-5901443518): the retract
//! out of a mark's own kerf is judged. `verify::stack_hits` excuses, on
//! a straight rapid up the tool axis from where a cutting move ended,
//! only the pose the cutting edge already stands in - the kerf that
//! move made, judged on that move by the mark law. Everything the lift
//! enters beyond it is swept against the part as it stands: the shank
//! and holder over the whole lift, and the column the edge rises
//! through above its own top. A part overhang above the end of an open
//! engraving line is a `RapidInStock` on the lift, not a pass.
//!
//! Provenance: our own fixtures (plain boxes) over the solid law; no
//! vendor material consulted.
//!
//! Gate map (#1595): covers camcraft_sim::verify::solid (stack_hits).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::verify::{stack_hits, Cutter, SolidLaw, Warning};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 6 mm flat end mill: 20 mm of flute, a 6 mm shank 30 mm long and a
/// 30 mm holder 40 mm long - 90 mm from the tip to the holder's top.
fn cutter() -> Cutter {
    Cutter {
        radius: 3.0,
        corner_r: 0.0,
        tip_angle: 0.0,
        flute_len: 20.0,
        shank_radius: 3.0,
        shank_len: 30.0,
        holder_radius: 15.0,
        holder_len: 40.0,
        body: None,
        neck: None,
    }
}

/// The finished part: a 40 x 40 x 10 block, plus `more`.
fn part(more: &[(Point3, Point3)]) -> Vec<Triangle> {
    let mut f = cube(p(0.0, 0.0, 0.0), p(40.0, 40.0, 10.0)).facets.to_vec();
    for &(lo, hi) in more {
        f.extend(cube(lo, hi).facets.iter().copied());
    }
    f
}

/// An open engraved line along y at x 15 with the tip at `z`, then the
/// straight retract from its end (15, 30) up to `lift_to`.
fn line_then_lift(z: f64, lift_to: f64) -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Feed, p(15.0, 10.0, z), p(15.0, 30.0, z), Some(500.0)),
        Move::new(MoveKind::Rapid, p(15.0, 30.0, z), p(15.0, 30.0, lift_to), None),
    ])
}

fn hits(path: &Toolpath, facets: &[Triangle], c: &Cutter) -> Vec<Warning> {
    stack_hits(path, facets, c, &SolidLaw { step: 0.25, allow: 0.01, hole_walls: false })
}

#[test]
fn a_holder_lifting_into_an_overhang_above_the_lines_end_is_a_rapid_in_the_part() {
    // The mark is 0.3 mm deep; the holder's top stands at Z 99.7 while
    // it is cut, so a slab at Z 105..110 is over the whole assembly and
    // no cutting move reaches it. The retract to Z 130 carries the
    // holder through it.
    let over_end = part(&[(p(5.0, 22.0, 105.0), p(25.0, 38.0, 110.0))]);
    assert_eq!(hits(&line_then_lift(9.7, 130.0), &over_end, &cutter()), vec![Warning::RapidInStock { at: 1 }]);
    // A lift that stops under the slab touches nothing.
    assert_eq!(hits(&line_then_lift(9.7, 14.0), &over_end, &cutter()), Vec::new());
    // The same slab over the line's START (18 mm from the end, past the
    // holder's 15 mm radius) is not in the retract's way.
    let over_start = part(&[(p(5.0, 2.0, 105.0), p(25.0, 12.0, 110.0))]);
    assert_eq!(hits(&line_then_lift(9.7, 130.0), &over_start, &cutter()), Vec::new());
}

#[test]
fn the_edge_is_excused_only_where_it_already_stands() {
    // A cutter that is all flute: no shank or holder band to speak for
    // the lift. The edge rises out of its kerf into a slab at Z 40..45.
    let bare = Cutter { shank_len: 0.0, holder_len: 0.0, ..cutter() };
    let roofed = part(&[(p(5.0, 22.0, 40.0), p(25.0, 38.0, 45.0))]);
    assert_eq!(hits(&line_then_lift(9.7, 60.0), &roofed, &bare), vec![Warning::RapidInStock { at: 1 }]);
    assert_eq!(hits(&line_then_lift(9.7, 15.0), &roofed, &bare), Vec::new(), "stopped under the slab");
    // A shank narrower than the cutter: a lip 2.5 mm from the axis just
    // above the edge's top (Z 29.7) clears the 2 mm shank on the cut
    // and on the lift, and is struck by the 3 mm edge as it rises.
    let necked = Cutter { shank_radius: 2.0, ..cutter() };
    let lipped = part(&[(p(17.5, 25.0, 31.0), p(25.0, 35.0, 33.0))]);
    assert_eq!(hits(&line_then_lift(9.7, 130.0), &lipped, &necked), vec![Warning::RapidInStock { at: 1 }]);
    // The same lip clear of the edge by its radius: nothing.
    let clear = part(&[(p(18.2, 25.0, 31.0), p(25.0, 35.0, 33.0))]);
    assert_eq!(hits(&line_then_lift(9.7, 130.0), &clear, &necked), Vec::new());
}

#[test]
fn the_retract_out_of_the_kerf_itself_stays_clean() {
    let block = part(&[]);
    for c in [cutter(), Cutter { shank_len: 0.0, holder_len: 0.0, ..cutter() }, Cutter { shank_radius: 2.0, ..cutter() }] {
        // 0.3 mm and 3 mm deep (the edge stands in the part, in its own
        // kerf), lifted clear, and lifted by less than the kerf is deep.
        for (z, to) in [(9.7, 130.0), (7.0, 130.0), (7.0, 8.0), (7.0, 7.0)] {
            assert_eq!(hits(&line_then_lift(z, to), &block, &c), Vec::new(), "tip {z} lifted to {to}: {c:?}");
        }
    }
    // A ball's flank widens as it rises, but never past where it stood.
    let ball = Cutter { corner_r: 3.0, ..cutter() };
    assert_eq!(hits(&line_then_lift(7.0, 130.0), &block, &ball), Vec::new());
}

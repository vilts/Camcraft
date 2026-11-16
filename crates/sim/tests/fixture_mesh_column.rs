//! #1530: the steel a closed fixture mesh puts on one vertical line -
//! the carried-workholding gate's per-column reading. A block reads one
//! span, a miss reads none, a nest reads its floor in the pocket and its
//! floor and wall beside it (shells sharing a face merge), and a line on
//! a facet diagonal or in a wall face is nudged off it and still read.
//! Provenance: our own geometry.
//!
//! The #1530 review added: an inside-out shell over another is never
//! read as air, a far-off line is nudged by a capped step, and a box
//! read WHOLE (`steel_in_prism`) sees a post narrower than any line
//! pitch.
//!
//! Gate map (#1530): covers sim::fixture_mesh::column (steel_on_line),
//! sim::fixture_mesh::prism (steel_in_prism, PrismReading).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_sim::fixture_mesh::{FixtureMesh, PrismReading};
use camcraft_sim::geometry::{Point3, Triangle, cube};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn mesh(boxes: &[(Point3, Point3)]) -> FixtureMesh {
    let tris: Vec<Triangle> = boxes.iter().flat_map(|(a, b)| cube(*a, *b).facets.iter().copied().collect::<Vec<_>>()).collect();
    FixtureMesh::build(&tris).expect("closed")
}

/// One line read with an unbounded budget.
fn read(m: &FixtureMesh, x: f64, y: f64) -> Option<Vec<(f64, f64)>> {
    let mut budget = u64::MAX;
    m.steel_on_line(x, y, &mut budget)
}

fn near(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x.0 - y.0).abs() < 1e-9 && (x.1 - y.1).abs() < 1e-9)
}

/// An 80 x 80 slab Z -10..0 with walls Z 0..5 round a 50 x 50 pocket.
fn nest() -> FixtureMesh {
    mesh(&[
        (p(-40.0, -40.0, -10.0), p(40.0, 40.0, 0.0)),
        (p(-40.0, -40.0, 0.0), p(-25.0, 40.0, 5.0)),
        (p(25.0, -40.0, 0.0), p(40.0, 40.0, 5.0)),
        (p(-25.0, -40.0, 0.0), p(25.0, -25.0, 5.0)),
        (p(-25.0, 25.0, 0.0), p(25.0, 40.0, 5.0)),
    ])
}

#[test]
fn a_block_reads_its_span_and_a_miss_reads_none() {
    let m = mesh(&[(p(0.0, 0.0, 2.0), p(10.0, 10.0, 7.0))]);
    let s = read(&m, 3.3, 6.1).expect("read");
    assert!(near(&s, &[(2.0, 7.0)]), "{s:?}");
    assert_eq!(read(&m, 12.0, 6.1), Some(vec![]));
}

#[test]
fn a_line_on_a_facet_diagonal_is_nudged_off_it_and_read() {
    // A centred square's top and bottom faces are split on x == y.
    let m = mesh(&[(p(-40.0, -40.0, -10.0), p(40.0, 40.0, 0.0))]);
    for q in [0.0, 12.5, -33.75] {
        let s = read(&m, q, q).expect("nudged and read");
        assert!(near(&s, &[(-10.0, 0.0)]), "{q}: {s:?}");
    }
}

#[test]
fn a_nest_reads_its_floor_in_the_pocket_and_floor_and_wall_beside_it() {
    let m = nest();
    let s = read(&m, 1.1, -2.3).expect("pocket");
    assert!(near(&s, &[(-10.0, 0.0)]), "{s:?}");
    let s = read(&m, 30.2, 1.7).expect("wall");
    assert!(near(&s, &[(-10.0, 5.0)]), "shells sharing a face read as one: {s:?}");
}

#[test]
fn a_line_in_a_wall_face_is_still_read() {
    let m = nest();
    let s = read(&m, 25.0, 3.1).expect("nudged off the face");
    assert!(near(&s, &[(-10.0, 0.0)]) || near(&s, &[(-10.0, 5.0)]), "{s:?}");
}

#[test]
fn a_spent_budget_reads_nothing_and_never_guesses() {
    let m = nest();
    let mut budget = 3u64;
    assert_eq!(m.steel_on_line(30.2, 1.7, &mut budget), None, "three facets cannot read a wall");
    assert_eq!(budget, 0);
    let mut budget = 10_000u64;
    assert!(m.steel_on_line(30.2, 1.7, &mut budget).is_some());
    assert!(budget < 10_000, "each facet visited is spent");
}

#[test]
fn a_non_finite_line_is_not_read() {
    assert_eq!(read(&nest(), f64::NAN, 0.0), None);
}

/// Two closed shells: A at Z 0..10 and B at Z 5..15 with its facets
/// reversed (an imported multi-body mesh with one body flipped). The
/// edges pair, so the mesh is "closed"; the winding goes negative on
/// any line through both.
fn a_over_inside_out_b() -> FixtureMesh {
    let a = cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 10.0));
    let b = cube(p(0.0, 0.0, 5.0), p(10.0, 10.0, 15.0));
    let mut tris: Vec<Triangle> = a.facets.iter().copied().collect();
    tris.extend(b.facets.iter().map(|t| Triangle { a: t.a, b: t.c, c: t.b }));
    FixtureMesh::build(&tris).expect("edges pair")
}

#[test]
fn an_inside_out_shell_over_another_is_not_read_as_air() {
    let m = a_over_inside_out_b();
    assert_eq!(read(&m, 3.3, 6.1), None, "the steel at 5..10 must not read as air");
    let mut budget = u64::MAX;
    let r = m.steel_in_prism((2.0, 4.0), (5.0, 7.0), (6.0, 9.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Unread, "a box wholly in the overlap keeps the caller's box");
}

#[test]
fn a_far_off_block_is_read_where_it_stands() {
    // A 6 micron block at X 5000; the line runs on its top face's
    // diagonal, so it is nudged - by at most 0.56 micron, never off it.
    let h = 0.003;
    let m = mesh(&[(p(5000.0 - h, -h, 0.0), p(5000.0 + h, h, 1.0))]);
    let s = read(&m, 5000.0, 0.0).expect("nudged and read");
    assert!(near(&s, &[(0.0, 1.0)]), "{s:?}");
}

/// A 20 x 20 plate at Z -5..0 with a 0.19 x 0.19 post Z 0..3 at (5, 5).
fn plate_with_post() -> FixtureMesh {
    let r = 0.095;
    mesh(&[(p(0.0, 0.0, -5.0), p(20.0, 20.0, 0.0)), (p(5.0 - r, 5.0 - r, 0.0), p(5.0 + r, 5.0 + r, 3.0))])
}

#[test]
fn a_box_read_whole_sees_a_post_no_line_meets() {
    let m = plate_with_post();
    let mut budget = u64::MAX;
    // The corners' lines (4.6 / 5.4) all miss the post.
    for (x, y) in [(4.6, 4.6), (5.4, 4.6), (4.6, 5.4), (5.4, 5.4)] {
        let s = read(&m, x, y).expect("read");
        assert!(near(&s, &[(-5.0, 0.0)]), "{s:?}");
    }
    let r = m.steel_in_prism((4.61, 5.39), (4.61, 5.39), (0.0, 10.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Steel(3.0), "the post stands 3 mm into the material");
    let r = m.steel_in_prism((6.0, 7.0), (6.0, 7.0), (0.0, 10.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Clear, "beside the post only the plate, in contact at Z 0");
    let r = m.steel_in_prism((4.61, 5.39), (4.61, 5.39), (3.0 - 0.005, 10.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Clear, "the post's top within the contact band");
}

#[test]
fn a_box_wholly_inside_the_steel_is_steel() {
    // No facet in the slab: one line decides, and it reads steel.
    let m = mesh(&[(p(0.0, 0.0, -5.0), p(20.0, 20.0, 20.0))]);
    let mut budget = u64::MAX;
    let r = m.steel_in_prism((4.0, 6.0), (4.0, 6.0), (0.0, 10.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Steel(10.0));
    let r = m.steel_in_prism((24.0, 26.0), (4.0, 6.0), (0.0, 10.0), 0.01, &mut budget);
    assert_eq!(r, PrismReading::Clear, "no facet over the box: no steel");
}

#[test]
fn a_box_read_past_its_budget_or_on_bad_input_is_unread() {
    let m = plate_with_post();
    let mut budget = 0u64;
    assert_eq!(m.steel_in_prism((4.61, 5.39), (4.61, 5.39), (0.0, 10.0), 0.01, &mut budget), PrismReading::Unread);
    let mut budget = u64::MAX;
    assert_eq!(m.steel_in_prism((f64::NAN, 5.0), (4.0, 5.0), (0.0, 10.0), 0.01, &mut budget), PrismReading::Unread);
    assert_eq!(m.steel_in_prism((5.0, 4.0), (4.0, 5.0), (0.0, 10.0), 0.01, &mut budget), PrismReading::Unread);
}

#[test]
fn a_budget_spent_mid_line_never_reads_part_of_it() {
    // Two blocks stacked on one line: a walk cut short after the first
    // block's two crossings would read a whole-looking line. Every
    // budget either reads the full line or nothing.
    let m = mesh(&[(p(0.0, 0.0, 0.0), p(10.0, 10.0, 1.0)), (p(0.0, 0.0, 5.0), p(10.0, 10.0, 6.0))]);
    let full = read(&m, 3.3, 6.1).expect("read");
    assert!(near(&full, &[(0.0, 1.0), (5.0, 6.0)]), "{full:?}");
    for b in 0..64u64 {
        let mut budget = b;
        let r = m.steel_on_line(3.3, 6.1, &mut budget);
        assert!(r.is_none() || r.as_deref().is_some_and(|s| near(s, &full)), "budget {b}: {r:?}");
    }
}

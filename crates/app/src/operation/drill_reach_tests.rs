//! Unit tests of drill_reach (issue #84): the hull and swept-holder
//! distances, the holder field guard, the disk and band readers.
//! Provenance: textbook planar geometry; our own design notes.

use super::*;

#[test]
fn hull_distance_is_zero_inside_and_the_edge_gap_outside() {
    let mut sq = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    assert_eq!(hull_dist(&mut sq), 0.0);
    let mut off = [(2.0, -1.0), (3.0, -1.0), (3.0, 1.0), (2.0, 1.0)];
    assert!((hull_dist(&mut off) - 2.0).abs() < 1e-12);
}

#[test]
fn a_holder_swept_across_the_spindle_reaches_it() {
    // Tip from x=-25 to x=25, axis +X, holder face 22 behind: it
    // starts at x=-3, over the spindle.
    let d = holder_approach(Point3::new(-25.0, 0.0, 0.0), Point3::new(25.0, 0.0, 0.0), [1.0, 0.0, 0.0], 22.0, 35.0, 22.0);
    assert_eq!(d, 0.0);
    // A tip parked at x=25 with the holder outward: face at x=47.
    let p = Point3::new(25.0, 0.0, 0.0);
    let d = holder_approach(p, p, [1.0, 0.0, 0.0], 22.0, 35.0, 22.0);
    assert!((d - 47.0).abs() < 1e-9, "{d}");
}

#[test]
fn a_non_finite_holder_is_refused_not_read_as_no_holder() {
    let base = crate::tools::catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t);
    let Some(base) = base else { panic!("catalog DR-0400") };
    for (hd, hl) in [(f64::NAN, 35.0), (44.0, f64::INFINITY), (-1.0, 35.0)] {
        let t = Tool { holder_d: hd, holder_len: hl, ..base.clone() };
        let e = holder(&t, "T1").err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("T1's holder") && e.contains("not a usable size"), "({hd}, {hl}): {e}");
    }
    let none = Tool { holder_d: 0.0, ..base.clone() };
    assert!(matches!(holder(&none, "T1"), Ok(None)), "0 still means no holder");
    assert!(matches!(holder(&base, "T1"), Ok(Some(_))));
}

#[test]
fn a_collapsed_facet_sets_no_top_and_a_wall_meets_only_by_its_edges() {
    let p = Point3::new(50.0, 0.0, 9.0);
    assert!(!meets_disk(&Triangle::new(p, p, p), 0.0, 0.0, 1.0), "a point facet");
    let line = Triangle::new(Point3::new(-5.0, 0.0, 9.0), Point3::new(0.0, 0.0, 9.0), Point3::new(5.0, 0.0, 9.0));
    assert!(!meets_disk(&line, 0.0, 0.0, 1.0), "a line facet");
    // A wall in the plane y=0 through the disk's centre, 40 mm away:
    // its projection is a segment on the centre's line - not inside.
    let wall = Triangle::new(Point3::new(40.0, 0.0, 0.0), Point3::new(50.0, 0.0, 0.0), Point3::new(45.0, 0.0, 9.0));
    assert!(!meets_disk(&wall, 0.0, 0.0, 1.0), "a far wall");
    assert!(meets_disk(&wall, 40.5, 0.5, 1.0), "a wall the disk touches");
}

#[test]
fn the_band_radius_reads_only_material_inside_the_band() {
    // A flange's face: an annulus sliver at z-10 out to r30, and a
    // slanted facet from r10 at z0 to r20 at z10.
    let face = Triangle::new(Point3::new(10.0, 0.0, -10.0), Point3::new(30.0, 0.0, -10.0), Point3::new(30.0, 1.0, -10.0));
    let cone = Triangle::new(Point3::new(10.0, 0.0, 0.0), Point3::new(20.0, 0.0, 10.0), Point3::new(20.0, 1.0, 10.0));
    let f = [face, cone];
    assert_eq!(band_r(&f, (0.0, 0.0), 20.0, 40.0), None);
    let r = band_r(&f, (0.0, 0.0), -2.0, 5.0).unwrap_or(f64::NAN);
    assert!((r - 15.0_f64.hypot(0.5)).abs() < 1e-9, "the cone clipped at z5: {r}");
    let r = band_r(&f, (0.0, 0.0), -12.0, 5.0).unwrap_or(f64::NAN);
    assert!((r - 30.0_f64.hypot(1.0)).abs() < 1e-9, "the flange face: {r}");
}

#[test]
fn a_sloped_facet_is_read_over_the_disk_not_at_its_far_vertex() {
    // z = x / 10 over x 0..50: the disk r 2 at x 5 sees at most z 0.7
    // (the octagon's reach), never the far vertex's 5.0, never under
    // the true 0.7 at x 7.
    let t = Triangle::new(Point3::new(0.0, -50.0, 0.0), Point3::new(50.0, 0.0, 5.0), Point3::new(0.0, 50.0, 0.0));
    let z = facet_top(&t, 5.0, 0.0, 2.0);
    assert!((0.7 - 1e-9..=0.7 + 1e-3).contains(&z), "{z}");
    // A wall's top inside the disk is its top edge.
    let wall = Triangle::new(Point3::new(-5.0, 0.0, 0.0), Point3::new(5.0, 0.0, 0.0), Point3::new(5.0, 0.0, 9.0));
    let z = facet_top(&wall, 0.0, 0.0, 1.0);
    assert!((5.4 - 1e-9..=5.4 + 1e-3).contains(&z), "the wall's slanted top z 4.5 + 0.9x at x 1: {z}");
    let nan = Triangle::new(Point3::new(0.0, 0.0, f64::NAN), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0));
    assert_eq!(facet_top(&nan, 0.0, 0.0, 1.0), f64::INFINITY, "a NaN height refuses");
}

/// DR-0400 with its Ø44 holder `reach` behind the tip (flute 12, the
/// rest shank).
fn holder_tool(reach: f64) -> Tool {
    let base = crate::tools::catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t);
    let Some(base) = base else { panic!("catalog DR-0400") };
    Tool { flute_len: 12.0, shank_len: reach - 12.0, holder_d: 44.0, holder_len: 30.0, ..base }
}

/// A chain from `at` with the pose `axis`, through `to`.
fn chain(at: Point3, axis: [f64; 3], to: &[Point3]) -> Toolpath {
    let mut b = crate::cl::PathBuilder::new(at);
    b.set_axis(Some(axis));
    for &p in to {
        b.push(crate::cl::MoveKind::Rapid, p, None);
    }
    b.finish()
}

fn box60() -> Vec<Triangle> {
    crate::geometry::cube(Point3::new(-30.0, -30.0, -20.0), Point3::new(30.0, 30.0, 20.0)).facets.to_vec()
}

/// Issue #119: on one station the holder reads the box at its face.
/// A tip at x7 on the +X line with the face 30 behind it stands at x37:
/// outside the r30 face, inside the r42.43 corner circle - clear. With
/// 20 of reach (face at x27) the holder really meets the face: refused
/// by name, read along its line.
#[test]
fn an_on_station_holder_reads_the_box_at_its_face_not_its_corner_circle() {
    let (b, p) = (box60(), Point3::new(50.0, 0.0, 0.0));
    let t = chain(p, [1.0, 0.0, 0.0], &[Point3::new(7.0, 0.0, 0.0), p]);
    let r = cross_holder(&t, &[], Some(&b), (0.0, 0.0), &holder_tool(30.0), None, "T1");
    assert!(r.is_ok(), "the holder clears the face: {:?}", r.err());
    let e = cross_holder(&t, &[], Some(&b), (0.0, 0.0), &holder_tool(20.0), None, "T1").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("a holder face at x27 sank into the r30 box face"));
    for s in ["T1's holder", "27.000 mm out along its hole's line", "inside material 30.000 mm out on that line", "stands 27.000 mm out"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
    // A slide in Z on the station (a column, the holder's face at x32)
    // reads the same face.
    let col = chain(Point3::new(20.0, 0.0, 60.0), [1.0, 0.0, 0.0], &[Point3::new(20.0, 0.0, 0.0)]);
    assert!(cross_holder(&col, &[], Some(&b), (0.0, 0.0), &holder_tool(12.0), None, "T1").is_ok(), "the column stands off the face");
}

/// A swing round the bar keeps the corner radius: the quarter-turn
/// chord from (50, 0) to (0, 50) on the old +X pose passes the box
/// corner at r35.36 - inside its r42.43 corner circle - and is refused
/// at that radius, not read along a line.
#[test]
fn a_swing_round_the_bar_keeps_the_corner_radius() {
    let b = box60();
    let t = chain(Point3::new(50.0, 0.0, 0.0), [1.0, 0.0, 0.0], &[Point3::new(0.0, 50.0, 0.0)]);
    let e = cross_holder(&t, &[], Some(&b), (0.0, 0.0), &holder_tool(12.0), None, "T1").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("the swing's holder passed inside the box corners"));
    assert!(e.contains("T1's holder") && e.contains("inside the stock's radius 42.426 at Z"), "{e}");
}

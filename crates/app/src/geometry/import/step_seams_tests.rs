//! Unit pins for `step_seams` (#1573 review): a unit cube meshed face by
//! face, then bent into each seam fault.

use super::*;

const WELD: f64 = 1e-6;
const GAP: f64 = 1e-3;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The six faces of the unit cube, each a loop wound outward.
fn loops() -> Vec<Vec<Point3>> {
    vec![
        vec![p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)], // z = 0
        vec![p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)], // z = 1
        vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)], // y = 0
        vec![p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)], // y = 1
        vec![p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)], // x = 0
        vec![p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)], // x = 1
    ]
}

/// Fan each loop from its centroid into facets; `ends` per face.
fn mesh(loops: &[Vec<Point3>]) -> (Vec<Triangle>, Vec<usize>) {
    let mut tris = Vec::new();
    let mut ends = Vec::new();
    for l in loops {
        let n = l.len() as f64;
        let c = l.iter().fold(p(0., 0., 0.), |s, q| p(s.x + q.x / n, s.y + q.y / n, s.z + q.z / n));
        for i in 0..l.len() {
            tris.push(Triangle::new(c, l[i], l[(i + 1) % l.len()]));
        }
        ends.push(tris.len());
    }
    (tris, ends)
}

fn reversed(l: &[Vec<Point3>], face: usize) -> Vec<Vec<Point3>> {
    let mut l = l.to_vec();
    l[face].reverse();
    l
}

#[test]
fn a_cube_meshed_face_by_face_is_closed_and_untouched() {
    let (mut tris, ends) = mesh(&loops());
    let before = tris.clone();
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!((s.gaps, s.reversed_edges, s.t_junction_edges), (0, 0, 0));
    assert!(s.flipped.is_empty());
    assert_eq!(tris, before);
}

#[test]
fn a_face_wound_against_its_neighbours_is_flipped_not_called_open() {
    let (good, _) = mesh(&loops());
    let (mut tris, ends) = mesh(&reversed(&loops(), 3));
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!(s.reversed_edges, 4, "the face's four seams run the same way as its neighbours'");
    assert_eq!(s.gaps, 0, "a reversed face is not a hole");
    assert_eq!(s.flipped, vec![3]);
    // Flipping reverses each facet's corners; the cube is wound outward again.
    let again = close(&mut tris, &ends, WELD, GAP);
    assert_eq!((again.reversed_edges, again.flipped.len()), (0, 0));
    assert_eq!(tris.len(), good.len());
}

#[test]
fn the_flips_turn_the_shell_outward() {
    // Five faces reversed: they are the ones wound inward, however few
    // faces the outward side has.
    let mut l = loops();
    for f in [0, 1, 2, 3, 4] {
        l = reversed(&l, f);
    }
    let (mut tris, ends) = mesh(&l);
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!(s.flipped, vec![0, 1, 2, 3, 4]);
}

/// The top face with every edge split at its midpoint: none of its edges
/// matches a side face's edge exactly (T-junctions all round).
fn split_top(l: &mut [Vec<Point3>]) {
    let top = &l[1];
    let mut split = Vec::new();
    for i in 0..top.len() {
        let (a, b) = (top[i], top[(i + 1) % top.len()]);
        split.push(a);
        split.push(p((a.x + b.x) / 2., (a.y + b.y) / 2., (a.z + b.z) / 2.));
    }
    l[1] = split;
}

#[test]
fn a_boundary_split_differently_on_each_side_is_closed() {
    let mut l = loops();
    split_top(&mut l);
    let (mut tris, ends) = mesh(&l);
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!(s.gaps, 0, "T-junctions are not holes");
    assert_eq!(s.t_junction_edges, 12, "8 split top edges and the 4 side edges they cover");
    assert!(s.flipped.is_empty());
}

#[test]
fn a_reversed_face_met_only_through_t_junctions_is_still_flipped() {
    let mut l = loops();
    split_top(&mut l);
    let (mut tris, ends) = mesh(&reversed(&l, 1));
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!(s.reversed_edges, 0, "no seam of the top face is shared exactly");
    assert_eq!(s.gaps, 0);
    assert_eq!(s.flipped, vec![1], "the T-junction sides vote the top face over");
}

#[test]
fn a_boundary_bowed_within_the_gap_tolerance_is_closed() {
    // A side's top edge discretised through a point 0.5 * GAP off the chord:
    // what two faces' chords of one curved edge look like.
    let mut l = loops();
    l[3] = vec![p(0., 1., 0.), p(0., 1., 1.), p(0.5, 1., 1. + GAP * 0.5), p(1., 1., 1.), p(1., 1., 0.)];
    let (mut tris, ends) = mesh(&l);
    assert_eq!(close(&mut tris, &ends, WELD, GAP).gaps, 0);
}

#[test]
fn a_real_gap_wider_than_the_tolerance_is_found() {
    // The top face lifted by 10x the gap tolerance: a slot all round.
    let mut l = loops();
    for q in &mut l[1] {
        q.z += GAP * 10.0;
    }
    let (mut tris, ends) = mesh(&l);
    let s = close(&mut tris, &ends, WELD, GAP);
    assert_eq!(s.gaps, 8, "the top face's 4 edges and the sides' 4 top edges");
    assert!(s.first_gap.is_some());
    assert_eq!(s.gap_face, Some(1), "the top face owns 4 of the 8");
    assert!((s.widest - GAP * 10.0).abs() < 1e-12, "the slot's width: {}", s.widest);
}

#[test]
fn a_missing_face_leaves_a_gap() {
    let mut l = loops();
    l.remove(1);
    let (mut tris, ends) = mesh(&l);
    assert_eq!(close(&mut tris, &ends, WELD, GAP).gaps, 4);
}

#[test]
fn copies_of_a_vertex_within_the_weld_still_meet() {
    let (mut tris, ends) = mesh(&loops());
    let nudge = |q: Point3| p(q.x + 4e-7, q.y - 4e-7, q.z);
    for t in tris.iter_mut().take(4) {
        *t = Triangle::new(nudge(t.a), nudge(t.b), nudge(t.c));
    }
    let s = close(&mut tris, &ends, WELD, 1e-9);
    assert_eq!((s.gaps, s.t_junction_edges), (0, 0), "welded, the seams match exactly");
}

#[test]
fn a_face_never_closes_its_own_boundary() {
    // A strip narrower than the gap tolerance, alone: its long edges lie
    // within the tolerance of each other, but they are one face's own
    // boundary - the strip is open all round.
    let w = GAP * 0.5;
    let l = vec![vec![p(0., 0., 0.), p(1., 0., 0.), p(1., w, 0.), p(0., w, 0.)]];
    let (mut tris, ends) = mesh(&l);
    assert_eq!(close(&mut tris, &ends, WELD, GAP).gaps, 4);
}

/// The importer's own gap: a side's top edge bowed `bow` chord
/// tolerances off the top face's edge, judged as a closed shell.
fn judged_with_bow(bow: f64) -> Result<super::super::step_closed::Judged, crate::geometry::import::ImportError> {
    use super::super::step_closed::{Shell, judge};
    let tol = 1e-3;
    let mut l = loops();
    l[3] = vec![p(0., 1., 0.), p(0., 1., 1.), p(0.5, 1., 1. + tol * bow), p(1., 1., 1.), p(1., 1., 0.)];
    let (mut tris, ends) = mesh(&l);
    let shell = Shell { id: 1, faces: 6, closed: true, mm_per_unit: 1.0 };
    judge(&mut tris, Some(&ends), tol, &shell, |_| None, None)
}

#[test]
fn the_importer_closes_seams_within_two_chord_tolerances_only() {
    // Each side's chords stand within one tolerance of the shared curve,
    // so two sides within two of each other: 1.5 is a seam, 3 is a gap.
    let seam = judged_with_bow(1.5).expect("a seam imports");
    assert!(seam.open.is_none(), "{:?}", seam.open);
    // #1610: the gap is named for the caller, who refuses it or lands the
    // model incomplete.
    let gap = judged_with_bow(3.0).expect("a gap is named, not refused here");
    let hole = gap.open.unwrap_or_default();
    assert!(hole.contains("closed shell #1 (6 faces) meshes open"), "{hole}");
}

fn judged(tris: &mut [Triangle], ends: &[usize]) -> Result<Vec<String>, crate::geometry::import::ImportError> {
    use super::super::step_closed::{Shell, judge};
    let shell = Shell { id: 7, faces: ends.len(), closed: true, mm_per_unit: 1.0 };
    let judged = judge(tris, Some(ends), GAP / 2.0, &shell, |f| Some(100 + f as u64), None)?;
    match judged.open {
        Some(hole) => Err(super::super::step_closed::open_refusal(&hole)),
        None => Ok(judged.notes),
    }
}

#[test]
fn a_flipped_face_is_said_by_name_in_the_result() {
    let (mut tris, ends) = mesh(&reversed(&loops(), 3));
    let notes = judged(&mut tris, &ends).expect("a reversed face is fixed, not refused");
    assert_eq!(notes, vec!["shell #7: 1 face(s) wound against their neighbours were flipped: face #103 (4 of 6)".to_string()]);
    let (mut good, ends) = mesh(&loops());
    assert_eq!(judged(&mut good, &ends).expect("a cube imports"), Vec::<String>::new(), "nothing changed, nothing said");
}

#[test]
fn a_shared_edge_left_reversed_after_the_flips_is_refused_by_name() {
    // Face 3 meshed as two facets wound against each other: whichever way
    // it is turned, one of its seams runs the same way as its neighbour's.
    let l = loops();
    let (mut tris, mut ends) = mesh(&l[..3]);
    let q = &l[3];
    tris.push(Triangle::new(q[0], q[1], q[2]));
    tris.push(Triangle::new(q[0], q[3], q[2]));
    ends.push(tris.len());
    let (rest, rest_ends) = mesh(&l[4..]);
    let base = tris.len();
    tris.extend(rest);
    ends.extend(rest_ends.iter().map(|e| e + base));
    let err = judged(&mut tris, &ends).expect_err("a face left wound against a neighbour must not import");
    let msg = err.to_string();
    assert!(msg.contains("closed shell #7 (6 faces) is wound inconsistently: 2 shared edges still run the same way"), "{msg}");
    assert!(msg.contains("most between face #101 (2 of 6) and face #103 (4 of 6)"), "{msg}");
}

/// Reverse facet `i` of the mesh in place (its corners b and c swap).
fn turn(tris: &mut [Triangle], i: usize) {
    if let Some(t) = tris.get_mut(i) {
        *t = Triangle::new(t.a, t.c, t.b);
    }
}

#[test]
fn a_pair_met_only_across_t_junctions_that_still_disagrees_is_said() {
    // The top face is split at every edge midpoint, so it meets each side
    // across T-junctions only. Its two facets along the y = 0 edge are
    // turned against the rest: that pair's near votes disagree with the
    // others, and every exact shared edge still agrees.
    let mut l = loops();
    split_top(&mut l);
    let (mut tris, ends) = mesh(&l);
    let top = ends.first().copied().unwrap_or(0);
    for i in [top, top + 1] {
        turn(&mut tris, i);
    }
    let notes = judged(&mut tris, &ends).expect("T-junction-only disagreement imports, with a note");
    assert!(
        notes.iter().any(|n| n.contains("face pair(s) met only across T-junctions still disagree on winding after the flips")),
        "{notes:?}"
    );
}

#[test]
fn exactly_one_shared_edge_left_reversed_is_refused() {
    // Face 3 is fanned from its centroid, one facet per seam; one facet is
    // turned, so exactly one seam runs the same way as its neighbour's.
    let l = loops();
    let (mut tris, ends) = mesh(&l);
    let at = ends.get(2).copied().unwrap_or(0);
    turn(&mut tris, at);
    let err = judged(&mut tris, &ends).expect_err("one reversed shared edge is still a fault");
    let msg = err.to_string();
    assert!(msg.contains("is wound inconsistently: 1 shared edges still run the same way"), "{msg}");
}

//! Unit pins for `step_orient` (#1576 review): the flips are a function of
//! the votes alone, strongest evidence first, and outward is decided once
//! for the whole shell.

use super::*;
use crate::geometry::import::step_seams::close;

const WELD: f64 = 1e-6;
const GAP: f64 = 1e-3;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The six faces of the cube `lo..hi`, each a loop wound outward.
fn cube(lo: f64, hi: f64) -> Vec<Vec<Point3>> {
    let (a, b) = (lo, hi);
    vec![
        vec![p(a, a, a), p(a, b, a), p(b, b, a), p(b, a, a)],
        vec![p(a, a, b), p(b, a, b), p(b, b, b), p(a, b, b)],
        vec![p(a, a, a), p(b, a, a), p(b, a, b), p(a, a, b)],
        vec![p(a, b, a), p(a, b, b), p(b, b, b), p(b, b, a)],
        vec![p(a, a, a), p(a, a, b), p(a, b, b), p(a, b, a)],
        vec![p(b, a, a), p(b, b, a), p(b, b, b), p(b, a, b)],
    ]
}

/// Fan each loop from its centroid; `ends` per face.
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

fn flipped_of(mut tris: Vec<Triangle>, ends: &[usize]) -> Vec<usize> {
    close(&mut tris, ends, WELD, GAP).flipped
}

/// The unit cube with face 3 meshed as two facets wound against each
/// other: two of its seams agree with their neighbours and two do not, so
/// no set of flips satisfies every vote.
fn conflicted() -> (Vec<Triangle>, Vec<usize>) {
    let l = cube(0., 1.);
    let (mut tris, mut ends) = mesh(&l[..3]);
    let q = &l[3];
    tris.push(Triangle::new(q[0], q[1], q[2]));
    tris.push(Triangle::new(q[0], q[3], q[2])); // wound against its twin
    ends.push(tris.len());
    let (rest, rest_ends) = mesh(&l[4..]);
    let base = tris.len();
    tris.extend(rest);
    ends.extend(rest_ends.iter().map(|e| e + base));
    (tris, ends)
}

/// A fixed pseudo-random permutation of each face's facets (an LCG: no
/// dependency, the same on every run of the test).
fn shuffled(tris: &[Triangle], ends: &[usize], seed: u64) -> Vec<Triangle> {
    let mut out = tris.to_vec();
    let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for f in 0..ends.len() {
        let r = range(ends, f);
        for i in (r.start + 1..r.end).rev() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = r.start + (s >> 33) as usize % (i - r.start + 1);
            out.swap(i, j);
        }
    }
    out
}

#[test]
fn conflicting_votes_give_one_answer_on_every_run_and_in_every_order() {
    let (tris, ends) = conflicted();
    let first = close(&mut tris.clone(), &ends, WELD, GAP);
    assert!(first.contradictions > 0, "the fixture's votes conflict: {first:?}");
    assert!(first.unresolved_edges > 0, "a seam stays reversed whatever is flipped: {first:?}");
    for seed in 0..64 {
        // A new hash seed on every call, and a new facet order per seed.
        let mut t = shuffled(&tris, &ends, seed);
        let s = close(&mut t, &ends, WELD, GAP);
        let got = (&s.flipped, s.contradictions, s.unresolved_edges, s.unresolved_pair);
        let want = (&first.flipped, first.contradictions, first.unresolved_edges, first.unresolved_pair);
        assert_eq!(got, want, "seed {seed}");
    }
    // The one answer, pinned: all votes weigh one edge, so they are taken
    // by face number; (0, 3) turns face 3 over, and its seams with faces 1
    // and 4 are the two left reversed.
    assert_eq!(first.flipped, vec![3]);
    assert_eq!((first.contradictions, first.unresolved_edges, first.unresolved_pair), (2, 2, Some((1, 3))));
}

#[test]
fn the_first_gap_named_is_the_same_in_every_order() {
    // The top face lifted off: a gap all round, eight edges to name first.
    let mut l = cube(0., 1.);
    for q in &mut l[1] {
        q.z += GAP * 10.0;
    }
    let (tris, ends) = mesh(&l);
    let first = close(&mut tris.clone(), &ends, WELD, GAP);
    assert_eq!(first.gaps, 8);
    for seed in 0..64 {
        let s = close(&mut shuffled(&tris, &ends, seed), &ends, WELD, GAP);
        assert_eq!((s.first_gap, s.gap_face, s.widest), (first.first_gap, first.gap_face, first.widest), "seed {seed}");
    }
}

fn votes(list: &[((u32, u32), Vote)]) -> BTreeMap<(u32, u32), Vote> {
    list.iter().copied().collect()
}

#[test]
fn many_shared_edges_outvote_one_whatever_the_face_numbers() {
    // Faces 0 and 1 share one reversed edge; both share five agreeing
    // edges with face 2. By face number the weak pair would come first.
    let (tris, ends) = mesh(&cube(0., 1.)[..3]);
    let v = votes(&[
        ((0, 1), Vote { exact: [0, 1], near: [0, 0] }),
        ((0, 2), Vote { exact: [5, 0], near: [0, 0] }),
        ((1, 2), Vote { exact: [5, 0], near: [0, 0] }),
    ]);
    let o = orient(&tris, &ends, &v);
    assert_eq!(o, Oriented { flipped: vec![], contradictions: 1, unresolved_edges: 1, unresolved_pair: Some((0, 1)) });
}

#[test]
fn a_shared_edge_outvotes_any_number_of_t_junction_votes() {
    let (tris, ends) = mesh(&cube(0., 1.)[..3]);
    let v = votes(&[
        ((0, 1), Vote { exact: [1, 0], near: [0, 0] }),
        ((0, 2), Vote { exact: [0, 0], near: [0, 9] }),
        ((1, 2), Vote { exact: [1, 0], near: [0, 0] }),
    ]);
    let o = orient(&tris, &ends, &v);
    assert_eq!((o.flipped, o.contradictions, o.unresolved_edges), (vec![], 1, 0));
}

/// The unit cube with a cavity `0.25..0.75` inside it, wound inward as a
/// cavity's faces are: one shell, two groups that share no seam.
fn box_in_box() -> Vec<Vec<Point3>> {
    let mut l = cube(0., 1.);
    l.extend(cube(0.25, 0.75).into_iter().map(|mut f| {
        f.reverse();
        f
    }));
    l
}

#[test]
fn a_cavity_wound_inward_keeps_every_face() {
    let (tris, ends) = mesh(&box_in_box());
    assert_eq!(flipped_of(tris, &ends), Vec::<usize>::new());
}

#[test]
fn outward_is_decided_once_for_the_whole_shell_not_per_group() {
    // One outer face wound inward: it alone is flipped; the cavity, a
    // group of its own enclosing a negative volume, keeps its winding.
    // Face 0, the outer group's lowest, so rooting a group at its lowest
    // face (not at its majority) would turn the cavity over too.
    let mut l = box_in_box();
    l[0].reverse();
    let (tris, ends) = mesh(&l);
    assert_eq!(flipped_of(tris, &ends), vec![0]);
}

#[test]
fn a_lone_curved_face_keeps_its_winding() {
    // A whole cylinder wall as one face, wound into its concavity, joined
    // to nothing but its own seam: no vote says it is wrong, so its own
    // curvature must not turn it over.
    let n = 16;
    let ring = |i: usize, z: f64| {
        let a = std::f64::consts::TAU * (i % n) as f64 / n as f64;
        p(a.cos(), a.sin(), z)
    };
    let mut tris = Vec::new();
    for i in 0..n {
        let (a, b, c, d) = (ring(i, 0.), ring(i + 1, 0.), ring(i + 1, 1.), ring(i, 1.));
        tris.push(Triangle::new(a, c, b));
        tris.push(Triangle::new(a, d, c));
    }
    let ends = vec![tris.len()];
    assert_eq!(flipped_of(tris, &ends), Vec::<usize>::new());
}

#[test]
fn the_side_with_more_faces_keeps_the_kernels_winding() {
    // Faces 0..4 agree with each other, face 5 not: the group's majority
    // stands, then the whole shell's volume says which side is outward.
    let mut l = cube(0., 1.);
    l[5].reverse();
    let (tris, ends) = mesh(&l);
    assert_eq!(flipped_of(tris, &ends), vec![5]);
}

#[test]
fn parity_find_compresses_without_losing_the_relation() {
    let mut u = Parity::new(5);
    assert!(u.join(0, 1, true));
    assert!(u.join(1, 2, true));
    assert!(u.join(3, 4, false));
    assert!(u.join(2, 3, true));
    let rel = |u: &mut Parity, f, g| u.find(f).1 ^ u.find(g).1;
    assert!(!rel(&mut u, 0, 2));
    assert!(rel(&mut u, 0, 4));
    assert!(!u.join(0, 4, false), "already joined the other way");
    assert!(u.join(0, 4, true));
}

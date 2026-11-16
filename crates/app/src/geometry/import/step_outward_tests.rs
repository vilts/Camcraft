//! #1617: which bodies `turn_outward` turns, and which it leaves alone.
//! Provenance: our own hand-built facet sets; no vendor material.

use super::*;
use crate::geometry::Point3;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// An axis-aligned box at `o` with side `s`, wound outward.
fn cube(o: [f64; 3], s: f64) -> Vec<Triangle> {
    let v = |i: usize| {
        let (x, y, z) = ((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
        p(o[0] + x * s, o[1] + y * s, o[2] + z * s)
    };
    // Each quad listed counter-clockwise seen from outside.
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut out = Vec::new();
    for q in quads {
        out.push(Triangle::new(v(q[0]), v(q[1]), v(q[2])));
        out.push(Triangle::new(v(q[0]), v(q[2]), v(q[3])));
    }
    out
}

fn inverted(t: &[Triangle]) -> Vec<Triangle> {
    t.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect()
}

fn volume(t: &[Triangle]) -> f64 {
    t.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum()
}

#[test]
fn an_outward_cube_is_left_alone() {
    let mut m = Mesh::from_facets(cube([0.0; 3], 2.0));
    let before = m.clone();
    assert!((volume(&m.facets) - 8.0).abs() < 1e-9);
    assert_eq!(turn_outward(&mut m), None);
    assert_eq!(m, before);
}

#[test]
fn an_inward_cube_is_turned_outward_and_said() {
    let mut m = Mesh::from_facets(inverted(&cube([0.0; 3], 2.0)));
    assert!((volume(&m.facets) + 8.0).abs() < 1e-9);
    let note = turn_outward(&mut m).expect("an inward closed cube is turned");
    assert!((volume(&m.facets) - 8.0).abs() < 1e-9, "{}", volume(&m.facets));
    assert!(note.contains("1 closed solid(s) wound inward") && note.contains("12 facet(s) reversed"), "{note}");
}

#[test]
fn only_the_inward_body_of_two_is_turned() {
    let good = cube([10.0, 0.0, 0.0], 1.0);
    let mut all = inverted(&cube([0.0; 3], 2.0));
    all.extend(good.iter().copied());
    let mut m = Mesh::from_facets(all);
    let note = turn_outward(&mut m).expect("one body is inward");
    assert!(note.contains("12 facet(s) reversed"), "{note}");
    assert_eq!(&m.facets[12..], &good[..], "the outward body keeps its facets");
    assert!((volume(&m.facets) - 9.0).abs() < 1e-9);
}

/// A lone curved face (a half cylinder, open along its seams and ends),
/// wound either way: it has no inside, so it is never turned.
#[test]
fn a_lone_curved_face_is_left_alone() {
    let n = 12;
    let mut strip = Vec::new();
    for i in 0..n {
        let (a0, a1) = (std::f64::consts::PI * i as f64 / n as f64, std::f64::consts::PI * (i + 1) as f64 / n as f64);
        let (p0, p1) = ((a0.cos(), a0.sin()), (a1.cos(), a1.sin()));
        strip.push(Triangle::new(p(p0.0, p0.1, 0.0), p(p1.0, p1.1, 0.0), p(p1.0, p1.1, 5.0)));
        strip.push(Triangle::new(p(p0.0, p0.1, 0.0), p(p1.0, p1.1, 5.0), p(p0.0, p0.1, 5.0)));
    }
    for facets in [strip.clone(), inverted(&strip)] {
        let mut m = Mesh::from_facets(facets);
        let before = m.clone();
        assert_eq!(turn_outward(&mut m), None);
        assert_eq!(m, before);
    }
}

/// A cube with one facet wound against its neighbours is mixed: the
/// importer does not guess its outside.
#[test]
fn a_mixed_cube_is_left_alone() {
    let mut facets = inverted(&cube([0.0; 3], 2.0));
    if let Some(t) = facets.first_mut() {
        *t = Triangle::new(t.a, t.c, t.b);
    }
    let mut m = Mesh::from_facets(facets);
    let before = m.clone();
    assert_eq!(turn_outward(&mut m), None);
    assert_eq!(m, before);
}

//! Unit tests for `solid_rest` (split from `solid_rest.rs` by the 400-line law).
//! Provenance: our own parity and boundary-sector law tests (including #2880 revolved meridian seams and V-roots); no vendor material.

use super::*;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// One horizontal square (side 2, centered), split on a diagonal;
/// `up` picks the winding (law c: parity must not care).
fn sheet(z: f64, up: bool) -> Vec<Triangle> {
    let (a, b, c, d) = (p(-1., -1., z), p(1., -1., z), p(1., 1., z), p(-1., 1., z));
    if up {
        vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
    } else {
        vec![Triangle::new(a, d, c), Triangle::new(a, c, b)]
    }
}

/// Closed axis-aligned box [-1,1]^2 x [z0, z1]: 12 triangles.
fn closed_box(z0: f64, z1: f64) -> Vec<Triangle> {
    let mut f = sheet(z0, false);
    f.extend(sheet(z1, true));
    let wall = |q: [Point3; 4]| {
        vec![
            Triangle::new(q[0], q[1], q[2]),
            Triangle::new(q[0], q[2], q[3]),
        ]
    };
    f.extend(wall([p(-1., -1., z0), p(1., -1., z0), p(1., -1., z1), p(-1., -1., z1)]));
    f.extend(wall([p(1., -1., z0), p(1., 1., z0), p(1., 1., z1), p(1., -1., z1)]));
    f.extend(wall([p(1., 1., z0), p(-1., 1., z0), p(-1., 1., z1), p(1., 1., z1)]));
    f.extend(wall([p(-1., 1., z0), p(-1., -1., z0), p(-1., -1., z1), p(-1., 1., z1)]));
    f
}

fn flip(f: &[Triangle]) -> Vec<Triangle> {
    f.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect()
}

/// Closed revolve of a `(radius, z)` meridian about Z, `segs` sides.
fn revolve(meridian: &[(f64, f64)], segs: usize) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / segs as f64;
        p(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

#[test]
fn closed_box_columns_pair_to_one_interval() {
    let f = closed_box(0.0, 10.0);
    let c = column_crossings(&f, 0.0, 0.0);
    // (0,0) sits ON both sheets' shared diagonal - law (a)'s
    // boundary merge, the exact case a half-open rule loses.
    assert_eq!(c, Crossings::Paired(vec![0.0, 10.0]));
    assert_eq!(c.intervals(), Some(vec![(0.0, 10.0)]));
}

#[test]
fn an_open_sheet_is_unknown_not_a_lie() {
    // Law (b): one sheet is not material - it is an unknown.
    let c = column_crossings(&sheet(5.0, true), 0.4, 0.1);
    assert_eq!(c, Crossings::Open { z: vec![5.0] });
    assert_eq!(c.intervals(), None);
}

#[test]
fn ceiling_over_floor_finds_the_sheltered_material() {
    // The hidden-rest shape: a ceiling plate 9..10 over a floor
    // slab 0..2. The height field of either alone would see only
    // its own top; the column pairs [0,2],[9,10] and the slab's
    // material reads as material BELOW the plate, where a Z-
    // following ball never sees it.
    let mut f = closed_box(9.0, 10.0);
    f.extend(closed_box(0.0, 2.0));
    let c = column_crossings(&f, 0.4, 0.1); // off all diagonals
    assert_eq!(c.intervals(), Some(vec![(0.0, 2.0), (9.0, 10.0)]));
}

#[test]
fn winding_is_irrelevant() {
    // Law c: flip EVERY facet of a closed box; crossings identical.
    let f = closed_box(0.0, 10.0);
    for (x, y) in [(0.3, 0.2), (0.0, 0.0), (1.0, -0.5), (-1.0, 1.0), (2.0, 0.0)] {
        let a = column_crossings(&f, x, y);
        let b = column_crossings(&flip(&f), x, y);
        assert_eq!(a, b, "winding changed the column at ({x},{y})");
    }
}

#[test]
fn columns_on_edges_and_vertices_stay_even() {
    // Law (a) witnesses: through shared edges and corners of a
    // closed box, parity must stay even and the box must still
    // read as ONE interval - boundary merge, not double count.
    let f = closed_box(0.0, 10.0);
    for (x, y) in [
        (0.0, 0.0),   // the shared diagonal of both sheets
        (1.0, 0.0),   // mid of the boundary edge x=1
        (1.0, 1.0),   // up a vertical corner
        (-1.0, -1.0), // another corner
        (0.5, -1.0),  // on a bottom-edge line of the top sheet
    ] {
        match column_crossings(&f, x, y) {
            Crossings::Paired(z) => {
                assert_eq!(z, vec![0.0, 10.0], "column at ({x},{y})")
            }
            Crossings::Open { z } => panic!("odd parity at ({x},{y}): {z:?}"),
        }
    }
}

#[test]
fn flush_stacked_shells_are_reported_not_guessed() {
    // The knife edge law (a) promises to REPORT: two boxes sharing
    // the plane z=5 as flush sheets. Ordinary columns pair to one
    // material interval; on the flush pair's own shared diagonal
    // the level is ambiguous and must come out Open, never a lie.
    let mut f = closed_box(0.0, 5.0);
    f.extend(closed_box(5.0, 10.0));
    let inside = column_crossings(&f, 0.4, 0.1);
    assert_eq!(inside, Crossings::Paired(vec![0.0, 5.0, 5.0, 10.0]));
    assert_eq!(inside.intervals(), Some(vec![(0.0, 5.0), (5.0, 10.0)]));
    // A column riding both sheets' boundary edges makes only
    // BOUNDARY claims at z=5 - and the merge cannot tell "two
    // faces of one sheet" from "one face of each of two flush
    // sheets". It merges (that is the honest half of the law:
    // when in doubt report unknown, never assert material), so
    // the corner reads Open, not a false solid interval.
    match column_crossings(&f, 1.0, 0.0) {
        Crossings::Open { z } => assert_eq!(z, vec![0.0, 5.0, 10.0]),
        Crossings::Paired(z) => panic!("a knife edge was guessed: {z:?}"),
    }
}

#[test]
fn degenerate_input_is_total() {
    // Law (b) fuzz floor: NaNs, zero-area facets, empty mesh.
    let junk = vec![
        Triangle::new(p(f64::NAN, 0., 0.), p(1., 0., 0.), p(0., 1., 0.)),
        Triangle::new(p(0., 0., 0.), p(0., 0., 0.), p(0., 0., 0.)),
        Triangle::new(p(0., 0., 1.), p(0., 0., 1.), p(0., 0., 1.)),
    ];
    let _ = column_crossings(&junk, 0.0, 0.0);
    let _ = column_crossings(&[], 0.0, 0.0);
    let _ = column_crossings(&junk, f64::NAN, 0.0);
}

#[test]
fn a_level_facet_crosses_at_exactly_its_height() {
    // #780: the barycentric sum rounds by an ulp on some rays
    // ((5.25, -6.25) over a 2.0 plane did); a level facet answers
    // its own z, bit for bit, whether the plane is on a lattice
    // (2.0) or off it (5.1), from every ray inside it.
    for (z0, z1) in [(-638.0, 2.0), (-6.3, 5.1), (0.0, 1e-3)] {
        let facets = closed_box(z0, z1);
        for i in 0..21 {
            for j in 0..21 {
                let (x, y) = (-0.95 + 0.095 * f64::from(i), -0.95 + 0.095 * f64::from(j));
                match column_crossings(&facets, x, y) {
                    Crossings::Paired(c) => {
                        assert_eq!(c.len(), 2, "({x}, {y})");
                        assert_eq!(c[0].to_bits(), z0.to_bits(), "({x}, {y}): floor {} vs {z0}", c[0]);
                        assert_eq!(c[1].to_bits(), z1.to_bits(), "({x}, {y}): top {} vs {z1}", c[1]);
                    }
                    Crossings::Open { z } => panic!("({x}, {y}): open {z:?}"),
                }
            }
        }
    }
}

#[test]
fn revolved_meridian_edge_and_v_root_keep_true_parity() {
    // #2880: on a 64-sided revolve, a column on the 45-degree meridian
    // (x == y) hits two non-coplanar tilted facets along their shared
    // edge. Each tilted surface must count ONCE (disjoint XY sectors),
    // never twice - otherwise a V-groove's air gap pairs as solid and
    // exposes its downward ceiling as a hidden top.
    let cone = revolve(&[(0.0, 0.0), (10.0, 0.0), (5.0, 40.0), (0.0, 40.0)], 64);
    for (x, y) in [(5.0, 5.0), (-5.0, -5.0), (7.0, 0.0), (0.0, 0.0)] {
        let ivs = column_crossings(&cone, x, y).intervals().expect("closed cone pairs on meridian and apex");
        assert_eq!(ivs.len(), 1, "cone at ({x}, {y}): {ivs:?}");
        assert_eq!(column_crossings(&cone, x, y), column_crossings(&flip(&cone), x, y), "law c at ({x}, {y})");
    }
    // A bar with one 60-degree V-groove: on the 45-degree meridian at
    // rho = 4.1719 (inside the groove) the air gap is [lo_flank, hi_flank],
    // NEVER a solid interval; at the exact V-root radius (4.08) the
    // ray grazes the root and stays one unbroken solid interval [0, 10].
    let half = 0.92 * 30.0_f64.to_radians().tan();
    let vbar = revolve(
        &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0 - half), (4.08, 5.0), (5.0, 5.0 + half), (5.0, 10.0), (0.0, 10.0)],
        64,
    );
    let ivs = column_crossings(&vbar, -2.95, -2.95).intervals().expect("vbar on 45-degree meridian");
    assert_eq!(ivs.len(), 2, "two solid teeth, one air groove between them: {ivs:?}");
    assert!(ivs[0].1 < 5.0 && ivs[1].0 > 5.0, "lower flank exits before 5, upper re-enters after 5: {ivs:?}");
    let root = column_crossings(&vbar, 4.08, 0.0).intervals().expect("grazing V-root stays solid");
    assert_eq!(root, vec![(0.0, 10.0)], "grazing V-root does not split the solid");
    assert_eq!(column_crossings(&vbar, -2.95, -2.95), column_crossings(&flip(&vbar), -2.95, -2.95));
}

#[test]
fn claim_stays_forty_bytes() {
    assert_eq!(std::mem::size_of::<Claim>(), 40);
}

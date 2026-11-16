//! Unit tests of `sinker_electrode_body` (#1152): the laws the offset's
//! answer must keep. A sound offset never breaks them, so each is
//! handed a wrong section on purpose - the guard that stands between a
//! wrong offset and a wrong electrode is pinned, not assumed.
//! Provenance: our own synthetic cavities.

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::sinker_cavity::Cavity;
use crate::sinker_electrode::{derive, standing_in, ElectrodeBox, ElectrodeSpec};
use crate::sinker_electrode_body::body_of;

fn q(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn spec(undersize: f64) -> ElectrodeSpec {
    ElectrodeSpec { undersize, orbit: None, stub: 10.0, pickup: 15.0 }
}

/// An L cavity: 20 x 12 with its upper right 12 x 6 left standing.
fn ell() -> Cavity {
    Cavity { outline: vec![q(0.0, 0.0), q(20.0, 0.0), q(20.0, 6.0), q(8.0, 6.0), q(8.0, 12.0), q(0.0, 12.0)], floor_z: 0.0, top_z: 5.0, walls: 6 }
}

fn region(c: &Cavity) -> Region {
    Region::new(vec![Loop::new(c.outline.clone()).expect("a ring")]).expect("a region")
}

fn body(c: &Cavity, s: &ElectrodeSpec, outline: Vec<Point2>) -> Result<ElectrodeBox, String> {
    body_of(c, s, 0.0, &region(c), outline).map_err(|e| e.to_string())
}

/// The L's true section for an inset of `d`.
fn inset(d: f64) -> Vec<Point2> {
    vec![q(d, d), q(20.0 - d, d), q(20.0 - d, 6.0 - d), q(8.0 - d, 6.0 - d), q(8.0 - d, 12.0 - d), q(d, 12.0 - d)]
}

#[test]
fn a_sound_section_is_the_electrode() {
    let e = body(&ell(), &spec(0.1), inset(0.1)).expect("the L's electrode");
    assert!(e.body && e.outline == inset(0.1) && e.ring == ell().outline, "{e:?}");
    assert!((e.face.0 - 19.8).abs() < 1e-12 && (e.face.1 - 11.8).abs() < 1e-12 && e.center == (10.0, 6.0), "{e:?}");
    assert_eq!((e.cavity, e.depth, e.length, e.pickup, e.inset), ((20.0, 12.0), 5.0, 15.0, 15.0, 0.1));
    // The same answer as the published offset gives, within its lattice.
    let d = derive(&ell(), &spec(0.1)).expect("derived");
    assert!((d.face.0 - e.face.0).abs() < 1e-5 && (d.face.1 - e.face.1).abs() < 1e-5 && d.outline.len() == 6, "{d:?}");
}

#[test]
fn a_section_nearer_the_wall_than_the_inset_is_refused_by_name() {
    // The section of a 0.05 inset, offered for a 0.1 one: too large.
    let why = body(&ell(), &spec(0.1), inset(0.05)).expect_err("too near the wall");
    assert!(why.contains("nearer than the inset 0.100000") && why.contains("stands 0.050000 from the cavity's wall"), "{why}");
    // A point outside the cavity altogether (in the L's notch).
    let mut out = inset(0.1);
    out[3] = q(12.0, 9.0);
    let why = body(&ell(), &spec(0.1), out).expect_err("outside the cavity");
    assert!(why.contains("nearer than the inset") && why.contains("(12.000, 9.000)"), "{why}");
    // Within the lattice's slack it stands.
    assert!(body(&ell(), &spec(0.1), inset(0.1 - 5e-6)).is_ok());
}

#[test]
fn a_section_too_small_too_fine_too_large_or_no_section_is_refused_by_name() {
    // A sliver left in the middle of the cavity.
    let sliver = vec![q(3.0, 3.0), q(3.0005, 3.0), q(3.0005, 5.0), q(3.0, 5.0)];
    let why = body(&ell(), &spec(0.1), sliver).expect_err("a sliver");
    assert!(why.contains("under the 0.001 mm worth making"), "{why}");
    // More points than the stock sim sweeps: a 1100-gon in the cavity.
    let fine: Vec<Point2> = (0..1100).map(|i| {
        let a = f64::from(i) * std::f64::consts::TAU / 1100.0;
        q(4.0 + 2.0 * a.cos(), 4.0 + 2.0 * a.sin())
    }).collect();
    let why = body(&ell(), &spec(0.1), fine).expect_err("too fine");
    assert!(why.contains("gives an electrode of 1100 points, more than the 1024 the stock sim sweeps"), "{why}");
    // An extent past the tool record's bound.
    let wide = Cavity { outline: vec![q(0.0, 0.0), q(600.0, 0.0), q(601.0, 1.0), q(601.0, 12.0), q(0.0, 12.0)], ..ell() };
    let section = vec![q(1.0, 1.0), q(599.0, 1.0), q(600.0, 2.0), q(600.0, 11.0), q(1.0, 11.0)];
    let why = body(&wide, &spec(0.1), section).expect_err("too large");
    assert!(why.contains("extent 599.000 x 10.000 is larger than the 500 mm"), "{why}");
    // A length that does not stand above the cavity.
    let deep = Cavity { floor_z: -1e17, top_z: 1e17, ..ell() };
    let why = body(&deep, &ElectrodeSpec { stub: 1e-3, ..spec(0.1) }, inset(0.1)).expect_err("no stub left");
    assert!(why.contains("does not stand above"), "{why}");
    // A bow tie well inside the cavity: every point keeps the inset, but
    // it is no section - the tool record's own door names it.
    let tie = vec![q(2.0, 2.0), q(6.0, 2.0), q(3.0, 5.0), q(6.0, 5.0)];
    let why = body(&ell(), &spec(0.1), tie).expect_err("no section");
    assert!(why.contains("the offset outline is no electrode section") && why.contains("crosses itself"), "{why}");
}

#[test]
fn a_column_that_cannot_be_built_is_refused_by_name() {
    let e = body(&ell(), &spec(0.1), inset(0.1)).expect("the L's electrode");
    let boss = [Triangle::new(Point3::new(2.0, 8.0, 2.0), Point3::new(6.0, 8.0, 2.0), Point3::new(6.0, 10.0, 2.0))];
    assert!(standing_in(&e, &boss).is_ok_and(|at| at.is_some()));
    // A ring of two points bounds no column: said, never read as clear.
    let broken = ElectrodeBox { ring: vec![q(0.0, 0.0), q(20.0, 0.0)], ..e };
    let why = standing_in(&broken, &boss).expect_err("no column").to_string();
    assert!(why.contains("the cavity's column could not be built") && why.contains("no electrode is made"), "{why}");
}

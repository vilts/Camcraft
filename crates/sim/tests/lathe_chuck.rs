//! Lathe chucks generated from their figures (#1442): the envelope each
//! chuck's shells stand in (body diameter and length, jaw reach and
//! height, collet nose), the mesh law reads them as closed solids, and
//! every nonsense figure is refused by name. The envelope is pinned to
//! the tessellation allowance (#1470): steel a hair inside every true
//! outer face and a hair outside every true bore wall, all the way round,
//! and air a hair past the polygon's own allowance - so a solid shrunk by
//! more than [`HAIR`] anywhere fails here.
//!
//! Gate map (M4.4): covers camcraft_sim::chuck.

use camcraft_sim::chuck::{self, ColletChuck, JawChuck};
use camcraft_sim::fixture_mesh::FixtureMesh;
use camcraft_sim::geometry::{Point3, Triangle};

fn three_jaw() -> JawChuck {
    JawChuck {
        body_diameter: 160.0,
        body_length: 70.0,
        bore_diameter: 40.0,
        grip_diameter: 40.0,
        jaw_length: 35.0,
        jaw_width: 20.0,
        jaw_height: 30.0,
    }
}

fn collet() -> ColletChuck {
    ColletChuck { body_diameter: 100.0, body_length: 40.0, nose_diameter: 50.0, nose_length: 30.0, collet_bore: 20.0 }
}

fn bounds(t: &[Triangle]) -> (Point3, Point3) {
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in t.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = Point3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Point3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo, hi)
}

/// How close (mm) to a true surface the envelope probes stand.
const HAIR: f64 = 1e-3;

/// The angles probed round a ring: on every polygon vertex and every
/// flat's middle (each nudged off the exact seam, so the ray-crossing law
/// never grazes an edge), the two places a round face's drawing is
/// farthest from and nearest to its circle.
fn sweep() -> Vec<f64> {
    let n = chuck::SIDES as f64;
    (0..2 * chuck::SIDES).map(|k| std::f64::consts::PI * k as f64 / n + 1e-7).collect()
}

/// The tessellation allowance of a round face of radius `r`: how far the
/// polygon's vertices stand off its flats' middles.
fn allowance(r: f64) -> f64 {
    r * (1.0 / (std::f64::consts::PI / chuck::SIDES as f64).cos() - 1.0)
}

/// Steel a hair inside the true outer face `r_out` and a hair outside the
/// true bore `r_in` all round at height `z`; air a hair past the outer
/// face's allowance and a hair inside the bore's.
fn ring_envelope(m: &FixtureMesh, r_in: f64, r_out: f64, z: f64, what: &str) {
    for a in sweep() {
        let at = |r: f64| Point3::new(r * a.cos(), r * a.sin(), z);
        assert!(m.contains(at(r_out - HAIR)), "{what}: steel just inside the outer face at {a} rad");
        assert!(m.contains(at(r_in + HAIR)), "{what}: steel just outside the bore wall at {a} rad");
        assert!(!m.contains(at(r_out + allowance(r_out) + HAIR)), "{what}: air past the outer allowance at {a} rad");
        assert!(!m.contains(at(r_in - allowance(r_in) - HAIR)), "{what}: air inside the bore allowance at {a} rad");
    }
}

fn max_radius(t: &[Triangle]) -> f64 {
    t.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| p.x.hypot(p.y)).fold(0.0, f64::max)
}

#[test]
fn three_jaw_envelope_is_pinned() {
    let c = three_jaw();
    c.check("3-jaw chuck", 3).expect("default figures pass");
    let t = c.solid(3);
    let (lo, hi) = bounds(&t);
    assert!((lo.z + 70.0).abs() < 1e-9, "body length 70 below the face, got {}", lo.z);
    assert!((hi.z - 30.0).abs() < 1e-9, "jaws stand 30 above the face, got {}", hi.z);
    // The drawn body encloses the true 80 mm radius, within one flat's sag.
    let sag = 80.0 / (std::f64::consts::PI / chuck::SIDES as f64).cos();
    let r = max_radius(&t);
    assert!(r >= 80.0 && r <= sag + 1e-9, "body radius drawn {r}");
    let m = FixtureMesh::build(&t).expect("closed shells");
    // Steel: just inside the body's rim, in the body under the face, a
    // jaw's tip (the first jaw on +x reaches 20 + 35 = 55).
    ring_envelope(&m, 20.0, 80.0, -35.0, "body");
    assert!(m.contains(Point3::new(0.0, 50.0, -1.0)), "body under the face");
    assert!(m.contains(Point3::new(55.0 - HAIR, 0.0, 15.0)), "first jaw's tip");
    assert!(m.contains(Point3::new(20.0 + HAIR, 0.0, 30.0 - HAIR)), "first jaw's gripping face");
    assert!(m.contains(Point3::new(40.0, 10.0 - HAIR, 15.0)), "first jaw's side");
    // Air: past the rim, past the jaw tip, the bore, the work's room
    // between the jaws, and above the jaws.
    assert!(!m.contains(Point3::new(81.0, 0.0, -35.0)), "past the rim");
    assert!(!m.contains(Point3::new(55.0 + HAIR, 0.0, 15.0)), "past the jaw tip");
    assert!(!m.contains(Point3::new(20.0 - HAIR, 0.0, 15.0)), "before the gripping face");
    assert!(!m.contains(Point3::new(40.0, 10.0 + HAIR, 15.0)), "beside the first jaw");
    assert!(!m.contains(Point3::new(40.0, 0.0, 30.0 + HAIR)), "over the first jaw");
    assert!(!m.contains(Point3::new(0.0, 0.0, -35.0)), "the through bore");
    assert!(!m.contains(Point3::new(19.0, 0.0, 15.0)), "the work's room");
    assert!(!m.contains(Point3::new(54.0, 0.0, 31.0)), "above the jaws");
    // The other two jaws stand 120 degrees round.
    let a = std::f64::consts::TAU / 3.0;
    assert!(m.contains(Point3::new(50.0 * a.cos(), 50.0 * a.sin(), 15.0)), "second jaw");
    assert!(!m.contains(Point3::new(-50.0, 0.0, 15.0)), "no jaw at 180 degrees on a 3-jaw");
}

#[test]
fn four_jaw_has_four_jaws_at_quarters() {
    let c = JawChuck { body_diameter: 200.0, jaw_length: 45.0, ..three_jaw() };
    c.check("4-jaw chuck", 4).expect("passes");
    let m = FixtureMesh::build(&c.solid(4)).expect("closed");
    for p in [(50.0, 0.0), (0.0, 50.0), (-50.0, 0.0), (0.0, -50.0)] {
        assert!(m.contains(Point3::new(p.0, p.1, 15.0)), "jaw at {p:?}");
    }
    let d = 50.0 / 2f64.sqrt();
    assert!(!m.contains(Point3::new(d, d, 15.0)), "no jaw at 45 degrees");
}

#[test]
fn collet_envelope_is_pinned() {
    let c = collet();
    c.check("Collet chuck").expect("passes");
    let t = c.solid();
    let (lo, hi) = bounds(&t);
    assert!((lo.z + 70.0).abs() < 1e-9 && hi.z.abs() < 1e-9, "z {}..{}", lo.z, hi.z);
    let m = FixtureMesh::build(&t).expect("closed shells");
    ring_envelope(&m, 10.0, 25.0, -15.0, "nose");
    ring_envelope(&m, 10.0, 50.0, -50.0, "body");
    assert!(!m.contains(Point3::new(0.0, 0.0, -50.0)), "the collet bore");
    assert!(!m.contains(Point3::new(0.0, 0.0, 5.0)), "above the nose");
}

#[test]
fn jaw_chuck_nonsense_is_refused_by_name() {
    let bad = |c: JawChuck, jaws: u32, needle: &str| {
        let e = c.check("3-jaw chuck", jaws).expect_err(needle);
        assert!(e.contains("3-jaw chuck") && e.contains(needle), "{e}");
    };
    bad(JawChuck { body_diameter: 0.0, ..three_jaw() }, 3, "body diameter must be more than zero");
    bad(JawChuck { body_length: -5.0, ..three_jaw() }, 3, "body length must be more than zero");
    bad(JawChuck { jaw_width: f64::NAN, ..three_jaw() }, 3, "jaw width must be a finite length");
    bad(JawChuck { jaw_height: 1e9, ..three_jaw() }, 3, "jaw height (above the face)");
    bad(JawChuck { bore_diameter: 160.0, ..three_jaw() }, 3, "through bore");
    bad(JawChuck { jaw_length: 61.0, ..three_jaw() }, 3, "past the body");
    bad(JawChuck { jaw_width: 70.0, ..three_jaw() }, 3, "would meet each other");
    bad(three_jaw(), 1, "2 to 12 jaws");
}

#[test]
fn collet_nonsense_is_refused_by_name() {
    let bad = |c: ColletChuck, needle: &str| {
        let e = c.check("Collet chuck").expect_err(needle);
        assert!(e.contains("Collet chuck") && e.contains(needle), "{e}");
    };
    bad(ColletChuck { collet_bore: 50.0, ..collet() }, "collet bore");
    bad(ColletChuck { nose_diameter: 120.0, ..collet() }, "nose diameter");
    bad(ColletChuck { nose_length: f64::INFINITY, ..collet() }, "nose length must be a finite length");
}

#[test]
fn round_stock_fits_and_square_stock_is_refused() {
    let (d, at) = chuck::round_stock(Point3::new(-12.0, -12.0, -16.0), Point3::new(12.0, 12.0, 20.0)).expect("round");
    assert!((d - 24.0).abs() < 1e-12 && at.x.abs() < 1e-12 && at.y.abs() < 1e-12 && (at.z + 16.0).abs() < 1e-12);
    let e = chuck::round_stock(Point3::new(0.0, 0.0, 0.0), Point3::new(30.0, 20.0, 10.0)).expect_err("not round");
    assert!(e.contains("only round stock"), "{e}");
    assert!(chuck::round_stock(Point3::new(0.0, 0.0, 0.0), Point3::new(f64::NAN, 1.0, 1.0)).is_err());
    assert_eq!(three_jaw().gripping(24.0).grip_diameter, 24.0);
    assert_eq!(collet().gripping(18.0).collet_bore, 18.0);
}

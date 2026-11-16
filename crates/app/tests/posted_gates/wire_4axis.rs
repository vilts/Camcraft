//! The 4-axis punch's CL golden (#537) judged by the laws its post
//! runs (#534): `wire_4axis_punch.cl` regenerated as `wire_4axis.rs`
//! makes it - a 16 mm square at Z0 lofted to a 9.6 mm radius circle at
//! Z12, each ring 64 points, cut as a punch from (-30, 0) paired by arc
//! length on the synthetic fixture's C110 (wire 0.25, gap 0.04) and 15
//! degree maximum - proved byte-identical, then judged with `taper:
//! true` (its rulings read as the wire's lean) against the loft's own
//! mesh sliced at 25 heights, more finely than the post's 9.
//!
//! Provenance: our own harness over our own goldens; no vendor material.

use camcraft_app::geo2d::{Loop, Point2};
use camcraft_app::geometry::{slice, Point3, Triangle};
use camcraft_app::verify::wire::{judge, CutSide, WireFinding, WireJob, WireSection, WireVerdict};
use camcraft_app::wire_edm::{wire_contour_ruled, WirePairing, WireParams, WireProfile, WireRuled, WireSide};

use super::golden;
use crate::common::gen_to_cltext;
use crate::common::solid_part::{rect, region};

const H: f64 = 12.0;
const N: usize = 64;
/// The judged heights through the part, both ends just inside.
const HEIGHTS: usize = 25;

type P2 = (f64, f64);

/// `wire_4axis.rs`'s square: half-side `a`, N points CCW from the
/// middle of its left side.
fn square(a: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let s = 8.0 * a * j as f64 / N as f64;
            match s {
                s if s <= a => (-a, -s),
                s if s <= 3.0 * a => (s - 2.0 * a, -a),
                s if s <= 5.0 * a => (a, s - 4.0 * a),
                s if s <= 7.0 * a => (6.0 * a - s, a),
                s => (-a, 8.0 * a - s),
            }
        })
        .collect()
}

/// `wire_4axis.rs`'s circle: radius `r`, N points CCW from its left.
fn circle(r: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let t = std::f64::consts::PI + std::f64::consts::TAU * j as f64 / N as f64;
            (r * t.cos(), r * t.sin())
        })
        .collect()
}

/// The loft's side band, as `wire_4axis.rs` lays it (the caps are never
/// sliced: the judged heights stand inside the part).
fn band(lo: &[P2], hi: &[P2]) -> Vec<Triangle> {
    let at = |(x, y): P2, z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for j in 0..lo.len() {
        let k = (j + 1) % lo.len();
        let (a, b, c, d) = (at(lo[j], 0.0), at(lo[k], 0.0), at(hi[k], H), at(hi[j], H));
        let mid = |p: Point3, q: Point3| ((p.x + q.x) * 0.5).hypot((p.y + q.y) * 0.5);
        if mid(a, c) <= mid(b, d) {
            f.extend([Triangle::new(a, b, c), Triangle::new(a, c, d)]);
        } else {
            f.extend([Triangle::new(a, b, d), Triangle::new(b, c, d)]);
        }
    }
    f
}

fn ring(pts: &[P2]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("a profile ring")
}

/// The wire laws' verdict on the 4-axis punch golden.
pub fn wire_4axis_verdict() -> WireVerdict {
    let start = Point2::new(-30.0, 0.0);
    let p = WireParams { wire_diameter: 0.25, gap: 0.04, condition: "C110".into(), side: WireSide::Punch, start: Some(start), plane_z: 0.0, tabs: None, stock: None, clearance: 1.0 };
    let (lower, upper) = (square(8.0), circle(9.6));
    let (lo, up) = ([ring(&lower)], [ring(&upper)]);
    let r = WireRuled { top_z: H, pairing: WirePairing::ArcLength, tolerance: 0.05, max_deg: Some(15.0), machine: "Generic ISO Wire EDM fixture".into() };
    let t = wire_contour_ruled(WireProfile { loops: &up, chains: &[] }, WireProfile { loops: &lo, chains: &[] }, &p, &r)
        .expect("the 4-axis punch generates");
    assert_eq!(gen_to_cltext(&t), golden("wire_4axis_punch.cl"), "wire_4axis_punch.cl: the regenerated program is the golden");
    let facets = band(&lower, &upper);
    let stock = region(&[rect(-40.0, -40.0, 40.0, 40.0)]);
    let sections = (0..HEIGHTS)
        .map(|i| {
            let z = 0.01 + (H - 0.02) * i as f64 / (HEIGHTS - 1) as f64;
            let part = slice::slice(&facets, z).expect("the loft's section");
            WireSection { z, part, stock: Some(stock.clone()) }
        })
        .collect();
    judge(&WireJob { path: &t, kerf_offset: 0.25 * 0.5 + 0.04, skim_offsets: Vec::new(), side: CutSide::Punch, start: Some(start), sections, taper: true, burnout: false, cutoff_from: None, capture: true })
}

/// The 4-axis punch golden's findings at the wire laws.
pub fn wire_4axis_findings() -> Vec<WireFinding> {
    wire_4axis_verdict().findings
}

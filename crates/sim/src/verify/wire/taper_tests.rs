//! The leaning wire's laws (#536, M3's review pins): a gouge at ANY
//! judged height blocks - the wire is read where it stands at that
//! height, not only on its programmed plane - and the kerf across a
//! leaning wire reaches `k * sqrt(1 + (t.u)^2)` toward the product, not
//! the vertical `k`; a pose on a cut stating no taper is refused.
//! Provenance: our own fixtures.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geometry::Point3;

const K: f64 = 0.2;

fn square(h: f64) -> Region {
    let pts = vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)];
    Region::new(vec![crate::geo2d::Loop::new(pts).expect("loop")]).expect("region")
}

/// A punch's closed square path of half-side `h` on Z0, threaded from
/// (-10, 0), every move leaning `t` per unit height (`None`: vertical,
/// no pose).
fn leaning_path(h: f64, t: Option<[f64; 2]>) -> Toolpath {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(-10.0, 0.0));
    let axis = t.map(|[x, y]| {
        let l = (x * x + y * y + 1.0).sqrt();
        [x / l, y / l, 1.0 / l]
    });
    b.set_axis(axis);
    for (x, y) in [(-h, 0.0), (-h, -h), (h, -h), (h, h), (-h, h), (-h, 0.0), (-10.0, 0.0)] {
        b.push(MoveKind::Feed, p(x, y), None);
    }
    b.finish()
}

fn judge_at(tp: &Toolpath, zs: &[f64], taper: bool) -> WireVerdict {
    let (part, stock) = (square(5.0), square(15.0));
    let sections = zs.iter().map(|&z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
    judge(&WireJob { path: tp, kerf_offset: K, skim_offsets: Vec::new(), side: CutSide::Punch, start: Some(Point2::new(-10.0, 0.0)), sections, taper, burnout: false, cutoff_from: None, capture: true })
}

fn gouges(v: &WireVerdict) -> Vec<f64> {
    v.findings.iter().filter_map(|f| if let WireFinding::Gouge { z, .. } = f { Some(*z) } else { None }).collect()
}

/// Only the UPPER section gouges: the path stands 0.4 mm off the part on
/// Z0 and leans 0.03 per mm toward it, so the left side's kerf is clear
/// at Z0.001 and Z6 (0.22 mm) and enters the part at Z11.999 (0.04 mm).
/// Reading the wire on its programmed plane at every height passes it.
#[test]
fn a_gouge_only_in_an_upper_section_blocks() {
    let tp = leaning_path(5.0 + K + 0.2, Some([0.03, 0.0]));
    let lower = judge_at(&tp, &[0.001, 6.0], true);
    assert!(lower.findings.is_empty(), "the lower sections are clear: {:?}", lower.findings);
    let v = judge_at(&tp, &[0.001, 6.0, 11.999], true);
    assert_eq!(gouges(&v), vec![11.999], "the upper section gouges, and that blocks: {:?}", v.findings);
    // The same plane path standing vertical is clean at every height.
    let upright = judge_at(&leaning_path(5.0 + K + 0.2, None), &[0.001, 6.0, 11.999], false);
    assert!(upright.findings.is_empty(), "{:?}", upright.findings);
}

/// The leaning kerf's reach: on the programmed plane (where the lean
/// moves nothing) the path stands 0.25 mm off the part - clear of a
/// vertical kerf of 0.2 - but the wire leans 45 degrees toward it, so
/// its kerf reaches 0.2 * sqrt(2) = 0.283 mm: a gouge.
#[test]
fn the_leaning_kerf_reaches_past_the_vertical_one() {
    let off = 5.0 + 0.25;
    let v = judge_at(&leaning_path(off, Some([1.0, 0.0])), &[0.0], true);
    let g = v.findings.iter().find_map(|f| if let WireFinding::Gouge { depth, .. } = f { Some(*depth) } else { None });
    let depth = g.unwrap_or_else(|| panic!("the leaning kerf gouges: {:?}", v.findings));
    assert!((depth - (K * 2.0_f64.sqrt() - 0.25)).abs() < 1e-9, "depth {depth}");
    // Upright, the same path is clear: the reach, not the position, bites.
    let upright = judge_at(&leaning_path(off, None), &[0.0], false);
    assert!(gouges(&upright).is_empty(), "{:?}", upright.findings);
}

/// A pose on a cut that states no taper is unreadable, never read as a
/// lean nor dropped: the laws refuse it by name.
#[test]
fn a_pose_on_a_cut_with_no_taper_is_refused() {
    let v = judge_at(&leaning_path(5.0 + K, Some([0.03, 0.0])), &[0.001], false);
    assert!(
        matches!(v.findings.as_slice(), [WireFinding::Unreadable(why)] if why.contains("states a pose, and the cut states no taper")),
        "{:?}",
        v.findings
    );
}

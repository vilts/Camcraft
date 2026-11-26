//! The wire punch's CL golden judged by the laws its post runs (#534):
//! `verify::wire::judge` - kerf, threading, slug, every finding binding -
//! as `posting_wire::wire_clause` hands it a cut. The program is
//! regenerated as golden_cl makes it (the 40 x 20 profile with 2 mm
//! corners, cut as a punch from (-30, 0) on its SYNTHETIC condition,
//! wire 0.25 and gap 0.04, from no machine's manual), proved byte-
//! identical to `wire_contour_punch.cl` and judged at full precision
//! (the CL text's micron grid is coarser than the kerf law's 0.1 um); its
//! product is that same profile through the 12 mm the wire passes, in a
//! 80 x 80 plate. (The posted `wire-iso/wire_slug_punch.nc` is the
//! document route's: `wire_post.rs` posts it through the gate itself.)
//!
//! The tapered punch (#536) the same way: `wire_taper_punch.cl`
//! regenerated as `wire_taper.rs` makes it (a sharp 40 x 20 block from
//! Z0 to Z12, cut as a punch from (-30, 0) at 3 degrees on the
//! synthetic fixture's C110 and 15 degree maximum), proved byte-
//! identical and judged with `taper: true` - its poses read as the
//! wire's lean at every judged height.
//!
//! Provenance: our own harness over our own goldens; no vendor material.

use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::wire_edm::{wire_contour, wire_contour_tapered, WireCorner, WireParams, WireSide, WireTaper};
use camcraft_app::verify::wire::{judge, CutSide, WireFinding, WireJob, WireSection, WireVerdict};

use super::golden;
use crate::common::gen_to_cltext;
use crate::common::solid_part::{rect, region};

/// golden_cl's profile: 40 x 20 with 2 mm corners in 3-degree facets, CCW.
fn outline() -> Loop {
    let (r, hx, hy) = (2.0, 18.0, 8.0);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push(Point2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    Loop::new(pts).expect("the profile loop")
}

/// The wire laws' verdict on the punch golden.
pub fn wire_verdict() -> WireVerdict {
    let start = Point2::new(-30.0, 0.0);
    let p = WireParams { wire_diameter: 0.25, gap: 0.04, condition: "C110".into(), side: WireSide::Punch, start: Some(start), plane_z: 0.0, stock: None, clearance: 1.0, tabs: None, sharp_corners: false };
    let t = wire_contour(&[outline()], &[], &p).expect("the punch generates");
    assert_eq!(gen_to_cltext(&t), golden("wire_contour_punch.cl"), "wire_contour_punch.cl: the regenerated program is the golden");
    let part = Region::new(vec![outline()]).expect("the profile region");
    let stock = region(&[rect(-40.0, -40.0, 40.0, 40.0)]);
    let sections = [0.01, 6.0, 11.99].into_iter().map(|z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
    judge(&WireJob { path: &t, kerf_offset: 0.25 * 0.5 + 0.04, skim_offsets: Vec::new(), side: CutSide::Punch, start: Some(start), sections, burnout: false, cutoff_from: None, capture: true, taper: false, verify_tol: 0.0, taper_chord: false })
}

/// The punch golden's findings at the wire laws, each one blocking.
pub fn wire_findings() -> Vec<WireFinding> {
    wire_verdict().findings
}

/// The wire laws' verdict on the tapered punch golden (#536).
pub fn wire_taper_verdict() -> WireVerdict {
    let start = Point2::new(-30.0, 0.0);
    let p = WireParams { wire_diameter: 0.25, gap: 0.04, condition: "C110".into(), side: WireSide::Punch, start: Some(start), plane_z: 0.0, stock: None, clearance: 1.0, tabs: None, sharp_corners: false };
    let part = region(&[rect(-20.0, -10.0, 20.0, 10.0)]);
    let taper = WireTaper { angle_deg: 3.0, top_z: 12.0, max_deg: Some(15.0), machine: "Generic ISO Wire EDM fixture".into(), corner: WireCorner::Mitre };
    let t = wire_contour_tapered(part.loops(), &[], &p, &taper).expect("the tapered punch generates");
    assert_eq!(gen_to_cltext(&t), golden("wire_taper_punch.cl"), "wire_taper_punch.cl: the regenerated program is the golden");
    let stock = region(&[rect(-40.0, -40.0, 40.0, 40.0)]);
    let sections = [0.01, 6.0, 11.99].into_iter().map(|z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
    judge(&WireJob { path: &t, kerf_offset: 0.25 * 0.5 + 0.04, skim_offsets: Vec::new(), side: CutSide::Punch, start: Some(start), sections, taper: true, burnout: false, cutoff_from: None, capture: true, verify_tol: 0.0, taper_chord: false })
}

/// The tapered punch golden's findings at the wire laws.
pub fn wire_taper_findings() -> Vec<WireFinding> {
    wire_taper_verdict().findings
}

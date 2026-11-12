//! #842: the cross drill's part law sweeps the drill's modelled body by
//! its band stack - a stepped body whose flange stands clear of the part
//! is no longer refused, one whose flange meets it still is, and a tool
//! with no body reads exactly as `Cutter::of` did.
//! Provenance: our own box fixtures.

use super::*;
use crate::cl::PathBuilder;
use crate::geometry::cube;
use camcraft_tools::holder_profile::{HolderBand, HolderStack};

/// DR-0400 with 20 of reach (flute 12, shank 8) under the Ø44 x 30
/// holder enclosing its stepped body.
fn tool() -> Tool {
    let base = crate::tools::catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t);
    let Some(base) = base else { panic!("catalog DR-0400") };
    Tool { flute_len: 12.0, shank_len: 8.0, holder_d: 44.0, holder_len: 30.0, ..base }
}

/// The stepped body: an r 6 nose 20..40 above the tip, an r 22 flange
/// 40..50 - carried the way `operation::generate` builds every cutter.
fn stepped() -> verify::Cutter {
    let t = tool();
    let g = t.geom();
    let stack = HolderStack {
        bands: vec![HolderBand { above: 20.0, len: 20.0, r: 6.0 }, HolderBand { above: 40.0, len: 10.0, r: 22.0 }],
        top: 50.0,
        neck: Vec::new(),
    };
    let c = verify::Cutter::of(&t).with_body(&g, &stack);
    assert!(c.body.is_some(), "the stepped stack fits");
    c
}

/// An ear of the finished part beside the +X hole line: y 8..30, out
/// along the line to x `ear`.
fn ear(ear: f64) -> Vec<Triangle> {
    cube(Point3::new(10.0, 8.0, -10.0), Point3::new(ear, 30.0, 10.0)).facets.to_vec()
}

fn hole() -> CrossHole {
    CrossHole { origin: Point3::new(10.0, 0.0, 0.0), dir: Point3::new(-1.0, 0.0, 0.0), radius: 2.0, entry_d: 0.0, floor_d: -3.0, through: false }
}

/// The plunge down the hole line from x 60 to the floor at x 7, and out.
fn plunge() -> Toolpath {
    let p = Point3::new(60.0, 0.0, 0.0);
    let mut b = PathBuilder::new(p);
    b.set_axis(Some([1.0, 0.0, 0.0]));
    b.push(MoveKind::Feed, Point3::new(7.0, 0.0, 0.0), Some(80.0));
    b.push(MoveKind::Feed, p, Some(80.0));
    b.finish()
}

fn judge(ear_x: f64, cutter: &verify::Cutter) -> Result<(), String> {
    part_clear(&plunge(), &ear(ear_x), &tool(), cutter, &[hole()]).map_err(|e| e.to_string())
}

#[test]
fn a_stepped_body_whose_flange_clears_the_part_is_not_refused_by_the_cross_part_law() {
    // The enclosing cylinder (r 22 from x 27 out) stands in the ear (to
    // x 32): refused. The nose (r 6) misses the ear at y 8, the flange
    // (from x 47) stands past it.
    let plain = verify::Cutter::of(&tool());
    let e = judge(32.0, &plain).err().unwrap_or_else(|| panic!("the enclosing cylinder met the ear"));
    assert!(e.contains("rubs its shank or holder on the finished part"), "{e}");
    assert_eq!(judge(32.0, &stepped()), Ok(()), "the nose misses, the flange stands past the ear");
}

#[test]
fn a_stepped_body_whose_flange_meets_the_part_is_still_refused_by_the_cross_part_law() {
    let e = judge(55.0, &stepped()).err().unwrap_or_else(|| panic!("the flange met the long ear"));
    assert!(e.contains("rubs its shank or holder on the finished part"), "{e}");
}

#[test]
fn a_tool_with_no_body_reads_the_cutter_it_always_did() {
    let t = tool();
    let g = t.geom();
    let plain = verify::Cutter::of(&t);
    // The plain holder cylinder is never carried; the judged cutter is
    // the very value `Cutter::of` gave before.
    assert_eq!(plain.with_body(&g, &HolderStack::cylinder(&g)), plain);
    assert_eq!(verify::Cutter::judged(&g, Some(plain)), plain);
    for x in [32.0, 55.0] {
        assert_eq!(judge(x, &plain), judge(x, &plain.with_body(&g, &HolderStack::cylinder(&g))), "ear to x {x}");
    }
}

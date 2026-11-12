//! #842: the drill-reach holder laws read a modelled body's band stack.
//! A stepped body (a narrow nose under a wide flange) is no longer
//! refused for a flange that stands clear of the material, a flange that
//! does reach it is still refused - naming the band - and a tool with no
//! body reads bit-identically to the one holder cylinder.
//! Provenance: our own box fixtures; textbook geometry.

use super::*;
use crate::cl::{Cycle, CycleKind, MoveKind, PathBuilder};
use crate::geometry::cube;
use camcraft_tools::holder_profile::HolderStack;

fn boxes(b: &[([f64; 3], [f64; 3])]) -> Vec<Triangle> {
    b.iter()
        .flat_map(|(lo, hi)| cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.to_vec())
        .collect()
}

/// DR-0400 on a 24 mm flute and 2 mm of shank, its stated holder the
/// Ø40 x 35 cylinder enclosing the stepped body (face 26 above the tip).
fn drill() -> Tool {
    let base = crate::tools::catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t);
    let Some(base) = base else { panic!("catalog DR-0400") };
    Tool { flute_len: 24.0, shank_len: 2.0, holder_d: 40.0, holder_len: 35.0, ..base }
}

/// The stepped body over `drill`: an r 6 nose 26..`step` above the tip,
/// an r 20 flange `step`..61.
fn stepped(tool: &Tool, step: f64) -> BodyBands {
    let g = tool.geom();
    let stack = HolderStack {
        bands: vec![HolderBand { above: 26.0, len: step - 26.0, r: 6.0 }, HolderBand { above: step, len: 61.0 - step, r: 20.0 }],
        top: 61.0,
        neck: Vec::new(),
    };
    let Some(b) = BodyBands::of(&g, &stack) else { panic!("the stepped stack fits its enclosing cylinder") };
    b
}

/// A slab (top z -30) under the hole at the origin and a wall block
/// x 10..30 standing to z 0 beside it.
fn slab_and_wall() -> Vec<Triangle> {
    boxes(&[([-50.0, -50.0, -40.0], [50.0, 50.0, -30.0]), ([10.0, -20.0, -30.0], [30.0, 20.0, 0.0])])
}

/// One drill cycle at the origin from the slab's top to `bottom`.
fn cycle_to(bottom: f64) -> Toolpath {
    let c = Cycle {
        kind: CycleKind::Drill,
        xy: Point2::new(0.0, 0.0),
        r_z: -28.0,
        bottom_z: bottom,
        entry_z: -30.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
    };
    Toolpath { cycles: vec![c], ..Toolpath::default() }
}

fn axial(bottom: f64, body: Option<&BodyBands>) -> Result<(), String> {
    axial_reach(&cycle_to(bottom), &[], &slab_and_wall(), None, None, (&drill(), body), "T1").map_err(|e| e.to_string())
}

#[test]
fn a_stepped_body_whose_flange_clears_the_wall_is_not_refused_by_the_axial_holder_law() {
    // 10 mm into the slab: the enclosing cylinder's face (z -14, r 20)
    // stands under the wall's top (z 0) - refused. The body's nose (r 6,
    // face z -14) is clear of the wall at x 10; its flange (face z 6)
    // stands above it.
    let tool = drill();
    let body = stepped(&tool, 46.0);
    let e = axial(-40.0, None).err().unwrap_or_else(|| panic!("the enclosing cylinder sank beside the wall"));
    assert!(e.contains("T1's holder (dia. 40.000, 26.000 mm above the tip) sinks to Z -14.000"), "{e}");
    assert_eq!(axial(-40.0, Some(&body)), Ok(()), "the nose clears, the flange stands above the wall");
}

#[test]
fn a_stepped_body_whose_flange_sinks_beside_the_wall_is_still_refused_by_the_axial_holder_law() {
    // The flange starts 36 above the tip: at the same hole its face is
    // z -4, under the wall's top.
    let tool = drill();
    let e = axial(-40.0, Some(&stepped(&tool, 36.0))).err().unwrap_or_else(|| panic!("the flange sank beside the wall"));
    assert!(e.contains("T1's body band (dia. 40.000, 36.000 mm above the tip) sinks to Z -4.000"), "{e}");
    assert!(e.contains("below the material top 0.000"), "{e}");
    // The nose alone: a hole drilled where the wall stands 4 mm off the
    // axis convicts the r 6 nose (face z -14) though the flange clears.
    let near = boxes(&[([-50.0, -50.0, -40.0], [50.0, 50.0, -30.0]), ([4.0, -20.0, -30.0], [30.0, 20.0, 0.0])]);
    let e = axial_reach(&cycle_to(-40.0), &[], &near, None, None, (&tool, Some(&stepped(&tool, 46.0))), "T1").err();
    let e = e.map(|e| e.to_string()).unwrap_or_else(|| panic!("the nose sank beside the near wall"));
    assert!(e.contains("T1's body band (dia. 12.000, 26.000 mm above the tip)"), "{e}");
}

#[test]
fn a_tool_with_no_body_reads_the_holder_cylinder_bit_identically() {
    let tool = drill();
    let g = tool.geom();
    // The plain cylinder is never carried as a body.
    assert_eq!(BodyBands::of(&g, &HolderStack::cylinder(&g)), None);
    let Ok((bands, what)) = stack_bands(&tool, None, "T1") else { panic!("a sane holder") };
    assert_eq!((bands, what), (vec![Band { above: 26.0, r: 20.0, len: 35.0 }], "holder"));
    // A carried stack that no longer fits the record (wider than its
    // holder) is not trusted: the cylinder stands in.
    let wide = HolderStack { bands: vec![HolderBand { above: 26.0, len: 35.0, r: 25.0 }], top: 61.0, neck: Vec::new() };
    assert_eq!(BodyBands::of(&g, &wide), None, "wider than the enclosing holder");
    // No holder modelled sweeps nothing, body or not.
    let bare = Tool { holder_d: 0.0, ..tool.clone() };
    assert!(matches!(stack_bands(&bare, Some(&stepped(&tool, 46.0)), "T1"), Ok((b, _)) if b.is_empty()));
    // The refusal reads as it always did.
    let e = axial(-40.0, None).err().unwrap_or_default();
    assert_eq!(
        e,
        "invalid machining parameters: tool T1's holder (dia. 40.000, 26.000 mm above the tip) sinks to Z -14.000 \
         at the hole at (0.000, 0.000), below the material top 0.000 around it - the holder feeds into the stock \
         (a collision); use a longer tool or a slimmer holder"
    );
}

// ---- the cross drill's holder law -----------------------------------

/// A core x -10..10 (the hole enters it from +X 3 mm) and an ear off to
/// the side: y 8..30, standing out along the line to x `ear`.
fn core_and_ear(ear: f64) -> Vec<Triangle> {
    boxes(&[([-10.0, -10.0, -10.0], [10.0, 10.0, 10.0]), ([10.0, 8.0, -10.0], [ear, 30.0, 10.0])])
}

/// DR-0400 with 20 of reach (flute 12, shank 8) under a Ø44 x 30
/// enclosing holder; its stepped body an r 6 nose 20..`step`, then an
/// r 22 flange to 50.
fn cross_tool() -> Tool {
    Tool { flute_len: 12.0, shank_len: 8.0, holder_d: 44.0, holder_len: 30.0, ..drill() }
}

fn cross_body(step: f64) -> BodyBands {
    let g = cross_tool().geom();
    let stack = HolderStack {
        bands: vec![HolderBand { above: 20.0, len: step - 20.0, r: 6.0 }, HolderBand { above: step, len: 50.0 - step, r: 22.0 }],
        top: 50.0,
        neck: Vec::new(),
    };
    let Some(b) = BodyBands::of(&g, &stack) else { panic!("the stepped stack fits") };
    b
}

/// The +X station's plunge: from x 60 in to the floor at x 7, and back.
fn plunge() -> Toolpath {
    let p = Point3::new(60.0, 0.0, 0.0);
    let mut b = PathBuilder::new(p);
    b.set_axis(Some([1.0, 0.0, 0.0]));
    b.push(MoveKind::Rapid, Point3::new(7.0, 0.0, 0.0), None);
    b.push(MoveKind::Rapid, p, None);
    b.finish()
}

fn cross(ear: f64, body: Option<&BodyBands>) -> Result<(), String> {
    cross_holder(&plunge(), &core_and_ear(ear), None, (0.0, 0.0), &cross_tool(), body, "T1").map_err(|e| e.to_string())
}

#[test]
fn a_stepped_body_whose_flange_clears_the_ear_is_not_refused_by_the_cross_holder_law() {
    // The enclosing cylinder's face stands at x 27, inside the ear (out
    // to x 32 within its r 22): refused. The nose (r 6) misses the ear
    // at y 8; the flange's face stands at x 47, past it.
    let e = cross(32.0, None).err().unwrap_or_else(|| panic!("the enclosing cylinder met the ear"));
    assert!(e.contains("T1's holder (dia. 44.000, 20.000 mm behind the tip) stands 27.000 mm out"), "{e}");
    assert_eq!(cross(32.0, Some(&cross_body(40.0))), Ok(()), "the nose misses the ear, the flange stands past it");
}

#[test]
fn a_stepped_body_whose_flange_meets_the_ear_is_still_refused_by_the_cross_holder_law() {
    // An ear out to x 55: the flange's face (x 47) stands inside it.
    let e = cross(55.0, Some(&cross_body(40.0))).err().unwrap_or_else(|| panic!("the flange met the long ear"));
    assert!(e.contains("T1's body band (dia. 44.000, 40.000 mm behind the tip) stands 47.000 mm out"), "{e}");
    // No body: the cylinder reading, unchanged.
    let e = cross(55.0, None).err().unwrap_or_default();
    assert!(e.contains("T1's holder (dia. 44.000, 20.000 mm behind the tip) stands 27.000 mm out"), "{e}");
}

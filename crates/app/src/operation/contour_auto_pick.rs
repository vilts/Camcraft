//! Side Auto of a picked Contour (#1933): the heights the picked rings
//! stand at and the material they belong to, read from the world and
//! handed to the level law (`machining2d::auto_side`). Provenance: our
//! own design notes on 2.5D profiling; no vendor material consulted.

use super::*;

/// The Z span of the picks that spell `loops`: every closed chain, every
/// open run the stitcher closed into a ring (`stitched`), every closed
/// face loop and every closed patch ring - in the setup frame, or along
/// the tilted frame's axis. `None` when no pick carries a point.
fn pick_span(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    stitched: &HashMap<usize, usize>,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Option<(f64, f64)> {
    let opframe = OpFrame::new(setup.map(str::to_owned), None);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (gi, g) in items.iter().enumerate() {
        let Some(entity) = model::find_by_name(world, &g.entity) else { continue };
        let Some(features) = model::features_of(world, &entity) else { continue };
        let loc = opframe.pose_in_setup(world, &entity).unwrap_or_default();
        let verts: Vec<[f64; 3]> = match &g.feature {
            FeatureSel::Chain(cid) => match crate::model::resolve_chain(&features, *cid) {
                Some(c) if c.closed || stitched.contains_key(&gi) => feature::chain_points(&features, &c),
                _ => Vec::new(),
            },
            FeatureSel::Face(fid) => features
                .faces
                .iter()
                .filter(|f| f.id == *fid)
                .flat_map(|f| f.loops.iter().filter(|l| !matches!(l.kind, LoopKind::Open)))
                .flat_map(|l| l.verts.iter().filter_map(|&vi| features.verts.get(vi as usize).copied()))
                .collect(),
            FeatureSel::Patch(pid) => feature::patch_rings(&features, *pid)
                .unwrap_or_default()
                .iter()
                .filter(|r| r.closed)
                .flat_map(|r| r.verts.iter().filter_map(|&vi| features.verts.get(vi as usize).copied()))
                .collect(),
            FeatureSel::Vertex(_)
            | FeatureSel::SketchChain(_)
            | FeatureSel::SketchLayer(_) => Vec::new(),
        };
        for p in verts {
            let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
            let z = frame.map_or(w.z, |f| f.z_local(w));
            if z.is_finite() {
                (lo, hi) = (lo.min(z), hi.max(z));
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Side Auto for the rings `loops` that the picks `items` spell. No
/// readable material, or no pick height, keeps the old answer: Outside.
/// The part's XY outline rides along for rings with no material at
/// their level (a sheet part): there the outline decides, as before.
pub(super) fn auto_side_of_picks(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    loops: &[Loop],
    stitched: &HashMap<usize, usize>,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<machining2d::AutoSide, GenError> {
    let Some((lo, hi)) = pick_span(world, setup, items, stitched, frame) else {
        return Ok(machining2d::AutoSide::Outside);
    };
    let Ok(m) = material_of(world, setup, &SrcRef::Selection(items.to_vec())) else {
        return Ok(machining2d::AutoSide::Outside);
    };
    let facets = match frame {
        Some(f) => f.local_facets(&m.facets),
        None => m.facets,
    };
    let rings = Region::new(loops.to_vec())?;
    machining2d::auto_side(&rings, &facets, &m.bodies, || silhouette(&facets), lo, hi)
}

//! Resolve placed sketch centerlines for Trace and Engrave. Provenance: CamCraft's
//! sketch selection, stock top-face placement, and planar milling commands (#1869).

use flecs_ecs::prelude::World;

use super::contour_sketch_pick::visit_src_chains;
use super::{FeatureSel, OpParams, SrcRef};
use crate::geo2d::Point2;
use crate::geometry::{Point3, quat_rotate_point, transform_point};
use crate::machining2d::{GenError, TraceStrokes};

pub(super) fn resolve(
    world: &World,
    src: &SrcRef,
    params: &OpParams,
) -> Option<Result<TraceStrokes, GenError>> {
    let (OpParams::Trace { climb, .. } | OpParams::Engrave { climb, .. }) = params else {
        return None;
    };
    let profiles = crate::sketch_model::source_profiles(world, src);
    if profiles.is_empty() {
        return None;
    }
    // A whole drawing or layer must not silently omit rejected primitives.
    for name in profiles {
        let (_, sketch) = crate::sketch::find(world, &name)?;
        for rejected in &sketch.data.chains().rejected {
            let selected = match src {
                SrcRef::Entity(_) => true,
                SrcRef::Selection(items) => items.iter().any(|g| {
                    g.entity == name
                        && matches!(&g.feature, FeatureSel::SketchLayer(l) if l == &rejected.layer)
                }),
                SrcRef::Stock => false,
            };
            if selected {
                return Some(Err(GenError::Invalid(format!(
                    "DXF profile '{name}' has unusable geometry on layer '{}': {}",
                    rejected.layer, rejected.error
                ))));
            }
        }
    }

    let mut strokes = TraceStrokes::default();
    let result = visit_src_chains(world, src, |name, pose, chain, _| {
        let pose = pose.ok_or_else(|| {
            GenError::Invalid(format!("DXF profile '{name}' has no stock to lie on"))
        })?;
        let up = quat_rotate_point(pose.rot, Point3::new(0.0, 0.0, 1.0));
        if !up.is_finite() || up.x.hypot(up.y) > 1e-9 || up.z <= 0.0 {
            return Err(GenError::Invalid(format!(
                "DXF profile '{name}' must lie on a horizontal stock face"
            )));
        }
        strokes.add_chain(chain, *climb, |p| {
            let q = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
            Point2::new(q.x, q.y)
        })
    });
    Some(result.map(|()| strokes))
}

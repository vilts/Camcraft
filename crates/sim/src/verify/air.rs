//! The all-air verdict (M13.8): a finish whose every cutting move stands upright above all material provably cuts nothing - our own law on the tip-up tool stack of the RS-274 tool-length convention; no vendor material consulted.

use super::upright::is_upright;
use crate::cl::{MoveKind, Toolpath};
use crate::geometry::Triangle;
use crate::tools::ToolGeom;

/// Whether `path` provably never touches material: it has at least one
/// cutting move, and every cutting move is upright (no pose, or a
/// vertical one) with its lowest tool point at or above the material
/// top plus `tol` - the material top being the higher of `stock_top_z`
/// and the model's highest vertex. An upright tool stack rises from its
/// tip, so nothing of it reaches below that plane; a nose-centre
/// turning path (`tool.nose_radius` > 0) is the nose's centre, whose
/// insert reaches `nose_radius` below it. Rapids are not cuts and are
/// judged by the rapid laws.
///
/// `false` means "not proven", never "cuts": a posed move, an arc in a
/// vertical plane (its Z between the endpoints is not their range), a
/// non-finite height, or any cut below the plane leaves the verdict to
/// the other laws. This is a statement the envelope laws cannot make -
/// they speak only where the swept ball meets the part, so an all-air
/// pass is silent there, which reads as clean without being judged.
#[must_use]
pub fn cuts_no_material(
    path: &Toolpath,
    model: Option<&[Triangle]>,
    stock_top_z: f64,
    tool: &ToolGeom,
    tol: f64,
) -> bool {
    let model_top = model
        .unwrap_or(&[])
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold(f64::NEG_INFINITY, f64::max);
    let top = stock_top_z.max(model_top);
    if !top.is_finite() || !tol.is_finite() {
        return false;
    }
    let below = if tool.nose_radius.is_finite() && tool.nose_radius > 0.0 {
        tool.nose_radius
    } else {
        0.0
    };
    let floor = top + tol.max(0.0);
    let mut cuts = 0usize;
    for m in &path.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        cuts += 1;
        let upright = m.axis.is_none_or(|a| is_upright(&a));
        let planar = m.arc.is_none_or(|a| a.plane == crate::cl::ArcPlane::Xy);
        let low = m.from.z.min(m.to.z) - below;
        if !(upright && planar && low.is_finite() && low >= floor) {
            return false;
        }
    }
    cuts > 0
}

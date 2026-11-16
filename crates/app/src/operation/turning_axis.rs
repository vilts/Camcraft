//! #1408: the spindle a turning nose path's envelope verdict turns the work about - the turning kernels' own fitted axis (`turning::Frame`, our axis-fit law) handed to the verify context; no vendor material consulted.

use crate::geometry::Triangle;
use crate::operation::{Family, OpParams};
use crate::stock_sim::LatheAxis;
use crate::tools::ToolGeom;

/// The spindle of a turning op cut by a nose (`nose_radius` > 0) - the
/// axis its kernel fitted to `facets`, read the way the lathe sim reads
/// it (`stock_sim_lathe`). `None` for every other op, and for a source
/// no axis fits: the verdict then reads as it always did, never a
/// guessed spindle.
pub(super) fn lathe_axis_of(params: &OpParams, geom: &ToolGeom, facets: &[Triangle]) -> Option<LatheAxis> {
    if params.kind().info().family() != Family::Turn || !(geom.nose_radius.is_finite() && geom.nose_radius > 0.0) {
        return None;
    }
    let frame = crate::turning::profile::Frame::about(facets).ok()?;
    let (o, p) = (frame.point(0.0, 0.0), frame.point(0.0, 1.0));
    LatheAxis::new(o, [p.x - o.x, p.y - o.y, p.z - o.z]).ok()
}

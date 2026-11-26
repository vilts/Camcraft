//! Issue #130: the setup stock read as the turning kernels' revolved envelope - the bar's widest radius about the part's own fitted axis (and, #229, its far end along that axis), the reading the meridian rapid law (#103) judges against; the shop rule that a lathe tool approaches clear of the bar's OD (machine-tool handbooks); our own design, no vendor material consulted.

use crate::cl::Toolpath;
use crate::geometry::Triangle;
use crate::turning::profile::{profile_of, Frame};
use crate::turning::StockEnv;

use super::OpParams;

/// The radial gap every turning rapid keeps over the bar's OD (mm) -
/// a shop's habitual millimetre on radius, wide of the meridian law's
/// outline tolerance and gouge margin by two orders.
pub(super) const STOCK_CLEARANCE: f64 = 1.0;

/// Does `params` name a kernel that approaches from the stock (#130)?
/// Facing (#122) keeps its own approach; contour rides also read stock (#323).
fn reads_stock(params: &OpParams) -> bool {
    matches!(
        params,
        OpParams::ProfileRough { .. }
            | OpParams::ProfileFinish { .. }
            | OpParams::GrooveRough { .. }
            | OpParams::GrooveFinish { .. }
            | OpParams::Part { .. }
            | OpParams::Threading { .. }
            | OpParams::FormTurning { .. }
            | OpParams::Chamfering { .. }
            | OpParams::TaperTurning { .. }
            | OpParams::MultiPassTurning { .. }
    )
}

/// The stock envelope for a turning kernel, and the note the path
/// carries when no stock could be read (the part-based approach then
/// stands, as it did before #130). Kinds that do not read the stock
/// get neither.
pub(super) fn read(params: &OpParams, part: &[Triangle], stock: Option<&[Triangle]>) -> (Option<StockEnv>, Option<String>) {
    if !reads_stock(params) {
        return (None, None);
    }
    let env = stock.filter(|s| !s.is_empty()).and_then(|s| {
        let f = Frame::about(part).ok()?;
        // The stock read in the PART's frame - the one the kernel walks
        // - so its far end (#229) is a station the kernel's windows share.
        let sp = profile_of(s, &f).ok()?;
        let r = sp.r_max;
        // #3452: and its near end, so a part-off kerf past the part's
        // end can tell bar from air.
        let s_bot = s.iter().flat_map(|t| [t.a, t.b, t.c]).map(|q| f.sr(q).0).fold(f64::INFINITY, f64::min);
        (r.is_finite() && r > 0.0).then_some(StockEnv { r_out: r, clearance: STOCK_CLEARANCE, s_top: sp.z_top, s_bot })
    });
    match env {
        Some(e) => (Some(e), None),
        None => (
            None,
            Some(
                "no setup stock could be read about the spindle - the approach and retract \
                 clear the part only; set the setup's stock so the lathe approaches from the bar"
                    .into(),
            ),
        ),
    }
}

/// The path with the no-stock note, once.
pub(super) fn noted(mut path: Toolpath, note: Option<String>) -> Toolpath {
    if let Some(n) = note
        && !path.notes.contains(&n)
    {
        path.notes.push(n);
    }
    path
}

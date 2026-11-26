//! #3525: a 2D milling operation that cuts deeper below the stock top than
//! its tool's flute length is named by that cause, FIRST. The law itself is
//! `camcraft_sim::verify::flute_reach`; this file picks the operations and
//! places the finding. Provenance: the shop rule that the depth of cut
//! stays within the flute length (machine-tool handbooks); our own reading.
//!
//! - Always a note, first in the notes, with the tool's name.
//! - A warning, first in the warnings, only when the shank and holder laws
//!   convicted the body on this path: then it explains the findings after
//!   it. Without them, the body passed beside every wall, and the note is
//!   the whole word (#3525 decision: the collision laws decide the post).
//! - Never a reason the post is held.

use crate::cl::Toolpath;
use crate::operation::OpParams;
use crate::tools::ToolGeom;
use crate::verify::{self, Warning};

/// The 2D milling kinds whose depth is a stack of levels down from the
/// stock: the pocket family and the 2D contour.
fn judged(params: &OpParams) -> bool {
    matches!(
        params,
        OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. } | OpParams::Contour { .. }
    )
}

/// The findings of the body above the flutes: the shank and holder laws.
fn body_hit(w: &Warning) -> bool {
    matches!(w, Warning::ShankHit { .. } | Warning::HolderHit { .. } | Warning::SweepHit { .. })
}

/// Put the flute-reach note first in the path's notes and, when the body
/// was convicted, its warning first in `warns`. Nothing for another kind,
/// or when the cut stays within the flute length.
pub(super) fn name_first(params: &OpParams, path: &mut Toolpath, warns: &mut Vec<Warning>, geom: &ToolGeom, tool_name: &str, stock_top_z: f64) {
    if !judged(params) {
        return;
    }
    let Some(w) = verify::flute_reach(path, geom, stock_top_z) else {
        return;
    };
    if let Some(note) = verify::flute_note(tool_name, &w) {
        path.notes.insert(0, note);
    }
    if warns.iter().any(body_hit) {
        warns.insert(0, w);
    }
}

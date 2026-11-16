//! The edge-break bound (#529, follow-up to #303): the program sim
//! credits a declared bevel only within what its OWN operation asked
//! for. A cached record is file data - a hand-edited or corrupted
//! project could state any depth, and a deeper record would forgive a
//! real gouge under it. So each record is held to its op's stated
//! break width (`Chamfer2D` / `Deburr` `width`: the bevel's plan width
//! at the edge, `depth / slope` in the record's terms), and a record on
//! any other kind is not a declaration at all. A record over the bound
//! is dropped by name in the log; its columns are then judged against
//! the sharp model - stricter, never looser.
//!
//! Provenance: our own bound over this repo's #303 record.

use crate::cl::EdgeBreak;
use crate::operation::OpParams;

/// The plan width op `params` declares for its edge breaks, or `None`
/// for a kind that declares none.
#[must_use]
pub fn declared_width(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Chamfer2D { width, .. } | OpParams::Deburr { width, .. } => Some(*width),
        _ => None,
    }
}

/// The records of op `name` the sim may credit: sane, on a declaring
/// kind, and no wider than the op's stated width (a micron of slack for
/// the lattice the file stores the edge on).
#[must_use]
pub fn bounded(name: &str, params: &OpParams, breaks: Vec<EdgeBreak>) -> Vec<EdgeBreak> {
    if breaks.is_empty() {
        return breaks;
    }
    let Some(w) = declared_width(params).filter(|w| w.is_finite() && *w > 0.0) else {
        tracing::warn!(op = name, kind = %params.kind(), "edge-break records on a kind that declares none - dropped, judged sharp");
        return Vec::new();
    };
    let slack = 1e-6 * w.max(1.0);
    breaks
        .into_iter()
        .filter(|b| {
            let ok = b.is_sane() && b.width() <= w + slack;
            if !ok {
                tracing::warn!(
                    op = name,
                    declared = w,
                    record = b.width(),
                    "edge-break record has invalid dimensions, profile or frame, or is wider than its operation declares - dropped, judged sharp"
                );
            }
            ok
        })
        .collect()
}

//! #1732: the hand-typed A and B points of a Slot, in the viewport while
//! they are edited. Read off the open page's DRAFT (`State::op_ui.params`),
//! so each keystroke in A X / A Y / B X / B Y moves the marks before Apply;
//! ephemeral display state - nothing here writes a command, and the
//! committed edit still rides `EditOperationParams`. Drawn only for a Slot
//! with no picked selection (a pick seeds A and B itself) and only while
//! its Geometry tab is open. A top Z that does not resolve draws nothing:
//! never a guessed height.
//!
//! Provenance: our own display design; no vendor material consulted.

use camcraft_app::operation::{self, OpParams};

use crate::egui_app::ground_ink::{Overlay, overlay};
use crate::egui_app::op_tabs::SEC_GEO;
use crate::interact::project_uv;
use crate::state::{State, UiTarget};

/// What a mark says, for the tests and the draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotMarkKind {
    /// The slot's centre line, A to B.
    Line,
    /// The cross at point A.
    PointA,
    /// The cross at point B.
    PointB,
}

/// One screen-space stroke, viewport uv (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotMark {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub rgb: [f32; 3],
    pub width: f32,
    pub kind: SlotMarkKind,
}

/// Points A and B with the Z they stand at, when the marks draw this
/// frame: an open Slot operation, no picked selection, the Geometry tab
/// shown, finite coordinates, a resolvable top Z.
#[must_use]
pub fn draft_points(state: &State) -> Option<([f64; 3], [f64; 3])> {
    if !state.panels_open.properties
        || !state.properties_shown
        || state.pane_target() != UiTarget::Operation(state.op_ui.name.clone())
        || state.op_ui.name.is_empty()
        || !state.op_ui.geo.is_empty()
        || state.op_tab != SEC_GEO
    {
        return None;
    }
    let OpParams::Slot { a, b, top_z, tilt, .. } = &state.op_ui.params else {
        return None;
    };
    if ![a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()) {
        return None;
    }
    let op = camcraft_app::model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| camcraft_app::model::operation_of(&state.world, &e))?;
    let z = operation::resolve_height(&state.world, op.setup.as_deref(), Some(&op.src), *top_z)?;
    let frame = match tilt {
        Some(tilt) => Some(tilt.frame()?),
        None => None,
    };
    // Match generation: typed X/Y are in the cutting plane, and resolved
    // heights shift by origin Z before the frame maps them to world space.
    let world = |p: &camcraft_app::geo2d::Point2| {
        let p = camcraft_app::geometry::Point3::new(p.x, p.y, z - frame.map_or(0.0, |f| f.origin.z));
        let p = frame.map_or(p, |f| f.to_world(p));
        [p.x, p.y, p.z]
    };
    Some((world(a), world(b)))
}

/// Every mark this frame (empty when [`draft_points`] is None).
#[must_use]
pub fn slot_marks(state: &State, aspect: f64) -> Vec<SlotMark> {
    let mut out = Vec::new();
    let Some((pa, pb)) = draft_points(state) else {
        return out;
    };
    let mvp = state.cam.mvp(aspect);
    let rgb = overlay(Overlay::Pick);
    let mut push = |p: [f64; 3], q: [f64; 3], width, kind| {
        if let (Some(a), Some(b)) = (project_uv(&mvp, p), project_uv(&mvp, q))
            && a != b
        {
            out.push(SlotMark { a, b, rgb, width, kind });
        }
    };
    push(pa, pb, 2.0, SlotMarkKind::Line);
    let s = ((pa[0] - pb[0]).hypot(pa[1] - pb[1]) * 0.04).clamp(0.5, 4.0);
    for (p, kind) in [(pa, SlotMarkKind::PointA), (pb, SlotMarkKind::PointB)] {
        push([p[0] - s, p[1], p[2]], [p[0] + s, p[1], p[2]], 3.0, kind);
        push([p[0], p[1] - s, p[2]], [p[0], p[1] + s, p[2]], 3.0, kind);
    }
    out
}

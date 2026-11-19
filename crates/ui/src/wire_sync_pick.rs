//! The Wire Contour page's sync-point pick (#1171, part of #290): while
//! the pick is armed, a viewport Vertex click states one point of a
//! 4-axis cut's sync pair - the first click of a pair the upper profile's
//! point, the second the lower profile's, which must lie below it. Each
//! click edits the page draft (`op_ui.params`, `WireSyncPair` in world XY
//! mm); Apply then rides `EditOperationParams`, undoable like every page
//! edit. The picks themselves are ephemeral view state. The stored pairs
//! are drawn as rulings (upper point to lower point) over the model.
//!
//! Provenance: our own UI over the wire sync-pair parameters (#537); the
//! ruling is plain viewport projection. No vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel, OpParams, WirePairingRule, WireSyncPair, SYNC_PAIRS_MAX};

use crate::interact::project_uv;
use crate::state::{GeoPick, State};

/// How far (mm) the lower point must lie below the upper one.
const MIN_DROP: f64 = 1e-6;

/// One drawn ruling: the picked upper and lower points, world mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ruling {
    pub upper: [f64; 3],
    pub lower: [f64; 3],
}

/// The pick's view state. Never a command: the pairs it makes are draft
/// edits until Apply.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WireSync {
    /// Vertex clicks go to the sync pairs, not to the op's selection.
    pub armed: bool,
    /// The upper point picked, waiting for its lower point.
    pub half: Option<[f64; 3]>,
    /// Both points of each pair picked here; drawn while the draft still
    /// holds the same XY pair (a hand edit of the numbers drops its ruling).
    pub rulings: Vec<Ruling>,
}

/// Start collecting sync points: Vertex mode, and no waiting half.
pub fn arm(state: &mut State) {
    state.wire_sync.armed = true;
    state.wire_sync.half = None;
    state.geo_pick = GeoPick::Vertex;
    state.status = String::from("wire sync: click a vertex of the upper profile");
}

/// Stop collecting.
pub fn disarm(state: &mut State) {
    state.wire_sync.armed = false;
    state.wire_sync.half = None;
}

/// The draft's sync pairs, when it is a 4-axis wire cut paired by sync points.
fn pairs_mut(state: &mut State) -> Option<&mut Vec<WireSyncPair>> {
    match &mut state.op_ui.params {
        OpParams::WireContour(w) => w
            .four_axis
            .as_mut()
            .filter(|f| f.pairing == WirePairingRule::SyncPoints)
            .map(|f| &mut f.sync),
        _ => None,
    }
}

/// One viewport pick into the draft: a vertex becomes the next sync
/// point; anything else, or a lower point not below its upper one, is
/// refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let FeatureSel::Vertex(id) = pick.feature else {
        let what = camcraft_app::pick::geo_label(&state.world, &pick);
        state.status = format!("wire sync: {what} is not a vertex - click a corner of a profile");
        return;
    };
    let Some(w) = crate::vertex_pick::vertex_world(state, &pick.entity, id) else {
        state.status = format!("wire sync: the vertex of '{}' is no longer in the model", pick.entity);
        return;
    };
    if !w.iter().all(|v| v.is_finite()) {
        state.status = String::from("wire sync: that vertex has no finite position");
        return;
    }
    let half = state.wire_sync.half;
    let Some(pairs) = pairs_mut(state) else {
        disarm(state);
        state.status = String::from("wire sync: the open operation is not paired by sync points - pick stopped");
        return;
    };
    let waiting = half.filter(|_| pairs.last().is_some_and(|p| p.upper.is_some() && p.lower.is_none()));
    if let Some(up) = waiting {
        if up[2] - w[2] <= MIN_DROP {
            state.status = format!(
                "wire sync: the lower point (Z{:.3}) must lie below the upper point (Z{:.3}) - click a vertex of the lower profile",
                w[2], up[2]
            );
            return;
        }
        if let Some(last) = pairs.last_mut() {
            last.lower = Some([w[0], w[1]]);
        }
        let n = pairs.len();
        state.wire_sync.half = None;
        state.wire_sync.rulings.push(Ruling { upper: up, lower: w });
        state.status = format!("wire sync: pair {n} stated - click the upper point of pair {}, or Apply", n + 1);
    } else {
        if pairs.len() >= SYNC_PAIRS_MAX {
            state.status = format!("wire sync: {SYNC_PAIRS_MAX} pairs are the most an operation stores");
            return;
        }
        pairs.push(WireSyncPair { upper: Some([w[0], w[1]]), lower: None });
        let n = pairs.len();
        state.wire_sync.half = Some(w);
        state.status = format!("wire sync: upper point of pair {n} picked - click a vertex of the lower profile");
    }
}

/// Screen-space segments (viewport uv) of every stored ruling the draft
/// still holds, and a diamond on a waiting upper point. Nothing draws
/// while no 4-axis wire cut is open.
pub(crate) fn segments(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    let OpParams::WireContour(w) = &state.op_ui.params else { return out };
    let Some(f) = w.four_axis.as_ref().filter(|_| !state.op_ui.name.is_empty()) else { return out };
    let mvp = state.cam.mvp(aspect);
    for r in &state.wire_sync.rulings {
        let held = f.sync.iter().any(|p| p.upper == Some([r.upper[0], r.upper[1]]) && p.lower == Some([r.lower[0], r.lower[1]]));
        if !held {
            continue;
        }
        if let (Some(a), Some(b)) = (project_uv(&mvp, r.upper), project_uv(&mvp, r.lower)) {
            out.push((a, b));
        }
    }
    if let Some(h) = state.wire_sync.half {
        crate::vertex_pick::vertex_marker(&mvp, h, aspect, &mut out);
    }
    out
}

/// The page's note on the next click, from the draft's pairs.
#[must_use]
pub fn next_click(sync: &[WireSyncPair]) -> String {
    match sync.last() {
        Some(p) if p.upper.is_some() && p.lower.is_none() => {
            format!("click the lower point of pair {} (a vertex of the lower profile)", sync.len())
        }
        _ => format!("click the upper point of pair {} (a vertex of the upper profile)", sync.len() + 1),
    }
}

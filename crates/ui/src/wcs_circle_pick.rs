//! The WCS page's viewport picks: a click on an anchor marker (the box
//! points of the "Anchor on" grid; #1502: or a corner / round-edge centre of the part) and, #739, a click on a circular edge
//! for the "Circle centre" anchor. Both win over the pick mode while
//! armed. The edge resolves through the one pick rule
//! (`feature_click`, edges only, one run per click); the circle itself is
//! the app's shared `pick::circle_centre`, read by the command the click
//! queues - nothing here measures geometry.
//!
//! Provenance: our own routing over our own pick API and commands; no
//! vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{EdgeUnit, PickKinds};

use crate::feature_click::{Ask, feature_at};
use crate::state::{CommandKind, State};

/// What a circle-centre click takes: feature edges, one run (an arc or a
/// whole round rim) per click.
pub(crate) const CIRCLE_ASK: Ask = Ask { kinds: PickKinds::EDGES, unit: EdgeUnit::Run };

/// What a click beside the anchor markers takes (#1502): the part's own
/// corners (feature points) and the centres of its round edges, one run
/// per click - the same point set the three-points pick offers (#692).
pub(crate) const ANCHOR_ASK: Ask = Ask { kinds: PickKinds { points: true, centres: true, ..PickKinds::POINTS }, unit: EdgeUnit::Run };

/// Anchor-marker hit radius, logical px.
const MARKER_PX: f32 = 12.0;

/// A viewport click the WCS page takes; `false` leaves it to the pick
/// mode. A circle pick takes every click while armed; anchor markers
/// only a click ON a marker (beside one, the viewport picks normally).
pub(crate) fn take_click(state: &mut State, p: [f32; 2]) -> bool {
    if state.wcs_ui.pick_circle {
        circle_click(state, p);
        return true;
    }
    if state.wcs_ui.pick_anchor
        && let Some((k, pos)) = crate::interact::anchor_hit(state, p, MARKER_PX)
    {
        state.wcs_ui.anchor_xy = k;
        queue_origin(state, pos);
        return true;
    }
    state.wcs_ui.pick_anchor && part_point_click(state, p)
}

/// #1502: with the markers armed, a click ON one of the part's corners or
/// round-edge centres anchors the origin there, through the same
/// undoable commands the markers and the circle pick use. A click on
/// neither is left to the pick mode (`false`).
fn part_point_click(state: &mut State, p: [f32; 2]) -> bool {
    let Some((found, Some(feature))) = feature_at(state, p, ANCHOR_ASK) else { return false };
    let pick = GeoSel { entity: found.entity, feature };
    match pick.feature {
        FeatureSel::Vertex(_) => queue_vertex(state, &pick),
        FeatureSel::Chain(_) => queue_circle(state, pick),
        _ => return false,
    }
    true
}

/// The pane's angles with a new origin, as the one `SetWcs` command.
fn queue_origin(state: &mut State, pos: [f64; 3]) {
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

fn circle_click(state: &mut State, p: [f32; 2]) {
    let Some((found, sel)) = feature_at(state, p, CIRCLE_ASK) else {
        state.status = String::from("no edge under the cursor - pick a circular edge");
        return;
    };
    let Some(feature) = sel else {
        state.status = format!("the pick on '{}' names no edge - try beside it", found.entity);
        return;
    };
    queue_circle(state, GeoSel { entity: found.entity, feature });
}

/// Queue the origin onto a picked corner at the pane's angles; a vertex
/// that no longer resolves is refused by name and queues nothing.
pub fn queue_vertex(state: &mut State, pick: &GeoSel) {
    match camcraft_app::pick::align::vertex(&state.world, pick) {
        Ok(pos) => queue_origin(state, pos),
        Err(why) => state.status = format!("origin not placed: {why}"),
    }
}

/// Queue the origin onto a picked edge's circle centre at the pane's
/// angles; the command refuses a non-circular edge by name.
pub fn queue_circle(state: &mut State, pick: GeoSel) {
    state.pending.push_back(CommandKind::SetWcsCircle {
        pick,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

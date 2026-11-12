//! The lay-on-three-points pick draft (#555): while a Setups page row
//! collects points, a viewport Vertex click lands here instead of in the
//! open operation's geometry - points 1, 2 and 3, in order. A fourth
//! click is refused by name (Remove last or Clear first), and so is a
//! face or an edge: the plane is set by points. Ephemeral view state like
//! the face draft - nothing here writes the world; Apply queues the
//! undoable `AlignPartToSetup` through `CommandKind::LayPartOnPoints`.
//!
//! Provenance: our own UI over the 3-2-1 datum practice of fixture
//! design; no vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The points being collected for one setup's lay-on-three-points.
#[derive(Clone, Debug, PartialEq)]
pub struct PointsPick {
    /// The setup the points will align.
    pub setup: String,
    /// Points 1, 2, 3, in pick order.
    pub picks: Vec<GeoSel>,
}

/// Start (or restart) collecting points for `setup`: Vertex mode, and
/// the other alignment drafts stop (one draft collects at a time).
pub fn arm(state: &mut State, setup: &str) {
    state.plane_pick = None;
    crate::axis_pick::disarm(state);
    crate::wcs_pick::disarm(state); // #701: one draft collects at a time
    state.points_pick = Some(PointsPick { setup: setup.to_owned(), picks: Vec::new() });
    state.geo_pick = GeoPick::Vertex;
    state.status = format!("{setup}: pick point 1 of the plane");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.points_pick = None;
}

/// Drop the last point picked.
pub fn remove_last(state: &mut State) {
    if let Some(d) = state.points_pick.as_mut() {
        d.picks.pop();
        state.status = format!("{}: pick point {} of the plane", d.setup, d.picks.len() + 1);
    }
}

/// Drop every point picked; the draft keeps collecting.
pub fn clear(state: &mut State) {
    if let Some(d) = state.points_pick.as_mut() {
        d.picks.clear();
        state.status = format!("{}: pick point 1 of the plane", d.setup);
    }
}

/// One viewport pick into the draft: a point joins as the next of three;
/// anything else, or a fourth point, is refused by name and changes
/// nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(d) = state.points_pick.as_mut() else { return };
    let setup = d.setup.clone();
    let words = format!("{} of '{}'", pick.feature, pick.entity);
    state.status = match pick.feature {
        FeatureSel::Vertex(_) if d.picks.len() >= 3 => {
            format!("{setup}: three points are picked - Remove last or Clear to re-pick")
        }
        FeatureSel::Vertex(_) => {
            d.picks.push(pick);
            match d.picks.len() {
                3 => format!("{setup}: three points picked for the plane"),
                n => format!("{setup}: point {n} picked - pick point {}", n + 1),
            }
        }
        FeatureSel::Face(_) | FeatureSel::Patch(_) => {
            format!("{setup}: {words} is a face - this row takes three points (the face row lays a face)")
        }
        FeatureSel::Chain(_) => format!("{setup}: {words} is an edge - this row takes three points"),
    };
}

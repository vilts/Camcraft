//! The orientation-point pick draft (#771): while a Setups page row
//! collects, a viewport Vertex click lands here instead of in the open
//! operation's geometry. One vertex - the setup's WCS origin point; a
//! click replaces it. Arming sets Vertex mode (like the three-points
//! lay, #555); a face or an edge that arrives anyway is refused by
//! name. Ephemeral view state like the plane draft - nothing here
//! writes the world; Apply queues the undoable `SetWcsOrigin` through
//! `CommandKind::PlaceSetupOrigin`, which keeps the WCS's rotation
//! (the frame comes from the plane side).
//!
//! Provenance: our own UI over plain datum practice; no vendor
//! material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The pick being collected for one setup's origin point.
#[derive(Clone, Debug, PartialEq)]
pub struct OriginPick {
    /// The setup whose WCS origin the pick places.
    pub setup: String,
    /// The picked vertex.
    pub pick: Option<GeoSel>,
    /// Viewport clicks land here; `false` is a paused draft (#664: its
    /// page was left) - the pick stays, shown when the page is back.
    pub picking: bool,
}

/// Start (or restart) collecting a vertex for `setup`: Vertex mode,
/// like the three-points lay (#555). A paused draft of the same setup
/// resumes with its pick kept (#664).
pub fn arm(state: &mut State, setup: &str) {
    crate::axis_pick::disarm(state); // #557: one draft collects at a time
    state.plane_pick = None; // #330
    state.points_pick = None; // #555
    crate::wcs_pick::disarm(state); // #701: one draft collects at a time
    state.cylinder_pick = None; // #771
    match state.origin_pick.as_mut().filter(|d| d.setup == setup) {
        Some(d) => d.picking = true,
        None => {
            state.origin_pick = Some(OriginPick { setup: setup.to_owned(), pick: None, picking: true });
        }
    }
    state.geo_pick = GeoPick::Vertex;
    state.status = format!("{setup}: pick a vertex for the WCS origin");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.origin_pick = None;
}

/// Stop collecting but keep the pick (#664: the page was left).
pub fn pause(state: &mut State) {
    if let Some(d) = state.origin_pick.as_mut() {
        d.picking = false;
    }
}

/// Is a Setups page row collecting a vertex for the origin?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.origin_pick.as_ref().is_some_and(|d| d.picking)
}

/// One viewport pick into the draft: a vertex lands and replaces any
/// earlier one; anything else is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.origin_pick.as_mut().filter(|d| d.picking) else { return };
    let setup = draft.setup.clone();
    state.status = match pick.feature {
        FeatureSel::Vertex(_) => {
            draft.pick = Some(pick);
            format!("{setup}: vertex picked for the origin - Apply places the WCS")
        }
        FeatureSel::Face(_) => format!(
            "{setup}: {} of '{}' is a face - an origin point takes a vertex (or use a box reading)",
            pick.feature, pick.entity
        ),
        FeatureSel::Chain(_) => format!(
            "{setup}: {} of '{}' is an edge - an origin point takes a vertex (or use a box reading)",
            pick.feature, pick.entity
        ),
        FeatureSel::Patch(_) => format!(
            "{setup}: {} of '{}' is not a vertex - an origin point takes a vertex",
            pick.feature, pick.entity
        ),
    };
}

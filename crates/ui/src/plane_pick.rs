//! The lay-on-a-plane pick draft (#330): while a Setups page row collects
//! a pick, a viewport Face click lands here instead of in the open
//! operation's geometry. One flat face; a click replaces it. #554: faces
//! only - arming sets Face mode, so a click near an edge still takes the
//! face, and an edge that arrives anyway is refused by name (an edge
//! cannot set a plane; edges set an axis, `crate::axis_pick`). Ephemeral
//! view state like the boundary draft - nothing here writes the world;
//! Apply queues the undoable `AlignPartToSetup` through
//! `CommandKind::LayPartOnPlane`.
//!
//! Provenance: our own UI over plain datum-face practice; no vendor
//! material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The pick being collected for one setup's lay-on-a-plane.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanePick {
    /// The setup the pick will align.
    pub setup: String,
    pub picks: Vec<GeoSel>,
    /// Viewport clicks land here; `false` is a paused draft (#664: its
    /// page was left) - the picks stay, shown when the page is back.
    pub picking: bool,
}

/// Start (or restart) collecting a face for `setup`: Face mode, so a
/// click anywhere on a face - near its edge included - takes the face.
/// A paused draft of the same setup resumes with its pick kept (#664).
pub fn arm(state: &mut State, setup: &str) {
    crate::axis_pick::disarm(state); // #557: one draft collects at a time
    state.points_pick = None; // #555: one draft collects at a time
    crate::wcs_pick::disarm(state); // #701: one draft collects at a time
    match state.plane_pick.as_mut().filter(|d| d.setup == setup) {
        Some(d) => d.picking = true,
        None => state.plane_pick = Some(PlanePick { setup: setup.to_owned(), picks: Vec::new(), picking: true }),
    }
    state.geo_pick = GeoPick::Face;
    state.status = format!("{setup}: pick a flat face to lay on the plane");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.plane_pick = None;
}

/// Stop collecting but keep the pick (#664: the page was left).
pub fn pause(state: &mut State) {
    if let Some(d) = state.plane_pick.as_mut() {
        d.picking = false;
    }
}

/// Is a Setups page row collecting a face for the plane?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.plane_pick.as_ref().is_some_and(|d| d.picking)
}

/// One viewport pick into the draft: a face replaces it; anything else
/// is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.plane_pick.as_mut().filter(|d| d.picking) else { return };
    let setup = draft.setup.clone();
    state.status = match pick.feature {
        FeatureSel::Face(_) => {
            draft.picks = vec![pick];
            format!("{setup}: face picked for the plane")
        }
        FeatureSel::Chain(_) => format!(
            "{setup}: {} of '{}' is an edge - an edge cannot set a plane; pick a flat face \
             (edges set an axis: Axes in the plane)",
            pick.feature, pick.entity
        ),
        FeatureSel::Patch(_) | FeatureSel::Vertex(_) => {
            format!("{setup}: {} of '{}' is not a flat face - pick a flat face", pick.feature, pick.entity)
        }
    };
}

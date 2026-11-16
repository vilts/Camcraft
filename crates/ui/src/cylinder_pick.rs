//! The cylinder-angle pick draft (#771): while a Setups page row
//! collects, a viewport Face click on a curved wall lands here instead
//! of in the open operation's geometry. One wall - a bore's or
//! cylinder's welded wall names its axis (the app reads the axis from
//! the wall's own facet normals, `align_from_cylinder`); a click
//! replaces it. #554's rule mirrored: faces only - arming sets Face
//! mode, so a click near an edge still takes the face, and an edge
//! that arrives anyway is refused by name (an edge cannot name a
//! cylinder's axis; edges set an axis, `crate::axis_pick`). Whether
//! the picked face is a curved wall is the app's reading at Apply,
//! which refuses a flat face by name. Ephemeral view state like the
//! plane draft - nothing here writes the world; Apply queues the
//! undoable `AlignPartToSetup` through `CommandKind::AlignPartCylinder`.
//!
//! Provenance: our own UI over plain datum-face practice; no vendor
//! material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The pick being collected for one setup's cylinder-angle align.
#[derive(Clone, Debug, PartialEq)]
pub struct CylinderPick {
    /// The setup the pick will align.
    pub setup: String,
    /// The picked curved wall.
    pub pick: Option<GeoSel>,
    /// Viewport clicks land here; `false` is a paused draft (#664: its
    /// page was left) - the pick stays, shown when the page is back.
    pub picking: bool,
}

/// Start (or restart) collecting a wall for `setup`: Face mode, so a
/// click anywhere on a wall - near its edge included - takes the
/// face. A paused draft of the same setup resumes with its pick
/// kept (#664).
pub fn arm(state: &mut State, setup: &str) {
    crate::axis_pick::disarm(state); // #557: one draft collects at a time
    state.plane_pick = None; // #330
    state.points_pick = None; // #555
    state.origin_pick = None; // #771
    crate::wcs_pick::disarm(state); // #701: one draft collects at a time
    match state.cylinder_pick.as_mut().filter(|d| d.setup == setup) {
        Some(d) => d.picking = true,
        None => {
            state.cylinder_pick = Some(CylinderPick { setup: setup.to_owned(), pick: None, picking: true });
        }
    }
    state.geo_pick = GeoPick::Face;
    state.status = format!("{setup}: pick the bore's or cylinder's curved wall to name its axis");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.cylinder_pick = None;
}

/// Stop collecting but keep the pick (#664: the page was left).
pub fn pause(state: &mut State) {
    if let Some(d) = state.cylinder_pick.as_mut() {
        d.picking = false;
    }
}

/// Is a Setups page row collecting a wall for the cylinder?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.cylinder_pick.as_ref().is_some_and(|d| d.picking)
}

/// One viewport pick into the draft: a face lands and replaces any
/// earlier one; anything else is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.cylinder_pick.as_mut().filter(|d| d.picking) else { return };
    let setup = draft.setup.clone();
    state.status = match pick.feature {
        FeatureSel::Face(_) | FeatureSel::Patch(_) => {
            draft.pick = Some(pick);
            format!("{setup}: wall picked - choose the target axis and Apply")
        }
        FeatureSel::Chain(_) => format!(
            "{setup}: {} of '{}' is an edge - an edge cannot name a cylinder's axis; pick its curved wall \
             (edges set an axis: Axes)",
            pick.feature, pick.entity
        ),
        FeatureSel::Vertex(_) => {
            format!("{setup}: {} of '{}' is a point - pick the bore's or cylinder's curved wall", pick.feature, pick.entity)
        }
    };
}

//! The viewport's orientation frame (#1993): which world axis the orbit
//! camera keeps "up", chosen from the mounted machine's kind.
//! Provenance: ISO 841 / RS-274 axis naming (on a lathe Z is the spindle
//! axis and X the cross-slide, radial from the spindle centre line); the
//! frame, its names and the home views are our own design.
//!
//! The law: a display rotation only. The world, the WCS, every toolpath
//! and the posted program keep the right-handed program frame; the camera
//! orbits in a local frame and maps its eye and basis back to the world
//! through one proper rotation, so draw, pick, pan, dolly and the view
//! cube all read one basis by construction.

use crate::state::State;

/// The camera's orbit frame. `Mill` is the identity (world Z up);
/// `Lathe` puts the spindle axis (world Z) across the screen and the
/// cross-slide (world X) up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewFrame {
    /// World Z is up on the screen: the milling convention.
    #[default]
    Mill,
    /// World X (the cross-slide) is up on the screen, and the home view
    /// shows world +Z (the spindle axis, away from the chuck) to the
    /// right. Local axes: x = -Z, y = +Y, z = +X (a proper rotation).
    Lathe,
}

impl ViewFrame {
    /// The frame a machine kind is viewed in. Only a lathe turns the
    /// view: a mill-turn machine keeps the milling frame until #2019
    /// decides its convention.
    #[must_use]
    pub fn for_kind(kind: camcraft_app::machine::Kind) -> ViewFrame {
        match kind {
            camcraft_app::machine::Kind::Lathe => ViewFrame::Lathe,
            _ => ViewFrame::Mill,
        }
    }

    /// The frame of the machine the active setup mounts.
    #[must_use]
    pub fn of(state: &State) -> ViewFrame {
        ViewFrame::for_kind(camcraft_app::machine::kind_of(&state.world))
    }

    /// A local (orbit-frame) vector in world coordinates.
    #[must_use]
    pub fn to_world(self, v: (f64, f64, f64)) -> (f64, f64, f64) {
        match self {
            ViewFrame::Mill => v,
            // world = x * (-Z) + y * (+Y) + z * (+X)
            ViewFrame::Lathe => (v.2, v.1, -v.0),
        }
    }

    /// A world vector in local (orbit-frame) coordinates: the inverse
    /// (the transpose) of [`ViewFrame::to_world`].
    #[must_use]
    pub fn to_local(self, v: (f64, f64, f64)) -> (f64, f64, f64) {
        match self {
            ViewFrame::Mill => v,
            ViewFrame::Lathe => (-v.2, v.1, v.0),
        }
    }

    /// The (yaw, pitch) a camera takes when it enters this frame: the
    /// house isometric for a mill; for a lathe the front view, a little
    /// raised toward +X, with the spindle axis left to right.
    #[must_use]
    pub fn home(self) -> (f64, f64) {
        match self {
            ViewFrame::Mill => (0.9, 0.5),
            ViewFrame::Lathe => (std::f64::consts::FRAC_PI_2, 0.35),
        }
    }
}

/// Keep the camera in the frame of the mounted machine. Runs once per
/// frame before the viewport reads the camera; a change of frame (a
/// lathe mounted, or a mill again) takes that frame's home view.
pub(crate) fn sync(state: &mut State) {
    let want = ViewFrame::of(state);
    if state.cam.frame != want {
        state.cam = state.cam.with_frame(want);
    }
}

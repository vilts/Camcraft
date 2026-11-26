//! The viewport bar's plain-English labels and hover tooltips (#2914).
//!
//! Provenance: original text, labels and descriptions for CamCraft's
//! viewport bar controls; no vendor material.

use super::Nav;
use crate::state::{DisplayMode, DragMode, State};
use crate::viewport::ProjectionMode;

/// The plain-English state line of the pick-through button (#692).
#[must_use]
pub(crate) fn pick_through_label(on: bool) -> &'static str {
    if on { "Pick through: on" } else { "Pick through: off" }
}

/// The plain-English state line of the Rotate button (#2047).
#[must_use]
pub(crate) fn rotate_label(on: bool) -> &'static str {
    if on {
        "Rotate: drag a ring on the selected part"
    } else {
        "Rotate: off"
    }
}

/// The plain-English state line of the Move button (#1198 b).
#[must_use]
pub(crate) fn move_label(on: bool) -> &'static str {
    if on {
        "Move: drag a ball to move or a ring to rotate the selected part"
    } else {
        "Move: off"
    }
}

/// The status line the projection button flips the viewer between.
#[must_use]
pub(crate) fn projection_label(mode: ProjectionMode) -> &'static str {
    match mode {
        ProjectionMode::Perspective => "projection: perspective",
        ProjectionMode::Orthographic => "projection: orthographic",
    }
}

/// The status line a latched nav tool wears on the status bar.
#[must_use]
pub(crate) fn nav_tool_label(tool: DragMode) -> &'static str {
    match tool {
        DragMode::Orbit => "nav tool: orbit",
        DragMode::Pan => "nav tool: pan",
        DragMode::Zoom => "nav tool: zoom",
    }
}

/// The status line when a nav tool latches off.
pub(crate) const NAV_TOOL_CLEAR: &str = "nav tool: off";

/// The base plain-English label (the accessible name) for each of the
/// viewport controls. Each name is unique in the whole window: a script or
/// a screen reader must tell the bar's button from the View menu's
/// "Show all toolpaths" check box and "Projection" row, and from the
/// Display menu's "Display mode" group and "Wireframe" entry (#2914).
/// The state and the longer text go in the tooltip, not in the name.
#[must_use]
pub(crate) fn label(kind: Nav) -> &'static str {
    match kind {
        Nav::Frame => "Frame all",
        Nav::Iso => "Isometric view",
        Nav::Wire => "Switch wireframe",
        Nav::Display => "Display menu",
        Nav::Paths => "Show or hide toolpaths",
        Nav::Axes => "World axes",
        Nav::PickThrough => "Pick through",
        Nav::Move => "Move mode",
        Nav::Rotate => "Rotate mode",
        Nav::Projection => "Switch projection",
        Nav::Orbit => "Orbit tool",
        Nav::Pan => "Pan tool",
        Nav::Zoom => "Zoom tool",
    }
}

/// The descriptive hover tooltip for each of the viewport controls (#2914),
/// reflecting the current state of toggle controls.
#[must_use]
pub(crate) fn tooltip(kind: Nav, state: &State) -> String {
    match kind {
        Nav::Frame => "Frame all (F)\nFit the whole document in the view.".to_owned(),
        Nav::Iso => "Isometric view\nLook at the model from an isometric angle.".to_owned(),
        Nav::Wire => match state.display {
            DisplayMode::Wireframe => {
                "Wireframe: wireframe (click for shaded with edges)\nSwitch between shaded edges and wireframe.".to_owned()
            }
            DisplayMode::WireframeSeeThrough => {
                "Wireframe: wireframe see-through (click for shaded with edges)\nSwitch between shaded edges and wireframe.".to_owned()
            }
            DisplayMode::Shaded => {
                "Wireframe: shaded (click for wireframe)\nSwitch between shaded edges and wireframe.".to_owned()
            }
            DisplayMode::ShadedHiddenEdges => {
                "Wireframe: shaded with hidden edges (click for wireframe)\nSwitch between shaded edges and wireframe.".to_owned()
            }
            DisplayMode::ShadedEdges => {
                "Wireframe: shaded with edges (click for wireframe)\nSwitch between shaded edges and wireframe.".to_owned()
            }
        },
        Nav::Display => {
            if state.display_menu_open {
                "Display menu: open\nOpen or close the menu of display modes and graphics.".to_owned()
            } else {
                "Display menu: closed\nOpen or close the menu of display modes and graphics.".to_owned()
            }
        }
        Nav::Paths => {
            if state.show_all_paths {
                "Toolpaths: on\nShow or hide all cached toolpaths.".to_owned()
            } else {
                "Toolpaths: off\nShow or hide all cached toolpaths.".to_owned()
            }
        }
        Nav::Axes => {
            if state.show_world_axes {
                "World axes: on\nShow or hide the world origin axes.".to_owned()
            } else {
                "World axes: off\nShow or hide the world origin axes.".to_owned()
            }
        }
        Nav::PickThrough => {
            if state.pick_through {
                "Pick through: on (picks reach far side)\nAllow selection through the part.".to_owned()
            } else {
                "Pick through: off (picks stop at front face)\nAllow selection through the part.".to_owned()
            }
        }
        Nav::Move => {
            if state.move_handles {
                "Move part: on (drag handles to move part)\nShow handles to move the selected part.".to_owned()
            } else {
                "Move part: off (show move handles)\nShow handles to move the selected part.".to_owned()
            }
        }
        Nav::Rotate => {
            if state.rotate_handles {
                "Rotate part: on (drag rings to rotate part)\nShow rings to rotate the selected part.".to_owned()
            } else {
                "Rotate part: off (show rotation rings)\nShow rings to rotate the selected part.".to_owned()
            }
        }
        Nav::Projection => match state.cam.projection {
            ProjectionMode::Perspective => {
                "Projection: perspective (P, click for orthographic)\nSwitch between perspective and orthographic views.".to_owned()
            }
            ProjectionMode::Orthographic => {
                "Projection: orthographic (P, click for perspective)\nSwitch between perspective and orthographic views.".to_owned()
            }
        },
        Nav::Orbit => {
            if state.nav_tool == Some(DragMode::Orbit) {
                "Orbit: active (drag to orbit)\nDrag to turn the camera around the model.".to_owned()
            } else {
                "Orbit: click to latch\nDrag to turn the camera around the model.".to_owned()
            }
        }
        Nav::Pan => {
            if state.nav_tool == Some(DragMode::Pan) {
                "Pan: active (drag to pan)\nDrag to move the view sideways.".to_owned()
            } else {
                "Pan: click to latch\nDrag to move the view sideways.".to_owned()
            }
        }
        Nav::Zoom => {
            if state.nav_tool == Some(DragMode::Zoom) {
                "Zoom: active (drag to zoom)\nDrag to move the camera closer or farther.".to_owned()
            } else {
                "Zoom: click to latch\nDrag to move the camera closer or farther.".to_owned()
            }
        }
    }
}

//! Which operation kinds carry the level transitions and links (M11.2
//! C/D), and the default each kind starts from - one table the stamp,
//! the project file and the operation page all read. Provenance: our own
//! parameter model over standard CAM link strategies (`camcraft_motion`'s
//! `links.rs` names the sources); no vendor material.

use crate::links::Links;

use super::params::OpParams;

impl OpParams {
    /// The level transitions and links of a level-ladder kind; `None`
    /// for every kind without a level ladder to link.
    #[must_use]
    pub fn links(&self) -> Option<&Links> {
        match self {
            OpParams::Contour { links, .. }
            | OpParams::Pocket { links, .. }
            | OpParams::Adaptive2D { links, .. }
            | OpParams::Offset2D { links, .. }
            | OpParams::Circular { links, .. }
            | OpParams::Slot { links, .. }
            | OpParams::Bore { links, .. }
            | OpParams::Pocket3D { links, .. }
            | OpParams::RotaryOffset { links, .. }
            | OpParams::MultiAxisRough { links, .. } => Some(links),
            _ => None,
        }
    }

    /// [`Self::links`], for the editor.
    pub fn links_mut(&mut self) -> Option<&mut Links> {
        match self {
            OpParams::Contour { links, .. }
            | OpParams::Pocket { links, .. }
            | OpParams::Adaptive2D { links, .. }
            | OpParams::Offset2D { links, .. }
            | OpParams::Circular { links, .. }
            | OpParams::Slot { links, .. }
            | OpParams::Bore { links, .. }
            | OpParams::Pocket3D { links, .. }
            | OpParams::RotaryOffset { links, .. }
            | OpParams::MultiAxisRough { links, .. } => Some(links),
            _ => None,
        }
    }

    /// The stepdown this op's depth ladder walks between `top` and
    /// `bottom` (#2012): the generator's own reading of the ladder mode,
    /// so a region built at the ladder's planes meets the planes cut. A
    /// kind without links keeps `stepdown`.
    #[must_use]
    pub fn ladder_stepdown(&self, top: f64, bottom: f64, stepdown: f64) -> f64 {
        self.links().map_or(stepdown, |l| crate::machining2d::ladder_stepdown(&l.approach, top, bottom, stepdown))
    }

    /// The links a kind posted before M11.2 - what a file without the
    /// key means and what the stamp and the file leave unwritten: the
    /// 3-D ladder's pass links have always stayed down where proved, the
    /// planar ladders' have always hopped.
    #[must_use]
    pub fn default_links(&self) -> Links {
        match self {
            OpParams::Pocket3D { .. } | OpParams::RotaryOffset { .. } | OpParams::MultiAxisRough { .. } => {
                Links::ROUGH3D
            }
            _ => Links::PLANAR,
        }
    }
}

//! The user's start and exit picks of a cutting operation (#1448, #3324):
//! one shared type that every path kind answers through the application's
//! `OpParams::path_picks`, so each family planner reads one shape. A pick
//! is a wish: a planner projects it into its own frame (setup XY, a tilted
//! frame, the lathe's (Z, X) plane) or refuses it by name - it never moves
//! a pick without a note.
//!
//! Provenance: our own parameter model over the general machining practice
//! of a chosen entry and exit position (machine-tool handbooks); no vendor
//! material.

use crate::geo2d::Point2;

/// Which end of the cut a pick sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickRole {
    /// Where the tool enters the material.
    Start,
    /// Where the tool leaves the material.
    Exit,
}

impl PickRole {
    /// The role's name in a refusal: "start" or "exit".
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Exit => "exit",
        }
    }
}

/// One pick: a model-frame point, with an optional Z where the family
/// reads a height (a 3D finish, a multi-axis start). The 2D families read
/// `xy` only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPick {
    /// Model X and Y, mm.
    pub xy: Point2,
    /// Model Z, mm; `None` is "the family's own height".
    pub z: Option<f64>,
}

impl PathPick {
    /// A pick in model XY with no Z.
    #[must_use]
    pub fn xy(xy: Point2) -> Self {
        Self { xy, z: None }
    }
}

/// The largest coordinate a pick may hold (mm): 1 km from the origin,
/// the contour's law since #1448.
pub const PICK_COORD_MAX: f64 = 1.0e6;

/// The most picks of one role an operation takes so far. The type holds
/// a list (one pick per chain or region is the design, #1448) so that the
/// file format does not change again; a family that reads a list raises
/// this number in its own slice.
pub const PICKS_PER_ROLE: usize = 1;

/// The start and exit picks of one operation. Empty lists are the
/// automatic choice of every earlier file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathPicks {
    pub start: Vec<PathPick>,
    pub exit: Vec<PathPick>,
}

impl PathPicks {
    /// No pick of either role.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.exit.is_empty()
    }

    /// The picks of `role`.
    #[must_use]
    pub fn of(&self, role: PickRole) -> &[PathPick] {
        match role {
            PickRole::Start => &self.start,
            PickRole::Exit => &self.exit,
        }
    }

    /// The first pick of `role`, if any.
    #[must_use]
    pub fn first(&self, role: PickRole) -> Option<PathPick> {
        self.of(role).first().copied()
    }

    /// The by-name refusal of a pick no planner can read: a coordinate
    /// that is not finite or is more than 1 km from the origin, or more
    /// picks of one role than [`PICKS_PER_ROLE`]. `None` is sane.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let coord = |v: f64| v.is_finite() && v.abs() <= PICK_COORD_MAX;
        for role in [PickRole::Start, PickRole::Exit] {
            let picks = self.of(role);
            let what = role.word();
            if picks.len() > PICKS_PER_ROLE {
                return Some(format!(
                    "only one {what} point is taken for each operation so far - clear the other {what} points"
                ));
            }
            if picks.iter().any(|p| !(coord(p.xy.x) && coord(p.xy.y) && p.z.is_none_or(coord))) {
                return Some(format!("the {what} point must be finite coordinates within 1 km of the origin"));
            }
        }
        None
    }
}

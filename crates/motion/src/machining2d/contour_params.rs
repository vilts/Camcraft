//! The contour's parameter model (moved out of `contour.rs` by M11.2,
//! structure only): leads, side, the numbers, and the door that refuses
//! nonsense before any geometry is touched. Provenance: published CAM
//! practice for lead-in families and multi-depth profiling (see
//! `contour.rs`); the model is ours, no vendor material.

use crate::entry::Entry3D;
use crate::links::Links;

use super::face::GenError;
use super::levels::cut_levels;

/// Entry/exit motion appended to each contour subpath. Every lead that
/// is not [`Lead::None`] keeps the tool from dwelling (and gouging) at a
/// plunge point directly on the finished wall: it starts the cut already
/// moving, in air, and joins the wall tangent-continuous. Specced from
/// published CAM practice (lead-in families: linear, quarter-arc and the
/// tangential half-arc that enters parallel to the path), no vendor
/// material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lead {
    /// Plunge straight onto the path.
    None,
    /// Start `length` behind the path along its tangent, run in, and
    /// overrun the same way on exit.
    Linear { length: f64 },
    /// Quarter arc of `radius`, tangent to the path at entry and exit,
    /// bulging to the air side. The tool starts the arc moving normal to
    /// the path. Posted as one arc move; chord error at `tolerance` only
    /// appears where a path gets flattened downstream (verification,
    /// display, the golden dump).
    Arc { radius: f64 },
    /// Half arc of `radius` (a semicircle), tangent to the path at the
    /// join: the tool plunges `2 * radius` off the wall, in air, moving
    /// *parallel* to the path, curves over and merges into the cut at the
    /// join still moving along it - G1 continuous end to end, which is
    /// what earns the name. Lead-out mirrors it: the tool overruns the
    /// end tangentially and leaves the wall moving parallel to the path.
    /// One posted arc move like [`Lead::Arc`].
    Tangential { radius: f64 },
    /// Straight in along the wall's normal from `length` out in the air,
    /// meeting the wall square (M11.3 C); lead-out mirrors it. Appended:
    /// no earlier file holds it.
    Perpendicular { length: f64 },
}

/// Which side of a contour boundary the cutter runs on - a sign law on
/// the offset and one traversal flip, nothing else. Auto is resolved by
/// the selection layer (only there does anything know what a picked ring
/// bounds); inside the generator Auto means Outside, so every
/// explicit-region caller - goldens, direct tests - keeps cutting today's
/// law byte-for-byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    /// Generator: the Outside law. The app's fill-ratio check on picked
    /// rings may resolve Auto to Inside, and says so in a note.
    #[default]
    Auto,
    /// Cutter inside the boundary: the region erodes by `r + leave` and
    /// each loop's traversal reverses, so material stays on the left and
    /// leads bulge into the bounded air - the wall of a bore. That
    /// bounded air is exactly what certifies the entry family's descents
    /// across the level planes (M8.2 A).
    Inside,
    /// Today's law: the region grows by `r + leave`, the tool grazes the
    /// boundary from the material side - a boss or an outside wall.
    Outside,
}

/// 2D contouring: trim a material region's boundary at successive depths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourParams {
    pub tool_diameter: f64,
    /// Thickness of material deliberately left on the finished faces for
    /// a later finishing pass.
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Plane tab hops travel in, between the cut and `clearance_z`.
    pub retract_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling under the M3 spindle every 2D op posts (#1555): the
    /// material stays on the RIGHT of travel - an outside ring runs
    /// clockwise, an inside ring counter-clockwise, and control
    /// compensation is G41; conventional reverses all three. The field
    /// means the process, not a winding: the generator applies the side.
    pub climb: bool,
    /// Which side of the boundary to cut; see [`Side`]. Only the offset
    /// sign and the traversal follow from it - hops, leads and tabs are
    /// side-agnostic.
    pub side: Side,
    pub lead: Lead,
    /// Chord tolerance for arcs: bounds how far a fitted arc may sit
    /// from the tessellation it replaces (both directions), and is the
    /// chord error arcs flatten to wherever a law needs segments.
    pub tolerance: f64,
    /// Tabs: `tab_count` bridges of `tab_width`, holding the part until
    /// the cut drops below `bottom_z + tab_height`.
    pub tab_count: u32,
    pub tab_width: f64,
    pub tab_height: f64,
    /// How a level crossing engages (M8.2 A): the family the pocket
    /// ladders already read. It applies where the descent can be
    /// certified - `Side::Inside`, whose bounded air the eroded region
    /// states exactly. On the wall side it is refused by name in the
    /// path's notes, and the graze this generator has always posted
    /// stands (the grown region would certify a chord across the
    /// part's own interior - a lie, and M8.1's distance law is the
    /// reason it is not told).
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); read only when `entry` is
    /// Helix, and certified per level by `dive_room`'s margin, not by
    /// the roughing overlap law: a contour orbit descends air, so a
    /// radius above the tool's is legal wherever the bore holds it.
    pub helix_radius: f64,
    /// Turning-point blends (M11.1 B): corners (line or arc on either
    /// side) that turn toward the air ride one tangent fillet where the
    /// law of `fillet.rs` / `fillet_arc.rs` proves it; corners toward the part, and every other refusal,
    /// stay sharp and are named in the notes with the worst corner
    /// rest. `false` - every pre-M11.1 file - is unchanged to the byte.
    pub blend_turns: bool,
    /// Level transitions and links (M11.2 C/D, `contour_link`): the
    /// descent between levels, and how the tool travels between levels
    /// and between rings - clearance, the retract plane, or keep down
    /// (between levels: ring by ring, descending on the ring itself).
    /// The user's choice - nothing here is premade; [`Links::PLANAR`] is
    /// every file before the option (the old `level_link` stay-down ramp
    /// is level Keep down with a Ramp transition).
    pub links: Links,
}

impl ContourParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.retract_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.tolerance,
            self.tab_width,
            self.tab_height,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z < self.top_z {
            // Hops travel at the retract plane; one at or below the
            // material top would drag across solid - a gouge by
            // definition, refused by name. Above clearance is legal in
            // old files and gets clamped, not refused (see `contour`).
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        if self.tolerance <= 0.0 {
            return Err(GenError::Invalid("tolerance must be positive".into()));
        }
        match self.lead {
            Lead::None => {}
            Lead::Linear { length } if length >= 0.0 && length.is_finite() => {}
            Lead::Arc { radius } if radius > 0.0 => {}
            Lead::Tangential { radius } if radius > 0.0 => {}
            Lead::Perpendicular { length } if length >= 0.0 && length.is_finite() => {}
            _ => return Err(GenError::Invalid("bad lead-in parameters".into())),
        }
        if self.tab_count > 0 && (self.tab_width <= 0.0 || self.tab_height < 0.0) {
            return Err(GenError::Invalid("tabs need positive width".into()));
        }
        self.links.validate().map_err(|e| GenError::Invalid(e.into()))?;
        let helix = self.entry == Entry3D::Helix || self.links.transition == Some(Entry3D::Helix);
        if helix && self.helix_radius <= 0.0 {
            // Only positivity: the roughing ladders cap the radius at
            // the tool's so successive turns overlap, but a contour
            // orbit descends an already-open bore and cuts nothing on
            // the way down - fit is certified per level by `dive_room`
            // against the wall, which is where a too-wide disc is
            // refused (loudly, as a substitution).
            return Err(GenError::Invalid("helix radius must be positive".into()));
        }
        Ok(())
    }

    /// The lead-out's shape (M11.3 C): the approach's own, or the
    /// lead-in mirrored - every file before the option.
    pub(super) fn lead_out_shape(&self) -> Lead {
        self.links.approach.lead_out.unwrap_or(self.lead)
    }

    /// Cut levels from `top_z` down to `bottom_z`, always ending exactly
    /// at the bottom (same snap rule as facing rows).
    pub(super) fn levels(&self) -> Result<Vec<f64>, GenError> {
        cut_levels(self.top_z, self.bottom_z, self.stepdown)
    }
}

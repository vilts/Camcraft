//! 2D rest machining (#1446): the part of a pocket a larger previous tool
//! could not reach, and the pocket passes that cut only that part.
//!
//! Provenance: the area a cutter of radius R can sweep inside a pocket P
//! is the morphological opening of P by a disc of radius R - erode P by R
//! (the legal cutter centers), then dilate the result by R (what the
//! cutter covers from them); the residue is P minus that opening (J.
//! Serra, "Image Analysis and Mathematical Morphology", Academic Press
//! 1982, for erosion, dilation and opening). In pocket machining the same
//! construction is the inward offset of the boundary followed by an
//! outward one (M. Held, "On the Computational Geometry of Pocket
//! Machining", LNCS 500, Springer 1991), and the material a large cutter
//! leaves for a smaller one is treated that way in D. Veeramani and Y.-S.
//! Gau, "Selection of an optimal set of cutting-tool sizes for 2.5D pocket
//! machining", Computer-Aided Design 29(12), 1997, and Z. Yao, S. K. Gupta
//! and D. S. Nau, "Algorithms for selecting cutters in multi-part milling
//! problems", Computer-Aided Design 35(9), 2003. Written from those and
//! this project's own polygon offsets (`geo2d`); the margins, the film
//! tolerance and every refusal are ours. No other CAM product's code or
//! output was consulted.
//!
//! Prime Directive: the rest area is read large, never small. The cleared
//! area is shrunk by [`REST_MARGIN`] before it is subtracted, a flat
//! width the previous tool's corner radius does not cut is not counted as
//! cleared, and the allowance only grows the area. The one thing dropped
//! is a film thinner than [`REST_FILM`], which is below what the offsets
//! resolve (without it every wall would read as rest material). The
//! passes are the pocket's own: every move below the top of the cut is a
//! feed or a plunge, so a wrong previous tool costs a heavier cut, never
//! a rapid into stock.

use crate::cl::Toolpath;
use crate::geo2d::Region;

use super::face::GenError;
use super::pocket::{PocketParams, PocketStrategy, pocket_over};

/// How far the previous tool's cleared area is shrunk before it is
/// subtracted (mm): two polygon-offset arc tolerances (5 um each, one per
/// offset of the opening), so chord error can only add rest material.
pub const REST_MARGIN: f64 = 0.01;

/// Residue thinner than this (mm) is not rest material: the band
/// [`REST_MARGIN`] leaves along every wall is this thin, and a film of 20
/// um is under what the region offsets resolve.
pub const REST_FILM: f64 = 0.02;

/// The tool's reach onto the rest area is read this much (mm) past its
/// radius. The rest area shares the pocket's walls, so its offset by the
/// bare radius would run along the pocket's own center boundary and the
/// intersection of the two would leave slivers; with the reach read
/// long the pocket's boundary decides there. Only adds tool positions,
/// all of them inside the pocket's own center area.
pub const REST_REACH: f64 = 0.05;

/// The previous tool, as the user entered it. The generator trusts it:
/// it cannot see what really cut the pocket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestParams {
    /// Diameter of the tool that roughed the pocket; must be larger than
    /// this operation's tool.
    pub previous_tool_diameter: f64,
    /// Corner (bull-nose) radius of that tool; 0 for a flat end mill. The
    /// floor within this distance of a wall keeps a fillet, so that
    /// width is not read as cleared.
    pub previous_corner_radius: f64,
    /// Radial stock the previous operation left on the walls.
    pub previous_stock_to_leave: f64,
    /// Grows the rest area by this much (mm) so the passes overlap the
    /// previous tool's cut. Never shrinks it.
    pub allowance: f64,
}

impl RestParams {
    fn validate(&self, tool_diameter: f64) -> Result<(), GenError> {
        let all = [
            self.previous_tool_diameter,
            self.previous_corner_radius,
            self.previous_stock_to_leave,
            self.allowance,
        ];
        if !all.into_iter().all(f64::is_finite) {
            return Err(GenError::Invalid("rest machining parameters must be finite".into()));
        }
        if self.previous_tool_diameter <= tool_diameter {
            return Err(GenError::Invalid(format!(
                "rest machining needs a previous tool larger than this one: the previous tool is {} mm, this tool is {} mm",
                self.previous_tool_diameter, tool_diameter
            )));
        }
        if self.previous_corner_radius < 0.0 || self.previous_corner_radius > self.previous_tool_diameter * 0.5 {
            return Err(GenError::Invalid(
                "the previous tool's corner radius must be between 0 and its radius".into(),
            ));
        }
        if self.previous_stock_to_leave < 0.0 {
            return Err(GenError::Invalid("the previous stock to leave must not be negative".into()));
        }
        if self.allowance < 0.0 {
            return Err(GenError::Invalid("the rest allowance must not be negative".into()));
        }
        Ok(())
    }
}

/// What the previous tool left, and where this tool's center may go to
/// cut it.
#[derive(Clone, Debug)]
pub struct RestPlan {
    /// The rest material (a superset of it, see the module text), inside
    /// the pocket less this operation's stock to leave.
    pub rest: Region,
    /// Cutter-center area: the pocket's own center area where the tool
    /// touches `rest`. Empty exactly when `rest` is.
    pub center: Region,
    /// False when the previous tool fits nowhere in the pocket: nothing
    /// was cleared and `rest` is the whole pocket.
    pub previous_fits: bool,
}

/// The rest plan for `region` (the pocket, as [`super::pocket`] reads it)
/// cut by a tool of `tool_diameter` leaving `stock_to_leave`, after the
/// previous tool `rest` describes. An empty plan (the previous tool
/// reached everything) is `Ok` with empty regions; the caller decides
/// what that means.
pub fn rest_plan(
    region: &Region,
    tool_diameter: f64,
    stock_to_leave: f64,
    rest: &RestParams,
) -> Result<RestPlan, GenError> {
    if !(tool_diameter.is_finite() && stock_to_leave.is_finite()) || tool_diameter <= 0.0 || stock_to_leave < 0.0 {
        return Err(GenError::Invalid("tool diameter must be positive and stock to leave not negative".into()));
    }
    rest.validate(tool_diameter)?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let r = tool_diameter * 0.5;
    let own_center = region.offset(-(r + stock_to_leave))?;
    if own_center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    // What this operation is to leave free of material.
    let target = if stock_to_leave > 0.0 { region.offset(-stock_to_leave)? } else { region.clone() };
    let big_r = rest.previous_tool_diameter * 0.5;
    let previous_center = region.offset(-(big_r + rest.previous_stock_to_leave))?;
    if previous_center.is_empty() {
        // Nothing was cleared: the whole pocket is rest material, and
        // the center area is the pocket's own, untouched.
        return Ok(RestPlan { rest: target, center: own_center, previous_fits: false });
    }
    // The opening, read small: the flat of the previous tool, less the
    // margin, swept from its legal centers.
    let sweep = big_r - rest.previous_corner_radius - REST_MARGIN;
    let cleared = if sweep > 0.0 { previous_center.offset(sweep)? } else { previous_center };
    let raw = target.difference(&cleared)?;
    // Drop the film (an opening by half its width), then grow by the
    // allowance; never past the target.
    let half = REST_FILM * 0.5;
    let core = raw.offset(-half)?;
    if core.is_empty() {
        return Ok(RestPlan { rest: Region::empty(), center: Region::empty(), previous_fits: true });
    }
    let rest_area = core.offset(half + rest.allowance)?.intersect(&target)?;
    // Centers from which the tool touches the rest area, kept inside the
    // pocket's own center area: no wall is crossed that the plain pocket
    // would not cross.
    let center = rest_area.offset(r + REST_REACH)?.intersect(&own_center)?;
    Ok(RestPlan { rest: rest_area, center, previous_fits: true })
}

/// Pocket `region` with the tool of `p`, cutting only what the previous
/// tool of `rest` left. Levels, entries and links are [`super::pocket`]'s,
/// planned as rings over the rest plan's center area: `Waterline` and
/// `OffsetRings` as chosen, and `Auto` reads as `Waterline` here (the
/// first ring of each rest area runs along the wall, which is what
/// clears a corner).
///
/// Refused by name: a previous tool that is not larger than this one;
/// `Zigzag` (rows leave cusps along the walls of a rest area, so the
/// corner would not come clean); `Adaptive` (its entry and engagement
/// plan assume a full pocket; not built for rest areas yet); nothing
/// left to cut; rest material this tool cannot reach either.
pub fn rest_pocket(region: &Region, p: &PocketParams, rest: &RestParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let strategy = match p.strategy {
        PocketStrategy::Auto | PocketStrategy::Waterline => PocketStrategy::Waterline,
        PocketStrategy::OffsetRings => PocketStrategy::OffsetRings,
        PocketStrategy::Zigzag => {
            return Err(GenError::Invalid(
                "rest machining is not available with the Zigzag strategy: its rows leave cusps along the walls of a rest area; choose Waterline or Offset rings".into(),
            ));
        }
        PocketStrategy::Adaptive => {
            return Err(GenError::Invalid(
                "rest machining is not available with the Adaptive strategy yet: choose Waterline or Offset rings".into(),
            ));
        }
    };
    let p = &PocketParams { strategy, ..*p };
    let plan = rest_plan(region, p.tool_diameter, p.stock_to_leave, rest)?;
    if plan.rest.is_empty() {
        return Err(GenError::Empty(format!(
            "no rest material: the previous {} mm tool reached everything in this pocket",
            rest.previous_tool_diameter
        )));
    }
    if plan.center.is_empty() {
        return Err(GenError::Invalid(
            "the rest material is narrower than the tool: no legal tool position touches it".into(),
        ));
    }
    let mut notes = vec![format!(
        "rest machining trusts the entered previous tool ({} mm, corner radius {} mm, {} mm left on the walls) to have cut this pocket to the same depth: {:.1} of {:.1} mm2 is read as rest material",
        rest.previous_tool_diameter,
        rest.previous_corner_radius,
        rest.previous_stock_to_leave,
        plan.rest.area().abs(),
        region.area().abs(),
    )];
    if !plan.previous_fits {
        notes.push("the previous tool fits nowhere in this pocket: the whole pocket is cut".into());
    }
    pocket_over(region, &plan.center, p, notes, true, &[])
}

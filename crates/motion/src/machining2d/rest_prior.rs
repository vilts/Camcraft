//! 2D rest machining from the previous operations (#2033): the rest area
//! is the material the earlier operations of the setup are NOT proved to
//! have cut, read from the stock they leave at every cut depth.
//!
//! Provenance: the in-process workpiece of 2.5D pocketing as planar
//! regions per depth (M. Held, "On the Computational Geometry of Pocket
//! Machining", LNCS 500, Springer 1991; Z. Yao, S. K. Gupta and D. S.
//! Nau, "Algorithms for selecting cutters in multi-part milling
//! problems", Computer-Aided Design 35(9), 2003). The reading rules (one
//! cell of growth, unknown columns stand, every depth is read) are this
//! project's decision; no other CAM product's code or output was consulted.
//!
//! Prime Directive: the rest area is read large, never small.
//! - A column stands at a cut depth when its top is above that depth. A
//!   shallow earlier pass proves nothing about the floor under it.
//! - The standing columns grow by one cell diagonal: the grid samples the
//!   stock, so material thinner than a cell beside a standing column is
//!   not proved absent.
//! - A column that cannot be read, and the pocket outside the grid, stand.
//! - The rest area is the union over every cut depth. A level can cut air
//!   that a deeper level needs; it never skips material.
//! - The passes are the pocket's own (`rest2d`): every move under the top
//!   of the cut is a feed or a plunge.

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::stock_sim::Columns;

use camcraft_sim::verify::AvoidSpec;

use super::adaptive::{RestStock, StockFloor};
use super::corner_relief::CornerRelief;
use super::face::GenError;
use super::levels::cut_levels_under;
use super::pocket::PocketParams;
use super::pocket_avoid_level::emit_rest_avoid;
use super::rest2d::{rest_strategy, RestParams, RestPlan, RestSource, REST_REACH};

/// A column whose top is within this much (mm) above a cut depth does
/// not stand there: float noise of a floor cut at that depth.
pub const PRIOR_Z_TOL: f64 = 1e-6;

/// The area under which the whole target counts as standing (mm2).
const WHOLE_TOL: f64 = 1e-9;

/// The reading guards, each on in every build. The unit tests switch one
/// off at a time to show that its pinning test then fails (#2033).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Guards {
    /// Every cut depth is read, not only the first.
    pub depth: bool,
    /// A column that cannot be read stands.
    pub unknown: bool,
    /// The pocket outside the grid stands.
    pub outside: bool,
    /// The standing columns grow by one cell diagonal.
    pub cell: bool,
}

impl Guards {
    pub(super) const ALL: Guards = Guards { depth: true, unknown: true, outside: true, cell: true };
}

/// The stock the columns `cols` prove inside `region`, one floor for each
/// depth of `levels`. Refused by name: an empty pocket, no depth, a depth
/// that is not finite, a grid without cells.
pub fn rest_stock_from_columns(cols: &Columns<'_>, region: &Region, levels: &[f64]) -> Result<RestStock, GenError> {
    stock_from(cols, region, levels, Guards::ALL)
}

pub(super) fn stock_from(cols: &Columns<'_>, region: &Region, levels: &[f64], g: Guards) -> Result<RestStock, GenError> {
    let cell = cols.cell();
    if !(cell.is_finite() && cell > 0.0) || cols.nx() == 0 || cols.ny() == 0 {
        return Err(GenError::Invalid(
            "the stock of the previous operations has no readable grid: rest machining cannot be planned from it".into(),
        ));
    }
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    if levels.is_empty() || levels.iter().any(|z| !z.is_finite()) {
        return Err(GenError::Invalid("rest machining needs finite cut depths to read the stock at".into()));
    }
    let grow = cell * std::f64::consts::SQRT_2;
    let o = cols.origin();
    let (x1, y1) = (o.x + cols.nx() as f64 * cell, o.y + cols.ny() as f64 * cell);
    let lattice = Region::new(vec![Loop::new(vec![o, Point2::new(x1, o.y), Point2::new(x1, y1), Point2::new(o.x, y1)])?])?;
    let mut uncertain = if g.outside { region.difference(&lattice)? } else { Region::empty() };
    if g.unknown && unreadable(cols, region, grow) {
        uncertain = region.clone();
    }
    let mut floors = Vec::with_capacity(levels.len());
    for &z in levels {
        let above = cols
            .above(z + PRIOR_Z_TOL)
            .map_err(|e| GenError::Invalid(format!("the stock of the previous operations cannot be read at Z{z:.3}: {e}")))?;
        let read = if g.cell && !above.is_empty() { above.offset(grow)? } else { above };
        floors.push(StockFloor { z, remaining: read.intersect(region)?, uncertain: uncertain.clone() });
    }
    RestStock::from_verified(floors)?.with_film(grow)
}

/// Does a column that can meet `region` have no readable top (NaN)?
fn unreadable(cols: &Columns<'_>, region: &Region, pad: f64) -> bool {
    let Some((lo, hi)) = region.bounds() else {
        return false;
    };
    let (cell, o) = (cols.cell(), cols.origin());
    let span = |a: f64, b: f64, o: f64, n: usize| {
        let i0 = ((a - pad - o) / cell).floor().clamp(0.0, n as f64) as usize;
        let i1 = ((b + pad - o) / cell).ceil().clamp(0.0, n as f64) as usize;
        i0..i1
    };
    let xs = span(lo.x, hi.x, o.x, cols.nx());
    span(lo.y, hi.y, o.y, cols.ny()).any(|iy| xs.clone().any(|ix| cols.top_at(ix, iy).is_nan()))
}

/// The rest plan of `region` for a tool of `tool_diameter` leaving
/// `stock_to_leave`, on the proved `stock`, cut at `levels`. The rest
/// area is the material standing at ANY of the levels, grown by
/// `allowance`, inside the pocket. An empty plan (the earlier operations
/// cleared every level) is `Ok` with empty regions.
///
/// Refused by name: a level the stock does not prove ([`RestStock`]),
/// bad numbers, an empty pocket, a pocket narrower than the tool.
pub fn rest_plan_stock(
    region: &Region,
    tool_diameter: f64,
    stock_to_leave: f64,
    allowance: f64,
    stock: &RestStock,
    levels: &[f64],
) -> Result<RestPlan, GenError> {
    plan_from(region, tool_diameter, stock_to_leave, allowance, stock, levels, Guards::ALL)
}

pub(super) fn plan_from(
    region: &Region,
    tool_diameter: f64,
    stock_to_leave: f64,
    allowance: f64,
    stock: &RestStock,
    levels: &[f64],
    g: Guards,
) -> Result<RestPlan, GenError> {
    let (target, own_center) = target_of(region, tool_diameter, stock_to_leave)?;
    if !(allowance.is_finite() && allowance >= 0.0) {
        return Err(GenError::Invalid("the rest allowance must not be negative".into()));
    }
    if levels.is_empty() {
        return Err(GenError::Invalid("rest machining needs finite cut depths to read the stock at".into()));
    }
    // The depth guard: every level speaks. Without it only the first does.
    let read = if g.depth { levels } else { &levels[..1] };
    let mut material = Region::empty();
    for &z in read {
        material = material.union(&stock.standing_at(z)?)?;
    }
    let raw = target.intersect(&material)?;
    if raw.is_empty() {
        return Ok(RestPlan { rest: Region::empty(), center: Region::empty(), previous_fits: true });
    }
    if target.difference(&raw)?.area().abs() <= WHOLE_TOL {
        return Ok(RestPlan { rest: target, center: own_center, previous_fits: false });
    }
    let rest = if allowance > 0.0 { raw.offset(allowance)?.intersect(&target)? } else { raw };
    let center = rest.offset(tool_diameter * 0.5 + REST_REACH)?.intersect(&own_center)?;
    Ok(RestPlan { rest, center, previous_fits: true })
}

/// What the operation is to leave free of material, and its own
/// cutter-center area.
fn target_of(region: &Region, tool_diameter: f64, stock_to_leave: f64) -> Result<(Region, Region), GenError> {
    if !(tool_diameter.is_finite() && stock_to_leave.is_finite()) || tool_diameter <= 0.0 || stock_to_leave < 0.0 {
        return Err(GenError::Invalid("tool diameter must be positive and stock to leave not negative".into()));
    }
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let own_center = region.offset(-(tool_diameter * 0.5 + stock_to_leave))?;
    if own_center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let target = if stock_to_leave > 0.0 { region.offset(-stock_to_leave)? } else { region.clone() };
    Ok((target, own_center))
}

/// Pocket `region` with the tool of `p`, cutting only what the earlier
/// operations are not proved to have cut. `prior` is the stock they
/// leave, in the generator's frame; `None` means that no earlier
/// operation cut, so the whole pocket stands. Levels, entries, links and
/// strategies are [`super::rest_pocket`]'s.
///
/// Refused by name: a rest group that is not in `PreviousOperations`
/// mode; `Adaptive`; a stock that cannot be read; nothing left to cut;
/// rest material that this tool cannot reach.
pub fn rest_pocket_previous(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
) -> Result<Toolpath, GenError> {
    rest_pocket_previous_with_relief(region, p, rest, prior, &CornerRelief::default())
}

/// [`rest_pocket_previous`] with optional inside-corner reliefs (#2400)
/// in the corners that the earlier operations left.
pub fn rest_pocket_previous_with_relief(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    rest_pocket_previous_inner(region, p, rest, prior, None, relief)
}

/// [`rest_pocket_previous_with_relief`] keeping the full tool clear of `avoid` (#3345).
pub fn rest_pocket_previous_avoid_with_relief(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    rest_pocket_previous_inner(region, p, rest, prior, Some(avoid), relief)
}

fn rest_pocket_previous_inner(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
    avoid: Option<&AvoidSpec>,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    relief.validate()?;
    if rest.source != RestSource::PreviousOperations {
        return Err(GenError::Invalid(
            "this rest group reads the entered previous tool: it is not planned from the previous operations".into(),
        ));
    }
    rest.validate_alone()?;
    let p = &PocketParams { strategy: rest_strategy(p.strategy)?, ..*p };
    let zs = cut_levels_under(p.top_z, p.links.approach.stock_top, p.bottom_z, p.stepdown)?;
    let bottom = zs.iter().copied().fold(f64::INFINITY, f64::min);
    let (plan, read) = match prior {
        Some(cols) => {
            let stock = rest_stock_from_columns(cols, region, &zs)?;
            let plan = rest_plan_stock(region, p.tool_diameter, p.stock_to_leave, rest.allowance, &stock, &zs)?;
            (plan, format!("on a {:.3} mm grid, read large by one cell", cols.cell()))
        }
        None => {
            let (target, own_center) = target_of(region, p.tool_diameter, p.stock_to_leave)?;
            (RestPlan { rest: target, center: own_center, previous_fits: false }, "no earlier operation cut".to_string())
        }
    };
    if plan.rest.is_empty() {
        return Err(GenError::Empty(format!(
            "no rest material: the previous operations cleared this pocket at every cut depth down to Z{bottom:.3}"
        )));
    }
    if plan.center.is_empty() {
        return Err(GenError::Invalid(
            "the rest material is narrower than the tool: no legal tool position touches it".into(),
        ));
    }
    let mut notes = vec![format!(
        "rest machining reads the stock that the previous operations leave ({read}) at {} cut depth(s) down to Z{bottom:.3}: {:.1} of {:.1} mm2 is read as rest material",
        zs.len(),
        plan.rest.area().abs(),
        region.area().abs(),
    )];
    if !plan.previous_fits {
        notes.push("the previous operations cleared nothing of this pocket at its cut depths: the whole pocket is cut".into());
    }
    emit_rest_avoid(region, p, &plan, notes, relief, avoid)
}

#[cfg(test)]
#[path = "tests/rest_prior.rs"]
mod tests;

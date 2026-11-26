//! 2D Adaptive rest machining from the previous operations (#1446): the
//! stock that the earlier operations of the setup leave (`rest_prior`,
//! #2033) is handed to the rest Adaptive planner (`adaptive::rest`,
//! #2031), which plunges only where that stock proves the whole cutter
//! clear and holds the load on the material that stands.
//!
//! Provenance: the in-process workpiece of 2.5D pocketing as planar
//! regions per depth (M. Held, "On the Computational Geometry of Pocket
//! Machining", LNCS 500, Springer 1991; Z. Yao, S. K. Gupta and D. S.
//! Nau, "Algorithms for selecting cutters in multi-part milling
//! problems", Computer-Aided Design 35(9), 2003). The rules below are
//! this project's decision; no other CAM product's code or output was
//! consulted.
//!
//! Prime Directive: nothing is guessed.
//! - An entered previous tool is not proof that material is absent
//!   (#2031): that mode is refused by name here.
//! - No stock of the previous operations (none cut, or the stock is not
//!   trusted) is refused by name: the planner has no proved-clear space
//!   to enter from, and a full pocket is the plain 2D Adaptive's work.
//! - The stock is read at exactly the depths the planner cuts
//!   (`adaptive::rest_levels`), so no cut depth is below a read floor.
//! - The allowance only grows the material that stands.
//! - The band that the column reading leaves along a cut wall is not
//!   planned for (`reading_film`). It still stands for every plunge.

use crate::cl::Toolpath;
use crate::geo2d::Region;
use crate::stock_sim::Columns;

use super::adaptive::{rest_adaptive, rest_levels, RestStock, StockFloor};
use super::face::GenError;
use super::pocket::PocketParams;
use super::rest2d::{RestParams, RestSource};
use super::rest_prior::rest_stock_from_columns;

/// The refusal of a rest group in entered-tool mode on a 2D Adaptive.
pub const REST_ADAPTIVE_ENTERED: &str = "rest machining on a 2D Adaptive reads the previous operations only: an entered previous tool does not prove where material is absent - set the rest stock to the previous operations, or use a 2D Pocket";

/// The refusal when the previous operations give no stock to plan on.
pub const REST_ADAPTIVE_NO_STOCK: &str = "rest machining on a 2D Adaptive needs a cut of the previous operations to enter from: no stock that they leave is established - generate a roughing operation ahead in the same setup, or turn rest machining off";

/// The reading guards, each on in every build. The unit tests switch one
/// off to show that its pinning test then fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Guards {
    /// The entered-tool mode is refused.
    pub source: bool,
    /// A missing stock is refused.
    pub stock: bool,
    /// The allowance grows the standing material.
    pub allowance: bool,
}

impl Guards {
    pub(super) const ALL: Guards = Guards { source: true, stock: true, allowance: true };
}

/// Clear `region` with the tool of `p` by Adaptive passes that cut only
/// what the earlier operations are not proved to have cut. `prior` is
/// the stock they leave, in the generator's frame.
///
/// Refused by name: a rest group that is not in `PreviousOperations`
/// mode; bad rest values; no stock (`prior` is `None`); a stock that
/// cannot be read; and every refusal of [`rest_adaptive`] (no
/// proved-clear entry, a load that cannot be held, nothing left to cut).
pub fn rest_adaptive_previous(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
) -> Result<Toolpath, GenError> {
    guarded(region, p, rest, prior, Guards::ALL)
}

pub(super) fn guarded(
    region: &Region,
    p: &PocketParams,
    rest: &RestParams,
    prior: Option<&Columns<'_>>,
    g: Guards,
) -> Result<Toolpath, GenError> {
    if g.source && rest.source != RestSource::PreviousOperations {
        return Err(GenError::Invalid(REST_ADAPTIVE_ENTERED.into()));
    }
    rest.validate_alone()?;
    let zs = rest_levels(p)?;
    let stock = match prior {
        Some(cols) => rest_stock_from_columns(cols, region, &zs)?.with_film(reading_film(cols))?,
        None if g.stock => return Err(GenError::Invalid(REST_ADAPTIVE_NO_STOCK.into())),
        // Guard off (tests only): the pocket reads as cleared.
        None => RestStock::from_verified(
            zs.iter().map(|&z| StockFloor { z, remaining: Region::empty(), uncertain: Region::empty() }).collect(),
        )?,
    };
    let stock = if g.allowance { grown(&stock, region, rest.allowance)? } else { stock };
    let mut path = rest_adaptive(region, p, &stock)?;
    let bottom = zs.iter().copied().fold(f64::INFINITY, f64::min);
    let cell = prior.map_or(0.0, Columns::cell);
    path.notes.push(format!(
        "rest machining reads the stock that the previous operations leave (on a {cell:.3} mm grid, read large by one cell) at {} cut depth(s) down to Z{bottom:.3}: residue thinner than {:.3} mm is the reading's own band along a cut wall, and no pass is planned for it",
        zs.len(),
        stock.film()
    ));
    Ok(path)
}

/// The thickest band that the column reading alone leaves along a wall
/// that an earlier tool cut to: a column that stands reaches up to one
/// cell past the wall, and the reading grows it by one cell diagonal
/// (`rest_prior`). The planner plans no pass for residue thinner than
/// this (`RestStock::with_film`); the band still stands for every plunge
/// and every load reading.
fn reading_film(cols: &Columns<'_>) -> f64 {
    cols.cell() * (1.0 + std::f64::consts::SQRT_2)
}

/// `stock` with the material of every floor grown by `allowance` inside
/// `region`. The material is never made smaller. Only material thicker
/// than the stock's film grows: the reading's band along a cut wall
/// stays a band, so the allowance does not turn it into a pass along
/// every wall.
fn grown(stock: &RestStock, region: &Region, allowance: f64) -> Result<RestStock, GenError> {
    if allowance <= 0.0 {
        return Ok(stock.clone());
    }
    let half = 0.5 * stock.film();
    let mut floors = Vec::with_capacity(stock.floors().len());
    for f in stock.floors() {
        let body = if f.remaining.is_empty() || half <= 0.0 { f.remaining.clone() } else { f.remaining.offset(-half)? };
        let remaining = if body.is_empty() {
            f.remaining.clone()
        } else {
            body.offset(half + allowance)?.intersect(region)?.union(&f.remaining)?
        };
        floors.push(StockFloor { z: f.z, remaining, uncertain: f.uncertain.clone() });
    }
    RestStock::from_verified(floors)?.with_film(stock.film())
}

#[cfg(test)]
#[path = "tests/rest_prior_adaptive.rs"]
mod tests;

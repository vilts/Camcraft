//! The open sides of a whole-model 3D pocket (#1749): where the model's
//! silhouette edge has air beside it, the cutter may pass the edge.
//!
//! Provenance: the "tool centre on boundary" containment of a machining
//! boundary, as public CAM documentation describes it, read with the
//! level law of this module's parent; our own design, no vendor
//! material consulted.
//!
//! The level law erodes each level's target by the tool radius, so a
//! footprint that IS the silhouette keeps the cutter's edge inside it:
//! a notch open to the part's outside keeps a band a radius wide along
//! its open edge, the full stock height tall. Outside the silhouette no
//! part material stands at any Z (that is what a silhouette is), so the
//! footprint grows past it by just under a radius: the centre then rides
//! up to the silhouette edge where it is open, and where a level's
//! material stands flush with the silhouette the grown band is narrower
//! than the erosion and cuts nothing. Other parts beside the model are
//! taken out of the band, and the erosion keeps a full radius off them.
//!
//! #1774: where the stock reaches past the silhouette by more than that
//! band, the band grows from the stock's outline too - the 2D pocket's
//! reading since #12: a whole model clears the stock around it out to
//! the stock's edge, never a ring of the stock left standing past the
//! band while the op reads ok.

use crate::geo2d::Region;
use crate::machining2d::GenError;

/// How far short of a full radius the footprint grows (mm): a band of
/// exactly the radius would erode to a zero-width sliver along every
/// flush wall, so the band stays this much narrower and erodes to
/// nothing there. The centre then stops this far inside an open edge,
/// and the cutter still passes the edge by the radius less this.
pub const OPEN_SLACK: f64 = 0.01;

/// The footprint of a whole-model 3D pocket whose centre may ride the
/// silhouette: `silhouette` united with the offset by `r - OPEN_SLACK`
/// of the silhouette and the offset by `stock_band` of `stock` (the
/// setup's stock outline, #1774; `None` with no stock set up), less
/// `keep_out` (the setup's other parts' silhouettes). A radius too
/// small for the slack leaves the silhouette as it is.
///
/// Issue #1950: `stock_band` is [`stock_band`] of the strategy - the
/// Adaptive ring is entered from the air and needs the whole disc there
/// ([`ADAPTIVE_BAND_PER_R`]); every other strategy keeps `r - OPEN_SLACK`.
pub fn open_sides(silhouette: &Region, r: f64, keep_out: &Region, stock: Option<&Region>, stock_band: f64) -> Result<Region, GenError> {
    let grow = r - OPEN_SLACK;
    if !grow.is_finite() || grow <= 0.0 || silhouette.is_empty() {
        return Ok(silhouette.clone());
    }
    let reach = match stock.filter(|s| !s.is_empty()) {
        None => silhouette.offset(grow)?,
        Some(_) if !(stock_band.is_finite() && stock_band >= grow) => {
            return Err(GenError::Invalid(format!("open sides: the stock band {stock_band} is under the open band {grow}")));
        }
        // The band every strategy had, computed as it always was.
        Some(s) if stock_band == grow => silhouette.union(s)?.offset(grow)?,
        Some(s) => silhouette.offset(grow)?.union(&s.offset(stock_band)?)?,
    };
    let band = reach.difference(keep_out)?;
    Ok(silhouette.union(&band)?)
}

/// The Adaptive stock band in tool radii (plus [`OPEN_SLACK`]): the
/// tool centre reaches a full radius past the stock, the air entry disc
/// clear of it (issue #1950).
pub const ADAPTIVE_BAND_PER_R: f64 = 2.0;

/// The stock band of `open_sides` for a tool of radius `r`: the
/// Adaptive one (`adaptive`), else the open band every strategy had.
pub fn stock_band(r: f64, adaptive: bool) -> f64 {
    if adaptive { ADAPTIVE_BAND_PER_R * r + OPEN_SLACK } else { r - OPEN_SLACK }
}

//! 2D rest machining with the Adaptive strategy (issue 2031, part of
//! issue 1490): the material earlier operations left in a pocket, cut
//! from the space they are PROVED to have cleared, with the load held
//! on that remaining stock at every cut depth.
//!
//! The plain Adaptive plan (`adaptive`, `pocket_hsm`) starts from a full
//! pocket and its own entry bore. Here nothing is assumed cut: the
//! planner is given, per cut depth, a legal cutter-center area and a
//! conservative bound on the material that still stands ([`RestStock`]),
//! plunges only where the stock proves the whole cutter footprint and
//! its approach clear (`entry`), and peels the material from the
//! cleared side one engagement front at a time (`peel`, `runs`), each
//! pass read on a stock raster that never reads the material small.
//!
//! Provenance: pocket geometry as offset regions (M. Held, "On the
//! Computational Geometry of Pocket Machining", LNCS 500, Springer
//! 1991); the material a larger cutter leaves a smaller one (Z. Yao, S.
//! K. Gupta and D. S. Nau, "Algorithms for selecting cutters in
//! multi-part milling problems", Computer-Aided Design 35(9), 2003);
//! the engagement front of the in-process stock (J. A. Stori and P. K.
//! Wright, J. Manufacturing Systems 19(3), 2000). The clearance and
//! stock-proof contract, the guards and every refusal are this
//! project's own. No other CAM product's code or output was consulted.
//!
//! Prime Directive: refused by name, never guessed - stock that is not
//! proved, a cut depth below every proved floor, an entry without
//! proved clearance, a load or a coverage that cannot be proved, a plan
//! past its work budget or its travel budget (`budget`). Every move below the retract plane is a feed
//! or a plunge; every plunge stands where the stock proves it clear.

mod budget;
mod canon;
mod entry;
mod leads;
mod pass;
mod peel;
mod post;
mod runs;
mod stock;
#[cfg(test)]
mod tests;

use crate::cl::Toolpath;
use crate::geo2d::Region;
use crate::machining2d::levels::cut_levels_under;
use crate::machining2d::pocket::{PocketParams, PocketStrategy};
use crate::machining2d::GenError;

use peel::{Level, LevelPlan};
use stock::Guards;
pub use stock::{REST_STOCK_FILM, RestStock, StockFloor};

/// Pocket `region` with the tool of `p`, cutting only the material
/// `stock` says the earlier operations left, by Adaptive passes whose
/// load is held on that material.
///
/// Read from `p`: the tool, the stock to leave, the planes, the cut
/// depths, the feeds, the cut direction and the optimal load. The entry
/// is always a plunge into space the stock proves clear (the `entry`
/// family and the pocket's links describe entries into material, which
/// a rest pass never makes).
///
/// Refused by name: stock with no proved floor; a cut depth below every
/// proved floor; rest material with no proved-clear entry; a pass whose
/// load cannot be held; coverage that cannot be proved; a plan past its
/// work or travel budget; nothing left to cut.
pub fn rest_adaptive(region: &Region, p: &PocketParams, stock: &RestStock) -> Result<Toolpath, GenError> {
    guarded(region, p, stock, Guards::ALL)
}

/// [`rest_adaptive`] under `guards` (all on outside the unit tests).
fn guarded(region: &Region, p: &PocketParams, stock: &RestStock, guards: Guards) -> Result<Toolpath, GenError> {
    PocketParams { strategy: PocketStrategy::Adaptive, ..*p }.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let r = p.tool_diameter * 0.5;
    let center = region.offset(-(r + p.stock_to_leave))?;
    if center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let reach = center.offset(r)?;
    // One plan per answer of the stock: depths whose material bound is
    // the same region share it.
    let mut plans: Vec<(Region, LevelPlan)> = Vec::new();
    let mut levels: Vec<(f64, usize)> = Vec::new();
    // The rest stock stands over the rest Top Z (the earlier operations
    // left it there), so Top Z is a cut level, not an air level (#2004):
    // with no material top stated, the ladder keeps the plane at Top Z.
    let stock_top = p.links.approach.stock_top.or(Some(f64::INFINITY));
    let zs = cut_levels_under(p.top_z, stock_top, p.bottom_z, p.stepdown)?;
    // The depth guard speaks first, for every depth, before anything is
    // planned (`stock`).
    let materials = zs.iter().map(|&z| stock.material_at(z, guards)).collect::<Result<Vec<Region>, GenError>>()?;
    for (z, material) in zs.into_iter().zip(materials) {
        let at = match plans.iter().position(|(m, _)| same(m, &material)) {
            Some(i) => i,
            None => {
                let lv = Level {
                    center: &center,
                    reach: &reach,
                    material: &material,
                    film: stock.film(),
                    r,
                    load: p.optimal_load,
                    climb: p.climb,
                    z,
                    guards,
                };
                let plan = peel::plan(&lv)?;
                plans.push((material, plan));
                plans.len() - 1
            }
        };
        levels.push((z, at));
    }
    if levels.iter().all(|&(_, i)| plans[i].1.chains.is_empty()) {
        return Err(GenError::Empty(
            "no rest material: the remaining stock leaves nothing in this tool's reach in this pocket".into(),
        ));
    }
    let path = post::post(p, &levels, &plans, stock);
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Do two regions cover the same area (whatever their vertex order)?
fn same(a: &Region, b: &Region) -> bool {
    let empty = |r: Result<Region, crate::geo2d::GeoError>| r.is_ok_and(|r| r.is_empty());
    a == b || (empty(a.difference(b)) && empty(b.difference(a)))
}

//! The two-face carried verdict, split for the module size limit.
//! Provenance: the existing column verdict and credit law, unchanged.

use super::carry::{Body, mirror_z};
use super::deviate::{Deviation, deviate};
use super::dexel::Dexel;
use super::kernel::StockError;
use crate::geometry::{Point3, Triangle};

impl Body {
    /// The verdict against the part, in the body's frame: the top
    /// against the part's top as the column verdict always read it, and,
    /// once the floor has faced a tool, the floor against the part's
    /// bottom. Per column the worse of the two stands (a gouge before
    /// rest); the aggregates add up.
    ///
    /// # Errors
    /// A non-finite tolerance.
    pub fn deviate(&self, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
        self.deviate_with_top_credit(facets, tol, |_, _| {})
    }

    /// Credit declared top cuts before combining the two face verdicts.
    /// The underside keeps its own depths, locations and counts even when
    /// a deeper top cut in the same column receives credit.
    ///
    /// # Errors
    /// A non-finite tolerance.
    pub fn deviate_with_top_credit(
        &self,
        facets: &[Triangle],
        tol: f64,
        credit: impl FnOnce(&mut Deviation, &Dexel),
    ) -> Result<Deviation, StockError> {
        if let Some(h) = self.dex.history.as_ref().filter(|h| h.active) {
            let mut current = h.read(self.floor_judged()).map_err(StockError::Invalid)?;
            current.dex.history = None;
            return current.deviate_with_top_credit(facets, tol, credit);
        }
        let mut top = deviate(&self.dex, facets, tol)?;
        credit(&mut top, &self.dex);
        if !self.floor_judged() {
            return Ok(top);
        }
        let d = &self.dex;
        let neg: Vec<f64> = self.floor.iter().map(|f| -f).collect();
        let mirror = Dexel::from_grid(d.x0, d.y0, d.cell, d.nx, d.ny, -d.stock_z, neg);
        let flipped: Vec<Triangle> = facets.iter().map(mirror_z).collect();
        let mut bot = deviate(&mirror, &flipped, tol)?;
        let back = |p: Option<Point3>| p.map(|p| Point3::new(p.x, p.y, -p.z));
        bot.max_rest_at = back(bot.max_rest_at);
        bot.max_gouge_at = back(bot.max_gouge_at);
        Ok(merge(top, bot))
    }

}

fn merge(top: Deviation, bot: Deviation) -> Deviation {
    let tol = top.tol;
    let dev = top
        .dev
        .iter()
        .zip(&bot.dev)
        .map(|(&a, &b)| match (a.is_nan(), b.is_nan()) {
            (true, true) => f64::NAN,
            (true, false) => b,
            (false, true) => a,
            (false, false) if a < -tol || b < -tol => a.min(b),
            (false, false) => a.max(b),
        })
        .collect();
    let (max_rest, max_rest_at) =
        if bot.max_rest > top.max_rest { (bot.max_rest, bot.max_rest_at) } else { (top.max_rest, top.max_rest_at) };
    let (max_gouge, max_gouge_at) = if bot.max_gouge > top.max_gouge {
        (bot.max_gouge, bot.max_gouge_at)
    } else {
        (top.max_gouge, top.max_gouge_at)
    };
    Deviation {
        cells_x: top.cells_x,
        cells_y: top.cells_y,
        cell: top.cell,
        tol,
        dev,
        rest_cells: top.rest_cells + bot.rest_cells,
        gouge_cells: top.gouge_cells + bot.gouge_cells,
        rest_volume: top.rest_volume + bot.rest_volume,
        gouge_volume: top.gouge_volume + bot.gouge_volume,
        max_rest,
        max_gouge,
        max_rest_at,
        max_gouge_at,
    }
}

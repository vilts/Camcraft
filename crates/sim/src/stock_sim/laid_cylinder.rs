//! Provenance: analytic circle sections, z = centre +/- sqrt(r*r - offset*offset) (#2758).

use crate::geometry::Point3;
use super::{dexel::Dexel, kernel::StockError};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LaidCylinder {
    pub origin: Point3,
    radius: f64,
    length: f64,
    along_x: bool,
}

impl LaidCylinder {
    fn section(&self, q: crate::geo2d::Point2) -> Option<(f64, f64)> {
        let (long, cross) = if self.along_x {
            (q.x - self.origin.x, q.y - self.origin.y)
        } else {
            (q.y - self.origin.y, q.x - self.origin.x)
        };
        if !(0.0..=self.length).contains(&long) || cross.abs() >= self.radius {
            return None;
        }
        // The ratio avoids squaring dimensions that could overflow.
        let ratio = cross / self.radius;
        let half = self.radius * (1.0 - ratio * ratio).max(0.0).sqrt();
        Some((self.origin.z - half, self.origin.z + half))
    }
}

impl Dexel {
    /// A horizontal cylinder, starting at zero along +X or +Y. Its axis
    /// stands at Z=0. Translate the grid to place the stock in world.
    ///
    /// # Errors
    /// Invalid dimensions, cell size or a grid beyond the cell limit.
    pub fn laid_cylinder(diameter: f64, height: f64, along_x: bool, cell: f64) -> Result<Self, StockError> {
        if !(diameter.is_finite() && height.is_finite() && diameter > 0.0 && height > 0.0) {
            return Err(StockError::Invalid("laid cylinder stock needs finite positive diameter, height".into()));
        }
        let r = diameter * 0.5;
        let (x0, y0, x1, y1) = if along_x { (0.0, -r, height, r) } else { (-r, 0.0, r, height) };
        let mut dex = Self::block(x0, y0, x1, y1, r, cell)?;
        let cylinder = LaidCylinder { origin: Point3::ZERO, radius: r, length: height, along_x };
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let top = cylinder.section(dex.center(ix, iy)).map_or(f64::NEG_INFINITY, |(_, hi)| hi);
                dex.set(ix + iy * dex.nx, top);
            }
        }
        dex.laid = Some(cylinder);
        Ok(dex)
    }

    /// The initial rounded underside, when this grid models a laid
    /// cylinder. `Some(+inf)` denotes a column outside the initial stock.
    #[must_use]
    pub(super) fn stock_floor_at(&self, k: usize) -> Option<f64> {
        self.laid.as_ref().map(|c| c.section(self.center(k % self.nx, k / self.nx))
            .map_or(f64::INFINITY, |(lo, _)| lo))
    }

    pub(super) fn removed_depth(&self, k: usize) -> f64 {
        let sim = self.top_at(k);
        match &self.laid {
            Some(c) => c.section(self.center(k % self.nx, k / self.nx))
                .map_or(0.0, |(lo, hi)| (hi - sim.max(lo)).max(0.0)),
            None => (self.stock_z - sim).max(0.0),
        }
    }
}

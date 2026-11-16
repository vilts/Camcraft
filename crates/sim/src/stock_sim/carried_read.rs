//! The field arm's read-back (#637 step E3): a carried field's columns
//! read onto its lattice as the column [`Body`] every carried verdict is
//! made of.
//!
//! ## The laws here
//!
//! - **One exact read per column.** Under a lattice turn a vertical
//!   column of the current frame is a vertical column of the field's own,
//!   and the lattice was built so its centres are the field's cell
//!   centres: each column is one `column_span` (the top by the down-march,
//!   the floor from the shell or the cut records), swapped by a flip.
//! - **A cut-through column is convicted, not air.** The column engine
//!   keeps the tool's depth in a column it cut through (`top < floor`,
//!   both finite), so the verdict still judges it against the part. The
//!   field knows only that nothing stands there. It reads the top at the
//!   stock's own floor and the floor at the stock's own top: the column
//!   stays empty (`Body::empty_at`), the verdict convicts the part from
//!   above (and from below once the floor is judged), and the depth it
//!   reports is the stock's own thickness - the part lies inside the
//!   stock, so that is at least everything the part had there. Writing
//!   "air" (`-inf`) instead would read as "nothing to judge": a false
//!   clear.
//! - **A clip is applied here.** Each `Ceiling` bounds the column from
//!   above (from below after a flip), only where the clip's own grid has
//!   a standing column - `clip_columns_under`'s law, column for column.
//! - **No guess.** A column the field cannot read, or a floor a bounded
//!   tool cut near, refuses the whole read by name.
//!
//! Provenance: our own design (#637 design comments, step D's read-back).

use super::carried::{clip_xy, lattice_ok, FieldBody};
use super::carry::Body;
use super::dexel::Dexel;
use super::field_floor::{shell_extent, ColumnSpan};
use crate::geometry::Point3;

impl FieldBody {
    /// One column's (top, floor) in the field's own frame at `(x, y)`;
    /// `None`: air beside the stock.
    fn span(&self, x: f64, y: f64) -> Result<Option<(f64, f64)>, String> {
        let f = &self.carried.field;
        match f.column_span(x, y) {
            ColumnSpan::Material { top, floor } if top.is_finite() && floor.is_finite() => Ok(Some((top, floor))),
            ColumnSpan::Material { .. } => {
                Err(format!("the carried stock read a non-finite top or floor at ({x:.4}, {y:.4}) - refused rather than guessed"))
            }
            // Nothing stands: beside the stock (no shell on the column),
            // or cut through (the module law).
            ColumnSpan::Empty => match f.shell_crossings(x, y).as_deref().and_then(shell_extent) {
                Some((lo, hi)) if lo < hi => Ok(Some((lo, hi))),
                _ => Ok(None),
            },
            ColumnSpan::CutFromBelow => Err(format!(
                "a bounded tool's cut reached the floor of the carried stock at ({x:.4}, {y:.4}) - an upward read there is not exact, refused rather than guessed"
            )),
            ColumnSpan::Unreadable => Err(format!("the carried field cannot read its column at ({x:.4}, {y:.4})")),
        }
    }

    /// The carried stock as a column body on its lattice (current frame).
    ///
    /// # Errors
    /// A column the field cannot read; a lattice too large to index.
    pub fn body(&self) -> Result<Body, String> {
        let lat = self.lat;
        if !lattice_ok(&lat) {
            return Err("the carried lattice is not a finite, non-empty grid".to_owned());
        }
        let n = lat.nx.checked_mul(lat.ny).ok_or("the carried lattice is too large")?;
        let turn = &self.carried.turn;
        let tz = turn.apply(Point3::new(0.0, 0.0, 0.0)).z;
        let mut top = vec![f64::NEG_INFINITY; n];
        let mut floor = vec![f64::INFINITY; n];
        for jy in 0..lat.ny {
            for jx in 0..lat.nx {
                let cx = lat.x0 + (jx as f64 + 0.5) * lat.cell;
                let cy = lat.y0 + (jy as f64 + 0.5) * lat.cell;
                let q = turn.unapply(Point3::new(cx, cy, 0.0));
                let Some((t, f)) = self.span(q.x, q.y)? else { continue };
                // The turn's Z: z' = s z + tz; a flip swaps the two ends.
                let (mut t, mut f) = if turn.flips() { (tz - f, tz - t) } else { (t + tz, f + tz) };
                for c in &self.ceilings {
                    let Some(k) = c.grid.col(clip_xy(&c.since, cx, cy)) else { continue };
                    let z = c.grid.top_at(k);
                    if !z.is_finite() {
                        continue;
                    }
                    let z = c.since.apply(Point3::new(0.0, 0.0, z)).z;
                    if c.since.flips() {
                        f = f.max(z);
                    } else {
                        t = t.min(z);
                    }
                }
                let j = jx + jy * lat.nx;
                top[j] = t;
                floor[j] = f;
            }
        }
        let stock_z = top.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
        let stock_z = if stock_z.is_finite() { stock_z } else { 0.0 };
        let dex = Dexel::from_grid(lat.x0, lat.y0, lat.cell, lat.nx, lat.ny, stock_z, top);
        Body::from_parts(dex, floor, self.carried.flipped())
    }
}

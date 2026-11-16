//! The box atom (#541): a sinker EDM electrode's rectangular face,
//! axis-aligned in XY, swept along straight Z-linear chords through the
//! column grid - the electrode as the tool solid (#290 D7). Everything
//! above the face is the electrode (or its holder), so a column under
//! the face is lowered to the face's lowest height over the window it
//! spends there, exactly as the flat disc's `cut_chord` does.
//!
//! Provenance: the Minkowski sweep of an axis-aligned rectangle along a
//! segment, read per column as the intersection of the two per-axis
//! parameter windows `|q - p(t)| <= h` - textbook interval arithmetic;
//! our own design.

use super::dexel::Dexel;
use super::kernel::StockError;

use crate::cl::{Move, MoveKind};
use crate::geometry::Point3;

/// The refusal of an arc under the box atom, said once.
pub const BOX_ARC_REFUSAL: &str =
    "a sinker electrode's box atom sweeps straight moves only - an arc under a rectangular electrode is not modelled";

/// The window of `t` in `[0, 1]` where `|w - t*d| <= h` on one axis,
/// or `None` when the face never covers the column on that axis.
fn axis_window(w: f64, d: f64, h: f64) -> Option<(f64, f64)> {
    if d.abs() <= f64::EPSILON {
        return (w.abs() <= h).then_some((0.0, 1.0));
    }
    let (a, b) = ((w - h) / d, (w + h) / d);
    let (lo, hi) = (a.min(b).max(0.0), a.max(b).min(1.0));
    (lo <= hi).then_some((lo, hi))
}

impl Dexel {
    /// Lower every column the face `±hx × ±hy` covers while its centre
    /// rides the chord `a -> b`, to the face's lowest height over that
    /// window. Exact for a straight chord: Z is linear in `t`, so its
    /// minimum over the window is at one of the window's ends.
    pub fn cut_box(&mut self, a: Point3, b: Point3, hx: f64, hy: f64) {
        if !(a.is_finite() && b.is_finite() && hx.is_finite() && hy.is_finite() && hx >= 0.0 && hy >= 0.0) {
            return; // the gate in `simulate_box` refuses before this
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - hx).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + hx).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - hy).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + hy).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let Some((x0, x1)) = axis_window(q.x - a.x, dx, hx) else { continue };
                let Some((y0, y1)) = axis_window(q.y - a.y, dy, hy) else { continue };
                let (t0, t1) = (x0.max(y0), x1.min(y1));
                if t0 > t1 {
                    continue;
                }
                let za = a.z + (b.z - a.z) * t0;
                let zb = a.z + (b.z - a.z) * t1;
                self.lower(ix + iy * self.nx, za.min(zb));
            }
        }
    }
}

/// Run one electrode's moves through the grid: the box atom of half
/// sizes `hx`, `hy` (the face centre rides the moves), burning a spark
/// `gap` beyond the face on every side and below it - the cavity a
/// sinker leaves is the electrode grown by its condition's gap (#531).
/// Rapids touch
/// nothing (RS-274 G0, as for every atom); a non-finite move, a posed
/// cutting move, an arc, or a garbled face is refused by name BEFORE
/// any column is lowered, so a refusal never leaves a half-run grid.
pub fn simulate_box(dex: &mut Dexel, moves: &[Move], hx: f64, hy: f64, gap: f64) -> Result<(), StockError> {
    if !(hx.is_finite() && hy.is_finite() && hx > 0.0 && hy > 0.0) {
        return Err(StockError::Invalid(format!(
            "electrode face half sizes {hx} x {hy} are not finite-positive"
        )));
    }
    if !(gap.is_finite() && gap >= 0.0) {
        return Err(StockError::Invalid(format!("electrode spark gap {gap} is not finite and non-negative")));
    }
    if !moves.iter().all(|m| m.is_finite()) {
        return Err(StockError::Invalid(
            "an electrode move is non-finite - the sim refuses instead of cutting nothing silently".into(),
        ));
    }
    if let Some(k) = super::cut::first_posed(moves) {
        return Err(StockError::Invalid(format!("electrode move {k}: {}", super::cut::POSED_REFUSAL)));
    }
    if let Some(k) = moves.iter().position(|m| m.kind != MoveKind::Rapid && m.arc.is_some()) {
        return Err(StockError::Invalid(format!("electrode move {k}: {BOX_ARC_REFUSAL}")));
    }
    let down = |p: Point3| Point3::new(p.x, p.y, p.z - gap);
    for m in moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        dex.cut_box(down(m.from), down(m.to), hx + gap, hy + gap);
    }
    Ok(())
}

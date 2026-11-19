//! The display cut (M7.2 B3): the field arm's program subtraction with
//! the flat tool's flank sized to the STOCK, not to the verdict world.
//! Provenance: the M7.1 B closed-form swept-cylinder subtraction, fed
//! a shorter cylinder; nothing from vendor binaries.
//!
//! `Stock::simulate` gives the flat tool `spanning_flank` - a cylinder
//! reaching past anything a verdict march can stand on, the column
//! atom's infinite reach made exact. The cut window grows by the
//! tool's bounding radius on every axis, so that flank (the world's
//! diagonal) costs millions of cells per move. Inside the stock's own
//! box, a flank from the lowest tip to the stock's ceiling (plus the
//! band) removes exactly what the spanning one removes: every cell of
//! material within the radius and above a tip lies below the ceiling.
//! So the picture is the verdict's own geometry wherever there is
//! stock to see, at a window bounded by the stock's height. Posed
//! (multi-axis) moves fall back to the spanning law - a tilted flank
//! is not this argument's to shorten. (Since M7.3 every walk is
//! clipped to the region a cut can matter in and batched per brick -
//! `field_batch` - so the spanning flank no longer costs its length;
//! the shorter flank stays as this module's exactness argument.)

use super::backend::{op_sdf_tool, Stock};
use super::cut::{check_ops, SimOp};
use super::kernel::StockError;

impl Stock {
    /// `simulate`, with the flat tool's flank bounded by the stock's
    /// height (see the module law). The column arm is `simulate`
    /// itself; so is any program carrying a tool axis.
    pub fn simulate_display(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_display_with(ops, false)
    }

    /// [`Self::simulate_display`] for twist-drill operations (#2441): cuts a
    /// cylindrical flute above the cone (`SdfTool::Drill`) bounded by the
    /// stock height.
    pub fn simulate_drill_display(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_display_with(ops, true)
    }

    fn simulate_display_with(&mut self, ops: &[SimOp], drill: bool) -> Result<(), StockError> {
        let posed = ops.iter().any(|op| op.moves.iter().any(|m| m.axis.is_some()));
        let f = match self {
            Stock::Field(f) if !posed => f,
            _ => {
                return if drill { self.simulate_drill(ops) } else { self.simulate(ops) };
            }
        };
        check_ops(ops)?;
        let ceiling = f.bounds().max.z;
        let lowest = ops
            .iter()
            .flat_map(|op| op.moves.iter())
            .flat_map(|m| [m.from.z, m.to.z])
            .fold(f64::INFINITY, f64::min);
        let slack = 2.0 * f.cap() + 4.0 * f.cell();
        let flank = if lowest.is_finite() && ceiling.is_finite() {
            (ceiling - lowest).max(0.0) + slack
        } else {
            slack
        };
        let chord_tol = (f.cell() * 0.25).max(1e-9);
        for (i, op) in ops.iter().enumerate() {
            let tool = op_sdf_tool(op, flank, false, drill);
            for chunk in op.moves.chunks(super::field_batch::BATCH_MOVES) {
                f.subtract_batch(chunk, &tool, chord_tol).map_err(|e| match e {
                    StockError::Invalid(s) => StockError::Invalid(format!("op {i}: {s}")),
                    other => other,
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geometry::{cube, Point3};
    use crate::stock_sim::backend::{Backend, FIELD_BAND};

    fn feed(from: Point3, to: Point3) -> Move {
        Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
    }

    #[test]
    fn the_display_cut_is_the_spanning_cut_inside_the_stock() {
        // A plunge and a slot across the box: the picture's field and
        // the verdict's field must agree in sign everywhere in the
        // stock's box, and in value wherever a surface is resolved
        // (|d| < cap) - deep air may read the shorter cylinder's far
        // cap, which no surface and no sign can see.
        let c = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 12.0, 6.0));
        let mk = || Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 1.0).unwrap();
        let (mut verdict, mut picture) = (mk(), mk());
        let mv = vec![
            feed(Point3::new(4.0, 6.0, 8.0), Point3::new(4.0, 6.0, 3.0)),
            feed(Point3::new(4.0, 6.0, 3.0), Point3::new(16.0, 6.0, 2.0)),
        ];
        let ops = [SimOp { moves: &mv, radius: 2.0, ball: false, cone: None, corner: None }];
        verdict.simulate(&ops).unwrap();
        picture.simulate_display(&ops).unwrap();
        let cap = verdict.field().unwrap().cap();
        let mut n = 0;
        for i in 0..40 {
            for j in 0..24 {
                for k in 0..12 {
                    let p = Point3::new(i as f64 * 0.5 + 0.13, j as f64 * 0.5 + 0.07, k as f64 * 0.5 + 0.11);
                    let (a, b) = (verdict.distance(p), picture.distance(p));
                    assert_eq!(a < 0.0, b < 0.0, "sign differs at {p:?}: {a} vs {b}");
                    if a.abs() < cap {
                        assert!((a - b).abs() < 1e-9, "value differs at {p:?}: {a} vs {b}");
                        n += 1;
                    }
                }
            }
        }
        assert!(n > 1000, "the comparison saw the surfaces: {n} samples");
    }

    #[test]
    fn drill_engines_cut_cone_with_cylindrical_flute_and_agree() {
        let c = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0));
        let mut field = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 0.5).unwrap();
        let mut pic = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 0.5).unwrap();
        let mut col = Stock::from_facets(Backend::Column, &c.facets, 0.5).unwrap();
        let cot = 1.0 / 59.0_f64.to_radians().tan();
        let mv = vec![feed(Point3::new(10.25, 10.25, 12.0), Point3::new(10.25, 10.25, 2.0))];
        let ops = [SimOp { moves: &mv, radius: 3.0, ball: false, cone: Some(cot), corner: None }];
        field.simulate_drill(&ops).unwrap();
        pic.simulate_drill_display(&ops).unwrap();
        col.simulate_drill(&ops).unwrap();
        // At r = 2.0 mm from the hole center, the 118 deg cone rises to 2 + 2 * cot ≈ 3.202 mm.
        let want_z = 2.0 + 2.0 * cot;
        for (tag, s) in [("field", &field), ("display", &pic), ("column", &col)] {
            let z = s.top(12.25, 10.25);
            assert!((z - want_z).abs() < 1e-4, "{tag}: cone top at r=2 was {z}, want {want_z}");
        }
        // Check parity between field and column inside the full hole & outside flute.
        assert!((field.top(12.25, 10.25) - col.top(12.25, 10.25)).abs() < 1e-4);
        assert!((field.top(13.75, 10.25) - 10.0).abs() < 1e-4, "outside r=3 stays uncut");
    }
}

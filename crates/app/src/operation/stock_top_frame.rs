//! Stock caps for the #2004 level law in the cutting frame.
//! Provenance: rigid-frame projection of box corners; our own implementation.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::area::{ladder_planes, stock_top};
    use super::*;

    #[test]
    fn indexed_area_planes_keep_top_z_under_rotated_stock() {
        use crate::commands::{History, SetStock};
        let world = crate::world::new_world();
        let mut h = History::default();
        h.execute(
            &world,
            SetStock::new(
                &world,
                crate::stock::StockShape::Box {
                    dx: 80.0,
                    dy: 60.0,
                    dz: 20.0,
                },
                crate::schema::Location::default(),
            )
            .expect("stock"),
        );
        for (b, origin, cap) in [
            (90.0, [0.0; 3], 40.0),
            (45.0, [4.0, 0.0, 2.0], 54.0 / 2.0_f64.sqrt()),
        ] {
            let f = Tilt {
                basis_version: 1,
                a_deg: 0.0,
                b_deg: b,
                origin,
            }
            .frame()
            .expect("frame");
            let mt = stock_top(&world, None, &[], f64::NEG_INFINITY, Some(&f)).expect("stock cap");
            assert!((mt - cap).abs() < 1e-9, "B{b}: {mt} != {cap}");
            assert_eq!(
                ladder_planes(cap - 2.0, Some(mt), cap - 8.0, 3.0),
                vec![cap - 2.0, cap - 5.0, cap - 8.0]
            );
        }
    }

}

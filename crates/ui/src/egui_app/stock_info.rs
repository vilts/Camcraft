//! The Stock page's part-size readout (#197): the models' overall size and
//! their X/Y/Z extents in the document's units - the very box "Fit to
//! Models" wraps - so stock can be sized against it by hand.
//! Provenance: our own UI; the box is `stock_fit::part_bounds` - the parts,
//! never the fixtures (#1538).

use camcraft_app::units::{self, Unit};
use flecs_ecs::prelude::World;

/// The two readout lines - size, then extents - or `None` with no model
/// geometry in the document.
pub(super) fn part_size_lines(world: &World, u: Unit) -> Option<[String; 2]> {
    // #326: the part as the active setup holds it.
    let b = camcraft_app::stock_fit::part_bounds(world, camcraft_app::setup::active(world).as_deref())?;
    let (lo, hi) = (b.min, b.max);
    let s = |v: f64| units::len(v, u, 3);
    let size = format!(
        "Models: {} x {} x {} {}",
        s(hi.x - lo.x),
        s(hi.y - lo.y),
        s(hi.z - lo.z),
        u.suffix().trim()
    );
    let extents = format!(
        "X {} .. {}   Y {} .. {}   Z {} .. {}",
        s(lo.x),
        s(hi.x),
        s(lo.y),
        s(hi.y),
        s(lo.z),
        s(hi.z)
    );
    Some([size, extents])
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::commands::{History, ImportModel};
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::world::new_world;

    #[test]
    fn the_readout_names_the_models_size_and_extents() {
        let world = new_world();
        assert!(part_size_lines(&world, Unit::Millimeters).is_none(), "no models, no readout");
        let mut h = History::default();
        h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(-10.0, -5.0, 0.0), Point3::new(30.0, 15.0, 12.5))));
        let [size, extents] = part_size_lines(&world, Unit::Millimeters).expect("a model");
        assert_eq!(size, "Models: 40.000 x 20.000 x 12.500 mm", "{size}");
        assert_eq!(extents, "X -10.000 .. 30.000   Y -5.000 .. 15.000   Z 0.000 .. 12.500", "{extents}");
        let [inch, _] = part_size_lines(&world, Unit::Inches).expect("a model");
        assert!(inch.starts_with("Models: 1.57480 x 0.78740 x 0.49213"), "{inch}");
    }

    /// #1538: a 5 in vise closed on the stock is workholding, not a
    /// model - the readout is the 50 x 50 x 6 part's, as without it.
    #[test]
    fn the_readout_is_the_parts_never_the_vise() {
        use camcraft_app::commands::{InsertVise, SetStock, VisePlacement};
        use camcraft_app::schema::Location;
        use camcraft_app::stock::StockShape;
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(2.5, 2.5, 4.0), Point3::new(52.5, 52.5, 10.0))));
        let stock = SetStock::new(&world, StockShape::Box { dx: 55.0, dy: 55.0, dz: 11.0 }, Location::default());
        h.execute(&world, stock.expect("stock"));
        let p = camcraft_app::vise::size("5 in").expect("5 in").half_open();
        h.execute(&world, InsertVise::new(&world, &p, None, VisePlacement::FitStock).expect("vise"));
        let [size, extents] = part_size_lines(&world, Unit::Millimeters).expect("a part");
        assert_eq!(size, "Models: 50.000 x 50.000 x 6.000 mm", "{size}");
        assert_eq!(extents, "X 2.500 .. 52.500   Y 2.500 .. 52.500   Z 4.000 .. 10.000", "{extents}");
    }
}

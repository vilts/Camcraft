//! Document units: the one mm/inch choice a document displays in, and
//! (#902) the unit every program posts in - one setting, never a second
//! derivation from the machine file, a post option or a setup.
//!
//! Provenance: the international inch (exactly 25.4 mm) and the
//! internal-metric / display-at-the-edge pattern of CAD/CAM systems and
//! G-code (RS-274 `G20`/`G21`); general published practice, no vendor
//! library copied.
//!
//! Internal law: every stored number - geometry, the micron lattice
//! ([`crate::cl::lattice`]), toolpaths, the golden files, entity and draft
//! parameters - is millimeters, always. The unit is a *lens*: a flecs
//! singleton (document-level, on no entity, so it never appears in a
//! browser row or a saved entity list) applied at the widget and string
//! edges of the UI. The formatters themselves are pure Unit functions
//! and live in [`crate::geometry::units`] (the M4.1 crate seam); this
//! module keeps the singleton glue and re-exports them. A conversion never round-trips through a rounded
//! string: [`from_display`] snaps to the micron lattice, so inch-typed
//! values land as exact microns and the goldens keep their bits. The post
//! (M1.7) converts at output time and declares the mode with G20/G21.

use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::schema::Units;

pub use crate::geometry::units::{
    Unit, MM_PER_INCH, area, feed, from_display, len, len_signed, to_display, vol,
};

/// The document's unit; a document that never set one displays
/// millimeters, which is what every pre-v8 project file meant.
#[must_use]
pub fn of(world: &World) -> Unit {
    world
        .try_get::<&Units>(|u| u.0)
        .unwrap_or_default()
}

/// Write the document's unit. For the [`crate::commands::SetUnits`]
/// command and document lifecycle (New resets, Open lands the file's
/// value) only - the UI queues the command, it never pokes this.
pub fn set(world: &World, unit: Unit) {
    world.set(Units(unit));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::new_world;

    #[test]
    fn the_default_document_is_millimeters() {
        let w = new_world();
        assert_eq!(of(&w), Unit::Millimeters);
        set(&w, Unit::Inches);
        assert_eq!(of(&w), Unit::Inches);
        set(&w, Unit::Millimeters);
        assert_eq!(of(&w), Unit::Millimeters);
    }

    #[test]
    fn inch_input_lands_on_the_micron_lattice() {
        // The exact numbers first: the inch is exactly 25.4 mm.
        assert_eq!(from_display(1.0, Unit::Inches), 25.4);
        assert_eq!(from_display(0.5, Unit::Inches), 12.7);
        // A round-trip of any stored mm is the lattice snap of itself:
        // lossless to the micron, and a half-step never stores dust.
        for v in [3.0, 0.001_1, 12.345_678_9, 40.0, 1234.5] {
            let back = from_display(to_display(v, Unit::Inches), Unit::Inches);
            assert!((back - v).abs() < 5e-7, "{v} mm round-tripped to {back}");
        }
        // mm mode is identity: typing has never snapped mm drafts.
        assert_eq!(from_display(12.345_678_9, Unit::Millimeters), 12.345_678_9);
    }

    #[test]
    fn display_precision_keeps_mm_and_widens_inch() {
        // mm: exactly the site's historical formatting.
        assert_eq!(len(40.0, Unit::Millimeters, 2), "40.00");
        assert_eq!(len(12.34567, Unit::Millimeters, 1), "12.3");
        // inch: +2 decimals, so 1/8 in still shows its exact 0.1250.
        assert_eq!(len(25.4, Unit::Inches, 2), "1.0000");
        assert_eq!(len(3.175, Unit::Inches, 2), "0.1250");
        assert_eq!(len_signed(-0.05, Unit::Inches, 2), "-0.0020");
        assert_eq!(len_signed(25.4, Unit::Inches, 2), "+1.0000");
        assert_eq!(feed(900.0, Unit::Inches), "35.43");
        assert_eq!(feed(900.0, Unit::Millimeters), "900");
        assert_eq!(area(1600.0, Unit::Inches), "2.4800");
        assert_eq!(area(1600.0, Unit::Millimeters), "1600.0");
    }

    #[test]
    fn wire_spellings_round_trip() {
        for u in [Unit::Millimeters, Unit::Inches] {
            assert_eq!(Unit::parse(u.as_str()), Some(u));
        }
        assert_eq!(Unit::parse("MM"), None);
        assert_eq!(Unit::parse("m"), None);
    }
}

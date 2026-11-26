//! Display-only cache of the Geometry chip text of a DXF sketch pick (#1864 /
//! #2238): a chain's length and bounds and a layer's chain counts are read
//! when the drawing, the entity, the pick or the display unit changes, not
//! on each frame. Provenance: our own frame-cost memo over
//! `geometry::sketch_data`; no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use camcraft_app::operation::FeatureSel;
use camcraft_app::sketch_data::SketchData;
use camcraft_app::units::{self, Unit};
use flecs_ecs::prelude::{Entity, World};

/// Chips are all read on each frame; the cap holds a large pick list.
const CAP: usize = 256;

struct DrawingPtr(Weak<SketchData>);

impl PartialEq for DrawingPtr {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

#[derive(PartialEq)]
struct ChipKey {
    name: String,
    entity: Option<Entity>,
    drawing: Option<DrawingPtr>,
    feature: FeatureSel,
    unit: Unit,
}

/// The chip text after the pick name, and whether the pick is missing.
type Chip = (String, bool);

thread_local! {
    static CHIP_MEMO: RefCell<Vec<(ChipKey, Chip)>> = const { RefCell::new(Vec::new()) };
    static CHIP_BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// Uncached chip text of one sketch pick. None for a pick that is not a
/// sketch feature.
#[must_use]
pub(crate) fn sketch_chip_fresh(world: &World, entity: &str, feature: &FeatureSel, unit: Unit) -> Option<Chip> {
    let found = camcraft_app::sketch::find(world, entity);
    let chip = match feature {
        FeatureSel::SketchChain(id) => {
            let read = found.and_then(|(_, sk)| {
                let ch = sk.data.chains().find(*id)?;
                Some((ch.length(), ch.is_closed(), ch.layer().to_owned(), ch.bounds()))
            });
            match read {
                Some((len, true, layer, Some((lo, hi)))) => (
                    format!(
                        "closed · {} {} · {} × {} · {layer}",
                        units::len(len, unit, 1),
                        unit.suffix(),
                        units::len(hi.x - lo.x, unit, 1),
                        units::len(hi.y - lo.y, unit, 1),
                    ),
                    false,
                ),
                Some((len, _, layer, _)) => {
                    (format!("open · {} {} · {layer}", units::len(len, unit, 1), unit.suffix()), false)
                }
                None => ("missing".to_owned(), true),
            }
        }
        FeatureSel::SketchLayer(layer) => {
            let read = found.and_then(|(_, sk)| {
                sk.data.layers().iter().any(|l| l == layer).then(|| {
                    let chains = sk.data.chains().by_layer(layer);
                    let closed = chains.iter().filter(|c| c.is_closed()).count();
                    (closed, chains.len().saturating_sub(closed))
                })
            });
            match read {
                Some((closed, open)) => (format!("{closed} closed, {open} open"), false),
                None => ("missing".to_owned(), true),
            }
        }
        _ => return None,
    };
    Some(chip)
}

/// [`sketch_chip_fresh`], memoized by the entity, its live drawing
/// allocation, the pick and the display unit.
#[must_use]
pub(crate) fn sketch_chip(world: &World, entity: &str, feature: &FeatureSel, unit: Unit) -> Option<Chip> {
    if !matches!(feature, FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_)) {
        return None;
    }
    let found = camcraft_app::sketch::find(world, entity);
    let key = ChipKey {
        name: entity.to_owned(),
        entity: found.as_ref().map(|(e, _)| *e),
        drawing: found.map(|(_, sk)| DrawingPtr(Arc::downgrade(&sk.data))),
        feature: feature.clone(),
        unit,
    };
    let hit = CHIP_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|(k, _)| k.drawing.as_ref().is_none_or(|d| d.0.strong_count() > 0));
        let i = m.iter().position(|(k, _)| *k == key)?;
        let entry = m.remove(i);
        let chip = entry.1.clone();
        m.push(entry);
        Some(chip)
    });
    if hit.is_some() {
        return hit;
    }
    CHIP_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let chip = sketch_chip_fresh(world, entity, feature, unit)?;
    CHIP_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.remove(0);
        }
        m.push((key, chip.clone()));
    });
    Some(chip)
}

/// How many fresh [`sketch_chip`] reads ran on this thread.
#[cfg(test)]
#[must_use]
pub(crate) fn sketch_chip_builds() -> u64 {
    CHIP_BUILDS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::commands::{DeleteEntity, DxfOptions, History, ImportDxf, SetStock};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;
    use camcraft_app::world::new_world;

    fn dxf(width: &str) -> Vec<u8> {
        format!(
            "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
             0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n{width}\n20\n0\n10\n{width}\n20\n30\n10\n0\n20\n30\n\
             0\nCIRCLE\n8\nholes\n10\n20\n20\n15\n40\n4\n\
             0\nENDSEC\n0\nEOF\n"
        )
        .into_bytes()
    }

    /// #2716 review: the sketch chip text is read once for a still input,
    /// and again when the unit, the pick or the drawing changes.
    #[test]
    fn sketch_chips_skip_still_reads_and_track_changed_inputs() {
        let world = new_world();
        let mut hist = History::default();
        hist.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Location::default())
                .expect("stock"),
        );
        let cmd = ImportDxf::from_bytes(&world, None, "plate.dxf", &dxf("40"), &DxfOptions::default()).expect("dxf");
        hist.execute(&world, cmd);
        let (_, sk) = camcraft_app::sketch::find(&world, "plate").expect("sketch");
        let cut = sk.data.chains().by_layer("cut").first().map(|c| c.id()).expect("cut chain");
        let chain = FeatureSel::SketchChain(cut);
        let layer = FeatureSel::SketchLayer("holes".into());
        let mm = Unit::Millimeters;

        // Unchanged input: one build, then cache hits that equal the fresh read.
        let b0 = sketch_chip_builds();
        let first = sketch_chip(&world, "plate", &chain, mm);
        for _ in 0..4 {
            assert_eq!(sketch_chip(&world, "plate", &chain, mm), first);
        }
        assert_eq!(first, sketch_chip_fresh(&world, "plate", &chain, mm));
        assert!(first.as_ref().is_some_and(|(t, m)| t.starts_with("closed · 140") && !m), "{first:?}");
        assert_eq!(sketch_chip_builds() - b0, 1, "a still read builds nothing");

        // Changed display unit.
        let b1 = sketch_chip_builds();
        let inch = sketch_chip(&world, "plate", &chain, Unit::Inches);
        assert_eq!(inch, sketch_chip_fresh(&world, "plate", &chain, Unit::Inches));
        assert_ne!(inch, first);
        assert_eq!(sketch_chip_builds() - b1, 1);

        // Changed pick.
        let b2 = sketch_chip_builds();
        let holes = sketch_chip(&world, "plate", &layer, mm);
        assert_eq!(holes, Some(("1 closed, 0 open".to_owned(), false)));
        assert_eq!(sketch_chip_builds() - b2, 1);

        // Replaced drawing under the same name: a new allocation, a new read.
        hist.execute(&world, DeleteEntity::new(&world, "plate").expect("delete"));
        let cmd = ImportDxf::from_bytes(&world, None, "plate.dxf", &dxf("60"), &DxfOptions::default()).expect("dxf2");
        hist.execute(&world, cmd);
        let b3 = sketch_chip(&world, "plate", &chain, mm);
        assert_eq!(b3, sketch_chip_fresh(&world, "plate", &chain, mm));
        assert_ne!(b3, first, "the new drawing is read, not the cached text");

        // Not a sketch pick: no chip from this cache.
        assert_eq!(sketch_chip(&world, "plate", &FeatureSel::Face(1), mm), None);
    }
}

//! Input-keyed memos for DXF sketch chain descriptors and Z spans (#1864 /
//! #2238): avoids re-scanning sketch chains and re-sampling curves at 0.1 mm on
//! unchanged frames. Provenance: our own exact-input memo
//! (`docs/decisions/2238-frame-cost-rule.md`); no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use flecs_ecs::prelude::{Entity, World};

use crate::geometry::sketch_data::SketchData;
use crate::schema::Location;

use super::contour_sketch_pick::{SketchChainInfo, sketch_chain_infos_fresh, sketch_z_range_fresh};
use super::SrcRef;

const CAP: usize = 8;

/// A sketch drawing allocation, compared by address. The held `Weak` keeps
/// the address from being reused while the entry lives.
struct SketchRef(Weak<SketchData>);

impl PartialEq for SketchRef {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl SketchRef {
    fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

#[derive(PartialEq)]
struct ProfileMeta {
    name: String,
    entity: Option<Entity>,
    sketch: Option<SketchRef>,
}

#[derive(PartialEq)]
struct InfoKey {
    src: SrcRef,
    profiles: Vec<ProfileMeta>,
}

#[derive(PartialEq)]
struct ProfilePose {
    name: String,
    entity: Option<Entity>,
    sketch: Option<SketchRef>,
    pose: Option<Location>,
}

#[derive(PartialEq)]
struct ZKey {
    profiles: Vec<ProfilePose>,
}

type ZSpan = Result<(f64, f64), &'static str>;

thread_local! {
    static INFO_MEMO: RefCell<Vec<(InfoKey, Vec<SketchChainInfo>)>> = const { RefCell::new(Vec::new()) };
    static INFO_BUILDS: Cell<u64> = const { Cell::new(0) };
    static Z_MEMO: RefCell<Vec<(ZKey, ZSpan)>> = const { RefCell::new(Vec::new()) };
    static Z_BUILDS: Cell<u64> = const { Cell::new(0) };
    static DISPLAY_DEPTH: Cell<u32> = const { Cell::new(0) };
    static Z_FRESH_READS: Cell<u64> = const { Cell::new(0) };
}

/// Leaves the display scope when dropped, also on unwind.
struct DisplayScope;

impl Drop for DisplayScope {
    fn drop(&mut self) {
        DISPLAY_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Runs `f` as a display read: inside it, a selection height on a DXF
/// sketch may read the frame memo. Outside it (generation, verify, the
/// post gate, the command doors) the sketch Z span is always sampled
/// fresh - the post gate never reads a display cache (#2238).
pub fn display_heights<R>(f: impl FnOnce() -> R) -> R {
    DISPLAY_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
    let _scope = DisplayScope;
    f()
}

/// The sketch Z span for a height read: the frame memo inside
/// [`display_heights`], a fresh sample everywhere else.
pub(crate) fn sketch_z_for_heights(world: &World, src: &SrcRef) -> Option<ZSpan> {
    if DISPLAY_DEPTH.with(Cell::get) > 0 {
        return sketch_z_range(world, src);
    }
    let span = sketch_z_range_fresh(world, src)?;
    Z_FRESH_READS.with(|n| n.set(n.get().saturating_add(1)));
    Some(span)
}

/// How many authoritative (fresh, not memoized) sketch Z samplings ran on
/// this thread through [`sketch_z_for_heights`].
#[must_use]
pub fn sketch_z_fresh_reads() -> u64 {
    Z_FRESH_READS.with(Cell::get)
}

fn mru_get<K: PartialEq, V: Clone>(memo: &mut Vec<(K, V)>, key: &K) -> Option<V> {
    let i = memo.iter().position(|(k, _)| k == key)?;
    let entry = memo.remove(i);
    let val = entry.1.clone();
    memo.push(entry);
    Some(val)
}

fn mru_put<K, V>(memo: &mut Vec<(K, V)>, key: K, val: V) {
    if memo.len() >= CAP {
        memo.remove(0);
    }
    memo.push((key, val));
}

/// UI metadata list for every cuttable DXF sketch chain named by `src`,
/// memoized by `src` and each profile's live `Arc<SketchData>` allocation.
#[must_use]
pub fn sketch_chain_infos(world: &World, src: &SrcRef) -> Vec<SketchChainInfo> {
    let profiles = crate::sketch_model::source_profiles(world, src);
    if profiles.is_empty() {
        return Vec::new();
    }
    let key = InfoKey {
        src: src.clone(),
        profiles: profiles
            .into_iter()
            .map(|name| {
                let found = crate::sketch::find(world, &name);
                ProfileMeta {
                    name,
                    entity: found.as_ref().map(|(e, _)| *e),
                    sketch: found.map(|(_, s)| SketchRef(Arc::downgrade(&s.data))),
                }
            })
            .collect(),
    };
    let hit = INFO_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|(k, _)| {
            k.profiles
                .iter()
                .all(|p| p.sketch.as_ref().is_none_or(SketchRef::alive))
        });
        mru_get(&mut m, &key)
    });
    if let Some(infos) = hit {
        return infos;
    }
    INFO_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let infos = sketch_chain_infos_fresh(world, src);
    INFO_MEMO.with(|m| mru_put(&mut m.borrow_mut(), key, infos.clone()));
    infos
}

/// How many fresh [`sketch_chain_infos`] scans ran on this thread.
#[must_use]
pub fn sketch_chain_info_builds() -> u64 {
    INFO_BUILDS.with(Cell::get)
}

/// `(min_z, max_z)` across every selected DXF sketch entity in world space,
/// memoized by each profile's live `Arc<SketchData>` allocation and world pose.
pub(crate) fn sketch_z_range(world: &World, src: &SrcRef) -> Option<ZSpan> {
    let profiles = crate::sketch_model::source_profiles(world, src);
    if profiles.is_empty() {
        return None;
    }
    let key = ZKey {
        profiles: profiles
            .into_iter()
            .map(|name| {
                let found = crate::sketch::find(world, &name);
                let entity = found.as_ref().map(|(e, _)| *e);
                let sketch = found.map(|(_, s)| SketchRef(Arc::downgrade(&s.data)));
                let pose = entity.as_ref().and_then(|e| crate::sketch::world_pose(world, e));
                ProfilePose {
                    name,
                    entity,
                    sketch,
                    pose,
                }
            })
            .collect(),
    };
    let hit = Z_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|(k, _)| {
            k.profiles
                .iter()
                .all(|p| p.sketch.as_ref().is_none_or(SketchRef::alive))
        });
        mru_get(&mut m, &key)
    });
    if let Some(span) = hit {
        return Some(span);
    }
    Z_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let span = sketch_z_range_fresh(world, src)?;
    Z_MEMO.with(|m| mru_put(&mut m.borrow_mut(), key, span));
    Some(span)
}

/// How many fresh [`sketch_z_range`] samplings ran on this thread.
#[must_use]
pub fn sketch_z_builds() -> u64 {
    Z_BUILDS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DxfOptions, History, ImportDxf, MoveSketch, SetStock};
    use crate::operation::{FeatureSel, GeoSel};
    use crate::schema::Location;
    use crate::stock::StockShape;
    use crate::world::new_world;

    fn dxf_one() -> &'static [u8] {
        b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
          0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n40\n20\n0\n10\n40\n20\n30\n10\n0\n20\n30\n\
          0\nCIRCLE\n8\nholes\n10\n20\n20\n15\n40\n4\n\
          0\nENDSEC\n0\nEOF\n"
    }

    fn dxf_two() -> &'static [u8] {
        b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
          0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n60\n20\n0\n10\n60\n20\n30\n10\n0\n20\n30\n\
          0\nENDSEC\n0\nEOF\n"
    }

    #[test]
    fn sketch_chain_infos_and_z_range_memoize_per_input_and_track_changes() {
        let world = new_world();
        let mut hist = History::default();
        SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Location::default())
            .expect("stock")
            .apply(&world);
        let cmd = ImportDxf::from_bytes(&world, None, "plate.dxf", dxf_one(), &DxfOptions::default()).expect("dxf");
        hist.execute(&world, cmd);

        let whole = SrcRef::Entity("plate".into());
        let layer = SrcRef::Selection(vec![GeoSel {
            entity: "plate".into(),
            feature: FeatureSel::SketchLayer("holes".into()),
        }]);

        let i0 = sketch_chain_info_builds();
        let infos_a = sketch_chain_infos(&world, &whole);
        let infos_b = sketch_chain_infos(&world, &whole);
        assert_eq!(infos_a, sketch_chain_infos_fresh(&world, &whole));
        assert_eq!(infos_a, infos_b);
        assert_eq!(sketch_chain_info_builds() - i0, 1, "still read hits info memo");

        let z0 = sketch_z_builds();
        let za = sketch_z_range(&world, &whole);
        let zb = sketch_z_range(&world, &whole);
        assert_eq!(za, sketch_z_range_fresh(&world, &whole));
        assert_eq!(za, zb);
        assert_eq!(sketch_z_builds() - z0, 1, "still read hits Z memo");

        // Changed SrcRef recomputes chain infos; moving stock height recomputes Z span.
        let i1 = sketch_chain_info_builds();
        let layer_infos = sketch_chain_infos(&world, &layer);
        assert_eq!(layer_infos, sketch_chain_infos_fresh(&world, &layer));
        assert_eq!(layer_infos.len(), 1);
        assert_eq!(sketch_chain_info_builds() - i1, 1);

        let z1 = sketch_z_builds();
        hist.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 25.0 }, Location::default()).expect("stock"),
        );
        let z_tall = sketch_z_range(&world, &whole);
        assert_eq!(z_tall, sketch_z_range_fresh(&world, &whole));
        assert_ne!(z_tall, za);
        assert_eq!(sketch_z_builds() - z1, 1);

        // Moving sketch keeps chain infos cached and recomputes Z range.
        let i2 = sketch_chain_info_builds();
        assert_eq!(sketch_chain_infos(&world, &whole), infos_a);
        assert_eq!(sketch_chain_info_builds(), i2);

        hist.execute(&world, MoveSketch::new(&world, "plate", 10.0, 5.0, 45.0).expect("move sketch"));
        let z2 = sketch_z_builds();
        let z_moved = sketch_z_range(&world, &whole);
        assert_eq!(z_moved, sketch_z_range_fresh(&world, &whole));
        assert_eq!(sketch_z_builds() - z2, 1);

        // Replacing sketch data under the same name invalidates both caches.
        hist.execute(&world, crate::commands::DeleteEntity::new(&world, "plate").expect("delete"));
        let cmd2 = ImportDxf::from_bytes(&world, None, "plate.dxf", dxf_two(), &DxfOptions::default()).expect("dxf2");
        hist.execute(&world, cmd2);
        let i3 = sketch_chain_info_builds();
        let z3 = sketch_z_builds();
        assert_eq!(sketch_chain_infos(&world, &whole), sketch_chain_infos_fresh(&world, &whole));
        assert_eq!(sketch_z_range(&world, &whole), sketch_z_range_fresh(&world, &whole));
        assert_eq!(sketch_chain_info_builds() - i3, 1);
        assert_eq!(sketch_z_builds() - z3, 1);

        // Undo stock move and replacement restores answers matching fresh.
        hist.undo(&world);
        hist.undo(&world);
        assert_eq!(sketch_chain_infos(&world, &whole), sketch_chain_infos_fresh(&world, &whole));
        assert_eq!(sketch_z_range(&world, &whole), sketch_z_range_fresh(&world, &whole));
    }

    /// #2716 review: a selection height on a DXF sketch resolves fresh on
    /// the authoritative path (generation, the post gate's
    /// `resolve_params`). Only a display scope reads the frame memo.
    #[test]
    fn selection_heights_on_a_sketch_read_fresh_outside_the_display_scope() {
        use crate::operation::{Height, HeightReference};
        let world = new_world();
        let mut hist = History::default();
        SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Location::default())
            .expect("stock")
            .apply(&world);
        let cmd = ImportDxf::from_bytes(&world, None, "plate.dxf", dxf_one(), &DxfOptions::default()).expect("dxf");
        hist.execute(&world, cmd);
        let whole = SrcRef::Entity("plate".into());
        let top = Height::of(HeightReference::TopOfSelection, 0.0);
        let expect = sketch_z_range_fresh(&world, &whole).expect("sketch").expect("span").1;

        let (z0, f0) = (sketch_z_builds(), sketch_z_fresh_reads());
        for _ in 0..3 {
            let z = crate::operation::resolve_height(&world, None, Some(&whole), top);
            assert_eq!(z, Some(expect));
        }
        assert_eq!(sketch_z_builds() - z0, 0, "the authoritative path reads no frame memo");
        assert_eq!(sketch_z_fresh_reads() - f0, 3, "each authoritative read samples fresh");

        let (z1, f1) = (sketch_z_builds(), sketch_z_fresh_reads());
        display_heights(|| {
            for _ in 0..3 {
                let z = crate::operation::resolve_height(&world, None, Some(&whole), top);
                assert_eq!(z, Some(expect));
            }
        });
        assert!(sketch_z_builds() - z1 <= 1, "the display scope reads the frame memo");
        assert_eq!(sketch_z_fresh_reads() - f1, 0, "the display scope samples nothing fresh");

        // The scope ends with the closure: the next read is fresh again.
        let f2 = sketch_z_fresh_reads();
        let _ = crate::operation::resolve_height(&world, None, Some(&whole), top);
        assert_eq!(sketch_z_fresh_reads() - f2, 1);
    }
}

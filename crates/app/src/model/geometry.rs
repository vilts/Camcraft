//! The memoized-geometry seam of the model module (M5.3 A): feature
//! topology cached by content digest (M1.5), and the derived geometry
//! signature cached per entity against the revision counter that
//! [`super::set_facets`] moves with every landing (M3.1's counter doing
//! M5.3's work). Both caches are pure fuel - correctness is independent
//! of hits, and no verdict ever reads them for truth.

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::feature::{self, Features};
use crate::geometry::revision;
use crate::geometry::Triangle;
use crate::schema::{Facets, GeoSig};

use super::{facets_of, rev_of};

thread_local! {
    /// Cap-8 MRU of analyzed meshes, keyed by local-facet content digest.
    /// Faces resolve in the LOCAL frame, so transforms never invalidate an
    /// entry; correctness is independent of hits - this is pure fuel for
    /// per-frame hover (M1.5).
    static FEATURES_CACHE: std::cell::RefCell<Vec<(u64, std::sync::Arc<Features>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Feature topology (faces, loops, feature edges) of a model's LOCAL
/// facets, memoized by content. `None` only for a facet-less entity;
/// garbage facets still return complete (degenerate-tagged) features.
///
/// #2235: the per-frame callers (one or more per selected chain, every
/// frame) hit [`features_of_shared`]'s allocation-keyed memo first, so
/// a frame without a new facet landing hashes no facets.
#[must_use]
pub fn features_of(world: &World, entity: &Entity) -> Option<std::sync::Arc<Features>> {
    let tris = facets_of(world, entity)?;
    Some(features_of_shared(&tris))
}

/// How many live meshes [`features_of_shared`] remembers.
const SHARED_CAP: usize = 32;

/// One allocation-keyed entry: the mesh (weak) and its features.
type SharedEntry = (std::sync::Weak<[Triangle]>, std::sync::Arc<Features>);

thread_local! {
    /// Features keyed by the facet allocation itself (#2235), the law of
    /// `pick::feature_graph_shared`: an entity's `Facets` is an immutable
    /// `Arc<[Triangle]>` that every landing replaces, and the held `Weak`
    /// keeps the allocation's address from being reused, so a live entry
    /// at the same address IS the same bytes. Pure fuel: a miss falls
    /// back to the content-digest cache.
    static SHARED_FEATURES: std::cell::RefCell<Vec<SharedEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Content digests taken by [`features_of_local`] - the counter the
    /// #2235 pins read (a frame without a landing takes none).
    static DIGESTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// [`features_of_local`] memoized by the facet allocation: no content
/// hash on a hit. The answer is the same `Features` the digest path
/// gives for these bytes.
#[must_use]
pub fn features_of_shared(tris: &std::sync::Arc<[Triangle]>) -> std::sync::Arc<Features> {
    if tris.is_empty() {
        return std::sync::Arc::new(Features::default());
    }
    let key = std::sync::Arc::downgrade(tris);
    let hit = SHARED_FEATURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|(w, _)| w.strong_count() > 0);
        let i = cache.iter().position(|(w, _)| w.ptr_eq(&key))?;
        let entry = cache.remove(i);
        let f = entry.1.clone();
        cache.push(entry); // MRU bump
        Some(f)
    });
    if let Some(f) = hit {
        return f;
    }
    let f = features_of_local(tris);
    SHARED_FEATURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= SHARED_CAP {
            cache.remove(0);
        }
        cache.push((key, f.clone()));
    });
    f
}

/// How many content digests [`features_of_local`] took on this thread
/// (#2235 pins: a frame with no facet landing takes none).
#[must_use]
pub fn feature_digests() -> u64 {
    DIGESTS.with(std::cell::Cell::get)
}

/// The cache chokepoint, exposed for callers already holding the facets.
#[must_use]
pub fn features_of_local(tris: &[Triangle]) -> std::sync::Arc<Features> {
    if tris.is_empty() {
        return std::sync::Arc::new(Features::default());
    }
    DIGESTS.with(|n| n.set(n.get().saturating_add(1)));
    let key = feature::digest_of(tris);
    FEATURES_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(i);
            cache.push(entry.clone()); // MRU bump
            return entry.1;
        }
        let f = std::sync::Arc::new(feature::features_of_facets(tris));
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((key, f.clone()));
        f
    })
}

/// The local-frame geometry signature of an entity's mounted facets
/// (M3.1 A's digest), memoized per entity: a hit needs the live
/// revision to equal the entry's, and `set_facets` moves the counter
/// with EVERY landing, so content cannot change while the key stands.
/// Restore commands pin the counter back together with the exact bytes
/// that revision carried (see [`super::set_rev`]), which is what lets
/// an undo/redo oscillation replay hits instead of re-hashing.
///
/// The answer is identical to `revision::signature(&features_of_local(
/// facets_of(..)?))` computed fresh on every call - the whole point of
/// the memo is that per-frame staleness checks (the stamp, one digest
/// per visible operation per frame) stop cloning and hashing unmoved
/// geometry. `None` for a facet-less entity, exactly as `facets_of(..)?`
/// read at the stamp's call sites before the memo existed.
#[must_use]
pub fn geometry_signature(world: &World, entity: &Entity) -> Option<u64> {
    let view = entity.entity_view(world);
    // Presence first: the cached digest is only offered for geometry
    // that is still mounted - a component REMOVAL (an invert with no
    // facets to restore) must read `None` like the fresh path does,
    // whatever revision number the pin left behind.
    view.try_get::<&Facets>(|_| ())?;
    let rev = rev_of(world, entity);
    if let Some(hit) = view
        .try_get::<&GeoSig>(|g| (g.rev == rev).then_some(g.sig))
        .flatten()
    {
        return Some(hit);
    }
    let tris = facets_of(world, entity)?;
    let sig = revision::signature(&features_of_local(&tris));
    view.set(GeoSig { rev, sig });
    Some(sig)
}

#[cfg(test)]
mod tests {
    use flecs_ecs::core::EntityViewGet;

    use super::{geometry_signature, rev_of};
    use crate::geometry::{Point3, Triangle, cube};
    use crate::model::{self, set_rev};
    use crate::schema::{Facets, GeoSig};
    use crate::world::new_world;

    fn box_at(z_max: f64) -> std::sync::Arc<[Triangle]> {
        cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, z_max)).facets
    }

    #[test]
    fn every_landing_moves_the_counter() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        assert_eq!(rev_of(&world, &e), 0, "spawned models carry no counter");
        model::set_facets(&world, &e, box_at(10.0));
        assert_eq!(rev_of(&world, &e), 1, "the first landing is revision 1");
        model::set_facets(&world, &e, box_at(10.0));
        assert_eq!(rev_of(&world, &e), 2, "even an identical re-land moves it");
        set_rev(&world, &e, 7);
        model::set_facets(&world, &e, box_at(5.0));
        assert_eq!(rev_of(&world, &e), 8, "the landing counts from the pinned value");
    }

    #[test]
    fn the_memo_answers_what_the_fresh_path_answers() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        model::set_facets(&world, &e, box_at(10.0));
        let a = geometry_signature(&world, &e);
        assert_eq!(
            a,
            Some(super::revision::signature(&super::features_of_local(
                &box_at(10.0)
            ))),
            "the memo says what the fresh path says"
        );
        model::set_facets(&world, &e, box_at(5.0));
        let b = geometry_signature(&world, &e);
        assert!(b.is_some() && a != b, "moved material moves the digest");
        assert_eq!(
            b,
            Some(super::revision::signature(&super::features_of_local(
                &box_at(5.0)
            ))),
            "and still says what the fresh path says"
        );
    }

    #[test]
    fn undo_redo_oscillation_restores_exact_digests() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        model::set_facets(&world, &e, box_at(10.0));
        let a = geometry_signature(&world, &e);
        // The command's own accounting (UpdateModel): new mesh, revision 2.
        model::set_facets(&world, &e, box_at(5.0));
        set_rev(&world, &e, 2);
        let b = geometry_signature(&world, &e);
        assert_ne!(a, b);
        // Undo: the previous bytes re-land (auto-bump), then the pin puts
        // the counter back where the memo first saw them.
        model::set_facets(&world, &e, box_at(10.0));
        set_rev(&world, &e, 1);
        assert_eq!(geometry_signature(&world, &e), a, "undo replays the original digest");
        // Redo: the command's own mesh again, own revision again.
        model::set_facets(&world, &e, box_at(5.0));
        set_rev(&world, &e, 2);
        assert_eq!(geometry_signature(&world, &e), b, "redo replays the update's digest");
    }

    #[test]
    fn placement_never_moves_the_signature() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        model::set_facets(&world, &e, box_at(10.0));
        let a = geometry_signature(&world, &e);
        model::set_location(&world, &e, Point3::new(100.0, 0.0, 25.0));
        assert_eq!(geometry_signature(&world, &e), a, "local-frame content, placement elsewhere");
    }

    #[test]
    fn a_hit_still_asks_whether_geometry_is_mounted() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        assert_eq!(geometry_signature(&world, &e), None, "no facets, no signature");
        model::set_facets(&world, &e, box_at(10.0));
        let a = geometry_signature(&world, &e);
        assert!(a.is_some());
        // The law says pins land with their own bytes, but the memo must
        // not TRUST that even when a pin repeats a revision on foreign
        // (here: removed) geometry - the presence gate answers None no
        // matter what the counter says.
        e.entity_view(&world).remove(world.component::<Facets>());
        set_rev(&world, &e, 1);
        assert_eq!(
            geometry_signature(&world, &e),
            None,
            "a reused key cannot resurrect a digest for unmounted facets"
        );
    }

    #[test]
    fn the_entry_pairs_the_digest_with_the_counter_it_describes() {
        let world = new_world();
        let e = model::spawn(&world, "m");
        model::set_facets(&world, &e, box_at(10.0));
        let a = geometry_signature(&world, &e);
        let entry = e
            .entity_view(&world)
            .try_get::<&GeoSig>(|g| *g)
            .unwrap_or(GeoSig { rev: u64::MAX, sig: 0 });
        assert_eq!(entry.rev, rev_of(&world, &e), "the entry names a live revision");
        assert_eq!(entry.sig, a.unwrap_or_default());
        model::set_facets(&world, &e, box_at(5.0));
        let _ = geometry_signature(&world, &e);
        assert_eq!(
            e.entity_view(&world)
                .try_get::<&GeoSig>(|g| g.rev)
                .unwrap_or(u64::MAX),
            rev_of(&world, &e),
            "the memo moves to the new revision, not beside it"
        );
    }
}

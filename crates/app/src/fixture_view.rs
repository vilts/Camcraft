//! What the fixture rows say about each fixture's collision truth (#339
//! slice 4): judged by its closed mesh, or by its bounding box and why.
//! The answer is the mesh law's own ([`FixtureMesh::build`], the very
//! validation `camcraft_sim::verify::mesh_steel` runs), so the row can
//! never claim a mesh the gate would refuse. Building it indexes the
//! mesh, so the browser reads it through [`VersionCache`] - once per
//! document version, never per frame.
//!
//! Provenance: our own view over #339 slice 1's mesh law; no vendor
//! material consulted.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use camcraft_sim::fixture_mesh::MeshError;
use camcraft_sim::fixture_mesh::FixtureMesh;
use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::fixture::{self, FixtureBox};
use crate::{model, setup};

/// Why a fixture is judged by its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxReason {
    /// The mesh law refused the facets, by name.
    Refused(MeshError),
    /// Another fixture of the same setup has the identical world box:
    /// a box hit cannot be pinned on either mesh, so both keep the box.
    SharedBox,
    /// The visual profile mesh approximates curved steel with chords.
    ProfileChord,
}

/// A fixture's collision truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshState {
    /// A closed mesh: the gate judges moves against the real steel.
    Mesh,
    /// The bounding box stands (conservative: a path through its gap
    /// still reports).
    BoxOnly(BoxReason),
}

impl BoxReason {
    /// The few words a browser row has room for.
    #[must_use]
    pub const fn short(self) -> &'static str {
        match self {
            Self::Refused(MeshError::Empty) => "no facets",
            Self::Refused(MeshError::NonFinite) => "non-finite",
            Self::Refused(MeshError::Degenerate) => "collapsed",
            Self::Refused(MeshError::TooLarge) => "over the facet cap",
            Self::Refused(MeshError::Open) => "open mesh",
            Self::SharedBox => "shared box",
            Self::ProfileChord => "curved profile",
        }
    }
}

impl MeshState {
    /// The row suffix: `mesh`, or `box only: <reason>`.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Mesh => "mesh".to_owned(),
            Self::BoxOnly(why) => format!("box only: {}", why.short()),
        }
    }

    /// The Fixtures page's sentence.
    #[must_use]
    pub fn detail(self) -> String {
        match self {
            Self::Mesh => "judged by its closed mesh - a path through its gaps is clear".to_owned(),
            Self::BoxOnly(BoxReason::Refused(e)) => format!("judged by its bounding box: {e}"),
            Self::BoxOnly(BoxReason::ProfileChord) => {
                "judged by its bounding box: the profile mesh approximates curved surfaces".to_owned()
            }
            Self::BoxOnly(BoxReason::SharedBox) => {
                "judged by its bounding box: another fixture of this setup has the identical box".to_owned()
            }
        }
    }
}

/// The mesh state of one world mesh standing beside `others` (the
/// world boxes of the rest of its setup's fixtures). The mesh's own
/// refusal outranks a shared box: it is the one the user can fix.
#[must_use]
pub fn judge(tris: &[crate::geometry::Triangle], aabb: Option<FixtureBox>, others: &[FixtureBox]) -> MeshState {
    if let Err(e) = FixtureMesh::build(tris) {
        return MeshState::BoxOnly(BoxReason::Refused(e));
    }
    if aabb.is_some_and(|b| others.contains(&b)) {
        return MeshState::BoxOnly(BoxReason::SharedBox);
    }
    MeshState::Mesh
}

/// Every fixture of setup context `declared`, name order, with its
/// mesh state. A tagged model with no facets has no box and no mesh.
#[must_use]
pub fn mesh_states(world: &World, declared: Option<&str>) -> Vec<(String, MeshState)> {
    let rows: Vec<(String, Vec<crate::geometry::Triangle>, Option<FixtureBox>, bool)> = fixture::tagged(world, declared)
        .into_iter()
        .map(|(name, e)| {
            let tris = model::world_facets(world, &e).unwrap_or_default();
            let aabb = model::entity_world_aabb(world, &e).map(|b| FixtureBox { min: b.min, max: b.max });
            let curved = fixture::profile_collision_chord_error(world, &e) > 0.0;
            (name, tris, aabb, curved)
        })
        .collect();
    rows.iter()
        .enumerate()
        .map(|(i, (name, tris, aabb, curved))| {
            let others: Vec<FixtureBox> =
                rows.iter().enumerate().filter(|(j, _)| *j != i).filter_map(|(_, r)| r.2).collect();
            let state = if *curved { MeshState::BoxOnly(BoxReason::ProfileChord) } else { judge(tris, *aabb, &others) };
            (name.clone(), state)
        })
        .collect()
}

/// [`mesh_states`] over every setup context (the implicit one when the
/// document has none), keyed by fixture name - a fixture belongs to
/// exactly one context, so its name is the key.
#[must_use]
pub fn all_mesh_states(world: &World) -> HashMap<String, MeshState> {
    let names = setup::names(world);
    let ctxs: Vec<Option<&str>> =
        if names.is_empty() { vec![None] } else { names.iter().map(|n| Some(n.as_str())).collect() };
    let mut out = HashMap::new();
    for ctx in ctxs {
        out.extend(mesh_states(world, ctx));
    }
    out
}

/// One cached answer per key (a document version, an op): the view
/// layer reads it every frame from `&self`, the work runs when the key
/// moves. Interior mutability is a `Mutex` (a poisoned lock is taken
/// back - the slot is a cache, never truth).
pub struct VersionCache<K, T> {
    slot: Mutex<Option<(K, Arc<T>)>>,
}

impl<K, T> Default for VersionCache<K, T> {
    fn default() -> Self {
        Self { slot: Mutex::new(None) }
    }
}

impl<K: PartialEq, T> VersionCache<K, T> {
    /// The cached answer for `key`, or `make()`'s, kept for next time.
    pub fn get_or(&self, key: K, make: impl FnOnce() -> T) -> Arc<T> {
        let mut slot = match self.slot.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some((k, v)) = slot.as_ref()
            && *k == key
        {
            return Arc::clone(v);
        }
        let v = Arc::new(make());
        *slot = Some((key, Arc::clone(&v)));
        v
    }
}

impl<K, T> std::fmt::Debug for VersionCache<K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VersionCache")
    }
}

/// The browser's cache of [`all_mesh_states`], per document version.
pub type MeshStateCache = VersionCache<u64, HashMap<String, MeshState>>;

/// [`all_mesh_states`] through `cache`, recomputed when `version` moves.
pub fn mesh_states_cached(cache: &MeshStateCache, world: &World, version: u64) -> Arc<HashMap<String, MeshState>> {
    cache.get_or(version, || all_mesh_states(world))
}

/// The op key a struck-fixture view is cached under: document version,
/// op name, and the cached path's signature (a regeneration landing
/// without a version bump still re-resolves).
pub type StruckKey = (u64, String, u64);

/// The viewport's cache of [`crate::fixture_struck::struck_by`].
pub type StruckCache = VersionCache<StruckKey, Option<crate::fixture_struck::Struck>>;

/// [`crate::fixture_struck::struck_by`] for `op_name` through `cache`.
pub fn struck_cached(
    cache: &StruckCache,
    world: &World,
    version: u64,
    op_name: &str,
) -> Arc<Option<crate::fixture_struck::Struck>> {
    // The signature alone: a per-frame read must not clone the path.
    let sig = model::find_by_name(world, op_name)
        .and_then(|e| e.entity_view(world).try_get::<&crate::schema::Toolpath>(|tp| tp.path_sig()))
        .unwrap_or(0);
    cache.get_or((version, op_name.to_owned(), sig), || crate::fixture_struck::struck_by(world, op_name))
}

//! Cached world-space highlight polylines for selected picks, viewport hover,
//! and Geometry-row hover. Resolves `GeoSel` picks once when an input changes
//! instead of re-walking features per frame (#2201).
//!
//! The key is the input itself (crates/ui/AGENTS.md, "Frame cost"): the
//! picks by value, and for each picked model its display-off flag, its
//! entity, the allocation of its `Arc<Features>` (a landing replaces it) and
//! the pose the viewport draws it in - the same reads as
//! `pick_line_memo`'s key. No document counter is part of it.
//!
//! Provenance: our own selection highlight cache built over `geometry::feature`.

use std::sync::{Arc, Mutex, Weak};

use camcraft_app::geometry::feature::Features;
use camcraft_app::geometry::{Point3, feature, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::schema::Location;
use flecs_ecs::prelude::Entity;

use crate::state::State;

/// One resolved feature outline in world space.
#[derive(Clone, Debug)]
pub(crate) struct WorldPolyline {
    pub pts: Vec<[f64; 3]>,
    pub closed: bool,
}

/// Cached resolved geometry for a set of picks or one hovered feature.
#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedHighlights {
    pub id: u64,
    pub polylines: Vec<WorldPolyline>,
    pub vertices: Vec<[f64; 3]>,
}

/// A feature set, compared by allocation. The held `Weak` keeps the
/// address from being reused while the entry lives.
struct Feats(Weak<Features>);

impl PartialEq for Feats {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

/// A sketch dataset, compared by allocation.
struct SketchPtr(Weak<camcraft_app::sketch_data::SketchData>);

impl PartialEq for SketchPtr {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

/// One picked model as the build reads it.
#[derive(PartialEq)]
struct Body {
    name: String,
    hidden: bool,
    entity: Option<Entity>,
    features: Option<Feats>,
    sketch: Option<SketchPtr>,
    pose: Option<Location>,
}

impl Body {
    fn of(state: &State, name: &str) -> Self {
        let entity = model::find_by_name(&state.world, name);
        let sketch = camcraft_app::sketch::find(&state.world, name)
            .map(|(_, s)| SketchPtr(Arc::downgrade(&s.data)));
        let sketch_pose = entity.and_then(|e| camcraft_app::sketch::world_pose(&state.world, &e));
        Self {
            name: name.to_owned(),
            hidden: state.hidden_in_view(name),
            entity,
            features: entity
                .and_then(|e| model::features_of(&state.world, &e))
                .map(|f| Feats(Arc::downgrade(&f))),
            sketch,
            // A sketch's build reads `sketch::world_pose` (it rides the
            // stock top), not its local `view_pose`: key on what the build
            // reads, so a stock move or height change rebuilds (#1864 review).
            pose: sketch_pose.or_else(|| entity.and_then(|e| camcraft_app::setup_align::view_pose(&state.world, &e))),
        }
    }
}

/// Every read of a build, taken without building.
#[derive(PartialEq)]
struct Key {
    picks: Vec<GeoSel>,
    bodies: Vec<Body>,
}

impl Key {
    fn of(state: &State, picks: Vec<GeoSel>) -> Self {
        let mut names: Vec<&str> = Vec::new();
        for g in &picks {
            if !names.contains(&g.entity.as_str()) {
                names.push(&g.entity);
            }
        }
        let bodies = names.into_iter().map(|n| Body::of(state, n)).collect();
        Self { picks, bodies }
    }
}

#[derive(Default)]
pub(crate) struct HighlightInner {
    pub(crate) next_id: u64,
    pub(crate) selection_builds: u64,
    pub(crate) hover_builds: u64,
    pub(crate) row_hover_builds: u64,
    selection: Option<(Key, Arc<ResolvedHighlights>)>,
    hover: Option<(Key, Arc<ResolvedHighlights>)>,
    row_hover: Option<(Key, Arc<ResolvedHighlights>)>,
    pub(crate) selected_mesh: crate::pick_highlight_mesh::MeshSlot,
    pub(crate) hover_mesh: crate::pick_highlight_mesh::MeshSlot,
    pub(crate) row_hover_mesh: crate::pick_highlight_mesh::MeshSlot,
}

/// Per-`State` cache of resolved pick outlines and batched screen meshes.
#[derive(Default)]
pub(crate) struct HighlightCache {
    pub(crate) inner: Mutex<HighlightInner>,
}

pub(crate) fn lock(state: &State) -> std::sync::MutexGuard<'_, HighlightInner> {
    state
        .pick_highlight
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Number of times selected-feature world polylines were rebuilt for this state.
#[cfg(test)]
#[must_use]
pub(crate) fn selection_builds(state: &State) -> u64 {
    lock(state).selection_builds
}

/// Number of times the viewport hover outline was rebuilt for this state.
#[cfg(test)]
#[must_use]
pub(crate) fn hover_builds(state: &State) -> u64 {
    lock(state).hover_builds
}

/// Number of times the panel row-hover outline was rebuilt for this state.
#[cfg(test)]
#[must_use]
pub(crate) fn row_hover_builds(state: &State) -> u64 {
    lock(state).row_hover_builds
}

/// Resolve world-space polylines for a `FeatureSel::SketchChain` or `FeatureSel::SketchLayer` pick.
pub(crate) fn sketch_polylines(
    world: &flecs_ecs::prelude::World,
    entity_name: &str,
    sel: &FeatureSel,
) -> Vec<WorldPolyline> {
    let Some((entity, sketch)) = camcraft_app::sketch::find(world, entity_name) else {
        return Vec::new();
    };
    let Some(pose) = camcraft_app::sketch::world_pose(world, &entity) else {
        return Vec::new();
    };
    let chains = match sel {
        FeatureSel::SketchChain(cid) => sketch.data.chains().find(*cid).into_iter().collect::<Vec<_>>(),
        FeatureSel::SketchLayer(layer) => sketch.data.chains().by_layer(layer),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for ch in chains {
        let pts = ch
            .sample(0.05)
            .into_iter()
            .map(|p| {
                let w = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
                [w.x, w.y, w.z]
            })
            .collect();
        out.push(WorldPolyline { pts, closed: ch.is_closed() });
    }
    out
}

fn append_feature(
    state: &State,
    item: &GeoSel,
    polylines: &mut Vec<WorldPolyline>,
    vertices: &mut Vec<[f64; 3]>,
) {
    if state.hidden_in_view(&item.entity) {
        return;
    }
    if matches!(item.feature, FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_)) {
        polylines.extend(sketch_polylines(&state.world, &item.entity, &item.feature));
        return;
    }
    let Some(entity) = model::find_by_name(&state.world, &item.entity) else {
        return;
    };
    let Some(features) = model::features_of(&state.world, &entity) else {
        return;
    };
    let loc = camcraft_app::setup_align::view_pose(&state.world, &entity).unwrap_or_default();
    let world_pts = |pts: &[[f64; 3]]| -> Vec<[f64; 3]> {
        pts.iter()
            .map(|p| {
                let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                [w.x, w.y, w.z]
            })
            .collect()
    };
    match &item.feature {
        FeatureSel::Face(fid) => {
            let Some(face) = features.faces.iter().find(|f| f.id == *fid) else {
                return;
            };
            for l in &face.loops {
                let local: Vec<[f64; 3]> = l
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                polylines.push(WorldPolyline {
                    pts: world_pts(&local),
                    closed: !matches!(l.kind, feature::LoopKind::Open),
                });
            }
        }
        FeatureSel::Chain(cid) => {
            let Some(chain) = model::resolve_chain(&features, *cid) else {
                return;
            };
            let local = feature::chain_points(&features, &chain);
            polylines.push(WorldPolyline {
                pts: world_pts(&local),
                closed: chain.closed,
            });
        }
        FeatureSel::Patch(pid) => {
            let Some(rings) = model::patch_rings(&features, *pid) else {
                return;
            };
            for r in rings.iter() {
                let local: Vec<[f64; 3]> = r
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                polylines.push(WorldPolyline {
                    pts: world_pts(&local),
                    closed: r.closed,
                });
            }
        }
        FeatureSel::Vertex(vid) => {
            let Some(v) = feature::resolve_vertex(&features, *vid)
                .and_then(|vi| features.verts.get(vi as usize))
            else {
                return;
            };
            let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
            vertices.push([w.x, w.y, w.z]);
        }
        FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => {}
    }
}

fn next_resolved(
    inner: &mut HighlightInner,
    polylines: Vec<WorldPolyline>,
    vertices: Vec<[f64; 3]>,
) -> Arc<ResolvedHighlights> {
    inner.next_id = inner.next_id.wrapping_add(1);
    Arc::new(ResolvedHighlights {
        id: inner.next_id,
        polylines,
        vertices,
    })
}

/// The cached entry of `slot` when its key equals `key`, else a fresh
/// build stored under `key`. `count` is bumped for a build of real picks.
fn resolve_cached(
    state: &State,
    key: Key,
    slot: impl Fn(&mut HighlightInner) -> &mut Option<(Key, Arc<ResolvedHighlights>)>,
    count: impl Fn(&mut HighlightInner) -> &mut u64,
) -> Arc<ResolvedHighlights> {
    {
        let mut inner = lock(state);
        if let Some((k, cached)) = slot(&mut inner)
            && *k == key
        {
            return Arc::clone(cached);
        }
    }
    let mut polylines = Vec::new();
    let mut vertices = Vec::new();
    for item in &key.picks {
        append_feature(state, item, &mut polylines, &mut vertices);
    }
    let mut inner = lock(state);
    if !key.picks.is_empty() {
        let n = count(&mut inner);
        *n = n.saturating_add(1);
    }
    let resolved = next_resolved(&mut inner, polylines, vertices);
    *slot(&mut inner) = Some((key, Arc::clone(&resolved)));
    resolved
}

/// Cached resolved world outlines and points for the currently shown selection.
pub(crate) fn resolved_selected(state: &State) -> Arc<ResolvedHighlights> {
    let key = Key::of(state, crate::interact::shown_picks(state));
    resolve_cached(state, key, |i| &mut i.selection, |i| &mut i.selection_builds)
}

pub(crate) fn resolved_hover(state: &State) -> Arc<ResolvedHighlights> {
    let pick = state.hover.as_ref().and_then(|hit| {
        hit.feature.as_ref().map(|feature| GeoSel {
            entity: hit.entity.clone(),
            feature: feature.clone(),
        })
    });
    let key = Key::of(state, pick.into_iter().collect());
    resolve_cached(state, key, |i| &mut i.hover, |i| &mut i.hover_builds)
}

pub(crate) fn resolved_row_hover(state: &State) -> Arc<ResolvedHighlights> {
    let key = Key::of(state, state.geo_hover_drawn.clone().into_iter().collect());
    resolve_cached(state, key, |i| &mut i.row_hover, |i| &mut i.row_hover_builds)
}

/// Project cached world polylines (and optional vertex crosses) to viewport UV segments.
#[cfg(test)]
pub(crate) fn project_segments(
    resolved: &ResolvedHighlights,
    include_vertices: bool,
    mvp: &[[f32; 4]; 4],
    aspect: f64,
) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    for poly in &resolved.polylines {
        let n = poly.pts.len();
        if n < 2 {
            continue;
        }
        let uvs: Vec<Option<[f32; 2]>> = poly
            .pts
            .iter()
            .map(|&p| crate::interact::project_uv(mvp, p))
            .collect();
        let last = if poly.closed { n } else { n - 1 };
        for i in 0..last {
            if let (Some(a), Some(b)) = (uvs[i], uvs[(i + 1) % n])
                && a != b
            {
                out.push((a, b));
            }
        }
    }
    if include_vertices {
        for &w in &resolved.vertices {
            crate::vertex_pick::vertex_marker(mvp, w, aspect, &mut out);
        }
    }
    out
}

#[cfg(test)]
#[path = "pick_highlight_tests.rs"]
mod tests;

//! Display-only DXF sketch layer list cache (#1864 / #2238): avoids scanning
//! sketch layers and chains on every still frame. Provenance: our own
//! frame-cost memo over `geometry::sketch_data`; no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use camcraft_app::sketch_data::SketchData;
use flecs_ecs::prelude::{Entity, World};

const CAP: usize = 8;

/// One layer toggle button descriptor for a placed DXF sketch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SketchLayerButton {
    pub layer: String,
    pub label: String,
}

/// Non-empty layers of one placed DXF sketch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SketchLayersGroup {
    pub entity: String,
    pub layers: Vec<SketchLayerButton>,
}

struct SketchPtr(Weak<SketchData>);

impl PartialEq for SketchPtr {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl SketchPtr {
    fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

#[derive(PartialEq)]
struct SketchKey {
    name: String,
    entity: Option<Entity>,
    data: Option<SketchPtr>,
}

struct Entry {
    key: Vec<SketchKey>,
    groups: Arc<[SketchLayersGroup]>,
}

thread_local! {
    static LAYER_MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    static LAYER_BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// Uncached fresh scan of non-empty layers for `sketches` (for tests).
#[must_use]
pub(crate) fn sketch_layers_fresh(world: &World, sketches: &[String]) -> Vec<SketchLayersGroup> {
    let mut out = Vec::new();
    for name in sketches {
        let Some((_, sk)) = camcraft_app::sketch::find(world, name) else {
            continue;
        };
        let layers: Vec<SketchLayerButton> = sk
            .data
            .layers()
            .iter()
            .filter_map(|layer| {
                let count = sk.data.chains().by_layer(layer).len();
                (count > 0).then(|| SketchLayerButton {
                    layer: layer.clone(),
                    label: format!("{layer} ({count})"),
                })
            })
            .collect();
        if !layers.is_empty() {
            out.push(SketchLayersGroup {
                entity: name.clone(),
                layers,
            });
        }
    }
    out
}

/// Non-empty layers for `sketches`, memoized by each entity's live `Arc<SketchData>`.
#[must_use]
pub(crate) fn sketch_layers(world: &World, sketches: &[String]) -> Arc<[SketchLayersGroup]> {
    let key: Vec<SketchKey> = sketches
        .iter()
        .map(|name| {
            let found = camcraft_app::sketch::find(world, name);
            SketchKey {
                name: name.clone(),
                entity: found.as_ref().map(|(e, _)| *e),
                data: found.map(|(_, sk)| SketchPtr(Arc::downgrade(&sk.data))),
            }
        })
        .collect();
    let hit = LAYER_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|e| {
            e.key
                .iter()
                .all(|k| k.data.as_ref().is_none_or(SketchPtr::alive))
        });
        let i = m.iter().position(|e| e.key == key)?;
        let entry = m.remove(i);
        let groups = Arc::clone(&entry.groups);
        m.push(entry);
        Some(groups)
    });
    if let Some(groups) = hit {
        return groups;
    }
    LAYER_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let groups: Arc<[SketchLayersGroup]> = sketch_layers_fresh(world, sketches).into();
    LAYER_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.remove(0);
        }
        m.push(Entry {
            key,
            groups: Arc::clone(&groups),
        });
    });
    groups
}

/// How many fresh [`sketch_layers`] scans ran on this thread.
#[cfg(test)]
#[must_use]
pub(crate) fn sketch_layer_builds() -> u64 {
    LAYER_BUILDS.with(Cell::get)
}

//! The pick overlay lines memoized per input (#2250):
//! `interact::selection_feature_lines` outlined every shown pick again on
//! each scene collection - a model lookup, a pose and, for a welded face,
//! a regrouping of its patch rings. Our own design notes
//! (`docs/decisions/2238-frame-cost-rule.md`); no vendor material
//! consulted.
//!
//! The key is the input itself (rule 2): the pick list by value, the
//! pick ink, the vertex cross arm when a vertex is picked, and for each
//! picked model its display-off flag, its entity, the allocation of its
//! `Arc<Features>` (immutable; a landing replaces it) and the pose the
//! viewport draws it in. A hit is the line set a fresh build gives. The
//! lines are display only: no generator, check or post reads them.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use camcraft_app::geometry::feature::Features;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::schema::Location;
use flecs_ecs::prelude::Entity;

use crate::state::State;

/// How many inputs the memo remembers: the open operation's picks, a
/// setup draft and a few recent pick lists.
const CAP: usize = 8;

/// A feature set, compared by allocation. The held `Weak` keeps the
/// address from being reused while the entry lives.
struct Feats(Weak<Features>);

impl PartialEq for Feats {
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
    pose: Option<Location>,
}

/// Every read of the build, taken without building.
#[derive(PartialEq)]
struct Key {
    items: Vec<GeoSel>,
    ink: [f32; 3],
    arm: Option<f64>,
    bodies: Vec<Body>,
}

impl Key {
    fn of(state: &State, items: &[GeoSel], ink: [f32; 3], arm: impl FnOnce() -> f64) -> Self {
        let mut names: Vec<&str> = Vec::new();
        for g in items {
            if !names.contains(&g.entity.as_str()) {
                names.push(&g.entity);
            }
        }
        let bodies = names
            .into_iter()
            .map(|name| {
                let entity = model::find_by_name(&state.world, name);
                Body {
                    name: name.to_owned(),
                    hidden: state.hidden_in_view(name),
                    entity,
                    features: entity
                        .and_then(|e| model::features_of(&state.world, &e))
                        .map(|f| Feats(Arc::downgrade(&f))),
                    pose: entity.and_then(|e| camcraft_app::setup_align::view_pose(&state.world, &e)),
                }
            })
            .collect();
        let vertex = items.iter().any(|g| matches!(g.feature, FeatureSel::Vertex(_)));
        Self { items: items.to_vec(), ink, arm: vertex.then(arm), bodies }
    }
}

struct Entry {
    key: Key,
    lines: Arc<[[f32; 6]]>,
}

thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Real line builds on this thread - the counter the #2250 pins read.
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// The lines of `items`: `build` on a miss, the remembered set on a hit.
/// `arm` (the vertex cross size) is asked only when a vertex is picked.
pub(crate) fn lines(
    state: &State,
    items: &[GeoSel],
    ink: [f32; 3],
    arm: impl FnOnce() -> f64,
    build: impl FnOnce() -> Vec<[f32; 6]>,
) -> Arc<[[f32; 6]]> {
    let key = Key::of(state, items, ink, arm);
    let hit = MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let i = m.iter().position(|e| e.key == key)?;
        let entry = m.remove(i);
        let lines = Arc::clone(&entry.lines);
        m.push(entry); // MRU bump
        Some(lines)
    });
    if let Some(lines) = hit {
        return lines;
    }
    BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let lines: Arc<[[f32; 6]]> = build().into();
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.remove(0);
        }
        m.push(Entry { key, lines: Arc::clone(&lines) });
    });
    lines
}

/// How many real line builds [`lines`] made on this thread.
#[must_use]
pub fn pick_line_builds() -> u64 {
    BUILDS.with(Cell::get)
}

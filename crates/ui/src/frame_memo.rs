//! One scene collection, one region per operation (#2235): the region
//! outline, the region fill and the height-plane gate each ask
//! `operation::region_of` for the same selected operation while one
//! scene is collected. Inside [`scene_scope`] the state is borrowed
//! immutably, so the first answer is the answer for the whole scope;
//! the memo is dropped when the scope ends and is never read outside
//! one. Our own design notes; no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use camcraft_app::geo2d::Region;
use camcraft_app::operation;
use camcraft_app::schema::Operation;
use flecs_ecs::prelude::World;

thread_local! {
    /// `Some` while a scope is open: operation name -> its region.
    static SCOPE: RefCell<Option<HashMap<String, Option<Region>>>> = const { RefCell::new(None) };
    /// Real `region_of` calls made through [`region_of`] (#2235 pins).
    static CALLS: Cell<u64> = const { Cell::new(0) };
}

/// Run `f` with the per-scene region memo open. A nested call keeps
/// the outer memo.
pub fn scene_scope<R>(f: impl FnOnce() -> R) -> R {
    let opened = SCOPE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_some() {
            return false;
        }
        *s = Some(HashMap::new());
        true
    });
    // The guard closes the scope on every exit, an unwind included, so a
    // memo can never outlive the borrow it was filled under.
    struct Close(bool);
    impl Drop for Close {
        fn drop(&mut self) {
            if self.0 {
                SCOPE.with(|s| *s.borrow_mut() = None);
            }
        }
    }
    let _close = Close(opened);
    f()
}

/// [`operation::region_of`] for the operation named `name`, asked once
/// per open scope (and on every call outside one).
#[must_use]
pub fn region_of(world: &World, name: &str, op: &Operation) -> Option<Region> {
    let hit = SCOPE.with(|s| s.borrow().as_ref().and_then(|m| m.get(name).cloned()));
    if let Some(region) = hit {
        return region;
    }
    CALLS.with(|n| n.set(n.get().saturating_add(1)));
    let region = operation::region_of(world, op);
    SCOPE.with(|s| {
        if let Some(m) = s.borrow_mut().as_mut() {
            m.insert(name.to_owned(), region.clone());
        }
    });
    region
}

/// How many real region builds [`region_of`] made on this thread.
#[must_use]
pub fn region_builds() -> u64 {
    CALLS.with(Cell::get)
}

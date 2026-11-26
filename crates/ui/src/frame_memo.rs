//! The region of each shown operation, built when its input changes and
//! read from a cache on each other frame (#2235, #2201). The region
//! outline, the region fill and the height-plane gate each ask
//! `operation::region_of` for the same selected operation. Inside one
//! [`scene_scope`] the first answer is the answer for the whole scope.
//! Across frames an answer is kept under its input (crates/ui/AGENTS.md
//! "Frame cost"): the operation value and its generation stamp
//! (`operation::op_stamp`), the digest of everything a generation of it
//! reads - tool, parameters, source pose and geometry signature, stock,
//! setup, area controls and boundaries. No document counter is part of
//! the key. Our own design notes; no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use camcraft_app::geo2d::Region;
use camcraft_app::operation::{self, GenStamp};
use camcraft_app::schema::Operation;
use flecs_ecs::prelude::World;

/// Kept answers past this many operations drop the oldest first.
const KEPT_CAP: usize = 16;

struct Kept {
    name: String,
    op: Operation,
    stamp: GenStamp,
    region: Option<Region>,
}

thread_local! {
    /// `Some` while a scope is open: operation name -> its region.
    static SCOPE: RefCell<Option<HashMap<String, Option<Region>>>> = const { RefCell::new(None) };
    /// The answers kept between frames, newest last.
    static KEPT: RefCell<Vec<Kept>> = const { RefCell::new(Vec::new()) };
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

/// [`operation::region_of`] for the operation named `name`: asked once
/// per open scope, and built again only when the operation or its stamp
/// moved since the kept answer. An operation whose stamp cannot resolve
/// (a source or tool gone) is never kept: it is asked again each scope.
#[must_use]
pub fn region_of(world: &World, name: &str, op: &Operation) -> Option<Region> {
    let hit = SCOPE.with(|s| s.borrow().as_ref().and_then(|m| m.get(name).cloned()));
    if let Some(region) = hit {
        return region;
    }
    let stamp = operation::op_stamp(world, op, name);
    let kept = stamp.and_then(|stamp| {
        KEPT.with(|k| {
            k.borrow()
                .iter()
                .find(|e| e.name == name && e.stamp == stamp && e.op == *op)
                .map(|e| e.region.clone())
        })
    });
    let region = match kept {
        Some(region) => region,
        None => {
            CALLS.with(|n| n.set(n.get().saturating_add(1)));
            let region = operation::region_of(world, op);
            if let Some(stamp) = stamp {
                keep(Kept { name: name.to_owned(), op: op.clone(), stamp, region: region.clone() });
            }
            region
        }
    };
    SCOPE.with(|s| {
        if let Some(m) = s.borrow_mut().as_mut() {
            m.insert(name.to_owned(), region.clone());
        }
    });
    region
}

/// Store one answer: it replaces the operation's older one, and the
/// oldest answer goes past [`KEPT_CAP`] (a bounded cache, never a leak).
fn keep(entry: Kept) {
    KEPT.with(|k| {
        let mut k = k.borrow_mut();
        k.retain(|e| e.name != entry.name);
        if k.len() >= KEPT_CAP {
            k.remove(0);
        }
        k.push(entry);
    });
}

/// How many real region builds [`region_of`] made on this thread.
#[must_use]
pub fn region_builds() -> u64 {
    CALLS.with(Cell::get)
}

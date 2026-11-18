//! Chain resolution memoized per feature set (#2235): `feature::
//! resolve_chain` walks the whole model's chains (and its runs and lone
//! edges) on every call, and the operation page, the overlay and the
//! selection account call it per picked chain on every frame. Our own
//! design notes; the `Weak` identity key is the law of
//! `pick::feature_graph_shared`. No vendor material consulted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use crate::geometry::feature::{self, Chain, ChainId, Features};

/// How many live feature sets the memo remembers.
const CAP: usize = 32;

/// One feature set (weak: it never keeps a model alive) and the answers
/// already given for it.
type Entry = (Weak<Features>, HashMap<ChainId, Option<Chain>>);

thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Real `feature::resolve_chain` walks on this thread - the counter
    /// the #2235 pins read.
    static WALKS: Cell<u64> = const { Cell::new(0) };
}

/// [`feature::resolve_chain`] for a shared feature set, memoized by the
/// set's allocation and the id. The held `Weak` keeps the address from
/// being reused, and `Features` behind an `Arc` never changes, so a hit
/// is the answer a fresh walk gives. Pure fuel: correctness is
/// independent of hits.
#[must_use]
pub fn resolve_chain(features: &Arc<Features>, id: ChainId) -> Option<Chain> {
    let key = Arc::downgrade(features);
    let hit = MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|(w, _)| w.strong_count() > 0);
        m.iter().find(|(w, _)| w.ptr_eq(&key)).and_then(|(_, map)| map.get(&id).cloned())
    });
    if let Some(answer) = hit {
        return answer;
    }
    WALKS.with(|n| n.set(n.get().saturating_add(1)));
    let answer = feature::resolve_chain(features, id);
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if let Some((_, map)) = m.iter_mut().find(|(w, _)| w.ptr_eq(&key)) {
            map.insert(id, answer.clone());
        } else {
            if m.len() >= CAP {
                m.remove(0);
            }
            m.push((key, HashMap::from([(id, answer.clone())])));
        }
    });
    answer
}

/// How many real chain walks [`resolve_chain`] made on this thread.
#[must_use]
pub fn chain_walks() -> u64 {
    WALKS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{chain_walks, resolve_chain};
    use crate::geometry::feature::{self, features_of_facets};
    use crate::geometry::{Point3, cube};

    #[test]
    fn a_hit_answers_what_a_fresh_walk_answers_and_walks_once() {
        let tris = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0)).facets;
        let f = Arc::new(features_of_facets(&tris));
        let ids: Vec<_> = feature::chains(&f).into_iter().map(|c| c.id).collect();
        assert!(!ids.is_empty(), "a box has edge chains");
        for &id in &ids {
            let fresh = feature::resolve_chain(&f, id).map(|c| c.edges);
            let before = chain_walks();
            assert_eq!(resolve_chain(&f, id).map(|c| c.edges), fresh);
            assert_eq!(resolve_chain(&f, id).map(|c| c.edges), fresh);
            assert_eq!(chain_walks() - before, 1, "the second ask is a hit");
        }
        let before = chain_walks();
        assert!(resolve_chain(&f, 12345).is_none());
        assert!(resolve_chain(&f, 12345).is_none(), "a miss is remembered too");
        assert_eq!(chain_walks() - before, 1);
        // A new feature set (a landing) is a new key: it walks again.
        let g = Arc::new(features_of_facets(&tris));
        let before = chain_walks();
        let _ = resolve_chain(&g, ids[0]);
        assert_eq!(chain_walks() - before, 1, "another allocation never reads this one's answers");
    }
}

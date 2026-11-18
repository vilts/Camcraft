//! Welded-face (patch) answers memoized per feature set (#2247):
//! `feature::patches` and `feature::patch_rings` group every face of the
//! model on each call, and the pick chips, the face-origin targets and
//! the 3-2-1 check ask them on every frame. Same key law as
//! `chain_memo` (#2235, `docs/decisions/2238-frame-cost-rule.md` rule 2:
//! the allocation of the `Arc<Features>` a landing replaces). Our own
//! design notes; no vendor material consulted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use crate::geometry::feature::{self, Features, Patch, PatchId, PatchRing};

/// How many live feature sets the memo remembers.
const CAP: usize = 32;

/// One feature set (weak: it never keeps a model alive), its patches and
/// the rings already asked for.
struct Entry {
    key: Weak<Features>,
    patches: Arc<[Patch]>,
    rings: HashMap<PatchId, Option<Arc<[PatchRing]>>>,
}

thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Real patch groupings on this thread - the counter the #2247 pins
    /// read.
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    BUILDS.with(|n| n.set(n.get().saturating_add(1)));
}

/// Run `f` on the entry of `features`, made (with its patches) on a miss.
fn with_entry<R>(features: &Arc<Features>, f: impl FnOnce(&mut Entry) -> R) -> R {
    let key = Arc::downgrade(features);
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|e| e.key.strong_count() > 0);
        let i = match m.iter().position(|e| e.key.ptr_eq(&key)) {
            Some(i) => i,
            None => {
                count();
                let patches: Arc<[Patch]> = feature::patches(features).into();
                if m.len() >= CAP {
                    m.remove(0);
                }
                m.push(Entry { key, patches, rings: HashMap::new() });
                m.len() - 1
            }
        };
        f(&mut m[i])
    })
}

/// [`feature::patches`] for a shared feature set, memoized by the set's
/// allocation. `Features` behind an `Arc` never changes and the held
/// `Weak` keeps the address from being reused, so a hit is the answer a
/// fresh grouping gives.
#[must_use]
pub fn patches(features: &Arc<Features>) -> Arc<[Patch]> {
    with_entry(features, |e| Arc::clone(&e.patches))
}

/// [`feature::patch_rings`] for a shared feature set, memoized like
/// [`patches`]; a miss (`None`) is remembered too.
#[must_use]
pub fn patch_rings(features: &Arc<Features>, id: PatchId) -> Option<Arc<[PatchRing]>> {
    if let Some(hit) = with_entry(features, |e| e.rings.get(&id).cloned()) {
        return hit;
    }
    count();
    let answer: Option<Arc<[PatchRing]>> = feature::patch_rings(features, id).map(Into::into);
    with_entry(features, |e| e.rings.insert(id, answer.clone()));
    answer
}

/// How many real patch groupings [`patches`] and [`patch_rings`] made on
/// this thread.
#[must_use]
pub fn patch_builds() -> u64 {
    BUILDS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{patch_builds, patch_rings, patches};
    use crate::geometry::feature::{self, Features, features_of_facets};
    use crate::geometry::{Point3, Triangle};

    /// A faceted cylinder wall: its strips weld into one patch.
    fn tube() -> Vec<Triangle> {
        let n = 64;
        let (r, h) = (5.0, 8.0);
        let p = |k: usize, z: f64| {
            let a = std::f64::consts::TAU * (k % n) as f64 / n as f64;
            Point3::new(r * a.cos(), r * a.sin(), z)
        };
        (0..n)
            .flat_map(|k| {
                let (a0, a1, b0, b1) = (p(k, 0.0), p(k + 1, 0.0), p(k, h), p(k + 1, h));
                [Triangle::new(a0, a1, b1), Triangle::new(a0, b1, b0)]
            })
            .collect()
    }

    fn ids(f: &Features) -> Vec<u64> {
        feature::patches(f).into_iter().map(|p| p.id).collect()
    }

    #[test]
    fn a_hit_answers_what_a_fresh_grouping_answers_and_groups_once() {
        let f = Arc::new(features_of_facets(&tube()));
        let fresh = ids(&f);
        assert!(!fresh.is_empty(), "a faceted tube welds into a patch");
        let before = patch_builds();
        assert_eq!(patches(&f).iter().map(|p| p.id).collect::<Vec<_>>(), fresh);
        assert_eq!(patches(&f).iter().map(|p| (p.id, p.faces.clone())).collect::<Vec<_>>(),
            feature::patches(&f).into_iter().map(|p| (p.id, p.faces)).collect::<Vec<_>>());
        assert_eq!(patch_builds() - before, 1, "the second ask is a hit");

        let rings = |r: Option<Vec<feature::PatchRing>>| r.map(|v| v.into_iter().map(|r| (r.verts, r.closed)).collect::<Vec<_>>());
        let before = patch_builds();
        let want = rings(feature::patch_rings(&f, fresh[0]));
        assert!(want.is_some(), "the patch has rings");
        assert_eq!(rings(patch_rings(&f, fresh[0]).map(|a| a.to_vec())), want);
        assert_eq!(rings(patch_rings(&f, fresh[0]).map(|a| a.to_vec())), want);
        assert!(patch_rings(&f, 12345).is_none());
        assert!(patch_rings(&f, 12345).is_none(), "a miss is remembered too");
        assert_eq!(patch_builds() - before, 2, "one build for each id");

        // A new feature set (a landing) is a new key: it groups again.
        let g = Arc::new(features_of_facets(&tube()));
        let before = patch_builds();
        assert_eq!(patches(&g).iter().map(|p| p.id).collect::<Vec<_>>(), fresh);
        assert_eq!(patch_builds() - before, 1, "another allocation never reads this one's answers");
    }
}

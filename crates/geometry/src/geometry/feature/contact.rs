//! Provenance: directed manifold incidence and the shared_edge contact rule.
//! Keep every incident facet until contact validation; a valid shared edge
//! yields one two-facet edge per solid, so smooth seams keep their patches.

use super::{EdgeEntry, find, union};
use crate::geometry::Point3;
use crate::geometry::shared_edge::{self, EdgeUse};
use std::collections::HashMap;

type Use = (u32, bool, u32);
type Key = (u32, u32);

/// Ordinary edges need no allocation. Keep extra incident facets only at
/// shared edges, rather than truncating them before contact validation.
#[derive(Default)]
struct Uses {
    first: [Use; 2],
    extra: Vec<Use>,
    count: usize,
}

impl Uses {
    fn push(&mut self, u: Use) {
        if let Some(slot) = self.first.get_mut(self.count) {
            *slot = u;
        } else {
            self.extra.push(u);
        }
        self.count += 1;
    }

    fn iter(&self) -> impl Iterator<Item = &Use> {
        self.first.iter().take(self.count.min(2)).chain(&self.extra)
    }
}

fn entry(uses: &Uses) -> EdgeEntry {
    let mut e = EdgeEntry {
        facets: [u32::MAX; 3],
        count: uses.count as u32,
    };
    for (slot, &(facet, _, _)) in e.facets.iter_mut().zip(uses.iter()) {
        *slot = facet;
    }
    e
}

pub(super) fn edges(corners: &[Option<[u32; 3]>], verts: &[[f64; 3]]) -> Vec<(Key, EdgeEntry)> {
    let mut on: HashMap<Key, Uses> = HashMap::new();
    for (i, row) in corners.iter().enumerate() {
        let Some([a, b, c]) = *row else { continue };
        for (p, q, r) in [(a, b, c), (b, c, a), (c, a, b)] {
            let key = if p < q { (p, q) } else { (q, p) };
            on.entry(key).or_default().push((i as u32, p < q, r));
        }
    }
    let mut on: Vec<_> = on.into_iter().collect();
    on.sort_by_key(|(key, _)| *key);
    if !on
        .iter()
        .any(|(_, uses)| uses.count >= 4 && uses.count.is_multiple_of(2))
    {
        return on
            .into_iter()
            .map(|(key, uses)| (key, entry(&uses)))
            .collect();
    }
    let mut parent: Vec<_> = (0..corners.len() as u32).collect();
    for (_, uses) in &on {
        if uses.count == 2 && uses.first[0].1 != uses.first[1].1 {
            union(&mut parent, uses.first[0].0, uses.first[1].0);
        }
    }
    let at = |v: u32| verts.get(v as usize).map(|p| Point3::new(p[0], p[1], p[2]));
    let edge_uses = |uses: &Uses, parent: &mut [u32]| -> Option<Vec<EdgeUse>> {
        uses.iter()
            .map(|&(i, forward, r)| {
                Some(EdgeUse {
                    body: find(parent, i) as usize,
                    forward,
                    apex: at(r)?,
                })
            })
            .collect()
    };
    for (key, uses) in &on {
        if uses.count != 4 {
            continue;
        }
        let (Some(lo), Some(hi), Some(eu)) = (at(key.0), at(key.1), edge_uses(uses, &mut parent))
        else {
            continue;
        };
        if !shared_edge::solids_touch(lo, hi, &eu)
            && let Some(pairs) = shared_edge::fragment_pairs(lo, hi, &eu)
        {
            for (i, j) in pairs {
                if let (Some(a), Some(b)) = (uses.iter().nth(i), uses.iter().nth(j)) {
                    union(&mut parent, a.0, b.0);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (key, uses) in on {
        let touch = (|| {
            if uses.count < 4 || !uses.count.is_multiple_of(2) {
                return None;
            }
            let eu = edge_uses(&uses, &mut parent)?;
            Some(shared_edge::solids_touch(at(key.0)?, at(key.1)?, &eu))
        })();
        if touch == Some(true) {
            let mut bodies: Vec<(u32, Uses)> = Vec::new();
            for &u in uses.iter() {
                let (i, _, _) = u;
                let root = find(&mut parent, i);
                if let Some((_, facets)) = bodies.iter_mut().find(|(r, _)| *r == root) {
                    facets.push(u);
                } else {
                    let mut facets = Uses::default();
                    facets.push(u);
                    bodies.push((root, facets));
                }
            }
            for (_, facets) in bodies {
                out.push((key, entry(&facets)));
            }
        } else {
            out.push((key, entry(&uses)));
        }
    }
    out
}

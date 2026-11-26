//! Is a body's facet winding its real outside? (#366) Face-normal
//! alignment picks (the plane lay, the 3-2-1) read a
//! face's outside from its facets' winding; a body wound inside-out (a
//! common STL export fault) would lay the part upside down silently. This
//! checks each body - a connected set of facets - once per mesh content:
//! closed (every edge shared by exactly two facets, wound opposite ways)
//! and its signed volume positive. #917: an edge where closed solids touch
//! along a wall (`shared_edge`) is no fault; each solid is its own body. Anything else refuses the pick by name;
//! nothing here mutates the user's geometry.
//!
//! Provenance: the signed volume of a closed triangulated surface by the
//! divergence theorem, summed as signed tetrahedra to a reference point
//! (Zhang & Chen, "Efficient feature extraction for 2D/3D objects in mesh
//! representation", ICIP 2001); the closed, consistently oriented
//! 2-manifold test (each undirected edge used once in each direction) is
//! textbook mesh topology (Botsch et al., "Polygon Mesh Processing", A K
//! Peters 2010, ch. 1). Vertices weld by
//! spatial hashing (Teschner et al., VMV 2003), as the mesh repair pass does.
//! Our own code, no vendor material.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use crate::geometry::{Point3, Triangle, shared_edge};

mod weld;
use weld::{Welder, root, union};

/// A closed body whose |volume| is at most this fraction of its bounding
/// box's volume (diagonal cubed) has no outside worth trusting.
const FLAT_REL: f64 = 1e-12;
/// Meshes remembered (most recently used last); bounded, owns no entity.
const CACHE_CAP: usize = 8;

/// What one body's winding says about its outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyWinding {
    /// Closed, consistently wound, positive volume: the winding is the outside.
    Outward,
    /// Closed and consistent, but its volume is negative: wound inward.
    InsideOut,
    /// Some edge borders only one facet: the shell has holes.
    Open,
    /// Some edge is shared by more than two facets.
    NonManifold,
    /// Neighbouring facets are wound opposite ways across an edge.
    Mixed,
    /// Closed and consistent but with no volume to tell a side by.
    Flat,
}

/// Per-mesh verdict: each facet's body (`None` for a collapsed or
/// unreadable facet) and each body's winding.
#[derive(Debug, Default)]
pub struct Winding {
    pub facet_body: Vec<Option<u32>>,
    pub bodies: Vec<BodyWinding>,
}

thread_local! {
    static CACHE: std::cell::RefCell<Vec<(u64, Arc<Winding>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The winding verdict of `tris`, memoized by an ORDERED, winding-sensitive
/// content key (the feature digest is order- and winding-free, so it cannot
/// tell an inverted mesh from its twin). Pure fuel: a miss recomputes.
#[must_use]
pub fn winding_of(tris: &[Triangle]) -> Arc<Winding> {
    let key = content_key(tris);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(i);
            cache.push(entry.clone());
            return entry.1;
        }
        let w = Arc::new(classify(tris));
        if cache.len() >= CACHE_CAP {
            cache.remove(0);
        }
        cache.push((key, w.clone()));
        w
    })
}

/// The worst verdict over the bodies the facets `which` belong to;
/// `None` when no facet of them could be read at all.
#[must_use]
pub fn verdict_for(w: &Winding, which: &[u32]) -> Option<BodyWinding> {
    let mut out: Option<BodyWinding> = None;
    for &i in which {
        let Some(Some(b)) = w.facet_body.get(i as usize) else { continue };
        let Some(&v) = w.bodies.get(*b as usize) else { continue };
        out = Some(match out {
            Some(prev) if rank(prev) >= rank(v) => prev,
            _ => v,
        });
    }
    out
}

/// Why a face of a body with verdict `v` has no trustworthy outside, as the
/// tail of a refusal ("Face 12 of 'part' {why}"); `None` for `Outward`.
/// No verdict at all (no readable facet) refuses too.
#[must_use]
pub fn refusal(v: Option<BodyWinding>) -> Option<&'static str> {
    let Some(v) = v else {
        return Some("has no readable facets, so its outside cannot be told - re-pick it");
    };
    Some(match v {
        BodyWinding::Outward => return None,
        BodyWinding::InsideOut => {
            "belongs to a body whose facets are inside-out (wound inward), so its outside would be read backwards - fix the model's facet winding (re-export it) before aligning to it"
        }
        BodyWinding::Open => {
            "belongs to an open body (its shell has holes), so its outside cannot be told - repair the mesh (fill its holes) or align to a closed part"
        }
        BodyWinding::NonManifold => {
            "belongs to a non-manifold body (an edge is shared by more than two facets), so its outside cannot be told - repair the model before aligning to it"
        }
        BodyWinding::Mixed => {
            "belongs to a body whose facets are wound both ways, so its outside cannot be told - fix the model's facet winding before aligning to it"
        }
        BodyWinding::Flat => "belongs to a body with no volume, so its outside cannot be told - align to a solid part",
    })
}

fn rank(v: BodyWinding) -> u8 {
    match v {
        BodyWinding::Outward => 0,
        BodyWinding::Flat => 1,
        BodyWinding::Open => 2,
        BodyWinding::Mixed => 3,
        BodyWinding::NonManifold => 4,
        BodyWinding::InsideOut => 5,
    }
}

fn content_key(tris: &[Triangle]) -> u64 {
    let mut h = DefaultHasher::new();
    tris.len().hash(&mut h);
    for t in tris {
        for p in [t.a, t.b, t.c] {
            p.x.to_bits().hash(&mut h);
            p.y.to_bits().hash(&mut h);
            p.z.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

/// Edge use across the mesh: facets running low->high and high->low, and
/// one facet on it (to find its body).
struct EdgeUse {
    fwd: u32,
    bwd: u32,
    facet: u32,
}

/// One facet on an edge used more than twice: its index, its direction
/// (low -> high corner) and its third corner.
type OnEdge = (u32, bool, u32);

/// #917: the edges used more than twice where closed solids touch along a
/// wall (`shared_edge::solids_touch`, as the slicer's bodies), judged on the
/// bodies linked over the other edges: they link and flag nothing. Any other
/// such edge links its facets and flags non-manifold, as before #917.
fn touching_solids(
    keys: &[Option<[u32; 3]>],
    welder: &Welder,
    edges: &HashMap<(u32, u32), EdgeUse>,
    parent: &mut [u32],
) -> HashSet<(u32, u32)> {
    let mut on: HashMap<(u32, u32), Vec<OnEdge>> = HashMap::new();
    for (i, k) in keys.iter().enumerate() {
        let Some([a, b, c]) = k else { continue };
        for (p, q, r) in [(*a, *b, *c), (*b, *c, *a), (*c, *a, *b)] {
            let key = if p < q { (p, q) } else { (q, p) };
            if edges.get(&key).is_some_and(|e| e.fwd.saturating_add(e.bwd) > 2) {
                on.entry(key).or_default().push((i as u32, p < q, r));
            }
        }
    }
    let at = |v: u32| welder.pts.get(v as usize).map(|v| Point3::new(v[0], v[1], v[2]));
    let mut contacts: Vec<_> = on.iter().collect();
    contacts.sort_by_key(|(key, _)| **key);
    for (key, uses) in contacts {
        let _ = (|| {
            let eu: Option<Vec<_>> = uses.iter().map(|&(i, forward, r)| {
                Some(shared_edge::EdgeUse { body: root(parent, i) as usize, forward, apex: at(r)? })
            }).collect();
            let eu = eu?;
            if !shared_edge::solids_touch(at(key.0)?, at(key.1)?, &eu)
                && let Some(pairs) = shared_edge::fragment_pairs(at(key.0)?, at(key.1)?, &eu)
            {
                for (i, j) in pairs {
                    union(parent, uses[i].0, uses[j].0);
                }
            }
            Some(())
        })();
    }
    let mut shared = HashSet::new();
    let mut faults = Vec::new();
    for (key, uses) in on {
        let touch = (|| {
            let mut eu = Vec::with_capacity(uses.len());
            for &(i, forward, r) in &uses {
                eu.push(shared_edge::EdgeUse { body: root(parent, i) as usize, forward, apex: at(r)? });
            }
            Some(shared_edge::solids_touch(at(key.0)?, at(key.1)?, &eu))
        })();
        if touch == Some(true) {
            shared.insert(key);
        } else {
            faults.push(uses);
        }
    }
    for uses in faults {
        if let Some(&(first, _, _)) = uses.first() {
            for &(i, _, _) in &uses {
                union(parent, first, i);
            }
        }
    }
    shared
}

/// The full pass: weld, link facets into bodies over shared edges, read
/// each body's edge topology, then its signed volume.
fn classify(tris: &[Triangle]) -> Winding {
    let n = tris.len().min(u32::MAX as usize);
    let mut welder = Welder::default();
    let mut keys: Vec<Option<[u32; 3]>> = Vec::with_capacity(n);
    for t in tris.iter().take(n) {
        let k = welder.corners(t);
        // A collapsed facet (two corners on one welded vertex) carries no
        // edge of its own: its two remaining sides cancel.
        keys.push(k.filter(|[a, b, c]| a != b && b != c && c != a));
    }
    let mut parent: Vec<u32> = (0..n as u32).collect();
    let mut edges: HashMap<(u32, u32), EdgeUse> = HashMap::new();
    for (i, k) in keys.iter().enumerate() {
        let Some([a, b, c]) = k else { continue };
        let i = i as u32;
        for (p, q) in [(*a, *b), (*b, *c), (*c, *a)] {
            let (lo, hi, fwd) = if p < q { (p, q, true) } else { (q, p, false) };
            let e = edges.entry((lo, hi)).or_insert(EdgeUse { fwd: 0, bwd: 0, facet: i });
            if fwd {
                e.fwd = e.fwd.saturating_add(1);
            } else {
                e.bwd = e.bwd.saturating_add(1);
            }
        }
    }
    // Link over the edges used at most twice. An edge used more than twice
    // is linked after, once it is known if solids touch there (#917).
    for (i, k) in keys.iter().enumerate() {
        let Some([a, b, c]) = *k else { continue };
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let Some(e) = edges.get(&if p < q { (p, q) } else { (q, p) }) else { continue };
            if e.fwd.saturating_add(e.bwd) <= 2 {
                union(&mut parent, e.facet, i as u32);
            }
        }
    }
    let shared = touching_solids(&keys, &welder, &edges, &mut parent);
    // Number the bodies by root.
    let mut body_of_root: HashMap<u32, u32> = HashMap::new();
    let mut facet_body: Vec<Option<u32>> = vec![None; tris.len()];
    for (i, k) in keys.iter().enumerate() {
        if k.is_none() {
            continue;
        }
        let r = root(&mut parent, i as u32);
        let next = body_of_root.len() as u32;
        let b = *body_of_root.entry(r).or_insert(next);
        if let Some(slot) = facet_body.get_mut(i) {
            *slot = Some(b);
        }
    }
    // Topology flags per body: (open, non-manifold, mixed).
    let mut flags = vec![(false, false, false); body_of_root.len()];
    for (key, e) in &edges {
        if shared.contains(key) {
            continue;
        }
        let Some(Some(b)) = facet_body.get(e.facet as usize) else { continue };
        let Some(f) = flags.get_mut(*b as usize) else { continue };
        match e.fwd.saturating_add(e.bwd) {
            1 => f.0 = true,
            2 if e.fwd == 1 => {}
            2 => f.2 = true,
            _ => f.1 = true,
        }
    }
    // Signed volume per body, about a reference corner of the body (keeps
    // the sum's rounding at the body's own scale), plus its bounds.
    let mut refs: Vec<Option<[f64; 3]>> = vec![None; flags.len()];
    let mut vol = vec![0.0_f64; flags.len()];
    let mut lo = vec![[f64::INFINITY; 3]; flags.len()];
    let mut hi = vec![[f64::NEG_INFINITY; 3]; flags.len()];
    for (i, t) in tris.iter().enumerate() {
        let Some(Some(b)) = facet_body.get(i) else { continue };
        let b = *b as usize;
        let (Some(r), Some(v), Some(l), Some(h)) = (refs.get_mut(b), vol.get_mut(b), lo.get_mut(b), hi.get_mut(b))
        else {
            continue;
        };
        let o = *r.get_or_insert([t.a.x, t.a.y, t.a.z]);
        let [a, bb, c] = [t.a, t.b, t.c].map(|p| [p.x - o[0], p.y - o[1], p.z - o[2]]);
        let cr = [bb[1] * c[2] - bb[2] * c[1], bb[2] * c[0] - bb[0] * c[2], bb[0] * c[1] - bb[1] * c[0]];
        *v += (a[0] * cr[0] + a[1] * cr[1] + a[2] * cr[2]) / 6.0;
        for p in [t.a, t.b, t.c] {
            for (k, x) in [p.x, p.y, p.z].into_iter().enumerate() {
                if let (Some(lk), Some(hk)) = (l.get_mut(k), h.get_mut(k)) {
                    *lk = lk.min(x);
                    *hk = hk.max(x);
                }
            }
        }
    }
    let bodies = flags
        .iter()
        .enumerate()
        .map(|(b, &(open, non_manifold, mixed))| {
            if non_manifold {
                return BodyWinding::NonManifold;
            }
            if open {
                return BodyWinding::Open;
            }
            if mixed {
                return BodyWinding::Mixed;
            }
            let v = vol.get(b).copied().unwrap_or(0.0);
            let (l, h) = (lo.get(b).copied().unwrap_or([0.0; 3]), hi.get(b).copied().unwrap_or([0.0; 3]));
            let d2: f64 = (0..3).map(|k| (h[k] - l[k]).powi(2)).sum();
            let box_vol = d2.sqrt().powi(3);
            if !(v.is_finite() && box_vol.is_finite()) || v.abs() <= FLAT_REL * box_vol {
                BodyWinding::Flat
            } else if v > 0.0 {
                BodyWinding::Outward
            } else {
                BodyWinding::InsideOut
            }
        })
        .collect();
    Winding { facet_body, bodies }
}

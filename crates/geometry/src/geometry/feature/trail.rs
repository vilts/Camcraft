//! The trail walk of [`chains`]: one joined component into ordered trails.
//!
//! Provenance: our own design notes (an Euler-style trail walk that takes
//! the straightest unused turn); the G1 seam preference is #2209. No vendor
//! material consulted.
//!
//! The walk takes the straightest unused continuation at each vertex. Where
//! the G1 seam rule ([`g1_seam`]) joined two edges at a vertex, the walk
//! keeps them consecutive: arriving over one, it takes the other first, and
//! it takes neither from a third edge while a pair is still unused. That
//! keeps a smooth crease one trail even when its component also holds
//! tessellation seams. With no seam pair in a component the walk is the
//! plain straightest-turn walk.

use super::*;

/// One component's walk state.
pub(super) struct Walk<'a> {
    pub edges: &'a [TopEdge],
    pub lattice: &'a [Pos],
    pub es: &'a [u32],
    /// vertex -> [(edge slot, far vertex)].
    pub adj: &'a HashMap<u32, Vec<(usize, u32)>>,
    /// (vertex, edge) -> the edge the G1 seam rule joined it to there.
    pub seam: &'a HashMap<(u32, u32), u32>,
    /// edge id -> slot in `es`.
    pub slot: HashMap<u32, usize>,
}

impl Walk<'_> {
    /// The unused slot of the seam partner of `e` at `v`, if any.
    pub fn partner_unused(&self, used: &[bool], v: u32, e: u32) -> Option<usize> {
        let p = self.seam.get(&(v, e))?;
        let k = *self.slot.get(p)?;
        (!used.get(k).copied().unwrap_or(true)).then_some(k)
    }

    /// Straightness of the turn `prev -> cur -> nxt` (cosine); unrankable
    /// (an unweldable position) ranks last but still walks.
    fn rank(&self, prev: u32, cur: u32, nxt: u32) -> f64 {
        match (
            self.lattice.get(prev as usize),
            self.lattice.get(cur as usize),
            self.lattice.get(nxt as usize),
        ) {
            (Some(p), Some(q), Some(r)) => {
                let (d1, d2) = (delta(*p, *q), delta(*q, *r));
                let dot = d1[0] * d2[0] + d1[1] * d2[1] + d1[2] * d2[2];
                let m = (norm2(d1) * norm2(d2)).sqrt();
                if m > 0.0 && m.is_finite() { dot / m } else { f64::MIN }
            }
            _ => f64::MIN,
        }
    }

    /// Walks on from `cur` (reached from `prev` over edge `last`) until no
    /// unused edge is left there or the walk reaches `stop`. Returns the
    /// edges taken and the end vertex.
    pub fn run(&self, used: &mut [bool], mut prev: u32, mut cur: u32, mut last: u32, stop: u32) -> (Vec<u32>, u32) {
        let mut out = Vec::new();
        while let Some(cands) = self.adj.get(&cur) {
            let forced = self.partner_unused(used, cur, last);
            // Key: (tier, straightness). Tier 2 = the seam partner of the
            // edge we arrived on; tier 0 = an edge whose own seam partner
            // here is still unused (it waits for that partner).
            let mut pick: Option<(u8, f64, usize, u32)> = None;
            for &(k, nxt) in cands {
                if used.get(k).copied().unwrap_or(true) {
                    continue;
                }
                let Some(&ek) = self.es.get(k) else { continue };
                let tier = if Some(k) == forced {
                    2
                } else if self.partner_unused(used, cur, ek).is_some() {
                    0
                } else {
                    1
                };
                let rank = self.rank(prev, cur, nxt);
                let better = match pick {
                    None => true,
                    Some((bt, br, _, _)) => tier > bt || (tier == bt && rank > br),
                };
                if better {
                    pick = Some((tier, rank, k, nxt));
                }
            }
            let Some((_, _, k, nxt)) = pick else { break };
            let Some(&ek) = self.es.get(k) else { break };
            if let Some(u) = used.get_mut(k) {
                *u = true;
            }
            out.push(ek);
            last = ek;
            prev = cur;
            cur = nxt;
            if cur == stop {
                break;
            }
        }
        (out, cur)
    }
}

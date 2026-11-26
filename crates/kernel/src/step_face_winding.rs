//! The faces of a STEP closed shell whose loops run against their
//! neighbours (#917, the note of #1576): solidcore's reader orients them
//! with the shell and leaves no trace, so the seam finds them in the file
//! and names them, as the earlier importer did ("shell #7: 1 face(s) wound
//! against their neighbours were flipped: face #32 (3 of 6)").
//! Provenance: ISO 10303-42 topology (CLOSED_SHELL, ORIENTED_FACE,
//! FACE_BOUND orientation, ORIENTED_EDGE orientation: in a closed shell
//! each edge is used twice, once in each direction); consistent orientation
//! by relative parity across shared edges (Botsch et al., "Polygon Mesh
//! Processing", 2010, ch. 1) as a union-find with parity (Tarjan, J. ACM
//! 22(2), 1975). Read through the public p21 API of solidcore-step. Our
//! own code; no HSMWorks material consulted.

use std::collections::{BTreeMap, BTreeSet};

use solidcore::kernel::CancelToken;
use solidcore_step::p21::{Exchange, Param};

/// The most faces one note names; the rest are counted.
const NAMED: usize = 8;

/// One note for each closed shell that has faces wound against their
/// neighbours, naming the faces (the smaller side of each connected
/// group). A shell whose edge uses do not resolve to one winding (an edge
/// used other than twice, or an odd cycle) gets no note: the reader and
/// the mesh judge it.
/// `Err(Stopped)` when `stop` is cancelled during the pass: the caller
/// refuses the file as timed out (the check reads each face of each shell,
/// and a shell can name one large face many times).
pub(crate) fn notes(ex: &Exchange, stop: &CancelToken) -> Result<Vec<String>, Stopped> {
    let mut out = Vec::new();
    for id in closed_shells(ex) {
        if let Some(note) = shell_note(ex, id, stop)? {
            out.push(note);
        }
    }
    Ok(out)
}

/// The time bound ran out during the winding check.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stopped;

/// Every CLOSED_SHELL, and every outer shell of a MANIFOLD_SOLID_BREP
/// whatever its keyword, by id.
fn closed_shells(ex: &Exchange) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    for (id, e) in &ex.entities {
        if e.has("CLOSED_SHELL") && !e.has("ORIENTED_CLOSED_SHELL") {
            found.insert(*id);
        } else if let Some(outer) = e.record("MANIFOLD_SOLID_BREP").and_then(|p| p.get(1)).and_then(Param::as_ref_id) {
            found.insert(outer);
        }
    }
    found
}

fn flag(p: Option<&Param>) -> Option<bool> {
    p.and_then(Param::as_bool)
}

/// The most ORIENTED_FACE / ORIENTED_EDGE wrappers read around one entity.
/// A deeper chain (or one that names itself) is not read: the shell gets no
/// note, and the topology reader judges it (#2904 review: no unbounded
/// recursion on corrupt input).
const MAX_WRAP: u32 = 4;

/// The face entity of a shell member and whether the shell uses it as
/// written.
fn face_of(ex: &Exchange, id: u64, depth: u32) -> Option<(u64, bool)> {
    let e = ex.get(id)?;
    match e.record("ORIENTED_FACE") {
        Some(p) if depth < MAX_WRAP => {
            let (inner, fwd) = face_of(ex, p.get(2)?.as_ref_id()?, depth + 1)?;
            Some((inner, fwd == flag(p.get(3))?))
        }
        Some(_) => None,
        None => Some((id, true)),
    }
}

/// The edge entity of a loop member and whether the loop runs along it.
fn edge_of(ex: &Exchange, id: u64, depth: u32) -> Option<(u64, bool)> {
    let e = ex.get(id)?;
    match e.record("ORIENTED_EDGE") {
        Some(p) if depth < MAX_WRAP => {
            let (inner, fwd) = edge_of(ex, p.get(3)?.as_ref_id()?, depth + 1)?;
            Some((inner, fwd == flag(p.get(4))?))
        }
        Some(_) => None,
        None => Some((id, true)),
    }
}

/// Each edge use of one face, `(edge, runs along it)`, with the face's use
/// in the shell applied. `None`: a member this check does not read.
fn edge_uses(ex: &Exchange, face: u64, fwd: bool) -> Option<Vec<(u64, bool)>> {
    let e = ex.get(face)?;
    let p = ["ADVANCED_FACE", "FACE_SURFACE", "FACE"].iter().find_map(|n| e.record(n))?;
    let mut uses = Vec::new();
    for b in p.get(1)?.as_list()? {
        let bound = ex.get(b.as_ref_id()?)?;
        let bp = bound.record("FACE_OUTER_BOUND").or_else(|| bound.record("FACE_BOUND"))?;
        let along = flag(bp.get(2))? == fwd;
        let Some(lp) = ex.get(bp.get(1)?.as_ref_id()?)?.record("EDGE_LOOP") else {
            continue; // a VERTEX_LOOP uses no edge
        };
        for oe in lp.get(1)?.as_list()? {
            let (edge, dir) = edge_of(ex, oe.as_ref_id()?, 0)?;
            uses.push((edge, dir == along));
        }
    }
    Some(uses)
}

/// Union-find with parity: a face's winding relative to its root. Union by
/// size and full path compression keep each `find` near constant, whatever
/// the order of the edges in the file (#2904 review: a hostile order made
/// the plain chain quadratic).
struct Parity {
    up: Vec<usize>,
    odd: Vec<bool>,
    size: Vec<usize>,
    /// Links walked by every `find` so far: the cost the tests bound.
    steps: usize,
}

impl Parity {
    fn new(n: usize) -> Self {
        Self { up: (0..n).collect(), odd: vec![false; n], size: vec![1; n], steps: 0 }
    }

    fn find(&mut self, f: usize) -> (usize, bool) {
        let (mut r, mut total) = (f, false);
        while let Some(&u) = self.up.get(r) {
            if u == r {
                break;
            }
            self.steps = self.steps.saturating_add(1);
            total ^= self.odd.get(r).copied().unwrap_or(false);
            r = u;
        }
        // Second walk: point each node of the path at the root, with its
        // own parity to the root.
        let (mut x, mut p) = (f, total);
        while x != r {
            let (Some(up), Some(odd)) = (self.up.get_mut(x), self.odd.get_mut(x)) else { break };
            let (next, was) = (*up, *odd);
            (*up, *odd) = (r, p);
            p ^= was;
            x = next;
        }
        (r, total)
    }

    /// Joins `a` and `b` with `b` wound `differ` against `a`; `false` when
    /// that contradicts what is known.
    fn join(&mut self, a: usize, b: usize, differ: bool) -> bool {
        let ((ra, pa), (rb, pb)) = (self.find(a), self.find(b));
        if ra == rb {
            return (pa ^ pb) == differ;
        }
        // The smaller tree goes under the larger one; the link parity is
        // the same either way round.
        let (sa, sb) = (self.size.get(ra).copied().unwrap_or(1), self.size.get(rb).copied().unwrap_or(1));
        let (child, root) = if sa < sb { (ra, rb) } else { (rb, ra) };
        if let (Some(u), Some(o)) = (self.up.get_mut(child), self.odd.get_mut(child)) {
            (*u, *o) = (root, pa ^ pb ^ differ);
        }
        if let Some(s) = self.size.get_mut(root) {
            *s = sa.saturating_add(sb);
        }
        true
    }
}

fn shell_note(ex: &Exchange, shell: u64, stop: &CancelToken) -> Result<Option<String>, Stopped> {
    let Some(members) = shell_members(ex, shell) else { return Ok(None) };
    let mut by_edge: BTreeMap<u64, Vec<(usize, bool)>> = BTreeMap::new();
    let mut names = Vec::with_capacity(members.len());
    for (i, m) in members.iter().enumerate() {
        if stop.is_cancelled() {
            return Err(Stopped);
        }
        let Some((face, fwd)) = face_of(ex, *m, 0) else { return Ok(None) };
        names.push(face);
        let Some(uses) = edge_uses(ex, face, fwd) else { return Ok(None) };
        for (edge, dir) in uses {
            let at = by_edge.entry(edge).or_default();
            if at.len() > 2 {
                return Ok(None); // an edge used other than twice: not judged here
            }
            at.push((i, dir));
        }
    }
    if stop.is_cancelled() {
        return Err(Stopped);
    }
    Ok(winding_note(shell, &names, &by_edge))
}

fn shell_members(ex: &Exchange, shell: u64) -> Option<Vec<u64>> {
    ex.get(shell)?.params().get(1)?.as_list()?.iter().map(Param::as_ref_id).collect()
}

/// The note of one shell from its edge uses; `None` when the uses do not
/// resolve to one winding or no face is wound against the rest.
fn winding_note(shell: u64, names: &[u64], by_edge: &BTreeMap<u64, Vec<(usize, bool)>>) -> Option<String> {
    let n = names.len();
    let mut parity = Parity::new(n);
    for uses in by_edge.values() {
        let [(a, da), (b, db)] = uses.as_slice() else { return None };
        // Two uses the same way: the two faces are wound against each other.
        if a != b && !parity.join(*a, *b, da == db) {
            return None;
        }
    }
    // In each group the side with fewer faces is the one wound against the
    // rest (a tie: the side without the group's lowest face).
    let mut groups: BTreeMap<usize, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for f in 0..n {
        let (root, odd) = parity.find(f);
        let g = groups.entry(root).or_default();
        if odd { g.1.push(f) } else { g.0.push(f) }
    }
    let mut flipped: Vec<usize> = Vec::new();
    for (even, odd) in groups.into_values() {
        let lowest_even = even.first().copied().unwrap_or(usize::MAX) < odd.first().copied().unwrap_or(usize::MAX);
        let take_odd = odd.len() < even.len() || (odd.len() == even.len() && lowest_even);
        flipped.extend(if take_odd { odd } else { even });
    }
    if flipped.is_empty() {
        return None;
    }
    flipped.sort_unstable();
    let mut said: Vec<String> = flipped
        .iter()
        .take(NAMED)
        .filter_map(|&f| names.get(f).map(|id| format!("face #{id} ({} of {n})", f + 1)))
        .collect();
    if flipped.len() > NAMED {
        said.push(format!("{} more", flipped.len() - NAMED));
    }
    Some(format!(
        "shell #{shell}: {} face(s) wound against their neighbours were flipped: {}",
        flipped.len(),
        said.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order that made the plain chain quadratic: each join hangs the
    /// whole group under a new single face, so face 0 sinks one level each
    /// time. Without union by size and path compression this walks about
    /// n^2 / 2 = 2e10 links; the bound below is linear.
    #[test]
    fn a_hostile_edge_order_costs_linear_steps_and_keeps_each_parity() {
        let n = 200_000;
        let mut p = Parity::new(n);
        for k in 1..n {
            assert!(p.join(k, 0, k % 2 == 1), "join {k}");
            let _ = p.find(0);
        }
        assert!(p.steps <= 8 * n, "{} links walked for {n} faces", p.steps);
        let (r0, p0) = p.find(0);
        for k in 0..n {
            let (r, pk) = p.find(k);
            assert_eq!(r, r0, "face {k} is in the one group");
            assert_eq!(pk ^ p0, k % 2 == 1, "face {k} parity against face 0");
        }
        assert!(!p.join(1, 0, false), "a contradiction is still seen after compression");
        assert!(p.join(2, 0, false), "a consistent join is still accepted");
    }

    fn box_exchange() -> Exchange {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
        let text = std::fs::read_to_string(path).expect("box.step");
        solidcore_step::p21::parse(&text).expect("box.step parses")
    }

    #[test]
    fn a_cancelled_winding_pass_stops_by_name() {
        let stop = CancelToken::new();
        assert_eq!(notes(&box_exchange(), &stop), Ok(Vec::new()), "the good box has no note");
        stop.cancel();
        assert_eq!(notes(&box_exchange(), &stop), Err(Stopped));
    }
}

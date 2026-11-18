//! Sketch chaining: join 2D line, arc and circle primitives into closed loops and open chains,
//! give each a stable identity, and read the closed loops as outer boundaries or holes.
//!
//! Provenance: our own design from published computational-geometry literature; each submodule
//! names its sources (grid neighbour search and union-find in `connect`, FNV-1a in `identity`,
//! sweep and prune in `crossing`, closed-form contacts in `intersect`, crossing-number
//! containment in `nesting`). No vendor code or output was consulted.
//!
//! Contract. Primitives and the join tolerance are in millimetres (scale a DXF document by its
//! units first). Ends join only when they lie within the tolerance, and only on the same layer;
//! a wider gap stays open and is reported, never closed. Curves are kept analytic: every chain
//! segment is the input line, arc or circle, turned to the walk direction. Anything that cannot
//! be a chain is listed in `rejected` with a named error, never dropped silently and never
//! repaired. Nesting is read over all closed chains passed in, whatever their layers: filter the
//! primitives first to nest a subset. This module has no file I/O and no application state.

mod connect;
mod crossing;
mod curve;
mod identity;
mod intersect;
mod nesting;
mod types;

use crate::dxf::LayeredPrimitive;
use connect::{Component, Walk};
use crossing::{Sealed, Search};
use curve::{Key, Piece, Shape};
use std::collections::{BTreeMap, HashMap, HashSet};

pub use types::*;

/// Pair tests one call may spend on joining, crossing search and nesting before it is refused.
const WORK_LIMIT: u64 = 200_000_000;

pub(crate) struct Budget(u64);

impl Budget {
    fn spend(&mut self, work: usize) -> Result<(), ChainError> {
        self.0 = self
            .0
            .checked_sub(work as u64)
            .ok_or(ChainError::TooComplex)?;
        Ok(())
    }
}

/// Geometry of a primitive as a key that is equal for a repeat drawn in either direction.
fn repeat_key(piece: &Piece) -> (u8, [Key; 3], i64) {
    let (a, b) = (curve::key(piece.a), curve::key(piece.b));
    match piece.shape {
        Shape::Line => (1, [a.min(b), a.max(b), (0, 0)], 0),
        Shape::Arc {
            center,
            radius,
            sweep,
            ..
        } => {
            let (from, to) = if sweep < 0.0 { (b, a) } else { (a, b) };
            (2, [from, to, curve::key(center)], curve::quantum(radius))
        }
        Shape::Circle { center, radius } => (3, [curve::key(center); 3], curve::quantum(radius)),
    }
}

struct Draft {
    layer: String,
    walk: Walk,
    id: ChainId,
    area: f64,
}

impl Draft {
    fn order(&self) -> (&str, Key, ChainId) {
        (&self.layer, identity::start_key(&self.walk), self.id)
    }

    fn indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self.walk.pieces.iter().map(|(index, _)| *index).collect();
        indices.sort_unstable();
        indices
    }

    fn segments(&self) -> Vec<ChainSegment> {
        let segment = |&(primitive, piece): &(usize, Piece)| ChainSegment {
            primitive,
            curve: piece.oriented(),
            reversed: piece.reversed,
            start: piece.a,
            end: piece.b,
        };
        self.walk.pieces.iter().map(segment).collect()
    }
}

/// Chain `primitives` (millimetres) by `options`. `Err` is for input no chain can be read
/// from at all: a bad tolerance, a non-finite, out-of-range or invalid primitive, or a drawing
/// over the size and work limits. Everything else is in the result, chained or rejected by name.
pub fn chain(
    primitives: &[LayeredPrimitive],
    options: ChainOptions,
) -> Result<SketchChains, ChainError> {
    let tol = options.join_tolerance;
    if !(tol.is_finite() && (MIN_JOIN_TOLERANCE..=MAX_JOIN_TOLERANCE).contains(&tol)) {
        return Err(ChainError::InvalidTolerance);
    }
    if primitives.len() > MAX_PRIMITIVES {
        return Err(ChainError::TooComplex);
    }
    let mut budget = Budget(WORK_LIMIT);
    let mut out = SketchChains::default();
    let mut layers: BTreeMap<&str, Vec<(usize, Piece)>> = BTreeMap::new();
    let mut seen = HashMap::new();
    let mut extent = 0.0f64;
    for (index, item) in primitives.iter().enumerate() {
        let piece = Piece::from_primitive(index, &item.primitive)?;
        let (lo, hi) = piece.bounds();
        extent = [lo.x, lo.y, hi.x, hi.y]
            .iter()
            .fold(extent, |m, v| m.max(v.abs()));
        let first = *seen
            .entry((item.layer.as_str(), repeat_key(&piece)))
            .or_insert(index);
        if first == index {
            layers
                .entry(item.layer.as_str())
                .or_default()
                .push((index, piece));
        } else {
            let error = ChainError::DuplicatePrimitive {
                primitive: index,
                of: first,
            };
            out.rejected.push(Rejected {
                layer: item.layer.clone(),
                primitives: vec![index],
                error,
            });
        }
    }
    // Absolute slack of the contact tests: above float noise at this extent, and (coordinates
    // are below 1e7 mm) never above the smallest join tolerance.
    let eps = 1e-12 * (1.0 + extent);

    let mut drafts = Vec::new();
    for (layer, pieces) in &layers {
        for component in connect::connect(pieces, tol, &mut budget)? {
            match component {
                Component::Rejected(mut primitives, error) => {
                    primitives.sort_unstable();
                    out.rejected.push(Rejected {
                        layer: (*layer).into(),
                        primitives,
                        error,
                    });
                }
                Component::Walk(mut walk) => {
                    let area = identity::canonicalize(&mut walk);
                    let id = identity::chain_id(layer, &walk);
                    drafts.push(Draft {
                        layer: (*layer).into(),
                        walk,
                        id,
                        area,
                    });
                }
            }
        }
    }
    drafts.sort_by(|p, q| p.order().cmp(&q.order()));
    let mut ids = HashSet::new();
    let mut closed = Vec::new();
    for draft in drafts {
        if !ids.insert(draft.id) {
            let error = ChainError::DuplicateChain { of: draft.id };
            out.rejected.push(Rejected {
                layer: draft.layer.clone(),
                primitives: draft.indices(),
                error,
            });
        } else if draft.walk.closed {
            closed.push(draft);
        } else {
            let (first, last) = (draft.walk.pieces.first(), draft.walk.pieces.last());
            let (Some((_, first)), Some((_, last))) = (first, last) else {
                continue;
            };
            out.open.push(OpenChain {
                id: draft.id,
                layer: draft.layer.clone(),
                segments: draft.segments(),
                start: first.a,
                end: last.b,
                max_gap: identity::max_gap(&draft.walk),
            });
        }
    }
    nest(closed, tol, eps, &mut budget, &mut out)?;
    out.rejected
        .sort_by(|p, q| (&p.layer, &p.primitives).cmp(&(&q.layer, &q.primitives)));
    Ok(out)
}

/// Refuse the self-crossing and the empty loops, then read the nesting of the rest.
fn nest(
    closed: Vec<Draft>,
    tol: f64,
    eps: f64,
    budget: &mut Budget,
    out: &mut SketchChains,
) -> Result<(), ChainError> {
    let loops: Vec<Sealed> = closed
        .iter()
        .map(|draft| {
            Sealed::new(
                &draft
                    .walk
                    .pieces
                    .iter()
                    .map(|(_, piece)| *piece)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let within = crossing::search(
        &loops,
        &vec![false; loops.len()],
        Search::Within,
        tol,
        eps,
        budget,
    )?;
    // A loop that crosses itself or encloses nothing is refused and takes no part in nesting.
    let refusal = |i: usize| match (within.get(i), closed.get(i)) {
        (Some(&Some((_, at))), _) => Some(ChainError::SelfCrossing { at }),
        (_, Some(draft)) if draft.area <= tol * tol => Some(ChainError::ZeroArea),
        _ => None,
    };
    let refused: Vec<bool> = (0..loops.len()).map(|i| refusal(i).is_some()).collect();
    let between = crossing::search(&loops, &refused, Search::Between, tol, eps, budget)?;
    let containers = nesting::containers(&loops, &refused, budget)?;
    let id_of = |i: usize| closed.get(i).map(|draft| draft.id);
    let ambiguous = |i: usize| match between.get(i) {
        Some(&Some((other, at))) => {
            id_of(other).map(|other| ChainError::NestingAmbiguous { other, at })
        }
        _ => None,
    };
    for (i, draft) in closed.iter().enumerate() {
        if let Some(error) = refusal(i) {
            out.rejected.push(Rejected {
                layer: draft.layer.clone(),
                primitives: draft.indices(),
                error,
            });
            continue;
        }
        let around = containers.get(i).map_or(&[][..], Vec::as_slice);
        let depth_of = |j: usize| containers.get(j).map_or(0, Vec::len);
        let nesting = match ambiguous(i).or_else(|| around.iter().find_map(|&j| ambiguous(j))) {
            Some(error) => Err(error),
            None => Ok(Nesting {
                role: if around.len().is_multiple_of(2) {
                    LoopRole::Outer
                } else {
                    LoopRole::Hole
                },
                depth: around.len(),
                parent: around
                    .iter()
                    .copied()
                    .max_by_key(|&j| depth_of(j))
                    .and_then(id_of),
            }),
        };
        out.closed.push(ClosedChain {
            id: draft.id,
            layer: draft.layer.clone(),
            segments: draft.segments(),
            area: draft.area,
            max_gap: identity::max_gap(&draft.walk),
            nesting,
        });
    }
    Ok(())
}

//! Provenance: tolerance joining of sketch endpoints. Fixed-radius neighbour search on a uniform
//! grid (Bentley, Stanat and Williams, "The complexity of finding fixed-radius near neighbors",
//! Information Processing Letters 6(6), 1977); clusters by disjoint-set union (Tarjan,
//! "Efficiency of a good but not linear set union algorithm", JACM 22(2), 1975); a component
//! whose joints all have degree two or less is walked as a path or a cycle (any graph text,
//! e.g. Cormen et al., "Introduction to Algorithms", ch. 22). Original implementation.

use super::curve::{Piece, Shape, dist};
use super::{Budget, ChainError};
use crate::geo2d::Point2;
use std::collections::HashMap;
use std::f64::consts::PI;

/// Pieces in walk order, each with its input index.
#[derive(Clone, Debug)]
pub(super) struct Walk {
    pub pieces: Vec<(usize, Piece)>,
    pub closed: bool,
}

pub(super) enum Component {
    Walk(Walk),
    /// The input indices of the component and why it cannot be a chain.
    Rejected(Vec<usize>, ChainError),
}

struct Sets(Vec<usize>);

impl Sets {
    fn new(n: usize) -> Sets {
        Sets((0..n).collect())
    }

    fn find(&mut self, mut i: usize) -> usize {
        // Path halving: every step shortens the path, so the loop ends at the root.
        while let Some(&parent) = self.0.get(i) {
            if parent == i {
                break;
            }
            let grand = self.0.get(parent).copied().unwrap_or(parent);
            if let Some(slot) = self.0.get_mut(i) {
                *slot = grand;
            }
            i = grand;
        }
        i
    }

    /// The smaller root wins, so the result does not depend on the order of the unions.
    fn union(&mut self, i: usize, j: usize) {
        let (a, b) = (self.find(i), self.find(j));
        let (keep, drop) = if a <= b { (a, b) } else { (b, a) };
        if let Some(slot) = self.0.get_mut(drop) {
            *slot = keep;
        }
    }
}

fn end_point(items: &[(usize, Piece)], end: usize) -> Option<Point2> {
    let (_, piece) = items.get(end / 2)?;
    Some(if end.is_multiple_of(2) {
        piece.a
    } else {
        piece.b
    })
}

/// Which cluster each piece end joined, and the ends of every cluster in ascending order.
struct Joints {
    of_end: Vec<usize>,
    members: Vec<Vec<usize>>,
}

impl Joints {
    fn cluster(&self, end: usize) -> usize {
        self.of_end.get(end).copied().unwrap_or(usize::MAX)
    }

    fn ends_of(&self, end: usize) -> &[usize] {
        self.members
            .get(self.cluster(end))
            .map_or(&[][..], Vec::as_slice)
    }
}

/// Group `keys` (one per element) into lists of element numbers, in order of first appearance.
fn group(keys: impl Iterator<Item = usize>) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut number: HashMap<usize, usize> = HashMap::new();
    let mut of = Vec::new();
    let mut lists: Vec<Vec<usize>> = Vec::new();
    for (element, key) in keys.enumerate() {
        let id = *number.entry(key).or_insert(lists.len());
        if id == lists.len() {
            lists.push(Vec::new());
        }
        if let Some(list) = lists.get_mut(id) {
            list.push(element);
        }
        of.push(id);
    }
    (of, lists)
}

/// Cluster the ends of `items` (no circles) that lie within `tol` of each other.
fn clusters(items: &[(usize, Piece)], tol: f64, budget: &mut Budget) -> Result<Joints, ChainError> {
    let ends = items.len() * 2;
    // Cells of twice the tolerance: two ends within the tolerance are at most one cell apart
    // even after the rounding of the division (coordinates are below SKETCH_LIMIT).
    let cell = |p: Point2| {
        (
            (p.x / (2.0 * tol)).floor() as i64,
            (p.y / (2.0 * tol)).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut sets = Sets::new(ends);
    for end in 0..ends {
        let Some(p) = end_point(items, end) else {
            continue;
        };
        let (cx, cy) = cell(p);
        for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                let Some(others) = grid.get(&(cx.saturating_add(dx), cy.saturating_add(dy))) else {
                    continue;
                };
                budget.spend(others.len())?;
                for &other in others {
                    if end_point(items, other).is_some_and(|q| dist(p, q) <= tol) {
                        sets.union(end, other);
                    }
                }
            }
        }
        grid.entry((cx, cy)).or_default().push(end);
    }
    let roots: Vec<usize> = (0..ends).map(|end| sets.find(end)).collect();
    let (of_end, members) = group(roots.into_iter());
    Ok(Joints { of_end, members })
}

fn centroid(items: &[(usize, Piece)], ends: &[usize]) -> Point2 {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
    for p in ends.iter().filter_map(|&end| end_point(items, end)) {
        x += p.x;
        y += p.y;
        n += 1.0;
    }
    if n == 0.0 {
        Point2::default()
    } else {
        Point2::new(x / n, y / n)
    }
}

/// Join one layer's validated pieces into components and walk each clean one.
pub(super) fn connect(
    pieces: &[(usize, Piece)],
    tol: f64,
    budget: &mut Budget,
) -> Result<Vec<Component>, ChainError> {
    let mut out = Vec::new();
    let mut items = Vec::new();
    for &(index, piece) in pieces {
        match piece.shape {
            Shape::Circle { .. } => {
                out.push(Component::Walk(Walk {
                    pieces: vec![(index, piece)],
                    closed: true,
                }));
            }
            _ => items.push((index, piece)),
        }
    }
    let joints = clusters(&items, tol, budget)?;
    // Pieces that share a joint belong to one component.
    let mut sets = Sets::new(items.len());
    for ends in &joints.members {
        if let Some((&first, rest)) = ends.split_first() {
            for &end in rest {
                sets.union(first / 2, end / 2);
            }
        }
    }
    let roots: Vec<usize> = (0..items.len()).map(|item| sets.find(item)).collect();
    let (_, components) = group(roots.into_iter());
    for component in components {
        match walk(&items, &component, &joints) {
            Ok(walk) => out.push(Component::Walk(walk)),
            Err(error) => {
                let indices = component
                    .iter()
                    .filter_map(|&item| items.get(item).map(|(index, _)| *index));
                out.push(Component::Rejected(indices.collect(), error));
            }
        }
    }
    Ok(out)
}

/// An arc that returns to its own start within the tolerance is a closed loop of one piece.
fn closes_on_itself(piece: &Piece) -> bool {
    matches!(piece.shape, Shape::Arc { sweep, .. } if sweep.abs() > PI)
}

fn walk(
    items: &[(usize, Piece)],
    component: &[usize],
    joints: &Joints,
) -> Result<Walk, ChainError> {
    let alone = component.len() == 1;
    for &item in component {
        if joints.cluster(item * 2) != joints.cluster(item * 2 + 1) {
            continue;
        }
        let Some(&(index, piece)) = items.get(item) else {
            continue;
        };
        if alone && joints.ends_of(item * 2).len() == 2 && closes_on_itself(&piece) {
            return Ok(Walk {
                pieces: vec![(index, piece)],
                closed: true,
            });
        }
        return Err(ChainError::DegenerateSegment { primitive: index });
    }
    // The branch reported is the lowest in (x, y), whatever the input order.
    let mut branch: Option<(Point2, usize)> = None;
    let mut start = None;
    for &item in component {
        for end in [item * 2, item * 2 + 1] {
            let ends = joints.ends_of(end);
            if ends.len() > 2 {
                let at = centroid(items, ends);
                let lower = |old: Point2| at.x < old.x || (at.x == old.x && at.y < old.y);
                if branch.is_none_or(|(old, _)| lower(old)) {
                    branch = Some((at, ends.len()));
                }
            } else if ends.len() == 1 && start.is_none() {
                start = Some(end);
            }
        }
    }
    if let Some((at, degree)) = branch {
        return Err(ChainError::Branch { at, degree });
    }
    let closed = start.is_none();
    let first = start
        .or_else(|| component.first().map(|&item| item * 2))
        .unwrap_or(0);
    let mut pieces = Vec::with_capacity(component.len());
    let mut enter = first;
    // Every piece is entered once, so the walk takes at most `component.len()` steps.
    for _ in 0..component.len() {
        let Some(&(index, piece)) = items.get(enter / 2) else {
            break;
        };
        pieces.push((
            index,
            if enter.is_multiple_of(2) {
                piece
            } else {
                piece.flipped()
            },
        ));
        let exit = enter ^ 1;
        match joints.ends_of(exit).iter().find(|&&end| end != exit) {
            Some(&next) if next / 2 != first / 2 => enter = next,
            _ => break,
        }
    }
    Ok(Walk { pieces, closed })
}

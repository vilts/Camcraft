//! The slicer's union fallback (#1407): a soup that meets a plane
//! non-manifold because it holds several closed solids that touch or
//! overlap (a STEP part exported as stacked bodies sharing a wall) is
//! sectioned body by body and the sections are united.
//!
//! Provenance: a plane section of a union of solids is the union of the
//! solids' sections (regularized set operations, Requicha, "Representations
//! for Rigid Solids", ACM Computing Surveys 12(4), 1980); a body is a set of
//! facets connected across shared edges, accepted only when it is a closed,
//! consistently oriented 2-manifold (every undirected edge used once in
//! each direction - Botsch et al., "Polygon Mesh Processing", A K Peters
//! 2010, ch. 1) with positive enclosed volume (signed tetrahedra by the
//! divergence theorem - Zhang & Chen, ICIP 2001). Corners weld by spatial
//! hashing (Teschner et al., VMV 2003). Our own code, no vendor material.
//!
//! Prime Directive: the fallback only ever turns a refusal into the exact
//! union, never a guess. Any facet that does not fit - a collapsed one, an
//! edge used three or more times (two solids welded along an edge or face
//! share it, so they cannot be told apart and stay one non-manifold body),
//! an open or mixed-winding body, an inward (cavity) shell whose own section
//! would read as material - and the caller keeps its non-manifold refusal.

use std::collections::HashMap;

use crate::geo2d::Region;
use crate::geometry::Triangle;

use super::slice::{SliceError, material_above, material_overhead, shadow_above};

/// Corners closer than this (mm) are one vertex: the importers' seam drift
/// (nm) welds, nothing a cutter could resolve is merged.
const WELD_TOL_MM: f64 = 1e-4;

type Cell = (i64, i64, i64);

#[derive(Default)]
struct Welder {
    cells: HashMap<Cell, Vec<u32>>,
    pts: Vec<[f64; 3]>,
}

impl Welder {
    /// The id of `p`; `None` for a coordinate no grid cell can hold.
    fn id(&mut self, p: [f64; 3]) -> Option<u32> {
        let cell = |v: f64| {
            let q = (v / WELD_TOL_MM).floor();
            (q.is_finite() && (-9.0e18..=9.0e18).contains(&q)).then_some(q as i64)
        };
        let c = (cell(p[0])?, cell(p[1])?, cell(p[2])?);
        let tol2 = WELD_TOL_MM * WELD_TOL_MM;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let k = (c.0.saturating_add(dx), c.1.saturating_add(dy), c.2.saturating_add(dz));
                    let Some(ids) = self.cells.get(&k) else { continue };
                    for &id in ids {
                        let Some(q) = self.pts.get(id as usize) else { continue };
                        let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= tol2 {
                            return Some(id);
                        }
                    }
                }
            }
        }
        let id = u32::try_from(self.pts.len()).ok()?;
        self.pts.push(p);
        self.cells.entry(c).or_default().push(id);
        Some(id)
    }
}

/// Union-find root with path halving; indices are always in range.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = parent.get(i) {
        if p == i {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = gp;
        }
        i = gp;
    }
    i
}

/// The facets of `facets` grouped into closed outward solids, or `None`
/// when any facet does not belong to one (see the module's list).
pub(super) fn closed_solids(facets: &[Triangle]) -> Option<Vec<Vec<Triangle>>> {
    let mut welder = Welder::default();
    let mut corners = Vec::with_capacity(facets.len());
    for t in facets {
        let [a, b, c] = [t.a, t.b, t.c].map(|p| welder.id([p.x, p.y, p.z]));
        // A collapsed facet's degenerate side fails the closed test below.
        corners.push([a?, b?, c?]);
    }
    // Directed edge use: (lo, hi) -> (lo->hi count, hi->lo count, a facet).
    let mut edges: HashMap<(u32, u32), (u32, u32, usize)> = HashMap::new();
    let mut parent: Vec<usize> = (0..corners.len()).collect();
    for (i, [a, b, c]) in corners.iter().enumerate() {
        for (p, q) in [(*a, *b), (*b, *c), (*c, *a)] {
            let key = if p < q { (p, q) } else { (q, p) };
            let e = edges.entry(key).or_insert((0, 0, i));
            if p < q {
                e.0 = e.0.saturating_add(1);
            } else {
                e.1 = e.1.saturating_add(1);
            }
            let (ra, rb) = (root(&mut parent, e.2), root(&mut parent, i));
            if ra != rb
                && let Some(slot) = parent.get_mut(ra)
            {
                *slot = rb;
            }
        }
    }
    // Closed and consistently wound: every edge once each way.
    if edges.values().any(|&(f, b, _)| f != 1 || b != 1) {
        return None;
    }
    let mut body_of_root: HashMap<usize, usize> = HashMap::new();
    let mut bodies: Vec<Vec<Triangle>> = Vec::new();
    for (i, t) in facets.iter().enumerate() {
        let r = root(&mut parent, i);
        let next = body_of_root.len();
        let b = *body_of_root.entry(r).or_insert(next);
        if b == bodies.len() {
            bodies.push(Vec::new());
        }
        bodies.get_mut(b)?.push(*t);
    }
    if !bodies.iter().all(|b| outward(b)) {
        return None;
    }
    Some(bodies)
}

/// A closed body's signed volume is positive: its winding is its
/// outside (an inward cavity shell reads negative).
fn outward(body: &[Triangle]) -> bool {
    let Some(o) = body.first().map(|t| t.a) else { return false };
    let vol: f64 = body.iter().map(|t| (t.a - o).dot((t.b - o).cross(t.c - o)) / 6.0).sum();
    vol.is_finite() && vol > 0.0
}

/// The section at `z` of the union of `facets`' closed solids, each read
/// by `plain` (the slicer without this fallback). `None` when the soup is
/// not such a union - the caller's refusal stands. A solid that will not
/// slice on its own refuses with its own error.
pub(super) fn union_section(
    facets: &[Triangle],
    z: f64,
    plain: impl Fn(&[Triangle], f64) -> Result<Region, SliceError>,
) -> Option<Result<Region, SliceError>> {
    let bodies = closed_solids(facets)?;
    let mut acc: Option<Region> = None;
    for b in &bodies {
        let r = match plain(b, z) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        if r.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => r,
            Some(a) => match a.union(&r) {
                Ok(u) => u,
                Err(e) => return Some(Err(e.into())),
            },
        });
    }
    Some(Ok(acc.unwrap_or_default()))
}

/// [`material_above`] of several bodies that TOUCH, read as the union
/// they stand for (M13.11): `bodies` are index ranges of `facets`, one
/// per closed solid. Two bodies butted face to face put two coincident
/// walls of opposite normal into one soup, which the chord graph of
/// [`slice`](super::slice::slice) meets as a non-manifold contact and refuses
/// when the bodies' walls weld into one shell; sliced each on
/// its own they are two clean sections, and the 2D boolean union of
/// those sections is the section of the union solid - the same law the
/// region builders apply to picks across entities. Fewer than two
/// bodies is the plain call, byte for byte; a body that will not slice
/// on its own (a self-intersecting shell) still refuses with its own
/// error - the seam read never widens what counts as manifold. A range
/// outside `facets` is an input error, never a silent partial read.
pub fn material_above_of(
    facets: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    z: f64,
) -> Result<Region, SliceError> {
    if bodies.len() < 2 {
        return material_above(facets, z);
    }
    let mut acc: Option<Region> = None;
    for b in bodies {
        let Some(part) = facets.get(b.clone()) else {
            return Err(SliceError::Invalid(format!(
                "body range {}..{} is outside the {} facets",
                b.start,
                b.end,
                facets.len()
            )));
        };
        let r = material_above(part, z)?;
        if r.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => r,
            Some(a) => a.union(&r)?,
        });
    }
    Ok(acc.unwrap_or_default())
}

/// [`material_overhead`] over touching bodies ([`material_above_of`]'s
/// section united with the shadow of the whole soup - the shadow is a
/// projection of down-facing faces, which a vertical seam never has).
pub fn material_overhead_of(
    facets: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    z: f64,
) -> Result<Region, SliceError> {
    if bodies.len() < 2 {
        return material_overhead(facets, z);
    }
    let cut = material_above_of(facets, bodies, z)?;
    let hang = shadow_above(facets, z)?;
    if hang.is_empty() {
        return Ok(cut);
    }
    Ok(cut.union(&hang)?)
}

//! A fixture's real triangle mesh as collision truth (#339): the
//! narrow phase behind the world-AABB fixture laws. A vise is two jaws
//! with a gap; its box is one block - every path through the gap was a
//! false crash. This module answers, for one tool band - upright, or
//! posed along any axis (slice 3) - swept along one tip segment, "does
//! it come within a margin of the steel, or stand inside it?" against
//! the mesh itself.
//!
//! Provenance: our own design on published methods - GJK separation
//! ([`gjk`]), an axis-aligned BVH ([`bvh`]), ray-crossing point-in-solid
//! ([`ray`]); a closed mesh as a 2-cycle (every edge used equally often
//! in both directions) is the boundary condition of simplicial homology
//! as stated in Botsch et al., "Polygon Mesh Processing", 2010, ch. 1.
//! No vendor geometry, model, or implementation was inspected.
//!
//! ## The laws here
//!
//! - **Only a closed mesh judges.** Point-in-solid needs a solid: a
//!   mesh whose edges do not pair up (an open sheet, a crack, a
//!   T-junction) refuses by name ([`MeshError::Open`]) and its fixture
//!   keeps the box verdict. Vertices pair by exact bits, the importers'
//!   own sharing; `RepairModel` welds a cracked one.
//! - **Hit = not proven clear.** A band is clear of the mesh only when
//!   GJK proves a separating plane with more than the margin to spare
//!   for every triangle near it, AND the band does not stand inside the
//!   solid. Every unproven case is a hit.
//! - **Degenerate input is named, never fatal.** No facets, non-finite
//!   coordinates, all facets collapsed, or past [`MAX_MESH_TRIS`]: a
//!   [`MeshError`], and the caller falls back to the box. Zero-area
//!   facets inside a good mesh still pair edges and still meet tools;
//!   only the ray test skips them (they have no interior to cross).

mod bvh;
mod column;
mod gjk;
mod prism;
mod ray;

pub use prism::PrismReading;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::fixture::FixtureBox;
use crate::geometry::{Point3, Triangle};

use gjk::{Boxed, SweptBand, Tri, V};

/// The most facets one fixture mesh is judged with; past it the box
/// verdict stands (memory: ~200 bytes a facet with its BVH share).
pub const MAX_MESH_TRIS: usize = 4_000_000;

/// Why a fixture's mesh cannot judge. Each refusal names itself; the
/// fixture then answers with its world box, as before #339.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshError {
    Empty,
    NonFinite,
    Degenerate,
    TooLarge,
    /// Edges that do not pair up: no inside to speak of.
    Open,
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "the fixture mesh has no facets",
            Self::NonFinite => "the fixture mesh has non-finite coordinates",
            Self::Degenerate => "every facet of the fixture mesh is collapsed",
            Self::TooLarge => "the fixture mesh has more than 4,000,000 facets",
            Self::Open => "the fixture mesh is not closed (open edges, cracks or T-junctions - Repair Model may close it)",
        })
    }
}

impl std::error::Error for MeshError {}

/// A validated, closed fixture mesh with its BVH.
#[derive(Clone, Debug)]
pub struct FixtureMesh {
    tris: Vec<[V; 3]>,
    /// Zero-area facets (skipped by the ray test only).
    flat: Vec<bool>,
    bvh: bvh::Bvh,
    /// Float-dust pad at this mesh's coordinate scale.
    pad: f64,
}

fn v(p: Point3) -> V {
    [p.x, p.y, p.z]
}

/// Exact-bits vertex key; -0.0 and 0.0 are one position.
fn key(p: V) -> [u64; 3] {
    p.map(|c| if c == 0.0 { 0u64 } else { c.to_bits() })
}

impl FixtureMesh {
    /// Validate and index `tris` (world frame). Refuses by name; see
    /// [`MeshError`].
    pub fn build(tris: &[Triangle]) -> Result<Self, MeshError> {
        if tris.is_empty() {
            return Err(MeshError::Empty);
        }
        if tris.len() > MAX_MESH_TRIS {
            return Err(MeshError::TooLarge);
        }
        if !tris.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            return Err(MeshError::NonFinite);
        }
        let mut ids: HashMap<[u64; 3], u32> = HashMap::with_capacity(tris.len());
        let mut id = |p: V| {
            let next = u32::try_from(ids.len()).unwrap_or(u32::MAX);
            *ids.entry(key(p)).or_insert(next)
        };
        // Signed use count per undirected edge: +1 low->high, -1 back.
        let mut edges: HashMap<(u32, u32), i32> = HashMap::with_capacity(tris.len() * 2);
        let mut kept: Vec<[V; 3]> = Vec::with_capacity(tris.len());
        let mut scale = 1.0f64;
        for t in tris {
            let c = [v(t.a), v(t.b), v(t.c)];
            let (ia, ib, ic) = (id(c[0]), id(c[1]), id(c[2]));
            if ia == ib || ib == ic || ia == ic {
                continue; // collapsed: no edges, no steel
            }
            for (p, q) in [(ia, ib), (ib, ic), (ic, ia)] {
                let (k, s) = if p < q { ((p, q), 1) } else { ((q, p), -1) };
                *edges.entry(k).or_insert(0) += s;
            }
            for p in &c {
                scale = scale.max(p[0].abs()).max(p[1].abs()).max(p[2].abs());
            }
            kept.push(c);
        }
        if kept.is_empty() {
            return Err(MeshError::Degenerate);
        }
        if edges.values().any(|&s| s != 0) {
            return Err(MeshError::Open);
        }
        let flat = kept
            .iter()
            .map(|[a, b, c]| {
                let n = gjk::cross(gjk::sub(*b, *a), gjk::sub(*c, *a));
                gjk::dot(n, n) <= 1e-24
            })
            .collect();
        let bvh = bvh::Bvh::build(&kept);
        Ok(Self { tris: kept, flat, bvh, pad: 1e-9 * scale })
    }

    /// Facets judged (collapsed ones dropped).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tris.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Does the upright band of radius `r`, standing `lo..hi` above the
    /// tip, swept as the tip runs `p0`->`p1`, come within `margin` of
    /// any facet? (Not inside-the-solid: see [`Self::contains`].)
    /// Non-finite input is a hit.
    #[must_use]
    pub fn band_meets(&self, p0: Point3, p1: Point3, lo: f64, hi: f64, r: f64, margin: f64) -> bool {
        self.band_meets_along(p0, p1, gjk::UP, lo, hi, r, margin)
    }

    /// [`Self::band_meets`] with the band standing along `axis` (tip
    /// toward shank; normalized here) instead of +Z: a posed band, its
    /// pose held along the segment (#339 slice 3). A zero or non-finite
    /// axis, like any non-finite input, is a hit.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // segment, axis, band, margin
    pub fn band_meets_along(
        &self,
        p0: Point3,
        p1: Point3,
        axis: [f64; 3],
        lo: f64,
        hi: f64,
        r: f64,
        margin: f64,
    ) -> bool {
        let Some(band) = swept_band(p0, p1, axis, lo, hi, r) else { return true };
        if !margin.is_finite() {
            return true;
        }
        let margin = margin.max(0.0) + self.pad;
        let (blo, bhi) = band.bounds();
        self.bvh.walk(
            |n| bvh::boxes_meet(n.lo, n.hi, blo, bhi, margin) && !gjk::separated(&band, &Boxed { lo: n.lo, hi: n.hi }, margin),
            |t| {
                let Some(tri) = self.tris.get(t) else { return false };
                let (tlo, thi) = bvh::tri_box(tri);
                bvh::boxes_meet(tlo, thi, blo, bhi, margin) && !gjk::separated(&band, &Tri(*tri), margin)
            },
        )
    }

    /// Is `p` inside the closed solid? Ambiguity reads as inside.
    #[must_use]
    pub fn contains(&self, p: Point3) -> bool {
        ray::inside(self, v(p))
    }
}

/// The swept band the GJK search reads: the cylinder of radius `r`
/// standing `lo..hi` along `axis` (normalized here) from the tip, the
/// tip carried `p0`->`p1`. None on a zero or non-finite axis or any
/// non-finite input - the caller fails closed.
fn swept_band(p0: Point3, p1: Point3, axis: [f64; 3], lo: f64, hi: f64, r: f64) -> Option<SweptBand> {
    if !(p0.is_finite() && p1.is_finite() && lo.is_finite() && hi.is_finite() && r.is_finite()) {
        return None;
    }
    let a = if axis == gjk::UP {
        gjk::UP
    } else {
        let l = gjk::dot(axis, axis).sqrt();
        if !(l.is_finite() && l > 1e-12) {
            return None;
        }
        gjk::scale(axis, 1.0 / l)
    };
    Some(SweptBand { p0: v(p0), p1: v(p1), a, lo: lo.min(hi), hi: hi.max(lo), r: r.max(0.0) })
}

/// Is the flat-ended band of [`FixtureMesh::band_meets_along`] PROVEN
/// more than `margin` clear of the box (#1305)? The box law's twin of
/// the mesh law's narrow phase: the same swept cylinder, the same GJK
/// proof ([`gjk`]'s header - a separating plane or nothing). Anything
/// else - contact, overlap, non-finite input, a zero axis, a search
/// that ran out - is `false`: not proven, so the caller keeps its hit.
#[must_use]
#[allow(clippy::too_many_arguments)] // segment, axis, band, box, margin
pub(crate) fn band_clear_of_box(
    p0: Point3,
    p1: Point3,
    axis: [f64; 3],
    lo: f64,
    hi: f64,
    r: f64,
    b: &FixtureBox,
    margin: f64,
) -> bool {
    let Some(band) = swept_band(p0, p1, axis, lo, hi, r) else { return false };
    if !(margin.is_finite() && b.min.is_finite() && b.max.is_finite()) {
        return false;
    }
    let (m, x) = (v(b.min), v(b.max));
    let boxed = Boxed { lo: [0, 1, 2].map(|k| m[k].min(x[k])), hi: [0, 1, 2].map(|k| m[k].max(x[k])) };
    gjk::separated(&band, &boxed, margin.max(0.0))
}

/// One fixture as the mesh law reads it: its world box (the very box
/// the box laws judge, so the two can be paired) and its world facets,
/// indexed on first use - a fixture no box law ever flags never pays
/// for its BVH.
#[derive(Debug)]
pub struct MeshedFixture {
    pub name: String,
    pub aabb: FixtureBox,
    tris: Arc<[Triangle]>,
    mesh: OnceLock<Result<FixtureMesh, MeshError>>,
}

impl MeshedFixture {
    #[must_use]
    pub fn new(name: impl Into<String>, aabb: FixtureBox, tris: Arc<[Triangle]>) -> Self {
        Self { name: name.into(), aabb, tris, mesh: OnceLock::new() }
    }

    /// The indexed mesh, built on first call.
    pub fn mesh(&self) -> Result<&FixtureMesh, MeshError> {
        self.mesh.get_or_init(|| FixtureMesh::build(&self.tris)).as_ref().map_err(|e| *e)
    }

    /// Why this fixture's mesh could not judge - only once something
    /// asked it to (None before that, and None when it judged).
    #[must_use]
    pub fn refusal(&self) -> Option<MeshError> {
        self.mesh.get().and_then(|r| r.as_ref().err().copied())
    }
}

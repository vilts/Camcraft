//! Ambient occlusion data of one body (#2834): one value for each vertex of
//! its triangle list, 1.0 in the open and lower in a corner. Calculated one
//! time for a facet list and cached by `shade_cache`; a frame reads it.
//! The values are rigid: they do not change with the pose of the body.
//!
//! Provenance: the obscurance model of Zhukov, Iones and Kronin, "An
//! ambient light illumination model" (Eurographics Rendering Workshop 1998):
//! the share of short rays over the surface that meet the body again. The
//! rays are marched through a surface voxel grid (Amanatides and Woo, "A
//! fast voxel traversal algorithm for ray tracing", Eurographics 1987, in
//! its fixed-step form). The values are baked on the vertices as in Landis,
//! "Production-ready global illumination" (SIGGRAPH 2002 course notes). No
//! vendor material.

use camcraft_app::geometry::Triangle;

/// Voxels along the longest side of the body.
const GRID: usize = 48;
/// The ray origin stands this many voxels above its face. More than the
/// diagonal of a voxel (1.74), so a flat or slanted face never meets itself.
const LIFT: f64 = 1.8;
/// A ray is this many voxels long, in steps of one voxel.
const REACH: usize = 6;
/// The most that a fully closed corner darkens the light.
const STRENGTH: f32 = 0.45;
/// The most samples along one triangle edge when the grid is filled.
const MAX_SPLIT: usize = 128;

/// The ray directions over a face, as (tangent u, tangent v, normal) parts:
/// six low rays, three steep rays and the normal.
const RAYS: [[f64; 3]; 10] = [
    [0.937, 0.0, 0.35],
    [0.468, 0.811, 0.35],
    [-0.468, 0.811, 0.35],
    [-0.937, 0.0, 0.35],
    [-0.468, -0.811, 0.35],
    [0.468, -0.811, 0.35],
    [0.0, 0.6, 0.8],
    [0.52, -0.3, 0.8],
    [-0.52, -0.3, 0.8],
    [0.0, 0.0, 1.0],
];

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add_scaled(a: V3, b: V3, k: f64) -> V3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// `a` with length 1, or `None` for a zero or non-finite vector.
fn unit(a: V3) -> Option<V3> {
    let l = len(a);
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

fn corners(t: &Triangle) -> [V3; 3] {
    [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]]
}

/// The surface voxels of a body: a cell is set when a triangle goes through it.
struct Grid {
    min: V3,
    cell: f64,
    dims: [usize; 3],
    set: Vec<bool>,
}

impl Grid {
    /// The grid over the finite vertices of `tris`; `None` for no extent.
    fn over(tris: &[Triangle]) -> Option<Grid> {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in tris.iter().flat_map(corners) {
            if p.iter().all(|v| v.is_finite()) {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        let long = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max);
        if !long.is_finite() || long <= 1e-9 {
            return None;
        }
        let cell = long / GRID as f64;
        let dims = std::array::from_fn(|k| (((hi[k] - lo[k]) / cell).floor() as usize + 1).clamp(1, GRID + 1));
        Some(Grid { min: lo, cell, dims, set: vec![false; dims[0] * dims[1] * dims[2]] })
    }

    /// The index of the cell that holds `p`; `None` outside the grid.
    fn index(&self, p: V3) -> Option<usize> {
        let mut at = [0usize; 3];
        for k in 0..3 {
            let f = ((p[k] - self.min[k]) / self.cell).floor();
            if !(f >= 0.0 && f < self.dims[k] as f64) {
                return None;
            }
            at[k] = f as usize;
        }
        Some((at[2] * self.dims[1] + at[1]) * self.dims[0] + at[0])
    }

    /// Set the cells that `t` goes through: points on the triangle at half
    /// a cell or less from each other.
    fn fill(&mut self, t: [V3; 3]) {
        let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
        let long = len(e1).max(len(e2)).max(len(sub(t[2], t[1])));
        if !long.is_finite() {
            return;
        }
        let n = ((long / (self.cell * 0.5)).ceil() as usize).clamp(1, MAX_SPLIT);
        for i in 0..=n {
            for j in 0..=(n - i) {
                let p = add_scaled(add_scaled(t[0], e1, i as f64 / n as f64), e2, j as f64 / n as f64);
                if let Some(ix) = self.index(p)
                    && let Some(slot) = self.set.get_mut(ix)
                {
                    *slot = true;
                }
            }
        }
    }

    fn hit(&self, p: V3) -> bool {
        self.index(p).and_then(|ix| self.set.get(ix).copied()).unwrap_or(false)
    }

    /// Where the rays of a vertex start: `lifted` (the vertex, lifted off
    /// its face), or, when that point is in the cells of a wall beside the
    /// vertex, two cells away from the wall along the face. The result
    /// depends on the point and on the face plane only, so the triangles
    /// that share a vertex of a flat face read the same value there.
    /// `None` when each way along the face is closed.
    fn ray_origin(&self, lifted: V3, frame: &[V3; 3]) -> Option<V3> {
        if !self.hit(lifted) {
            return Some(lifted);
        }
        let (mut away, mut first) = ([0.0; 3], None);
        for r in RAYS.iter().take(6) {
            let along = unit(add_scaled(add_scaled([0.0; 3], frame[0], r[0]), frame[1], r[1]))?;
            if !self.hit(add_scaled(lifted, along, 2.0 * self.cell)) {
                away = add_scaled(away, along, 1.0);
                first.get_or_insert(along);
            }
        }
        let dir = unit(away).or(first)?;
        Some(add_scaled(lifted, dir, 2.0 * self.cell))
    }

    /// The share (0..=1) of the rays from `origin` that meet the body.
    fn closed_share(&self, origin: V3, frame: &[V3; 3]) -> f32 {
        let mut hits = 0u32;
        for r in &RAYS {
            let mut dir = [0.0; 3];
            for (axis, part) in frame.iter().zip(r) {
                dir = add_scaled(dir, *axis, *part);
            }
            if (0..=REACH).any(|s| self.hit(add_scaled(origin, dir, s as f64 * self.cell))) {
                hits += 1;
            }
        }
        hits as f32 / RAYS.len() as f32
    }
}

/// The occlusion value of each vertex of `tris`, in triangle order (a, b, c):
/// 1.0 is open, `1.0 - STRENGTH` is a fully closed corner. A body with no
/// extent, and each degenerate triangle, reads 1.0. Never fails.
#[must_use]
pub fn vertex_occlusion(tris: &[Triangle]) -> Vec<f32> {
    let mut out = vec![1.0f32; tris.len() * 3];
    let Some(mut grid) = Grid::over(tris) else {
        return out;
    };
    for t in tris {
        grid.fill(corners(t));
    }
    for (t, values) in tris.iter().zip(out.chunks_mut(3)) {
        let p = corners(t);
        let Some(n) = unit(cross(sub(p[1], p[0]), sub(p[2], p[0]))) else {
            continue;
        };
        // A tangent frame on the face: any axis that is not the normal.
        let seed = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let Some(u) = unit(cross(n, seed)) else { continue };
        let frame = [u, cross(n, u), n];
        for (corner, value) in p.iter().zip(values.iter_mut()) {
            let share = match grid.ray_origin(add_scaled(*corner, n, LIFT * grid.cell), &frame) {
                Some(origin) => grid.closed_share(origin, &frame),
                None => 1.0,
            };
            *value = 1.0 - STRENGTH * share;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use camcraft_app::geometry::{Point3, Triangle, cube};

    use super::{STRENGTH, vertex_occlusion};

    fn boxed(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
        cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.to_vec()
    }

    #[test]
    fn a_convex_body_is_open_at_each_vertex() {
        let ao = vertex_occlusion(&boxed([0.0; 3], [30.0, 20.0, 10.0]));
        assert_eq!(ao.len(), 36);
        assert!(ao.iter().all(|v| *v == 1.0), "a box has no corner that closes: {ao:?}");
    }

    #[test]
    fn the_floor_is_darker_at_the_wall_and_open_far_from_it() {
        // A slab with a tall wall on its +X end.
        let mut tris = boxed([0.0, 0.0, 0.0], [40.0, 40.0, 4.0]);
        tris.extend(boxed([36.0, 0.0, 4.0], [40.0, 40.0, 34.0]));
        let ao = vertex_occlusion(&tris);
        let mut at_wall = Vec::new();
        let mut far = Vec::new();
        for (t, values) in tris.iter().zip(ao.chunks(3)) {
            for (p, v) in [t.a, t.b, t.c].iter().zip(values) {
                let top_of_slab = (p.z - 4.0).abs() < 1e-9 && t.a.z == t.b.z && t.b.z == t.c.z;
                if top_of_slab && p.x == 0.0 {
                    far.push(*v);
                }
                // The wall face that looks along -X, at its foot.
                let wall_face = t.a.x == 36.0 && t.b.x == 36.0 && t.c.x == 36.0;
                if wall_face && (p.z - 4.0).abs() < 1e-9 {
                    at_wall.push(*v);
                }
            }
        }
        assert!(!far.is_empty() && !at_wall.is_empty());
        assert!(far.iter().all(|v| *v == 1.0), "the floor far from the wall is open: {far:?}");
        assert!(at_wall.iter().all(|v| *v < 0.95), "the foot of the wall is in a corner: {at_wall:?}");
        assert!(ao.iter().all(|v| (1.0 - STRENGTH - 1e-6..=1.0).contains(v)), "each value is in range");
    }

    #[test]
    fn degenerate_input_reads_open_and_does_not_fail() {
        assert!(vertex_occlusion(&[]).is_empty());
        let p = Point3::new(1.0, 2.0, 3.0);
        assert_eq!(vertex_occlusion(&[Triangle::new(p, p, p)]), [1.0; 3], "no extent");
        let nan = Point3::new(f64::NAN, 0.0, f64::INFINITY);
        let mut tris = boxed([0.0; 3], [10.0; 3]);
        tris.push(Triangle::new(nan, p, nan));
        let ao = vertex_occlusion(&tris);
        assert_eq!(ao.len(), tris.len() * 3);
        assert!(ao.iter().all(|v| v.is_finite()), "a bad triangle poisons nothing");
    }
}

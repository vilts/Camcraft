//! The ground shadow mesh (#2834): a soft shadow of the model bodies on the
//! floor plane. Calculated one time for a set of bodies and a floor height
//! and cached by `shade_cache`; a frame draws the cached mesh.
//!
//! The bodies are projected straight down on the floor, the footprint is
//! sampled on a coarse grid, the grid is blurred, and the cells that hold
//! shade become triangles with the shade on each vertex. The GPU blends
//! them; no texture is needed, so the native window and the browser draw
//! the same thing. The light is straight above the floor: the shadow then
//! shows as a soft rim around the bodies from each view direction. A light
//! at the key light of the face shader put the shadow behind the bodies in
//! the house view, where nobody sees it.
//!
//! Provenance: the planar projected shadow of Blinn, "Me and my (fake)
//! shadow" (IEEE Computer Graphics and Applications 8(1), 1988), with the
//! light at the zenith, made soft with a repeated box blur (Wells,
//! "Efficient synthesis of Gaussian filters by cascaded uniform filters",
//! IEEE PAMI 8(2), 1986). No vendor material.

/// The most grid nodes along one side of the shadow.
const GRID: usize = 96;
/// The radius of one box blur pass, in cells; two passes run.
const BLUR: usize = 3;
/// Clear cells around the footprint, so that the blur has room to fade.
const MARGIN: usize = 2 * BLUR + 1;
/// A node with less shade than this is not drawn.
const FAINT: f32 = 1.0 / 255.0;

/// One shadow vertex: a world position on the floor and its shade (0..=1).
pub type ShadowVertex = [f32; 4];

/// The sampled footprint: node (i, j) is at `min + (i, j) * cell`.
struct Footprint {
    min: [f32; 2],
    cell: f32,
    nx: usize,
    ny: usize,
    shade: Vec<f32>,
}

/// The point of the floor that `v` shades: the point straight below it.
/// `None` for a non-finite vertex.
fn project(v: [f32; 3]) -> Option<[f32; 2]> {
    v.iter().all(|c| c.is_finite()).then_some([v[0], v[1]])
}

impl Footprint {
    /// The grid over the projected `tris`; `None` when they have no extent.
    fn over(tris: &[[[f32; 2]; 3]]) -> Option<Footprint> {
        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for p in tris.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
        let long = w.max(h);
        if !long.is_finite() || long <= 1e-6 {
            return None;
        }
        let cell = long / (GRID - 2 * MARGIN - 1) as f32;
        let nodes = |side: f32| ((side / cell).ceil() as usize + 1 + 2 * MARGIN).clamp(2, GRID);
        let (nx, ny) = (nodes(w), nodes(h));
        let min = [lo[0] - MARGIN as f32 * cell, lo[1] - MARGIN as f32 * cell];
        Some(Footprint { min, cell, nx, ny, shade: vec![0.0; nx * ny] })
    }

    /// Set each node that is inside `t` (its edges count as inside).
    fn cover(&mut self, t: &[[f32; 2]; 3]) {
        let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
        if !area.is_finite() || area == 0.0 {
            return;
        }
        let span = |k: usize, n: usize| {
            let lo = t.iter().map(|p| p[k]).fold(f32::INFINITY, f32::min);
            let hi = t.iter().map(|p| p[k]).fold(f32::NEG_INFINITY, f32::max);
            // Clipped to the grid: a huge triangle scans the grid one time
            // at most (Prime Directive: no hang on a corrupt mesh).
            let first = ((lo - self.min[k]) / self.cell).ceil().clamp(0.0, n as f32) as usize;
            let last = ((hi - self.min[k]) / self.cell).floor().clamp(-1.0, n as f32 - 1.0) as isize;
            (first as isize, last)
        };
        let ((i0, i1), (j0, j1)) = (span(0, self.nx), span(1, self.ny));
        for j in j0..=j1 {
            for i in i0..=i1 {
                let p = [self.min[0] + i as f32 * self.cell, self.min[1] + j as f32 * self.cell];
                let side = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
                let (e0, e1, e2) = (side(t[0], t[1]) * area, side(t[1], t[2]) * area, side(t[2], t[0]) * area);
                if e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0
                    && let Some(slot) = self.shade.get_mut(j as usize * self.nx + i as usize)
                {
                    *slot = 1.0;
                }
            }
        }
    }

    /// One box blur pass of radius `BLUR` along X, then along Y.
    fn blur(&mut self) {
        let (nx, ny) = (self.nx, self.ny);
        let pass = |src: &[f32], along_x: bool| -> Vec<f32> {
            let mut out = vec![0.0f32; src.len()];
            let (len, lines) = if along_x { (nx, ny) } else { (ny, nx) };
            for line in 0..lines {
                let at = |k: usize| if along_x { line * nx + k } else { k * nx + line };
                for k in 0..len {
                    let (a, b) = (k.saturating_sub(BLUR), (k + BLUR).min(len - 1));
                    let sum: f32 = (a..=b).map(|m| src.get(at(m)).copied().unwrap_or(0.0)).sum();
                    if let Some(slot) = out.get_mut(at(k)) {
                        *slot = sum / (2 * BLUR + 1) as f32;
                    }
                }
            }
            out
        };
        let x = pass(&self.shade, true);
        self.shade = pass(&x, false);
    }
}

/// The shadow mesh of `bodies` (world-space triangle lists) on the floor at
/// height `floor`: a triangle list, three vertices for each triangle. Empty
/// when the bodies have no footprint. Never fails: a non-finite vertex
/// drops its triangle.
#[must_use]
pub fn mesh(bodies: &[&[[f32; 3]]], floor: f32) -> Vec<ShadowVertex> {
    if !floor.is_finite() {
        return Vec::new();
    }
    let tris: Vec<[[f32; 2]; 3]> = bodies
        .iter()
        .flat_map(|verts| verts.as_chunks::<3>().0)
        .filter_map(|t| Some([project(t[0])?, project(t[1])?, project(t[2])?]))
        .collect();
    let Some(mut grid) = Footprint::over(&tris) else {
        return Vec::new();
    };
    for t in &tris {
        grid.cover(t);
    }
    grid.blur();
    grid.blur();
    // A hair below the floor: the bottom face of a body that stands on the
    // floor is at the floor height, and the two must not fight for depth.
    let z = floor - grid.cell * 0.01;
    let node = |i: usize, j: usize| -> ShadowVertex {
        let shade = grid.shade.get(j * grid.nx + i).copied().unwrap_or(0.0);
        [grid.min[0] + i as f32 * grid.cell, grid.min[1] + j as f32 * grid.cell, z, shade.clamp(0.0, 1.0)]
    };
    let mut out = Vec::new();
    for j in 0..grid.ny.saturating_sub(1) {
        for i in 0..grid.nx.saturating_sub(1) {
            let q = [node(i, j), node(i + 1, j), node(i + 1, j + 1), node(i, j + 1)];
            if q.iter().any(|v| v[3] > FAINT) {
                out.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{GRID, mesh};

    /// A box as a triangle list (the winding is not read by the shadow).
    fn boxed(lo: [f32; 3], hi: [f32; 3]) -> Vec<[f32; 3]> {
        let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
        let quads = [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]];
        quads.iter().flat_map(|q| [c(q[0]), c(q[1]), c(q[2]), c(q[0]), c(q[2]), c(q[3])]).collect()
    }

    fn shade_at(mesh: &[[f32; 4]], x: f32, y: f32) -> f32 {
        // The shade of the nearest shadow vertex; 0 when none is near.
        let near = mesh.iter().map(|v| ((v[0] - x).hypot(v[1] - y), v[3])).fold((f32::INFINITY, 0.0), |a, b| if b.0 < a.0 { b } else { a });
        if near.0 < 0.3 { near.1 } else { 0.0 }
    }

    #[test]
    fn a_box_on_the_floor_shades_its_footprint_and_a_soft_rim_around_it() {
        let body = boxed([0.0, 0.0, 0.0], [20.0, 20.0, 20.0]);
        let m = mesh(&[&body], 0.0);
        assert!(!m.is_empty() && m.len().is_multiple_of(3), "a triangle list");
        assert!(m.len() <= GRID * GRID * 6, "the mesh has a fixed bound");
        assert!(m.iter().all(|v| v.iter().all(|c| c.is_finite()) && (0.0..=1.0).contains(&v[3])));
        assert!(m.iter().all(|v| v[2] < 0.0 && v[2] > -0.1), "the mesh lies a hair below the floor");
        assert!(shade_at(&m, 10.0, 10.0) > 0.95, "full shade below the body");
        // The rim fades with the distance from the footprint, on each side.
        for (near, far) in [([-0.5, 10.0], [-1.25, 10.0]), ([20.5, 10.0], [21.25, 10.0]), ([10.0, -0.5], [10.0, -1.25])] {
            let (a, b) = (shade_at(&m, near[0], near[1]), shade_at(&m, far[0], far[1]));
            assert!(a > b && b > 0.0 && a < 0.6, "the rim fades: {a} then {b} at {near:?}, {far:?}");
        }
        assert_eq!(shade_at(&m, 30.0, 10.0), 0.0, "no shade far from the body");
    }

    #[test]
    fn a_body_above_the_floor_casts_its_shadow_straight_down() {
        let body = boxed([0.0, 0.0, 30.0], [10.0, 10.0, 40.0]);
        let m = mesh(&[&body], 0.0);
        assert!(shade_at(&m, 5.0, 5.0) > 0.95, "the shadow is straight below the body");
        assert!(m.iter().all(|v| v[2] < 0.0 && v[2] > -0.1), "on the floor, not at the body");
        assert_eq!(shade_at(&m, 20.0, 5.0), 0.0);
    }

    #[test]
    fn degenerate_input_gives_no_mesh_and_does_not_fail() {
        assert!(mesh(&[], 0.0).is_empty());
        assert!(mesh(&[&[]], 0.0).is_empty());
        let point = [[1.0f32, 2.0, 3.0]; 3];
        assert!(mesh(&[&point], 0.0).is_empty(), "no extent");
        let body = boxed([0.0; 3], [10.0; 3]);
        assert!(mesh(&[&body], f32::NAN).is_empty(), "a bad floor");
        let mut bad = body.clone();
        bad.extend([[f32::NAN, 0.0, 0.0], [1.0e30, 1.0e30, 0.0], [f32::INFINITY, 0.0, 0.0]]);
        let m = mesh(&[&bad], 0.0);
        assert!(m.iter().all(|v| v.iter().all(|c| c.is_finite())), "a bad triangle poisons nothing");
        // A huge finite triangle cannot make the grid scan without end.
        let huge = [[-1.0e30f32, -1.0e30, 5.0], [1.0e30, -1.0e30, 5.0], [0.0, 1.0e30, 5.0]];
        let m = mesh(&[&body, &huge], 0.0);
        assert!(m.len() <= GRID * GRID * 6);
    }
}

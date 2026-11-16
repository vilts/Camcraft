//! 2D toolpath shape-mask verifier and PNG diff reporter (#1768).
//! Provenance: compares the expected 2D Region mask against the toolpath's swept disc
//! footprint on a fixed planar grid, checking overcut, leftover, progressive rim engagement,
//! symmetry and stock-extension laws, and writing a pure-Rust RGB difference PNG on failure.

use std::path::{Path, PathBuf};

use camcraft_motion::cl::{ArcMove, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::geometry::Point3;

use super::mask_grid::{CHORD_TOL, Grid, Z_EPS};
use super::png;

#[derive(Clone, Copy, Debug)]
pub enum CutDepth {
    AllCut,
    AtOrBelow(f64),
    NearZ { z: f64, tol: f64 },
}

impl CutDepth {
    fn clip(self, a: Point3, b: Point3) -> Option<(Point3, Point3)> {
        let (lo, hi) = match self {
            CutDepth::AllCut => return Some((a, b)),
            CutDepth::AtOrBelow(z) => (f64::NEG_INFINITY, z + Z_EPS),
            CutDepth::NearZ { z, tol } => (z - tol, z + tol),
        };
        let dz = b.z - a.z;
        if dz.abs() <= f64::EPSILON {
            return (a.z >= lo && a.z <= hi).then_some((a, b));
        }
        let t0 = (lo - a.z) / dz;
        let t1 = (hi - a.z) / dz;
        let start = t0.min(t1).max(0.0);
        let end = t0.max(t1).min(1.0);
        if start > end {
            return None;
        }
        let at = |t: f64| Point3::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y), a.z + t * dz);
        Some((at(start), at(end)))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Symmetry {
    MirrorX(f64),
    MirrorY(f64),
    Rotate180(f64, f64),
}

impl Symmetry {
    fn reflect(self, p: Point2) -> Point2 {
        match self {
            Symmetry::MirrorX(cx) => Point2::new(2.0 * cx - p.x, p.y),
            Symmetry::MirrorY(cy) => Point2::new(p.x, 2.0 * cy - p.y),
            Symmetry::Rotate180(cx, cy) => Point2::new(2.0 * cx - p.x, 2.0 * cy - p.y),
        }
    }
}

pub struct MaskSpec<'a> {
    pub name: &'a str,
    pub expected: &'a Region,
    pub path: &'a Toolpath,
    pub tool_diameter: f64,
    pub cell: f64,
    pub depth: CutDepth,
    pub diff_dir: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug)]
pub struct MaskMetrics {
    pub expected_cells: usize,
    pub swept_cells: usize,
    pub overcut_cells: usize,
    pub uncut_cells: usize,
    pub uncut_area: f64,
    pub interior_uncut_cells: usize,
    pub out_of_grid_moves: usize,
    pub max_cut_width: f64,
    pub worst_cut_point: Option<Point2>,
}

pub struct ShapeMask {
    name: String,
    diff_dir: PathBuf,
    grid: Grid,
    expected: Vec<bool>,
    expected_d1: Vec<bool>,
    expected_interior: Vec<bool>,
    swept: Vec<bool>,
    out_of_grid_moves: usize,
    first_out_point: Option<Point2>,
    max_cut_width: f64,
    worst_cut_point: Option<Point2>,
}

impl ShapeMask {
    pub fn evaluate(spec: &MaskSpec<'_>) -> Result<Self, String> {
        Self::evaluate_stock(spec, None)
    }

    /// Start with a separately drilled region cleared only down to `cleared_z`.
    /// This is initial stock, not a cut attributed to the path under test:
    /// overcut and leftover still use only that path's actual swept footprint.
    pub fn evaluate_with_precleared(
        spec: &MaskSpec<'_>,
        precleared: &Region,
        cleared_z: f64,
    ) -> Result<Self, String> {
        if !cleared_z.is_finite() {
            return Err("precleared depth must be finite".into());
        }
        Self::evaluate_stock(spec, Some((precleared, cleared_z)))
    }

    fn evaluate_stock(
        spec: &MaskSpec<'_>,
        precleared: Option<(&Region, f64)>,
    ) -> Result<Self, String> {
        if !spec.tool_diameter.is_finite() || spec.tool_diameter <= 0.0 {
            return Err("tool_diameter must be finite and positive".into());
        }
        let r = 0.5 * spec.tool_diameter;
        let pad = (r + 6.0 * spec.cell).max(spec.tool_diameter + 4.0 * spec.cell);
        let grid = Grid::for_region(spec.expected, spec.cell, pad)?;
        let expected = grid.rasterize_region(spec.expected);
        let expected_d1 = grid.dilate1(&expected);
        let expected_interior = grid.erode1(&expected);

        let mut swept = vec![false; grid.len()];
        let mut swept_z = vec![f64::INFINITY; grid.len()];
        if let Some((region, z)) = precleared {
            for (depth, cleared) in swept_z.iter_mut().zip(grid.rasterize_region(region)) {
                if cleared {
                    *depth = z;
                }
            }
        }
        let mut out_of_grid_moves = 0usize;
        let mut first_out_point = None;
        let mut max_cut_width = 0.0f64;
        let mut worst_cut_point = None;
        for mv in &spec.path.moves {
            if mv.kind == MoveKind::Rapid {
                continue;
            }
            let is_planar_feed =
                mv.kind == MoveKind::Feed && (mv.from.z - mv.to.z).abs() <= f64::EPSILON;
            for (pa, pb) in mv.segments3(CHORD_TOL) {
                let p2a = Point2::new(pa.x, pa.y);
                let p2b = Point2::new(pb.x, pb.y);
                if !grid.segment_in_bounds(p2a, p2b, r) {
                    out_of_grid_moves += 1;
                    if first_out_point.is_none() {
                        first_out_point = Some(if grid.cell_of(p2a).is_none() { p2a } else { p2b });
                    }
                }
                let clipped = spec.depth.clip(pa, pb);
                let len = (p2b.x - p2a.x).hypot(p2b.y - p2a.y);
                if is_planar_feed && len > 1e-9 {
                    let steps = (len / grid.cell).ceil().max(1.0) as usize;
                    let dir = (p2b.x - p2a.x, p2b.y - p2a.y);
                    let z = 0.5 * (pa.z + pb.z);
                    let mut prev = p2a;
                    for s in 1..=steps {
                        let t = s as f64 / steps as f64;
                        let cur = Point2::new(p2a.x + dir.0 * t, p2a.y + dir.1 * t);
                        let tan = move_tangent(mv.arc, cur, dir);
                        let w = grid.contact_width(cur, tan, z, r, &expected, &swept_z);
                        if w > max_cut_width {
                            max_cut_width = w;
                            worst_cut_point = Some(cur);
                        }
                        grid.sweep_capsule(
                            (prev, cur),
                            r,
                            (z, z),
                            clipped.is_some(),
                            &mut swept,
                            &mut swept_z,
                        );
                        prev = cur;
                    }
                } else {
                    // Record the lowest actual cutter depth at each covered cell, not
                    // the segment endpoint depth across its entire XY footprint.
                    grid.sweep_capsule(
                        (p2a, p2b),
                        r,
                        (pa.z, pb.z),
                        false,
                        &mut swept,
                        &mut swept_z,
                    );
                    if let Some((a, b)) = clipped {
                        grid.sweep_capsule(
                            (Point2::new(a.x, a.y), Point2::new(b.x, b.y)),
                            r,
                            (a.z, b.z),
                            true,
                            &mut swept,
                            &mut swept_z,
                        );
                    }
                }
            }
        }

        let diff_dir = spec.diff_dir.map_or_else(png::default_diff_dir, Path::to_path_buf);
        Ok(Self {
            name: spec.name.to_string(),
            diff_dir,
            grid,
            expected,
            expected_d1,
            expected_interior,
            swept,
            out_of_grid_moves,
            first_out_point,
            max_cut_width,
            worst_cut_point,
        })
    }

    pub fn metrics(&self) -> MaskMetrics {
        let cell_area = self.grid.cell * self.grid.cell;
        let expected_cells = self.expected.iter().filter(|&&b| b).count();
        let swept_cells = self.swept.iter().filter(|&&b| b).count();
        let overcut_cells = self
            .swept
            .iter()
            .zip(&self.expected_d1)
            .filter(|&(&s, &ed)| s && !ed)
            .count();
        let uncut_cells = self
            .expected
            .iter()
            .zip(&self.swept)
            .filter(|&(&e, &s)| e && !s)
            .count();
        let interior_uncut_cells = self
            .expected_interior
            .iter()
            .zip(&self.swept)
            .filter(|&(&ei, &s)| ei && !s)
            .count();
        MaskMetrics {
            expected_cells,
            swept_cells,
            overcut_cells,
            uncut_cells,
            uncut_area: uncut_cells as f64 * cell_area,
            interior_uncut_cells,
            out_of_grid_moves: self.out_of_grid_moves,
            max_cut_width: self.max_cut_width,
            worst_cut_point: self.worst_cut_point,
        }
    }

    pub fn assert_no_overcut(&self) {
        let m = self.metrics();
        if m.out_of_grid_moves > 0 || m.overcut_cells > 0 {
            self.fail_with_png(
                None,
                &format!(
                    "overcut: {} overcut cell(s), {} out-of-grid segment(s) (first at {:?})",
                    m.overcut_cells, m.out_of_grid_moves, self.first_out_point
                ),
            );
        }
    }

    pub fn assert_bounded_leftover(&self, max_uncut_area: f64, allowed: Option<&Region>) {
        let m = self.metrics();
        let allowed_d1 = allowed.map(|reg| self.grid.dilate1(&self.grid.rasterize_region(reg)));
        let mut disallowed = 0usize;
        let mut sample_at = None;
        for iy in 0..self.grid.h {
            for ix in 0..self.grid.w {
                let i = self.grid.idx(ix, iy);
                if self.expected_interior[i] && !self.swept[i] {
                    let permitted = allowed_d1.as_ref().is_some_and(|a| a[i]);
                    if !permitted {
                        disallowed += 1;
                        if sample_at.is_none() {
                            sample_at = Some(self.grid.center(ix, iy));
                        }
                    }
                }
            }
        }
        if m.uncut_area > max_uncut_area || disallowed > 0 {
            self.fail_with_png(
                None,
                &format!(
                    "leftover exceeded: uncut_area={:.2} mm2 (limit {max_uncut_area:.2}), disallowed interior uncut cells={disallowed} (first at {sample_at:?})",
                    m.uncut_area
                ),
            );
        }
    }

    pub fn assert_max_engagement(&self, max_stepover: f64) {
        let limit = max_stepover + 1.5 * self.grid.cell;
        if self.max_cut_width > limit {
            self.fail_with_png(
                self.worst_cut_point,
                &format!(
                    "engagement exceeded: max cut width {:.3} mm > limit {:.3} mm (stepover {max_stepover:.3} + 1.5*cell) at {:?}",
                    self.max_cut_width, limit, self.worst_cut_point
                ),
            );
        }
    }

    pub fn assert_symmetric(&self, sym: Symmetry) {
        let swept_d1 = self.grid.dilate1(&self.swept);
        let expected_sym_miss = count_symmetry_mismatch(&self.grid, &self.expected, &self.expected_d1, sym);
        if expected_sym_miss > 0 {
            self.fail_with_png(
                None,
                &format!("expected region is not symmetric under {sym:?}: {expected_sym_miss} asymmetric cell(s)"),
            );
        }
        let swept_sym_miss = count_symmetry_mismatch(&self.grid, &self.swept, &swept_d1, sym);
        if swept_sym_miss > 0 {
            self.fail_with_png(
                None,
                &format!("swept cut is not symmetric under {sym:?}: {swept_sym_miss} asymmetric cell(s)"),
            );
        }
    }

    pub fn assert_within_stock_extension(&self, stock: &Region, ext: f64) {
        let allowed_reg = if ext == 0.0 {
            stock.clone()
        } else {
            match stock.offset_sharp(ext) {
                Ok(r) => r,
                Err(e) => panic!("shape_mask '{}': failed to offset stock by {ext}: {e}", self.name),
            }
        };
        let allowed_d1 = self.grid.dilate1(&self.grid.rasterize_region(&allowed_reg));
        let outside = self
            .swept
            .iter()
            .zip(&allowed_d1)
            .filter(|&(&s, &a)| s && !a)
            .count();
        if self.out_of_grid_moves > 0 || outside > 0 {
            self.fail_with_png(
                None,
                &format!(
                    "exceeded stock + {ext:.2} mm extension: {outside} swept cell(s) outside, {} out-of-grid segment(s)",
                    self.out_of_grid_moves
                ),
            );
        }
    }

    fn fail_with_png(&self, highlight: Option<Point2>, msg: &str) -> ! {
        let cell = highlight.and_then(|pt| self.grid.cell_of(pt));
        match png::write_diff_mask(
            &self.diff_dir,
            &self.name,
            (self.grid.w, self.grid.h),
            &self.expected,
            &self.expected_d1,
            &self.swept,
            cell,
        ) {
            Ok(p) => panic!("shape_mask '{}': {msg}\ndiff mask PNG written to {}", self.name, p.display()),
            Err(e) => panic!("shape_mask '{}': {msg}\n(failed to write diff PNG: {e})", self.name),
        }
    }
}

fn count_symmetry_mismatch(grid: &Grid, src: &[bool], target_d1: &[bool], sym: Symmetry) -> usize {
    let mut miss = 0usize;
    for iy in 0..grid.h {
        for ix in 0..grid.w {
            let i = grid.idx(ix, iy);
            if !src[i] {
                continue;
            }
            let reflected = sym.reflect(grid.center(ix, iy));
            match grid.cell_of(reflected) {
                Some((rx, ry)) if target_d1[grid.idx(rx, ry)] => {}
                _ => miss += 1,
            }
        }
    }
    miss
}

fn move_tangent(arc: Option<ArcMove>, cur: Point2, chord: (f64, f64)) -> (f64, f64) {
    if let Some(a) = arc
        && a.plane.is_xy()
    {
        let rx = cur.x - a.center.x;
        let ry = cur.y - a.center.y;
        if rx * rx + ry * ry > 1e-12 {
            return if a.ccw { (-ry, rx) } else { (ry, -rx) };
        }
    }
    chord
}

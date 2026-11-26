//! Conservative raster tests (#2612): the over-approximation covers every
//! point of the region, a free cell lies fully inside the sheet, the bits
//! are deterministic, and each refusal names its reason.
//!
//! Gate map: covers camcraft_geometry::nesting::raster.

use camcraft_geometry::geo2d::{Loop, Point2, Region, SCALE};
use camcraft_geometry::nesting::raster::{
    BitRaster, DEFAULT_CELL_UNITS, RasterError, RasterGrid, cell_units_from_mm, rasterize_free,
    rasterize_over,
};

const MM: i64 = 1_000_000;

fn poly(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("test loop")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

fn region(loops: Vec<Loop>) -> Region {
    Region::new(loops).expect("test region")
}

fn rotated_rect(cx: f64, cy: f64, w: f64, h: f64, deg: f64) -> Loop {
    let (s, c) = deg.to_radians().sin_cos();
    let corners = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)];
    poly(&corners.map(|(x, y)| (cx + x * c - y * s, cy + x * s + y * c)))
}

fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Loop {
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    poly(&pts)
}

fn units(mm: f64) -> i128 {
    (mm * SCALE).round() as i128
}

/// Cell indices on one axis whose closed interval holds `v` (two on a line).
fn axis_cells(v: i128, origin: i64, cell: i64) -> Vec<i128> {
    let d = v - i128::from(origin);
    let c = i128::from(cell);
    let i = d.div_euclid(c);
    if d.rem_euclid(c) == 0 { vec![i - 1, i] } else { vec![i] }
}

fn point_in_marked_cell(r: &BitRaster, x: i128, y: i128) -> bool {
    let g = r.grid();
    let (ox, oy) = g.origin();
    axis_cells(x, ox, g.cell()).iter().any(|&i| {
        axis_cells(y, oy, g.cell()).iter().any(|&j| {
            i >= 0 && j >= 0 && r.get(i as u32, j as u32)
        })
    })
}

/// Over-approximation law on a dense sample: every vertex and every sample
/// point that the exact region contains lies in a marked cell.
fn assert_over_covers(reg: &Region, cell: i64, step_mm: f64) -> BitRaster {
    let grid = RasterGrid::covering(reg, cell).expect("grid");
    let r = rasterize_over(reg, &grid).expect("over raster");
    for l in reg.loops() {
        for p in l.points() {
            assert!(point_in_marked_cell(&r, units(p.x), units(p.y)), "vertex {p:?} not covered");
        }
    }
    let (lo, hi) = reg.bounds().expect("bounds");
    let mut checked = 0;
    let mut y = lo.y + step_mm * 0.37;
    while y <= hi.y {
        let mut x = lo.x + step_mm * 0.61;
        while x <= hi.x {
            let p = Point2::new(x, y);
            if reg.contains(p) {
                checked += 1;
                assert!(point_in_marked_cell(&r, units(x), units(y)), "point {p:?} not covered");
            }
            x += step_mm;
        }
        y += step_mm;
    }
    assert!(checked > 0, "sample found no point inside");
    r
}

/// Exact test: does the closed segment touch the closed axis box?
fn segment_touches_box(a: (i128, i128), b: (i128, i128), bx: (i128, i128, i128, i128)) -> bool {
    let (x0, y0, x1, y1) = bx;
    if a.0.max(b.0) < x0 || a.0.min(b.0) > x1 || a.1.max(b.1) < y0 || a.1.min(b.1) > y1 {
        return false;
    }
    let side = |p: (i128, i128)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
    let s = [side((x0, y0)), side((x1, y0)), side((x1, y1)), side((x0, y1))];
    !(s.iter().all(|&v| v > 0) || s.iter().all(|&v| v < 0))
}

/// Under-approximation law: each free cell has its corners and centre in
/// the region, and no region edge touches the closed cell.
fn assert_free_inside(reg: &Region, r: &BitRaster) -> u64 {
    let g = r.grid();
    let edges: Vec<((i128, i128), (i128, i128))> = reg
        .loops()
        .iter()
        .flat_map(|l| {
            let p = l.points();
            (0..p.len()).map(move |k| {
                let (a, b) = (p[k], p[(k + 1) % p.len()]);
                ((units(a.x), units(a.y)), (units(b.x), units(b.y)))
            })
        })
        .collect();
    let mut free = 0;
    for j in 0..g.height() {
        for i in 0..g.width() {
            if !r.get(i, j) {
                continue;
            }
            free += 1;
            let (x0, y0, x1, y1) = g.cell_rect(i, j).expect("cell");
            let mm = |v: i64| v as f64 / SCALE;
            let xc = (x0 as f64 + x1 as f64) / 2.0 / SCALE;
            let yc = (y0 as f64 + y1 as f64) / 2.0 / SCALE;
            for (x, y) in [(mm(x0), mm(y0)), (mm(x1), mm(y0)), (mm(x1), mm(y1)), (mm(x0), mm(y1)), (xc, yc)] {
                assert!(reg.contains(Point2::new(x, y)), "free cell ({i},{j}) has ({x},{y}) outside");
            }
            let bx = (i128::from(x0), i128::from(y0), i128::from(x1), i128::from(y1));
            for (a, b) in &edges {
                assert!(!segment_touches_box(*a, *b, bx), "edge crosses free cell ({i},{j})");
            }
        }
    }
    free
}

#[test]
fn raster_over_square_golden_and_cover() {
    let reg = region(vec![rect(0.0, 0.0, 10.0, 10.0)]);
    let grid = RasterGrid::new(-2 * MM, -2 * MM, MM, 14, 14).expect("grid");
    let over = rasterize_over(&reg, &grid).expect("over");
    // Cells -1..=10 touch the closed square (the lines x=0 and x=10 touch
    // two cells each): 12 x 12.
    assert_eq!(over.count_ones(), 144);
    let free = rasterize_free(&reg, &grid).expect("free");
    // Cells 1..=8 are clear of the boundary: 8 x 8.
    assert_eq!(free.count_ones(), 64);
    assert_free_inside(&reg, &free);
    assert_over_covers(&region(vec![rect(0.3, 0.7, 10.3, 9.9)]), MM, 0.13);
}

#[test]
fn raster_over_part_with_hole_covers_and_keeps_hole_open() {
    let reg = region(vec![rect(0.0, 0.0, 20.0, 20.0), rect(6.0, 6.0, 14.0, 14.0)]);
    let r = assert_over_covers(&reg, MM, 0.11);
    // Cell [9,10] x [9,10] is deep in the hole: no part material touches it.
    assert!(!r.get(9, 9), "hole cell marked");
    assert!(r.get(6, 6) && r.get(5, 9), "hole rim cells must be marked");
}

#[test]
fn raster_over_thin_sliver_covers() {
    let reg = region(vec![rect(5.4, 0.2, 5.45, 30.1)]);
    let r = assert_over_covers(&reg, MM, 0.007);
    assert_eq!(r.grid().width(), 1, "a 0.05 mm sliver inside one column");
    // Rotated sliver: crosses many cells diagonally.
    let reg = region(vec![rotated_rect(10.0, 10.0, 25.0, 0.03, 37.0)]);
    assert_over_covers(&reg, MM / 2, 0.01);
}

#[test]
fn raster_over_rotated_part_covers() {
    let reg = region(vec![rotated_rect(3.3, -7.1, 12.0, 5.0, 30.0), ngon(20.0, 4.0, 3.7, 17)]);
    assert_over_covers(&reg, MM, 0.09);
    assert_over_covers(&reg, 333_333, 0.05);
}

#[test]
fn raster_free_sheet_cells_inside_and_holes_not_free() {
    let sheet = region(vec![
        rect(0.0, 0.0, 100.0, 50.0),
        ngon(30.0, 25.0, 10.0, 24),
        rotated_rect(75.0, 20.0, 15.0, 6.0, 20.0),
    ]);
    // A grid larger than the sheet: cells outside are never free.
    let grid = RasterGrid::new(-5 * MM, -5 * MM, MM, 112, 62).expect("grid");
    let free = rasterize_free(&sheet, &grid).expect("free");
    let n = assert_free_inside(&sheet, &free);
    assert!(n > 3500 && n < 4704, "free cells {n}");
    // Cell holding the hole centre (30, 25) is not free.
    assert!(!free.get(35, 30), "hole centre cell must not be free");
    // Cell [50,51] x [40,41] is plain sheet: free.
    assert!(free.get(55, 45));
    // Fine cell size not on the mm lattice.
    let grid = RasterGrid::new(0, 0, 777_777, 129, 65).expect("grid");
    assert_free_inside(&sheet, &rasterize_free(&sheet, &grid).expect("free"));
}

#[test]
fn raster_same_input_same_bits() {
    let reg = region(vec![rotated_rect(3.3, -7.1, 12.0, 5.0, 30.0), ngon(20.0, 4.0, 3.7, 17)]);
    let grid = RasterGrid::covering(&reg, 250_000).expect("grid");
    let a = rasterize_over(&reg, &grid).expect("a");
    let b = rasterize_over(&reg.clone(), &grid).expect("b");
    assert_eq!(a.words(), b.words());
    assert_eq!(rasterize_free(&reg, &grid), rasterize_free(&reg, &grid));
}

#[test]
fn raster_packing_rows_and_padding() {
    let reg = region(vec![rect(0.0, 0.0, 40.0, 3.0)]);
    let grid = RasterGrid::new(-MM, -MM, MM, 45, 5).expect("grid");
    assert_eq!(grid.words_per_row(), 2);
    let r = rasterize_over(&reg, &grid).expect("over");
    assert_eq!(r.words().len(), 10);
    for row in r.words().chunks(2) {
        assert_eq!(row[1] >> 13, 0, "padding bits past width 45 must be zero");
    }
    // Row 1 (y in [0,1]): cells 0..=41 are set (x from -1 to 41 mm).
    assert_eq!(r.words()[2], u32::MAX);
    assert_eq!(r.words()[3], (1 << 10) - 1);
}

#[test]
fn raster_refusals_name_their_reason() {
    let reg = region(vec![rect(0.0, 0.0, 10.0, 10.0)]);
    assert_eq!(RasterGrid::new(0, 0, 0, 4, 4), Err(RasterError::NonPositiveCellSize(0)));
    assert_eq!(RasterGrid::covering(&reg, -5), Err(RasterError::NonPositiveCellSize(-5)));
    assert_eq!(RasterGrid::new(0, 0, MM, 0, 4), Err(RasterError::EmptyGrid));
    assert!(matches!(RasterGrid::new(0, 0, 1, 5000, 5000), Err(RasterError::GridTooLarge { .. })));
    assert!(matches!(RasterGrid::new(0, 0, 1, 70_000, 1), Err(RasterError::GridTooLarge { .. })));
    assert!(matches!(RasterGrid::covering(&reg, 1), Err(RasterError::GridTooLarge { .. })));
    assert_eq!(RasterGrid::new(i64::MAX - 10, 0, MM, 4, 4), Err(RasterError::OutOfRange));
    assert_eq!(RasterGrid::covering(&Region::empty(), MM), Err(RasterError::EmptyRegion));
    assert_eq!(cell_units_from_mm(f64::NAN), Err(RasterError::NonFiniteCellSize));
    assert_eq!(cell_units_from_mm(f64::INFINITY), Err(RasterError::NonFiniteCellSize));
    assert_eq!(cell_units_from_mm(-1.0), Err(RasterError::NonPositiveCellSize(-1_000_000)));
    assert_eq!(cell_units_from_mm(1e-9), Err(RasterError::NonPositiveCellSize(0)));
    assert_eq!(cell_units_from_mm(1e13), Err(RasterError::OutOfRange));
    assert_eq!(cell_units_from_mm(1.0), Ok(DEFAULT_CELL_UNITS));
    // A part that does not fit is refused, never clipped.
    let small = RasterGrid::new(0, 0, MM, 5, 5).expect("grid");
    assert_eq!(rasterize_over(&reg, &small), Err(RasterError::RegionExceedsGrid));
    for e in [RasterError::RegionExceedsGrid, RasterError::EmptyRegion, RasterError::NonFinite] {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn raster_degenerate_input_is_clean() {
    // Region outside the grid: the sheet raster has no free cell.
    let reg = region(vec![rect(500.0, 500.0, 510.0, 510.0)]);
    let grid = RasterGrid::new(0, 0, MM, 20, 20).expect("grid");
    assert_eq!(rasterize_free(&reg, &grid).expect("free").count_ones(), 0);
    assert_eq!(rasterize_over(&reg, &grid), Err(RasterError::RegionExceedsGrid));
    // Empty region: all-zero rasters.
    assert_eq!(rasterize_over(&Region::empty(), &grid).expect("over").count_ones(), 0);
    assert_eq!(rasterize_free(&Region::empty(), &grid).expect("free").count_ones(), 0);
    // Zero-area sliver: a collinear loop is refused or normalized away.
    let line = vec![Point2::new(0.0, 0.0), Point2::new(5.0, 5.0), Point2::new(9.0, 9.0)];
    if let Ok(l) = Loop::new(line)
        && let Ok(z) = Region::new(vec![l])
    {
        assert!(z.is_empty(), "a zero-area loop normalizes to an empty region");
        assert_eq!(rasterize_over(&z, &grid).expect("over").count_ones(), 0);
        assert_eq!(RasterGrid::covering(&z, MM), Err(RasterError::EmptyRegion));
    }
    // NaN never reaches a region.
    assert!(Loop::new(vec![Point2::new(f64::NAN, 0.0), Point2::new(1.0, 0.0), Point2::new(0.0, 1.0)]).is_err());
    // Huge coordinates: a 1 mm part far out still rasterizes on its own grid.
    let far = region(vec![rect(9e11, -9e11, 9e11 + 1.0, -9e11 + 1.0)]);
    let g = RasterGrid::covering(&far, MM).expect("far grid");
    assert!(rasterize_over(&far, &g).expect("far over").count_ones() >= 1);
    // Huge part at a fine cell: refused by the limit, not a hang.
    let big = region(vec![rect(-9e11, -9e11, 9e11, 9e11)]);
    assert!(matches!(RasterGrid::covering(&big, MM), Err(RasterError::GridTooLarge { .. })));
    let edge_grid = RasterGrid::new(0, 0, MM, 30, 30).expect("grid");
    assert_eq!(rasterize_free(&big, &edge_grid).expect("free").count_ones(), 900);
}

#[test]
fn raster_reading_typical_sizes() {
    let part = region(vec![ngon(0.0, 0.0, 50.0, 256), ngon(0.0, 0.0, 20.0, 64)]);
    let t = std::time::Instant::now();
    let g = RasterGrid::covering(&part, MM / 10).expect("grid");
    let r = rasterize_over(&part, &g).expect("over");
    let part_ms = t.elapsed().as_secs_f64() * 1e3;
    let sheet = region(vec![rect(0.0, 0.0, 3000.0, 1500.0)]);
    let t = std::time::Instant::now();
    let sg = RasterGrid::covering(&sheet, DEFAULT_CELL_UNITS).expect("sheet grid");
    let s = rasterize_free(&sheet, &sg).expect("free");
    let sheet_ms = t.elapsed().as_secs_f64() * 1e3;
    eprintln!(
        "reading: part {}x{} cells, {} set, {part_ms:.1} ms; sheet {}x{} cells, {} free, {sheet_ms:.1} ms",
        g.width(), g.height(), r.count_ones(), sg.width(), sg.height(), s.count_ones()
    );
    assert_eq!(s.count_ones(), 2998 * 1498);
}

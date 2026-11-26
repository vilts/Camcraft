//! Parity and superset laws of the support facet grid (#2619): the grid scan
//! must answer the full scan's bits on every query. Our own tests.

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

use super::edge::fold_best;
use super::grid::SupSet;
use super::support::{
    ball_clear, build_sups, column_clear, plunge_support, support_at, support_grad, Sup,
};

/// A small deterministic generator (64-bit LCG, Knuth's MMIX constants).
struct Lcg(u64);
impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A bumpy height field on integer XY nodes (vertices on cell borders when
/// the cell is 1), split into two triangles per square: convex and concave
/// folds, slopes and near-flats.
fn bumps(n: usize) -> Vec<Triangle> {
    let z = |i: usize, j: usize| {
        let (x, y) = (i as f64, j as f64);
        (x * 0.7).sin() * 1.3 + (y * 0.45).cos() * 0.9 + 0.1 * x
    };
    let mut out = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b) = (p(i as f64, j as f64, z(i, j)), p(i as f64 + 1.0, j as f64, z(i + 1, j)));
            let (c, d) = (p(i as f64 + 1.0, j as f64 + 1.0, z(i + 1, j + 1)), p(i as f64, j as f64 + 1.0, z(i, j + 1)));
            out.push(Triangle::new(a, b, c));
            out.push(Triangle::new(a, c, d));
        }
    }
    out
}

/// Big facets (larger than many cells), a raised plateau, a steep wall and
/// degenerate facets: zero-area slivers, a repeated vertex, a collinear one.
fn mixed() -> Vec<Triangle> {
    let mut out = vec![
        Triangle::new(p(-20.0, -20.0, 0.0), p(20.0, -20.0, 0.0), p(20.0, 20.0, 0.0)),
        Triangle::new(p(-20.0, -20.0, 0.0), p(20.0, 20.0, 0.0), p(-20.0, 20.0, 0.0)),
        Triangle::new(p(-3.0, -3.0, 4.0), p(3.0, -3.0, 4.0), p(3.0, 3.0, 4.5)),
        Triangle::new(p(-3.0, -3.0, 4.0), p(3.0, 3.0, 4.5), p(-3.0, 3.0, 4.0)),
        Triangle::new(p(5.0, -4.0, 0.0), p(5.0, 4.0, 0.0), p(5.0, 0.0, 6.0)),
        Triangle::new(p(1.0, 1.0, 2.0), p(1.0, 1.0, 2.0), p(1.0, 1.0, 2.0)),
        Triangle::new(p(-2.0, 0.0, 1.0), p(0.0, 0.0, 1.0), p(2.0, 0.0, 1.0)),
        Triangle::new(p(8.0, 8.0, 3.0), p(8.0, 8.0, 3.0), p(9.0, 9.0, 3.5)),
    ];
    out.extend(bumps(6).into_iter().map(|t| Triangle::new(
        p(t.a.x - 12.0, t.a.y + 6.0, t.a.z + 2.0),
        p(t.b.x - 12.0, t.b.y + 6.0, t.b.z + 2.0),
        p(t.c.x - 12.0, t.c.y + 6.0, t.c.z + 2.0),
    )));
    out
}

fn copy(s: &SupSet) -> Vec<Sup> {
    s.iter().map(|x| Sup { ..*x }).collect()
}

fn bits(v: Option<f64>) -> Option<u64> {
    v.map(f64::to_bits)
}

fn bits2(v: Option<(f64, f64)>) -> Option<(u64, u64)> {
    v.map(|(a, b)| (a.to_bits(), b.to_bits()))
}

/// Query points: random, on integer cell borders, and on padded box edges.
fn queries(sups: &SupSet, rng: &mut Lcg, k: usize) -> Vec<(f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for s in sups.iter() {
        x0 = x0.min(s.xmin);
        y0 = y0.min(s.ymin);
        x1 = x1.max(s.xmax);
        y1 = y1.max(s.ymax);
    }
    let mut q = Vec::new();
    for _ in 0..k {
        q.push((rng.range(x0 - 1.0, x1 + 1.0), rng.range(y0 - 1.0, y1 + 1.0)));
        q.push((rng.range(x0, x1).round(), rng.range(y0, y1).round()));
    }
    for (i, s) in sups.iter().enumerate().step_by(3) {
        let y = rng.range(s.ymin, s.ymax);
        q.push((s.xmin, y));
        q.push((s.xmax, y));
        q.push((rng.range(s.xmin, s.xmax), if i % 2 == 0 { s.ymin } else { s.ymax }));
        q.push((s.xmax, s.ymax));
    }
    q
}

/// Every answer of the grid scan equals the full scan's, bit for bit.
fn assert_parity(grid: &SupSet, full: &SupSet, r: f64, rng: &mut Lcg, k: usize) {
    let floor = -1.0e6;
    for (x, y) in queries(full, rng, k) {
        let at = |s: &SupSet| support_at(x, y, r, floor, s);
        assert_eq!(bits(at(grid)), bits(at(full)), "support_at at ({x}, {y}) r {r}");
        let u = Point2::new(0.6, 0.8);
        let g = |s: &SupSet| support_grad(Point2::new(x, y), u, r, floor, s);
        assert_eq!(bits2(g(grid)), bits2(g(full)), "support_grad at ({x}, {y})");
        let f = |s: &SupSet| fold_best(s, x, y, r, floor, (1.0, 0.0), None);
        assert_eq!(bits2(f(grid)), bits2(f(full)), "fold_best at ({x}, {y})");
        let top = 50.0;
        let pl = |s: &SupSet| plunge_support(x, y, r, floor, top, s);
        let (a, b) = (pl(grid), pl(full));
        assert_eq!((bits(a.0), a.1), (bits(b.0), b.1), "plunge_support at ({x}, {y})");
        for cz in [rng.range(-2.0, 8.0), at(full).unwrap_or(1.0), 0.0] {
            let c = p(x, y, cz);
            assert_eq!(ball_clear(c, grid, r), ball_clear(c, full, r), "ball_clear at {c:?}");
            let col = |s: &SupSet| column_clear(x, y, cz, top, r * 0.999, s);
            assert_eq!(col(grid), col(full), "column_clear at ({x}, {y}, {cz})");
        }
    }
}

fn twins(facets: &[Triangle], r: f64, cell: Option<f64>) -> (SupSet, SupSet) {
    let auto = build_sups(facets, r);
    let full = SupSet::full_scan(copy(&auto));
    let grid = match cell {
        Some(s) => SupSet::with_cell(copy(&auto), s),
        None => auto,
    };
    (grid, full)
}

#[test]
fn grid_scan_matches_full_scan_on_bumps() {
    let mut rng = Lcg(7);
    let mesh = bumps(14);
    // r 0.4 and 0.5 put the padded boxes on cell borders of a unit cell; r 3
    // is a tool wider than several cells; `None` is the production sizing.
    for r in [0.4, 0.5, 1.0, 3.0] {
        for cell in [Some(1.0), Some(0.37), Some(5.0), None] {
            let (grid, full) = twins(&mesh, r, cell);
            assert!(grid.has_grid() && !full.has_grid(), "r {r} cell {cell:?}");
            assert_parity(&grid, &full, r, &mut rng, 150);
        }
    }
}

#[test]
fn grid_scan_matches_full_scan_on_big_and_degenerate_facets() {
    let mut rng = Lcg(11);
    let mesh = mixed();
    for r in [0.25, 2.0, 6.0] {
        for cell in [Some(0.5), Some(2.0), None] {
            let (grid, full) = twins(&mesh, r, cell);
            assert!(grid.has_grid(), "r {r} cell {cell:?}");
            assert_parity(&grid, &full, r, &mut rng, 200);
        }
    }
}

/// The superset law: each facet whose padded box holds the point is in the
/// point's candidates, the candidates ascend strictly (original order, no
/// duplicates), and only facets of the set are returned.
#[test]
fn grid_candidates_hold_every_facet_whose_box_holds_the_point() {
    let mut rng = Lcg(3);
    for (mesh, r) in [(bumps(12), 0.5), (bumps(12), 4.0), (mixed(), 1.0), (mixed(), 7.5)] {
        for cell in [Some(1.0), Some(0.3), None] {
            let (grid, full) = twins(&mesh, r, cell);
            let gbase = grid.as_ptr();
            for (x, y) in queries(&full, &mut rng, 200) {
                let got: Vec<usize> = grid
                    .near(x, y)
                    .map(|s| (s as *const Sup as usize - gbase as usize) / std::mem::size_of::<Sup>())
                    .collect();
                assert!(got.windows(2).all(|w| w[0] < w[1]), "order at ({x}, {y})");
                assert!(got.iter().all(|&i| i < grid.len()));
                for (i, s) in full.iter().enumerate() {
                    let inside = x >= s.xmin && x <= s.xmax && y >= s.ymin && y <= s.ymax;
                    if inside {
                        assert!(got.binary_search(&i).is_ok(), "facet {i} missed at ({x}, {y}) r {r} cell {cell:?}");
                    }
                }
            }
        }
    }
}

/// A non-finite box (one the grid cannot place) and a non-finite query
/// both fall back to the full scan: same answers, no panic. One NaN vertex
/// leaves a finite box (`f64::min` skips a NaN), and the grid then bins
/// that box exactly as the full scan's reject reads it.
#[test]
fn non_finite_input_falls_back_to_the_full_scan() {
    let mut rng = Lcg(5);
    let mut one = bumps(5);
    one.push(Triangle::new(p(f64::NAN, 1.0, 1.0), p(2.0, 2.0, 1.0), p(1.0, 3.0, 1.0)));
    let (grid, full) = twins(&one, 0.5, None);
    assert!(grid.has_grid(), "one NaN vertex keeps a finite box");
    assert_parity(&grid, &full, 0.5, &mut rng, 40);
    let mut mesh = bumps(5);
    mesh.push(Triangle::new(p(f64::NAN, 1.0, 1.0), p(f64::NAN, 2.0, 1.0), p(f64::NAN, 3.0, 1.0)));
    let (grid, full) = twins(&mesh, 0.5, None);
    assert!(!grid.has_grid(), "a NaN box disables the grid");
    let mut inf = bumps(5);
    inf.push(Triangle::new(p(f64::INFINITY, 1.0, 1.0), p(2.0, 2.0, 1.0), p(1.0, 3.0, 1.0)));
    assert!(!build_sups(&inf, 0.5).has_grid(), "an infinite box disables the grid");
    assert_parity(&grid, &full, 0.5, &mut rng, 40);
    let (grid, full) = twins(&bumps(5), 0.5, None);
    assert!(grid.has_grid());
    for (x, y) in [(f64::NAN, 1.0), (1.0, f64::INFINITY), (f64::NEG_INFINITY, f64::NAN)] {
        assert_eq!(grid.near(x, y).count(), grid.len(), "a non-finite query reads every facet");
        let a = support_at(x, y, 0.5, -1.0e6, &grid);
        let b = support_at(x, y, 0.5, -1.0e6, &full);
        assert_eq!(bits(a), bits(b));
        assert_eq!(ball_clear(p(x, y, 1.0), &grid, 0.5), ball_clear(p(x, y, 1.0), &full, 0.5));
    }
    assert!(!SupSet::new(Vec::new()).has_grid(), "an empty set has no grid");
}

/// The recorded grid that camcraft-gpu's parity test reads (#2620), so the
/// GPU crate checks its binning against this f64 law without a dependency
/// on this crate. `CAMCRAFT_BLESS=1` writes the file; otherwise the file
/// must match the grid byte for byte (line ends aside).
#[test]
fn gpu_parity_fixture_matches_the_grid() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../gpu/tests/fixtures/facet_grid_2619.grid");
    // r 0.5 on integer vertices with a unit cell puts box edges on cell borders.
    let (grid, _) = twins(&mixed(), 0.5, Some(1.0));
    let Some((x0, y0, s, nx, ny, lists)) = grid.grid_dump() else {
        panic!("the fixture mesh must have a grid");
    };
    let mut out = String::from(
        "# camcraft facet grid fixture (#2620): written by camcraft-motion's\n\
         # gpu_parity_fixture_matches_the_grid (CAMCRAFT_BLESS=1); read by\n\
         # camcraft-gpu's bin_parity. f64 values round-trip ({:?}).\n",
    );
    out.push_str(&format!("grid {x0:?} {y0:?} {s:?} {nx} {ny}\n"));
    for b in grid.iter() {
        out.push_str(&format!("box {:?} {:?} {:?} {:?}\n", b.xmin, b.ymin, b.xmax, b.ymax));
    }
    for (c, l) in lists.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        let ix: Vec<String> = l.iter().map(u32::to_string).collect();
        out.push_str(&format!("cell {c} {}\n", ix.join(" ")));
    }
    if std::env::var("CAMCRAFT_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(path, &out).unwrap_or_else(|e| panic!("write {path}: {e}"));
        return;
    }
    let got = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {path}: {e} (CAMCRAFT_BLESS=1 writes it)"))
        .replace("\r\n", "\n");
    assert!(got == out, "{path} is stale: the grid changed; CAMCRAFT_BLESS=1 writes it again");
}

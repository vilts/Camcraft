//! An exact point-location index over a sliced `Region` (#3615): a Y-slab edge
//! index that answers material membership with the same arithmetic, in the
//! same order, as `Region::contains` - bit for bit - while a query reads only
//! the edges whose Y span holds `q.y`. The even-odd (ray-crossing)
//! point-in-polygon rule is standard computational-geometry practice; no
//! HSMWorks material is used.

use crate::geo2d::{Point2, Region};

/// One directed edge, exactly as [`Region::contains`] walks a loop: `a` is
/// `pts[i]` and `b` is `pts[(i + 1) % n]`, never reordered. `x_at` is computed
/// from them in that order, so its rounding matches the source.
#[derive(Clone, Copy)]
struct Edge {
    a: Point2,
    b: Point2,
    /// The half-open Y span of the source's test, `[ylo, yhi)`.
    ylo: f64,
    yhi: f64,
}

/// A Y-slab edge index: the distinct edge-endpoint Y values cut the axis into
/// slabs, and each slab lists the edges whose `[ylo, yhi)` holds it. A query
/// then reads the edges that cross its own Y, not every edge of the slice.
pub(super) struct SliceIndex {
    /// Distinct edge-endpoint Y values, ascending. A query's Y falls in one
    /// slab `[ys[k], ys[k + 1])`.
    ys: Vec<f64>,
    edges: Vec<Edge>,
    /// Slab `k`'s edge indices are `edge_slots[slab_start[k]..slab_start[k + 1]]`.
    edge_slots: Vec<usize>,
    slab_start: Vec<usize>,
}

/// A cached slice with its index, so the cached `Region` is not scanned again
/// on every point query (M2.5 E: `surface` probes thousands of slices).
pub(super) struct IndexedSlice {
    region: Region,
    index: SliceIndex,
}

impl IndexedSlice {
    pub(super) fn new(region: Region) -> Self {
        let index = SliceIndex::new(&region);
        Self { region, index }
    }

    pub(super) fn region(&self) -> &Region {
        &self.region
    }

    /// Exactly [`Region::contains`] for the same point.
    pub(super) fn contains(&self, q: Point2) -> bool {
        self.index.contains(q)
    }
}

impl SliceIndex {
    pub(super) fn new(region: &Region) -> Self {
        let mut edges: Vec<Edge> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for l in region.loops() {
            let pts = l.points();
            let n = pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
                // The source's test `p.y >= ylo && p.y < yhi` never counts an
                // edge whose span is empty, horizontal, or NaN-bounded. Keep
                // exactly those with a non-empty span; the rest cannot cross.
                if ylo < yhi {
                    edges.push(Edge { a, b, ylo, yhi });
                    ys.push(ylo);
                    ys.push(yhi);
                }
            }
        }
        ys.sort_by(|a, b| a.total_cmp(b));
        // Signed zeros are one boundary to the source's `>=`/`<` tests, so
        // dedup them with `==` (partial equality) rather than `total_cmp`.
        ys.dedup_by(|a, b| *a == *b);
        let slabs = ys.len().saturating_sub(1);
        let mut lists: Vec<Vec<usize>> = vec![Vec::new(); slabs];
        for (ei, e) in edges.iter().enumerate() {
            // `ys` holds both bounds, so these are their positions. The edge
            // crosses every slab `[ys[k], ys[k + 1])` with `ylo <= ys[k]` and
            // `yhi >= ys[k + 1]`, that is `pos(ylo) .. pos(yhi)`.
            let lo = ys.partition_point(|&v| v < e.ylo).min(slabs);
            let end = ys.partition_point(|&v| v <= e.yhi).saturating_sub(1).min(slabs);
            if lo < end {
                for slot in &mut lists[lo..end] {
                    slot.push(ei);
                }
            }
        }
        let mut slab_start = Vec::with_capacity(slabs + 1);
        let mut edge_slots = Vec::with_capacity(lists.iter().map(Vec::len).sum());
        slab_start.push(0);
        for slot in &lists {
            edge_slots.extend_from_slice(slot);
            slab_start.push(edge_slots.len());
        }
        SliceIndex { ys, edges, edge_slots, slab_start }
    }

    /// The source test over the edges that can cross the query's Y, with the
    /// same crossing arithmetic, in the same order within each edge. The XOR
    /// of per-edge crossings equals the source's per-loop XOR then per-loop
    /// XOR, because parity is associative.
    pub(super) fn contains(&self, q: Point2) -> bool {
        // The source counts no crossing for a non-finite Y: `p.y < yhi` fails
        // for +inf and NaN, and `p.y >= ylo` fails for -inf.
        if !q.y.is_finite() {
            return false;
        }
        let m = self.ys.len();
        if m < 2 {
            return false;
        }
        // First boundary strictly above `q.y`; the slab below it holds the
        // query (a query at or above the top boundary crosses nothing).
        let k = self.ys.partition_point(|&v| v <= q.y);
        if k == 0 || k >= m {
            return false;
        }
        let slab = k - 1;
        let (start, end) = (self.slab_start[slab], self.slab_start[slab + 1]);
        let mut inside = false;
        for &ei in &self.edge_slots[start..end] {
            let Some(e) = self.edges.get(ei) else {
                continue;
            };
            // The slab already guarantees this; recompute it so a slab bug
            // can only cost a comparison, never move a verdict.
            if q.y >= e.ylo && q.y < e.yhi {
                let x_at = e.a.x + (q.y - e.a.y) * (e.b.x - e.a.x) / (e.b.y - e.a.y);
                if q.x < x_at {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

#[cfg(test)]
mod tests {
    use super::SliceIndex;
    use crate::geo2d::{Loop, Point2, Region};

    fn region(tag: &str, loops: Vec<Vec<(f64, f64)>>) -> Region {
        let mut built = Vec::new();
        for pts in loops {
            let pts: Vec<Point2> = pts.iter().map(|&(x, y)| Point2::new(x, y)).collect();
            match Loop::new(pts) {
                Ok(l) => built.push(l),
                Err(e) => panic!("{tag}: {e:?}"),
            }
        }
        match Region::new(built) {
            Ok(r) => r,
            Err(e) => panic!("{tag}: {e:?}"),
        }
    }

    /// Compare the index against `Region::contains` over a grid that lands on
    /// every vertex line and on points between them.
    fn check(tag: &str, r: &Region) {
        let index = SliceIndex::new(r);
        let mut y = -1.0;
        while y <= 11.0 {
            let mut x = -1.0;
            while x <= 11.0 {
                let q = Point2::new(x, y);
                assert_eq!(index.contains(q), r.contains(q), "{tag}: point {q:?}");
                x += 0.25;
            }
            y += 0.25;
        }
    }

    #[test]
    fn the_index_equals_region_contains_bit_for_bit() {
        // A U: a notch from the top puts a different boundary on each Y slab.
        let u = region(
            "u",
            vec![vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (7.0, 10.0),
                (7.0, 5.0),
                (3.0, 5.0),
                (3.0, 10.0),
                (0.0, 10.0),
            ]],
        );
        check("u", &u);

        // A staircase: one X wall per Y band, the levels-only plane set's
        // failure shape.
        let step = region(
            "step",
            vec![vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 3.0),
                (7.0, 3.0),
                (7.0, 6.0),
                (4.0, 6.0),
                (4.0, 9.0),
                (0.0, 9.0),
            ]],
        );
        check("step", &step);

        // An outer square with a hole: two crossing pairs on each slab.
        let holed = region(
            "holed",
            vec![
                vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
                vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)],
            ],
        );
        check("holed", &holed);
    }
}

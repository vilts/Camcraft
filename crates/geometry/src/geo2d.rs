//! 2D regions: closed loops with even-odd semantics, booleans and buffering.
//!
//! Provenance: the region model (closed boundaries, even-odd containment) is
//! the standard polygon-clipping formulation from the published literature;
//! the boolean/offset engine behind the bridge below is `clipper2-rust`, a
//! pure-Rust port (BSL-1.0) of the published Clipper2 library - used as a
//! dependency, not adapted code, and zero vendor CAM provenance either way.
//! The millimeter bridge, validation gates and measurements are ours.
//!
//! Units: the public surface is millimeters in `f64`. Clipper's domain is
//! integers, so every crossing quantizes to the fixed [`SCALE`] (1 unit =
//! 1 um, far finer than any CAM tolerance). Inputs must be finite and far
//! inside the i64 horizon; anything else becomes a `GeoError`, never a
//! garbage number or a panic (Prime Directive).

use thiserror::Error;

mod clip_line;
mod slivers;
mod sweep;

pub use clip_line::RingClip;

use clipper2_rust::{
    EndType, FillRule, JoinType, Path64, Paths64, Point64, inflate_paths_64, intersect_64,
    difference_64, union_64, union_subjects_64,
};

/// Millimeters to Clipper units and back: 1e6, so a unit is a micron.
pub const SCALE: f64 = 1e6;

/// Coordinate magnitude guard: |mm| must stay far below i64::MAX / SCALE
/// (about 9.2e12) so quantization can never wrap. Half that horizon is still
/// a thousand times any machine travel.
pub(crate) const COORD_LIMIT: f64 = 1e12;

/// Round-join arc flattening: maximum radial error in units. 5e3 = 5 um -
/// finer than CAM gouge tolerances, coarse enough that a full circle at
/// tool radius stays in the dozens of segments.
pub(crate) const ARC_TOL_UNITS: f64 = 5e3;

/// Miter limit of [`Region::offset_sharp`] (Clipper's default): a corner
/// sharper than ~60 degrees is squared off at twice the offset instead of
/// running a spike out to infinity.
pub(crate) const MITER_LIMIT: f64 = 2.0;

#[derive(Clone, Error, Debug, PartialEq)]
pub enum GeoError {
    #[error("invalid geometry: {0}")]
    Invalid(String),
    #[error("coordinates outside the machinable range")]
    OutOfRange,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Point2 { x, y }
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// One closed boundary, an ordered ring of points. Orientation is not part
/// of the contract: `Region` interprets its loops by even-odd parity, and
/// `Region::new` normalizes whatever it is handed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loop {
    pts: Vec<Point2>,
}

impl Loop {
    /// Validate for external input (sketch points, sliced geometry): finite,
    /// in range, at least three distinct vertices. Consecutive duplicates
    /// are dropped, not refused - quantization round-trips create them.
    pub fn new(pts: Vec<Point2>) -> Result<Self, GeoError> {
        for p in &pts {
            if !p.is_finite() {
                return Err(GeoError::Invalid("non-finite vertex".into()));
            }
            if p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT {
                return Err(GeoError::OutOfRange);
            }
        }
        let looped = Self::dedup(pts);
        if looped.len() < 3 {
            return Err(GeoError::Invalid("loop has fewer than 3 vertices".into()));
        }
        Ok(Loop { pts: looped })
    }

    /// Drop consecutive repeats, including the closing wrap (input may or
    /// may not repeat the first vertex).
    fn dedup(pts: Vec<Point2>) -> Vec<Point2> {
        let mut out: Vec<Point2> = Vec::with_capacity(pts.len());
        for p in pts {
            if out.last().is_none_or(|q| q.x != p.x || q.y != p.y) {
                out.push(p);
            }
        }
        if out.len() > 1 && out[0].x == out[out.len() - 1].x && out[0].y == out[out.len() - 1].y {
            out.pop();
        }
        out
    }

    pub fn points(&self) -> &[Point2] {
        &self.pts
    }

    /// Signed shoelace area in mm^2; positive when the ring runs
    /// counterclockwise. Zero only for collinear (degenerate) rings.
    pub fn area(&self) -> f64 {
        let mut sum = 0.0f64;
        for i in 0..self.pts.len() {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % self.pts.len()];
            sum += a.x * b.y - b.x * a.y;
        }
        sum * 0.5
    }

    pub fn is_ccw(&self) -> bool {
        self.area() > 0.0
    }

    fn contains_xy(&self, p: Point2) -> bool {
        // Ray parity to +x with the half-open edge rule: an edge counts if
        // p.y is in [yi, yj) (lower-inclusive, upper-exclusive, after the
        // swap), which makes shared vertices and horizontal rays exact.
        let mut inside = false;
        let n = self.pts.len();
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            if p.y >= ylo && p.y < yhi && n != 0 {
                let x_at = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if p.x < x_at {
                    inside = !inside;
                }
            }
        }
        inside
    }

    fn to_path64(&self) -> Result<Path64, GeoError> {
        let mut path = Path64::with_capacity(self.pts.len());
        for p in &self.pts {
            path.push(Point64 {
                x: quantize(p.x)?,
                y: quantize(p.y)?,
            });
        }
        Ok(path)
    }

    fn from_path64(path: &Path64) -> Loop {
        Loop {
            pts: path
                .iter()
                .map(|q| Point2 {
                    x: q.x as f64 / SCALE,
                    y: q.y as f64 / SCALE,
                })
                .collect(),
        }
    }
}

/// An even-odd region: a set of closed loops whose combined interior is the
/// material. After construction the loops are disjoint, so containment is
/// plain parity and the area is a signed sum by nesting depth.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Region {
    loops: Vec<Loop>,
}

impl Region {
    /// Accept any loop set - overlapping outer loops from several solids,
    /// holes in either orientation - and normalize. This is the only entry
    /// from sketched/raw geometry; never build Region by assignment (see
    /// also `from_slices`, which relaxes the self-crossing refusal for
    /// slicer output). The semantics: loops are shells, so non-nesting
    /// loops carry material jointly (two overlapping boxes union, their
    /// overlap is not a void), while a loop nested in another at odd depth
    /// is a hole. Implementation: orient by nesting parity, then fuse with
    /// a non-zero union (see the Clipper2 fill-rule docs).
    pub fn new(loops: Vec<Loop>) -> Result<Region, GeoError> {
        Self::normalize(loops, false, true)
    }

    /// Region from sliced-shell output, where a self-crossing cycle is a
    /// real boundary (two lobes touching along a line: the shell encloses
    /// both) rather than ambiguous user input, so the even-odd decomposition
    /// is kept instead of refused. Everything else is `new`'s contract.
    pub fn from_slices(loops: Vec<Loop>) -> Result<Region, GeoError> {
        Self::normalize(loops, true, true)
    }

    /// Region from loops that are all material - a pure union, with no
    /// nesting-parity hole reading. This is the entry for forbidden-center
    /// scans (`finishing3d`'s waterline): the contributions of the facets
    /// of ONE solid legitimately nest (a box's bottom-facet footprint sits
    /// inside its walls' offset square), and a nested shell there is area
    /// already covered, never a hole - the law `new`'s shell parity exists
    /// to read is the law of sketch geometry, not of overlapping
    /// contributions. Same self-crossing refusal as `new`. Input
    /// orientation must be CCW (material); `Loop::convex_hull` guarantees
    /// it, and duplicate loops double-wind without cancelling.
    pub fn union_all(loops: Vec<Loop>) -> Result<Region, GeoError> {
        Self::normalize(loops, false, false)
    }

    fn normalize(
        loops: Vec<Loop>,
        decompose_crossings: bool,
        parity: bool,
    ) -> Result<Region, GeoError> {
        // 1. A loop that crosses itself is not a boundary of anything a
        //    sketch could mean; its "area" is convention dependent. For
        //    sketch input (`new`) refuse rather than half-compute (Prime
        //    Directive); for slicer output (`from_slices`) the even-odd
        //    decomposition IS the canonical reading, so keep the pieces.
        //    A simple loop yields exactly one path either way.
        let mut simple: Vec<Loop> = Vec::new();
        for l in &loops {
            let dec = union_subjects_64(&vec![l.to_path64()?], FillRule::EvenOdd);
            if dec.len() > 1 && !decompose_crossings {
                return Err(GeoError::Invalid("loop crosses itself".into()));
            }
            for o in dec {
                let o = Loop::from_path64(&o);
                if o.pts.len() >= 3 {
                    simple.push(o);
                }
            }
        }
        // 2. Orient by nesting depth: even depth CCW (material), odd CW
        //    (hole). Nesting is all-vertices-inside, not one anchor point
        //    inside - a partial overlap must read as two outers (a non-zero
        //    union fuses them), not as a hole punched in the first.
        //    `union_all` skips this step entirely: its loops are all
        //    material by contract, so nesting is coverage, never a hole.
        let oriented: Vec<Loop> = if parity {
            simple
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let depth = simple
                        .iter()
                        .enumerate()
                        .filter(|(j, o)| {
                            *j != i && l.points().iter().all(|p| o.contains_xy(*p))
                        })
                        .count();
                    if (depth % 2 == 0) == l.is_ccw() {
                        l.clone()
                    } else {
                        Loop {
                            pts: l.pts.iter().rev().copied().collect(),
                        }
                    }
                })
                .collect()
        } else {
            simple
        };
        // 3. Fuse: overlapping outers become one material; nesting stays.
        Ok(Self::from_paths(union_subjects_64(
            &to_paths(&oriented)?,
            FillRule::NonZero,
        )))
    }

    pub fn empty() -> Region {
        Region::default()
    }

    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    pub fn loops(&self) -> &[Loop] {
        &self.loops
    }

    /// Axis-aligned XY box of all vertices, (min, max); None when empty.
    /// Generators band their passes with this; it bounds the boundary
    /// points, which for the polygonized regions here is exact enough -
    /// round-join arcs bulge past their chord by at most ARC_TOL_UNITS.
    pub fn bounds(&self) -> Option<(Point2, Point2)> {
        let mut it = self.loops.iter().flat_map(|l| l.points().iter());
        let first = it.next()?;
        let mut box_ = (Point2::new(first.x, first.y), Point2::new(first.x, first.y));
        for p in it {
            box_.0.x = box_.0.x.min(p.x);
            box_.0.y = box_.0.y.min(p.y);
            box_.1.x = box_.1.x.max(p.x);
            box_.1.y = box_.1.y.max(p.y);
        }
        Some(box_)
    }

    fn from_paths(paths: Paths64) -> Region {
        Region {
            loops: paths
                .iter()
                .map(Loop::from_path64)
                .filter(|l| l.pts.len() >= 3)
                .collect(),
        }
    }

    fn to_paths(&self) -> Result<Paths64, GeoError> {
        to_paths(&self.loops)
    }

    /// Containment: parity of the total ray crossings over all loops.
    /// (On the normalized, disjoint loop set this equals winding, which is
    /// what the overlap-union contract in `new` guarantees.) On an exact
    /// edge the half-open ray rule decides; gouge checks stay a tolerance
    /// away from boundaries anyway.
    pub fn contains(&self, p: Point2) -> bool {
        let mut inside = false;
        for l in &self.loops {
            inside ^= l.contains_xy(p);
        }
        inside
    }

    /// Region area in mm^2: the signed loop-area sum. On a normalized
    /// region the winding number equals the material indicator (outer
    /// CCW = +1, hole CW brings its interior back to 0, an island inside
    /// a hole to +1, ...), so the integral is the sum of signed shoelace
    /// areas - exact, O(n), and unlike a per-loop containment test it
    /// cannot be fooled by loops that touch at a shared vertex.
    pub fn area(&self) -> f64 {
        self.loops.iter().map(Loop::area).sum()
    }

    pub fn union(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(union_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    pub fn intersect(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(intersect_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    pub fn difference(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(difference_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    /// Buffer the region by `d` mm: positive grows, negative shrinks
    /// (a shrink past the medial axis collapses to empty - a clean empty
    /// region, not an error: the generator decides what that means).
    /// Round joins keep corners circular within [`ARC_TOL_UNITS`].
    pub fn offset(&self, d: f64) -> Result<Region, GeoError> {
        if !d.is_finite() {
            return Err(GeoError::Invalid("non-finite offset distance".into()));
        }
        if d.abs() >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        Ok(Self::from_paths(inflate_paths_64(
            &self.to_paths()?,
            d * SCALE,
            JoinType::Round,
            EndType::Polygon,
            MITER_LIMIT,
            ARC_TOL_UNITS,
        )))
    }

    /// [`offset`](Self::offset) with mitred joins: a grown rectangle stays a
    /// rectangle (M11.5's sharp corner style). Same refusals, same lattice.
    pub fn offset_sharp(&self, d: f64) -> Result<Region, GeoError> {
        if !d.is_finite() {
            return Err(GeoError::Invalid("non-finite offset distance".into()));
        }
        if d.abs() >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        Ok(Self::from_paths(inflate_paths_64(
            &self.to_paths()?,
            d * SCALE,
            JoinType::Miter,
            EndType::Polygon,
            MITER_LIMIT,
            ARC_TOL_UNITS,
        )))
    }

    /// Triangulate the region's material for display: the returned
    /// triangles' union is exactly what [`contains`](Self::contains) says -
    /// holes stay void, islands nested inside holes come back filled.
    ///
    /// Provenance: our own, from the published textbook construction
    /// (merge each hole into its container along a horizontal ray from the
    /// hole's rightmost vertex, then ear-clip the merged walk - standard
    /// computational-geometry literature). clipper2-rust does booleans and
    /// offsets, not triangulation, so this is not a bridge around a
    /// dependency.
    ///
    /// Contract: nothing here may guess. Each piece is clipped with a
    /// bounded candidate budget, bridges are verified not to cross the
    /// boundary they span, and any degeneracy - a hole without a
    /// container, a bridge with no valid endpoint, an ear-clipping stuck on
    /// self-touching input - returns `Err`, never a partial or wrong fill.
    /// The UI's translucent cut-region fill is the only consumer; on `Err`
    /// it draws the wireframe outline and nothing else. Never hangs: every
    /// loop makes progress or refuses.
    pub fn triangles(&self) -> Result<Vec<[Point2; 3]>, GeoError> {
        let n = self.loops.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let total: usize = self.loops.iter().map(|l| l.pts.len()).sum();
        if total > TRI_MAX_VERTS {
            return Err(GeoError::Invalid(format!(
                "region too dense to triangulate ({total} vertices)"
            )));
        }
        let area: Vec<f64> = self.loops.iter().map(|l| l.area()).collect();

        // Containment forest: each loop's parent is the smallest-|area|
        // loop enclosing it. Loops touching only at boundary points still
        // read as parent/child (the on-loop clause); two coincident
        // duplicates have equal area and no honest reading, so they refuse.
        let mut parent: Vec<Option<usize>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut best: Option<usize> = None;
            for j in 0..n {
                if i == j || !encloses(&self.loops[j], &self.loops[i].pts) {
                    continue;
                }
                match best {
                    None => best = Some(j),
                    Some(b) if area[j].abs() == area[b].abs() => {
                        return Err(GeoError::Invalid("coincident duplicate loop".into()));
                    }
                    Some(b) if area[j].abs() < area[b].abs() => best = Some(j),
                    Some(_) => {}
                }
            }
            parent.push(best);
        }

        let mut tris: Vec<[Point2; 3]> = Vec::new();
        // Worklist of piece roots (even depth: outer boundaries and islands
        // inside holes). Odd-depth loops merge into their parent's walk;
        // their CCW children are the next pieces.
        let mut stack: Vec<usize> = (0..n).filter(|i| parent[*i].is_none()).collect();
        while let Some(root) = stack.pop() {
            if area[root] < 0.0 {
                return Err(GeoError::Invalid("hole outside any boundary".into()));
            }
            let mut walk = self.loops[root].pts.clone();
            for h in 0..n {
                if parent[h] != Some(root) {
                    continue;
                }
                if area[h] >= 0.0 {
                    return Err(GeoError::Invalid("boundary inside boundary".into()));
                }
                merge_hole(&mut walk, &self.loops[h].pts)?;
                for g in 0..n {
                    if parent[g] == Some(h) {
                        if area[g] <= 0.0 {
                            return Err(GeoError::Invalid("hole inside hole".into()));
                        }
                        stack.push(g);
                    }
                }
            }
            clip_walk(&walk, &mut tris)?;
        }
        Ok(tris)
    }

    pub fn square(side: f64) -> Result<Region, GeoError> {
        Region::new(vec![Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(side, 0.0),
            Point2::new(side, side),
            Point2::new(0.0, side),
        ])?])
    }
}

// --- triangulation helpers (see `Region::triangles` for provenance) ---

/// Refuse rather than risk the frame: the ear budget is quadratic in this,
/// and no display fill is worth an unbounded rebuild.
const TRI_MAX_VERTS: usize = 20_000;

/// Strictly positive cross product of (b - a) x (c - b): a left turn.
fn is_left(a: Point2, b: Point2, c: Point2) -> bool {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) > 0.0
}

fn is_right(a: Point2, b: Point2, c: Point2) -> bool {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) < 0.0
}

/// `p` lies exactly on the closed segment `a`-`b`.
fn on_segment(a: Point2, b: Point2, p: Point2) -> bool {
    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    cross == 0.0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

fn on_loop(l: &Loop, p: Point2) -> bool {
    let n = l.pts.len();
    (0..n).any(|i| on_segment(l.pts[i], l.pts[(i + 1) % n], p))
}

/// Loop `parent` encloses the point set `child`: every point inside or
/// exactly on its boundary, and at least one strictly inside (otherwise two
/// merely touching or coincident loops would "enclose" each other).
fn encloses(parent: &Loop, child: &[Point2]) -> bool {
    let mut strict = false;
    for p in child {
        if parent.contains_xy(*p) {
            strict = true;
        } else if !on_loop(parent, *p) {
            return false;
        }
    }
    strict
}

/// The open segments `a1`-`a2` and `b1`-`b2` cross at an interior point of
/// both - the test that keeps a bridge chord from cutting through boundary.
fn proper_cross(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> bool {
    let side = |p: Point2, q1: Point2, q2: Point2| -> f64 {
        (q2.x - q1.x) * (p.y - q1.y) - (q2.y - q1.y) * (p.x - q1.x)
    };
    let s1 = side(a1, b1, b2);
    let s2 = side(a2, b1, b2);
    let s3 = side(b1, a1, a2);
    let s4 = side(b2, a1, a2);
    s1 * s2 < 0.0 && s3 * s4 < 0.0
}

/// Parity of ray crossings to +x over a (possibly self-touching) closed
/// walk: doubled bridge chords contribute twice and cancel, so the answer
/// is this piece's material - the same law `Loop::contains_xy` runs, on a
/// ring with repeated vertices instead of a simple loop.
fn walk_contains(ring: &[Point2], p: Point2) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
        if p.y >= ylo && p.y < yhi {
            let x_at = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x_at {
                inside = !inside;
            }
        }
    }
    inside
}

/// Is `p` strictly inside triangle `t`? (Boundary belongs to whichever
/// triangle claims it, and the ear tests read the same way.)
fn tri_contains(t: [Point2; 3], p: Point2) -> bool {
    let side = |a: Point2, b: Point2| -> f64 {
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
    };
    let (a, b, c) = (t[0], t[1], t[2]);
    let (s1, s2, s3) = (side(a, b), side(b, c), side(c, a));
    (s1 > 0.0 && s2 > 0.0 && s3 > 0.0) || (s1 < 0.0 && s2 < 0.0 && s3 < 0.0)
}

/// Splice `hole`'s boundary into `walk` with a verified bridge chord,
/// turning two rings into one weakly-simple walk whose winding still equals
/// the region's material (the bridge is traversed both ways and cancels).
/// A walk vertex comes first, nearest first; the computed ray hit is the
/// last resort, as its rounding can leave it just off its edge. A candidate
/// is accepted only if the open bridge segment crosses no boundary edge and
/// stays on material along its length - never a guess (Prime Directive).
fn merge_hole(walk: &mut Vec<Point2>, hole: &[Point2]) -> Result<(), GeoError> {
    let mut r = hole[0];
    for p in hole {
        if p.x > r.x || (p.x == r.x && p.y > r.y) {
            r = *p;
        }
    }
    let nl = hole.len();
    let ri = hole
        .iter()
        .position(|p| *p == r)
        .ok_or_else(|| GeoError::Invalid("empty hole loop".into()))?;
    // The hole cycle leaves r for its successor and closes back on r;
    // spliced between the two bridge traversals below.
    let cycle: Vec<Point2> = (1..=nl).map(|k| hole[(ri + k) % nl]).collect();
    let hole_loop = Loop {
        pts: hole.to_vec(),
    };

    // Walk vertices come first (#2762): a bridge between two existing
    // vertices adds no new point. A computed ray hit lies on its edge only
    // to the rounding of its coordinates; a hit a few 1e-16 inside the
    // material reads as strictly inside the ears beside it, and the clip
    // stalls with "no ear". Eberly's construction (the visible vertex of
    // the hit edge or a reflex vertex in its triangle) shows that a walk
    // vertex at x >= r.x is visible whenever the ray hits, so the ray
    // points stay only as the last resort.
    let nw = walk.len();
    let by_distance = |p: &Point2, q: &Point2| {
        ((p.x - r.x) * (p.x - r.x) + (p.y - r.y) * (p.y - r.y))
            .partial_cmp(&((q.x - r.x) * (q.x - r.x) + (q.y - r.y) * (q.y - r.y)))
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    let mut cands: Vec<Point2> = walk.iter().filter(|p| **p != r && p.x >= r.x).copied().collect();
    cands.sort_by(by_distance);
    let mut hits: Vec<Point2> = Vec::new();
    for j in 0..nw {
        let (a, b) = (walk[j], walk[(j + 1) % nw]);
        if a.y == b.y {
            continue; // collinear with the ray or missing it: the vertex pass covers these
        }
        let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
        if r.y >= ylo && r.y < yhi {
            let x = a.x + (r.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x > r.x {
                hits.push(Point2::new(x, r.y));
            }
        }
    }
    hits.sort_by(by_distance);
    cands.extend(hits);

    for q in cands {
        let clear = (0..nw).all(|j| !proper_cross(r, q, walk[j], walk[(j + 1) % nw]));
        // The bridge must ride this piece's material: inside the walk's
        // winding (which already excludes any hole merged before this one)
        // and out of this hole - a sample inside an island nested in the
        // hole belongs to a different piece.
        let on_material = [1.0 / 3.0, 0.5, 2.0 / 3.0].iter().all(|t| {
            let m = Point2::new(r.x + (q.x - r.x) * t, r.y + (q.y - r.y) * t);
            walk_contains(walk, m) && !hole_loop.contains_xy(m)
        });
        if !clear || !on_material {
            continue;
        }
        // Splice after the edge containing q (a vertex hit is the endpoint case).
        let mut splice = nw; // append at the end
        for j in 0..nw {
            if on_segment(walk[j], walk[(j + 1) % nw], q) {
                splice = j + 1;
                break;
            }
        }
        let mut merged = walk[..splice].to_vec();
        merged.push(q);
        merged.push(r);
        merged.extend_from_slice(&cycle);
        merged.push(q);
        merged.extend_from_slice(&walk[splice..]);
        *walk = merged;
        return Ok(());
    }
    Err(GeoError::Invalid("no bridge reaches the hole".into()))
}

/// Ear-clip one weakly-simple walk (CCW boundary, holes spliced in through
/// doubled bridge chords). An ear is cut only when its tip is a left turn,
/// its centroid is material, and no reflex vertex lies inside it - on a
/// merged walk every hole vertex reads reflex, so no ear ever buries a
/// hole. Bounded by a quadratic candidate budget: stuck is refused, never
/// retried forever.
fn clip_walk(walk: &[Point2], tris: &mut Vec<[Point2; 3]>) -> Result<(), GeoError> {
    let mut ring = Loop::dedup(walk.to_vec());
    if ring.len() < 3 {
        return Ok(()); // no area to fill
    }
    let budget = ring.len().saturating_mul(ring.len()).saturating_add(1024);
    let mut churn = 0usize;
    while ring.len() > 3 {
        let n = ring.len();
        let mut found: Option<usize> = None;
        for i in 0..n {
            churn += 1;
            if churn > budget {
                return Err(GeoError::Invalid("triangulation budget exhausted".into()));
            }
            let (a, b, c) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            if !is_left(a, b, c) {
                continue;
            }
            let cen = Point2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
            if !walk_contains(&ring, cen) {
                continue;
            }
            let blocked = (0..n).any(|j| {
                if j == (i + n - 1) % n || j == i || j == (i + 1) % n {
                    return false;
                }
                let (p, q, t) = (ring[(j + n - 1) % n], ring[j], ring[(j + 1) % n]);
                is_right(p, q, t) && tri_contains([a, b, c], q)
            });
            if blocked {
                continue;
            }
            found = Some(i);
            break;
        }
        let Some(i) = found else {
            return Err(GeoError::Invalid("no ear in a non-triangle walk".into()));
        };
        let n = ring.len();
        let t = [ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]];
        ring.remove(i);
        let cross = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
        if cross != 0.0 {
            tris.push(t);
        }
    }
    if ring.len() == 3 {
        let t = [ring[0], ring[1], ring[2]];
        let cross = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
        if cross != 0.0 {
            tris.push(t);
        }
    }
    Ok(())
}

fn to_paths(loops: &[Loop]) -> Result<Paths64, GeoError> {
    let mut paths = Vec::with_capacity(loops.len());
    for l in loops {
        paths.push(l.to_path64()?);
    }
    Ok(paths)
}

pub(crate) fn quantize(mm: f64) -> Result<i64, GeoError> {
    if !mm.is_finite() {
        return Err(GeoError::Invalid("non-finite coordinate".into()));
    }
    if mm.abs() >= COORD_LIMIT {
        return Err(GeoError::OutOfRange);
    }
    Ok((mm * SCALE).round() as i64)
}

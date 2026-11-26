//! V-carve geometry (#1443): the largest circle inside a closed outline
//! that touches the outline at one given point. Its centre is a point of
//! the medial axis and its radius is the half width of the region there.
//!
//! Provenance: the medial axis transform as the set of maximal inscribed
//! discs (H. Blum, "A transformation for extracting new descriptors of
//! shape", 1967; Choi, Choi and Moon, "Mathematical theory of medial
//! axis transform", Pacific J. Math. 181, 1997), computed for each
//! boundary point with the tangent-circle shrink of Ma, Bae and Choi
//! ("3D medial axis point approximation using nearest neighbors and the
//! normal field", The Visual Computer 28, 2012), here in closed form
//! against line segments. The branch that runs into a nearly straight
//! corner is dropped by the separation angle of the simplified medial
//! axis (Foskey, Lin and Manocha, "Efficient computation of a simplified
//! medial axis", J. Comput. Inf. Sci. Eng. 3, 2003). Written from those
//! papers and plane geometry. No vendor material and no other program's
//! code or output was consulted.
//!
//! The circle through a point `q` that is tangent to the outline at `p`,
//! on the side of the unit normal `n`, has the radius
//! `|q - p|^2 / (2 n.(q - p))`. The smallest such radius over the whole
//! outline is the inscribed radius at `p`: the circles tangent at `p`
//! are nested, so a smaller one never holds a point of the outline.

use crate::geo2d::{Point2, Region};

use super::face::GenError;

/// Two contacts count as one tie when their radii agree to this part.
const TIE: f64 = 1e-4;
/// A turn smaller than this is a straight joint, radians.
const FLAT: f64 = 1e-9;

/// One edge of the outline. The region is on its left.
pub(super) struct Seg {
    pub(super) a: Point2,
    b: Point2,
    /// `b - a`.
    e: (f64, f64),
    len2: f64,
    /// Unit normal into the region.
    pub(super) n: (f64, f64),
    lo: (f64, f64),
    hi: (f64, f64),
    prev: usize,
    next: usize,
    /// The signed turn at `a`, radians: positive is a convex corner of
    /// the region, negative a reflex corner.
    pub(super) turn: f64,
}

impl Seg {
    pub(super) fn len(&self) -> f64 {
        self.len2.sqrt()
    }

    /// The smallest tangent-circle radius this edge allows at `p` with
    /// the normal `n`, and the point of the edge that sets it.
    fn limit(&self, p: Point2, n: (f64, f64)) -> (f64, Point2) {
        let w0 = (self.a.x - p.x, self.a.y - p.y);
        let a = w0.0 * self.e.0 + w0.1 * self.e.1;
        let b = w0.0 * n.0 + w0.1 * n.1;
        let c = self.e.0 * n.0 + self.e.1 * n.1;
        let w = w0.0 * w0.0 + w0.1 * w0.1;
        let e = self.len2;
        let mut best = (f64::INFINITY, self.a);
        let mut probe = |t: f64| {
            if !t.is_finite() {
                return;
            }
            let t = t.clamp(0.0, 1.0);
            let q = (w0.0 + t * self.e.0, w0.1 + t * self.e.1);
            let d = q.0 * n.0 + q.1 * n.1;
            let qq = q.0 * q.0 + q.1 * q.1;
            // Behind the tangent line, or on it: no circle reaches it.
            if d <= 1e-12 * qq.sqrt() {
                return;
            }
            let f = qq / (2.0 * d);
            if f < best.0 {
                best = (f, Point2::new(p.x + q.0, p.y + q.1));
            }
        };
        probe(0.0);
        probe(1.0);
        // The stationary points of the radius along the edge: the roots
        // of `e c t^2 + 2 e b t + (2 a b - w c)`.
        if c.abs() <= 1e-12 * e.sqrt() {
            probe(-a / e);
        } else {
            let disc = e * e * b * b - e * c * (2.0 * a * b - w * c);
            if disc >= 0.0 {
                let sq = disc.sqrt();
                probe((-e * b + sq) / (e * c));
                probe((-e * b - sq) / (e * c));
            }
        }
        best
    }

    fn dist(&self, p: Point2) -> f64 {
        let t = (((p.x - self.a.x) * self.e.0 + (p.y - self.a.y) * self.e.1) / self.len2).clamp(0.0, 1.0);
        (p.x - self.a.x - t * self.e.0).hypot(p.y - self.a.y - t * self.e.1)
    }

    /// The edge's box misses the box of the circle about `c`.
    fn misses(&self, c: Point2, r: f64) -> bool {
        self.lo.0 > c.x + r || self.hi.0 < c.x - r || self.lo.1 > c.y + r || self.hi.1 < c.y - r
    }
}

/// A place on the outline: on an edge, or in the fan of normals at a
/// reflex corner (the circle rolls about the corner there).
#[derive(Clone, Copy)]
pub(super) enum Piece {
    /// Edge `i`, from its start to its end.
    Edge(usize),
    /// The reflex corner at the start of edge `i`.
    Fan(usize),
}

/// The inscribed circle at one place of the outline.
#[derive(Clone, Copy)]
pub(super) struct Sample {
    /// The point of the outline.
    pub(super) p: Point2,
    /// The unit normal into the region at `p`.
    pub(super) n: (f64, f64),
    /// The inscribed radius.
    pub(super) r: f64,
    /// The angle at the circle's centre between `p` and the farthest
    /// other contact, radians.
    pub(super) sep: f64,
}

impl Sample {
    /// The centre of the tangent circle of radius `rho`.
    pub(super) fn centre(&self, rho: f64) -> Point2 {
        Point2::new(self.p.x + rho * self.n.0, self.p.y + rho * self.n.1)
    }
}

/// The outline of a region as edges, with the walk order of each loop.
pub(super) struct Outline {
    pub(super) segs: Vec<Seg>,
    /// Each loop's places in walk order.
    pub(super) loops: Vec<Vec<Piece>>,
    /// Edge tests done so far: the work budget reads it.
    pub(super) work: std::cell::Cell<u64>,
}

impl Outline {
    /// Build the edges. The loops are sorted by `|area|`, the largest
    /// first, so the path order does not depend on the input order.
    pub(super) fn new(region: &Region) -> Result<Self, GenError> {
        let mut loops: Vec<_> = region.loops().iter().collect();
        loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
        let mut segs: Vec<Seg> = Vec::new();
        let mut walks = Vec::with_capacity(loops.len());
        for l in loops {
            let pts: Vec<Point2> = l.points().to_vec();
            let m = pts.len();
            if m < 3 {
                return Err(GenError::Invalid("V-carve needs a closed region: a loop has fewer than three points".into()));
            }
            let base = segs.len();
            for i in 0..m {
                let (a, b) = (pts[i], pts[(i + 1) % m]);
                let e = (b.x - a.x, b.y - a.y);
                let len2 = e.0 * e.0 + e.1 * e.1;
                if !(len2.is_finite() && len2 > 0.0) {
                    return Err(GenError::Invalid("V-carve region has an edge of no length".into()));
                }
                let len = len2.sqrt();
                segs.push(Seg {
                    a,
                    b,
                    e,
                    len2,
                    n: (-e.1 / len, e.0 / len),
                    lo: (a.x.min(b.x), a.y.min(b.y)),
                    hi: (a.x.max(b.x), a.y.max(b.y)),
                    prev: base + (i + m - 1) % m,
                    next: base + (i + 1) % m,
                    turn: 0.0,
                });
            }
            let mut walk = Vec::with_capacity(2 * m);
            for i in base..base + m {
                let d0 = segs[segs[i].prev].e;
                let d1 = segs[i].e;
                let turn = (d0.0 * d1.1 - d0.1 * d1.0).atan2(d0.0 * d1.0 + d0.1 * d1.1);
                segs[i].turn = turn;
                if turn < -FLAT {
                    walk.push(Piece::Fan(i));
                }
                walk.push(Piece::Edge(i));
            }
            walks.push(walk);
        }
        Ok(Self { segs, loops: walks, work: std::cell::Cell::new(0) })
    }

    /// The length of a place, for the first split: millimetres for an
    /// edge, radians for a fan.
    pub(super) fn span(&self, piece: Piece) -> f64 {
        match piece {
            Piece::Edge(i) => self.segs[i].len(),
            Piece::Fan(i) => self.segs[i].turn.abs(),
        }
    }

    /// The inscribed circle at the parameter `t` in `[0, 1]` of a place.
    pub(super) fn eval(&self, piece: Piece, t: f64) -> Sample {
        match piece {
            Piece::Edge(i) => {
                let s = &self.segs[i];
                let p = Point2::new(s.a.x + t * s.e.0, s.a.y + t * s.e.1);
                let mut skip = [i, i, i];
                // At a convex corner the circle is the corner itself.
                if t <= 0.0 {
                    if s.turn > FLAT {
                        return Sample { p: s.a, n: s.n, r: 0.0, sep: s.turn };
                    }
                    skip[1] = s.prev;
                }
                if t >= 1.0 {
                    let turn = self.segs[s.next].turn;
                    if turn > FLAT {
                        return Sample { p: s.b, n: s.n, r: 0.0, sep: turn };
                    }
                    skip[2] = s.next;
                }
                self.touch(p, s.n, skip)
            }
            Piece::Fan(i) => {
                let s = &self.segs[i];
                let n0 = self.segs[s.prev].n;
                let (sin, cos) = (t * s.turn).sin_cos();
                let n = (n0.0 * cos - n0.1 * sin, n0.0 * sin + n0.1 * cos);
                self.touch(s.a, n, [i, s.prev, i])
            }
        }
    }

    /// The inscribed radius at `p` along `n`, and the widest angle to
    /// another contact. The edges in `skip` meet the outline at `p`.
    fn touch(&self, p: Point2, n: (f64, f64), skip: [usize; 3]) -> Sample {
        let mut best = f64::INFINITY;
        let mut contacts: Vec<(f64, Point2)> = Vec::new();
        let mut work = 0u64;
        for (j, s) in self.segs.iter().enumerate() {
            if skip.contains(&j) {
                continue;
            }
            work += 1;
            let bound = best * (1.0 + TIE) + 1e-9;
            if best.is_finite() && s.misses(Point2::new(p.x + best * n.0, p.y + best * n.1), bound) {
                continue;
            }
            let (f, q) = s.limit(p, n);
            if f <= bound {
                contacts.push((f, q));
                best = best.min(f);
            }
        }
        self.work.set(self.work.get().saturating_add(work));
        if !best.is_finite() {
            return Sample { p, n, r: f64::INFINITY, sep: 0.0 };
        }
        let c = Point2::new(p.x + best * n.0, p.y + best * n.1);
        let bound = best * (1.0 + TIE) + 1e-9;
        let mut sep: f64 = 0.0;
        for (f, q) in contacts {
            if f > bound {
                continue;
            }
            let v = (q.x - c.x, q.y - c.y);
            let len = v.0.hypot(v.1);
            if len > 0.0 {
                let cos = (-(n.0 * v.0 + n.1 * v.1) / len).clamp(-1.0, 1.0);
                sep = sep.max(cos.acos());
            }
        }
        Sample { p, n, r: best, sep }
    }

    /// The distance from `c` to the outline.
    pub(super) fn dist(&self, c: Point2) -> f64 {
        let mut best = f64::INFINITY;
        for s in &self.segs {
            if best.is_finite() && s.misses(c, best) {
                continue;
            }
            best = best.min(s.dist(c));
        }
        self.work.set(self.work.get().saturating_add(self.segs.len() as u64));
        best
    }
}

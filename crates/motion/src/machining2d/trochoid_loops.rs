//! The trochoidal slot's loop layout (issue #220): circular trochoid
//! loops `x(phi) = B cos phi, y(phi) = B sin phi + S phi / 2pi` (Otkur &
//! Lazoglu 2007, "Trochoidal milling", IJMTM 47; Rauch, Duc & Hascoet
//! 2009, IJMTM 49) laid along a centre chain, cut on their FRONT arc
//! only, the return between them lifted. Circular, trochoidal and
//! epicycloidal parameterisations (Prod. Eng. 2021, doi
//! 10.1007/s11740-021-01060-4) differ only in the return; ours is the
//! front arc retraced over the floor.
//!
//! Why front arcs only: the verifier's crossing law holds the post on
//! two Feed moves crossing at a shared depth, and every textbook full
//! circle crosses the loop before it. Each front arc here meets the two
//! walls of the loop band `|v| <= b` at the angle `END_DEG` (its circle
//! has radius `rho = b / cos END_DEG`, centre `rho sin END_DEG` behind
//! the wall points), so it is a graph over the band's width and the arcs
//! of a straight chain are translates of one another: they never cross,
//! whatever the step. The tool reaches the next arc's start by a lead
//! along the wall (the cusp between two loops is cut there), so the
//! walls come out clean. The band's ends are a bored circle at the
//! chain's start and a pencil of arcs through the end's two wall points
//! (tips stepped like the loops) closing the far cap. The layout, the
//! end angle and the cap pencil are ours; no vendor material.

use crate::geo2d::Point2;

use super::face::GenError;

/// The angle (degrees) a front arc meets the band's wall at.
pub(super) const END_DEG: f64 = 10.0;

/// Loops at most (the Prime Directive's "never hang").
pub(super) const MAX_LOOPS: usize = 20_000;

/// One element of a cut, from where the tool stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum El {
    Arc { center: Point2, to: Point2, ccw: bool },
}

/// One loop: the lead's target on the wall (where the next return comes
/// back down), then the cut from there (fillet and front arc).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Piece {
    pub(super) start: Point2,
    pub(super) cut: Vec<El>,
}

impl Piece {
    /// The cut walked backwards from its end: the retraced return.
    pub(super) fn retrace(&self) -> Vec<El> {
        let mut out = Vec::with_capacity(self.cut.len());
        for (i, el) in self.cut.iter().enumerate().rev() {
            let to = if i == 0 { self.start } else { end_of(&self.cut[i - 1]) };
            match *el {
                El::Arc { center, ccw, .. } => out.push(El::Arc { center, to, ccw: !ccw }),
            }
        }
        out
    }

    /// Where the cut ends.
    pub(super) fn end(&self) -> Point2 {
        self.cut.last().map_or(self.start, end_of)
    }
}

fn end_of(el: &El) -> Point2 {
    match *el {
        El::Arc { to, .. } => to,
    }
}

/// The loops of one level.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Layout {
    /// The bore: its centre (the chain's start), its radius `b`, the
    /// wall point the helix starts and ends at, the point opposite it.
    pub(super) c0: Point2,
    pub(super) b: f64,
    pub(super) entry: Point2,
    pub(super) opposite: Point2,
    /// Travel sense of the bore and of every arc (CCW for climb).
    pub(super) ccw: bool,
    pub(super) pieces: Vec<Piece>,
    /// Front-arc loops along the chain (the cap pencil not counted).
    pub(super) loops: usize,
    /// The loop circle's radius and the centre spacing actually used.
    pub(super) rho: f64,
    pub(super) spacing: f64,
}

/// A polyline walked by arc length (open, or wrapped seamlessly when closed).
pub(super) struct Chain<'a> {
    pts: &'a [Point2],
    acc: Vec<f64>,
    closed: bool,
}

impl<'a> Chain<'a> {
    /// `None` when fewer than two points or a zero-length span.
    pub(super) fn new(pts: &'a [Point2]) -> Option<Self> {
        Self::build(pts, false)
    }

    /// A closed loop wrapped continuously across `s = 0` / `s = len`.
    pub(super) fn new_closed(pts: &'a [Point2]) -> Option<Self> {
        if pts.len() < 3 || pts.first().zip(pts.last()).is_none_or(|(a, b)| dist(*a, *b) > 1e-6) {
            return None;
        }
        Self::build(pts, true)
    }

    fn build(pts: &'a [Point2], closed: bool) -> Option<Self> {
        if pts.len() < 2 || pts.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
            return None;
        }
        let mut acc = Vec::with_capacity(pts.len());
        let mut s = 0.0;
        acc.push(0.0);
        for w in pts.windows(2) {
            let l = dist(w[0], w[1]);
            if l <= 1e-9 {
                return None;
            }
            s += l;
            acc.push(s);
        }
        Some(Chain { pts, acc, closed })
    }

    pub(super) fn len(&self) -> f64 {
        self.acc.last().copied().unwrap_or(0.0)
    }

    /// The point at arc length `s` (wrapped when closed, clamped when open).
    pub(super) fn at(&self, s: f64) -> Point2 {
        let len = self.len();
        let s = if self.closed && len > 0.0 { s.rem_euclid(len) } else { s.clamp(0.0, len) };
        let i = self.acc.partition_point(|&a| a <= s).clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let l = self.acc[i] - self.acc[i - 1];
        let t = if l > 0.0 { ((s - self.acc[i - 1]) / l).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    /// The unit direction at `s`: the chord from `s - h` to `s + h`
    /// (a loop faces the way its neighbours advance), the end span's
    /// own where the chord degenerates.
    pub(super) fn dir(&self, s: f64, h: f64) -> Point2 {
        let (a, b) = (self.at(s - h), self.at(s + h));
        let l = dist(a, b);
        if l > 1e-9 {
            return Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
        }
        let n = self.pts.len();
        let (a, b) = (self.pts[n - 2], self.pts[n - 1]);
        let l = dist(a, b).max(1e-12);
        Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
    }

    /// Distance from `p` to the chain.
    pub(super) fn distance(&self, p: Point2) -> f64 {
        self.pts.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min)
    }
}

pub(super) fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, Point2::new(a.x + dx * t, a.y + dy * t))
}

/// A loop's local frame: centre, forward unit, wall-side unit (the side
/// the arcs start from is `-side`).
#[derive(Clone, Copy)]
struct Frame {
    c: Point2,
    t: Point2,
    n: Point2,
}

impl Frame {
    fn new(c: Point2, t: Point2, ccw: bool) -> Self {
        let s = if ccw { 1.0 } else { -1.0 };
        Frame { c, t, n: Point2::new(-t.y * s, t.x * s) }
    }

    fn at(&self, u: f64, v: f64) -> Point2 {
        Point2::new(self.c.x + u * self.t.x + v * self.n.x, self.c.y + u * self.t.y + v * self.n.y)
    }
}

/// One front arc in `f`: the circle of radius `rho` about the frame's
/// centre, from its wall point `(sqrt(rho^2 - b^2), -b)` to `(.., b)`,
/// entered tangentially off the wall by a fillet of radius `fillet`
/// (0: none; clamped under the band's half-width).
fn front(f: &Frame, rho: f64, b: f64, fillet: f64, ccw: bool) -> Piece {
    let u = (rho * rho - b * b).max(0.0).sqrt();
    let end = f.at(u, b);
    let rf = fillet.min(0.9 * b).max(0.0);
    if rf <= 1e-6 || rho - b <= 1e-9 {
        return Piece { start: f.at(u, -b), cut: vec![El::Arc { center: f.c, to: end, ccw }] };
    }
    // The fillet circle: tangent to the wall `v = -b` from inside and to
    // the loop circle from inside (`|F| = rho - rf`).
    let fx = ((rho - rf).powi(2) - (b - rf).powi(2)).max(0.0).sqrt();
    let (cu, cv) = (fx, -b + rf);
    let k = rho / (rho - rf);
    Piece {
        start: f.at(fx, -b),
        cut: vec![
            El::Arc { center: f.at(cu, cv), to: f.at(cu * k, cv * k), ccw },
            El::Arc { center: f.c, to: end, ccw },
        ],
    }
}

/// The loops for a band of half-width `b` along `chain`, about `step`
/// apart (the spacing is the chain's span split evenly, never more than
/// `step`); `None` on a step or band that is no number.
pub(super) fn layout(chain: &Chain, b: f64, step: f64, fillet: f64, climb: bool) -> Option<Layout> {
    if !(b.is_finite() && b > 0.0 && step.is_finite() && step > 0.0 && fillet.is_finite()) {
        return None;
    }
    let ccw = climb;
    let end = END_DEG.to_radians();
    let rho = b / end.cos();
    let back = rho * end.sin();
    let len = chain.len();
    let span = len - back;
    let n = if span > 1e-9 { ((span / step).ceil() as usize).max(1) } else { 0 };
    if n > MAX_LOOPS {
        return None;
    }
    let spacing = if n > 0 { span / n as f64 } else { step };
    let c0 = chain.at(0.0);
    let f0 = Frame::new(c0, chain.dir(0.0, spacing), ccw);
    let mut pieces = Vec::with_capacity(n + 8);
    for k in 1..=n {
        let s = k as f64 * spacing;
        // Each loop's circle sits `back` behind its wall points.
        let f = Frame::new(chain.at(s), chain.dir(s, spacing), ccw);
        pieces.push(front(&f, rho, b, fillet, ccw));
    }
    // The cap pencil: circles through the end's two wall points, tips
    // stepped forward from the last front (or the bore's) to the cap.
    let ce = chain.at(len);
    let fe = Frame::new(ce, chain.dir(len, spacing), ccw);
    let tip0 = if n > 0 { b * (std::f64::consts::FRAC_PI_4 - end * 0.5).tan() } else { b - len };
    let gap = b - tip0;
    if gap > 1e-9 {
        let m = ((gap / spacing.min(step)).ceil() as usize).clamp(1, MAX_LOOPS);
        for j in 1..=m {
            let tip = tip0 + gap * j as f64 / m as f64;
            let e = std::f64::consts::FRAC_PI_2 - 2.0 * (tip / b).atan();
            let rj = b / e.cos();
            let center = fe.at(-rj * e.sin(), 0.0);
            pieces.push(Piece { start: fe.at(0.0, -b), cut: vec![El::Arc { center, to: fe.at(0.0, b), ccw }] });
        }
    }
    Some(Layout { c0, b, entry: f0.at(0.0, -b), opposite: f0.at(0.0, b), ccw, pieces, loops: n, rho, spacing })
}

/// Loops along a closed spine (`chain.closed`), stopping cleanly behind
/// the starting bore circle (`|p - c0| > b + 0.02`) so closure overlaps
/// without crossing.
pub(super) fn layout_closed(chain: &Chain, b: f64, step: f64, fillet: f64, climb: bool) -> Option<Layout> {
    if !(b.is_finite() && b > 0.0 && step.is_finite() && step > 0.0 && fillet.is_finite()) {
        return None;
    }
    let ccw = climb;
    let end = END_DEG.to_radians();
    let rho = b / end.cos();
    let len = chain.len();
    let span = (len - b - rho - 0.1).max(0.0);
    let n = if span > 1e-9 { ((span / step).ceil() as usize).max(1) } else { 0 };
    if n > MAX_LOOPS {
        return None;
    }
    let spacing = if n > 0 { span / n as f64 } else { step };
    let c0 = chain.at(0.0);
    let f0 = Frame::new(c0, chain.dir(0.0, spacing), ccw);
    let mut pieces = Vec::with_capacity(n);
    for k in 1..=n {
        let s = k as f64 * spacing;
        let f = Frame::new(chain.at(s), chain.dir(s, spacing), ccw);
        let pc = front(&f, rho, b, fillet, ccw);
        if k > n / 2 && piece_near(&pc, c0, b + 0.02) {
            break;
        }
        pieces.push(pc);
    }
    let loops = pieces.len();
    Some(Layout { c0, b, entry: f0.at(0.0, -b), opposite: f0.at(0.0, b), ccw, pieces, loops, rho, spacing })
}

fn piece_near(pc: &Piece, c0: Point2, clear: f64) -> bool {
    if dist(pc.start, c0) <= clear || dist(pc.end(), c0) <= clear {
        return true;
    }
    let mut cur = pc.start;
    for el in &pc.cut {
        let El::Arc { center, to, ccw } = *el;
        let (a0, a1) = ((cur.y - center.y).atan2(cur.x - center.x), (to.y - center.y).atan2(to.x - center.x));
        let sweep = if ccw { (a1 - a0).rem_euclid(std::f64::consts::TAU) } else { -((a0 - a1).rem_euclid(std::f64::consts::TAU)) };
        let r = dist(center, to);
        for t in [0.25, 0.5, 0.75] {
            let a = a0 + sweep * t;
            if dist(Point2::new(center.x + r * a.cos(), center.y + r * a.sin()), c0) <= clear {
                return true;
            }
        }
        cur = to;
    }
    false
}

/// Every loop stays inside the cutter-centre band of the channel (a
/// chain turning tighter than the loops is refused, never gouged).
pub(super) fn contained(ch: &Chain, lay: &Layout, room: f64) -> Result<(), GenError> {
    let mut from = lay.entry;
    for pc in &lay.pieces {
        let mut at = pc.start;
        for el in &pc.cut {
            let El::Arc { center, to, ccw } = *el;
            for q in arc_samples(at, to, center, ccw) {
                if ch.distance(q) > room + 1e-6 {
                    return Err(GenError::Invalid(format!(
                        "trochoidal slot: a loop at ({:.3}, {:.3}) would leave the channel - the centre line turns too tightly for loops this wide",
                        q.x, q.y
                    )));
                }
            }
            at = to;
        }
        if ch.distance(pc.start) > room + 1e-6 || ch.distance(from) > room + 1e-6 {
            return Err(GenError::Invalid("trochoidal slot: a wall lead would leave the channel".into()));
        }
        from = pc.start;
    }
    Ok(())
}

/// Points along the arc `a -> b` about `c` (a degree apart at most).
fn arc_samples(a: Point2, b: Point2, c: Point2, ccw: bool) -> Vec<Point2> {
    let (a0, a1) = ((a.y - c.y).atan2(a.x - c.x), (b.y - c.y).atan2(b.x - c.x));
    let tau = std::f64::consts::TAU;
    let sweep = if ccw { (a1 - a0).rem_euclid(tau) } else { -(a0 - a1).rem_euclid(tau) };
    let rad = (a.x - c.x).hypot(a.y - c.y);
    let n = ((sweep.abs().to_degrees()).ceil() as usize).clamp(1, 720);
    (0..=n)
        .map(|k| {
            let t = a0 + sweep * k as f64 / n as f64;
            Point2::new(c.x + rad * t.cos(), c.y + rad * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Vec<Point2> {
        vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0)]
    }

    #[test]
    fn front_arcs_start_and_end_on_the_band_walls_and_step_evenly() {
        let pts = straight();
        let Some(ch) = Chain::new(&pts) else { panic!("chain") };
        let Some(l) = layout(&ch, 4.0, 1.0, 0.0, true) else { panic!("layout") };
        assert!(l.loops >= 39 && l.spacing <= 1.0 + 1e-12, "{} loops at {}", l.loops, l.spacing);
        for p in &l.pieces {
            assert!((p.start.y + 4.0).abs() < 1e-9, "start {:?}", p.start);
            assert!((p.end().y - 4.0).abs() < 1e-9, "end {:?}", p.end());
        }
        // The last piece closes the cap: a half circle about the end.
        let Some(last) = l.pieces.last() else { panic!("pieces") };
        let El::Arc { center, .. } = last.cut[0];
        assert!(dist(center, Point2::new(40.0, 0.0)) < 1e-9, "cap centre {center:?}");
    }

    #[test]
    fn a_fillet_enters_the_arc_tangentially_off_the_wall() {
        let pts = straight();
        let Some(ch) = Chain::new(&pts) else { panic!("chain") };
        let Some(l) = layout(&ch, 4.0, 1.0, 1.0, true) else { panic!("layout") };
        let p = &l.pieces[3];
        let El::Arc { center: fc, to: t2, .. } = p.cut[0];
        // Tangent to the wall at the start, to the loop circle at t2.
        assert!((dist(fc, p.start) - 1.0).abs() < 1e-9 && (fc.x - p.start.x).abs() < 1e-9);
        let El::Arc { center: c, .. } = p.cut[1];
        assert!((dist(c, t2) - l.rho).abs() < 1e-9 && (dist(c, fc) - (l.rho - 1.0)).abs() < 1e-9);
        // The retrace walks back to the start.
        let back = p.retrace();
        let Some(El::Arc { to, .. }) = back.last().copied() else { panic!("retrace") };
        assert_eq!(to, p.start);
    }
}

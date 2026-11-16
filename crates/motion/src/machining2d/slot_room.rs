//! Slot fit to a picked selection (#1504): the room a slot may cut in -
//! the strip two opposite straight edges bound over their shared length,
//! or a picked face's own region - the slot that fills that room, and
//! the refusal law for a slot that does not fit it. One law for every
//! slot kind that is authored by two endpoints and a width (the plain
//! slot here, the trochoidal slot of #1503).
//!
//! Provenance: our own plane geometry - point-to-segment distance and
//! the minimum-area enclosing rectangle by rotating calipers over the
//! convex hull (G. Toussaint, "Solving geometric problems with the
//! rotating calipers", 1983); no vendor material consulted.

use crate::geo2d::{Point2, Region};

use super::face::GenError;

#[path = "slot_room_face.rs"]
mod geom;
use geom::{face_overshoot, hull};

/// How far a picked edge may stray from its chord, or the second edge
/// from parallel to the first, mm.
pub const STRAIGHT_TOL: f64 = 0.01;
/// How far a slot may stand past the room its picks bound and still
/// fit, mm - the gouge law's own tolerance, so a tessellated round end
/// of a picked face (its chords a sag inside the true arc) is no wall.
pub const FIT_TOL: f64 = 0.01;
/// The most a face's fitted slot is drawn in PER SIDE to sit inside the
/// face, as a fraction of the room's width (so the width can lose up to
/// twice this, 10%): a tessellated round end's chord sag
/// (0.039 mm at 8 chords per quarter of an 8 mm radius, 0.5% of the
/// width) is inscribed; a face whose best slot overshoots by more is no
/// slot face (an L, a notch) and is refused, never shrunk into one.
const INSCRIBE_MAX: f64 = 0.05;

/// The slot that fills a bound: endpoints and width, the numbers a
/// slot operation is authored by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotFit {
    pub a: Point2,
    pub b: Point2,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Room {
    /// The strip between two picked edges: the frame's rectangle is all
    /// the room there is.
    Edges,
    /// A picked face (or closed ring): its region, inside the frame's
    /// minimum-area rectangle.
    Face(Region),
}

/// Where a selection lets a slot cut: a rectangle frame - `u` along the
/// slot, `n` across it, `s` in `[s0, s1]` along and `t` in `[t0, t1]`
/// across (projections of world XY) - plus, for a face, its region.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotBound {
    room: Room,
    u: Point2,
    n: Point2,
    s0: f64,
    s1: f64,
    t0: f64,
    t1: f64,
}

fn dot(p: Point2, d: Point2) -> f64 {
    p.x * d.x + p.y * d.y
}

fn at(u: Point2, n: Point2, s: f64, t: f64) -> Point2 {
    Point2::new(u.x * s + n.x * t, u.y * s + n.y * t)
}

fn finite(pts: &[Point2]) -> bool {
    pts.iter().all(|p| p.x.is_finite() && p.y.is_finite())
}

fn span(pts: &[Point2], d: Point2) -> (f64, f64) {
    pts.iter().map(|&p| dot(p, d)).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

impl SlotBound {
    /// The strip between two picked edges (XY polylines in pick order):
    /// both straight, parallel, apart, and facing each other over a
    /// shared length. Anything else is refused by name - two edges that
    /// do not bound a slot are never guessed into one.
    ///
    /// # Errors
    /// An edge that is not straight, edges that are not parallel, lie
    /// on one line, or share no length.
    pub fn from_edges(e1: &[Point2], e2: &[Point2]) -> Result<SlotBound, GenError> {
        if e1.len() < 2 || e2.len() < 2 || !finite(e1) || !finite(e2) {
            return Err(GenError::Invalid("a slot's picked edges need two points each".into()));
        }
        let (p, q) = (e1[0], e1[e1.len() - 1]);
        let len = (q.x - p.x).hypot(q.y - p.y);
        if len.is_nan() || len <= STRAIGHT_TOL {
            return Err(GenError::Invalid("the first picked edge has no length in XY - pick the edges along the slot".into()));
        }
        let u = Point2::new((q.x - p.x) / len, (q.y - p.y) / len);
        let n = Point2::new(-u.y, u.x);
        let (a0, a1) = span(e1, n);
        if a1 - a0 > STRAIGHT_TOL {
            return Err(GenError::Invalid(format!(
                "the first picked edge is not straight (it bows {:.3} mm off its chord) - a slot is picked by two straight opposite edges or one face",
                a1 - a0
            )));
        }
        let (b0, b1) = span(e2, n);
        if b1 - b0 > STRAIGHT_TOL {
            return Err(GenError::Invalid(format!(
                "the second picked edge is not straight and parallel to the first (it wanders {:.3} mm across the slot) - pick two opposite sides of the slot",
                b1 - b0
            )));
        }
        let d = 0.5 * (b0 + b1) - 0.5 * (a0 + a1);
        if d.abs() <= STRAIGHT_TOL {
            return Err(GenError::Invalid("the two picked edges lie on one line - pick two opposite sides of the slot".into()));
        }
        let (sa0, sa1) = span(e1, u);
        let (sb0, sb1) = span(e2, u);
        let (s0, s1) = (sa0.max(sb0), sa1.min(sb1));
        if s1 - s0 <= STRAIGHT_TOL {
            return Err(GenError::Invalid(
                "the two picked edges do not face each other - they share no length along the slot".into(),
            ));
        }
        // The narrowest strip both edges allow (their straightness
        // spread taken inward).
        let (t0, t1) = if d > 0.0 { (a1, b0) } else { (b1, a0) };
        Ok(SlotBound { room: Room::Edges, u, n, s0, s1, t0, t1 })
    }

    /// A picked face's (or closed ring's) region: the room is the region
    /// itself, framed by its minimum-area enclosing rectangle with `u`
    /// along the long side.
    ///
    /// # Errors
    /// An empty or degenerate region.
    pub fn from_face(region: Region) -> Result<SlotBound, GenError> {
        let pts: Vec<Point2> = region.loops().iter().flat_map(|l| l.points().iter().copied()).collect();
        if region.is_empty() || !finite(&pts) || region.area().abs().is_nan() || region.area().abs() <= STRAIGHT_TOL * STRAIGHT_TOL {
            return Err(GenError::Invalid("the picked face bounds no area in XY - pick a floor face or two opposite edges".into()));
        }
        let h = hull(&pts);
        let mut best: Option<(f64, Point2)> = None;
        for i in 0..h.len() {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len.is_nan() || len <= 0.0 {
                continue;
            }
            let u = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
            let n = Point2::new(-u.y, u.x);
            let ((s0, s1), (t0, t1)) = (span(&h, u), span(&h, n));
            let area = (s1 - s0) * (t1 - t0);
            if best.is_none_or(|(b, _)| area < b) {
                best = Some((area, u));
            }
        }
        let Some((_, mut u)) = best else {
            return Err(GenError::Invalid("the picked face bounds no area in XY - pick a floor face or two opposite edges".into()));
        };
        let n0 = Point2::new(-u.y, u.x);
        let ((s0, s1), (t0, t1)) = (span(&h, u), span(&h, n0));
        if t1 - t0 > s1 - s0 {
            u = n0;
        }
        let n = Point2::new(-u.y, u.x);
        let ((s0, s1), (t0, t1)) = (span(&h, u), span(&h, n));
        Ok(SlotBound { room: Room::Face(region), u, n, s0, s1, t0, t1 })
    }

    fn picks(&self) -> &'static str {
        match self.room {
            Room::Edges => "the picked edges",
            Room::Face(_) => "the picked face",
        }
    }

    fn leave(&self) -> &'static str {
        match self.room {
            Room::Edges => "leave",
            Room::Face(_) => "leaves",
        }
    }

    /// The room's length along the slot and width across it, mm.
    #[must_use]
    pub fn room(&self) -> (f64, f64) {
        (self.s1 - self.s0, self.t1 - self.t0)
    }

    /// The slot that fills the room: the full width across, round ends
    /// tangent to the room's ends. For a face, the slot that fits it -
    /// a face no straight slot fills (an L, a disk) is refused by name.
    ///
    /// # Errors
    /// A room no longer than it is wide, or a face its best slot leaves.
    pub fn fit(&self) -> Result<SlotFit, GenError> {
        let (l, w) = self.room();
        if w.is_nan() || w <= FIT_TOL {
            return Err(GenError::Invalid(format!("{} {} no width for a slot", self.picks(), self.leave())));
        }
        if l - w <= FIT_TOL {
            return Err(GenError::Invalid(format!(
                "{} {} room {:.3} mm long for a {:.3} mm wide slot - a slot needs length past its width for two distinct ends (machine a round pocket as Circular)",
                self.picks(),
                self.leave(),
                l,
                w
            )));
        }
        let tm = 0.5 * (self.t0 + self.t1);
        let mut fit = SlotFit {
            a: at(self.u, self.n, self.s0 + 0.5 * w, tm),
            b: at(self.u, self.n, self.s1 - 0.5 * w, tm),
            width: w,
        };
        // A face's tessellated round end stands a chord sag inside the
        // true arc the frame spans: the slot is inscribed - its radius
        // drawn in by the measured overshoot, which offsets the whole
        // capsule inward by it (both ends and both sides) - so the fit
        // sits inside the chords. Never the tolerance loosened into the
        // model; a larger overshoot is no slot face and is refused below.
        if let Room::Face(region) = &self.room {
            for _ in 0..2 {
                let out = face_overshoot(region, fit.a, fit.b, 0.5 * fit.width)?;
                if out <= FIT_TOL || out > INSCRIBE_MAX * w {
                    break;
                }
                fit.width -= 2.0 * out;
            }
        }
        self.confine(fit.a, fit.b, fit.width).map_err(|e| {
            GenError::Invalid(format!(
                "{} holds no straight slot: its best fit, {:.3} mm long and {:.3} mm wide, fails - {}",
                self.picks(),
                l,
                w,
                bare(&e)
            ))
        })?;
        Ok(fit)
    }

    /// [`confine`](Self::confine) as the user meets it: a slot that does
    /// not fit is refused, and when the picks themselves bound no slot
    /// (so they seeded nothing and the typed numbers stood) the refusal
    /// also names why - never only the typed slot's length.
    ///
    /// # Errors
    /// The slot does not fit; the picks' own refusal is appended.
    pub fn judge(&self, a: Point2, b: Point2, width: f64) -> Result<(), GenError> {
        let Err(e) = self.confine(a, b, width) else {
            return Ok(());
        };
        match self.fit() {
            Ok(_) => Err(e),
            Err(f) => Err(GenError::Invalid(format!("{} (the picks seed no slot: {})", bare(&e), bare(&f)))),
        }
    }

    /// The refusal law: the slot from `a` to `b` of `width` must lie
    /// inside the room, within [`FIT_TOL`]. A slot too wide, too long or
    /// placed past the picks is refused by name with its numbers - never
    /// generated across a wall.
    ///
    /// # Errors
    /// The slot does not fit, or its numbers are not finite.
    pub fn confine(&self, a: Point2, b: Point2, width: f64) -> Result<(), GenError> {
        if !(finite(&[a, b]) && width.is_finite() && width > 0.0) {
            return Err(GenError::Invalid("slot endpoints and width must be finite, the width positive".into()));
        }
        let no = |why: String| Err(GenError::Invalid(format!("the slot does not fit the selection: {why}")));
        let r = 0.5 * width;
        let (sa, sb, ta, tb) = (dot(a, self.u), dot(b, self.u), dot(a, self.n), dot(b, self.n));
        let (l, w) = self.room();
        let across = (tb - ta).abs() + width;
        let along = (sb - sa).abs() + width;
        let picks = self.picks();
        if across > w + FIT_TOL {
            let room = match self.room {
                Room::Edges => format!("{picks} stand only {w:.3} mm apart"),
                Room::Face(_) => format!("{picks} is only {w:.3} mm wide"),
            };
            return no(format!("it is {across:.3} mm wide across the slot, but {room} - narrow it or re-pick"));
        }
        if along > l + FIT_TOL {
            let room = match self.room {
                Room::Edges => format!("{picks} share only {l:.3} mm"),
                Room::Face(_) => format!("{picks} is only {l:.3} mm long"),
            };
            return no(format!("it is {along:.3} mm long end to end, but {room} - shorten it or re-pick"));
        }
        let out = match &self.room {
            Room::Edges => [(sa, ta), (sb, tb)]
                .iter()
                .map(|&(s, t)| (self.s0 - (s - r)).max(s + r - self.s1).max(self.t0 - (t - r)).max(t + r - self.t1))
                .fold(0.0_f64, f64::max),
            Room::Face(region) => face_overshoot(region, a, b, r)?,
        };
        if out > FIT_TOL {
            return no(format!(
                "it runs {out:.3} mm outside {picks} - move its ends A ({:.3}, {:.3}) and B ({:.3}, {:.3}) inside",
                a.x, a.y, b.x, b.y
            ));
        }
        Ok(())
    }
}

/// A refusal's sentence without the error's own prefix.
fn bare(e: &GenError) -> String {
    let s = e.to_string();
    s.strip_prefix("invalid machining parameters: ").map(str::to_owned).unwrap_or(s)
}

#[cfg(test)]
#[path = "slot_room_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "slot_room_pins.rs"]
mod pins;

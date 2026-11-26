//! The user's start and exit points on the waterline's closed rings
//! (#3330, the 5-Axis Contour slice of #1448). A start point sets the
//! seam of every closed ring: each ring is re-sequenced to begin at the
//! point of the ring nearest the pick (in XY), so the seams line up
//! toward the pick and the first ring starts at the pick's projection.
//! An exit point lets every closed ring cut its whole lap and then run
//! on along its own polyline to the point nearest the exit pick, where
//! the tool leaves. The cut geometry does not change: the same ring
//! points are cut, only the order and a re-trace over cut material.
//! What the pick cannot keep is refused by name, never moved: a pick
//! farther than its reach from every ring, a ladder with no closed ring,
//! and a plunge or retract column at the pick that crosses the part
//! (the ring's own column law, `support::column_clear`).
//!
//! Provenance: first principles - the closest point on a segment
//! (orthogonal projection clamped to the segment) and the rotation of a
//! closed polyline's vertex list, as the 2D contour's start point
//! (`machining2d::contour_start`). The seam position is the common
//! contour-entry vocabulary of published CAM practice. No vendor
//! material consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::path_picks::{PathPicks, PickRole};

use super::ladder::levels;
use super::params::{dirs, validate, Finish3DParams};

/// The start and exit points a waterline ladder reads, in the
/// generator's XY. Both `None` is the automatic seam of every earlier
/// path, byte for byte.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RingSeam {
    /// Each closed ring begins at its point nearest this pick.
    pub start: Option<Point2>,
    /// Each closed ring, after its lap, ends at its point nearest this pick.
    pub exit: Option<Point2>,
}

impl RingSeam {
    /// The seam of an operation's picks: the first start and the first
    /// exit, XY only (the ring ladder reads no pick Z: each level has
    /// its own ring, and the pick sets where on it the cut begins).
    #[must_use]
    pub fn of(picks: &PathPicks) -> Self {
        Self {
            start: picks.first(PickRole::Start).map(|p| p.xy),
            exit: picks.first(PickRole::Exit).map(|p| p.xy),
        }
    }

    /// No pick of either role.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.is_none() && self.exit.is_none()
    }
}

/// The farthest a pick may sit from the nearest ring of ball centres:
/// one and a half tool diameters, the 2D contour's slack for a click
/// beside the wall.
fn reach(tool_diameter: f64) -> f64 {
    tool_diameter * 1.5
}

/// "start point (x, y)" or "exit point (x, y)", the refusals' lead.
fn named(role: PickRole, q: Point2) -> String {
    format!("{} point ({:.3}, {:.3})", role.word(), q.x, q.y)
}

/// The closest point to `q` on the closed ring `pts` (the last vertex
/// wraps to the first): `(segment index, point, distance)`, the first
/// segment winning a tie. `None` for an empty ring or no finite answer.
/// A foot at the end of its segment (`t` at 1, within rounding) is
/// stated as the start of the next segment: the closing vertex is then
/// segment 0, so the run-on never re-traces a whole lap for it.
pub(super) fn nearest(pts: &[Point2], q: Point2) -> Option<(usize, Point2, f64)> {
    let n = pts.len();
    let mut best: Option<(usize, Point2, f64)> = None;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let (k, f) = if t > 1.0 - 1e-9 { ((i + 1) % n, b) } else { (i, Point2::new(a.x + dx * t, a.y + dy * t)) };
        let d = ((q.x - f.x).powi(2) + (q.y - f.y).powi(2)).sqrt();
        if d.is_finite() && best.is_none_or(|b| d < b.2) {
            best = Some((k, f, d));
        }
    }
    best
}

/// Drops each vertex that repeats its predecessor (a projection that
/// lands on a vertex), so no zero-length move is fitted.
fn dedupe(pts: &mut Vec<Point2>) {
    pts.dedup_by(|b, a| (a.x - b.x).abs() <= 1e-12 && (a.y - b.y).abs() <= 1e-12);
}

/// One ladder's walk of its picks: the seam it reads and the nearest
/// distance each pick found to a closed ring, for the reach law.
pub(super) struct SeamRun<'a> {
    seam: &'a RingSeam,
    best: [f64; 2],
}

impl<'a> SeamRun<'a> {
    pub(super) fn new(seam: &'a RingSeam) -> Self {
        Self { seam, best: [f64::INFINITY; 2] }
    }

    /// Does the run state a start point? The ring loop then refuses a
    /// blocked plunge column by name instead of moving the entry.
    pub(super) fn start(&self) -> Option<Point2> {
        self.seam.start
    }

    /// Re-sequences the closed ring `poly` (`poly[n] == poly[0]`) to
    /// begin at its point nearest the start pick. `false` when there is
    /// no start pick or the ring is too short to re-sequence.
    pub(super) fn seat(&mut self, poly: &mut Vec<Point2>) -> bool {
        let Some(q) = self.seam.start else { return false };
        let n = poly.len().saturating_sub(1);
        if n < 2 {
            return false;
        }
        let Some((i, f, d)) = nearest(&poly[..n], q) else { return false };
        self.best[0] = self.best[0].min(d);
        let mut ring = Vec::with_capacity(n + 2);
        ring.push(f);
        ring.extend((1..=n).map(|k| poly[(i + k) % n]));
        ring.push(f);
        dedupe(&mut ring);
        *poly = ring;
        true
    }

    /// Carries the closed ring `poly` (seam to seam) on along its own
    /// vertices to its point nearest the exit pick. Answers that point,
    /// where the tool leaves; `None` (and `poly` unchanged) without an
    /// exit pick or for a ring too short to walk.
    pub(super) fn run_on(&mut self, poly: &mut Vec<Point2>) -> Option<Point2> {
        let q = self.seam.exit?;
        let n = poly.len().saturating_sub(1);
        if n < 2 {
            return None;
        }
        let (i, f, d) = nearest(&poly[..n], q)?;
        self.best[1] = self.best[1].min(d);
        let lap: Vec<Point2> = poly[1..=i].to_vec();
        poly.extend(lap);
        poly.push(f);
        dedupe(poly);
        Some(f)
    }

    /// The by-name refusal of a blocked plunge column at a picked start.
    pub(super) fn blocked_start(&self, z_tip: f64) -> GenError {
        let q = self.seam.start.unwrap_or(Point2::new(0.0, 0.0));
        GenError::Invalid(format!(
            "{}: the plunge column to the ring at z {z_tip:.3} crosses the part there - pick another \
             point or clear the start point",
            named(PickRole::Start, q)
        ))
    }

    /// The by-name refusal of a blocked retract column at a picked exit.
    pub(super) fn blocked_exit(&self, z_tip: f64) -> GenError {
        let q = self.seam.exit.unwrap_or(Point2::new(0.0, 0.0));
        GenError::Invalid(format!(
            "{}: the retract column from the ring at z {z_tip:.3} crosses the part there - pick another \
             point or clear the exit point",
            named(PickRole::Exit, q)
        ))
    }

    /// The reach law, read once the ladder is walked: each pick must
    /// sit within [`reach`] of at least one closed ring. Refused by name.
    pub(super) fn verdict(&self, tool_diameter: f64) -> Result<(), GenError> {
        let lim = reach(tool_diameter);
        for (k, (role, q)) in [(PickRole::Start, self.seam.start), (PickRole::Exit, self.seam.exit)].into_iter().enumerate() {
            let Some(q) = q else { continue };
            let d = self.best[k];
            if d.is_infinite() {
                return Err(GenError::Invalid(format!(
                    "{}: the ladder has no closed ring to {} on - clear the {} point",
                    named(role, q),
                    role.word(),
                    role.word()
                )));
            }
            if d.is_nan() || d > lim {
                return Err(GenError::Invalid(format!(
                    "{} is {d:.3} mm from the nearest ring of the ladder, farther than the {lim:.3} mm \
                     a {} point may sit from it - pick on or beside the surface to cut",
                    named(role, q),
                    role.word()
                )));
            }
        }
        Ok(())
    }
}

/// The waterline: the ring ladder alone. One closed cut at each level
/// of the scallop stepdown ladder, wherever a ball of radius `r`
/// resting tip-deep at that Z touches the surface. (Moved from
/// `rings.rs` by #3330; the same body, with no pick.)
pub fn waterline(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    waterline_seamed(facets, p, &RingSeam::default())
}

/// The waterline with the user's start and exit points (#3330): the
/// ring ladder of [`super::waterline`], each closed ring seated at the
/// start pick and carried on to the exit pick. An empty `seam` is the
/// waterline itself, byte for byte.
pub fn waterline_seamed(facets: &[Triangle], p: &Finish3DParams, seam: &RingSeam) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    if !p.waterlines {
        return Err(GenError::Invalid(
            "Waterline 3D finish is the waterline ring pass - enable waterlines or pick a shallow-first pattern".into(),
        ));
    }
    let r = p.tool_diameter * 0.5;
    let zs = levels(p, r, facets)?;
    let ds = dirs(r, p.tolerance);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut last: Option<f64> = None;
    let mut run = (!seam.is_empty()).then(|| SeamRun::new(seam));
    // #1015: a level refused part-way hands back the rings above it.
    let any = super::rings::emit_rings_seamed(&mut b, p, facets, &zs, &ds, &mut last, true, run.as_mut())
        .map_err(|e| e.with_ghost_moves(b.moves()))?;
    if !any {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    if let Some(run) = &run {
        run.verdict(p.tool_diameter)?;
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("waterline path left the finite horizon".into()));
    }
    Ok(path)
}

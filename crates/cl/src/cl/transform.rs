//! Rigid toolpath transforms (#3011): translate, turn in XY, turn about
//! the A or B axis, mirror across a vertical plane.
//!
//! Provenance: elementary Euclidean rigid motions and reflections; the
//! G2 <-> G3 and G41 <-> G42 swap under a reflection is the RS-274 /
//! ISO 6983 direction rule, the one `post::invert` applies to a
//! machine's inverted axis. Our own model; no vendor material consulted.
//!
//! Every transform is one map `p -> M p + t` with `M` orthogonal
//! ([`Rigid`]). A path maps move for move, so the index records (cycle
//! `first_move`, compensated spans, torch spans) stay valid, except
//! where a tilt flattens arcs ([`rotate_about_axis`]); there the index
//! records that cannot follow are refused or dropped with a note.
//! Moves, arc centers and cycles land on the 1 um [`lattice`]; edge
//! break polylines do not, because a snap would bend their framed caps
//! off orthonormal. A record a rigid map cannot carry is refused by name
//! ([`TransformError`]), never guessed.

use std::fmt;

use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::comp::{CompSide, CompSpan};
use super::cycle::{Cycle, CycleKind};
use super::edge_break::EdgeBreak;
use super::moves::{lattice, ArcMove, Move};
use super::path::Toolpath;
use super::pose::{is_vertical, normalize_axis};

#[path = "transform_climb.rs"]
mod climb;
#[path = "transform_mirror.rs"]
mod mirror;
#[path = "transform_rotary.rs"]
mod rotary;

pub use mirror::{mirror_plane, MirrorMode, MirrorPlane, PassFamily};
pub use rotary::{rotate_about_axis, RotaryAxis};

/// Why a transform refused a path. Each names the record or the input
/// it cannot carry; the path is never transformed in part.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformError {
    /// A non-finite or out-of-range input (offset, angle, plane,
    /// tolerance), or a result that left the finite range.
    BadInput(&'static str),
    /// A turning thread cycle: lathe intent, not a milling map.
    ThreadCycle,
    /// A probing touch: it writes a work offset from its own target.
    ProbeTouch,
    /// A move with a stated spindle C angle (C-axis live tooling).
    RotaryMove,
    /// A tilted (A/B) move under a mirror.
    TiltedMove,
    /// A plasma statement under a mirror or a tilt: its kerf side and
    /// lit spans do not follow.
    PlasmaCut,
    /// An XZ / YZ arc whose plane the transform does not keep.
    VerticalArc,
    /// A canned drilling cycle turned off the Z axis.
    CannedCycleTilt,
    /// A fine-boring cycle under an XY turn or a mirror: its bottom shift
    /// follows the machine's oriented spindle stop, not the path.
    FineBoreShift,
    /// Arc subdivision and coordinate rounding cannot meet `chord_tol`.
    ArcChordTolerance,
    /// A mirror plane that flips Z.
    HorizontalMirrorPlane,
    /// A mirror plane that is neither vertical nor horizontal.
    TiltedMirrorPlane,
    /// `PreserveClimb` cannot reverse this path safely; the text says why.
    NotReversible(String),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match self {
            TransformError::BadInput(what) => return write!(f, "toolpath transform refused: {what} is not finite or out of range"),
            TransformError::NotReversible(why) => return write!(f, "toolpath mirror refused: cannot keep the climb cut: {why}"),
            TransformError::ThreadCycle => "a turning thread cycle does not ride a milling transform",
            TransformError::ProbeTouch => "a probing touch writes a work offset; a transform does not move it",
            TransformError::RotaryMove => "a move with a stated C angle (C-axis live tooling) does not ride a transform",
            TransformError::TiltedMove => "a tilted (A/B) move cannot be mirrored",
            TransformError::PlasmaCut => "a plasma cut's kerf side and torch spans do not follow a mirror or a tilt",
            TransformError::VerticalArc => "an XZ / YZ arc does not keep its plane under this transform",
            TransformError::CannedCycleTilt => "a canned drilling cycle cannot turn off the Z axis",
            TransformError::FineBoreShift => {
                "a fine-boring cycle's bottom shift is fixed to the machine by the oriented spindle stop: it does not turn or mirror"
            }
            TransformError::ArcChordTolerance => "the arc subdivision limit or coordinate rounding cannot meet the chord tolerance",
            TransformError::HorizontalMirrorPlane => {
                "a horizontal mirror plane flips Z: the tool would stand on the wrong side of its tip"
            }
            TransformError::TiltedMirrorPlane => "only a vertical mirror plane (normal with no Z part) is supported",
        };
        write!(f, "toolpath transform refused: {why}")
    }
}

impl std::error::Error for TransformError {}

/// Shift every move, arc center, cycle, compensated span and edge break
/// by `d = (dx, dy, dz)`.
pub fn translate(tp: &Toolpath, d: [f64; 3]) -> Result<Toolpath, TransformError> {
    if !d.iter().all(|v| v.is_finite()) {
        return Err(TransformError::BadInput("the translation"));
    }
    refuse_records(tp)?;
    map_path(tp, &Rigid { m: IDENTITY, t: d })
}

/// Turn the path in XY by `angle_deg` (counter-clockwise seen from +Z)
/// about `pivot`, then shift it by `(tx, ty)`. Z is unchanged. An XZ /
/// YZ arc keeps its plane only at a half or a whole turn; at other
/// angles it is refused ([`TransformError::VerticalArc`]).
pub fn transform_xy(tp: &Toolpath, tx: f64, ty: f64, angle_deg: f64, pivot: Point2) -> Result<Toolpath, TransformError> {
    if ![tx, ty, angle_deg, pivot.x, pivot.y].iter().all(|v| v.is_finite()) {
        return Err(TransformError::BadInput("the XY transform"));
    }
    refuse_records(tp)?;
    let (s, c) = sin_cos_deg(angle_deg);
    let m = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
    let o = [pivot.x, pivot.y, 0.0];
    let t = Rigid::pivot_offset(&m, o, [tx, ty, 0.0]);
    map_path(tp, &Rigid { m, t })
}

const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Sine and cosine of an angle in degrees, exact at the quarter turns
/// so a 90 / 180 / 270 degree map keeps the lattice bit for bit.
fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let r = deg.rem_euclid(360.0);
    [(0.0, (0.0, 1.0)), (90.0, (1.0, 0.0)), (180.0, (0.0, -1.0)), (270.0, (-1.0, 0.0))]
        .into_iter()
        .find_map(|(q, sc)| (r == q).then_some(sc))
        .unwrap_or_else(|| r.to_radians().sin_cos())
}

fn snap(p: Point3) -> Point3 {
    Point3::new(lattice(p.x), lattice(p.y), lattice(p.z))
}

/// The records no transform carries, and a path that is not finite.
fn refuse_records(tp: &Toolpath) -> Result<(), TransformError> {
    if !tp.all_finite() {
        return Err(TransformError::BadInput("the input path"));
    }
    if !tp.threads.is_empty() {
        return Err(TransformError::ThreadCycle);
    }
    if !tp.probes.is_empty() {
        return Err(TransformError::ProbeTouch);
    }
    if tp.moves.iter().any(|m| m.rotary.is_some()) {
        return Err(TransformError::RotaryMove);
    }
    Ok(())
}

/// The map `p -> m p + t`: `m` orthogonal (rows), `t` the offset.
#[derive(Clone, Copy, Debug)]
struct Rigid {
    m: [[f64; 3]; 3],
    t: [f64; 3],
}

impl Rigid {
    /// The offset that turns `m` about `pivot` and then shifts by
    /// `shift`: `pivot + shift - m pivot`.
    fn pivot_offset(m: &[[f64; 3]; 3], pivot: [f64; 3], shift: [f64; 3]) -> [f64; 3] {
        let r = Rigid { m: *m, t: [0.0; 3] }.dir(pivot);
        std::array::from_fn(|i| pivot[i] + shift[i] - r[i])
    }

    fn dir(&self, d: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| self.m[i][0] * d[0] + self.m[i][1] * d[1] + self.m[i][2] * d[2])
    }

    fn raw(&self, p: Point3) -> Point3 {
        let r = self.dir([p.x, p.y, p.z]);
        Point3::new(r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2])
    }

    fn point(&self, p: Point3) -> Point3 {
        snap(self.raw(p))
    }

    fn det(&self) -> f64 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// True when `+Z` maps exactly onto `+Z`: no tilt, cycles and
    /// vertical edge-break cones keep their meaning.
    fn keeps_up(&self) -> bool {
        self.dir([0.0, 0.0, 1.0]) == [0.0, 0.0, 1.0]
    }

    /// The arc under the map, or None when its plane is not kept. The
    /// plane is kept when its normal maps exactly onto plus or minus
    /// itself; the turning sense then flips when the in-plane part of
    /// the map is a reflection: `det(m)` times the normal's sign.
    fn arc(&self, a: ArcMove) -> Option<ArcMove> {
        let n = a.plane.point(0.0, 0.0, 1.0);
        let n = [n.x, n.y, n.z];
        let mn = self.dir(n);
        let sign = if mn == n {
            1.0
        } else if mn == n.map(|v| -v) {
            -1.0
        } else {
            return None;
        };
        let (cu, cv) = a.plane.center_uv(a.center);
        let c = a.plane.words(self.raw(a.plane.point(cu, cv, 0.0)));
        let flip = (self.det() < 0.0) != (sign < 0.0);
        Some(ArcMove { center: Point2::new(lattice(c.x), lattice(c.y)), ccw: a.ccw != flip, plane: a.plane })
    }

    /// The pose under the map. No stated pose stays none while `+Z`
    /// stays vertical; a tilt states the turned `+Z`.
    fn axis(&self, a: Option<[f64; 3]>) -> Result<Option<[f64; 3]>, TransformError> {
        let turned = |v: [f64; 3]| normalize_axis(self.dir(v)).ok_or(TransformError::BadInput("a turned tool axis"));
        match a {
            Some(v) => turned(v).map(Some),
            None => {
                let up = turned([0.0, 0.0, 1.0])?;
                Ok((!is_vertical(&up)).then_some(up))
            }
        }
    }

    fn straight(&self, m: &Move, from: Point3, to: Point3) -> Result<Move, TransformError> {
        Ok(Move { from: self.point(from), to: self.point(to), arc: None, axis: self.axis(m.axis)?, ..*m })
    }

    fn map_move(&self, m: &Move) -> Result<Move, TransformError> {
        let arc = match m.arc {
            None => None,
            Some(a) => Some(self.arc(a).ok_or(TransformError::VerticalArc)?),
        };
        Ok(Move { arc, ..self.straight(m, m.from, m.to)? })
    }

    fn map_cycle(&self, c: &Cycle) -> Result<Cycle, TransformError> {
        if !self.keeps_up() {
            return Err(TransformError::CannedCycleTilt);
        }
        // The shift rapid at the bottom moves off the wall in the
        // direction the oriented stop fixes; turned or mirrored, it would
        // drive the stopped tip into the finished wall. Only a shift keeps it.
        if c.kind == CycleKind::FineBore && self.m != IDENTITY {
            return Err(TransformError::FineBoreShift);
        }
        let xy = self.point(Point3::new(c.xy.x, c.xy.y, 0.0));
        let dz = self.t[2];
        Ok(Cycle {
            xy: Point2::new(xy.x, xy.y),
            r_z: lattice(c.r_z + dz),
            bottom_z: lattice(c.bottom_z + dz),
            entry_z: lattice(c.entry_z + dz),
            ..*c
        })
    }

    /// A compensated span: its moves map like the path's; a reflection
    /// puts the cutter on the other side (G41 <-> G42).
    fn map_comp(&self, c: &CompSpan) -> Result<CompSpan, TransformError> {
        let side = if self.det() < 0.0 { swap_side(c.side) } else { c.side };
        let programmed = c.programmed.iter().map(|m| self.map_move(m)).collect::<Result<_, _>>()?;
        Ok(CompSpan { side, programmed, ..c.clone() })
    }

    /// An edge break, or None for a legacy vertical cone under a tilt
    /// (it then declares nothing, as in a tilted frame).
    fn map_break(&self, b: &EdgeBreak) -> Option<EdgeBreak> {
        if b.volume.is_none() && !self.keeps_up() {
            return None;
        }
        let mut out = EdgeBreak { edge: b.edge.iter().map(|p| self.raw(*p)).collect(), ..b.clone() };
        if let Some(v) = &mut out.volume {
            for f in &mut v.frames {
                f.outward = self.dir(f.outward);
                f.across = self.dir(f.across);
            }
        }
        Some(out)
    }
}

fn swap_side(s: CompSide) -> CompSide {
    match s {
        CompSide::Left => CompSide::Right,
        CompSide::Right => CompSide::Left,
    }
}

/// Map a path move for move: every index record stays valid.
fn map_path(tp: &Toolpath, r: &Rigid) -> Result<Toolpath, TransformError> {
    let moves = tp.moves.iter().map(|m| r.map_move(m)).collect::<Result<_, _>>()?;
    let cycles = tp.cycles.iter().map(|c| r.map_cycle(c)).collect::<Result<_, _>>()?;
    let comps = tp.comps.iter().map(|c| r.map_comp(c)).collect::<Result<_, _>>()?;
    let mut notes = tp.notes.clone();
    let breaks = map_breaks(tp, r, &mut notes);
    checked(Toolpath { moves, cycles, notes, comps, threads: Vec::new(), probes: Vec::new(), breaks, plasma: tp.plasma.clone() })
}

fn map_breaks(tp: &Toolpath, r: &Rigid, notes: &mut Vec<String>) -> Vec<EdgeBreak> {
    let breaks: Vec<EdgeBreak> = tp.breaks.iter().filter_map(|b| r.map_break(b)).collect();
    if breaks.len() < tp.breaks.len() {
        notes.push("edge break: a vertical chamfer cone does not ride a tilt - its bevel is not declared".into());
    }
    breaks
}

/// The finished path, or a refusal when the map left the finite range.
fn checked(out: Toolpath) -> Result<Toolpath, TransformError> {
    let finite = out.all_finite()
        && out.comps.iter().all(|c| c.programmed.iter().all(|m| m.is_finite()))
        && out.breaks.iter().all(|b| b.edge.iter().all(|p| p.is_finite()));
    if finite { Ok(out) } else { Err(TransformError::BadInput("the transformed path")) }
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "transform_mirror_tests.rs"]
mod mirror_tests;

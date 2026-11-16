//! The solid law: no sampled position of a move may put the tool
//! assembly - cutting atom (flat, ball, bull nose, point cone) plus
//! shank and holder - inside the finished part's SOLID. Read off the
//! mesh itself ([`super::probe`], [`super::solid_index`]), never off
//! plane sections: material a plane at the move's depth does not cut -
//! a blind floor between two levels, a slab overhanging the ring, the
//! underside of an undercut lip - is still steel the tool cannot enter.
//! Specced from the shop rule "never cut the part you keep" read
//! against the closed-shell solid (point-in-solid by ray parity and the
//! swept-atom distance law, both cited in [`super::swept`]). A straight
//! move is judged as one exact sweep, an XY arc by its chords' sweeps
//! with the cutter widened by their sagitta; only a vertical-plane arc
//! is sampled (every `step`), its stated limit. A move whose POSE
//! changes along it (M13.7's mid-move reading, [`super::midpose`]) is
//! judged at the end pose as above AND, interval by interval, at the
//! poses the controller lerps between its ends - so a lean that
//! passes through a wall at mid-travel is convicted, not only one that
//! ends in it. Constant-pose moves keep their verdict and cost.
//!
//! Two readers: [`super::verify`]'s model law (b2), with the square
//! stack of its `ToolGeom` - never smaller than the real cutter - and
//! the operation layer's depth law via [`solid_hits`], with the tool's
//! own tip. A cutting move that enters the solid is a [`Warning::Gouge`];
//! a rapid that does is a [`Warning::RapidInStock`] - the part is stock
//! too, and steel at rapid feed is a crash.

use super::hole_wall::HoleAxis;
use super::midpose;
use super::probe::{Atom, Cutter, Frame, Prof};
use super::solid_index::SolidIndex;
use super::thread_bore::ThreadBore;
use super::types::Warning;
use super::upright::is_upright;
use crate::cl::{MoveKind, Toolpath, normalize_axis};
use crate::geometry::{Point3, Triangle};

/// How the solid law samples and what it excuses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolidLaw {
    /// Sample spacing along a vertical-plane arc, mm (every other move
    /// is swept exactly).
    pub step: f64,
    /// Penetration granted, radial and at the tip, mm: the gouge
    /// tolerance (plus any negative stock-to-leave the op asked for).
    pub allow: f64,
    /// The containment kinds (drill, bore, thread mill): their regions
    /// carry exact rims for the mesh's tessellated hole walls, so a
    /// facet of a ROUND wall about one of the holes the caller names
    /// ([`solid_hits_walled`]), entered only between vertices that stand
    /// outside the tool and by no more than its own sagitta about that
    /// hole's axis, is the rim, not a gouge ([`super::hole_wall`],
    /// #1205). With no holes named nothing is excused.
    pub hole_walls: bool,
}

/// Sagitta of the chords an XY arc is swept along, mm; every atom is
/// widened by it, so the chord sweep encloses the arc's (conservative by
/// at most this much, never blind).
const ARC_SAG: f64 = 0.002;

/// The swept pieces of one move as (start tip, end tip, widening): a
/// straight move is itself - one exact sweep; an XY arc or helix its
/// chords at [`ARC_SAG`]; a vertical-plane arc (whose chord deviation is
/// not horizontal) its on-arc points every `step`, the stated sampling
/// limit. Bounded, never an unbounded loop on absurd lengths.
fn pieces(m: &crate::cl::Move, step: f64) -> Vec<(Point3, Point3, f64)> {
    match m.arc {
        None => vec![(m.from, m.to, 0.0)],
        Some(arc) if arc.plane.is_xy() => m.segments3(ARC_SAG).into_iter().map(|(a, b)| (a, b, ARC_SAG)).collect(),
        Some(arc) => {
            let len = arc.length(m.from, m.to).filter(|l| l.is_finite()).unwrap_or(0.0);
            let n = if step > 0.0 { ((len / step).ceil() as usize).clamp(1, 4096) } else { 1 };
            let mut v = vec![(m.from, m.from, 0.0)];
            for k in 1..=n {
                match arc.point_at(m.from, m.to, k as f64 / n as f64) {
                    Some(p) => v.push((p, p, 0.0)),
                    // Degenerate arc: its straight chords, swept exactly.
                    None => return m.segments3(ARC_SAG).into_iter().map(|(a, b)| (a, b, ARC_SAG)).collect(),
                }
            }
            v
        }
    }
}

/// One warning per move whose assembly enters the solid, ascending. A
/// mesh or tool that cannot be read honestly fails closed: a Gouge at
/// the first cutting move, never a pass.
pub fn solid_hits(path: &Toolpath, facets: &[Triangle], cutter: &Cutter, law: &SolidLaw) -> Vec<Warning> {
    solid_hits_walled(path, facets, cutter, law, &[])
}

/// [`solid_hits`] for a containment kind: `walls` are the holes it
/// declares (in the path's frame), whose round walls' tessellation
/// [`SolidLaw::hole_walls`] excuses. A hole axis that does not run along
/// a move's tool axis excuses nothing on that move.
pub fn solid_hits_walled(path: &Toolpath, facets: &[Triangle], cutter: &Cutter, law: &SolidLaw, walls: &[HoleAxis]) -> Vec<Warning> {
    solid_hits_threaded(path, facets, cutter, law, walls, &[])
}

/// [`solid_hits_walled`] for a thread mill: `bores` are the holes it
/// threads, whose minor walls the helix cuts into by design
/// ([`super::thread_bore`], #1071).
pub fn solid_hits_threaded(
    path: &Toolpath,
    facets: &[Triangle],
    cutter: &Cutter,
    law: &SolidLaw,
    walls: &[HoleAxis],
    bores: &[ThreadBore],
) -> Vec<Warning> {
    judged(path, facets, cutter, law, walls, bores, false)
}

/// The stack's half of the solid law, for a kind whose cutting edge
/// answers to a law of its own (#1595's mark, which may lower the part
/// under the edge): a cutting move whose shank or holder enters the
/// solid is a [`Warning::ShankHit`] on the part whatever its edge does,
/// and a rapid that enters it with any of the assembly a
/// [`Warning::RapidInStock`]. The edge of a cutting move is not judged
/// here. One pose more is excused, and only that pose: on a straight
/// retract UP the tool's own axis from where a cutting move left it, the
/// edge where it already stands - the kerf that move made, which the
/// mesh does not show and the caller's law judged on that move. The
/// lift itself is judged ([`rise`]): the shank and holder over all
/// of it, and the column the edge rises through above its own top, so a
/// part overhang the retract carries the assembly into is a
/// `RapidInStock` on the lift. Fails closed as [`solid_hits`] does.
pub fn stack_hits(path: &Toolpath, facets: &[Triangle], cutter: &Cutter, law: &SolidLaw) -> Vec<Warning> {
    judged(path, facets, cutter, law, &[], &[], true)
}

/// The one walk both readings share; `stack_only` leaves a cutting
/// move's edge to its caller's own law.
fn judged(
    path: &Toolpath,
    facets: &[Triangle],
    cutter: &Cutter,
    law: &SolidLaw,
    walls: &[HoleAxis],
    bores: &[ThreadBore],
    stack_only: bool,
) -> Vec<Warning> {
    let walls = if law.hole_walls { walls } else { &[] };
    let first_cut = path.moves.iter().position(|m| m.kind != MoveKind::Rapid);
    if facets.is_empty() {
        return Vec::new();
    }
    let atoms = cutter.atoms();
    let allow = if law.allow.is_finite() { law.allow.max(0.0) } else { 0.0 };
    let index = SolidIndex::build(facets);
    let (Some(mut index), false) = (index, atoms.is_empty()) else {
        return first_cut.map(|at| vec![Warning::Gouge { at }]).unwrap_or_default();
    };
    // The fluted length is the cutting edge; the bands above it are the
    // shank and holder. A cut that only the bands make is not a gouge -
    // the edge never passed the finish surface - but steel rubbing the
    // part, said as such (ShankHit on the part), and said once.
    let fl = cutter.flute_len;
    let (edge, body): (Vec<_>, Vec<_>) = atoms.iter().copied().partition(|a| a.z0 < fl);
    // The pose along each move by the modal rule (M13.7, mid-move):
    // a move whose pose CHANGES is also judged at the poses between
    // its ends, interval by interval; every other move pays nothing.
    let poses = midpose::poses(path);
    let reach = atoms.iter().map(|a| a.z1).fold(0.0, f64::max);
    let mut out = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if !(m.from.is_finite() && m.to.is_finite()) {
            continue; // the NonFinite law owns it
        }
        let axis = m.axis.and_then(normalize_axis).filter(|a| !is_upright(a));
        let mid = poses
            .get(i)
            .and_then(|&(f, t)| midpose::change(f, t))
            .map(|(a, b)| midpose::intervals(m, a, b, reach, ARC_SAG))
            .unwrap_or_default();
        let grow_mid = if m.arc.is_some() { ARC_SAG } else { 0.0 };
        let mut hits = |set: &[Atom]| {
            !set.is_empty()
                && (pieces(m, law.step).into_iter().any(|(a, b, grow)| {
                    let fr = Frame::new(a, axis);
                    index.hits(&fr, fr.local(b), set, allow, grow, &axes_in(&fr, walls), bores)
                }) || mid.iter().any(|iv| {
                    // The interval's tip travel swept exactly under the
                    // pose held at its midpoint: the pose's own turn
                    // within the interval is the stated sampling limit
                    // (STEP of far-end travel), the same as the fixture
                    // laws' posed sweep.
                    let fr = Frame::new(iv.p0, normalize_axis(iv.am).filter(|a| !is_upright(a)));
                    index.hits(&fr, fr.local(iv.p1), set, allow, grow_mid, &axes_in(&fr, walls), bores)
                }))
        };
        if m.kind == MoveKind::Rapid {
            let lifts = stack_only && axis.is_none() && m.arc.is_none() && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z >= m.from.z;
            let from_cut = i.checked_sub(1).and_then(|j| path.moves.get(j)).is_some_and(|c| c.kind != MoveKind::Rapid && c.to == m.from);
            if lifts && from_cut {
                // Out of the kerf that cut made: the edge's start pose is
                // the caller's; everything the lift enters is ours.
                let (fr, column) = (Frame::new(m.from, None), rise(&edge, m.to.z - m.from.z));
                if hits(&body) || (!column.is_empty() && index.hits(&fr, [0.0; 3], &column, allow, 0.0, &[], &[])) {
                    out.push(Warning::RapidInStock { at: i });
                }
                continue;
            }
            if hits(&atoms) {
                out.push(Warning::RapidInStock { at: i });
            }
        } else if !stack_only && hits(&edge) {
            out.push(Warning::Gouge { at: i });
        } else if hits(&body) {
            out.push(Warning::ShankHit { at: i, part: true });
        }
    }
    out
}

/// What a cutting edge enters, beyond the pose it starts in, on a
/// straight lift of `h` up its own axis: the column from its top to
/// where that top ends, at its largest radius, as an atom to be judged
/// standing still at the start. Exact for the edge's profile, which
/// never narrows upward ([`super::probe`]'s atoms): every point under
/// the top stays inside where the edge stood. Nothing for no edge or no
/// rise; an unreadable rise is read as the tallest column the index
/// takes, never as none.
fn rise(edge: &[Atom], h: f64) -> Vec<Atom> {
    let top = edge.iter().map(|a| a.z1).fold(0.0, f64::max);
    let r = edge.iter().map(Atom::rho_max).fold(0.0, f64::max);
    let h = if h.is_finite() { h } else { f64::MAX };
    if edge.is_empty() || !(h > 0.0 && r > 0.0) {
        return Vec::new();
    }
    vec![Atom { z0: top, z1: top + h, prof: Prof::Flat(r) }]
}

/// The holes' axes in `fr`'s XY: those running along its tool axis.
fn axes_in(fr: &Frame, walls: &[HoleAxis]) -> Vec<[f64; 2]> {
    walls.iter().filter_map(|w| fr.axis_xy(w.at, w.dir)).collect()
}

//! The fixture laws (c2)/(d2) read against the fixture's real mesh
//! (#339): the box laws stay the broad phase, and a fixture whose
//! closed mesh is at hand gets the last word on every upright move its
//! box flagged. A path through the gap between two vise jaws - inside
//! the vise's one world box, nowhere near its steel - is then clean,
//! while a band clipping a jaw corner still blocks the post.
//!
//! ## The law here: the mesh only ever ACQUITS
//!
//! A move leaves the report only when every box that flagged it is a
//! meshed fixture whose mesh proves the move clear ([`judge_move`]);
//! the mesh never adds a hit the boxes did not make. So the box laws'
//! own silences (the strict-dip z rule: stock seated on a plate, a
//! retract off the mounting face) stand untouched, and a false hit can
//! only ever survive where the box already hit. Where nothing is
//! acquitted the report is the box laws' own, byte for byte.
//!
//! Judged by the mesh here: upright moves (held and stated pose both
//! upright - the frames agree, and the stack is the vertical column
//! the box laws read), the spindle's C angle at zero, straight or arcs
//! in any plane (XZ/YZ chorded in their own plane with the XY arcs'
//! pad, slice 3). Posed and C-turned moves are read by
//! [`super::mesh_posed`] (slice 3), in the frame each steel set is
//! judged in. A box two fixtures share, an open or refused mesh, and
//! whatever [`super::mesh_posed`] cannot place: the box verdict stands.
//!
//! Proven clear means, for every band of the stack (shank, holder, the
//! spindle housing when stated, flute - the box laws' order), along
//! every chord of the move: a GJK-proven gap of more than
//! [`FIXTURE_HIT_TOL`] plus the chord's own deviation from the arc to
//! every facet near it, and the band not standing inside the solid
//! (the swept band is connected and meets no facet, so one point
//! decides). See [`crate::fixture_mesh`] for why each answer is
//! conservative. The stack is the same one for rapids: an upright rapid
//! the boxes flagged keeps its hit if ANY band meets the steel - the
//! same whole stack the rapid box law (d2) sweeps since #424
//! ([`super::stack_span`]), named the way that law names it: a band
//! above the flute by its part, the flute itself as the rapid.
//!
//! Provenance: our own composition of the box laws with the mesh tests
//! of [`crate::fixture_mesh`] (GJK, BVH, ray-crossing inclusion -
//! published methods cited there); the chord-to-arc deviation is the
//! sagitta of elementary geometry. No vendor source consulted.

use std::slice;

use super::upright::is_upright;
use crate::cl::{Move, Toolpath};
use crate::fixture::{FixtureBox, FIXTURE_HIT_TOL};
use crate::fixture_mesh::{FixtureMesh, MeshedFixture};
use crate::geometry::Point3;
use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;

use super::mesh_posed;
use super::rotary::RotaryAxis;
use super::types::{FixturePart, Warning};
use super::workholding::Workholding;

/// One coaxial band of the tool assembly: `lo..hi` above the tip, `r`
/// from the axis, reported as `part` when struck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteelBand {
    pub part: FixturePart,
    pub lo: f64,
    pub hi: f64,
    pub r: f64,
}

/// The assembly the box laws sweep, in their first-struck order: shank,
/// holder, the spindle housing (radius, length) when the machine
/// states it, then the flute.
#[must_use]
pub fn steel_stack(tool: &ToolGeom, holder: &HolderStack, spindle: Option<(f64, f64)>) -> Vec<SteelBand> {
    let fl = tool.flute_len;
    // #852: the shank as swept - a drawn neck's bands, or the cylinder.
    let mut out: Vec<SteelBand> = holder
        .shank_bands(tool)
        .iter()
        .map(|b| SteelBand { part: FixturePart::Shank, lo: b.above, hi: b.above + b.len, r: b.r })
        .collect();
    // #368: what stands above the shank - the holder's own bands, the
    // same `HolderStack` the box laws (c2/d2) sweep.
    out.extend(holder.bands.iter().map(|b| SteelBand {
        part: FixturePart::Holder,
        lo: b.above,
        hi: b.above + b.len,
        r: b.r,
    }));
    if let Some((sr, slen)) = spindle {
        out.push(SteelBand { part: FixturePart::Spindle, lo: holder.top, hi: holder.top + slen, r: sr });
    }
    out.push(SteelBand { part: FixturePart::Flute, lo: 0.0, hi: fl, r: tool.radius });
    out
}

/// What the mesh law reads besides the path: the setup's meshed
/// fixtures, the tool stack, and the chord tolerance arcs are cut at.
#[derive(Clone, Copy, Debug)]
pub struct MeshSteel<'a> {
    pub fixtures: &'a [MeshedFixture],
    pub stack: &'a [SteelBand],
    pub chord_tol: f64,
}

/// Pad on every arc chord beyond its measured deviation: the arc centre
/// stands on the 1 um lattice, so the endpoints sit up to that far off
/// the circle the controller cuts.
const ARC_PAD: f64 = 1e-3;

/// One move's mesh reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Read {
    Clear,
    Hit(FixturePart),
    /// Not the mesh's to judge: the box verdict stands.
    Unjudged,
}

fn rank(p: FixturePart) -> u8 {
    match p {
        FixturePart::Shank => 0,
        FixturePart::Holder => 1,
        FixturePart::Spindle => 2,
        FixturePart::Flute => 3,
        FixturePart::Rapid => 4,
    }
}

fn keep(slot: &mut Option<FixturePart>, p: FixturePart) {
    if slot.is_none_or(|q| rank(p) < rank(q)) {
        *slot = Some(p);
    }
}

/// `law` (a box law over workholding - bands or rapids, `rapids` says
/// which) with each meshed fixture's hits re-read against its mesh. See
/// the module header: the result is `law(wh)` minus the moves every
/// flagging box's mesh acquits.
pub fn mesh_refined(
    path: &Toolpath,
    wh: &Workholding<'_>,
    steel: &MeshSteel<'_>,
    rapids: bool,
    law: impl Fn(&Workholding<'_>) -> Vec<Warning>,
) -> Vec<Warning> {
    let joint = law(wh);
    if steel.fixtures.is_empty() || joint.is_empty() {
        return joint;
    }
    // Pair each box with the one fixture that owns it; a box standing
    // twice, or owned twice, cannot be told apart and stays a box.
    let all: Vec<(bool, FixtureBox)> =
        wh.turning.iter().map(|b| (false, *b)).chain(wh.fixed.iter().map(|b| (true, *b))).collect();
    let (mut plain_t, mut plain_f) = (Vec::new(), Vec::new());
    let mut meshed: Vec<(bool, FixtureBox, &MeshedFixture)> = Vec::new();
    for &(fixed, b) in &all {
        let twins = all.iter().filter(|(_, o)| *o == b).count();
        let mut owners = steel.fixtures.iter().filter(|f| f.aabb == b);
        match (twins, owners.next(), owners.next()) {
            (1, Some(f), None) => meshed.push((fixed, b, f)),
            _ if fixed => plain_f.push(b),
            _ => plain_t.push(b),
        }
    }
    if meshed.is_empty() {
        return joint;
    }
    let n = path.moves.len();
    let mut kept: Vec<Option<FixturePart>> = vec![None; n];
    let mut dropped = vec![false; n];
    if !(plain_t.is_empty() && plain_f.is_empty()) {
        let plain = Workholding { turning: &plain_t, fixed: &plain_f, axis: wh.axis };
        for w in law(&plain) {
            if let Warning::FixtureHit { at, fixture_part } = w
                && let Some(s) = kept.get_mut(at)
            {
                keep(s, fixture_part);
            }
        }
    }
    let poses = move_poses(path);
    for (fixed, b, f) in &meshed {
        let one = slice::from_ref(b);
        let sub = if *fixed {
            Workholding { turning: &[], fixed: one, axis: wh.axis }
        } else {
            Workholding { turning: one, fixed: &[], axis: wh.axis }
        };
        let mesh = f.mesh();
        for w in law(&sub) {
            let Warning::FixtureHit { at, fixture_part } = w else { continue };
            let (Some(m), Some(slot), Some(gone)) = (path.moves.get(at), kept.get_mut(at), dropped.get_mut(at))
            else {
                continue;
            };
            let read = match (&mesh, poses.get(at)) {
                (Ok(mesh), Some(pose)) => judge(mesh, m, pose, steel, rapids, *fixed, wh.axis.as_ref()),
                _ => Read::Unjudged,
            };
            match read {
                Read::Clear => *gone = true,
                Read::Hit(p) => keep(slot, p),
                Read::Unjudged => keep(slot, fixture_part),
            }
        }
    }
    joint
        .into_iter()
        .filter_map(|w| match w {
            Warning::FixtureHit { at, .. } if dropped.get(at).copied().unwrap_or(false) => {
                kept.get(at).copied().flatten().map(|fixture_part| Warning::FixtureHit { at, fixture_part })
            }
            other => Some(other),
        })
        .collect()
}

/// One move's pose and spindle C by the modal rule (the posed sweep's
/// reading: a move stating none keeps the one held coming in), plus
/// the C the PREVIOUS move stated when it ends where this one starts -
/// the only C a turning move is known to start from.
#[derive(Clone, Copy, Debug)]
struct MovePose {
    from: Option<[f64; 3]>,
    to: Option<[f64; 3]>,
    from_c: Option<f64>,
    to_c: Option<f64>,
    prev_c: Option<f64>,
}

fn move_poses(path: &Toolpath) -> Vec<MovePose> {
    let mut held: Option<[f64; 3]> = path.moves.first().and_then(|m| m.axis);
    let mut held_c: Option<f64> = path.moves.first().and_then(|m| m.rotary);
    let mut prev: Option<&Move> = None;
    path.moves
        .iter()
        .map(|m| {
            let (from, from_c) = (held, held_c);
            held = m.axis.or(held);
            held_c = m.rotary.or(held_c);
            let prev_c = prev.filter(|p| p.to == m.from).and_then(|p| p.rotary);
            prev = Some(m);
            MovePose { from, to: held, from_c, to_c: held_c, prev_c }
        })
        .collect()
}

/// Which reading a move gets (#339 slices 1 and 3): upright with C at
/// zero - the upright band law here, in the path frame; posed - the
/// posed law, in the machine frame for fixed steel under a rotary
/// axis; C turned or held off zero - the spindle-C law, fixed steel
/// unjudged under a rotary axis too (two turns, no one frame).
#[allow(clippy::too_many_arguments)] // the move, its pose, the steel and its frame
fn judge(
    mesh: &FixtureMesh,
    m: &Move,
    pose: &MovePose,
    steel: &MeshSteel<'_>,
    rapids: bool,
    fixed: bool,
    axis: Option<&RotaryAxis>,
) -> Read {
    let flat = |c: Option<f64>| c.is_none_or(|c| c == 0.0);
    if !(flat(pose.from_c) && flat(pose.to_c)) {
        if fixed && axis.is_some() {
            return Read::Unjudged;
        }
        return mesh_posed::judge_c(mesh, m, (pose.prev_c, pose.to), steel, rapids, fixed);
    }
    let up = |a: Option<[f64; 3]>| a.is_none_or(|a| is_upright(&a));
    if up(pose.from) && up(pose.to) {
        return judge_move(mesh, m, steel, rapids);
    }
    mesh_posed::judge_posed(mesh, m, (pose.from, pose.to), steel, rapids, if fixed { axis } else { None })
}

/// A struck band's report, named the way law (d2) names it since #424:
/// its own part, except the flute on a rapid, which reads as
/// [`FixturePart::Rapid`] (a band above the flute keeps its part).
/// Shared by the upright, posed and spindle-C readings.
pub(super) fn label(p: FixturePart, rapids: bool) -> Read {
    Read::Hit(if rapids && p == FixturePart::Flute { FixturePart::Rapid } else { p })
}

/// The move's tip path as straight chords, each with how far the real
/// path can stray from it: the sagitta in the arc's own plane (XY, XZ
/// or YZ - [`crate::cl::ArcPlane::uvw`]) over the normal coordinate's
/// run, plus [`ARC_PAD`]. None: not the mesh's to read (an arc whose
/// radius is not a number, no chords).
pub(super) fn chords(m: &Move, chord_tol: f64) -> Option<Vec<(Point3, Point3, f64)>> {
    let Some(arc) = m.arc else { return Some(vec![(m.from, m.to, 0.0)]) };
    let pl = arc.plane;
    let (cu, cv) = pl.center_uv(arc.center);
    let rad_of = |p: Point3| {
        let (u, v, _) = pl.uvw(p);
        (u - cu).hypot(v - cv)
    };
    let rad = rad_of(m.from).max(rad_of(m.to));
    if !rad.is_finite() {
        return None;
    }
    let segs = m.segments3(chord_tol);
    if segs.is_empty() {
        return None;
    }
    Some(
        segs.into_iter()
            .map(|(p, q)| {
                let ((pu, pv, pw), (qu, qv, qw)) = (pl.uvw(p), pl.uvw(q));
                let half = 0.5 * (qu - pu).hypot(qv - pv);
                let sag = rad - (rad * rad - half * half).max(0.0).sqrt();
                (p, q, sag.max(0.0).hypot(qw - pw) + ARC_PAD)
            })
            .collect(),
    )
}

/// One upright move against one closed mesh: the first band struck
/// (shank, holder, spindle, flute; on a rapid the flute reads as
/// [`FixturePart::Rapid`], law (d2)'s own naming since #424), Clear
/// only on proof.
fn judge_move(mesh: &FixtureMesh, m: &Move, steel: &MeshSteel<'_>, rapids: bool) -> Read {
    if !(m.from.is_finite() && m.to.is_finite()) {
        return Read::Unjudged;
    }
    let tol = if steel.chord_tol.is_finite() && steel.chord_tol > 0.0 { steel.chord_tol } else { 0.01 };
    let Some(segs) = chords(m, tol) else { return Read::Unjudged };
    let hit = |p: FixturePart| label(p, rapids);
    for b in steel.stack {
        if !(b.lo.is_finite() && b.hi.is_finite() && b.r.is_finite()) {
            return hit(b.part); // a band that is not a number is not proven clear
        }
        if b.hi - b.lo <= 0.0 {
            continue; // no length, no steel
        }
        for &(p, q, pad) in &segs {
            if mesh.band_meets(p, q, b.lo, b.hi, b.r, FIXTURE_HIT_TOL + pad) {
                return hit(b.part);
            }
        }
    }
    // Clear of every facet: each swept band is wholly outside the solid
    // or wholly inside it - one point of it decides.
    let start = m.from;
    for b in steel.stack.iter().filter(|b| b.hi - b.lo > 0.0) {
        let mid = 0.5 * (b.lo + b.hi);
        if mesh.contains(Point3::new(start.x, start.y, start.z + mid)) {
            return hit(b.part);
        }
    }
    Read::Clear
}

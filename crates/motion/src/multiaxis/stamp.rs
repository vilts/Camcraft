//! The STAMP: the pass that reads a finished upright-ball path back
//! into an oriented one (M5.1 stage C). A sphere's envelope is
//! rotation-invariant about its own CENTRE, so the ball ladder a
//! 5-axis finish cuts is exactly the waterline/raster ladder of stage
//! A/B - and what makes it 5-axis is the pose each move holds: at
//! every move whose ball rests on the part, the nearest surface point
//! gives the normal (`d = |c - q| = r` is tangency, read at the BALL
//! CENTRE), the move's own travel gives the tangent, and
//! [`super::tilt::command_axis`] folds lead, lag and the clamp into
//! the direction the spindle must point. Moves whose ball is in air
//! (the hops, the clearances) and level floors hold no lean.
//!
//! The pivot law ([`super::retip`]): the base ladder stores TIPS of an
//! upright ball (finishing3d's convention, and the stream's - the post
//! locates the tool tip), so its centres stand `r` up Z from every
//! point. A pose turns the tool about that centre, never about the
//! tip, so every emitted point is the centre minus `r` along the pose
//! it holds. Keeping the tip and swapping the axis (what this pass did
//! before M13's follow-up) carried every posed ball `r·(a - up)` off
//! the skin it was planned on: most of a radius of air off a wall,
//! and a bite wherever a later upright move inherited the tilt.
//! Two consequences ride along:
//!
//! - **The post keeps the tilt modal.** A move stating nothing runs
//!   under the last pose on the machine, so once a lean has been
//!   stated every later move states its own: a cut the lean the law
//!   gives, or upright `Z` (which the post commands as explicit zero
//!   angles); a rapid the pose it inherits, so a retract lifts the
//!   ball straight off its last contact - the plan's own air path -
//!   instead of swinging the tool upright through that surface.
//! - **Posed arcs are chords.** A tip backed off a turning pose leaves
//!   any circle a G2/G3 could state, so an arc that leans anywhere is
//!   emitted as its chords at `ARC_FIT_TOL`, each end posed fresh; an
//!   arc that stays upright end to end keeps its exact intent.
//!
//! Provenance: the ball-center/axis-pose split and tool-tip (RTCP)
//! programming are standard multi-axis CL practice (tool-axis vectors
//! ride the location words, RS-274/ISO 10791 tradition); written from
//! those published descriptions and our own design notes only.

use crate::cl::{is_vertical, MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::avoid::{escape_axis, shank_hits_at, Shank};
use super::retip::{along, tip_of, UP};
use super::stamp_pose::{pose_law, Mesh, Ruled};
use super::stamp_rise::rapid_rise;
use super::tilt::AxisControl;

/// The search ceiling shank avoidance tilts within when the control
/// states no cone (`max_tilt_deg: None` = "as far as the geometry
/// asks"): a horizontal spindle is the last pose anyone means, and
/// the cone's own law is a cone, not a plane.
pub(super) const ESCAPE_CAP: f64 = 85.0;

/// Read an upright ball's tip path back as an oriented one: the same
/// BALLS in the same order, each tip re-derived from its centre and
/// the pose it holds. `r` is the ball radius the path was generated
/// with; contact is decided at the centre by the tangent law
/// `|c - q| = r`, with a band that forgives the project's micron
/// lattice and the arc fit, never a whole radius of slack. Facets are
/// scanned with an AABB shell reject. `shank` (M5.1 stage F) is the
/// tool above the ball: where `Some`, a pose whose shank column
/// touches the part tilts to the nearest clear angle inside the cone
/// (about the same centre), and a move no cone angle clears refuses
/// the path by name.
pub(super) fn stamp_axes(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    stamp_inner(path, facets, r, c, None, shank)
}

/// The same pass with the swarf law: on the facets the ruling
/// analysis marked ruled, the tool lies WITH the blade - its axis is
/// the ruling direction projected into the surface (tangent to the
/// flank, the side-machining contact) - and everywhere else the
/// lean law of stage C holds, second-guessed by the swarf fallback of
/// [`super::stamp_pose`] (a pose under the horizon or in the part
/// carries the ruling; a row no pose clears refuses by name). `dir`
/// is the region's unit XY direction, `ruled` the per-facet verdict
/// of [`super::ruling::analyze`], `tol` the plan's tolerance - the
/// band a station is read as contact to, since the plan's own chords
/// promise nothing tighter.
pub(super) fn stamp_axes_ruled(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    dir: Point3,
    ruled: &[bool],
    tol: f64,
) -> Result<Toolpath, GenError> {
    if !(tol.is_finite() && tol >= 0.0) {
        return Err(GenError::Invalid(format!(
            "a swarf stamp needs a finite non-negative tolerance, got {tol}"
        )));
    }
    // The ruled reads stay out of stage F: on a ruled facet the tool
    // lies WITH the blade - that direction is surface truth, not a
    // free axis to dodge with (RECOMMENDED.md).
    stamp_inner(path, facets, r, c, Some(Ruled { dir, ruled, tol }), None)
}

/// One station of the expanded ladder: a move's end (an arc's chord
/// ends) as the BALL CENTRE the plan put there.
pub(super) struct Station {
    pub(super) kind: MoveKind,
    pub(super) feed: Option<f64>,
    pub(super) from: Point3,
    pub(super) to: Point3,
    /// The base move this station belongs to.
    pub(super) mv: usize,
}

fn stamp_inner(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    ruling: Option<Ruled>,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    if !path.cycles.is_empty() {
        return Err(GenError::Invalid(
            "a 5-axis stamp cannot re-pose drill cycles".into(),
        ));
    }
    if !(r.is_finite() && r > 0.0) {
        return Err(GenError::Invalid(format!(
            "a 5-axis stamp needs a finite positive ball radius, got {r}"
        )));
    }
    // Micron lattice + float dust, not a radius - or, for a swarf
    // read, the plan's own tolerance (stamp_pose: a ring chord a few
    // hundredths off tangent is contact to the plan, not air).
    let band = (0.01 + 1e-6 * r).max(ruling.map_or(0.0, |rl| rl.tol));
    let boxes: Vec<[f64; 6]> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let mesh = Mesh::new(facets, &boxes);
    // The plan's balls: every tip lifted r up Z; arcs at their chords.
    let mut st: Vec<Station> = Vec::with_capacity(path.moves.len());
    for (i, mv) in path.moves.iter().enumerate() {
        let mut ends: Vec<Point3> = match mv.arc {
            Some(_) => mv.segments3(ARC_FIT_TOL).into_iter().map(|(_, e)| e).collect(),
            None => Vec::new(),
        };
        if ends.is_empty() {
            ends.push(mv.to);
        }
        let mut from = along(mv.from, UP, r);
        for e in ends {
            let to = along(e, UP, r);
            st.push(Station { kind: mv.kind, feed: mv.feed, from, to, mv: i });
            from = to;
        }
    }
    // The pose every station holds. `held` is the smoothing memory
    // (stage F): the axis the machine holds entering a station,
    // upright at the home the post wakes at. `leaned` turns true at
    // the first lean and never back: from there on every station
    // states its pose (the modal-tilt law above).
    let mut poses: Vec<Option<[f64; 3]>> = Vec::with_capacity(st.len());
    let mut held = UP;
    let mut leaned = false;
    // The last lean stated (the ruled sign rule's second witness: a
    // ruling is a line, and the end the spindle sits behind is the
    // end it already sat behind - see `ruled_sign`).
    let mut last_lean: Option<[f64; 3]> = None;
    // The kind each station posts as: the plan's own, except a swarf
    // exit rise proved in air under the held lean, which posts as a
    // rapid holding it (#48, [`super::stamp_rise`]).
    let mut kinds: Vec<MoveKind> = Vec::with_capacity(st.len());
    for (k, s) in st.iter().enumerate() {
        let rise = leaned && ruling.is_some_and(|rl| rapid_rise(s, &mesh, r, band, rl, held));
        let kind = if rise { MoveKind::Rapid } else { s.kind };
        kinds.push(kind);
        let lean = match kind {
            MoveKind::Rapid => None,
            // #1015: a station refused hands back the plan before its
            // move - world, as the stamp's input is, not yet posed.
            _ => cut_axis(s, &mesh, r, band, &st[k + 1..], c, ruling, shank, held, last_lean)
                .map_err(|e| e.with_ghost_moves(path.moves.get(..s.mv).unwrap_or_default()))?,
        };
        if lean.is_some_and(|a| !is_vertical(&a)) {
            leaned = true;
            last_lean = lean;
        }
        let pose = match (kind, leaned) {
            (_, false) => lean,
            // A rapid travels under the pose it inherits: the ball
            // leaves its last contact along the axis it cut with and
            // follows the plan's own air path, never swinging upright
            // through the surface it just left.
            (MoveKind::Rapid, true) => Some(held),
            (_, true) => lean.or(Some(UP)),
        };
        held = pose.unwrap_or(UP);
        poses.push(pose);
    }
    let mut b = PathBuilder::new(path.start().unwrap_or(Point3::ZERO));
    let mut k = 0usize;
    for (i, mv) in path.moves.iter().enumerate() {
        let k0 = k;
        while k < st.len() && st[k].mv == i {
            k += 1;
        }
        let group = &poses[k0..k];
        let upright = group.iter().all(|p| p.is_none_or(|a| is_vertical(&a)))
            && kinds[k0..k].iter().zip(&st[k0..k]).all(|(kd, s)| *kd == s.kind);
        match (mv.arc, upright, group.last()) {
            // A move that never leans keeps its exact intent: its
            // upright tips are the plan's own, bit for bit.
            (Some(a), true, Some(pose)) => {
                b.set_axis(*pose);
                b.push_arc_in(mv.kind, mv.to, mv.feed, a.center, a.ccw, a.plane);
            }
            (None, true, Some(pose)) => {
                b.set_axis(*pose);
                b.push(mv.kind, mv.to, mv.feed);
            }
            _ => {
                for ((s, pose), kind) in st[k0..k].iter().zip(group).zip(&kinds[k0..k]) {
                    b.set_axis(*pose);
                    let feed = if *kind == MoveKind::Rapid { None } else { s.feed };
                    b.push(*kind, tip_of(s.to, *pose, r), feed);
                }
            }
        }
    }
    let stamped = b.finish();
    if !stamped.all_finite() {
        return Err(GenError::Invalid("a 5-axis stamp left the finite horizon".into()));
    }
    Ok(stamped)
}

/// The pose one cutting station holds, before and after the shank
/// speaks: None in air and on level floors, the surface normal with
/// lead/lag/clamp folded in wherever the station's ball rests on a
/// facet at its tangent distance. With a `shank` (stage F) a pose that
/// touches is re-aimed by the escape search about the same ball
/// centre; a station nothing inside the cone clears refuses the whole
/// path by name. `seed` is the cone's axis the search tilts around:
/// the contact normal where there is one, vertical wherever the pose
/// is an answer to air or a level floor.
#[allow(clippy::too_many_arguments)] // the stamp's flat read of one station
fn cut_axis(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Station],
    c: &AxisControl,
    ruling: Option<Ruled>,
    shank: Option<&Shank>,
    held: [f64; 3],
    last_lean: Option<[f64; 3]>,
) -> Result<Option<[f64; 3]>, GenError> {
    let (a0, seed, skip) = pose_law(s, mesh, r, band, rest, c, ruling, held, last_lean)?;
    let Some(sh) = shank.filter(|_| !skip) else {
        return Ok(a0);
    };
    let a = a0.unwrap_or(UP);
    if !shank_hits_at(s.to, a, r, sh, mesh.facets, mesh.boxes) {
        return Ok(a0);
    }
    let cone = c.max_tilt_deg.unwrap_or(ESCAPE_CAP).min(ESCAPE_CAP);
    match escape_axis(s.to, a, seed, held, cone, r, sh, mesh.facets, mesh.boxes) {
        Some(b) => Ok(Some(b)),
        None => Err(GenError::Invalid(format!(
            "the shank of the tool touches the part at move {} of the posed path, and no \
             pose inside the max-tilt cone ({cone:.0} deg) clears it - widen the cone, \
             shorten the tool, or turn shank avoidance off",
            s.mv
        ))),
    }
}

// The stamp-level law tests live in `super::stamp_tests` (the AGENTS
// 400-line seam, crossed by moving the test module, not by trimming a
// law comment).

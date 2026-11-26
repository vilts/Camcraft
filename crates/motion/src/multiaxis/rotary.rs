//! The rotary generators (M5.1 stage E): the turned part's two
//! native path ladders, each riding a fitted axis [`super::axis`] and
//! the pose law of [`super::tilt`] with the surface normal replaced
//! by the RADIAL direction - the line from the axis to the tool,
//! which is what a turned work actually offers the spindle. Both
//! kinds are world-in, world-out: the path is planned in an axis
//! frame (an exact rotation of the work), posed, and carried back,
//! so the app branch is as thin as the 5-axis ones.
//!
//! `rotary_contour` is the waterline rung ladder about the axis -
//! the same rings `contour5d` cuts, posed radially: on a cylinder
//! the tool stands square to the wall and its axis MEETS the
//! rotation axis, the defining posture of rotary work. Away from the
//! horizontal stance, a world height H maps to the axial station
//! `(H - o.z) / u.z` along the axis, the exact stage-B identity when
//! the part stands on its axis (the lathe case) and the honest
//! station reading when it leans.
//!
//! `rotary_parallel` lays the bar down in its planning frame -
//! passes along the axis, stepping around it - and runs the swarf
//! raster there: on a laid bar the raster's shallow terrain is the
//! crown of the work and the steep flanks its sides, where the
//! rings' level loops degenerate into exactly the straight
//! generatrices the kind is named for. No new pattern kernel; the
//! same two ladders of stages A-D, composed. A bar standing nearly
//! on end refuses (its parallel passes are the contour's rings) and
//! a bar lying nearly flat reads contour top/bottom as axial ball-center
//! stations, and clearance/retract as radii about the fitted axis.
//! Its rings use radial links through [`flat`], without dividing by Z.
//! The raster's links do not survive the laying-down: its Z lifts are
//! rebuilt RADIAL - out along the radius to the retract cylinder,
//! index, back in - by [`super::rotary_links`], and both kinds open on
//! the clearance pose over their first cut (M13.8).
//!
//! The world exit is spelled here, not through `world_path`: that
//! stamps the frame's CONSTANT pose where these paths carry a
//! per-move radial one - and it turns that pose about the BALL
//! CENTER the ladder planned, emitting the tip the stream's contract
//! locates (the M13 pivot law, [`super::retip`]; turning about the
//! tip stood every ring one ball radius off its wall). Arcs ride as
//! chords at `ARC_FIT_TOL` with the pose computed fresh at every
//! chord end - a pose turning along the circle moves the tip off any
//! circle G2/G3 could state - never interpolated, always the law's
//! own answer at the point the move ends.
//!
//! Provenance: axis-of-revolution detection and the radial tool-axis
//! convention of 4/5-axis rotary machining from public CAM
//! literature and turning practice; composed on this codebase's own
//! ring, raster and lean laws. Nothing of any vendor's consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::cl::TiltedFrame;
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::axis::{self, Axis};
use super::retip::{along, radial_at, tip_of};
use super::{rotary_ends, rotary_links, rotary_pick};
use super::tilt::{command_axis, AxisControl};

mod flat;
mod gaps;
mod halves;
#[cfg(test)]
mod single_frame_tests;

/// The stance boundary (sin 1 degree): near horizontal, contour
/// uses explicit stations; near vertical, parallel refuses its raster.
const LEAN_MIN: f64 = 0.0175;
/// World up, spelled once (Point3::new is not a const fn, so the
/// literal stands in for it).
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

/// The rotary contour: the axis read off the mesh, the waterline
/// rung ladder in the axis frame, the radial pose on every cut move.
pub fn rotary_contour(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let ax = axis::fit(facets)?;
    rotary_contour_about(facets, ax, p, c)
}

/// [`rotary_contour`] about an axis the caller STATES and has judged
/// (#2772 parts 1-2, `axis_stated::judged_any`): the machine's rotary
/// line or the operation's own direction, anchored through the work
/// offset's origin - never a line the model's faces were not read
/// against. The fit stays inside the plain entry, so turning and
/// drilling and every golden path keep their own axis.
pub fn rotary_contour_stated(
    facets: &[Triangle],
    ax: Axis,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_contour_about(facets, ax, p, c)
}

/// The contour ladder about `ax`, the plain entry's own body.
fn rotary_contour_about(
    facets: &[Triangle],
    ax: Axis,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let lean = ax.dir.dot(UP).abs();
    if lean < LEAN_MIN {
        let f = frame(&ax)?;
        let r = p.tool_diameter * 0.5;
        let base = flat::plan(&f.local_facets(facets), p)
            .map_err(|e| to_world(e, &f, &ax, c, r))?;
        return posed(&f, &base, &ax, c, r);
    }
    let f = frame(&ax)?;
    let local = f.local_facets(facets);
    let uz = if ax.dir.dot(UP) < 0.0 { -lean } else { lean };
    let q = stations(&local, p, &f, &|h| (h - ax.origin.z) / uz);
    let r = p.tool_diameter * 0.5;
    let base = finishing3d::waterline(&local, &q).map_err(|e| to_world(e, &f, &ax, c, r))?;
    posed(&f, &base, &ax, c, r)
}

/// The rotary parallel: generatrix passes along the axis, stepping
/// around the bar - the swarf raster in a frame that lays the axis
/// down, with the same radial pose law.
pub fn rotary_parallel(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_parallel_in(facets, None, p, c)
}

/// [`rotary_parallel`] on a bar the setup `stock` describes (#2343):
/// where the stock goes on past an end of the model - the bar in the
/// chuck - the pass stops at that end's plane and leaves the end face
/// ([`rotary_ends`]), and a note names the end. None, or stock that
/// ends with the model, is the plain raster.
pub fn rotary_parallel_in(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_parallel_over(facets, stock, None, p, c)
}

/// [`rotary_parallel_in`] kept to `picked` faces of the model (#2848):
/// the axis and the raster still read every facet of `facets`, and the
/// cut keeps only where the radial through the ball's center meets a
/// picked facet ([`rotary_pick`]). None is the whole model. Picked
/// faces that no pass stands over are refused by name.
pub fn rotary_parallel_over(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    picked: Option<&[Triangle]>,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let ax = axis::fit(facets)?;
    rotary_parallel_about(facets, stock, picked, ax, p, c)
}

/// [`rotary_parallel_over`] about an axis the caller STATES and has
/// judged (#2772 parts 1-2, `axis_stated::judged_any`) - the machine's
/// rotary line or the operation's own direction, anchored through the
/// work offset's origin. The same body the plain entry runs about its
/// fit; the fit stays inside the plain entry.
pub fn rotary_parallel_over_stated(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    picked: Option<&[Triangle]>,
    ax: Axis,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_parallel_about(facets, stock, picked, ax, p, c)
}

/// The raster ladder about `ax`, the plain entry's own body.
fn rotary_parallel_about(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    picked: Option<&[Triangle]>,
    ax: Axis,
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let flat = 1.0 - ax.dir.dot(UP) * ax.dir.dot(UP);
    if flat < LEAN_MIN * LEAN_MIN {
        return Err(GenError::Invalid(
            "the work stands on its axis - passes along a standing bar are its rings: Rotary Contour".into(),
        ));
    }
    // The laid-down frame: local x along the axis, local z as close
    // to world up as the axis allows (exact for a bar lying flat),
    // right-handed the way TiltedFrame stores its basis.
    let uz = ax.dir.dot(UP);
    let up = Point3::new(-ax.dir.x * uz, -ax.dir.y * uz, 1.0 - uz * uz);
    let up = match normalize(&up) {
        Some(v) => v,
        None => {
            return Err(GenError::Invalid(
                "rotary could not lay the axis down - the fit is degenerate".into(),
            ))
        }
    };
    let v = up.cross(ax.dir); // u x v = axis, by construction
    let f = TiltedFrame {
        origin: ax.origin,
        u: [ax.dir.x, ax.dir.y, ax.dir.z],
        v: [v.x, v.y, v.z],
        axis: [up.x, up.y, up.z],
    };
    let local = f.local_facets(facets);
    let mut q = stations(&local, p, &f, &|h| h - ax.origin.z);
    // #15: the raster's certified diagonal hops and the rings' ramped
    // first contacts are an UPRIGHT tool's verbs - on the bar's flank
    // and end, posed radially, they run down the tool's own axis. The
    // radial relink owns every entry here, ramped below under the
    // sloped-entry law.
    q.ramp = false;
    q.entry_ramp = false;
    let r = p.tool_diameter * 0.5;
    // #2970: the laid raster finishes the bar from above only. Half B
    // is the same frame turned half a turn about the axis; each half
    // keeps the cut over its own upper half, B carried back into A's
    // frame ([`halves::both`] states the seam).
    let fb = TiltedFrame { v: [-v.x, -v.y, -v.z], axis: [-up.x, -up.y, -up.z], ..f };
    let local_b = fb.local_facets(facets);
    let mut qb = stations(&local_b, p, &fb, &|h| h - ax.origin.z);
    (qb.ramp, qb.entry_ramp) = (false, false);
    // The pass spacing the halves read their reach by: the stepover, or
    // the scallop step of the ball at the tolerance, the larger.
    let spacing = p.stepover.unwrap_or(0.0).max((8.0 * r * p.tolerance).max(0.0).sqrt());
    let half_a = finishing3d::swarf(&local, &q, 1.0, 0.0).map_err(|e| to_world(e, &f, &ax, c, r));
    let half_b = finishing3d::swarf(&local_b, &qb, 1.0, 0.0).map_err(|e| to_world(e, &fb, &ax, c, r));
    let base = match (half_a, half_b) {
        (Ok(a), Ok(b)) => halves::both(&a, &b, &local, r, spacing, q.bottom_z, p.tolerance),
        // A half with no surface under it (an open shell over one side
        // only) leaves the other half's whole cut, as before.
        (Ok(a), Err(GenError::Empty(_))) => a,
        (Err(GenError::Empty(_)), Ok(b)) => halves::only_b(&b, &local, r, spacing, q.bottom_z, p.tolerance),
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    // #2343: a held end's face is inside the bar - the cut keeps to
    // the model's side of its plane, before the links are built.
    let local_stock = stock.map(|s| f.local_facets(s));
    let window = rotary_ends::held(&local, local_stock.as_deref(), r, p.tolerance)?;
    let base = match &window {
        Some(w) => rotary_ends::clip(&base, w),
        None => base,
    };
    // #2848: picked faces keep the cut to their radial shadow, before
    // the links are built - the relink then links the kept pieces.
    let base = match picked {
        None => base,
        Some(faces) => {
            let kept = rotary_pick::Shadow::of(&f.local_facets(faces)).map(|sh| rotary_pick::clip(&base, r, &sh));
            match kept {
                Some(k) if k.moves.iter().any(|m| m.kind == MoveKind::Feed) => k,
                _ => {
                    return Err(GenError::Empty(
                        "Rotary Parallel: no pass lies over the picked faces - the tool stands on the radial through each cut point, so it reaches only faces of the turned surface; pick those, or set the geometry to the whole model".into(),
                    ))
                }
            }
        }
    };
    let span = window.as_ref().map_or((f64::NEG_INFINITY, f64::INFINITY), |w| (w.lo, w.hi));
    // M13.8: the raster's Z lifts become radial links - out along the
    // radius to the retract cylinder, index, back in along the radius
    // ([`rotary_links`]); the planes read as radii in the laid frame.
    let hop = q.retract_z.map_or(q.clearance_z, |h| h.min(q.clearance_z));
    let rho = rotary_links::local_rho_max(&local);
    // #15: under the sloped-entry law (`entry_ramp`, the centre-cutting
    // guard's retry) each infeed ramps along the pass where the ramp is
    // proved clear of the part; off, every infeed stays a radial plunge.
    let ramp = p.entry_ramp.then_some(rotary_links::Ramp { deg: p.ramp_angle, facets: &local, tol: p.tolerance, span });
    let base = rotary_links::radial_relink(&base, r, q.clearance_z, hop, rho, p.feed, p.plunge_feed, ramp)
        .map_err(|e| to_world(e, &f, &ax, c, r))?;
    let mut out = posed(&f, &base, &ax, c, r)?;
    // #2970: the raster's and the rings' notes (samples refused, rest
    // reported not cut) reach the path - never only the ramp's.
    out.notes.extend(base.notes.iter().cloned());
    for e in window.iter().flat_map(|w| &w.ends) {
        let at = f.to_world(Point3::new(e.station, 0.0, 0.0));
        out.notes.push(format!(
            "Rotary Parallel: the stock goes on {:.3} mm past the end of the model at ({:.3}, {:.3}, {:.3}) - that end face is left, and the tool stops at its plane",
            e.beyond, at.x, at.y, at.z
        ));
    }
    Ok(out)
}

/// #1015: a refusal raised in the planning frame, carried to the world
/// the path would have gone to. Its ghost rides [`posed`] - the same
/// pose law and world exit the finished path takes, so every point
/// lands where the posted path's would - and its spot rides the
/// frame's own `to_world`. A ghost that exit refuses (a cycle, nothing
/// finite) is dropped, never drawn in the planning frame.
fn to_world(e: GenError, f: &TiltedFrame, ax: &Axis, c: &AxisControl, r: f64) -> GenError {
    e.map_location(|q| f.to_world(q)).map_ghost(|t| posed(f, t, ax, c, r).unwrap_or_default())
}

/// The contour frame: the axis as local z, the canonical foot as
/// origin. `TiltedFrame::new` refuses a zero axis; a fitted unit
/// direction never is one, and the mapping names the impossibility.
fn frame(ax: &Axis) -> Result<TiltedFrame, GenError> {
    TiltedFrame::new(ax.origin, [ax.dir.x, ax.dir.y, ax.dir.z]).ok_or_else(|| {
        GenError::Invalid("the fitted axis is not a direction - the mesh is corrupt".into())
    })
}

/// The params as their planning frame reads them: the pattern
/// geometry untouched, the heights through `hmap` (axial stations
/// for the contour, plain local z for the laid raster - both exact
/// when the part stands on the table's z), and the ladder's top
/// starting where the local work actually tops out.
fn stations(
    local: &[Triangle],
    p: &Finish3DParams,
    f: &TiltedFrame,
    hmap: &dyn Fn(f64) -> f64,
) -> Finish3DParams {
    let mut q = *p;
    q.clearance_z = hmap(p.clearance_z);
    q.retract_z = p.retract_z.map(hmap);
    q.bottom_z = hmap(p.bottom_z);
    let top = hmap(p.top_z);
    q.top_z = local.iter().fold(top, |x, t| {
        x.max(f.to_local(t.a).z).max(f.to_local(t.b).z).max(f.to_local(t.c).z)
    });
    q
}

/// The pose stamp and the world exit in one pass. The base ladder is
/// a vertical ball's (finishing3d's tip convention, in the planning
/// frame), so its BALL CENTERS are the points `r` up the frame's own
/// axis - that sphere is what the ladder's tangency law placed, and
/// it is what every pose must keep. Each emitted point is that center
/// carried to the world and backed off `r` along the pose it holds
/// ([`super::retip`]): cutting moves the radial the lean law commands
/// about the fitted axis, rapids the pure radial at their center
/// (held where none exists), so the post's modal tilt and the path
/// agree on every record. Arcs ride as their own 3-D chords: a pose
/// turning along the circle moves the tip off any circle the G2/G3
/// contract could state, so every chord end is computed fresh by the
/// law, never interpolated. A cycle would ask the spindle to stand
/// still inside a moving-axis path: refused upstream, named here.
fn posed(
    f: &TiltedFrame,
    base: &Toolpath,
    ax: &Axis,
    c: &AxisControl,
    r: f64,
) -> Result<Toolpath, GenError> {
    if !base.cycles.is_empty() {
        return Err(GenError::Invalid(
            "rotary cannot pose drill cycles - the spindle never stands still here".into(),
        ));
    }
    if base.moves.is_empty() {
        return Err(GenError::Empty("the rotary ladder planned no moves".into()));
    }
    let center = |q: Point3| along(f.to_world(q), f.axis, r);
    // M13.8: the program opens ON the clearance pose its opening
    // traverse was heading for, holding the pure radial there - never
    // at the frame's home column (the axis line itself, for rings).
    let (skip, at) = rotary_links::opening(base);
    let c0 = center(at);
    let pose0 = radial_at(ax, c0);
    if pose0.is_none() {
        tracing::warn!("multiaxis: the rotary start {c0:?} stands on the axis; it opens upright");
    }
    let t0 = tip_of(c0, pose0, r);
    let mut b = PathBuilder::new(t0);
    b.set_axis(pose0);
    b.push(MoveKind::Rapid, t0, None);
    let mut held: Option<[f64; 3]> = pose0;
    let mut prev = c0;
    for mv in &base.moves[skip..] {
        let air = mv.kind == MoveKind::Rapid;
        let mut ends: Vec<Point3> = match mv.arc {
            None => Vec::new(),
            Some(_) => mv.segments3(ARC_FIT_TOL).into_iter().map(|(_, e)| e).collect(),
        };
        if ends.is_empty() {
            ends.push(mv.to);
        }
        for q in ends {
            let w = center(q);
            let pose = if air {
                radial_at(ax, w).or(held)
            } else {
                match cut_pose(ax, c, w, w - prev) {
                    Some(a) => Some(a),
                    None => {
                        tracing::warn!("multiaxis: no radial exists at {w:?}; the move holds vertical");
                        Some([0.0, 0.0, 1.0])
                    }
                }
            };
            b.set_axis(pose);
            held = pose;
            b.push(mv.kind, tip_of(w, pose, r), mv.feed);
            prev = w;
        }
    }
    let out = b.finish();
    if !out.all_finite() {
        return Err(GenError::Invalid("a rotary path left the finite horizon".into()));
    }
    Ok(out)
}

/// The cutting pose at a world ball center `w`: out of the axis line
/// to the center, along the travel `t` through the lean law. Where the
/// center lies ON the axis no radial exists - None, and the caller
/// says so, never silently.
fn cut_pose(ax: &Axis, c: &AxisControl, w: Point3, t: Point3) -> Option<[f64; 3]> {
    let n = radial_at(ax, w)?;
    let n = Point3::new(n[0], n[1], n[2]);
    let a = match normalize(&t) {
        Some(t) => command_axis(n, t, c)?,
        None => n, // nowhere to lead: hold the radial
    };
    Some([a.x, a.y, a.z])
}

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if !l.is_finite() || l <= 1e-12 {
        return None;
    }
    Some(Point3::new(v.x / l, v.y / l, v.z / l))
}


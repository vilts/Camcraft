//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::ladder::levels;
use super::params::{Finish3DParams, FinishPattern, dirs, refinement_floor, scallop_stepdown, scallop_stepover, validate};
use super::support::{Sup, build_sups, plunge_support};
use super::march::{emit_runs, march_curve};
use super::rim::rim_rows;
use super::rings::emit_rings;
/// One Z-following raster pass: lines perpendicular to `(ux, uy)`
/// across the shallow facets' padded bounds, stepped by the cos-law
/// (or an explicit `stepover`), each walked by `march_curve`'s laws
/// and delivered by `emit_runs`' choreography. Serpentine within
/// the pass: every piece of an odd line runs back, so the next
/// line starts where this one's travel ended.
/// Returns whether it emitted anything.
pub(super) fn raster_pass(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &[Sup],
    ux: f64,
    uy: f64,
    continuing: bool,
) -> Result<bool, GenError> {
    raster_pass_spaced(b, p, r, sups, (ux, uy), continuing, p.tolerance)
}

/// [`raster_pass`] with the line SPACING authored by `spacing_tol`
/// (the scallop the pass may leave) while every gouge-side law - the
/// march's deviation, its chord budget, its first sample step - keeps
/// `p.tolerance`. The multi-pass swarf's coarse rungs relax only the
/// former (M13.8): a coarse scallop is a promise to the next rung, a
/// coarse chord is a bite into the part no later rung can give back.
pub(super) fn raster_pass_spaced(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &[Sup],
    (ux, uy): (f64, f64),
    continuing: bool,
    spacing_tol: f64,
) -> Result<bool, GenError> {
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut mz_min = 1.0f64;
    for s in sups.iter().filter(|s| s.shallow) {
        let (vx0, vy0) = (-uy, ux);
        for (px, py) in [
            (s.xmin, s.ymin),
            (s.xmax, s.ymin),
            (s.xmax, s.ymax),
            (s.xmin, s.ymax),
        ] {
            umin = umin.min(px * ux + py * uy);
            umax = umax.max(px * ux + py * uy);
            vmin = vmin.min(px * vx0 + py * vy0);
            vmax = vmax.max(px * vx0 + py * vy0);
        }
        mz_min = mz_min.min(s.m.z);
    }
    if umin > umax {
        return Ok(false); // all-steep model: the rings alone carry it
    }
    let (vx, vy) = (-uy, ux);
    let step = p.stepover.unwrap_or(scallop_stepover(r, spacing_tol) * mz_min);
    const MAX_LINES: f64 = 8192.0;
    // Spelled through a binding so the NaN case reads what it is:
    // `step > 0.0` is false when step is not a number at all, and a
    // numberless spacing is refusal, never a silent skip.
    let step_positive = step > 0.0;
    if !step_positive || (vmax > vmin && step < (vmax - vmin) / MAX_LINES) {
        return Err(GenError::Invalid(format!(
            "raster needs {}+ lines at stepover {:.4} (cap {}): widen the stepover or the tolerance",
            ((vmax - vmin) / step.max(1e-12)) as i64,
            step,
            MAX_LINES as i64,
        )));
    }
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let eps_chord = p.tolerance * 0.25;
    let at = |t: f64, v: f64| Point2::new(ux * t + vx * v, uy * t + vy * v);
    // Ground a vertical plunge can actually REACH: a tangent center
    // under a canopy is not a floor the tool may promise (support's
    // column law), and a refusal this pass makes is a note, never a
    // quiet vanishing.
    let trimmed = std::cell::Cell::new(0usize);
    let sup = |q: Point2| {
        let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, sups);
        if cut {
            trimmed.set(trimmed.get() + 1);
        }
        c
    };

    let n_lines = ((vmax - vmin) / step).floor() as i64 + 1;
    let line = |v: f64| {
        march_curve(
            umin,
            umax,
            &|t: f64| at(t, v),
            &|a, c| (c - a).abs(),
            &|_| 1.0,
            &sup,
            delta0,
            p.tolerance,
            eps_chord,
            refinement_floor(delta0, p.min_step),
        )
    };
    // (offset, pieces, runs back) per line; serpentine parity is the
    // grid index's, so a rim row (#46, `rim`) takes its empty
    // neighbour's parity and every grid line keeps its own.
    type Line = (f64, Vec<Vec<(Point2, f64)>>, bool);
    let mut lines: Vec<Line> = (0..n_lines.max(1))
        .map(|j| {
            let v = vmin + step * j.min(n_lines - 1) as f64;
            (v, line(v), j % 2 == 1)
        })
        .collect();
    let vs: Vec<f64> = lines.iter().map(|l| l.0).collect();
    let filled: Vec<bool> = lines.iter().map(|l| !l.1.is_empty()).collect();
    // The probes are questions, not cuts: their trimmed samples are
    // not this pass's notes (the chosen row re-marches and counts).
    let seen = trimmed.get();
    let (low, high) = rim_rows(&vs, &filled, vmax, step, spacing_tol, &mut |v| !line(v).is_empty());
    trimmed.set(seen);
    if let Some(v) = high {
        let jl = filled.iter().rposition(|f| *f).unwrap_or(0);
        lines.insert(jl + 1, (v, line(v), (jl + 1) % 2 == 1));
    }
    if let Some(v) = low {
        let jf = filled.iter().position(|f| *f).unwrap_or(0);
        lines.insert(jf, (v, line(v), jf % 2 == 0));
    }
    let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
    for (_, mut pieces, back) in lines {
        if back {
            for pc in pieces.iter_mut() {
                pc.reverse();
            }
        }
        runs.extend(pieces);
    }
    if trimmed.get() > 0 {
        b.note(format!(
            "raster: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
            trimmed.get()
        ));
    }
    if runs.is_empty() {
        return Ok(false);
    }
    emit_runs(b, p, r, sups, &runs, continuing);
    Ok(true)
}

/// The raster strategies (Parallel, ParallelMulti, Scallop): the
/// shallow terrain cut Z-following in each direction the pattern
/// asks for - once (Parallel), across the axes (ParallelMulti), or
/// across the diagonals (Scallop); then the waterline rings run the
/// steep flanks at the same scallop ladder - the auto-split, in one
/// toolpath, with the two strategies meeting at the shared 45°
/// jurisdiction line.
pub fn parallel(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    // The crossing pair's directions. Scallop's are ±45 degrees:
    // the axes are already taken by ParallelMulti, and on prismatic
    // terrain the diagonals are the only directions no single-axis
    // raster ever crosses head-on.
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let crossed = matches!(p.pattern, FinishPattern::ParallelMulti | FinishPattern::Scallop);
    let (ax, ay, bx, by) = match p.pattern {
        FinishPattern::Scallop => (d, d, d, -d),
        FinishPattern::ParallelMulti => (1.0, 0.0, 0.0, 1.0),
        _ => (1.0, 0.0, 0.0, 0.0),
    };
    let mut emitted = raster_pass(&mut b, p, r, &sups, ax, ay, false)?;
    if crossed {
        // #1015: the crossing pass refuses with the first one built.
        emitted |= raster_pass(&mut b, p, r, &sups, bx, by, emitted).map_err(|e| e.with_ghost_moves(b.moves()))?;
    }
    // Then the rings, from wherever the raster left the tool: the
    // Some(clearance) marker makes the first ring's level-change
    // choreography climb up before anything else.
    rings_and_tail(b, p, r, facets, emitted)
}

/// The blend strategy (M2.5 D): Parallel's raster, then the ring
/// ladder with the border moved DOWN to the band line - every level
/// walks its loops, but a run survives when the tangency reaches
/// past 30 degrees instead of past 45. The 30-45° transition is
/// therefore ridden twice, once from each direction: the cusp
/// promises of the two families differ by less than the tolerance
/// itself there (`4/3` of the ring's own law at the band's flat
/// edge, per `BLEND_MZ`'s note), so the double cut is a shave, not
/// a waste - it removes the staircase the hard 45° border leaves
/// where the raster's last line and the ladder's first ring fail to
/// meet. Flat ground answers with lines only; the ladder never
/// rides a loop that touches nothing past the band.
pub fn blend(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let emitted = raster_pass(&mut b, p, r, &sups, 1.0, 0.0, false)?;
    rings_and_tail(b, p, r, facets, emitted)
}

/// The shared close of every raster-family strategy: the steep
/// flanks ride the waterline rings from wherever the shallow pass
/// left the tool, then the clearance exit, the emptiness refusal
/// and the finiteness gate. `parallel`, `radial` and `spiral` all
/// end here, so no strategy can drift from the ladder.
pub(super) fn rings_and_tail(
    mut b: PathBuilder,
    p: &Finish3DParams,
    r: f64,
    facets: &[Triangle],
    emitted: bool,
) -> Result<Toolpath, GenError> {
    // #1015: the rings refuse with the shallow pass (and any rings) built.
    let zs = levels(p, r, facets).map_err(|e| e.with_ghost_moves(b.moves()))?;
    let ds = dirs(r, p.tolerance);
    let mut last = Some(p.clearance_z);
    let mut emitted = emitted;
    emitted |= emit_rings(&mut b, p, r, facets, &zs, &ds, &mut last).map_err(|e| e.with_ghost_moves(b.moves()))?;
    if !emitted {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The swarf pattern (M5.1 D): the raster's whole machinery with the
/// ruling's DIRECTION. Lines run along `(ux, uy)` - the dominant
/// straight-line direction of a ruled (bladed) region, found by
/// `crate::multiaxis::ruling` - stepping across it at the same
/// cos-law promise; the steep flanks ride the rings, as they do for
/// every raster family. Geometry is the parallel pattern's exactly:
/// what swarf adds is that the pass direction is the surface's own,
/// so a ball tangent to the flank touches along a ruling, and the
/// orientation the 5-axis stamp writes is the tool lying WITH the
/// blade instead of against it.
pub fn swarf(
    facets: &[Triangle],
    p: &Finish3DParams,
    ux: f64,
    uy: f64,
) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let (ux, uy) = norm_dir(ux, uy)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let emitted = raster_pass(&mut b, p, r, &sups, ux, uy, false)?;
    rings_and_tail(b, p, r, facets, emitted)
}

/// The tolerance-aware multi-pass swarf (advanced swarf, M5.1 D):
/// the SAME path family run coarse-first. Pass i from the top is
/// SPACED at `tol·4^i` - every scallop law in the kernel is tolerance-
/// authored, so the rough pass lands at DOUBLE the step and DOUBLE
/// the level stepdown, fourfold the cusp - while its gouge budget
/// stays the nominal `tol` (M13.8); the finishing pass then
/// rides the nominal ladder. Each pass's centers stand tangent to
/// the MODEL (never to the previous pass), so the finishing pass's
/// promise does not depend on the rough pass having helped: the
/// pair is the shop's chip-thinning sequence, and a wrong pass
/// count cannot hide a gouge. `passes` is clamped by the caller's
/// sane door; here 0 and nonsense refuse by name.
pub fn swarf_multi(
    facets: &[Triangle],
    p: &Finish3DParams,
    ux: f64,
    uy: f64,
    passes: u32,
) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    if passes == 0 || passes > 4 {
        return Err(GenError::Invalid(format!(
            "swarf multi-pass needs 1..4 passes, got {passes}"
        )));
    }
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let (ux, uy) = norm_dir(ux, uy)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    // The gouge budget never relaxes (M13.8): the march, the ring
    // chords and the convex scan run at the nominal tolerance on
    // every rung, so no rung bites past it; only the SPACING - the
    // raster's step and the ring ladder's stepdown, both scallop
    // laws - is authored by the rung's `tol·4^i`. Relaxing the whole
    // tolerance let the coarse rung's chords sink ~4·tol (0.39 mm at
    // tol 0.1 on the capped blade), a conviction the finish rung,
    // tangent to the model, can never cut back.
    let ds = dirs(r, p.tolerance);
    for i in (0..passes).rev() {
        let mut q = *p;
        let mut spacing = *p;
        if i > 0 {
            spacing.tolerance = p.tolerance * (4.0f64).powi(i as i32);
            // An explicit stepover rides the same 2x-per-pass ladder
            // (scallop grows with the square of the spacing).
            q.stepover = p.stepover.map(|s| s * (2.0f64).powi(i as i32));
        }
        // #1015: a rung refuses with the rungs before it built.
        let ghost = |e: GenError, b: &PathBuilder| e.with_ghost_moves(b.moves());
        emitted |= raster_pass_spaced(&mut b, &q, r, &sups, (ux, uy), emitted, spacing.tolerance).map_err(|e| ghost(e, &b))?;
        let zs = levels(&spacing, r, facets).map_err(|e| ghost(e, &b))?;
        let mut last = Some(q.clearance_z);
        emitted |= emit_rings(&mut b, &q, r, facets, &zs, &ds, &mut last).map_err(|e| ghost(e, &b))?;
    }
    if !emitted {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The pass direction made honest: finite, non-zero, unit. A raster
/// needs a direction to run in; a zero or NaN pair is a caller's
/// bug, not an isotropic model, and the refusal says so.
fn norm_dir(ux: f64, uy: f64) -> Result<(f64, f64), GenError> {
    let l = (ux * ux + uy * uy).sqrt();
    if !l.is_finite() || l <= 1e-12 {
        return Err(GenError::Invalid(
            "swarf needs a finite non-zero pass direction".into(),
        ));
    }
    Ok((ux / l, uy / l))
}

/// The shallow terrain's polar stage: its footprint center, the
/// circumscribed radius over the bbox corner (one stepover of
/// margin added by the caller, so the rim's ground is reached),
/// and the smallest shallow tilt - the cos-law's `m_z`. `None`
/// when every facet is steep: the rings alone carry the model,
/// exactly as the raster answers to that case.
pub(super) fn shallow_footprint(sups: &[Sup]) -> Option<(Point2, f64, f64)> {
    let (mut x0, mut x1, mut y0, mut y1) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut mz = 1.0f64;
    for s in sups.iter().filter(|s| s.shallow) {
        x0 = x0.min(s.xmin);
        x1 = x1.max(s.xmax);
        y0 = y0.min(s.ymin);
        y1 = y1.max(s.ymax);
        mz = mz.min(s.m.z);
    }
    if x0 > x1 {
        return None;
    }
    let c = Point2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    Some((c, (x1 - c.x).hypot(y1 - c.y), mz))
}

/// The placement refusals, shared: a spacing without a number is
/// refusal, never a silent skip (the `step_positive` law of the
/// raster, spoken in polar words); and a family needing more
/// ground lines than the cap is the user's stepover asking for
/// hours of path - widen it, or the tolerance, and try again.
pub(super) fn placement_guard(step: f64, lines: f64, family: &str, cap: f64) -> Result<(), GenError> {
    let step_positive = step > 0.0;
    if !step_positive || lines > cap {
        return Err(GenError::Invalid(format!(
            "{family} needs {} ground lines at stepover {:.4} (cap {cap}): widen the stepover or the tolerance",
            lines.max(0.0) as i64,
            step,
        )));
    }
    Ok(())
}

//! The swarf raster family (M5.1 D), split from `raster.rs` (#2431) along
//! the pattern seam; structure only. Source: our own design notes.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::ladder::levels;
use super::params::{dirs, validate, Finish3DParams};
use super::raster::{note_waterlines_off, raster_pass, raster_pass_spaced, rings_and_tail_with_trim};
use super::rings::emit_rings_with_trim;
use super::support::build_sups;

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
    // Swarf tilts off +Z in 5-axis stamping; keep under-equator rings intact.
    rings_and_tail_with_trim(b, p, r, facets, emitted, false)
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
        if p.waterlines {
            let zs = levels(&spacing, r, facets).map_err(|e| ghost(e, &b))?;
            let mut last = Some(q.clearance_z);
            emitted |= emit_rings_with_trim(&mut b, &q, facets, &zs, &ds, &mut last, false)
                .map_err(|e| ghost(e, &b))?;
        } else if p.top_z < p.bottom_z {
            return Err(ghost(GenError::Invalid("waterline bottom is above its top".into()), &b));
        }
    }
    if !emitted {
        let msg = if p.waterlines {
            "no surface reaches within one ball radius of the requested depths"
        } else {
            "no shallow surface inside the requested depths, and the waterline ring pass is off - enable waterlines to cut steep flanks"
        };
        return Err(GenError::Empty(msg.into()));
    }
    if !p.waterlines {
        note_waterlines_off(&mut b, p, r, facets);
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

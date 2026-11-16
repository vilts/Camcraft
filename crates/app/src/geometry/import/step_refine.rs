//! The STEP importer's tessellation schedule: coarse first, then halve the
//! chord tolerance toward the target while the facet budget allows, and
//! report the tolerance of the mesh it KEPT (#864).
//!
//! Provenance: our own design (bounded refinement under a memory budget);
//! the chord-tolerance meaning is the tessellator's (truck, Apache-2.0).
//! Nothing from any CAM vendor.
//!
//! Split out of `step.rs` so the bookkeeping is testable without a STEP
//! kernel: `triangulate` is any pass at a tolerance that either returns
//! the facets or reports the budget was crossed.

use super::ImportError;

/// Why one tessellation pass produced no mesh.
#[derive(Debug)]
pub(super) enum PassError {
    /// The pass crossed the facet budget.
    Over,
    /// A face ran past its evaluation cap (#1536). `refusal` names it;
    /// `continuous` says its surface is proven continuous, so its
    /// subdivision would have ended (`step_continuity`) - the pass was only
    /// expensive, and a coarser pass that met is a sound mesh (#1568).
    Unconverged { continuous: bool, refusal: ImportError },
}

/// The facets of the last pass that fit, and the chord tolerance that pass
/// ran at - never a tolerance whose pass was thrown away.
#[derive(Debug)]
pub(super) struct Kept<T> {
    pub facets: Vec<T>,
    pub tol: f64,
    /// Refinement stopped because a finer pass ran out of evaluations on a
    /// continuous face (#1568), not at the target or the facet budget - so
    /// a larger facet budget would not refine it.
    pub eval_capped: bool,
}

/// Run the schedule. `extent` is the shell's half-size (its largest
/// absolute coordinate), `tol_target` the quality target, `budget` the
/// facet cap for this shell.
///
/// - Coarse start at `max(extent / 4, target)`; a first pass over budget
///   coarsens up to `16 * extent`, then refuses with `TooLarge`.
/// - Refinement halves `tol` (never below the target) while the kept mesh
///   is at most `budget / 8`, for at most 24 passes.
/// - A refinement pass that crosses the budget is dropped and the PREVIOUS
///   mesh is kept, with the previous tolerance. Before #864 the halved
///   tolerance was reported for the coarser mesh, under-reporting its
///   chord error (and saying nothing when the halved value hit the target).
/// - A face that runs out of evaluations (#1536) refuses the model by
///   name, except on a REFINEMENT pass over a face proven continuous: then
///   the coarser mesh is kept with its own tolerance, as for the budget
///   (#1568). A face that may jump is refused even after a coarser pass
///   met, because a pass wider than the jump smears it into a wall.
pub(super) fn refine<T>(
    extent: f64,
    tol_target: f64,
    budget: usize,
    mut triangulate: impl FnMut(f64) -> Result<Vec<T>, PassError>,
) -> Result<Kept<T>, ImportError> {
    let mut tol = (extent * 0.25).max(tol_target);
    let mut kept = loop {
        match triangulate(tol) {
            Ok(facets) => break Kept { facets, tol, eval_capped: false },
            // Face count alone can overflow the budget; coarsen up to a
            // bounded cap. tol grows strictly each retry, so this ends.
            Err(PassError::Over) => {
                if tol >= extent * 16.0 {
                    return Err(ImportError::TooLarge);
                }
                tol = (tol * 4.0).min(extent * 16.0);
            }
            // No coarser mesh to fall back to.
            Err(PassError::Unconverged { refusal, .. }) => return Err(refusal),
        }
    };
    // Halving the tolerance at most quadruples the count, so a pass is
    // taken only while the projected mesh stays within budget/2. The loop
    // ends at the target, at the budget, or at the pass cap.
    for _ in 0..24 {
        if kept.tol <= tol_target || kept.facets.len() > budget / 8 {
            break;
        }
        let next = (kept.tol * 0.5).max(tol_target);
        match triangulate(next) {
            Ok(facets) => kept = Kept { facets, tol: next, eval_capped: false },
            // Should not happen (the projection bounds it); if the
            // tessellator's density does not follow the tol^-2 law, keep
            // the last mesh that fit - with ITS tolerance (#864).
            Err(PassError::Over) => break,
            Err(PassError::Unconverged { continuous: true, refusal }) => {
                tracing::warn!(
                    "step: a finer pass (tol {next:e}) ran out of evaluations on a continuous face; kept the converged mesh at tol {:e} ({refusal})",
                    kept.tol
                );
                kept.eval_capped = true;
                break;
            }
            Err(PassError::Unconverged { continuous: false, refusal }) => return Err(refusal),
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake pass: `n(tol)` facets, or `TooLarge` at or below `wall`.
    fn pass(tol: f64, wall: f64, n: impl Fn(f64) -> usize) -> Result<Vec<u8>, PassError> {
        if tol <= wall {
            return Err(PassError::Over);
        }
        Ok(vec![0; n(tol)])
    }

    #[test]
    fn a_dropped_pass_keeps_the_tolerance_of_the_mesh_it_kept() {
        // extent 4: start tol 1.0, then 0.5, 0.25 ... ; the pass at 0.125
        // crosses the budget, so the 0.25 mesh is kept AND reported.
        let mut seen = Vec::new();
        let kept = refine(4.0, 0.001, 1_000_000, |t| {
            seen.push(t);
            pass(t, 0.2, |t| (1.0 / t) as usize)
        })
        .unwrap();
        assert_eq!(kept.tol, 0.25, "the kept mesh's own tolerance, not the halved one");
        assert!(!kept.eval_capped, "stopped by the budget, not by a face's evaluation cap");
        assert_eq!(kept.facets.len(), 4);
        assert_eq!(seen, vec![1.0, 0.5, 0.25, 0.125]);
    }

    #[test]
    fn a_dropped_pass_whose_halved_tolerance_is_the_target_still_reports_the_kept_one() {
        // The halved tolerance reaches the target exactly; the old code then
        // logged nothing although the mesh kept was twice as coarse.
        let kept = refine(4.0, 0.5, 1_000_000, |t| pass(t, 0.5, |_| 10)).unwrap();
        assert_eq!(kept.tol, 1.0);
        assert!(kept.tol > 0.5, "the kept mesh misses the target and must say so");
    }

    #[test]
    fn a_mesh_that_reaches_the_target_reports_the_target() {
        let kept = refine(4.0, 0.25, 1_000_000, |t| pass(t, 0.0, |_| 10)).unwrap();
        assert_eq!(kept.tol, 0.25);
    }

    #[test]
    fn the_budget_stop_reports_the_tolerance_of_the_last_pass() {
        // 100 facets at every pass, budget 400: 100 > 400/8 stops after the
        // first pass, at the coarse start.
        let kept = refine(4.0, 0.001, 400, |t| pass(t, 0.0, |_| 100)).unwrap();
        assert_eq!(kept.tol, 1.0);
    }

    /// A fake pass that runs out of evaluations at or below `wall`.
    fn tripping(tol: f64, wall: f64, continuous: bool) -> Result<Vec<u8>, PassError> {
        if tol <= wall {
            return Err(PassError::Unconverged { continuous, refusal: ImportError::Step("face #7 tripped".into()) });
        }
        Ok(vec![0; 10])
    }

    #[test]
    fn a_finer_pass_tripping_on_a_continuous_face_keeps_the_coarser_mesh() {
        let kept = refine(4.0, 0.001, 1_000_000, |t| tripping(t, 0.3, true)).expect("the coarser mesh is kept");
        assert_eq!(kept.tol, 0.5, "the kept mesh's own tolerance");
        assert!(kept.eval_capped, "the kept mesh says why it stopped: a larger budget would not refine it");
    }

    #[test]
    fn a_finer_pass_tripping_on_a_face_that_may_jump_refuses_by_name() {
        let err = refine(4.0, 0.001, 1_000_000, |t| tripping(t, 0.3, false)).expect_err("a smear is never kept");
        assert!(err.to_string().contains("face #7"), "{err}");
    }

    #[test]
    fn a_first_pass_trip_refuses_even_on_a_continuous_face() {
        let err = refine(4.0, 0.001, 1_000_000, |t| tripping(t, 10.0, true)).expect_err("no coarser mesh exists");
        assert!(err.to_string().contains("face #7"), "{err}");
    }
}

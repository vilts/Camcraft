//! What an operation kind's gouge verdicts need of its model's fidelity
//! (#864, #904): the tolerance its envelope law is judged at, the
//! smallest tolerance any of its laws holds, and whether those laws read
//! a coarse model's excess at all.
//!
//! Provenance: our own registry of our own verify laws; `finish_tol_of`
//! is the envelope budget `generate` always handed to `verify`, moved
//! here unchanged so the program gate reads the same number.
//!
//! ## Which law judges which kind (#904)
//!
//! A kind is judged on a reduced-fidelity model only when EVERY law that
//! convicts it against the model reads the excess; otherwise it is
//! refused at the program gate.
//!
//! - Turning, the nose kinds (facing, profile, chamfer, taper, multi-
//!   pass, boring): `verify`'s envelope law (f) on the nose circle.
//! - Turning, the band kinds (groove, part, thread, form): the meridian
//!   scan (`turning::*_warnings_excess`), and for a Part's inspection
//!   ask the composed scan at the post (`operation::probe`).
//! - Every turning kind: the meridian rapid, bar-feed and holder scans,
//!   whose margin is tightened the same way ([`tightened`]).
//! - The rotary kinds: law (f), and the radial field law's band and
//!   rapid allowances (`rotary_law`).
//! - The multi-axis ball kinds (5-axis contour and parallel, flow,
//!   multi-axis contour and rough): law (f) - a posed ball is the same
//!   ball about the same centre, so the envelope scan is the whole
//!   gouge verdict of the cut.
//!
//! Still refused, each for its reason:
//!
//! - Swarf and advanced swarf: the flank that does the cutting is proved
//!   clear only inside the generator (`multiaxis::stamp_pose`,
//!   `flank_clear` at the plan tolerance); no verify law re-measures it,
//!   so there is nothing here to tighten until that proof reads the
//!   excess.
//! - The cone and engrave rides (2D chamfer, deburr, trace, engrave),
//!   probe, the cross drill and plasma: their verdicts are construction
//!   laws or their own modules, not yet excess-aware.

use super::material::GOUGE_TOL;
use super::{OpParams, Section};
use crate::verify::Mode;

/// The envelope law's budget for `params`: the op's own tolerance for the
/// kinds that law judges, `None` for every other kind.
pub(crate) fn finish_tol_of(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Finish3D { tolerance, .. }
        | OpParams::Contour5D { tolerance, .. }
        | OpParams::Parallel5D { tolerance, .. }
        | OpParams::Swarf { tolerance, .. }
        | OpParams::AdvancedSwarf { tolerance, .. }
        | OpParams::RotaryContour { tolerance, .. }
        | OpParams::RotaryParallel { tolerance, .. }
        // The rough ladder's tolerance is exactly this
        // law's budget - its only reading (E2), and
        // the stamped ladder reads it the same way
        // (E3): the positions are the pocket's, the
        // verdict measures them under the pose.
        | OpParams::RotaryOffset { tolerance, .. }
        | OpParams::MultiAxisRough { tolerance, .. }
        // Flow is the finish's law on turning lines:
        // the tolerance is the envelope budget, as
        // always in this group (M5.1 I). The
        // drive-curve contour reads it the same way -
        // its ONLY reading, the path itself comes from
        // the picks (M5.1 J).
        | OpParams::Flow5D { tolerance, .. }
        | OpParams::MultiAxisContour { tolerance, .. }
        // Facing reads it the same way - its ONLY
        // reading: the stations come from the
        // profile, the budget judges them (M5.2 A).
        // The profile kinds read it TWICE (the
        // kernel chords its ride at it) and stand
        // before this same budget (M5.2 B).
        | OpParams::Facing { tolerance, .. }
        | OpParams::ProfileRough { tolerance, .. }
        | OpParams::ProfileFinish { tolerance, .. }
        // The contour rides read it the same way the
        // profile kinds do - TWICE: the kernel chords
        // the ride at it, the door judges the promised
        // line at it, and the envelope scan measures
        // the same skin under this budget (M5.2 E).
        // Without it the verdict cannot measure, and
        // an unmeasurable Envelope op refuses.
        | OpParams::Chamfering { tolerance, .. }
        | OpParams::TaperTurning { tolerance, .. }
        // The boring bar reads it the contour twins'
        // way - TWICE, and from the inside: the
        // kernel chords the wall and the center curve
        // at quarters of it, and the envelope scan
        // measures the ride against the same
        // promise (M5.2 F).
        | OpParams::BoringTurning { tolerance, .. }
        | OpParams::MultiPassTurning { tolerance, .. } => Some(*tolerance),
        _ => None,
    }
}

/// A verdict tolerance on a model imported coarser than promised: `tol`
/// less `excess` while a positive tolerance is left, `tol` untouched
/// past that - where [`floor_of`] makes the program gate refuse the
/// operation instead (`verify::excess_floor` is the one test both read).
pub(crate) fn tightened(tol: f64, excess: f64) -> f64 {
    crate::verify::excess_floor(tol, excess).unwrap_or(tol)
}

/// The band kinds' meridian-scan tolerance (`turning::revolve`), `None`
/// for every kind that scan does not judge.
pub(crate) fn meridian_tol_of(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::GrooveRough { tolerance, .. }
        | OpParams::GrooveFinish { tolerance, .. }
        | OpParams::Part { tolerance, .. }
        | OpParams::Threading { tolerance, .. }
        | OpParams::FormTurning { tolerance, .. } => Some(*tolerance),
        _ => None,
    }
}

/// A Part's inspection ask, when it states one: the composed blade scan
/// at the post judges the finished part at it (`operation::probe`).
fn ask_of(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Part { inspect_tol, .. } if *inspect_tol > 0.0 => Some(*inspect_tol),
        _ => None,
    }
}

/// The smallest tolerance a law of this kind is held at: `GOUGE_TOL`
/// (the section and solid laws, the rotary bands, the turning margin),
/// the envelope law's own budget, the meridian scan's, and a Part's
/// inspection ask, where the kind carries one. A coarse model's excess
/// must leave all of them positive, or the operation is refused instead
/// of judged.
pub(crate) fn floor_of(params: &OpParams) -> f64 {
    [finish_tol_of(params), meridian_tol_of(params), ask_of(params)]
        .into_iter()
        .flatten()
        .fold(GOUGE_TOL, f64::min)
}

/// Whether every law that convicts this kind against its model reads a
/// coarse model's excess (the module header names them per family): the
/// upright mill kinds (#864), and - #904 - turning, the rotary kinds and
/// the multi-axis ball kinds.
///
/// Not yet: swarf (its flank proof is the generator's), the cone and
/// engrave rides, probe, the cross drill and plasma. On a reduced-
/// fidelity model those are refused at the program gate rather than
/// judged loosely.
pub(crate) fn excess_aware(params: &OpParams) -> bool {
    let info = params.kind().info();
    match info.section {
        Section::Mill2D | Section::Mill3D | Section::Drilling => {
            let judged = matches!(info.mode, Mode::Area | Mode::Boundary)
                || (info.mode == Mode::Envelope && info.section == Section::Mill3D);
            let cross_drill = matches!(params, OpParams::Drill { cross, .. } if !cross.is_empty());
            judged && !cross_drill
        }
        Section::Turning => true,
        Section::MultiAxis => !matches!(params, OpParams::Swarf { .. } | OpParams::AdvancedSwarf { .. }),
        Section::Plasma => false,
    }
}

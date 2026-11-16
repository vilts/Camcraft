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
//! - Probe (#1053): its protected-move law (`probe_op::stylus_hits` and
//!   the stem and body bands of `stylus_reach_hits`) - the stylus slack
//!   gives the excess back (`probe_op::stylus_slack`). A probe cuts
//!   nothing, so no other law reads it.
//! - Plasma (#1053): the kerf law and the cover law
//!   (`plasma::PlasmaParams::kerf_tol`), the only laws that read the
//!   part; the cut-order, kerf-crossing and tip-up laws read the path.
//! - Cross drill (#1053): `cross_part` (`GOUGE_TOL`) and `rapids_radial`
//!   give the excess back; `cross_stock::standoff` and `rapids_clear`,
//!   `cross_entry`, `drill_reach` and `drill_station` grow measured
//!   standoffs, entries, reach and holder bands by [`band_growth`]
//!   while `floor_of` leaves a positive tolerance, and read untightened
//!   past it.
//!
//! Still refused, each for its reason:
//!
//! - Swarf and advanced swarf: the flank that does the cutting is proved
//!   clear only inside the generator (`multiaxis::stamp_pose`,
//!   `flank_clear` at the plan tolerance); no verify law re-measures it,
//!   so there is nothing here to tighten until that proof reads the
//!   excess (#1052).
//! - The cone rides (2D chamfer, deburr) and the engrave rides (trace,
//!   engrave): no law holds them to a tolerance against the model at
//!   all. The ride IS the mesh's edge by construction (law (b) stands
//!   down for both modes), so the cut stands off the true edge by the
//!   mesh's own chord error, which no reading of that mesh can see or
//!   bound - there is no tolerance to give the excess back from. Worse,
//!   deburr finds its edges by the bend between facets: a coarse mesh
//!   of a smooth face bends more between neighbours and can be read as
//!   a crease the ride then breaks - a cut no verify law speaks to.
//!   Judging them needs a stated ride accuracy, a new promise, not a
//!   tightened one (#1053's decision file).

use super::material::GOUGE_TOL;
use super::probe_op::STYLUS_SLACK;
use super::{OpParams, Section};
use crate::plasma::KERF_TOL;
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

/// How much a measured standoff, entry or band grows on a model
/// imported coarser than promised (#1053): `excess` while `GOUGE_TOL`
/// stays positive, `0.0` past that (where [`floor_of`] makes the
/// program gate refuse the operation instead).
pub(crate) fn band_growth(excess: f64) -> f64 {
    (GOUGE_TOL - tightened(GOUGE_TOL, excess)).max(0.0)
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

/// The slack of a kind's own law that is not a `verify` tolerance
/// (#1053): the probe's stylus slack, the plasma kerf law's.
fn own_law_tol_of(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Probe { .. } => Some(STYLUS_SLACK),
        OpParams::PlasmaProfile(_) => Some(KERF_TOL),
        _ => None,
    }
}

/// The smallest tolerance a law of this kind is held at: `GOUGE_TOL`
/// (the section and solid laws, the rotary bands, the turning margin),
/// the envelope law's own budget, the meridian scan's, a Part's
/// inspection ask, and the probe's and plasma's own slack, where the
/// kind carries one. A coarse model's excess must leave all of them
/// positive, or the operation is refused instead of judged. `GOUGE_TOL`
/// is the base for every kind, so plasma (whose kerf law holds 0.025
/// mm) is refused at the same 0.01 mm the rest are - stricter than its
/// own law needs, never looser.
pub(crate) fn floor_of(params: &OpParams) -> f64 {
    [finish_tol_of(params), meridian_tol_of(params), ask_of(params), own_law_tol_of(params)]
        .into_iter()
        .flatten()
        .fold(GOUGE_TOL, f64::min)
}

/// Whether every law that convicts this kind against its model reads a
/// coarse model's excess (the module header names them per family): the
/// upright mill kinds (#864), - #904 - turning, the rotary kinds and
/// the multi-axis ball kinds, and - #1053 - probe, plasma and cross
/// drill.
///
/// Not yet: swarf (its flank proof is the generator's), the cone and
/// engrave rides (no tolerance holds them to the model), the wire (#532)
/// and the sinker (#541). On a reduced-fidelity model those are refused
/// at the program gate rather than judged loosely.
pub(crate) fn excess_aware(params: &OpParams) -> bool {
    if matches!(params, OpParams::Probe { .. }) {
        return true;
    }
    let info = params.kind().info();
    match info.section {
        Section::Mill2D | Section::Mill3D | Section::Drilling => {
            matches!(info.mode, Mode::Area | Mode::Boundary)
                || (info.mode == Mode::Envelope && info.section == Section::Mill3D)
        }
        Section::Turning => true,
        Section::MultiAxis => !matches!(params, OpParams::Swarf { .. } | OpParams::AdvancedSwarf { .. }),
        Section::Plasma => true,
        // #532: the wire's kerf law is not excess-aware yet either.
        // #541: nor the sinker's burn verdict.
        Section::WireEdm | Section::Sinker => false,
    }
}

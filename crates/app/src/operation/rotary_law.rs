//! The rotary kinds' band and rapid verdict (M13): the radial field
//! law of `verify::rotary_hits` run with the same fitted axis the
//! generator posed about, the setup's stock solid and the part.
//! On a model imported coarser than promised (#904) both allowances
//! give the excess back: the field reads each ray's radius off the
//! facets, and a facet of a convex skin stands inside the true one.
//! Provenance: our own wiring of the sim crate's law (its sources are
//! cited there); no vendor material consulted.

use crate::geometry::Triangle;
use crate::verify::{self, Warning};

use super::fidelity_law::tightened_strict as tightened;
use super::material::GOUGE_TOL;
use super::types::OpParams;

/// Field cell on the widest skin and sample step along a move, mm:
/// a quarter millimetre - finer than any band's clearance this law
/// is asked to resolve, and the field stays a few tens of thousands
/// of cells on a bar a hand holds.
const CELL: f64 = 0.25;
const STEP: f64 = 0.25;

/// The rotary kinds' finish budget (their `tolerance`), None for every
/// other kind - which is exactly where this law does not run.
pub(super) fn rotary_tolerance(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::RotaryContour { tolerance, .. }
        | OpParams::RotaryParallel { tolerance, .. }
        | OpParams::RotaryOffset { tolerance, .. }
        // The flow band's scallop budget (#2826): the same radial
        // field judges its passes and cylinders.
        | OpParams::RotaryFlow { tolerance, .. } => Some(*tolerance),
        _ => None,
    }
}

/// Append the radial field law's warnings to `warns` (deduped). The
/// axis is the job's own resolution (`fixture::rotary_axis_of`, the
/// stated line judged or the fit): the very line the generator turned
/// about, so the field reads the same frame the path was posed in.
/// `None` (a selection source, a refused fit) falls back to today's
/// own fit here, which fails closed as a Gouge at the first cut,
/// never a silent pass.
/// `excess` is the judged model's chord error past the import promise
/// (`Material::excess`): each allowance shrinks by it while a positive
/// one is left - past that the program gate refuses the operation
/// (`fidelity_law::floor_of`), and the law reads untightened.
pub(super) fn rotary_into(
    warns: &mut Vec<Warning>,
    path: &crate::cl::Toolpath,
    part: &[Triangle],
    stock: Option<&[Triangle]>,
    cutter: &verify::Cutter,
    (tolerance, excess): (f64, f64),
    axis: Option<&crate::verify::RotaryAxis>,
) {
    let hits = match axis
        .map(|a| verify::RotaryAxis { origin: a.origin, dir: a.dir })
        .or_else(|| crate::multiaxis::axis::fit(part).ok().map(|ax| verify::RotaryAxis { origin: ax.origin, dir: [ax.dir.x, ax.dir.y, ax.dir.z] }))
    {
        Some(ax) => verify::rotary_hits(
            path,
            &ax,
            stock,
            part,
            cutter,
            &verify::RotaryLaw {
                cell: CELL,
                step: STEP,
                allow: tightened(GOUGE_TOL, excess),
                rapid_allow: tightened(tolerance.max(GOUGE_TOL), excess),
            },
        ),
        None => {
            tracing::warn!("rotary verdict: no axis can be read for the radial law; the path is not judged clean");
            path.moves
                .iter()
                .position(|m| m.kind != crate::cl::MoveKind::Rapid)
                .map(|at| vec![Warning::Gouge { at }])
                .unwrap_or_default()
        }
    };
    for w in hits {
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
}

#[cfg(test)]
#[path = "rotary_law_tests.rs"]
mod tests;

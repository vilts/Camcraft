//! The rotary kinds' axis-letter gate (M13.5 B): which machine
//! rotary turns the part a Rotary Contour/Parallel/Offset poses
//! about. The letter is the MACHINE's word, never the kind's: a mill
//! table's trunnion is A or B, a mill-turn's spindle is C, and the
//! fitted axis the generator turned about must be one the mounted
//! machine states - refused by name where it states none.
//!
//! The C half (issue #33): the spindle's C is admitted and answered
//! as 'C'; the caller then stamps the path with the stated C angle
//! (`cl::stamp_c`) - the post's pose law renders a tool AXIS as
//! [A, B, 0] and cannot carry C, so the angle rides beside it, and a
//! C-axis op never posts as A/B swings the mill-turn does not have.
//!
//! Provenance: our own wiring of the machine inventory to the pose
//! law; axis lettering per RS-274 / ISO 6983 convention. No vendor
//! material consulted.

use crate::geometry::Triangle;
use crate::machine::AxisLimits;
use crate::machining2d::GenError;

/// A cosine this close to one reads two unit directions as the same
/// line (either sign - a rotary turns both ways).
const SAME_LINE: f64 = 0.999;

/// The letter the mounted machine gives the rotary line `dir` (unit,
/// world frame) for `kind`. `None` machine, or a machine whose
/// rotaries state no axis vector, checks nothing (the unstated-limit
/// law) and answers `Ok(None)`. A stated inventory must hold a rotary
/// on that line.
pub(super) fn rotary_letter(
    machine: Option<&(String, Vec<AxisLimits>)>,
    dir: [f64; 3],
    kind: &str,
) -> Result<Option<char>, GenError> {
    let Some((model, rotaries)) = machine else {
        return Ok(None);
    };
    let stated: Vec<&AxisLimits> = rotaries.iter().filter(|a| a.axis.is_some()).collect();
    if stated.is_empty() {
        return Ok(None);
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return Err(GenError::Invalid(format!("{kind}: the fitted rotary axis states no direction")));
    }
    let hit = stated.iter().find(|a| {
        a.axis.is_some_and(|v| {
            let vl = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            vl > 1e-12
                && ((v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2]) / (vl * len)).abs() >= SAME_LINE
        })
    });
    let Some(ax) = hit else {
        let names: Vec<&str> = stated.iter().map(|a| a.name.as_str()).collect();
        return Err(GenError::Invalid(format!(
            "{kind} turns the part about ({:.3}, {:.3}, {:.3}); {model} states no rotary on that line (its rotaries: {}) - a mill table's A/B or a mill-turn spindle's C must hold it",
            dir[0] / len,
            dir[1] / len,
            dir[2] / len,
            if names.is_empty() { "none".to_owned() } else { names.join(", ") }
        )));
    };
    Ok(ax.name.chars().next().map(|c| c.to_ascii_uppercase()))
}

/// The rotary kinds' letter door (M13.5 B): the line the kernel will
/// turn the part about, held against the mounted machine's rotary
/// inventory. A STATED axis (#2772 parts 1-2) answers the letter
/// directly - the machine states its own rotaries, and the op's own
/// statement is held to the same inventory below - so no fit is
/// asked; nothing stated keeps the fit the kernel will read, and a
/// fit that fails there is left to the kernel, which refuses it by
/// its own name. The spindle's C answers the line (issue #33): the
/// caller then stamps the path with the axis's C.
pub(super) fn letter_gate(
    machine: Option<&(String, Vec<AxisLimits>)>,
    stated: Option<&crate::multiaxis::axis::Axis>,
    facets: &[Triangle],
    kind: &str,
) -> Result<Option<crate::multiaxis::axis::Axis>, GenError> {
    let ax = match stated {
        Some(ax) => *ax,
        None => match crate::multiaxis::axis::fit(facets) {
            Ok(ax) => ax,
            Err(_) => return Ok(None),
        },
    };
    let letter = rotary_letter(machine, [ax.dir.x, ax.dir.y, ax.dir.z], kind)?;
    Ok((letter == Some('C')).then_some(ax))
}

/// The kind's own link clearance the C unwind law is proven on end
/// to end (#2828): each proven kind names the plane or cylinder its
/// links rest on, [`Unwind::NoProof`] for the rest. A kind with no
/// proof posts only what fits the travel as stamped.
pub(super) enum Unwind {
    /// No end-to-end proof yet: never rewound, refused by name where
    /// its C leaves the travel.
    NoProof,
    /// Links over the axial clearance plane (Rotary Contour).
    Axial,
    /// Links on the retract cylinder (Rotary Flow, #2828).
    Radial,
}

/// Stamp a rotary kind's part-frame path with the spindle C (issue
/// #33) when the gate answered the machine's C for the fitted line
/// `c_axis`; any other letter returns the path untouched. The line
/// must stand along world Z - C turns about Z - and the stamp is split
/// to `tol` (the op's verify budget), refused by name where the law
/// cannot hold (`cl::stamp_c`).
///
/// A machine stating a C travel (`machine`'s rotary named C - the
/// one the machine check judges) gets the stamped path unwound by
/// whole turns at its clear links (`cl::unwind_c`, slice 2): the
/// rewinds ride as orbit rapids every later law judges, and a cut
/// that no rewind can fit is refused here by name. Only a kind whose
/// unwind is proven end to end asks for it - over the axial plane
/// (Rotary Contour) or on the retract cylinder (Rotary Flow, #2828);
/// any other kind is refused by name where its C leaves the travel.
pub(super) fn stamp_spindle_c(
    path: crate::cl::Toolpath,
    c_axis: Option<crate::multiaxis::axis::Axis>,
    (machine, unwind): (Option<&(String, Vec<AxisLimits>)>, Unwind),
    tol: f64,
    kind: &str,
) -> Result<crate::cl::Toolpath, GenError> {
    let Some(ax) = c_axis else {
        return Ok(path);
    };
    // #1015: the kernel's world path is in hand at every refusal below.
    stamp_c_about(&path, ax, machine, unwind, tol, kind).map_err(|e| e.with_ghost(&path))
}

/// The spindle line that C turns the part about: the fitted line `ax`'s
/// X/Y where it crosses z = 0. Refused by name when the line does not
/// stand along world Z (#3096: the live-tool traverse reads it too).
pub(super) fn c_center(ax: &crate::multiaxis::axis::Axis, kind: &str) -> Result<[f64; 2], GenError> {
    let d = ax.dir;
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    if !(len > 1e-12 && (d.z / len).abs() >= SAME_LINE) {
        return Err(GenError::Invalid(format!(
            "{kind}: the part turns about ({:.3}, {:.3}, {:.3}), not the spindle's Z line - C cannot hold it",
            d.x, d.y, d.z
        )));
    }
    // It stands within SAME_LINE of vertical, so the crossing is finite.
    let t = -ax.origin.z / d.z;
    Ok([ax.origin.x + d.x * t, ax.origin.y + d.y * t])
}

/// [`stamp_spindle_c`]'s body, over the path it may refuse.
fn stamp_c_about(
    path: &crate::cl::Toolpath,
    ax: crate::multiaxis::axis::Axis,
    machine: Option<&(String, Vec<AxisLimits>)>,
    unwind: Unwind,
    tol: f64,
    kind: &str,
) -> Result<crate::cl::Toolpath, GenError> {
    let center = c_center(&ax, kind)?;
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { crate::machining2d::ARC_FIT_TOL };
    let stamped = crate::cl::stamp_c(path, center, tol).map_err(|e| GenError::Invalid(format!("{kind}: {e}")))?;
    let Some(c_ax) = c_axis(machine) else {
        return Ok(stamped);
    };
    let model = machine.map_or("this machine", |m| m.0.as_str());
    let sign = c_ax
        .c_z_sense()
        .map_err(|e| GenError::Invalid(format!("{kind}: {model}'s {e}")))?;
    let Some((mach_lo, mach_hi)) = c_ax.travel else {
        return Ok(stamped);
    };
    let (lo, hi) = if sign < 0.0 { (-mach_hi, -mach_lo) } else { (mach_lo, mach_hi) };
    if !matches!(unwind, Unwind::NoProof) {
        // #2828: Rotary Flow links on the retract cylinder, so its
        // rewinds orbit there; Rotary Contour links over the axial
        // plane, and a path whose only clear point is radial is
        // refused by name through the law's own words.
        let unwound = crate::cl::unwind_c(&stamped, center, (lo, hi), tol, matches!(unwind, Unwind::Radial))
            .map_err(|e| GenError::Invalid(format!("{kind}: {e}")))?;
        // #906: a rewind orbit is C-turning rapids, capped only by the
        // C rate the machine states. None stated: refuse the rewind by
        // name rather than post it at an uncapped feed. (Shifting the
        // opening stretch adds no move: the section's first G00 C
        // positions it, at no F.)
        if unwound.moves.len() != stamped.moves.len() && !c_rated(machine) {
            return Err(GenError::Invalid(format!(
                "{kind}: the path needs a C rewind, but the machine file states a C travel but no C rate - a rewind would post at an uncapped feed; state the C axis's feed or rapid rate"
            )));
        }
        return Ok(unwound);
    }
    // A kind whose links the unwind law has not been proven on end to
    // end posts only what fits the travel as stamped - exactly what the
    // machine check passed before slice 2 - and is refused here, by
    // name, where it would need a rewind.
    let (c_lo, c_hi) = stamped
        .moves
        .iter()
        .filter_map(|m| m.rotary)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)));
    if c_lo < lo - 1e-9 || c_hi > hi + 1e-9 {
        let (word_lo, word_hi) = if sign < 0.0 { (-c_hi, -c_lo) } else { (c_lo, c_hi) };
        return Err(GenError::Invalid(format!(
            "{kind}: the path turns C over {word_lo:.3}..{word_hi:.3} deg, past the machine's C travel {mach_lo:.3}..{mach_hi:.3}; unwinding C at links is proven for Rotary Contour over the axial plane and for Rotary Flow on the retract cylinder only, so {kind} is refused where it would need a rewind"
        )));
    }
    Ok(stamped)
}

/// The rotary named C on the mounted machine (the same axis
/// `machine::check`'s C letter gate reads), or None.
fn c_axis(machine: Option<&(String, Vec<AxisLimits>)>) -> Option<&AxisLimits> {
    machine?.1.iter().find(|a| a.rotary && a.name.eq_ignore_ascii_case("C"))
}

/// True when the mounted machine's rotary named C states a rate -
/// its feed or its rapid (deg/min), as the lathe post caps by (#906).
fn c_rated(machine: Option<&(String, Vec<AxisLimits>)>) -> bool {
    let rated = |r: Option<f64>| r.is_some_and(|v| v.is_finite() && v > 0.0);
    machine.is_some_and(|m| m.1.iter().any(|a| a.rotary && a.name.eq_ignore_ascii_case("C") && (rated(a.feed) || rated(a.rapid))))
}

#[cfg(test)]
#[path = "rotary_gate_tests.rs"]
mod tests;

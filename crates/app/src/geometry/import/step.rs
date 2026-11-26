//! STEP reader (#917): every solid of the file read and meshed by the
//! solidcore kernel through `camcraft_geometry::step` (watertight, outward,
//! deviation measured), in millimetres. This module keeps the importer's
//! promise (the chord cap), the facet budget and the refusal wording.
//!
//! Provenance: file syntax per ISO 10303-21 and geometry per ISO 10303-42,
//! read by solidcore-step (Apache-2.0, written from those public
//! standards); our own budget and fidelity rules. Nothing from any CAM
//! vendor.
//!
//! Hardening notes (Prime Directive):
//! - The kernel refuses a file over its size bound, a syntax error (by line),
//!   a length unit it cannot convert and a solid that does not close, by
//!   name; a panic inside the reader or the mesher is caught there and
//!   becomes a refusal.
//! - A model reads whole or not at all: a file with one refused solid is
//!   refused, never handed out with a body missing.
//! - The mesh of each solid is watertight and measured inside its chord
//!   tolerance, or refused by name. A model over the facet budget is meshed
//!   again at a coarser chord, and that chord is reported (#864, #1054).
//! - A time bound (a watchdog thread) cancels a read that runs too long.

use super::{ImportError, MAX_FACETS};
use crate::geometry::Mesh;
use crate::units::{MM_PER_INCH, Unit};

/// The coarsest chord tolerance a STEP mesh is made at, in millimetres:
/// half the program sim's verdict tolerance, so the facets' sagitta on a
/// curved wall stays inside what the gouge verdict already forgives
/// (#689). The finest chord the importer promises: a model meshed coarser
/// than this is "reduced fidelity" (#864).
pub const CHORD_CAP: f64 = crate::stock_sim::DEV_TOL * 0.5;

/// The chord error of a kept mesh when it is coarser than the importer
/// promises, else `None` (#864). A nanometre of slack for round-off.
#[must_use]
pub fn coarse(chord_mm: f64) -> Option<f64> {
    (chord_mm.is_finite() && chord_mm > CHORD_CAP + 1e-9).then_some(chord_mm)
}

thread_local! {
    /// Test-only override of the facet budget (#864's pins force a small
    /// STEP fixture over budget instead of committing a huge model).
    static BUDGET_OVERRIDE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Run `f` with the STEP facet budget lowered to `budget` on this thread.
/// Never raises the budget an import asked for. For tests; the override
/// is cleared on return.
#[doc(hidden)]
pub fn with_facet_budget<R>(budget: usize, f: impl FnOnce() -> R) -> R {
    BUDGET_OVERRIDE.with(|b| b.set(Some(budget)));
    let out = f();
    BUDGET_OVERRIDE.with(|b| b.set(None));
    out
}

fn facet_budget(asked: usize) -> usize {
    BUDGET_OVERRIDE.with(std::cell::Cell::get).map_or(asked, |b| b.min(asked))
}

pub fn parse(data: &[u8]) -> Result<Mesh, ImportError> {
    parse_with_chord(data).map(|(mesh, _)| mesh)
}

/// [`parse_with_chord`] under the default budget, [`MAX_FACETS`].
pub fn parse_with_chord(data: &[u8]) -> Result<(Mesh, f64), ImportError> {
    parse_with_chord_budget(data, MAX_FACETS)
}

/// [`parse`], and the chord tolerance the kept mesh was made at, in
/// millimetres (#864): [`CHORD_CAP`], or coarser when the model did not
/// fit `budget` facets there (#1054) or its edges lie off their surfaces
/// past it (#2976). Never NaN, infinite or negative.
pub fn parse_with_chord_budget(data: &[u8], budget: usize) -> Result<(Mesh, f64), ImportError> {
    parse_noted(data, budget).map(|(mesh, chord, _)| (mesh, chord))
}

/// [`parse_with_chord_budget`], and what the reader and the mesher noted on
/// the way, one line each, for the import result.
pub fn parse_noted(data: &[u8], budget: usize) -> Result<(Mesh, f64, Vec<String>), ImportError> {
    match parse_open(data, budget)? {
        (mesh, chord, notes, None) => Ok((mesh, chord, notes)),
        (_, _, _, Some(hole)) => Err(super::open_refusal(&hole)),
    }
}

/// [`parse_noted`], with a solid that meshes with faces left out kept and
/// its hole named (#1610): every left-out face with the mesher's reason.
/// For the model import, which lands it incomplete; never a whole model.
pub fn parse_open(data: &[u8], budget: usize) -> Result<(Mesh, f64, Vec<String>, Option<String>), ImportError> {
    parse_with_cause(data, budget).map(|(mesh, chord, notes, open, _)| (mesh, chord, notes, open))
}

/// The complete import result, including the recorded reason for its chord.
pub type GradedStep = (Mesh, f64, Vec<String>, Option<String>, bool);

/// Parse the mesh together with its chord, notes, hole and recorded cause.
/// A file that states no length unit is refused (#1874).
pub fn parse_with_cause(data: &[u8], budget: usize) -> Result<GradedStep, ImportError> {
    parse_with_cause_in(data, budget, None)
}

/// [`parse_with_cause`], with the caller's length unit for a file that
/// states none (#1874). The length unit that the model was read in is the
/// last note: the file's own, or the caller's.
pub fn parse_with_cause_in(data: &[u8], budget: usize, units: Option<Unit>) -> Result<GradedStep, ImportError> {
    // The file's own syntax is checked before the kernel reads it, so the
    // refusals keep their wording: a nesting depth past the cap (#1075),
    // the bad entity by `#id` and line, a string or comment that never
    // closes, and a file with no DATA section (#912).
    super::step_depth::check(data)?;
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = std::str::from_utf8(data).map_err(|_| ImportError::Step("file is not valid UTF-8".into()))?;
    if let Some(found) = camcraft_geometry::step_diag::locate(text) {
        return Err(ImportError::Step(found.to_string()));
    }
    let read = camcraft_geometry::step::parse_open_assuming(data, facet_budget(budget), CHORD_CAP, units.map(mm_per)).map_err(|e| match e {
        camcraft_geometry::step::StepError::TooLarge => ImportError::TooLarge,
        camcraft_geometry::step::StepError::MissingUnits => ImportError::StepUnitsMissing,
        camcraft_geometry::step::StepError::Refused(reason) => ImportError::Step(reason),
    })?;
    let chord = read.chord_mm;
    if !(chord.is_finite() && chord > 0.0) {
        return Err(ImportError::Step(format!("tessellation reported a chord error of {chord} mm")));
    }
    if read.mesh.facets.is_empty() {
        return Err(ImportError::Empty);
    }
    let mut notes = read.notes;
    notes.push(unit_note(&read.length_units_mm, read.assumed_mm, units));
    // The geometry note of a model meshed coarser for its edge gaps says
    // the facts; what the checks then do is the fidelity law's (#2976).
    if read.loosened
        && let Some(first) = notes.first_mut()
    {
        first.push(' ');
        first.push_str(consequence(chord, read.file_tolerance));
    }
    Ok((read.mesh, chord, notes, read.open, read.file_tolerance))
}

/// Millimetres in one `unit`.
fn mm_per(unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => 1.0,
        Unit::Inches => MM_PER_INCH,
    }
}

/// The name of a length unit of `mm` millimetres: `mm`, `cm`, `m`, `in`
/// or `ft`, else the number of millimetres.
#[must_use]
pub fn unit_name(mm: f64) -> String {
    const NAMED: [(f64, &str); 5] = [(1.0, "mm"), (10.0, "cm"), (1000.0, "m"), (MM_PER_INCH, "in"), (304.8, "ft")];
    NAMED
        .iter()
        .find(|(k, _)| ((mm - k) / k).abs() <= 1e-9)
        .map_or_else(|| format!("{mm} mm"), |(_, name)| (*name).to_owned())
}

/// The import note that states the length unit the model was read in
/// (#1874): the file's own, or the caller's for a file that states none.
/// A caller's unit that the file overrules is named, never dropped.
#[must_use]
pub fn unit_note(stated_mm: &[f64], assumed_mm: Option<f64>, asked: Option<Unit>) -> String {
    let names: Vec<String> = stated_mm.iter().map(|&mm| unit_name(mm)).collect();
    let list = names.join(", ");
    let plural = if names.len() > 1 { "s" } else { "" };
    match (assumed_mm, asked) {
        (Some(_), _) => format!("length unit: {list} (stated at import; the file states no length unit)"),
        (None, Some(asked)) => format!(
            "length unit{plural}: {list} (stated by the file; the {} stated at import is not used)",
            asked.as_str()
        ),
        (None, None) => format!("length unit{plural}: {list} (stated by the file)"),
    }
}

/// What the fidelity law (#864) does with a model kept at `chord_mm`, in
/// plain words, for the note of a model meshed coarser for its edge gaps
/// (#2976): read from the same law the checks use
/// (`model_fidelity::verdict_tol`), never a second rule.
#[must_use]
pub fn loosened_consequence(chord_mm: f64) -> &'static str {
    consequence(chord_mm, true)
}

fn consequence(chord_mm: f64, file_tolerance: bool) -> &'static str {
    let excess = crate::model_fidelity::excess(chord_mm);
    if crate::fidelity_note::widened(chord_mm, file_tolerance) {
        // #2996: judged at its own accuracy, and the program says so.
        "Gouge checks against this model use a wider true-surface bound than the normal 0.010 mm. \
         Each posted program states this bound and names this model in its header. \
         An operation that they cannot check is refused at the post."
    } else if crate::verify::excess_floor_for(crate::stock_sim::DEV_TOL, excess, file_tolerance).is_none() {
        "A program cannot be posted while this model is in the document. \
         Export the file again with a tighter tolerance to post one."
    } else {
        "Gouge checks against this model are stricter, and an operation that they cannot check is refused at the post."
    }
}

//! STEP reader: B-rep shells tessellated to triangles with the `truck`
//! geometry crates (ricosjp, Apache-2.0), used as a library dependency.
//!
//! Provenance: file syntax per ISO 10303-21; surface tessellation is truck's
//! own published algorithm - nothing here from any CAM vendor.
//!
//! Hardening notes (Prime Directive):
//! - `truck` panics on shells that are not oriented/closed. Every truck call
//!   on the parse path runs inside `catch_unwind`; a kernel panic becomes a
//!   clean `ImportError::Step`, logged, never propagated.
//! - Tessellation is ADAPTIVE: each shell is meshed coarse first, then refined
//!   toward a quality target only while the projected facet count stays inside
//!   a memory budget. A model that cannot tessellate at target quality still
//!   imports - at reduced fidelity, logged - instead of spiking memory and
//!   being thrown away.
//! - Facet collection is capped per shell while extracting, as a last line of
//!   defense on top of the tolerance schedule.
//! - Every face's surface evaluations are metered per pass (`step_guard`,
//!   #1536): a face whose subdivision never converges is refused by name
//!   instead of hanging inside truck, where nothing else can stop it.
//! - A boundary point on a collapsed patch side (the pole of a corner
//!   blend) keeps its parameters inside the knot domain (`step_pole`,
//!   #1548), and an edge on a `SURFACE_CURVE` is read from its 3D curve
//!   (`step_edge3d`, #1548): a rounded-edge solid then meshes closed
//!   instead of being refused or losing faces.
//! - A shell the file declares closed must mesh closed, and a face the
//!   kernel returns no mesh for is refused by name (`step_closed`, #1573):
//!   truck drops or mis-trims such faces without an error.

use std::panic::AssertUnwindSafe;

use truck_meshalgo::prelude::*;
use truck_stepio::r#in::Table;

use super::step_refine::PassError;
use super::{ImportError, MAX_FACETS};
use crate::geometry::{Mesh, Point3, Triangle};

/// Lower bound for tessellation tolerance, in the FILE's length unit
/// (the unit truck meshes in): ten times truck's own `TOLERANCE` (1e-6,
/// also in model units), which it welds vertices at. Meshing within a
/// few of those split the metre-unit AP214 sample into a non-manifold
/// body (at 5e-6 m, #689); 1e-5 keeps it whole. For a millimetre or inch
/// file it never binds; a metre file stops at 1e-5 m = 0.01 mm, short of
/// the 0.005 mm cap, and the import logs it as reduced fidelity.
const MIN_TOL: f64 = 1e-5;
/// The coarsest chord tolerance a STEP mesh is made at, in MILLIMETRES:
/// half the program sim's verdict tolerance, so the facets' sagitta on a
/// curved wall stays inside what the gouge verdict already forgives
/// (#689). The mesh is made in the file's unit and scaled to mm after,
/// so `shell_facets` divides this by the file's mm-per-unit.
const MAX_CHORD: f64 = crate::stock_sim::DEV_TOL * 0.5;

/// The finest chord tolerance the STEP importer promises (mm); a model
/// tessellated coarser than this is "reduced fidelity" (#864).
pub const CHORD_CAP: f64 = MAX_CHORD;

/// The chord error of a kept mesh when it is coarser than the importer
/// promises, else `None` (#864). A nanometre of slack: an inch or metre
/// file's cap is divided into the file's unit and multiplied back, and
/// that round trip must not flag a mesh that met the promise.
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

thread_local! {
    /// Test-only override of the per-face evaluation cap (#1536's pins
    /// trip it on a small fixture in well under a second).
    static EVAL_CAP_OVERRIDE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Run `f` with the per-face evaluation cap lowered to `cap` on this
/// thread. Never raises it. For tests; cleared on return.
#[doc(hidden)]
pub fn with_face_eval_cap<R>(cap: u64, f: impl FnOnce() -> R) -> R {
    EVAL_CAP_OVERRIDE.with(|c| c.set(Some(cap)));
    let out = f();
    EVAL_CAP_OVERRIDE.with(|c| c.set(None));
    out
}

fn face_eval_cap() -> u64 {
    let cap = super::step_guard::FACE_EVAL_CAP;
    EVAL_CAP_OVERRIDE.with(std::cell::Cell::get).map_or(cap, |c| c.min(cap))
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

/// [`parse`], and the chord error the kept mesh was actually made at, in
/// millimetres: the coarsest kept tolerance over all shells, scaled by the
/// file's length unit (#864). Never NaN, infinite or negative - such a
/// value is refused as a corrupt import, not carried into a verdict.
/// `budget` caps the whole model's facets (#1054: the caller checked it
/// against `step_budget`'s bounds).
pub fn parse_with_chord_budget(data: &[u8], budget: usize) -> Result<(Mesh, f64), ImportError> {
    parse_noted(data, budget).map(|(mesh, chord, _)| (mesh, chord))
}

/// [`parse_with_chord_budget`], and what the importer changed on the way,
/// one line each, for the import result (#1573 review: a face flipped to
/// agree with its neighbours is said by name, not only logged).
/// A closed shell that meshes open is refused by name here.
pub fn parse_noted(data: &[u8], budget: usize) -> Result<(Mesh, f64, Vec<String>), ImportError> {
    match parse_open(data, budget)? {
        (_, _, _, Some(hole)) => Err(super::step_closed::open_refusal(&hole)),
        (mesh, chord, notes, None) => Ok((mesh, chord, notes)),
    }
}

/// [`parse_noted`], with a closed shell that meshes open kept and its hole
/// named instead of refused (#1610; one line per open shell, joined), for
/// a caller that lands the model incomplete.
pub fn parse_open(data: &[u8], budget: usize) -> Result<(Mesh, f64, Vec<String>, Option<String>), ImportError> {
    super::step_depth::check(data)?; // #1075: refuse before ruststep can overflow
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Step("file is not valid UTF-8".into()))?;
    let exchange = ruststep::parser::parse(text).map_err(|e| {
        // #912: ruststep fails the whole DATA section at once and its
        // position is not the bad entity; re-scan to name it.
        ImportError::Step(match super::step_diag::locate(text) {
            Some(found) => found.to_string(),
            None => format!("syntax error: {e}"),
        })
    })?;
    // An exchange with zero DATA sections parses fine but would index out of
    // bounds inside truck's convenience helpers; guard the only raw index.
    let Some(section) = exchange.data.first() else {
        return Err(ImportError::Step("file has no DATA section".into()));
    };
    // #306: the file's own length unit, read before any geometry - a
    // part saved in inches or metres must not import with its numbers
    // read as millimetres. No stated unit keeps millimetres.
    let mm_per_unit = super::step_units::mm_per_unit(section)
        .map_err(|e| ImportError::Step(format!("length unit: {e}")))?;
    let mut table = Table::from_data_section(section);
    // #1548: edges are read from their 3D curves, never rebuilt from a
    // pcurve in truck's own parametrisation of the neighbouring surface.
    let repointed = super::step_edge3d::prefer_3d(&mut table);
    if repointed > 0 {
        tracing::debug!("step: {repointed} surface-curve edges read from their 3D curve");
    }
    if table.shell.is_empty() {
        return Err(ImportError::Step("no shells found".into()));
    }

    // #1573: shells declared closed must mesh closed.
    let closed = super::step_closed::closed_shells(section);
    let max_facets = facet_budget(budget);
    let mut facets: Vec<Triangle> = Vec::new();
    let mut chord = 0.0f64;
    let mut notes = Vec::new();
    let mut holes: Vec<String> = Vec::new();
    // Shells in id order, not the table's hash order: the facets, the notes
    // and the first refusal are the same on every run (#1576 review).
    let mut shells: Vec<(&u64, &truck_stepio::r#in::ShellHolder)> = table.shell.iter().collect();
    shells.sort_unstable_by_key(|&(id, _)| *id);
    for (shell_id, shell) in shells {
        if facets.len() >= max_facets {
            return Err(ImportError::TooLarge);
        }
        let budget = max_facets - facets.len();
        // The whole per-shell kernel sequence runs inside one guarded call.
        // Everything it touches is shared-immutable or thread-local, so
        // AssertUnwindSafe holds for our purposes: on a panic we discard all
        // outputs and abort the import.
        let per_shell = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let closed = closed.contains(shell_id);
            shell_facets(&table, (*shell_id, shell), closed, budget, mm_per_unit.unwrap_or(1.0))
        }));
        let (tris, tol, judged) = match per_shell {
            Err(_) => {
                tracing::error!("step: truck kernel panicked on a shell; import aborted");
                return Err(ImportError::Step(
                    "geometry kernel failed on a shell (degenerate or unclosed B-rep)".into(),
                ));
            }
            Ok(Err(err)) => return Err(err),
            Ok(Ok(kept)) => kept,
        };
        chord = chord.max(tol);
        facets.extend(tris);
        notes.extend(judged.notes);
        holes.extend(judged.open);
    }
    if facets.is_empty() {
        return Err(ImportError::Empty);
    }
    // Scale after tessellating: the tolerance schedule is relative to
    // each shell's own extent, so the mesh's quality is the same in any
    // unit, and only the numbers change.
    if let Some(k) = mm_per_unit.filter(|k| *k != 1.0) {
        let mm = |p: Point3| Point3::new(p.x * k, p.y * k, p.z * k);
        for t in &mut facets {
            *t = Triangle::new(mm(t.a), mm(t.b), mm(t.c));
        }
        chord *= k.abs();
    }
    if !(chord.is_finite() && chord >= 0.0) {
        return Err(ImportError::Step(format!("tessellation reported a chord error of {chord} mm")));
    }
    let open = (!holes.is_empty()).then(|| holes.join("; "));
    Ok((Mesh::from_facets(facets), chord, notes, open))
}

/// Tessellate one STEP shell into triangles, staying inside `budget` facets.
/// The triangles are in the file's length unit; `mm_per_unit` (millimetres
/// in one of them) only converts the mm chord cap into that unit.
/// Runs inside `catch_unwind`; returns `Err` only for reasons we control.
fn shell_facets(
    table: &Table,
    (shell_id, shell): (u64, &truck_stepio::r#in::ShellHolder),
    closed: bool,
    budget: usize,
    mm_per_unit: f64,
) -> Result<(Vec<Triangle>, f64, super::step_closed::Judged), ImportError> {
    let cshell = table
        .to_compressed_shell(shell)
        .map_err(|e| ImportError::Step(format!("shell conversion failed: {e}")))?;
    // #1536: every face metered, so one whose subdivision never converges
    // is refused by name instead of hanging the import.
    let face_ids = super::step_guard::face_entities(shell, cshell.faces.len());
    let cap = face_eval_cap();
    let guarded = super::step_guard::Guarded::new(cshell, cap);
    let cshell = &guarded.shell;
    let mut extent = 0.0f64;
    for p in cshell.vertices.iter() {
        for c in [p.x, p.y, p.z] {
            if !c.is_finite() {
                return Err(ImportError::Step("shell vertices are not finite".into()));
            }
            extent = extent.max(c.abs());
        }
    }
    // One tessellation pass: mesh the whole shell at `tol`, extract
    // triangles, stop with `TooLarge` as soon as `budget` is crossed. A
    // closure so the heavily-aliased CompressedShell type is inferred
    // instead of spelled out.
    // The first face each pass got no polygon for, and each face's facet
    // range, by the pass's tol (#1573).
    let dropped: std::cell::RefCell<Vec<(f64, Option<usize>)>> = std::cell::RefCell::default();
    let owners: std::cell::RefCell<Vec<(f64, Vec<usize>)>> = std::cell::RefCell::default();
    let triangulate = |tol: f64| -> Result<Vec<Triangle>, PassError> {
        guarded.reset();
        // A panic is held until the meters are read: a face that ran past
        // its cap is the cause worth naming, not the panic it led to.
        let meshed = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let faces = cshell.robust_triangulation(tol);
            // #1573: truck's `to_polygon` skips a face it has no polygon
            // for; each face is kept apart, oriented as `to_polygon` would.
            let none = faces.faces.iter().position(|f| f.surface.is_none());
            dropped.borrow_mut().push((tol, none));
            let polys: Vec<Option<PolygonMesh>> = faces
                .faces
                .into_iter()
                .map(|f| {
                    f.surface.map(|mut s| {
                        if !f.orientation {
                            s.invert();
                        }
                        s
                    })
                })
                .collect();
            polys
        }));
        if let Some(i) = cshell.edges.iter().position(|e| e.curve.meter.refused()) {
            return Err(PassError::Unconverged {
                continuous: false,
                refusal: ImportError::Step(format!(
                    "edge {} of {} in shell #{shell_id} could not be tessellated: its curve subdivision did not converge within {cap} evaluations (a discontinuous or degenerate curve) - re-export the model, or import an STL",
                    i + 1, cshell.edges.len()
                )),
            });
        }
        if let Some(trip) = guarded.tripped() {
            // A face that may jump is refused even when a coarser pass
            // met: it converges at any tolerance wider than its gap, and
            // that mesh smears the gap into a wall that is not there. A
            // face proven continuous only ran out of evaluations, and
            // `refine` keeps the coarser mesh if there is one (#1568).
            let i = trip.index;
            let fault = guarded.fault(i);
            tracing::warn!(
                "step: face {} of shell #{shell_id} ({}, continuous: {}) refused at tol {tol:e}: {}",
                i + 1,
                trip.kind,
                trip.continuous,
                fault.map_or_else(|| format!("ran past {cap} evaluations"), |f| f.why().to_string())
            );
            let entity = face_ids.as_ref().and_then(|ids| ids.as_slice().get(i).copied());
            let faces = cshell.faces.len();
            // #1602: a boundary search that could not be answered soundly
            // is refused in its own words, not as a runaway division.
            let refusal = match fault {
                Some(f) => f.refusal(&super::step_guard::face_name(trip, faces, entity, Some(shell_id))),
                None => super::step_guard::refusal(trip, faces, entity, Some(shell_id), cap),
            };
            return Err(PassError::Unconverged { continuous: trip.continuous, refusal });
        }
        let polys = meshed.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        let (facets, ends) = super::step_facets::facets_of(&polys, budget)?;
        owners.borrow_mut().push((tol, ends));
        Ok(facets)
    };
    // Quality target: ~0.1% of the model extent, floored so tiny parts still
    // subdivide and truck's assert (tol >= 1e-7) holds - and never coarser
    // than half the program gate's verdict tolerance (#689). The sim judges
    // the program against these facets, and on a concave wall a chord
    // stands inside the true surface by its sagitta: a mesh coarser than
    // the verdict charged a bore at the hole's own size as a gouge the
    // wall's full height. Half the verdict leaves the other half for the
    // cut itself, so a nominal finish passes and 0.05 mm into the wall is
    // still a gouge. The facet budget below still bounds the mesh.
    //
    // Units: `extent`, `tol` and `tol_target` are in the FILE's length unit
    // (truck meshes there; `parse` scales to mm after). `extent * 1e-3` is
    // relative, so it needs no conversion; the mm cap does - an inch file
    // capped at a bare 0.005 got 0.005 in = 0.127 mm chords (#689).
    // `mm_per_unit` comes from step_units, which reads only positive
    // finite units; anything else falls back to millimetres.
    let k = if mm_per_unit.is_finite() && mm_per_unit > 0.0 { mm_per_unit } else { 1.0 };
    let max_chord = MAX_CHORD / k;
    // `wanted` is the target before truck's floor: a unit so large that
    // 0.005 mm is under MIN_TOL of it (the kilometre: 5e-9 km) can't be
    // meshed that fine, and the log below says so instead of passing.
    let wanted = (extent * 1e-3).min(max_chord);
    // min/max, not clamp: a NaN extent then lands on a finite target
    // instead of carrying NaN into truck's tolerance assert.
    #[allow(clippy::manual_clamp)]
    let tol_target = wanted.max(MIN_TOL);
    // Start 250x coarser than the target - the first pass then costs a few
    // dozen triangles per face no matter how big the model, so no shell can
    // silently allocate a monster mesh before we have measured it. The
    // schedule (coarse start, halving toward the target inside the budget)
    // lives in `step_refine`, which also keeps the tolerance of the mesh
    // it actually kept (#864).
    let mut kept = super::step_refine::refine(extent, tol_target, budget, triangulate)?;
    // #1573: the kept pass's dropped face, then the closed-shell law.
    let faces = cshell.faces.len();
    let dropped = dropped.borrow().iter().rev().find(|(t, _)| *t == kept.tol).and_then(|(_, i)| *i);
    let dropped = dropped.map(|i| match face_ids.as_ref().and_then(|ids| ids.get(i)) {
        Some(id) => format!("face #{id} (face {} of {faces})", i + 1),
        None => format!("face {} of {faces}", i + 1),
    });
    let ends = owners.borrow_mut().iter_mut().rev().find(|(t, _)| *t == kept.tol).map(|(_, e)| std::mem::take(e));
    let names = |i: usize| face_ids.as_ref().and_then(|ids| ids.get(i));
    let seam = super::step_closed::Shell { id: shell_id, faces, closed, mm_per_unit: k };
    let judged = super::step_closed::judge(&mut kept.facets, ends.as_deref(), kept.tol, &seam, names, dropped)?;
    if kept.tol > wanted {
        // The cause, so the advice fits it: a larger facet budget does
        // nothing for a face that ran out of evaluations (#1568 review).
        let why = if kept.eval_capped {
            "a finer pass ran past a face's evaluation cap (a larger facet budget will not help)"
        } else {
            "bounded by memory or by the kernel's least tolerance"
        };
        tracing::warn!(
            "step: tessellated at reduced fidelity (tol {:e} mm, target {:e} mm, {} facets) - {why}",
            kept.tol * k,
            wanted * k,
            kept.facets.len()
        );
    }
    Ok((kept.facets, kept.tol, judged))
}

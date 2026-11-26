//! Reduced-fidelity models (#864): a STEP model whose kept mesh is coarser
//! than the importer promises - the flag, what the gouge verdicts do about
//! it, and the program gate's refusal once they cannot be honest.
//!
//! Provenance: our own design (maintainer decision on #864). A facet of a
//! curved wall is a chord of the true surface and stands inside it by its
//! sagitta (elementary geometry), so against the mesh a convex wall looks
//! farther from the tool than the real one is. No vendor material consulted.
//!
//! ## The law here
//!
//! - A model carries [`ReducedFidelity`] exactly when its import kept a
//!   chord error above `step::CHORD_CAP`. Its EXCESS is that error less
//!   the cap: how much more a verdict against the mesh can forgive than
//!   the same verdict against a model that met the promise.
//! - While a verdict tolerance less the excess stays positive, the gouge
//!   laws judge with that smaller tolerance (`VerifyContext::model_excess`
//!   at generation, [`verdict_tol`] for the program sim): stricter, never
//!   looser. False gouges on a concave wall are the safe side.
//! - #2996: only a recorded file-tolerance cause can widen a chord past
//!   `DEV_TOL`, up to `verify::WIDE_CHORD_LIMIT`. The mesh allowance is
//!   0.005 mm; the header and model page state the true-surface bound:
//!   chord plus mesh allowance (`fidelity_note`). Budget and absent causes
//!   keep the original strict law. Collision bands
//!   still grow by the whole excess (`verify::band_growth`).
//! - Once the excess uses a whole tolerance, a tightened law would convict
//!   every exactly finished flat face - a misleading verdict. The laws
//!   then read untightened and [`gate_problems`] refuses the operation by
//!   name at the post; the program sim does not judge at all.
//! - A kind whose own laws do not read the excess yet
//!   (`operation::fidelity_law::excess_aware`) is refused on any reduced
//!   model: a refusal is the only verdict that cannot be looser. #904
//!   took turning, the rotary kinds and the multi-axis ball kinds off
//!   that list; `fidelity_law`'s header says which law judges which.
//! - A stock copied from a model carries the model's flag itself (#1056):
//!   it holds the same facets, so deleting or renaming the model never
//!   makes the laws stop tightening on them. The copy is written by
//!   `SetStock`, `UpdateModel`'s propagation, the setup clones and the
//!   loader, and saved with the project. A stock from a file that predates
//!   its own flag still reads its source's, and adopts it on load.
//! - When the gate refuses an operation here, the program sim and the
//!   eye's colours read no verdict either ([`program_tol`]): the named
//!   refusal is the one line said, never a gouge read off the same mesh.
//! - A mesh whose fidelity is NOT KNOWN (#1056: every mesh of a project
//!   saved before v59 without a recorded chord, `project::fidelity_migrate`)
//!   carries the sentinel chord [`UNKNOWN_MM`]: no tolerance survives it,
//!   so every operation judged against it, and the program verdict, are
//!   refused by name ([`UNKNOWN_NOTE`]), never tightened by a guess. It
//!   rides every path the flag rides (copies, clones, undo, save) and only
//!   a fresh import, or a person's vouch (#1339, `model_vouch`), clears it.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::import::{step, step_budget};
use crate::model;
use crate::schema::{ReducedFidelity, StepBudget};
use crate::stock::StockShape;

/// What every refusal here says, in the user's words. Two causes make a
/// model coarse: the facet budget (#1054) and edges that lie off their
/// surfaces (#2976). Missing cause records keep the original strict law;
/// this general refusal advice names both causes.
pub const TOO_COARSE: &str = "imported too coarsely to verify; re-import with a larger STEP facet budget, or export the file again with a tighter tolerance";

/// The chord a mesh of unknown fidelity carries (#1056), mm: far past any
/// verdict tolerance, so every law refuses rather than tightens, and small
/// enough that no arithmetic on it overflows to a non-finite "no excess".
/// Never shown as a number: [`is_unknown`] picks the words instead.
pub const UNKNOWN_MM: f64 = 1.0e12;

/// What a refusal or note says of a mesh of unknown fidelity.
pub const UNKNOWN_NOTE: &str = "saved before CamCraft recorded mesh fidelity (project v58 or older), so a gouge check against it cannot be trusted; re-import the model to verify its operations";

/// Whether a stored chord is the unknown-fidelity sentinel.
#[must_use]
pub fn is_unknown(chord_mm: f64) -> bool {
    chord_mm >= UNKNOWN_MM
}

/// The flag a loaded entity lands with: its recorded chord, or the
/// sentinel when the file marks its fidelity unknown.
#[must_use]
pub fn loaded(coarse_mm: Option<f64>, unknown: bool) -> Option<f64> {
    if unknown { Some(UNKNOWN_MM) } else { coarse_mm }
}

/// The chord error a reduced-fidelity model was imported at, mm; `None`
/// for a model at full fidelity. A stored value that is not a coarse
/// chord (a hand-edited file) reads as none.
#[must_use]
pub fn chord_of(world: &World, entity: &Entity) -> Option<f64> {
    entity.entity_view(world).try_get::<&ReducedFidelity>(|r| r.chord_mm).and_then(step::coarse)
}

/// Write or clear the flag. For the import/update commands, the vouch
/// (#1339), their undo and the project loader only - it describes the
/// facets it lands with.
pub fn set(world: &World, entity: &Entity, coarse_mm: Option<f64>) {
    set_with_cause(world, entity, coarse_mm, false);
}

/// The recorded cause, traded with the mesh. Missing records keep the old law.
#[must_use]
pub fn file_tolerance_of(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&ReducedFidelity>(|r| r.file_tolerance).unwrap_or(false)
}

/// Write the complete fidelity record; absent or unknown chords cannot widen.
pub fn set_with_cause(world: &World, entity: &Entity, coarse_mm: Option<f64>, file_tolerance: bool) {
    let view = entity.entity_view(world);
    match coarse_mm.and_then(step::coarse) {
        Some(chord_mm) => {
            view.set(ReducedFidelity { chord_mm, file_tolerance: file_tolerance && !is_unknown(chord_mm) });
        }
        None => {
            view.remove(world.component::<ReducedFidelity>());
        }
    }
}

/// Fidelity is part of a cached verification's input, including on unchanged facets.
pub(crate) fn write_stamp(world: &World, entity: &Entity, h: &mut impl std::hash::Hasher) {
    // Unknown fidelity is refused by name before any verdict is read.
    if let Some(chord) = chord_of(world, entity).filter(|c| !is_unknown(*c)) {
        h.write(b"coarse-fidelity-v1");
        h.write_u64(chord.to_bits());
        h.write_u8(u8::from(file_tolerance_of(world, entity)));
    }
}

/// The STEP facet budget the model was imported under when it is not the
/// default (#1054); `None` reads as the default. A stored value outside
/// the bounds (a hand-edited file) reads as the default too.
#[must_use]
pub fn budget_of(world: &World, entity: &Entity) -> Option<u64> {
    entity
        .entity_view(world)
        .try_get::<&StepBudget>(|b| b.facets)
        .filter(|b| step_budget::check(*b).is_ok())
        .and_then(|b| step_budget::recorded(Some(b)))
}

/// The budget a re-import of `path` carries when the caller gave none
/// (#1185): for a STEP file landing on a model that already exists under
/// `name` (default: the file's stem, as the importers name it), the budget
/// that model was imported under, so a restart or a fresh session never
/// silently re-meshes it at the default. `None` is the default budget.
#[must_use]
pub fn reimport_budget(world: &World, path: &std::path::Path, name: Option<&str>) -> Option<u64> {
    if !matches!(crate::geometry::import::format_from_path(path), Ok(crate::geometry::import::Format::Step)) {
        return None;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let entity = model::find_by_name(world, name.unwrap_or(stem))?;
    budget_of(world, &entity)
}

/// Write or clear the recorded budget. Like [`set`]: for the import and
/// update commands, their undo and the project loader only.
pub fn set_budget(world: &World, entity: &Entity, budget: Option<u64>) {
    let view = entity.entity_view(world);
    match step_budget::recorded(budget) {
        Some(facets) => {
            view.set(StepBudget { facets });
        }
        None => {
            view.remove(world.component::<StepBudget>());
        }
    }
}

/// The reduced-fidelity model behind the entity `name`: itself (a model,
/// or a stock copy carrying its source's flag, #1056), or - for a stock
/// copy from a file that predates that - the model it copies.
fn coarse_source(world: &World, name: &str) -> Option<Coarse> {
    let entity = model::find_by_name(world, name)?;
    let copied = match model::stock_def_of(world, &entity) {
        Some(StockShape::FromModel { source }) => Some(source),
        _ => None,
    };
    // A refusal names the model the facets came from, also once it is
    // gone: that is what a person re-imports. The hole is read off the
    // entity that carries the sentinel (#1700), falling back to the
    // source once when a v0.21-v0.24 file marked the copy without one.
    if let Some(c) = chord_of(world, &entity) {
        let hole = crate::mesh_open::hole_of(world, &entity).or_else(|| {
            is_unknown(c)
                .then(|| copied.as_deref().and_then(|s| crate::mesh_open::hole_behind(world, s)))
                .flatten()
        });
        return Some((copied.unwrap_or_else(|| name.to_string()), c, hole, file_tolerance_of(world, &entity)));
    }
    let source = copied?;
    let src = model::find_by_name(world, &source)?;
    chord_of(world, &src).map(|c| (source, c, crate::mesh_open::hole_of(world, &src), file_tolerance_of(world, &src)))
}

/// The model a mesh's facets came from, its chord, and its hole when it
/// was imported incomplete (#1610).
type Coarse = (String, f64, Option<String>, bool);

/// The chord error a stock copied from `source` lands with (#1056): the
/// source's own, or what a stock it is itself copied from carries.
#[must_use]
pub fn chord_behind(world: &World, source: &str) -> Option<f64> {
    coarse_source(world, source).map(|(_, c, _, _)| c)
}

/// The cause behind a named model or legacy stock copy.
#[must_use]
pub fn file_tolerance_behind(world: &World, name: &str) -> bool {
    let Some(e) = model::find_by_name(world, name) else { return false; };
    if chord_of(world, &e).is_some() { return file_tolerance_of(world, &e); }
    match model::stock_def_of(world, &e) {
        Some(StockShape::FromModel { source }) => model::find_by_name(world, &source)
            .is_some_and(|src| file_tolerance_of(world, &src)),
        _ => false,
    }
}

/// The loader's last pass (#1056): a `FromModel` stock from a file that
/// predates the copy's own flag takes its source's, so deleting that
/// source later keeps the stock judged as coarse as its facets are.
/// #1700: a copy marked `fidelity_unknown` without its own hole (written
/// by v0.21.0-v0.24.0 before the copy carried `mesh_open`) takes its
/// source's hole too, so a later delete keeps naming it and a re-save
/// repairs the file.
pub fn adopt_copies(world: &World) {
    for (_, e) in model::named_entities(world) {
        let Some(StockShape::FromModel { source }) = model::stock_def_of(world, &e) else {
            continue;
        };
        let Some(src) = model::find_by_name(world, &source) else {
            continue;
        };
        if chord_of(world, &e).is_none()
            && let Some(c) = chord_of(world, &src)
        {
            set_with_cause(world, &e, Some(c), file_tolerance_of(world, &src));
        }
        if chord_of(world, &e).is_some_and(is_unknown)
            && crate::mesh_open::hole_of(world, &e).is_none()
            && let Some(hole) = crate::mesh_open::hole_of(world, &src)
        {
            crate::mesh_open::set(world, &e, Some(&hole));
        }
    }
}

/// How much of a verdict tolerance the named entity's mesh has used up,
/// mm: its chord error less the cap, 0 at full fidelity.
#[must_use]
pub fn excess_of(world: &World, name: &str) -> f64 {
    coarse_source(world, name).map_or(0.0, |(_, c, _, _)| excess(c))
}

/// Whether the excess of `setup`'s stock is known (#1091 reviews): a box
/// or cylinder has no mesh to be coarse; a from-model copy's chord is read
/// through its source (`coarse_source`, by NAME), so it is known only
/// while the model of that name is provably the one the copy was taken
/// from - its local facets bit for bit the stock's own ([`same_mesh`]).
/// A gone source, a different model imported under the name, a source
/// re-meshed without its copy, a stock with no facets: not known. A model
/// with no `ReducedFidelity` record is known exact - the importer writes
/// the record for every coarse import, and a re-import in place
/// (`UpdateModel`) trades copy and record together. Placement is not
/// compared: the chord is a property of the mesh, and a moved source
/// carries the same mesh and record. No stock entity: nothing to know.
/// #1056: once a from-model stock carries its own chord (#1127), read it
/// here instead of proving the source.
#[must_use]
pub fn stock_fidelity_known(world: &World, setup: Option<&str>) -> bool {
    let Some(stock) = model::find_by_name(world, &crate::setup::stock_entity_name(world, setup)) else {
        return true;
    };
    match model::stock_def_of(world, &stock) {
        // #1056: the chord lives on the source only - prove it is the source.
        Some(StockShape::FromModel { source }) => model::find_by_name(world, &source).is_some_and(|src| {
            match (model::facets_of(world, &stock), model::facets_of(world, &src)) {
                (Some(copy), Some(now)) => same_mesh(&copy, &now),
                _ => false,
            }
        }),
        _ => true,
    }
}

/// The two facet lists are one mesh: the same `Arc` (a copy `SetStock`
/// took and nothing replaced), or the same count and every coordinate
/// equal bit for bit. Empty is never a mesh.
fn same_mesh(a: &std::sync::Arc<[crate::geometry::Triangle]>, b: &std::sync::Arc<[crate::geometry::Triangle]>) -> bool {
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    if std::sync::Arc::ptr_eq(a, b) {
        return true;
    }
    let bits = |t: &crate::geometry::Triangle| [t.a, t.b, t.c].map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]);
    a.iter().zip(b.iter()).all(|(x, y)| bits(x) == bits(y))
}

/// A coarse chord error's excess over the importer's promise.
#[must_use]
pub fn excess(chord_mm: f64) -> f64 {
    (chord_mm - step::CHORD_CAP).max(0.0)
}


mod law;
mod report;
pub use law::{gate_problems, op_problem, program_tol, verdict_tol};
pub use report::{facts, facts_with_cause, summary, summary_with_cause};

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
//!   a fresh import clears it.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::import::{step, step_budget};
use crate::model;
use crate::operation::{fidelity_law, SrcRef};
use crate::schema::{Operation, ReducedFidelity, StepBudget};
use crate::stock::StockShape;
use crate::stock_sim::DEV_TOL;
use crate::verify::excess_floor;

/// What every refusal here says, in the user's words.
pub const TOO_COARSE: &str = "imported too coarsely to verify; re-import with a larger STEP facet budget";

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

/// Write or clear the flag. For the import/update commands, their undo
/// and the project loader only - it describes the facets it lands with.
pub fn set(world: &World, entity: &Entity, coarse_mm: Option<f64>) {
    let view = entity.entity_view(world);
    match coarse_mm.and_then(step::coarse) {
        Some(chord_mm) => {
            view.set(ReducedFidelity { chord_mm });
        }
        None => {
            view.remove(world.component::<ReducedFidelity>());
        }
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
fn coarse_source(world: &World, name: &str) -> Option<(String, f64)> {
    let entity = model::find_by_name(world, name)?;
    let copied = match model::stock_def_of(world, &entity) {
        Some(StockShape::FromModel { source }) => Some(source),
        _ => None,
    };
    // A refusal names the model the facets came from, also once it is
    // gone: that is what a person re-imports.
    if let Some(c) = chord_of(world, &entity) {
        return Some((copied.unwrap_or_else(|| name.to_string()), c));
    }
    let source = copied?;
    let src = model::find_by_name(world, &source)?;
    chord_of(world, &src).map(|c| (source, c))
}

/// The chord error a stock copied from `source` lands with (#1056): the
/// source's own, or what a stock it is itself copied from carries.
#[must_use]
pub fn chord_behind(world: &World, source: &str) -> Option<f64> {
    coarse_source(world, source).map(|(_, c)| c)
}

/// The loader's last pass (#1056): a `FromModel` stock from a file that
/// predates the copy's own flag takes its source's, so deleting that
/// source later keeps the stock judged as coarse as its facets are.
pub fn adopt_copies(world: &World) {
    for (_, e) in model::named_entities(world) {
        if chord_of(world, &e).is_some() {
            continue;
        }
        if let Some(StockShape::FromModel { source }) = model::stock_def_of(world, &e)
            && let Some(src) = model::find_by_name(world, &source)
            && let Some(c) = chord_of(world, &src)
        {
            set(world, &e, Some(c));
        }
    }
}

/// How much of a verdict tolerance the named entity's mesh has used up,
/// mm: its chord error less the cap, 0 at full fidelity.
#[must_use]
pub fn excess_of(world: &World, name: &str) -> f64 {
    coarse_source(world, name).map_or(0.0, |(_, c)| excess(c))
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

/// The plain-words line the model page and the import report show.
#[must_use]
pub fn summary(world: &World, chord_mm: f64) -> String {
    if is_unknown(chord_mm) {
        return format!("Fidelity unknown: {UNKNOWN_NOTE}");
    }
    format!("Reduced fidelity: surfaces are approximated to within {}", within(world, chord_mm))
}

/// The same facts for MCP and the script API: the achieved error in mm,
/// the importer's promise, and the sentence a person would be shown.
#[must_use]
pub fn facts(world: &World, chord_mm: f64) -> serde_json::Value {
    if is_unknown(chord_mm) {
        return serde_json::json!({
            "fidelity_unknown": true,
            "promised_mm": step::CHORD_CAP,
            "note": format!("Fidelity unknown: this model was {UNKNOWN_NOTE}. Its operations are refused at the post until then."),
        });
    }
    serde_json::json!({
        "approximation_mm": chord_mm,
        "promised_mm": step::CHORD_CAP,
        "note": format!(
            "{}. Gouge checks against this model are stricter; if it is too coarse to check, its operations are refused at the post. Re-import with a larger STEP facet budget to clear this.",
            summary(world, chord_mm)
        ),
    })
}

/// "0.012 mm" in the document's unit.
fn within(world: &World, chord_mm: f64) -> String {
    let u = crate::units::of(world);
    format!("{} {}", crate::units::len(chord_mm, u, 3), u.suffix())
}

/// The coarsest model an operation is judged against, by name: the
/// entities its source names (one entity, or every entity of a
/// selection) and the setup's stock - the collision laws read clearance
/// against it (#904; `operation::material::judged_material` is the same
/// pair at generation).
fn op_coarse(world: &World, op: &Operation) -> Option<(String, f64)> {
    let mut names = vec![crate::setup::stock_entity_name(world, op.setup.as_deref())];
    match &op.src {
        SrcRef::Stock => {}
        SrcRef::Entity(n) => names.push(n.clone()),
        SrcRef::Selection(items) => names.extend(items.iter().map(|g| g.entity.clone())),
    }
    names
        .iter()
        .filter_map(|n| coarse_source(world, n))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Why `op` cannot be verified against the model it is judged on, or
/// `None` when it can (full fidelity, or tightened verdicts).
#[must_use]
pub fn op_problem(world: &World, name: &str, op: &Operation) -> Option<String> {
    let (model, chord) = op_coarse(world, op)?;
    if is_unknown(chord) {
        return Some(format!("{name}: model '{model}' was {}", crate::mesh_open::unknown_note(world, &model)));
    }
    let said = format!("surfaces are approximated to within {}", within(world, chord));
    if !fidelity_law::excess_aware(&op.params) {
        return Some(format!(
            "{name}: model '{model}' was imported at reduced fidelity ({said}) and this kind of operation cannot be verified against it; re-import with a larger STEP facet budget"
        ));
    }
    excess_floor(fidelity_law::floor_of(&op.params), excess(chord))
        .is_none()
        .then(|| format!("{name}: model '{model}' was {TOO_COARSE} ({said})"))
}

/// The program gate's clause: one line per operation of the program that
/// cannot be verified against its model, else one for the program when a
/// part is too coarse for the program verdict. A program the post would
/// refuse anyway (a stale op) is that refusal's to name.
#[must_use]
pub fn gate_problems(world: &World) -> Vec<String> {
    let Ok(ops) = crate::posting::program_ops(world) else {
        return Vec::new();
    };
    let mut out: Vec<String> = ops
        .iter()
        .filter_map(|(name, e)| op_problem(world, name, &model::operation_of(world, e)?))
        .collect();
    // A part no operation names is still what the program verdict is read
    // against: when it is too coarse for that verdict, the program is
    // refused rather than posted without it.
    if out.is_empty()
        && let Err(why) = verdict_tol(world)
    {
        out.push(why);
    }
    // #1056: a fixture of unknown fidelity has no honest collision box (a
    // coarse one is grown by its chord, `fixture::steel_box`).
    out.extend(
        model::model_entities(world)
            .into_iter()
            .filter(|(_, e)| model::is_fixture(world, e) && chord_of(world, e).is_some_and(is_unknown))
            .map(|(n, _)| format!("fixture '{n}' was {}", crate::mesh_open::unknown_note(world, &n))),
    );
    out
}

/// The tolerance the program sim and the eye's colours judge at (#1056):
/// `Err` with the gate's own refusal line when an operation of the
/// program is refused here - so the refusal is what is said, never a
/// gouge line read off a mesh the gate already called too coarse (an op
/// whose own floor is below the sim's, or one judged against a coarse
/// stock the parts do not show) - else [`verdict_tol`].
pub fn program_tol(world: &World) -> Result<f64, String> {
    if let Ok(ops) = crate::posting::program_ops(world) {
        for (name, e) in &ops {
            if let Some(why) = model::operation_of(world, e).and_then(|op| op_problem(world, name, &op)) {
                return Err(why);
            }
        }
    }
    verdict_tol(world)
}

/// The program sim's verdict tolerance for this document: `DEV_TOL`, less
/// the worst excess among the parts the verdict is read against. `Err`
/// names the model once nothing honest is left - the sim then judges
/// nothing rather than paint a misleading map.
pub fn verdict_tol(world: &World) -> Result<f64, String> {
    let worst = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(n, e)| chord_of(world, &e).map(|c| (n, c)))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    let Some((name, chord)) = worst else {
        return Ok(DEV_TOL);
    };
    if is_unknown(chord) {
        return Err(format!("model '{name}' was {}", crate::mesh_open::unknown_note(world, &name)));
    }
    excess_floor(DEV_TOL, excess(chord)).ok_or_else(|| {
        format!("model '{name}' was {TOO_COARSE} (surfaces are approximated to within {})", within(world, chord))
    })
}

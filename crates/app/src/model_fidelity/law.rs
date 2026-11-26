//! Fidelity refusal and program verdicts. Provenance: our own laws from maintainer decisions #864 and #2996.

use super::report::within;
use super::{Coarse, TOO_COARSE, chord_of, coarse_source, excess, file_tolerance_of, is_unknown};
use crate::model;
use crate::operation::{SrcRef, fidelity_law};
use crate::schema::Operation;
use crate::stock_sim::DEV_TOL;
use crate::verify::excess_floor_for;
use flecs_ecs::prelude::World;

/// The coarsest model an operation is judged against, by name: the
/// entities its source names (one entity, or every entity of a
/// selection) and the setup's stock - the collision laws read clearance
/// against it (#904; `operation::material::judged_material` is the same
/// pair at generation).
fn op_coarse(world: &World, op: &Operation) -> Option<Coarse> {
    let mut names = vec![crate::setup::stock_entity_name(world, op.setup.as_deref())];
    match &op.src {
        SrcRef::Stock => {}
        SrcRef::Entity(n) => names.push(n.clone()),
        SrcRef::Selection(items) => names.extend(items.iter().map(|g| g.entity.clone())),
    }
    let sources: Vec<_> = names
        .iter()
        .filter_map(|n| coarse_source(world, n))
        .collect();
    let file_tolerance = sources.iter().all(|s| s.3);
    sources
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|mut s| {
            s.3 = file_tolerance;
            s
        })
}

/// Why `op` cannot be verified against the model it is judged on, or
/// `None` when it can (full fidelity, or tightened verdicts).
#[must_use]
pub fn op_problem(world: &World, name: &str, op: &Operation) -> Option<String> {
    let (model, chord, hole, file_tolerance) = op_coarse(world, op)?;
    if is_unknown(chord) {
        return Some(format!(
            "{name}: model '{model}' was {}",
            crate::mesh_open::note_for(hole.as_deref())
        ));
    }
    let said = format!(
        "surfaces are approximated to within {}",
        within(world, chord)
    );
    if !fidelity_law::excess_aware(&op.params) {
        return Some(format!(
            "{name}: model '{model}' was imported at reduced fidelity ({said}) and this kind of operation cannot be verified against it; re-import with a larger STEP facet budget, or export the file again with a tighter tolerance"
        ));
    }
    // #2996: a kind whose laws also guard a holder, bar, shank or stylus
    // reads the strict law (`fidelity_law::widens`).
    excess_floor_for(
        fidelity_law::floor_of(&op.params),
        excess(chord),
        file_tolerance && fidelity_law::widens(&op.params),
    )
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
            .filter(|(_, e)| {
                model::is_fixture(world, e) && chord_of(world, e).is_some_and(is_unknown)
            })
            .map(|(n, _)| {
                format!(
                    "fixture '{n}' was {}",
                    crate::mesh_open::unknown_note(world, &n)
                )
            }),
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
            if let Some(why) =
                model::operation_of(world, e).and_then(|op| op_problem(world, name, &op))
            {
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
    let sources: Vec<_> = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(n, e)| chord_of(world, &e).map(|c| (n, c, file_tolerance_of(world, &e))))
        .collect();
    let file_tolerance = sources.iter().all(|s| s.2);
    let Some((name, chord, _)) = sources.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
        return Ok(DEV_TOL);
    };
    if is_unknown(chord) {
        return Err(format!(
            "model '{name}' was {}",
            crate::mesh_open::unknown_note(world, &name)
        ));
    }
    excess_floor_for(DEV_TOL, excess(chord), file_tolerance).ok_or_else(|| {
        format!(
            "model '{name}' was {TOO_COARSE} (surfaces are approximated to within {})",
            within(world, chord)
        )
    })
}

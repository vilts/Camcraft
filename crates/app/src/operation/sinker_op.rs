//! The Sinker Burn operation's seed, condition read and generation
//! (#541): the op's stored numbers resolved against the world (heights,
//! the electrode's length, the setup machine's spark gap) and
//! `sinker::burn_to_depth` run over them.
//!
//! Provenance: our own glue over the motion crate's sinker generator,
//! whose module head cites its public sources (general die-sink EDM
//! practice); the condition table is #531's user data (#290 D4).

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geometry::Triangle;
use crate::machine::{ConditionField, Kind};
use crate::machining2d::GenError;
use crate::sinker::{SinkerDatum, SinkerParams};
use crate::sinker_undersize::{inverted, judge, SinkerPass, Undersize};
use crate::sinker_wear::burn_compensated;
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;

use super::kinds::Carried;
use super::sinker_pass::PassRead;
use super::types::{OpParams, SinkerBurnOp};
use super::ResolvedHeights;

/// The sinker draft: the carried heights (work top, and the model's
/// bottom as the floor datum), a 5 mm burn from the work top at the
/// origin, a 50 mm/min servo feed, and no condition named - the user
/// names one of the machine's own (#290 D4: none ship).
pub(super) fn seed_sinker_burn(c: &Carried) -> OpParams {
    OpParams::SinkerBurn(burn_draft(c))
}

/// The burn's draft numbers ([`seed_sinker_burn`]); the side burn
/// (#545) starts from the same plunge.
pub(super) fn burn_draft(c: &Carried) -> SinkerBurnOp {
    SinkerBurnOp {
        clearance_z: c.clearance,
        top_z: c.top,
        floor_z: c.bottom,
        datum: SinkerDatum::WorkTop,
        depth: 5.0,
        x: 0.0,
        y: 0.0,
        feed: 50.0,
        condition: String::new(),
        pass: super::SinkerPass::Single,
        cavity_x: 0.0,
        cavity_y: 0.0,
        orbit: None,
        layer: None,
        wear_lost: None,
        wear_over: None,
    }
}

/// The spark gap per side of the burn's condition, read from the op's
/// setup's machine (#531's door). Refused by name: a setup not on a
/// sinker EDM, no condition named, or a condition that is missing or
/// states no gap. Only the sinker's own field (the gap) is asked for -
/// never a wire field.
///
/// # Errors
/// Each refusal above, in words.
pub fn condition_gap(world: &World, setup: Option<&str>, op: &SinkerBurnOp) -> Result<f64, String> {
    condition_gap_as(world, setup, "Sinker Burn", op)
}

/// [`condition_gap`] for any sinker kind: `label` names the op kind in
/// the refusals (#545: the side burn reads the same door).
///
/// # Errors
/// [`condition_gap`]'s refusals.
pub fn condition_gap_as(world: &World, setup: Option<&str>, label: &str, op: &SinkerBurnOp) -> Result<f64, String> {
    let c = condition_as(world, setup, label, op, &[ConditionField::Gap])?;
    let machine = crate::setup_mount::machine_label(&crate::machine::of_setup(world, setup));
    c.gap.ok_or_else(|| format!("machine {machine}: EDM condition {:?} states no spark gap", op.condition.trim()))
}

/// The Sinker Burn's condition with every field in `need` stated
/// ([`condition_as`] under the Sinker Burn's name) - the door #541's gap
/// and #543's undersize share.
///
/// # Errors
/// [`condition_as`]'s refusals.
pub fn condition(
    world: &World,
    setup: Option<&str>,
    op: &SinkerBurnOp,
    need: &[ConditionField],
) -> Result<crate::machine::Condition, String> {
    condition_as(world, setup, "Sinker Burn", op, need)
}

/// The burn's condition with every field in `need` stated, read from the
/// op's setup's machine; `label` names the op kind in the refusals.
/// Refused by name: a setup not on a sinker EDM, no condition named, no
/// EDM section, or a condition missing or short of a field.
///
/// # Errors
/// Each refusal above, in words.
pub fn condition_as(
    world: &World,
    setup: Option<&str>,
    label: &str,
    op: &SinkerBurnOp,
    need: &[ConditionField],
) -> Result<crate::machine::Condition, String> {
    let kind = crate::machine::kind_of_setup(world, setup);
    if kind != Kind::SinkerEdm {
        return Err(format!(
            "a {label} burns on a sinker EDM; the setup's machine is a {}",
            kind.word()
        ));
    }
    let what = label;
    let mounted = crate::machine::of_setup(world, setup);
    let label = crate::setup_mount::machine_label(&mounted);
    let id = op.condition.trim();
    if id.is_empty() {
        return Err(format!(
            "the {what} names no EDM condition - name one of machine {label}'s conditions (its spark gap places the electrode)"
        ));
    }
    let edm = mounted
        .1
        .as_ref()
        .and_then(|m| m.edm.as_ref())
        .ok_or_else(|| format!("machine {label} states no EDM section, so no condition {id:?}"))?;
    edm.condition(&label, id, need).cloned()
}

/// The orbit's machine law (#544, #290 D6): the setup's sinker lists
/// the op's pattern under `edm.orbits` and states a maximum orbit
/// radius the op's radius does not exceed. No orbit passes untouched.
/// Refused by name: an unlisted pattern, an unstated maximum (absent is
/// not zero - nor unlimited), a radius past it.
///
/// # Errors
/// Each refusal above, in words.
pub fn orbit_law(world: &World, setup: Option<&str>, op: &SinkerBurnOp) -> Result<(), String> {
    let Some(o) = op.orbit.as_ref() else { return Ok(()) };
    let mounted = crate::machine::of_setup(world, setup);
    let label = crate::setup_mount::machine_label(&mounted);
    let kind = o.kind.word();
    let edm = mounted.1.as_ref().and_then(|m| m.edm.as_ref());
    if !edm.is_some_and(|e| e.orbits.iter().any(|w| w.kind == kind)) {
        return Err(format!(
            "machine {label} lists no {kind} orbit under edm.orbits - burn without an orbit, or pick a pattern the machine offers"
        ));
    }
    let Some(max) = edm.and_then(|e| e.max_orbit_radius) else {
        return Err(format!(
            "machine {label} states no maximum orbit radius (edm.max_orbit_radius) - a {kind} orbit cannot be checked against it"
        ));
    };
    if o.radius > max {
        return Err(format!(
            "the {kind} orbit radius {:.3} exceeds machine {label}'s maximum orbit radius {max:.3}",
            o.radius
        ));
    }
    Ok(())
}

/// What the burn reads from its machine before it generates: the
/// condition's spark gap ([`condition_gap`]), once the orbit law
/// ([`orbit_law`]) holds. The job and the staleness stamp read this.
///
/// # Errors
/// Either law's refusal.
pub fn burn_gap(world: &World, setup: Option<&str>, op: &SinkerBurnOp) -> Result<f64, String> {
    let gap = condition_gap(world, setup, op)?;
    orbit_law(world, setup, op)?;
    Ok(gap)
}

/// The generator's numbers from the op's, the electrode's and the world's.
///
/// # Errors
/// A tool that is not a sound electrode, or heights that do not resolve.
pub fn sinker_params(
    op: &SinkerBurnOp,
    tool: &Tool,
    hs: &ResolvedHeights,
    gap: f64,
    facets: &[Triangle],
) -> Result<SinkerParams, GenError> {
    if tool.kind != ToolKind::Electrode {
        return Err(GenError::Invalid(format!(
            "a Sinker Burn mounts a sinker electrode, not a {}",
            tool.kind
        )));
    }
    if let Some(why) = tool.torch_refusal() {
        return Err(GenError::Invalid(why));
    }
    let (Some(top), Some(floor)) = (hs.top, hs.bottom) else {
        return Err(GenError::Invalid("sinker burn: the work top and the cavity floor must both resolve".into()));
    };
    let model_floor = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold(f64::INFINITY, f64::min);
    Ok(SinkerParams {
        x: op.x,
        y: op.y,
        approach_z: hs.clearance,
        top_z: top,
        floor_z: floor,
        model_floor_z: model_floor.is_finite().then_some(model_floor),
        datum: op.datum,
        depth: op.depth,
        electrode_len: tool.flute_len,
        feed: op.feed,
        gap,
        stand: None,
        orbit: op.orbit,
    })
}

/// Generate the burn. A rough or finish (#543) is first judged by the
/// undersize law (`sinker_undersize::judge`), which also places the tip
/// one undersize minus XY orbit radius above the finished floor; a finish whose undersize is
/// not smaller than the rough's it follows is warned by name.
/// The burn is then wear-compensated in Z when a rate is stated (#546:
/// measured on the op, else the condition's `cond_wear`), from the
/// stand-off the undersize placed.
pub(super) fn generate_sinker(
    op: &SinkerBurnOp,
    tool: &Tool,
    hs: &ResolvedHeights,
    gap: f64,
    pass: Option<Result<PassRead, String>>,
    cond_wear: Option<f64>,
    facets: &[Triangle],
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    if let Some(why) = op.pass_refusal() {
        return Err(GenError::Invalid(why));
    }
    if let Some(why) = op.wear_refusal() {
        return Err(GenError::Invalid(why));
    }
    let mut p = sinker_params(op, tool, hs, gap, facets)?;
    let read = match (op.pass, pass) {
        (SinkerPass::Single, _) => None,
        (_, Some(r)) => Some(r.map_err(GenError::Invalid)?),
        (_, None) => {
            return Err(GenError::Invalid(format!("sinker {}: its condition's undersize was never read", op.pass.label())));
        }
    };
    if let Some(r) = &read {
        let u = Undersize {
            pass: op.pass,
            cavity: (op.cavity_x, op.cavity_y),
            electrode: (tool.diameter, tool.width),
            gap,
            undersize: r.undersize,
            orbit: op.orbit,
            // #1152: a body electrode is judged on its outline's extents.
            outline: camcraft_tools::electrode::outline(tool).is_some(),
        };
        p.stand = Some(judge(&u, op.condition.trim())?);
    }
    let w = super::sinker_wear_op::sinker_wear(op, cond_wear);
    let path = burn_compensated(&p, &w)?;
    let mut warns = Vec::new();
    if let Some(PassRead { undersize, rough: Some((_, rough)) }) = read
        && op.pass == SinkerPass::Finish
        && inverted(undersize, rough)
    {
        let at = path.moves.iter().position(|m| m.kind == crate::cl::MoveKind::Feed).unwrap_or(0);
        warns.push(Warning::SinkerFinishOrder { at, finish_mm: undersize, rough_mm: rough });
    }
    Ok((path, warns))
}

//! Shared construction gates for the operation commands: source, tool, family, and tool-law checks.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::commands::CommandError;
use crate::model;
use crate::operation::{self, OpParams, SrcRef};

/// The source door: the source names something an operation can stand
/// on, and (#1862) the profile law has nothing against this kind on it.
pub(super) fn valid_src(world: &World, setup: Option<&str>, src: &SrcRef, params: &OpParams) -> Result<(), CommandError> {
    src_exists(world, setup, src)?;
    sketch_gate(world, setup, src, params)
}

/// The profile law (#1862, `crate::sketch_model`): a DXF profile is a
/// source for the 2D cut kinds of its own setup only, and a 3D kind
/// refuses a setup whose model is virtual. For the doors that change the
/// kind or the setup without a new source.
pub(in crate::commands) fn sketch_gate(world: &World, setup: Option<&str>, src: &SrcRef, params: &OpParams) -> Result<(), CommandError> {
    match crate::sketch_model::source_problem(world, setup, src, params) {
        Some(why) => Err(CommandError::Invalid(why)),
        None => Ok(()),
    }
}

fn src_exists(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<(), CommandError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            match model::find_by_name(world, &name) {
                Some(e) if model::is_stock(world, &e) => Ok(()),
                Some(_) => {
                    Err(CommandError::Invalid(format!("'{name}' exists but is not stock")))
                }
                None => Err(CommandError::Invalid("no stock: set up stock first".into())),
            }
        }
        SrcRef::Entity(name) => {
            if name.is_empty() {
                return Err(CommandError::Invalid("empty geometry source name".into()));
            }
            match model::find_by_name(world, name) {
                Some(e) if model::is_wcs(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is a work offset, not a solid"
                ))),
                // A source is stock or one of the user's parts - never a
                // tool, an operation, a setup, the stock entity by name,
                // or workholding (a fixture is a model the collision
                // checker guards, not geometry to machine).
                Some(e) if model::is_fixture(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is a fixture - workholding is checked for collisions, not machined"
                ))),
                // #1862: a DXF profile is a source by the profile law
                // (`sketch_gate`), not a model.
                Some(e) if crate::sketch::is_sketch(world, &e) => Ok(()),
                Some(e) if !model::is_model(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is not a model - an operation's source is the stock or a part"
                ))),
                Some(_) => Ok(()),
                None => Err(CommandError::NotFound(name.clone())),
            }
        }
        SrcRef::Selection(items) => {
            if items.is_empty() {
                return Err(CommandError::Invalid(
                    "a selection source needs at least one feature".into(),
                ));
            }
            // Feature ids resolve at generation (they are geometry
            // hashes, not pointers); the entity behind each must exist
            // and be a solid, and that is all construction can promise.
            for g in items {
                if g.entity.is_empty() {
                    return Err(CommandError::Invalid(
                        "a selected feature names no entity".into(),
                    ));
                }
                match model::find_by_name(world, &g.entity) {
                    Some(e) if model::is_wcs(world, &e) => {
                        return Err(CommandError::Invalid(format!(
                            "'{}' is a work offset, not a solid",
                            g.entity
                        )));
                    }
                    Some(_) => {}
                    None => return Err(CommandError::NotFound(g.entity.clone())),
                }
            }
            Ok(())
        }
    }
}

/// The tool door: every operation kind names an existing library tool.
pub(super) fn tool_door(world: &World, _params: &OpParams, tool: &str) -> Result<(), CommandError> {
    check_tool(world, tool)
}

/// The wire door (#531, #532, #2011): a wire EDM kind's mounted wire tool and
/// setup condition must agree and state a valid spark gap - refused by name here,
/// at command creation, never first in the generator.
pub(super) fn wire_gate(world: &World, setup: Option<&str>, tool: &str, params: &OpParams) -> Result<(), CommandError> {
    match params {
        OpParams::WireContour(w) => {
            if let Some(why) = operation::wire_op::tabs_taper_refusal(w) {
                return Err(CommandError::Invalid(why));
            }
            if let Some(why) = operation::wire_sketch_arcs::lead_arc_refusal(w) {
                return Err(CommandError::Invalid(why));
            }
            let cond = operation::wire_op::condition_of(world, setup, tool, w).map_err(CommandError::Invalid)?;
            // #3991: the arc lead radius against the resolved kerf offset.
            if let Some(why) = operation::wire_sketch_arcs::lead_arc_radius_refusal(w, &cond) {
                return Err(CommandError::Invalid(why));
            }
            // #1800: a taper the mounted machine cannot post is refused
            // here, by the machine check's own words, not first at the post.
            if let Some(m) = crate::machine::of_setup(world, setup).1 {
                crate::machine::wire_edm::taper_door(&m, w.taper_deg).map_err(CommandError::Invalid)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn check_tool(world: &World, tool: &str) -> Result<(), CommandError> {
    match model::find_by_name(world, tool) {
        Some(e) if model::is_tool(world, &e) => Ok(()),
        Some(_) => Err(CommandError::Invalid(format!("'{tool}' is not a tool"))),
        None => Err(CommandError::NotFound(tool.to_string())),
    }
}

/// The family gate (M5.2 stage A): which machine a kind lives on is
/// registry data - the row's [`Family`] read against the kind of the
/// machine the op's setup mounts (#388) - so the door opens on the table in `kinds.rs`, not
/// on a hand-typed list that forgets the twenty-second kind. A
/// mill-turn admits both families; an unstated machine is a mill
/// (the kind_of law the whole app already reads the document by).
/// The ribbon's darkening is a courtesy; this door is the law - an
/// op whose family the steel cannot spin never enters the document,
/// and the refusal names both sides by name.
pub(super) fn family_gate(world: &World, context: Option<&str>, params: &OpParams) -> Result<(), CommandError> {
    use crate::operation::Family;
    let info = params.kind().info();
    let family = info.family();
    // #388: the machine of the setup the op lives in - the active one
    // for a new op, the op's own for a paste or a retype.
    let machine = crate::machine::kind_of_setup(world, context);
    // The per-kind word (M13.5 A): the family table, plus the Drill
    // kind's admission on a plain lathe for the axial hole.
    if info.admits(machine) {
        return Ok(());
    }
    // #669: a plasma table cuts only the plasma family (#670); the
    // refusal names the table's rule, not a mill to mount.
    if machine == crate::machine::Kind::Plasma {
        return Err(CommandError::Invalid(format!(
            "a {} operation is {} work; the setup's machine is a plasma table, which cuts only plasma operations (a Plasma Profile)",
            params.kind(),
            family.work(),
        )));
    }
    // #532: wire EDM work needs a wire EDM, and a wire EDM cuts only it.
    if family == Family::WireEdm || machine == crate::machine::Kind::WireEdm {
        return Err(CommandError::Invalid(format!(
            "a {} operation is {} work; the setup's machine is a {} - {}",
            params.kind(),
            family.work(),
            machine.label(),
            if family == Family::WireEdm { "mount a wire EDM first" } else { "a wire EDM cuts only wire EDM operations (a Wire Contour)" },
        )));
    }
    // #1291: an unmounted setup reads as a mill (the kind_of law), so
    // the words must not call it one - they say no machine is mounted.
    let mounted = crate::machine::of_setup(world, context).1.is_some();
    let clause = if mounted {
        format!("the setup's machine is a {}", machine.word())
    } else {
        "the setup has no machine mounted".to_owned()
    };
    // #670: plasma work needs a plasma table - no mill or lathe (nor a
    // mill-turn) strikes an arc.
    if family == Family::Plasma {
        return Err(CommandError::Invalid(format!(
            "a {} operation is plasma cutting work; {clause} - mount a plasma table first",
            params.kind(),
        )));
    }
    // #1883: waterjet work needs a waterjet - no mill-turn carries a jet.
    if family == Family::Waterjet {
        return Err(CommandError::Invalid(format!(
            "a {} operation is waterjet work; {clause} - mount a waterjet first",
            params.kind(),
        )));
    }
    // #541: sinker work needs a sinker EDM - nothing else feeds an
    // electrode.
    if family == Family::Sinker {
        return Err(CommandError::Invalid(format!(
            "a {} operation is sinker EDM work; the setup's machine is a {} - mount a sinker EDM first",
            params.kind(),
            machine.word(),
        )));
    }
    Err(CommandError::Invalid(format!(
        "a {} operation is {} work; {clause} - mount a {} (or a mill-turn) first",
        params.kind(),
        family.work(),
        family.machine(),
    )))
}

/// The sinker side burn's door (#545): its numbers, and against a named
/// condition its allowance, refused by name before the op exists
/// (`sinker_side_op::creation_refusal`). Every other kind passes.
pub(super) fn sinker_gate(world: &World, context: Option<&str>, params: &OpParams) -> Result<(), CommandError> {
    // #543: a rough or finish names what it lacks before `is_sane`'s
    // generic refusal would.
    if let OpParams::SinkerBurn(p) = params
        && let Some(why) = p.pass_refusal()
    {
        return Err(CommandError::Invalid(why));
    }
    match operation::sinker_side_op::creation_refusal(world, context, params) {
        Some(msg) => Err(CommandError::Invalid(msg)),
        None => Ok(()),
    }
}

/// The tool law's door (M10.1 B): the named tool must belong to the
/// kind's family per the `tool_kinds` table, and when the active
/// setup's tools hold nothing that fits, the refusal says so by kind
/// and setup name. Runs at construction, before any entity exists, so
/// every surface (UI, CLI, scripts, undo replay) meets the same wall.
pub(super) fn tool_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    if let Some(msg) = operation::tool_gate::tool_refusal(world, params.kind().info(), tool) {
        return Err(CommandError::Invalid(msg));
    }
    thread_gate(world, params, tool)?;
    rest_gate(world, params, tool)
}

/// The slot door (#3475): a plain Slot's tool must fit its width, by the
/// slot generator's own rule (`machining2d::slot_centre_band`), refused
/// when the operation is made, re-tooled or retyped. Open for every other
/// kind, for a width that only falls back from picks that seed no slot
/// (generation names those picks), and for a tool name that does not
/// resolve (the tool door names that one).
pub(super) fn slot_gate(world: &World, setup: Option<&str>, src: &SrcRef, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    let OpParams::Slot { width, stock_to_leave, .. } = params else {
        return Ok(());
    };
    if !operation::slot_width_is_stated(world, setup, src, params) {
        return Ok(());
    }
    match model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e)) {
        Some(def) => {
            let unit = crate::units::of(world);
            match operation::tool_gate::slot_fit_refusal(tool, def.diameter, *width, *stock_to_leave, unit) {
                Some(msg) => Err(CommandError::Invalid(msg)),
                None => Ok(()),
            }
        }
        None => Ok(()),
    }
}

/// The rest machining door (#1489): a 2D Pocket's previous tool must be
/// larger than the tool it mounts - refused by name when the operation
/// is made, edited or re-tooled, in the rest generator's words. Open for
/// every other kind, for rest machining off, and for a tool name that
/// does not resolve (the tool door names that one).
pub(super) fn rest_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    if operation::rest_op::rest_of(params).is_none() {
        return Ok(());
    }
    match model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e)) {
        Some(def) => match operation::rest_op::rest_tool_refusal(params, def.diameter) {
            Some(msg) => Err(CommandError::Invalid(msg)),
            None => Ok(()),
        },
        None => Ok(()),
    }
}

/// The thread door (#894): a thread mill against the thread the
/// operation states - a pitch outside the tool's range, a form it is not
/// ground to, an infeed past the thread depth, a mill wider than a
/// hole's minor diameter - refused by name when the operation is made,
/// edited or re-tooled. Open for every other kind and for a tool that
/// is no thread mill (the tool law names that one).
pub(super) fn thread_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    match model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e)) {
        Some(def) => match operation::thread_op::door(&def, params) {
            Some(msg) => Err(CommandError::Invalid(msg)),
            None => Ok(()),
        },
        None => Ok(()),
    }
}

/// The heights door (M11.3 A), one call for every command that writes
/// params or a source: ordering on resolved values (an op that would cut
/// upward), then every From: Selection height resolving against the
/// op's own source - an empty or vanished selection refuses by name
/// here, never reaches a generator as a silent 0 - and a trochoidal
/// microlift that would feed above the clearance plane (#774).
pub(super) fn heights_gate(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Result<(), CommandError> {
    if let Some(why) = operation::selection_heights_sane(world, setup, src, params) {
        return Err(CommandError::Invalid(why));
    }
    if !operation::depth_sane(world, setup, Some(src), params) {
        return Err(CommandError::Invalid(format!(
            "a {} operation must cut downward: its top height resolves below its bottom height (check the From references)",
            params.kind()
        )));
    }
    if let Some(why) = operation::microlift_sane(world, setup, Some(src), params) {
        return Err(CommandError::Invalid(why));
    }
    Ok(())
}

/// The flat-area door: a pocket-family selection that collapses to a
/// line seen from above (a vertical wall's rim, a non-horizontal face)
/// is refused when it is picked or the kind is set, with the fix, not
/// only later at Generate - which keeps the same refusal as backstop.
pub(super) fn area_gate(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Result<(), CommandError> {
    match operation::selection_refusal(world, setup, src, params) {
        Some(why) => Err(CommandError::Invalid(why)),
        None => Ok(()),
    }
}

/// The threading plan's door (#416): a Threading op's infeed plan read
/// against its numbers and the mounted insert's included angle - a
/// flank angle past the insert's half-angle, a minimum depth past the
/// first, an allowance as deep as the thread, refused by name before
/// the shallow `is_sane` door says only "not usable". The tool's angle
/// is read when the named tool is a threading insert; any other tool is
/// the tool gate's to refuse. The kernel asks the same question at
/// Generate (a later tool change meets it there).
pub(super) fn infeed_plan_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    let OpParams::Threading { depth, infeed, infeed_plan, .. } = params else {
        return Ok(());
    };
    let angle = model::find_by_name(world, tool)
        .and_then(|e| model::tool_of(world, &e))
        .filter(|t| t.kind == crate::tools::ToolKind::Thread)
        .map_or(180.0, |t| t.tip_angle);
    match infeed_plan.refusal(*depth, *infeed, angle) {
        Some(why) => Err(CommandError::Invalid(format!("threading: {why}"))),
        None => Ok(()),
    }
}

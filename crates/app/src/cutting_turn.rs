//! The turning half of the cutting-data seam (#891): which operations
//! run under constant surface speed, the working diameter a suggestion
//! reads its spindle speed at, and the turning suggestion itself -
//! surface speed and feed per rev from the turning table, the rpm at
//! the stock's swing diameter clamped to every stated ceiling, and the
//! mm/min feeds that make the posted per-rev F exactly the table's.
//!
//! Provenance: the relations and the table are
//! `camcraft_tools::turning_data` (machine-tool handbook turning
//! chapters, our own conservative numbers); this module only reads the
//! document - stock, WCS, machine - into them. Specced from #891.
//!
//! The honesty law is `cutting`'s: nothing is suggested where an input
//! is unstated (material, edge material, a stock to read a diameter
//! from), and the rpm never runs over the tool's or the machine's
//! stated maximum.

use flecs_ecs::prelude::World;

use crate::cutting::{Suggestion, TurnPlan};
use crate::geometry::transform_point;
use crate::model;
use crate::operation::OpParams;
use crate::post::Css;
use crate::tools::{Tool, ToolKind, ToolMaterial, WorkMaterial};
use camcraft_tools::turning_data::{self, BAR_SHARE, BLADE_FEED_SHARE};

/// The turning kinds that run under constant surface speed when their
/// tool states turning data. Threading is not one: a synchronized pass
/// needs a constant rpm (G97), whatever the diameter.
#[must_use]
pub fn css_op(p: &OpParams) -> bool {
    matches!(
        p,
        OpParams::Facing { .. }
            | OpParams::ProfileRough { .. }
            | OpParams::ProfileFinish { .. }
            | OpParams::GrooveRough { .. }
            | OpParams::GrooveFinish { .. }
            | OpParams::Part { .. }
            | OpParams::Chamfering { .. }
            | OpParams::TaperTurning { .. }
            | OpParams::MultiPassTurning { .. }
            | OpParams::FormTurning { .. }
            | OpParams::BoringTurning { .. }
    )
}

/// The cutting and plunge feeds (mm/min) of a [`css_op`] kind.
pub fn turn_feeds_mut(p: &mut OpParams) -> Option<(&mut f64, &mut f64)> {
    match p {
        OpParams::Facing { feed, plunge_feed, .. }
        | OpParams::ProfileRough { feed, plunge_feed, .. }
        | OpParams::ProfileFinish { feed, plunge_feed, .. }
        | OpParams::GrooveRough { feed, plunge_feed, .. }
        | OpParams::GrooveFinish { feed, plunge_feed, .. }
        | OpParams::Part { feed, plunge_feed, .. }
        | OpParams::Chamfering { feed, plunge_feed, .. }
        | OpParams::TaperTurning { feed, plunge_feed, .. }
        | OpParams::MultiPassTurning { feed, plunge_feed, .. }
        | OpParams::FormTurning { feed, plunge_feed, .. }
        | OpParams::BoringTurning { feed, plunge_feed, .. } => Some((feed, plunge_feed)),
        _ => None,
    }
}

/// The lathe steels the turning table speaks for.
#[must_use]
pub fn tabulated(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Turning | ToolKind::Groove | ToolKind::Form | ToolKind::Bore)
}

/// What a section posts for constant surface speed: `Some` for a
/// [`css_op`] whose tool states turning data, else None (G97, as
/// before #891).
#[must_use]
pub fn css_of(p: &OpParams, tool: &Tool) -> Option<Css> {
    if !css_op(p) {
        return None;
    }
    tool.turning.filter(turning_data::TurningData::is_sane).map(|t| Css { vc: t.vc, max_rpm: t.max_rpm })
}

/// The stock's swing diameter about the spindle axis (the program Z
/// through the setup's WCS origin, the lathe posts' meridian law): the
/// widest the work turns, so the suggested rpm is the one the OUTER
/// cut runs Vc at - every smaller diameter runs slower under G97.
#[must_use]
pub fn working_diameter(world: &World, setup: Option<&str>) -> Option<f64> {
    let name = crate::setup::stock_entity_name(world, setup);
    let e = model::find_by_name(world, &name).filter(|e| model::is_stock(world, e))?;
    let tris = model::facets_of(world, &e)?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let (_, o) = crate::posting::wcs_context(world, setup);
    let r = tris
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| transform_point(p, loc.pos, loc.rot))
        .map(|p| (p.x - o[0]).hypot(p.y - o[1]))
        .filter(|r| r.is_finite())
        .fold(0.0f64, f64::max);
    (r > 0.0).then_some(2.0 * r)
}

/// The turning arithmetic, pure: the table window's middle (scaled for
/// a blade or a bar), n = Vc*1000/(pi*D) at the working diameter held
/// to every stated ceiling, feed = f*n and plunge = f/2*n, mm/min, so
/// the post's F = feed / rpm is the table's feed per rev. Err names the
/// input that cannot make a number.
pub fn compute(
    tool_name: &str,
    tool: &Tool,
    tm: ToolMaterial,
    material: WorkMaterial,
    diameter: f64,
    machine: Option<&crate::machine::Machine>,
    workholding: Option<f64>,
) -> Result<Suggestion, String> {
    let win = turning_data::turn_window(material, tm);
    let (mut vc, mut frev) = (0.5 * (win.vc.0 + win.vc.1), 0.5 * (win.frev.0 + win.frev.1));
    match tool.kind {
        ToolKind::Groove | ToolKind::Form => frev *= BLADE_FEED_SHARE,
        ToolKind::Bore => {
            vc *= BAR_SHARE;
            frev *= BAR_SHARE;
        }
        _ => {}
    }
    let mut rpm = turning_data::rpm_at(vc, diameter)
        .ok_or_else(|| format!("the working diameter {diameter} mm makes no spindle speed"))?
        .round();
    let mut notes = Vec::new();
    let max_rpm = tool.turning.and_then(|t| t.max_rpm);
    if let Some(hi) = max_rpm
        && rpm > hi
    {
        notes.push(format!("{rpm:.0} rpm held to the tool's maximum {hi:.0}"));
        rpm = hi.floor();
    }
    // #1031: the setup's chuck or collet rating.
    if let Some(hi) = workholding
        && rpm > hi
    {
        notes.push(format!("{rpm:.0} rpm held to the workholding's maximum {hi:.0}"));
        rpm = hi.floor();
    }
    if let Some(m) = machine {
        if let Some(hi) = m.spindle_max
            && rpm > hi
        {
            notes.push(format!("{rpm:.0} rpm held to the machine's {hi:.0}"));
            rpm = hi.floor();
        }
        if let Some(lo) = m.spindle_min
            && rpm < lo
        {
            // Raising it would run Vc over the table at this diameter
            // and could cross the tool's own ceiling: refuse instead.
            return Err(format!(
                "{rpm:.0} rpm at the {diameter:.1} mm working diameter is under the machine's minimum {lo:.0}"
            ));
        }
    }
    if !(rpm.is_finite() && rpm >= 1.0) {
        return Err(format!("the clamps leave no spindle speed ({rpm})"));
    }
    let feed = frev * rpm;
    let plunge_feed = 0.5 * frev * rpm;
    if let Some(ceil) = machine.and_then(|m| m.feed_ceiling([1.0, 1.0, 0.0]))
        && feed > ceil
    {
        return Err(format!("{feed:.0} mm/min is over the machine's feed ceiling {ceil:.0}"));
    }
    Ok(Suggestion {
        tool: tool_name.to_owned(),
        material,
        tool_material: tm,
        vc,
        fz: frev,
        thinning: None,
        rpm,
        feed,
        notes,
        turning: Some(TurnPlan { diameter, plunge_feed, max_rpm }),
    })
}

/// The suggestion for a [`css_op`] whose tool is `tool`.
pub fn suggest(world: &World, op: &crate::schema::Operation, tool: &Tool, material: WorkMaterial) -> Result<Suggestion, String> {
    if !tabulated(tool.kind) {
        return Err(format!("the turning table does not speak for a {}", tool.kind));
    }
    let tm = tool
        .tool_material
        .ok_or_else(|| format!("tool {} states no tool material (HSS or carbide)", op.tool))?;
    let setup = op.setup.as_deref();
    let d = working_diameter(world, setup).ok_or("no stock to read a working diameter from - set the stock first")?;
    let hold = crate::setup_workholding::of(world, setup);
    compute(&op.tool, tool, tm, material, d, crate::machine::of_setup(world, setup).1.as_ref(), hold)
}

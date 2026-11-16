//! The cutting-data seam (M13.2): the document's workpiece material,
//! the feeds-and-speeds suggestion an operation's page offers, and the
//! spindle-power estimate the machine check judges a program by.
//!
//! Provenance: the formulas and the table are `camcraft_tools::cutting`
//! (public handbook relations, our own numbers); this module only reads
//! an operation's own engagement numbers - stepdown, stepover, optimal
//! load - into them. Specced from ROADMAP M13.2.
//!
//! The honesty law is the unstated-limit law's: an estimate is claimed
//! only where every input is STATED - the document's material, the
//! tool's flute count, and an engagement the operation's numbers fix.
//! Where any is missing, nothing is claimed (and nothing is suggested);
//! a guess dressed as an estimate would be the silent wrong answer.

use flecs_ecs::core::WorldGet; // not in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::machining2d::PocketStrategy;
use crate::model;
use crate::operation::OpParams;
use crate::schema::WorkMaterialDef;
use camcraft_tools::cutting::{self, MaterialRow};
use crate::tools::{Tool, ToolKind, ToolMaterial, WorkMaterial};

/// The document's stated workpiece material; `None` when unstated.
#[must_use]
pub fn material_of(world: &World) -> Option<WorkMaterial> {
    world.try_get::<&WorkMaterialDef>(|m| m.0).unwrap_or_default()
}

/// Write the singleton. For `SetWorkMaterial` and the project reader.
pub fn material_set(world: &World, m: Option<WorkMaterial>) {
    world.set(WorkMaterialDef(m));
}

/// The rotating milling cutters the table speaks for. Drills, taps and
/// the turning steels (whose cutting speed is the WORK's diameter, not
/// the tool's) are not tabulated here - the turning steels have their
/// own table (#891, [`crate::cutting_turn::tabulated`]).
#[must_use]
pub fn tabulated(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::EndMill | ToolKind::BallNose | ToolKind::BullNose | ToolKind::Taper)
}

/// The operation's adaptive load (ae/D) where it rides one: the
/// constant-engagement ladders, whose radial width IS the load.
#[must_use]
pub fn adaptive_load(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Adaptive2D { optimal_load, .. } => Some(*optimal_load),
        OpParams::Pocket { strategy: PocketStrategy::Adaptive, optimal_load, .. }
        | OpParams::Pocket3D { strategy: PocketStrategy::Adaptive, optimal_load, .. }
        | OpParams::MultiAxisRough { strategy: PocketStrategy::Adaptive, optimal_load, .. } => {
            Some(*optimal_load)
        }
        _ => None,
    }
}

/// The engagement an operation's own numbers fix, (ap, ae) in mm, or
/// `None` where they fix none. Adaptive ladders bite `load x D`; the
/// other pocket ladders and the slot cut full width somewhere (a row or
/// ring with uncut material on both sides IS a slot - the worst case is
/// the honest one for a ceiling); a staged face bites its stepover.
#[must_use]
pub fn engagement(p: &OpParams, d: f64) -> Option<(f64, f64)> {
    if let Some(load) = adaptive_load(p) {
        let ap = match p {
            OpParams::Adaptive2D { stepdown, .. }
            | OpParams::Pocket { stepdown, .. }
            | OpParams::Pocket3D { stepdown, .. }
            | OpParams::MultiAxisRough { stepdown, .. } => *stepdown,
            _ => return None,
        };
        return Some((ap, load * d));
    }
    match p {
        OpParams::Pocket { stepdown, .. }
        | OpParams::Offset2D { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::TrochoidalSlot { stepdown, .. }
        | OpParams::Pocket3D { stepdown, .. }
        | OpParams::MultiAxisRough { stepdown, .. } => Some((*stepdown, d)),
        OpParams::Face { stepdown: Some(ap), top_z: Some(_), stepover, .. } => {
            Some((*ap, stepover.min(d)))
        }
        _ => None,
    }
}

/// A spindle-power estimate for one operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Power {
    pub kw: f64,
    /// Material removal rate, mm^3/min.
    pub mrr: f64,
    /// Specific cutting force at the operation's mean chip, N/mm^2.
    pub kc: f64,
}

/// The estimate: Pc = ae*ap*vf*kc/(60e6*eta), kc from the Kienzle law
/// at the mean chip of fz = vf/(n*z). `None` = not claimable (module
/// law): an untabulated tool, unstated flutes, no fixed engagement.
#[must_use]
pub fn power_of(p: &OpParams, tool: &Tool, m: WorkMaterial) -> Option<Power> {
    if !tabulated(tool.kind) || tool.kind == ToolKind::Taper {
        return None;
    }
    let z = tool.flutes?;
    let (ap, ae) = engagement(p, tool.diameter)?;
    let ap = ap.min(tool.flute_len);
    let vf = feed_of(p)?;
    let fz = vf / (tool.rpm * f64::from(z));
    let hm = cutting::mean_chip(fz, ae / tool.diameter)?;
    let kc = cutting::kc(m.row(), hm);
    let mrr = ae * ap * vf;
    let kw = cutting::power_kw(ae, ap, vf, kc);
    kw.is_finite().then_some(Power { kw, mrr, kc })
}

/// What the page offers: the table's numbers for the op's tool on the
/// document's material, and the spindle speed and feed they make.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub tool: String,
    pub material: WorkMaterial,
    pub tool_material: ToolMaterial,
    /// Cutting speed, m/min (the table window's middle).
    pub vc: f64,
    /// Feed per tooth the table states for this diameter, mm.
    pub fz: f64,
    /// The chip-thinning factor hex/fz the feed was raised by, where
    /// the op rides an optimal load.
    pub thinning: Option<f64>,
    /// Whole rpm, after the machine's stated spindle range.
    pub rpm: f64,
    /// Whole mm/min, after the machine's stated feed ceiling.
    pub feed: f64,
    /// Every clamp a stated machine limit applied, named.
    pub notes: Vec<String>,
    /// `Some` for a turning suggestion (#891): then `fz` is the feed
    /// per REV (mm/rev), `rpm` the speed at the working diameter and
    /// `feed` = fz x rpm, so the posted per-rev F is exactly `fz`.
    pub turning: Option<TurnPlan>,
}

/// What a turning suggestion adds (#891, [`crate::cutting_turn`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnPlan {
    /// The working (swing) diameter the rpm was read at, mm.
    pub diameter: f64,
    /// The plunge feed, mm/min: half the feed per rev, at `rpm`.
    pub plunge_feed: f64,
    /// The tool's own max-rpm clamp, kept as the tool states it.
    pub max_rpm: Option<f64>,
}

/// Compute the suggestion for operation `name`, or say by name why the
/// table cannot speak for it.
pub fn suggest(world: &World, name: &str) -> Result<Suggestion, String> {
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no operation named '{name}'"))?;
    let op = model::operation_of(world, &e).ok_or_else(|| format!("'{name}' is not an operation"))?;
    // #1110: a kind that mounts no library tool (a wire cut) has no tool
    // for the table to read - said so, not "tool  is not in the library".
    if !op.params.kind().info().mounts_tool() {
        return Err(format!(
            "a {} mounts no library tool - its cut is the machine's EDM condition, and the cutting-data table speaks for milling tools only",
            op.params.kind()
        ));
    }
    let tool = model::find_by_name(world, &op.tool)
        .and_then(|t| model::tool_of(world, &t))
        .ok_or_else(|| format!("tool {} is not in the library", op.tool))?;
    let material = material_of(world).ok_or("no workpiece material is stated - pick one first")?;
    if crate::cutting_turn::css_op(&op.params) {
        return crate::cutting_turn::suggest(world, &op, &tool, material);
    }
    if !tabulated(tool.kind) {
        return Err(format!("the cutting-data table does not speak for a {}", tool.kind));
    }
    let tm = tool
        .tool_material
        .ok_or_else(|| format!("tool {} states no tool material (HSS or carbide)", op.tool))?;
    let z = tool.flutes.ok_or_else(|| format!("tool {} states no flute count", op.tool))?;
    Ok(compute(&op.tool, &tool, z, tm, material, adaptive_load(&op.params), crate::machine::of_setup(world, op.setup.as_deref()).1.as_ref()))
}

/// The arithmetic, pure: n = Vc*1000/(pi*D), fz scaled by diameter and
/// raised by 1/sin(phi) under an optimal load, vf = fz*z*n - each
/// clamped to what a stated machine allows.
#[must_use]
pub fn compute(
    tool_name: &str,
    tool: &Tool,
    z: u32,
    tm: ToolMaterial,
    material: WorkMaterial,
    load: Option<f64>,
    machine: Option<&crate::machine::Machine>,
) -> Suggestion {
    let win = material.window(tm);
    let vc = 0.5 * (win.vc.0 + win.vc.1);
    let fz = 0.5 * (win.fz10.0 + win.fz10.1) * cutting::fz_scale(tool.diameter);
    let thinning = load.map(cutting::thinning_factor).filter(|f| *f < 1.0);
    let fz_prog = fz / thinning.unwrap_or(1.0);
    let mut notes = Vec::new();
    let mut rpm = cutting::rpm_for(vc, tool.diameter).round();
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
            notes.push(format!("{rpm:.0} rpm raised to the machine's minimum {lo:.0}"));
            rpm = lo.ceil();
        }
    }
    let mut feed = cutting::feed_for(fz_prog, z, rpm).round();
    if let Some(ceil) = machine.and_then(|m| m.feed_ceiling([1.0, 1.0, 0.0]))
        && feed > ceil
    {
        notes.push(format!("{feed:.0} mm/min held to the machine's {ceil:.0}"));
        feed = ceil.floor();
    }
    Suggestion { tool: tool_name.to_owned(), material, tool_material: tm, vc, fz, thinning, rpm, feed, notes, turning: None }
}

/// The row a material reads (re-exported for the pages' readouts).
#[must_use]
pub fn row(m: WorkMaterial) -> &'static MaterialRow {
    m.row()
}

/// An operation's cutting feed, mm/min (every kind states one).
#[must_use]
pub fn feed_of(p: &OpParams) -> Option<f64> {
    let mut q = p.clone();
    feed_mut(&mut q).map(|f| *f)
}

/// The cutting feed field of the milling kinds the table can suggest
/// for; `None` for the rest (turning feeds are per revolution of the
/// WORK, which no milling table speaks for).
pub fn feed_mut(p: &mut OpParams) -> Option<&mut f64> {
    match p {
        OpParams::Face { feed, .. }
        | OpParams::Contour { feed, .. }
        | OpParams::Pocket { feed, .. }
        | OpParams::Adaptive2D { feed, .. }
        | OpParams::Offset2D { feed, .. }
        | OpParams::Engrave { feed, .. }
        | OpParams::Chamfer2D { feed, .. }
        | OpParams::Circular { feed, .. }
        | OpParams::Slot { feed, .. }
        | OpParams::TrochoidalSlot { feed, .. }
        | OpParams::Trace { feed, .. }
        | OpParams::Pocket3D { feed, .. }
        | OpParams::Finish3D { feed, .. }
        | OpParams::Deburr { feed, .. }
        | OpParams::Contour5D { feed, .. }
        | OpParams::Parallel5D { feed, .. }
        | OpParams::Swarf { feed, .. }
        | OpParams::AdvancedSwarf { feed, .. }
        | OpParams::RotaryContour { feed, .. }
        | OpParams::RotaryParallel { feed, .. }
        | OpParams::RotaryOffset { feed, .. }
        | OpParams::MultiAxisRough { feed, .. }
        | OpParams::Flow5D { feed, .. }
        | OpParams::MultiAxisContour { feed, .. }
        | OpParams::Bore { feed, .. }
        | OpParams::Thread { feed, .. } => Some(feed),
        _ => None,
    }
}

//! Document stock: the world-facing half of `stock_sim`. The engine
//! (dexel grid, sweeps, measures, deviation, traverse timing) lives
//! in camcraft-sim (crates/sim/src/stock_sim.rs, M4.1 R4); what stays
//! here is exactly the part that walks the world - building the stock
//! solid from the `Stock`/`Setup` entities and driving a whole
//! document through the sim: the tuple seam the design note promised,
//! the engine consumes (facets, stock, tool) data, this module hands
//! it over.

pub use camcraft_sim::stock_sim::*;

// The rotary clause (M13.1): the eye's chord law, the verdict's
// refusal, the per-kind lift.
#[path = "stock_sim_rotary.rs"]
pub mod rotary;
pub use rotary::{
    rotary_kind_lift, rotary_pose_tol, simulate_rotary_slice, ROTARY_KIND_UNMEASURED, ROTARY_VERDICT_REFUSAL,
};

// The lathe clause (#19): the nose and the spindle in the plan, the
// blade's refusal, the verdict's refusal.
#[path = "stock_sim_lathe.rs"]
pub mod lathe;
pub use lathe::{LathePlan, LATHE_HELIX_REFUSAL, LATHE_THREAD_REFUSAL, LATHE_VERDICT_REFUSAL};

// The carry across setups (#317): the machined body moved into the
// next setup's frame through the transform between the two.
#[path = "stock_sim_carry.rs"]
pub mod carry;
// The stock as the walk holds it, and the engine that carries it (#637).
#[path = "stock_sim_held.rs"]
pub mod held;
#[path = "stock_sim_breaks.rs"]
pub mod breaks;
// #1595: the marks an Engrave or Trace on the part declares.
#[path = "stock_sim_marks.rs"]
mod marks;
// The wire clause (#1110): a wire cut stays out of the plan, by name.
#[path = "stock_sim_wire.rs"]
pub mod wire;
#[path = "stock_sim_waterjet.rs"]
pub mod waterjet;
pub use carry::{
    CARRY_MACHINE_CHANGE_REFUSAL, CARRY_MACHINE_REFUSAL, carry_machine_refusal, incoming_body, machine_kind_name,
};
pub use held::{CARRY_ENGINE_ENV, CARRY_ENGINE_SWITCH_HONOURED, FIELD_CARRY_SHAPE_REFUSAL, engine as carry_engine};
pub use held::{engine_from_switch as carry_engine_from_switch, engine_note as carry_engine_note, with_engine as with_carry_engine};

use crate::geometry::Triangle;

#[path = "stock_grid.rs"]
mod grid;
pub use grid::stock_grid;

/// Simulate the whole document's program: the operations in posting
/// order (through [`crate::posting::program_ops`] - so what the sim
/// judges IS what would post, one law shared, not two agreeing), each
/// op's CACHED moves through its tool's atom, against the stock as the
/// setup defines it. Move coordinates are pre-origin-shift world
/// millimeters, the same numbers the viewport draws and the post
/// shifts; the stock grid is built on that same frame. Refusals name
/// their reason (an op that never generated, a rotated stock, a grid
/// too fine) and change nothing.
pub fn simulate_document(
    world: &flecs_ecs::prelude::World,
) -> Result<ProgramSim, String> {
    let planned = plan_program(world)?;
    wire::refuse_wire_cuts(world)?;
    waterjet::refuse_waterjet_cuts(world)?;
    refuse_unjudged(&planned)?;
    // #864: a part imported coarser than promised tightens the verdict by
    // its excess; one too coarse for that is refused by name, not judged -
    // and so is a program with an op the gate refuses for it (#1056).
    let tol = crate::model_fidelity::program_tol(world)?;

    // Setup changes (#317): a setup that holds the part differently, or
    // is placed relative to the one before it, starts from the body the
    // earlier setups left, carried through the transform between them
    // (`carry`); two setups holding it alike keep the M3.2 A clip. The
    // verdict is read in the frame of the setup the program ends in.
    let walked = carry::walk(world, &planned, None)?;
    let model_facets = if walked.spanned {
        crate::setup_align::parts_facets_in(world, walked.frame.as_deref())
    } else {
        part_facets(world)?
    };
    let (mut dev, display) = walked.held.verdict(&model_facets, tol)?;
    // #303: the op owns its break - a chamfer's or deburr's declared
    // bevel is part of the finished part, judged by its own depth.
    let breaks = breaks::program_breaks(world, &planned, walked.spanned, &walked.frame, &model_facets, tol);
    credit_breaks(&mut dev, &display, &model_facets, &breaks);
    // #1595: and its mark - an Engrave's or Trace's own sweep, within
    // the budget the operation's own law holds it to.
    credit_marks(&mut dev, &display, &model_facets, &marks::program_marks(world, &planned, walked.spanned, &walked.frame));
    // #2400: declared pocket/slot corner reliefs, bounded by cutter radius and cut floor.
    let reliefs = program_corner_reliefs(world, &planned, walked.spanned, &walked.frame);
    credit_corner_reliefs(&mut dev, &display, &model_facets, &reliefs);
    // #320: a gouge on a block a later setup set aside for a new one.
    if let Some(left) = &walked.left {
        carry::fold_left(&mut dev, left);
    }
    let classes = color_classes(&dev);
    let surface = display.surface(&classes);
    Ok(ProgramSim { dev, surface })
}

/// The kinds the column verdict cannot judge, refused by name before
/// any material is cut.
///
/// The posed clause (M13.1 follow-up): the verdict is the COLUMN
/// engine, whose atoms stand upright - a 5-axis lean or a 3+2 frame's
/// tilted moves would be cut as if vertical, a silent wrong verdict.
/// The field arm does cut a posed move exactly, but its verdict reads
/// heights marched from above, and a tilted tool is precisely the tool
/// that can reach UNDER an overhang. So neither arm's height verdict
/// judges a posed program - refusal by name, which the post gate shows
/// as "not judged".
pub(crate) fn refuse_unjudged(planned: &[Planned]) -> Result<(), String> {
    for p in planned {
        // The lathe clause (#19): a turning program is the field
        // eye's to cut (`stock_sim::lathe`), never this height
        // field's to judge.
        if p.lathe.is_some() {
            return Err(format!("{}: {LATHE_VERDICT_REFUSAL}", p.name));
        }
        // The rotary clause (M13.1): named before the posed one, so
        // the gate says WHY this posed program is not judged here.
        if p.rotary.is_some() {
            return Err(format!("{}: {ROTARY_VERDICT_REFUSAL}", p.name));
        }
        if let Some(k) = first_posed(&p.moves) {
            return Err(format!("{}: move {k}: {POSED_REFUSAL}", p.name));
        }
    }
    Ok(())
}

/// One operation of the program, resolved for the sim: its cached
/// moves and the atom its tool sweeps.
pub(crate) struct Planned {
    /// The operation's name - the sim timeline's key for it.
    pub name: String,
    pub moves: Vec<crate::cl::Move>,
    pub radius: f64,
    pub ball: bool,
    pub cone: Option<f64>,
    /// The bull nose's corner radius (M13.1), `bull_corner`'s law.
    pub corner: Option<f64>,
    pub setup: Option<String>,
    /// A rotary op's fitted axis (M13.1): the eye chords its turns
    /// about it (`rotary::simulate_rotary_slice`); the column verdict
    /// refuses the program by name. `None` for every other kind.
    pub rotary: Option<crate::verify::RotaryAxis>,
    /// A Turn-family op's nose and spindle (#19): the eye revolves
    /// its moves about the axis (`lathe::lathe_plan`); the column
    /// verdict refuses the program by name. `None` for every mill op.
    pub lathe: Option<LathePlan>,
    /// The edge breaks the op was asked to cut (#303); empty for every
    /// kind but the chamfer and the deburr.
    pub breaks: Vec<crate::cl::EdgeBreak>,
    /// #2435: the breaks are a 2D Chamfer's, whose tip the law puts on
    /// a wall - credited only where it stands on one (`on_walls`).
    pub walled: bool,
    /// #2596: a 2D Chamfer with no picks (its source the whole model):
    /// on a model that carries a modelled bevel, none of its breaks are
    /// credited (`stock_sim_breaks::program_breaks`).
    pub auto_edges: bool,
    /// The mark budget and target part(s) the op declares (#1595, #1627,
    /// `operation::mark_law`): `Some` only for an Engrave or Trace.
    pub mark: Option<crate::operation::mark_law::SimMark>,
    /// A sinker electrode's atom (#541, #1152). The column verdict cuts
    /// it; the field eye has no electrode atom and refuses the program
    /// by name. `None` for every other tool.
    pub electrode: Option<held::ElectrodeAtom>,
    /// #2441: a twist drill's cone carries a cylindrical flute above
    /// `r * cot`, unlike a truncated chamfer `Taper`.
    pub drill: bool,
    /// An undercut cutter's pass (#1444): the head and neck solids and
    /// the moves they cut. `moves` above is then EMPTY: an undercut
    /// clears no column, so every column reader (the program verdict,
    /// the carried stock, the rest grids) cuts nothing for it, and only
    /// the field eye removes the true profile
    /// (`Stock::simulate_undercut`). `None` for every other tool.
    pub undercut: Option<UndercutPlan>,
    /// Declared inside-corner relief corridors computed live from operation geometry (#2400).
    pub corner_reliefs: Vec<crate::verify::CornerReliefZone>,
}

/// An undercut op's cut, for the field eye ([`Planned::undercut`]).
pub(crate) struct UndercutPlan {
    pub cutter: camcraft_tools::undercut::Undercut,
    /// The neck's length above the head, mm.
    pub neck_len: f64,
    pub moves: Vec<crate::cl::Move>,
}

pub(super) fn program_corner_reliefs(
    world: &flecs_ecs::prelude::World,
    planned: &[Planned],
    spanned: bool,
    frame: &Option<String>,
) -> Vec<crate::verify::CornerReliefZone> {
    planned
        .iter()
        .filter(|p| !spanned || crate::setup::effective(world, p.setup.as_deref()) == *frame)
        .flat_map(|p| p.corner_reliefs.iter().copied())
        .collect()
}

/// The field eye's refusal of a sinker electrode (#541), said once.
pub const ELECTRODE_FIELD_REFUSAL: &str = "the stock view has no sinker electrode atom yet - the program verdict (the column engine's box atom) judges the burn";

/// The corner radius a tool's atom carries (M13.1): Some only for a
/// bull nose with a real corner, clamped to the tool radius (the
/// `Tool::is_sane` bound, restated so the atom can never be handed a
/// corner wider than its tool). A zero corner IS the flat end mill,
/// cut by the disc exactly; a non-finite one refuses by name.
pub(crate) fn bull_corner(tool: &crate::tools::Tool) -> Result<Option<f64>, String> {
    if tool.kind != crate::tools::ToolKind::BullNose {
        return Ok(None);
    }
    let (c, r) = (tool.corner_r, tool.diameter * 0.5);
    if !(c.is_finite() && r.is_finite() && c >= 0.0 && r > 0.0) {
        return Err(format!("bull nose carries no sane corner (corner radius {c}, radius {r})"));
    }
    Ok((c > 0.0).then_some(c.min(r)))
}

/// The program in posting order, each op resolved to its tool's atom
/// - the one plan both the column verdict and the field eye walk.
pub(crate) fn plan_program(world: &flecs_ecs::prelude::World) -> Result<Vec<Planned>, String> {
    plan_ops(world, &crate::posting::program_ops(world)?)
}

/// The playback's plan (#2280): [`plan_program`] up to the first op
/// with no toolpath, and that op and every op after it, in program
/// order (empty when every op has a toolpath). The verdict and the
/// post never read this: they keep `plan_program`, which refuses.
pub(crate) fn plan_program_until_ungenerated(
    world: &flecs_ecs::prelude::World,
) -> Result<(Vec<Planned>, crate::posting::OpList), String> {
    let (ops, rest) = crate::posting::program_ops_until_ungenerated(world)?;
    Ok((plan_ops(world, &ops)?, rest))
}

fn plan_ops(world: &flecs_ecs::prelude::World, ops: &[(String, flecs_ecs::prelude::Entity)]) -> Result<Vec<Planned>, String> {
    use crate::model;

    // Tools and cached paths, resolved once, in program order. The
    // atom follows the tool kind (stage A's law): a ball nose sweeps
    // the exact sphere; a chamfer mill sweeps its exact cone (M2.4 B
    // - the disc would phantom-gouge every chamfer wall against the
    // full radius at the corner depth); a bull nose sweeps its exact
    // corner-rounded solid (M13.1 - the disc it rode before removed
    // the fillet's ring the real tool leaves); everything whose cut
    // IS its cylinder - end mills, drills, bores - sweeps the disc,
    // and a tap sweeps the disc of its thread's core (#266,
    // `tool_law::cutting_radius`).
    let mut planned: Vec<Planned> = Vec::with_capacity(ops.len());
    for (name, e) in ops {
        let op = model::operation_of(world, e)
            .ok_or_else(|| format!("{name}: lost its operation"))?;
        if wire::skips(&op) || waterjet::skips(&op) {
            continue;
        }
        // The rotary clause (M13.1): between two stations a rotary
        // move turns the WORK, so the tool sweeps an annular sector
        // about the fitted axis - a torus-family solid no stock engine
        // has in closed form. The field arm chords the turn
        // (`Stock::simulate_rotary`, the `rotary_chords` pre-pass:
        // pose lerped per sub-chord at a stated sag) with parity
        // against the radial law on the sim's bar and on the app's
        // fixtures (`rotary_field_parity.rs`), so the plan carries the
        // fitted axis for the eye; a kind whose parity or cost is not
        // proved refuses by name here (`rotary::rotary_kind_lift`),
        // and the column VERDICT refuses every rotary program by name
        // in `simulate_document` (`ROTARY_VERDICT_REFUSAL`).
        let rotary = rotary::rotary_plan(world, name, &op)?;
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        // A touch probe removes no stock (#108): its stylus only ever
        // meets the part tangent, and the protected-move law in
        // `operation::probe_op` already convicted any other meeting -
        // so the probe op carves nothing and stays out of the plan.
        // A plasma torch (#670) cuts THROUGH the plate along a kerf the
        // dexel column model has no atom for: the plasma verdicts (cut
        // order, kerf crossings, the through-cut) are #672's laws, and
        // the generator certified that no lit move's kerf reaches the
        // part - so the torch carves nothing here, said not silenced.
        if matches!(tool.kind, crate::tools::ToolKind::Probe | crate::tools::ToolKind::Torch | crate::tools::ToolKind::Wire) {
            continue;
        }
        let cache = model::toolpath_of(world, e)
            .ok_or_else(|| format!("{name}: lost its toolpath"))?;
        // The cone's slope, cot of the half-angle from the axis -
        // the number the dexel / drill cone reads directly, so the sim and
        // the congruence door in `generate` see the same steel (#2441).
        let (cone, drill) = match tool.kind {
            crate::tools::ToolKind::Taper => {
                let tan_half = (tool.tip_angle.to_radians() * 0.5).tan();
                if tan_half.is_finite() && tan_half > 0.0 && tool.tip_angle > 0.0 && tool.tip_angle < 180.0 {
                    (Some(1.0 / tan_half), false)
                } else {
                    return Err(format!(
                        "{name}: chamfer mill {} carries no sane cone (tip angle {})",
                        op.tool, tool.tip_angle
                    ));
                }
            }
            crate::tools::ToolKind::Drill => {
                if (tool.tip_angle - 180.0).abs() <= 1e-9 {
                    (None, false)
                } else {
                    let tan_half = (tool.tip_angle.to_radians() * 0.5).tan();
                    if tan_half.is_finite() && tan_half > 0.0 && tool.tip_angle > 0.0 && tool.tip_angle < 180.0 {
                        (Some(1.0 / tan_half), true)
                    } else {
                        return Err(format!(
                            "{name}: drill {} carries no sane point cone (tip angle {})",
                            op.tool, tool.tip_angle
                        ));
                    }
                }
            }
            _ => (None, false),
        };
        let corner = bull_corner(&tool).map_err(|e| format!("{name}: {e}"))?;
        // The revolved clause (#19, was M5.2 G.2's refusal): a Turn-
        // family path lives in the half-plane of its fitted axis, so
        // the plan carries the nose and that axis for the field eye's
        // revolved stadium, a blade its own footprint (`lathe`, #21).
        let lathe = lathe::lathe_plan(world, name, &op, &tool, &cache.path.moves)?;
        // A turning insert's cutting feature is its nose (a blade's,
        // half its band - `LathePlan::feature`): the display cell law
        // (`auto_cell`) reads it, not the IC. Every other tool sweeps
        // its cutting radius - a tap its thread's core (#266, #985).
        // A sinker electrode (#541) sweeps its rectangular face - or its
        // outline (#1152) - grown by the condition's spark gap; its
        // radius is the least half extent, which only the cell law reads.
        let electrode = match &op.params {
            crate::operation::OpParams::SinkerBurn(sb) if tool.kind == crate::tools::ToolKind::Electrode => {
                let gap = crate::operation::sinker_op::condition_gap(world, op.setup.as_deref(), sb)
                    .map_err(|e| format!("{name}: {e}"))?;
                Some(held::ElectrodeAtom::of(&tool, gap))
            }
            // #545: the side burn sweeps the same box atom along its
            // lateral move.
            crate::operation::OpParams::SinkerSideBurn(sb) if tool.kind == crate::tools::ToolKind::Electrode => {
                let gap = crate::operation::sinker_side_op::side_gap(world, op.setup.as_deref(), sb)
                    .map_err(|e| format!("{name}: {e}"))?;
                Some(held::ElectrodeAtom::of(&tool, gap))
            }
            _ => None,
        };
        let radius = match (lathe, &electrode) {
            (Some(plan), _) => plan.feature(),
            (None, Some(e)) => e.half[0].min(e.half[1]),
            (None, None) => crate::operation::thread_op::sim_radius(&tool, &op.params).map_err(|e| format!("{name}: {e}"))?,
        };
        // #546: a wear-compensated burn is judged as the worn electrode burns it.
        let moves = crate::operation::sinker_wear_op::judged(world, &op, electrode.as_ref().map(|e| e.gap), cache.path.moves)
            .map_err(|e| format!("{name}: {e}"))?;
        // #1444: an undercut cutter's moves ride its own plan.
        let undercut = if camcraft_tools::undercut::is_kind(tool.kind) {
            let cutter = camcraft_tools::undercut::Undercut::of(&tool).ok_or_else(|| {
                format!(
                    "{name}: {}",
                    camcraft_tools::undercut::refusal(&tool).unwrap_or_else(|| "its tool reads as no undercut cutter".into())
                )
            })?;
            Some(UndercutPlan { cutter, neck_len: tool.shank_len, moves: moves.clone() })
        } else {
            None
        };
        let moves = if undercut.is_some() { Vec::new() } else { moves };
        let corner_reliefs = crate::operation::corner_relief::zones_for_op(world, &op, name);
        planned.push(Planned {
            name: name.clone(),
            moves,
            // #529: only what the op itself declared, never wider. #2308:
            // and never a stock edge's bevel over a part.
            breaks: if breaks::breaks_stock_edge(world, &op) { Vec::new() } else { breaks::bounded(name, &op.params, cache.path.breaks) },
            walled: matches!(op.params, crate::operation::OpParams::Chamfer2D { .. }),
            auto_edges: breaks::auto_edges(&op),
            mark: crate::operation::mark_law::sim_mark_of(world, &op, crate::operation::mark_law::cutter_depth_limit(&op.params, &tool)),
            radius,
            ball: tool.kind == crate::tools::ToolKind::BallNose,
            cone,
            corner,
            setup: op.setup,
            rotary,
            lathe,
            electrode,
            drill,
            undercut,
            corner_reliefs,
        });
    }
    Ok(planned)
}

/// The part as the machine must leave it: every model's facets in
/// world, minus the stock (setup furniture, not design intent) - and
/// minus the workholding, which M3.2 B made models too. A vise is
/// steel the program must dodge, never a promise it must leave
/// standing at height: its facets belong to the collision gate, not
/// to the gouge verdict (counting them here would read a clamped job
/// as a part with a vise-shaped hole in it). Visibility is a display
/// choice; the verdict judges all the rest.
///
/// #326: in a document where a setup holds the part turned, the part is
/// read in the frame of the one setup the program machines in; a program
/// spanning setups that hold it differently is refused by name here - the
/// field view draws one setup's frame. The program verdict
/// (`simulate_document`) carries the body across such setups (#317) and
/// reads the part in the frame the program ends in instead.
pub(crate) fn part_facets(world: &flecs_ecs::prelude::World) -> Result<Vec<Triangle>, String> {
    if !crate::setup_align::any_aligned(world) {
        return Ok(part_facets_as_modelled(world));
    }
    let mut setups: Vec<Option<String>> = Vec::new();
    for (_, e) in crate::setup::operation_order(world) {
        let Some(op) = crate::model::operation_of(world, &e) else { continue };
        if op.suppressed {
            continue;
        }
        let eff = crate::setup::effective(world, op.setup.as_deref());
        if !setups.contains(&eff) {
            setups.push(eff);
        }
    }
    match setups.as_slice() {
        [] => Ok(crate::setup_align::parts_facets_in(world, crate::setup::active(world).as_deref())),
        [one] => Ok(crate::setup_align::parts_facets_in(world, one.as_deref())),
        many if many.iter().any(|s| crate::setup_align::alignment(world, s.as_deref()).is_some()) => Err(
            "the program spans setups that hold the part differently (part alignment) - the field view draws one setup's frame; the program verdict carries the stock between them"
                .into(),
        ),
        _ => Ok(part_facets_as_modelled(world)),
    }
}

fn part_facets_as_modelled(world: &flecs_ecs::prelude::World) -> Vec<Triangle> {
    use crate::geometry::transform_point;
    use crate::model;

    let mut model_facets: Vec<Triangle> = Vec::new();
    let skip = crate::stock_source::stock_only_model(world); // #1094
    for (_, e) in model::model_entities(world) {
        if !crate::stock_source::judged(world, &e, skip) {
            continue;
        }
        let tris = model::facets_of(world, &e).unwrap_or_default();
        let l = model::transform_of(world, &e).unwrap_or_default();
        model_facets.extend(tris.iter().map(|t| Triangle::new(
            transform_point(t.a, l.pos, l.rot),
            transform_point(t.b, l.pos, l.rot),
            transform_point(t.c, l.pos, l.rot),
        )));
    }
    model_facets
}

// The engine-law unit tests (the ball atom, the reach law, the
// envelope gate), split out by the 400-line law (M13.1).
#[cfg(test)]
#[path = "stock_sim_tests.rs"]
mod tests;

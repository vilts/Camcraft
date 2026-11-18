//! The operation page's params block (split out of `pages.rs` with the
//! property grid, user review 2026-11-06): the per-kind notes the arms
//! quote (Auto's verdicts), then one call per kind family module - each
//! draws only its own kinds' arms, for the open tab only. The placement
//! law is unchanged: a field renders when, and only when, its section's
//! guard is the tab being asked (`op_tabs::SECTIONS`).
//!
//! Provenance: our own port of the M-c page code; no vendor material.

use super::fields::*;

/// What every family's arms read besides the draft: the unit lens, the
/// open tab, the op's setup context, the Auto notes, and the one
/// deferred action (Detect holes) the arms may ask for.
pub(super) struct Cx<'a> {
    pub u: Unit,
    pub tab: usize,
    pub op_ctx: &'a Option<String>,
    pub side_note: &'a Option<String>,
    pub auto_note: &'a Option<String>,
    pub cycle_note: &'a Option<String>,
    pub engrave: bool,
    pub detect_holes: bool,
    /// #1171: the Wire Contour sync pick is armed (in), and the page
    /// asks to arm or disarm it (out; applied after the arms ran).
    pub sync_armed: bool,
    pub sync_toggle: bool,
    /// The Circular op's circle read off its picks (`Some(Ok)`), or the
    /// fit's refusal; `None` while the typed centre and radius rule.
    pub circle_fit: &'a Option<Result<(camcraft_app::geo2d::Point2, f64), String>>,
    /// #1504: the Slot's room read off its picks (`Some(Ok)` when the
    /// slot fits it), or the refusal; `None` while the typed slot rules.
    pub slot_fit: &'a Option<Result<String, String>>,
    /// #1815: diameter of the currently selected tool for seeding rest params.
    pub tool_diameter: Option<f64>,
}

/// The note explaining what `PocketStrategy::Auto` will generate: when rest
/// machining is on in the draft, generation resolves `Auto` via `rest_strategy`
/// rather than the pocket's outline.
fn pocket_auto_note(
    world: &World,
    op: Option<&camcraft_app::schema::Operation>,
    stepover: f64,
    rest_on: bool,
) -> String {
    let pick = "Auto picks the shape from the pocket's outline at generate.";
    if rest_on {
        return match camcraft_app::machining2d::rest_strategy(PocketStrategy::Auto) {
            Ok(PocketStrategy::Waterline) => {
                "Auto -> Waterline: rest machining cuts concentric rings along each rest area."
            }
            Ok(PocketStrategy::OffsetRings) => {
                "Auto -> Offset rings: rest machining uses concentric rings along each rest area."
            }
            Ok(PocketStrategy::Zigzag) => {
                "Auto -> Zigzag: rest machining fills each rest area with back-and-forth rows."
            }
            Ok(PocketStrategy::Adaptive) => {
                "Auto -> Adaptive: rest machining clears each rest area with adaptive passes."
            }
            Ok(PocketStrategy::Auto) | Err(_) => pick,
        }
        .into();
    }
    let Some(o) = op else {
        return pick.into();
    };
    let region = operation::region_of(world, o).unwrap_or_default();
    let diameter = model::find_by_name(world, &o.tool)
        .as_ref()
        .and_then(|t| model::tool_of(world, t))
        .map_or(6.0, |t| t.diameter);
    let outer: Vec<camcraft_app::geo2d::Point2> = region
        .loops()
        .iter()
        .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
        .map(|l| l.points().to_vec())
        .unwrap_or_default();
    if outer.is_empty() {
        return pick.into();
    }
    match PocketStrategy::Auto.resolve(&outer, stepover, diameter, ARC_FIT_TOL) {
        PocketStrategy::Waterline => {
            "Auto -> Waterline: the outline is round and the rings reach every band.".into()
        }
        _ => "Auto -> Zigzag: rings would not suit this outline.".into(),
    }
}

pub(super) fn params_fields(ui: &mut Ui, state: &mut State, tab: usize) {
    let u = state.units();
    let mut side_note: Option<String> = None;
    let mut auto_note: Option<String> = None;
    let mut cycle_note: Option<String> = None;
    let nm = state.op_ui.name.clone();
    // Height echoes resolve in the editing operation's declared setup
    // context (M3.2 A); an operation that does not exist yet - a draft
    // name - resolves `None`, the first setup, what CreateOperation will
    // declare for it if the cursor is untouched.
    let op_ctx: Option<String> = model::find_by_name(&state.world, &nm)
        .and_then(|e| model::operation_of(&state.world, &e))
        .and_then(|o| o.setup);
    // ...and against the operation's own source, which a From: Selection
    // height reads (M11.3 A): handed to every height editor of this
    // pass through the frame's temp data, so no row signature moves.
    ui.data_mut(|d| d.insert_temp(egui::Id::new(HEIGHT_SRC_ID), state.op_ui.src()));
    if let OpParams::Drill {
        cycle: camcraft_app::drilling::CycleChoice::Auto,
        holes,
        cross,
        ..
    } = &state.op_ui.params
    {
        // What Auto answers for, in the tool's and first hole's own
        // numbers - the same call generation makes (`auto_kind`), never
        // a panel-side guess that could drift from it. A radial list
        // keeps its note to generation (M5.2 F2): the kernel's depth
        // starts at the R standoff, which the panel would only guess.
        let radial = !cross.is_empty();
        cycle_note = match (
            model::find_by_name(&state.world, &state.op_ui.tool)
                .as_ref()
                .and_then(|t| model::tool_of(&state.world, t)),
            if radial { None } else { holes.first() },
        ) {
            (Some(t), Some(h)) => {
                let depth = h.entry_z - h.floor_z;
                let pick = camcraft_app::drilling::auto_kind(t.kind, depth, t.diameter);
                let name = match pick {
                    camcraft_app::cl::CycleKind::Spot => "Spot",
                    camcraft_app::cl::CycleKind::Drill => "Drill",
                    camcraft_app::cl::CycleKind::Peck => "Peck",
                    camcraft_app::cl::CycleKind::Deep => "Deep (G83)",
                    camcraft_app::cl::CycleKind::Tap => "Tap (rigid)",
                    camcraft_app::cl::CycleKind::FloatingTap => "Tap (floating holder)",
                    camcraft_app::cl::CycleKind::Bore => "Bore",
                    camcraft_app::cl::CycleKind::BackBore => "Back bore",
                    camcraft_app::cl::CycleKind::Ream => "Ream (feed in, feed out)",
                };
                Some(format!(
                    "Auto -> {name} for this tool and a depth of {}",
                    units::len(depth, units::of(&state.world), 3)
                ))
            }
            _ => Some("Auto picks the cycle from the tool and each hole's depth.".into()),
        };
    }
    let op = model::find_by_name(&state.world, &nm)
        .as_ref()
        .and_then(|e| model::operation_of(&state.world, e));
    if let OpParams::Contour {
        side: Side::Auto, ..
    } = &state.op_ui.params
    {
        let resolved = op.as_ref().and_then(|o| {
            let _s = crate::frame_cost::span("ui/properties.op.fields.contour_auto_side");
            operation::contour_auto_side(
                &state.world,
                o.setup.as_deref(),
                &o.src,
                &state.op_ui.params.clone(),
            )
        });
        side_note = Some(match resolved {
            Some(true) => {
                "Auto -> Inside: the picked rings bound air, so the metal is outside them.".into()
            }
            Some(false) => "Auto -> Outside: the picked rings bound material.".into(),
            None => "Auto picks the side from the selection at generate.".into(),
        });
    }
    if let OpParams::Pocket {
        strategy: PocketStrategy::Auto,
        stepover,
        rest,
        ..
    } = &state.op_ui.params
    {
        let _s = crate::frame_cost::span("ui/properties.op.fields.pocket_auto_note");
        auto_note = Some(pocket_auto_note(&state.world, op.as_ref(), *stepover, rest.is_some()));
    }

    // Which Trace-shape name the arm below renders: the engrave twin
    // IS Trace's field shape (M2.4 B), so they share one arm and only
    // the height-editor ids switch. Read before the match's borrow.
    let engrave = matches!(&state.op_ui.params, OpParams::Engrave { .. });

    // The picks' circle, through the same law generation runs, for the
    // draft's own source (the typed fields stay untouched beneath it).
    let draft_op = || camcraft_app::schema::Operation {
        tool: state.op_ui.tool.clone(),
        src: state.op_ui.src(),
        params: state.op_ui.params.clone(),
        stamp: None,
        setup: op_ctx.clone(),
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    // #1504: the Slot's picks, through the same law generation runs.
    let slot_fit = if matches!(&state.op_ui.params, OpParams::Slot { .. }) {
        operation::slot_fit_of(&state.world, &draft_op())
    } else {
        None
    };
    let circle_fit = if matches!(&state.op_ui.params, OpParams::Circular { .. }) {
        let draft = camcraft_app::schema::Operation {
            tool: state.op_ui.tool.clone(),
            src: state.op_ui.src(),
            params: state.op_ui.params.clone(),
            stamp: None,
            setup: op_ctx.clone(),
            area: Default::default(),
            coolant: Default::default(),
            after: Default::default(),
            suppressed: false,
        };
        operation::circular_fit_of(&state.world, &draft)
    } else {
        None
    };
    let tool_diameter = model::find_by_name(&state.world, &state.op_ui.tool)
        .as_ref()
        .and_then(|t| model::tool_of(&state.world, t))
        .map(|t| t.diameter);
    let mut cx = Cx {
        u,
        tab,
        op_ctx: &op_ctx,
        side_note: &side_note,
        auto_note: &auto_note,
        cycle_note: &cycle_note,
        engrave,
        detect_holes: false,
        sync_armed: state.wire_sync.armed,
        sync_toggle: false,
        circle_fit: &circle_fit,
        slot_fit: &slot_fit,
        tool_diameter,
    };
    let world = &state.world;
    let params = &mut state.op_ui.params;
    super::mill2d_page::fields(ui, world, params, &mut cx);
    super::mill2d_more_page::fields(ui, world, params, &mut cx);
    super::holes_page::fields(ui, world, params, &mut cx);
    super::probe_page::fields(ui, world, params, &mut cx);
    super::plasma_page::fields(ui, world, params, &mut cx);
    super::wire_page::fields(ui, world, params, &mut cx);
    super::sinker_page::fields(ui, world, params, &mut cx);
    super::trochoid_page::fields(ui, world, params, &mut cx);
    super::mill3d_page::fields(ui, world, params, &mut cx);
    super::multiaxis_page::fields(ui, world, params, &mut cx);
    super::rotary_page::fields(ui, world, params, &mut cx);
    super::turning_page::fields(ui, world, params, &mut cx);
    super::turning_more_page::fields(ui, world, params, &mut cx);
    let sync_toggle = cx.sync_toggle;
    let detect_holes = cx.detect_holes;
    if sync_toggle {
        if state.wire_sync.armed {
            crate::wire_sync_pick::disarm(state);
        } else {
            crate::wire_sync_pick::arm(state);
        }
    }
    if detect_holes {
        state.detect_holes(&nm);
    }
}


//! The operation page: the 5-axis kinds' fields: contour, parallel, flow, multi-axis contour, swarf - one module per kind family, split out
//! of `pages.rs` (the 400-line law) when the property grid landed (user
//! review 2026-11-06). Each arm draws its kind's fields for the open tab
//! through the `layout` form helpers; every value edits the page draft,
//! and Apply rides the command history.
//!
//! Provenance: our own UI over the operation parameter model (each
//! kind's own module names its sources); no vendor material or text.

use super::fields::*;

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let _ = (u, tab, op_ctx);
    #[allow(unused_variables)]
    let (side_note, auto_note, cycle_note, engrave) = (cx.side_note, cx.auto_note, cx.cycle_note, cx.engrave);
    match params {
        // The 5-axis twins (M5.1 stage C). The heights, feeds and the
        // scallop law are the finish's words verbatim - the paths ARE
        // the finish's paths with a stamped pose. There is no Pattern
        // combo: the kind is the choice of kinematic (contour = the
        // ring ladder, parallel = the raster), and the new content is
        // the axis trio the `multiaxis` law reads.
        OpParams::Contour5D {
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "contour5d.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "contour5d.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "contour5d.bottom",
                );
                hint(ui, "Top is the first ring's plane, Bottom the last level's");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "contour5d.retract");
                });
            }
            if tab == SEC_PASSES {
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the finish budget: levels drop by 2 x sqrt(tool radius x tolerance), so the cusp under the ball stays within it");
                hint(ui, "the rings are the waterline's; the leaning spindle is what makes this a 5-axis op");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        OpParams::Parallel5D {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "parallel5d.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "parallel5d.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "parallel5d.bottom",
                );
                hint(ui, "Top is the first line's plane, Bottom the last one's");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "parallel5d.retract");
                });
            }
            if tab == SEC_PASSES {
                auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
                    num(ui, u, s)
                });
                hint(ui, "Auto: from tolerance and tool by the scallop law, times the slope's cosine. Stated: flat-ground spacing; on a slope it follows the terrain's cosine");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the finish budget: lines are spaced and levels drop from it, so the cusp under the ball stays within it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // Flow (M5.1 I): the parallel's panel, field for field - the
        // direction the lines follow is read off the model, so the
        // only difference is the wording that says so.
        OpParams::Flow5D {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "flow5d.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "flow5d.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "flow5d.bottom",
                );
                hint(ui, "Top is the first line's plane, Bottom the last one's");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "flow5d.retract");
                });
            }
            if tab == SEC_PASSES {
                auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
                    num(ui, u, s)
                });
                hint(ui, "Auto: from tolerance and tool by the scallop law, times the slope's cosine. Stated: flat-ground spacing; on a slope it follows the terrain's cosine");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the finish budget: lines are spaced and levels drop from it, so the cusp under the ball stays within it");
                hint(ui, "the lines follow the surface's own least-curving direction; flat ground is cut in straight passes");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The drive-curve contour (M5.1 J): flow's panel minus the
        // stepover - the PASSES page has no pattern to author,
        // because the GEOMETRY page's picked chains ARE the path.
        OpParams::MultiAxisContour {
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multi_axis_contour.clearance",
                );
                height_editor(
                    ui,
                    "Top Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "multi_axis_contour.top",
                );
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multi_axis_contour.bottom",
                );
                hint(ui, "the machining window: a picked chain standing outside it is refused by name, never clipped");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "multi_axis_contour.retract");
                });
            }
            if tab == SEC_PASSES {
                hint(ui, "no pattern - the picked edge chains are the path, machined where they stand; see Geometry");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the verify budget: it authors nothing here - the envelope scan measures the chain-riding path against it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The flow band (#2826): the contour's panel minus the station
        // window - the picked chains bound the band, so the heights
        // read as BALL-CENTER RADII about the stated axis and no
        // Top/Bottom exists to author.
        OpParams::RotaryFlow {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            stated_axis,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance radius",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "rotary_flow.clearance",
                );
                hint(ui, "a ball-center radius about the rotation axis: the program opens, hops bands and exits on that cylinder");
                group(ui, LEVELS, None, |ui| {
                    retract_radius_hop(ui, world, op_ctx.as_deref(), retract_z, clearance_z, "rotary_flow.retract");
                });
            }
            if tab == SEC_PASSES {
                // #2772's stated axis, like the rotary twins: the band's
                // heights read as radii about the rotation axis, so the
                // axis stands above the pose trio it answers for.
                rotary_stated_axis_editor(ui, stated_axis);
                auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
                    num(ui, u, s)
                });
                hint(ui, "Auto: from tolerance and tool by the scallop law. Stated: developed spacing at the band's outermost radius");
                rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the scallop budget: the flow passes blend between the picked chains and the cusps between them stay within it");
                hint(ui, "the guide: the picked edge chains span the band - pick them on the Geometry tab; a pick that is not an edge chain refuses by name");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The swarf family (M5.1 stage D): the raster's panel, because
        // the path is the raster run along the ruling. Two arms like
        // the twins' - the ladder's extra word has no name for the
        // plain kind to bind - and one shared body, `swarf_editor`.
        OpParams::Swarf {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg, picks: _,
        } => swarf_editor(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            None,
        ),
        OpParams::AdvancedSwarf {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            passes, picks: _,
        } => swarf_editor(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            Some(passes),
        ),
        _ => {}
    }
}

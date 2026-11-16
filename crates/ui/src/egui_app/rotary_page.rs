//! The operation page: the rotary kinds' fields and the multi-axis rough - one module per kind family, split out
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
        // The rotary twins (M5.1 stage E): the swarf family's panel
        // worded for the TURN - the axis is a reading of the model, so
        // the trio stands in for it where the ruling's name would
        // otherwise be. Two arms, one body: the contour kind has no
        // spacing field for the or-pattern to name, so the arms hand
        // it in as data, the twins' law.
        OpParams::RotaryContour {
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
        } => rotary_editor(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            None,
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
        ),
        OpParams::RotaryParallel {
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
        } => rotary_editor(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            Some(stepover),
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
        ),
        // The wrap ladder (M5.1 stage E2): the pocket's panel worded
        // for the TURN - stepdown infeeds radii, stepover spaces
        // arc-length around the bar - and the twins' pose trio. No
        // retract row anywhere: the kernel's rapids ride the skin by
        // law, and the panel says so where the field would stand.
        OpParams::RotaryOffset {
            links,
            tolerance,
            stepover,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            optimal_load,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "ro", Some((entry, helix_radius)), links);
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "rotary_offset.clearance",
                );
                height_editor(
                    ui,
                    "Top Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "rotary_offset.top",
                );
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "rotary_offset.bottom",
                );
                hint(ui, "the world planes map onto stations along the fitted axis - the ladder roughs the bar only between them (inert on a bar lying flat)");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown (radial)", |ui| num(ui, u, stepdown)));
                hint(ui, "the radial infeed: what one wrap level peels off the skin");
            }
            if tab == SEC_PASSES {
                if !strategy.steps_by_load() {
                    prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
                    hint(ui, "spacing around the bar, in developed arc length - the wrap's own ruler");
                }
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                hint(ui, "the shelf the finishing kind is asked to take off");
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                    PocketStrategy::OffsetRings => 4,
                };
                let names = strategy3d_names();
                let before = st_idx;
                prop_row(ui, "Strategy", |ui| combo(ui, "ro_strat", &mut st_idx, names));
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        4 => PocketStrategy::OffsetRings,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if strategy.steps_by_load() {
                    super::load_field::optimal_load_field(ui, optimal_load);
                }
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the verify budget: the swept envelope is judged against the model within it");
                rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The stamped ladder (M5.1 stage E3): the pocket's rows verbatim
        // - no rest-roughing or feed-optimization switches, because the
        // kernel runs neither over a stamp - plus the tolerance (verify
        // budget) and the cone the poses live inside.
        OpParams::MultiAxisRough {
            links,
            tolerance,
            stepover,
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            optimal_load,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "ma", Some((entry, helix_radius)), links);
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multiaxis_rough.clearance",
                );
                height_editor(
                    ui,
                    "Top Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "multiaxis_rough.top",
                );
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multiaxis_rough.bottom",
                );
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "multiaxis_rough.retract");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_PASSES {
                if !strategy.steps_by_load() {
                    prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
                }
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                    PocketStrategy::OffsetRings => 4,
                };
                let names = strategy3d_names();
                let before = st_idx;
                prop_row(ui, "Strategy", |ui| combo(ui, "ma_strat", &mut st_idx, names));
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        4 => PocketStrategy::OffsetRings,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if strategy.steps_by_load() {
                    super::load_field::optimal_load_field(ui, optimal_load);
                }
                hint(ui, "levels come from the mesh: each shelf cuts around the material standing above it");
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the verify budget: the swept envelope is judged against the model within it");
                hint(ui, "the ladder is move for move the 3-D pocket's; the spindle adds a pose where a wall or corner needs one, and stands vertical where a floor does not ask");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        _ => {}
    }
}

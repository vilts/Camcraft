//! The operation page: the 2D milling kinds' fields: Face, Contour, Pocket and the roughing twins - one module per kind family, split out
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
        OpParams::Face {
            stepover,
            clearance_z,
            cut_z,
            top_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            blend_turns, blend_radius,
            holes,
            tilt, picks,
        } => {
            if tab == SEC_GEO {
                super::start_exit_rows::raster_groups(ui, u, picks); // #3326
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "face.clearance",
                );
                height_editor(ui, "Cut Z", world, op_ctx.as_deref(), cut_z, "face.cut");
                group(ui, LEVELS, None, |ui| {
                    let mut staged = top_z.is_some();
                    if check_row(ui, "Stage down to Cut Z", &mut staged).changed() {
                        if staged {
                            let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                            *top_z = Some(seed);
                            *stepdown = Some(1.0);
                        } else {
                            *top_z = None;
                            *stepdown = None;
                        }
                    }
                    hint(ui, "depth staging: off faces in one pass at Cut Z; on steps down from Top Z");
                    if staged {
                        let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                        height_editor(
                            ui,
                            "Top Z (first cut level)",
                            world,
                            op_ctx.as_deref(),
                            top_z.get_or_insert(seed),
                            "face.top",
                        );
                        prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown.get_or_insert(1.0)));
                    }
                });
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Stepover (Y spacing)", |ui| num(ui, u, stepover));
                // M11.1 A: row ends turn on one arc where it is legal;
                // the status line names every turn left sharp.
                // #3356: Face rows run edge to edge - no ring order.
                super::pass_start_row::no_pass_order_row(ui, "face", super::pass_start_row::FACE_NO_PASS_ORDER);
                super::kind_editors::arc_turn_rows_auto(ui, u, ("Arc turns at row ends", "Arc radius = half the row spacing"), blend_turns, blend_radius);
                // M13.11: how a row meets a hole in the faced region.
                // Lift is safe for a bore and a boss alike; cutting
                // across air is the user's call, islands stay lifted.
                let mut h_idx = match holes {
                    FaceHoles::Lift => 0,
                    FaceHoles::CutAir => 1,
                    FaceHoles::Through => 2,
                };
                let before = h_idx;
                prop_row(ui, "Holes", |ui| {
                    combo(ui, "f_holes", &mut h_idx, &["Lift over", "Cut across air, lift over islands", "Cut straight through"])
                });
                if h_idx != before {
                    *holes = match h_idx {
                        1 => FaceHoles::CutAir,
                        2 => FaceHoles::Through,
                        _ => FaceHoles::Lift,
                    };
                }
                hint(ui, "rows stop a tool radius short of a hole and cross it at clearance; cut-across runs through a bore at feed");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        OpParams::Contour {
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            side,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
            entry,
            helix_radius,
            blend_turns, blend_radius,
            links,
            tilt,
            chain_overrides,
            picks,
        } => {
            if tab == SEC_GEO {
                let mut s_idx = match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                };
                prop_row(ui, "Side", |ui| combo(ui, "c_side", &mut s_idx, side_names()));
                *side = match s_idx {
                    1 => Side::Inside,
                    2 => Side::Outside,
                    _ => Side::Auto,
                };
                if let Some(note) = &side_note {
                    consequence(ui, note);
                }
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                sketch_chain_overrides_ui(ui, world, cx.op_src, chain_overrides);
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "contour.clearance",
                );
                height_editor(
                    ui,
                    "Retract Z (between passes)",
                    world,
                    op_ctx.as_deref(),
                    retract_z,
                    "contour.retract",
                );
                retract_note(ui, world, op_ctx.as_deref(), retract_z, clearance_z);
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "contour.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "contour.bottom",
                );
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                // M11.1 B: air-side corners round on one arc where it
                // is legal; the status line names every corner left
                // sharp and the corner rest the rounding leaves.
                super::kind_editors::arc_turn_rows(ui, u, "Arc turns at corners", blend_turns, blend_radius);
            }
            if tab == SEC_LEADS {
                let mut l_idx = match lead {
                    Lead::None => 0,
                    Lead::Linear { .. } => 1,
                    Lead::Arc { .. } => 2,
                    Lead::Tangential { .. } => 3,
                    Lead::Perpendicular { .. } => 4,
                };
                let before = l_idx;
                prop_row(ui, "Lead in/out", |ui| combo(ui, "c_lead", &mut l_idx, &LEADS));
                if l_idx != before {
                    *lead = match l_idx {
                        1 => Lead::Linear { length: 8.0 },
                        2 => Lead::Arc { radius: 5.0 },
                        3 => Lead::Tangential { radius: 5.0 },
                        4 => Lead::Perpendicular { length: 5.0 },
                        _ => Lead::None,
                    };
                }
                match lead {
                    Lead::Linear { length } | Lead::Perpendicular { length } => {
                        prop_row(ui, "Lead length", |ui| num(ui, u, length));
                    }
                    Lead::Arc { radius } | Lead::Tangential { radius } => {
                        // One label for both arc radii: the widgets can
                        // never coexist (the draft holds one lead), and
                        // the number is the same thing - the arc's
                        // radius.
                        prop_row(ui, "Lead arc radius", |ui| num(ui, u, radius));
                    }
                    Lead::None => {}
                }
                lead_page::leads_tab(ui, u, "c", Some((entry, helix_radius)), links);
                hint(ui, "keep down between levels cuts each ring all the way down, descending on the ring itself");
                super::approach_page::contour_leads_editor(ui, u, &mut links.approach, picks);
                let mut tabs = f64::from(*tab_count);
                prop_row(ui, "Tab count", |ui| num_plain(ui, &mut tabs));
                *tab_count = tabs.clamp(0.0, 99.0).round().max(0.0) as u32;
                prop_row(ui, "Tab width", |ui| num(ui, u, tab_width));
                prop_row(ui, "Tab height", |ui| num(ui, u, tab_height));
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        OpParams::Pocket {
            links,
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
            blend_turns, blend_radius,
            rest,
            corner_relief,
            tilt,
            pass_start, picks: _,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "p", Some((&mut *entry, helix_radius)), links);
                super::approach_page::pocket_arc_group(ui, u, *entry, &mut links.approach);
                super::approach_page::approach_distance_group(ui, u, *pass_start, &mut links.approach);
                super::predrilled_page::predrilled_group(ui, &mut links.approach, rest.is_some());
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "pocket.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "pocket.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "pocket.bottom",
                );
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "pocket.retract");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_PASSES {
                // #1512: one radial-step field - Stepover, or the
                // Optimal load below for the strategies it spaces.
                if !strategy.steps_by_load() {
                    prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
                }
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                // M11.1 C: pass corners and row-to-row links round on
                // one arc inside the cleared area where it is legal.
                super::kind_editors::arc_turn_rows(ui, u, "Arc turns at corners and links", blend_turns, blend_radius);
                super::kind_editors::corner_relief_rows(ui, world, op_ctx.as_deref(), "pocket", corner_relief);
                // #1517: Adaptive left 2D Pocket for 2D Adaptive.
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto | PocketStrategy::Adaptive => 2,
                    PocketStrategy::OffsetRings => 3,
                };
                let names = strategy_names();
                let before = st_idx;
                prop_row(ui, "Strategy", |ui| combo(ui, "p_strat", &mut st_idx, names));
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Auto,
                        3 => PocketStrategy::OffsetRings,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if strategy.steps_by_load() {
                    super::load_field::optimal_load_field(ui, optimal_load);
                }
                // #3354: the shared pass-order row; the ring strategies
                // read it, Zigzag rows say by name that they do not.
                super::pass_start_row::pass_start_row(ui, "pocket", pass_start);
                if let Some(note) = &auto_note {
                    consequence(ui, note);
                }
                super::rest_pocket_page::rest_group(ui, u, cx.tool_diameter, *strategy, &links.approach, rest);
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The roughing twins (M2.4 A): the pocket's pane with the
        // Strategy combo retired - the kind IS the strategy, so the
        // choice that would undo the name is not offered. Adaptive
        // shows the engagement door; Offset has none to show.
        OpParams::Adaptive2D {
            links,
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
            entry,
            helix_radius,
            optimal_load, method, troch, blend_turns, blend_radius,
            rest: _, // #1446: no rows on this page yet; the draft keeps the value
            corner_relief,
            tilt,
            pass_start, picks: _,
        } => roughing2d_pane(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            "adaptive",
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
            entry,
            helix_radius,
            Some(optimal_load), Some((method, troch)),
            links, (blend_turns, blend_radius),
            corner_relief,
            tilt,
            Some(pass_start),
        ),
        OpParams::Offset2D {
            links,
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
            entry,
            helix_radius, blend_turns, blend_radius,
            corner_relief,
            tilt,
            pass_start, picks: _,
        } => roughing2d_pane(
            ui,
            world,
            op_ctx.as_deref(),
            u,
            tab,
            "offset",
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
            entry,
            helix_radius,
            None, None,
            links, (blend_turns, blend_radius),
            corner_relief,
            tilt,
            Some(pass_start),
        ),
        _ => {}
    }
}

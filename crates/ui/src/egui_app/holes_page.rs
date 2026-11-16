//! The operation page: the hole kinds' fields: Drill, Bore, Thread - one module per kind family, split out
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
        OpParams::Drill {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            cycle,
            peck,
            dwell,
            feed,
            plunge_feed,
            holes,
            cross,
        } => {
            if tab == SEC_GEO {
                field(ui, "Holes");
                if holes.is_empty() && cross.is_empty() {
                    hint(ui, "none yet - Detect holes reads them from the model");
                }
                axial_hole_rows(ui, u, holes);
                // The radial half of the list (M5.2 F2): a cross hole
                // states its mouth in world XYZ, its plunge by the
                // station's axis, and its floor as a travel INWARD
                // from the mouth - the mouth is the surface it opens
                // on, not a Z plane.
                for (i, h) in cross.iter().enumerate() {
                    let station = if h.dir.x != 0.0 { "X" } else { "Y" };
                    let floor = if h.through {
                        "through".to_owned()
                    } else {
                        format!("depth {}", units::len(-h.floor_d, u, 3))
                    };
                    list_row(
                        ui,
                        format!(
                            "R{}  ({}, {}, {})  Ø{}  plunge {station}  {floor}",
                            holes.len() + i + 1,
                            units::len(h.origin.x, u, 3),
                            units::len(h.origin.y, u, 3),
                            units::len(h.origin.z, u, 3),
                            units::len(2.0 * h.radius, u, 3),
                        ),
                    );
                }
                // Edge-driven, one recognize pass per click (never per
                // paint frame): the list it finds replaces the draft's,
                // and the command lands so undo restores the old one.
                if button(ui, "Detect holes").clicked() {
                    cx.detect_holes = true;
                }
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "drill.clearance",
                );
                height_editor(ui, "R plane (feed starts here)", world, op_ctx.as_deref(), top_z, "drill.top");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "drill.retract");
                    hole_floor(ui, world, op_ctx.as_deref(), bottom_z, top_z, "drill.bottom");
                });
            }
            if tab == SEC_PASSES {
                let mut c_idx = cycle_index(cycle);
                let before = c_idx;
                prop_row(ui, "Cycle", |ui| combo(ui, "d_cycle", &mut c_idx, cycle_names()));
                if c_idx != before {
                    *cycle = cycle_from_index(c_idx);
                }
                if let Some(note) = &cycle_note {
                    consequence(ui, note);
                }
                auto_value(ui, "Peck depth", "Auto", peck, "by the cycle", PECK_SEED, |ui, v| num(ui, u, v));
                hint(ui, "Auto lets the cycle size itself: 3 x dia chip-break, 1 x dia full-retract");
                prop_row(ui, "Dwell at bottom", |ui| num_unit(ui, dwell, "s"));
                hint(ui, "seconds held at the bottom; honored on the cycles that dwell");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
            }
        }
        // The standalone boring ride (M2.4 C): the drill's hole-list
        // shape - the same Geometry list, the same R-plane/Depth
        // promise - but its Passes are the pocket family's (a leave,
        // a tolerance for the ring's chords) and its ladder is the
        // stepdown's, spoken on one circle per hole.
        OpParams::Bore {
            links,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tolerance,
            holes,
        } => {
            if tab == SEC_LEADS {
                // Each hole opens with the dive at its centre; the
                // descents between levels and the links are the user's.
                hint(ui, "each hole opens with a plunge at its centre");
                lead_page::leads_tab(ui, u, "bore", None, links);
            }
            if tab == SEC_GEO {
                field(ui, "Holes");
                if holes.is_empty() {
                    hint(ui, "none yet - Detect holes reads them from the model");
                }
                axial_hole_rows(ui, u, holes);
                if button(ui, "Detect holes").clicked() {
                    cx.detect_holes = true;
                }
                hint(ui, "a stepped hole is declined by name at generate - this ride wants one finished wall per hole");
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "bore.clearance",
                );
                height_editor(ui, "R plane (feed starts here)", world, op_ctx.as_deref(), top_z, "bore.top");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "bore.retract");
                    hole_floor(ui, world, op_ctx.as_deref(), bottom_z, top_z, "bore.bottom");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the ring is chords to this tolerance - the same fit law the pocket walls keep");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The helical thread mill (M2.4 C): the hole list again - the
        // ride is one helix per hole - and its Passes says the only
        // thing the drill's cycle had no word for: how deep each
        // ring's infeed goes. The pitch lives on the TOOL; the pane
        // says so in one line, because a silent number the tool owns
        // is how staleness hides (the stamp's law).
        OpParams::Thread {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            infeed,
            feed,
            plunge_feed,
            climb,
            holes,
            pitch,
            hand,
            external,
            passes,
        } => {
            if tab == SEC_GEO {
                // #985: holes, or bosses for an external thread.
                cx.detect_holes |= super::thread_page::geometry_rows(ui, u, holes, external);
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "thread.clearance",
                );
                height_editor(ui, "R plane (feed starts here)", world, op_ctx.as_deref(), top_z, "thread.top");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "thread.retract");
                    hole_floor(ui, world, op_ctx.as_deref(), bottom_z, top_z, "thread.bottom");
                });
            }
            if tab == SEC_PASSES {
                super::thread_page::thread_rows(ui, u, pitch, hand);
                prop_row(ui, "Radial infeed per pass", |ui| num(ui, u, infeed));
                hint(ui, super::thread_page::infeed_hint(*external));
                super::thread_page::pass_rows(ui, u, passes);
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
                super::thread_page::spindle_rows(ui, passes);
            }
        }
        _ => {}
    }
}

/// Axial hole list rows shared by Drill and Bore Geometry tabs.
fn axial_hole_rows(ui: &mut Ui, u: Unit, holes: &[camcraft_app::drilling::Hole]) {
    for (i, h) in holes.iter().enumerate() {
        let floor = if h.through {
            "through".to_owned()
        } else {
            format!("floor {}", units::len(h.floor_z, u, 3))
        };
        list_row(
            ui,
            format!(
                "{}  X {}  Y {}  Ø{}  entry {}  {floor}",
                i + 1,
                units::len(h.xy.x, u, 3),
                units::len(h.xy.y, u, 3),
                units::len(2.0 * h.radius, u, 3),
                units::len(h.entry_z, u, 3),
            ),
        );
    }
}

/// The peck depth an un-Auto'd field starts from, mm: a modest bite the
/// user then sizes to the drill (Auto's own law is 3 x / 1 x diameter).
const PECK_SEED: f64 = 2.0;

/// The hole kinds' depth override, inside the Levels group: off, every
/// hole stops at its own floor from the model; on, one Bottom Z for all.
fn hole_floor(ui: &mut Ui, world: &World, setup: Option<&str>, bottom_z: &mut Option<Height>, top_z: &Height, tag: &str) {
    let mut over = bottom_z.is_some();
    if check_row(ui, "Override the hole floors", &mut over).changed() {
        *bottom_z = if over { Some(Height::of(top_z.reference, top_z.offset - 1.0)) } else { None };
    }
    match bottom_z {
        Some(b) => height_editor(ui, "Bottom Z", world, setup, b, tag),
        None => hint(ui, "every hole stops at its own floor from the model"),
    }
}

//! The operation page: the remaining 2D milling kinds' fields: Slot, Trace / Engrave, Chamfer, Deburr, Circular - one module per kind family, split out
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
        OpParams::Slot {
            links,
            a,
            b,
            width,
            stock_to_leave,
            clearance_z,
            retract_z: _,
            top_z,
            bottom_z,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
            entry,
            helix_radius,
            tilt,
            extend_start,
            extend_end,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "s", Some((entry, helix_radius)), links);
            }
            if tab == SEC_GEO {
                // #1504: picks seed the slot and bound it - echoed here,
                // or the refusal generation would make.
                match cx.slot_fit {
                    Some(Ok(note)) => hint(ui, note),
                    Some(Err(e)) => hint(ui, e),
                    None => {}
                }
                prop_row(ui, "A X", |ui| num(ui, u, &mut a.x));
                prop_row(ui, "A Y", |ui| num(ui, u, &mut a.y));
                prop_row(ui, "B X", |ui| num(ui, u, &mut b.x));
                prop_row(ui, "B Y", |ui| num(ui, u, &mut b.y));
                prop_row(ui, "Slot width", |ui| num(ui, u, width));
                // #1731: run the pass out past each end, along the slot.
                prop_row(ui, "Extend past A", |ui| num(ui, u, extend_start));
                prop_row(ui, "Extend past B", |ui| num(ui, u, extend_end));
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "slot.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "slot.top");
                height_editor(ui, "Bottom Z", world, op_ctx.as_deref(), bottom_z, "slot.bottom");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The engrave twin (M2.4 B): Trace's fields verbatim under the
        // catalog name - the arm is shared exactly because a second
        // pane body could only fork the equivalence.
        OpParams::Trace {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
        }
        | OpParams::Engrave {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
        } => {
            let tg = if engrave { "engrave" } else { "trace" };
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    &format!("{tg}.clearance"),
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "trace.top");
                height_editor(
                    ui,
                    "Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "trace.bottom",
                );
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "trace.retract");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The chamfer ride (M2.4 B): the edge is picked like Trace's
        // chains, the two authored numbers state the cone's line, and
        // the floor is DERIVED from them - so there is no bottom_z or
        // stepdown to edit (Passes is out by has_section) and the pane
        // reads the corner Z back so the derived number is visible,
        // never merely implied.
        OpParams::Chamfer2D {
            width,
            angle_deg,
            clearance_z,
            retract_z,
            top_z,
            feed,
            plunge_feed,
            climb,
            side,
            tolerance,
            tilt,
        } => {
            if tab == SEC_GEO {
                prop_row(ui, "Chamfer width", |ui| num(ui, u, width));
                prop_row(ui, "Chamfer angle", |ui| num_unit(ui, angle_deg, "°"));
                hint(ui, "the face's slope off the top plane; the depth follows as width x tan(angle) - authored here, never second-guessed by a depth field");
                let mut s_idx = match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                };
                prop_row(ui, "Side", |ui| combo(ui, "ch_side", &mut s_idx, side_names()));
                *side = match s_idx {
                    1 => Side::Inside,
                    2 => Side::Outside,
                    _ => Side::Auto,
                };
                if let Some(note) = &side_note {
                    consequence(ui, note);
                }
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "chamfer.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "chamfer.top");
                corner_note(ui, world, op_ctx.as_deref(), top_z, *width * angle_deg.to_radians().tan());
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "chamfer.retract");
                });
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                check_row(ui, "Climb milling", climb);
            }
        }
        // The deburr ride (M2.5 E B): the chamfer's word read on the
        // model's own edges - one authored number, the break width,
        // with the cone and its depth the TOOL's word (the door
        // refuses anything not a chamfer mill); heights are the
        // window the tip rides in, feeds the ride's. No pattern, no
        // stepover, no hop: the loci are what the weld exposes.
        OpParams::Deburr {
            width,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_GEO {
                prop_row(ui, "Break width", |ui| num(ui, u, width));
                hint(ui, "measured in the edge's own plane; the tip stands width / tan(half-angle) below it - the cone is the tool's, so its angle authors the depth, never a field here");
            }
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "deburr.clearance",
                );
                height_editor(ui, "Window Top Z", world, op_ctx.as_deref(), top_z, "deburr.top");
                height_editor(
                    ui,
                    "Window Bottom Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "deburr.bottom",
                );
                hint(ui, "the two planes the TIP rides between: an edge running outside the window is clipped along its own line, never dropped whole");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
            }
        }
        // The circular ladder (M2.4 B): the pocket family about an
        // AUTHORED centre - typed like Slot's endpoints, or fitted to a
        // round pick (0.8.0 hand test); heights and passes are the
        // pocket's shape, and `vault` stands the floor on the dome law
        // instead of at Bottom Z.
        OpParams::Circular {
            links,
            center,
            radius,
            vault,
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
            tilt,
        } => {
            if tab == SEC_LEADS {
                lead_page::leads_tab(ui, u, "circ", Some((entry, helix_radius)), links);
            }
            if tab == SEC_GEO {
                // Picks author the circle: echoed read-only, the typed
                // fields greyed but kept, so clearing the picks restores them.
                let typed = cx.circle_fit.is_none();
                match cx.circle_fit {
                    Some(Ok((c, r))) => hint(ui, &format!("From selection: Ø{:.3} at ({:.3}, {:.3})", 2.0 * r, c.x, c.y)),
                    Some(Err(e)) => hint(ui, e),
                    None => {}
                }
                ui.add_enabled_ui(typed, |ui| {
                    prop_row(ui, "Center X", |ui| num(ui, u, &mut center.x));
                    prop_row(ui, "Center Y", |ui| num(ui, u, &mut center.y));
                    prop_row(ui, "Radius", |ui| num(ui, u, radius));
                });
                check_row(ui, "Vault - dome floor", vault);
                hint(ui, "flat floors stand at Bottom Z; the vault sinks each ring by (r/R)^2 of the Top-to-Bottom span - a parabolic reflector, every ring a circle the spindle simply holds");
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "circ.clearance",
                );
                height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "circ.top");
                height_editor(ui, "Bottom Z", world, op_ctx.as_deref(), bottom_z, "circ.bottom");
                group(ui, LEVELS, None, |ui| {
                    retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "circ.retract");
                    prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                });
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
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

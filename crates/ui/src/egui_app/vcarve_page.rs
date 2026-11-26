//! The operation page: the V-Carve kind's fields (#1443) - the heights
//! (Top Z is the start depth, Bottom Z the maximum depth), the level
//! ladder, the flat clearing choice and the feeds. Each value edits the
//! page draft, and Apply rides the command history.
//!
//! Provenance: our own UI over the operation parameter model
//! (`operation::vcarve_op` names the kind's sources); no vendor material
//! or text.

use super::fields::*;

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::VCarve {
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown,
        feed,
        plunge_feed,
        flat_clear,
        flat_stepover,
        tilt, picks: _,
    } = params
    else {
        return;
    };
    if tab == SEC_GEO {
        hint(ui, "Pick closed outlines. The V-bit cuts inside them, as deep as each place is wide.");
    }
    if tab == SEC_HEIGHTS {
        orientation_editor(ui, u, tilt);
        height_editor(ui, "Clearance Z", world, op_ctx.as_deref(), clearance_z, "vcarve.clearance");
        height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "vcarve.top");
        height_editor(ui, "Bottom Z", world, op_ctx.as_deref(), bottom_z, "vcarve.bottom");
        hint(ui, "Top Z is the start depth. Bottom Z is the maximum depth of the tip.");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "vcarve.retract");
            prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
        });
    }
    if tab == SEC_PASSES {
        check_row(ui, "Flat clearing", flat_clear);
        if *flat_clear {
            prop_row(ui, "Floor stepover", |ui| num(ui, u, flat_stepover));
        }
        hint(ui, "Flat clearing cuts the floor where the region is wider than the cone at the maximum depth.");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
    }
}

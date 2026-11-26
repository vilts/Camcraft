//! The operation page: the Slot Undercut kind's fields (#1444) - the
//! centre line, the undercut width and the entry on Geometry, the
//! heights, the radial stepover on Passes and the feeds. Every value
//! edits the page draft; Apply rides the command history
//! (EditOperationParams, undoable).
//!
//! Provenance: our own UI over our own undercut parameters
//! (`machining2d::undercut`); no vendor material or text.

use super::fields::*;
use camcraft_app::operation::SlotUndercutOp;

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::SlotUndercut(SlotUndercutOp {
        a,
        b,
        bulge,
        width,
        run_in,
        neck_slot,
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepover,
        feed,
        plunge_feed,
        climb, picks: _,
    }) = params
    else {
        return;
    };
    if tab == SEC_GEO {
        prop_row(ui, "A X", |ui| num(ui, u, &mut a.x));
        prop_row(ui, "A Y", |ui| num(ui, u, &mut a.y));
        prop_row(ui, "B X", |ui| num(ui, u, &mut b.x));
        prop_row(ui, "B Y", |ui| num(ui, u, &mut b.y));
        prop_row(ui, "Bulge", |ui| num_unit(ui, bulge, ""));
        hint(ui, "0 is a straight slot; a curved slot is an arc from A to B: the bulge is the tangent of a quarter of its angle, positive counter-clockwise");
        prop_row(ui, "Undercut width", |ui| num(ui, u, width));
        hint(ui, "measured at the cutter's widest point: the rim of a T-slot or keyseat cutter, the tip corner of a dovetail, the equator of a lollipop");
        prop_row(ui, "Entry run-in", |ui| num(ui, u, run_in));
        hint(ui, "the cutter goes down and comes up this far before A, on the slot's line: place it outside the stock - a Z move with material over the head is refused");
        prop_row(ui, "Neck slot width", |ui| num(ui, u, neck_slot));
        hint(ui, "the slot cut earlier for the cutter's neck; 0 if nothing stands above the neck's path");
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, op_ctx.as_deref(), clearance_z, "undercut.clearance");
        height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "undercut.top");
        height_editor(ui, "Bottom Z", world, op_ctx.as_deref(), bottom_z, "undercut.bottom");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "undercut.retract");
        });
    }
    if tab == SEC_PASSES {
        prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
        hint(ui, "each pair of passes widens the undercut by this much, to the width");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Cutting feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

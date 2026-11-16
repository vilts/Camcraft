//! The operation page: the Trochoidal slot kind's fields (#220) - the
//! centre line and widths on Geometry, the heights, the loop numbers
//! (step, engagement limit, return, smoothing) on Passes and the three
//! feeds. Every value edits the page draft; Apply rides the command
//! history (EditOperationParams, undoable).
//!
//! Provenance: our own UI over our own trochoid parameters (Otkur &
//! Lazoglu 2007; Rauch, Duc & Hascoet 2009); no vendor material or text.

use super::fields::*;
use camcraft_app::machining2d::{DEFAULT_MICROLIFT, TrochoidReturn};

const RETURNS: [&str; 2] = ["Microlift", "Retract"];

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::TrochoidalSlot {
        a,
        b,
        width,
        loop_width,
        step,
        max_engagement,
        ret,
        smoothing,
        stock_to_leave,
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown,
        feed,
        return_feed,
        plunge_feed,
        climb,
    } = params
    else {
        return;
    };
    if tab == SEC_GEO {
        prop_row(ui, "A X", |ui| num(ui, u, &mut a.x));
        prop_row(ui, "A Y", |ui| num(ui, u, &mut a.y));
        prop_row(ui, "B X", |ui| num(ui, u, &mut b.x));
        prop_row(ui, "B Y", |ui| num(ui, u, &mut b.y));
        prop_row(ui, "Slot width", |ui| num(ui, u, width));
        let seed = (*width * 0.5).max(0.1);
        auto_opt(ui, "Loop width", "All the room", loop_width, "width less the tool", seed, |ui, v| num(ui, u, v));
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, op_ctx.as_deref(), clearance_z, "trochoid.clearance");
        height_editor(ui, "Top Z", world, op_ctx.as_deref(), top_z, "trochoid.top");
        height_editor(ui, "Bottom Z", world, op_ctx.as_deref(), bottom_z, "trochoid.bottom");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, op_ctx.as_deref(), retract_z, top_z, clearance_z, "trochoid.retract");
            prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
        });
    }
    if tab == SEC_PASSES {
        prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
        prop_row(ui, "Forward step", |ui| num(ui, u, step));
        prop_row(ui, "Max engagement", |ui| num_unit(ui, max_engagement, "deg"));
        // #1512: which of the two wins.
        hint(ui, "the forward step is the most a loop advances: where it would bite past Max engagement, the loops shorten to hold the limit and the path says so");
        let mut ri = usize::from(*ret == TrochoidReturn::Retract);
        prop_row(ui, "Return", |ui| combo(ui, "trochoid.return", &mut ri, &RETURNS));
        let lift = match *ret {
            TrochoidReturn::Microlift { height } => height,
            TrochoidReturn::Retract => DEFAULT_MICROLIFT,
        };
        *ret = if ri == 1 { TrochoidReturn::Retract } else { TrochoidReturn::Microlift { height: lift } };
        if let TrochoidReturn::Microlift { height } = ret {
            prop_row(ui, "Microlift height", |ui| num(ui, u, height));
        }
        prop_row(ui, "Smoothing radius", |ui| num(ui, u, smoothing));
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Cutting feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Return feed", |ui| num_feed(ui, u, return_feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

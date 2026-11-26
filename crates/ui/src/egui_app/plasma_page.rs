//! The operation page: the Plasma Profile's fields (#670) - the kerf,
//! compensation and cut direction on Geometry, the plate and the
//! controller's heights on Heights, the small-hole rule on Passes, the
//! leads on Lead in / Lead out, the feed and pierce delay on Feeds.
//! Every value edits the page draft; Apply rides the command history
//! (EditOperationParams, undoable). A blank (Auto) number is the
//! controller's: the CL carries none for it.
//!
//! Provenance: our own UI over our own plasma parameters (Hypertherm's
//! cut-direction guidance, the LinuxCNC Plasma Primer §13, Tormach's
//! 1300PL manual and PathPilot post guidelines); no vendor text.

use super::fields::*;
use camcraft_app::operation::{PlasmaComp, PlasmaOp};

const COMPS: [&str; 2] = ["In computer (offset path)", "In control (G41/G42)"];
const DIRS: [&str; 2] = ["Outer CW, holes CCW", "Reversed: outer CCW, holes CW"];

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::PlasmaProfile(p) = params else {
        return;
    };
    let PlasmaOp {
        clearance_z,
        top_z,
        bottom_z,
        kerf,
        comp,
        reversed,
        lead_in,
        lead_out,
        small_hole_dia,
        small_hole_feed,
        overburn,
        feed,
        pierce_height,
        cut_height,
        pierce_delay, picks: _,
    } = p;
    if tab == SEC_GEO {
        auto_opt(ui, "Kerf width", "Torch", kerf, "the torch's diameter", 1.5, |ui, v| num(ui, u, v));
        let mut ci = usize::from(*comp == PlasmaComp::Control);
        prop_row(ui, "Compensation", |ui| combo(ui, "plasma.comp", &mut ci, &COMPS));
        *comp = if ci == 1 { PlasmaComp::Control } else { PlasmaComp::Computer };
        let mut di = usize::from(*reversed);
        prop_row(ui, "Direction", |ui| combo(ui, "plasma.dir", &mut di, &DIRS));
        *reversed = di == 1;
        hint(ui, "pick the closed contours and open runs to cut; holes are cut before the outline that frees the part");
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance (torch off)", world, op_ctx.as_deref(), clearance_z, "plasma.clearance");
        height_editor(ui, "Top of plate", world, op_ctx.as_deref(), top_z, "plasma.top");
        height_editor(ui, "Bottom of plate", world, op_ctx.as_deref(), bottom_z, "plasma.bottom");
        auto_opt(ui, "Pierce height", "Controller", pierce_height, "set by the controller", 3.8, |ui, v| num(ui, u, v));
        auto_opt(ui, "Cut height", "Controller", cut_height, "set by the controller", 1.5, |ui, v| num(ui, u, v));
        hint(ui, "one through-cut at the plate top - no depth steps; blank heights are the controller's material table");
    }
    if tab == SEC_PASSES {
        prop_row(ui, "Small hole below", |ui| num(ui, u, small_hole_dia));
        let shown = *small_hole_feed * 100.0;
        let mut pct = shown;
        prop_row(ui, "Small hole feed", |ui| num_unit(ui, &mut pct, "%"));
        // Written back only on an edit: a percent round trip must never
        // dirty the draft by a rounding ulp.
        if pct != shown {
            *small_hole_feed = pct / 100.0;
        }
        prop_row(ui, "Overburn", |ui| num(ui, u, overburn));
        hint(ui, "holes under this diameter (or 1.5x the plate) lead in from the centre, take no lead-out and cut at this share of the feed");
    }
    if tab == SEC_LEADS {
        prop_row(ui, "Lead-in length", |ui| num(ui, u, lead_in));
        prop_row(ui, "Lead-out length", |ui| num(ui, u, lead_out));
        hint(ui, "arcs on the scrap side; under G41/G42 straight leads, each longer than half the kerf");
    }
    if tab == SEC_FEEDS {
        auto_opt(ui, "Cut feed", "AutoFS", feed, "the material table's", 2500.0, |ui, v| num_feed(ui, u, v));
        auto_opt(ui, "Pierce delay", "AutoFS", pierce_delay, "the material table's", 0.5, |ui, v| num_unit(ui, v, "s"));
    }
}

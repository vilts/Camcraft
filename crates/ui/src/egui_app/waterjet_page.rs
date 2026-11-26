//! The operation page: the Waterjet Profile's fields (#1883) - the
//! kerf, compensation, direction, open-run side and arc linearization
//! on Geometry, the sheet's heights on Heights, the tabs on Passes, the
//! leads on Lead in / Lead out, the cut feed and the pierce and abrasive
//! dwell on Feeds. Standoff output is unavailable; posting refuses waterjet. Every
//! value edits the page draft; Apply rides the command history
//! (EditOperationParams, undoable).
//!
//! Provenance: our own UI over our own waterjet parameters (general
//! abrasive-waterjet practice, RS-274 / ISO 6983); no vendor text.

use super::fields::*;
use camcraft_app::operation::{JetLead, JetSide, JetTabsOp, WaterjetOp};

const COMPS: [&str; 2] = ["In computer (offset path)", "In control"];
const DIRS: [&str; 2] = ["Outer CW, holes CCW", "Reversed: outer CCW, holes CW"];
const SIDES: [&str; 3] = ["On the centre line", "Left of travel", "Right of travel"];
const LEADS: [&str; 2] = ["Arc", "Straight"];

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::WaterjetProfile(p) = params else {
        return;
    };
    let WaterjetOp {
        clearance_z,
        top_z,
        bottom_z,
        kerf,
        control_comp,
        reversed,
        lead,
        lead_in,
        lead_out,
        open_side,
        tabs,
        feed,
        pierce_time,
        abrasive_delay,
        linearize, picks: _,
    } = p;
    if tab == SEC_GEO {
        auto_opt(ui, "Kerf width", "Nozzle", kerf, "the nozzle's diameter", 0.8, |ui, v| num(ui, u, v));
        let mut ci = usize::from(*control_comp);
        prop_row(ui, "Compensation", |ui| combo(ui, "waterjet.comp", &mut ci, &COMPS));
        *control_comp = ci == 1;
        let mut di = usize::from(*reversed);
        prop_row(ui, "Direction", |ui| combo(ui, "waterjet.dir", &mut di, &DIRS));
        *reversed = di == 1;
        let mut si = match open_side {
            JetSide::Center => 0,
            JetSide::Left => 1,
            JetSide::Right => 2,
        };
        prop_row(ui, "Open run side", |ui| combo(ui, "waterjet.side", &mut si, &SIDES));
        *open_side = match si {
            1 => JetSide::Left,
            2 => JetSide::Right,
            _ => JetSide::Center,
        };
        let mut chords = linearize.is_some();
        check_row(ui, "Send arcs as lines", &mut chords);
        if chords {
            prop_row(ui, "Line tolerance", |ui| num(ui, u, linearize.get_or_insert(0.01)));
        } else {
            *linearize = None;
        }
        hint(ui, "pick the closed contours and open runs to cut; holes are cut before the outline that frees the part");
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance (jet off)", world, op_ctx.as_deref(), clearance_z, "waterjet.clearance");
        height_editor(ui, "Top of sheet", world, op_ctx.as_deref(), top_z, "waterjet.top");
        height_editor(ui, "Bottom of sheet", world, op_ctx.as_deref(), bottom_z, "waterjet.bottom");
        hint(ui, "one through-cut at the sheet top - no depth steps");
    }
    if tab == SEC_PASSES {
        tab_fields(ui, u, tabs);
    }
    if tab == SEC_LEADS {
        let mut li = usize::from(*lead == JetLead::Straight);
        prop_row(ui, "Lead shape", |ui| combo(ui, "waterjet.lead", &mut li, &LEADS));
        *lead = if li == 1 { JetLead::Straight } else { JetLead::Arc };
        prop_row(ui, "Lead-in length", |ui| num(ui, u, lead_in));
        prop_row(ui, "Lead-out length", |ui| num(ui, u, lead_out));
        if *lead_in == 0.0 {
            consequence(ui, "zero lead-in can place a pierce at the part edge on the kerf ring");
        } else {
            hint(ui, "the pierce is made in the scrap; a lead with no room is shortened, and a tight hole is pierced at its centre");
        }
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Cut feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Pierce time", |ui| num_unit(ui, pierce_time, "s"));
        prop_row(ui, "Abrasive delay", |ui| num_unit(ui, abrasive_delay, "s"));
        hint(ui, "state the cut data for the material and the sheet thickness from the nozzle's recipe");
    }
}

/// The tabs: on or off, the width, and one world XY position per tab.
/// The count is the number of positions.
fn tab_fields(ui: &mut Ui, u: Unit, tabs: &mut Option<JetTabsOp>) {
    let mut on = tabs.is_some();
    check_row(ui, "Hold with tabs", &mut on);
    if !on {
        *tabs = None;
        return;
    }
    let t = tabs.get_or_insert_with(|| JetTabsOp { count: 1, width: 1.0, at: vec![[0.0, 0.0]] });
    prop_row(ui, "Tab width", |ui| num(ui, u, &mut t.width));
    let mut remove = None;
    for (i, xy) in t.at.iter_mut().enumerate() {
        prop_row(ui, &format!("Tab {} X", i + 1), |ui| num(ui, u, &mut xy[0]));
        prop_row(ui, &format!("Tab {} Y", i + 1), |ui| num(ui, u, &mut xy[1]));
        prop_row(ui, &format!("Tab {} position", i + 1), |ui| {
            if ui.button(format!("Remove tab position {}", i + 1)).clicked() {
                remove = Some(i);
            }
        });
    }
    // The last position stays: no tabs is "Hold with tabs" off.
    if let Some(i) = remove.filter(|_| t.at.len() > 1) {
        t.at.remove(i);
    }
    prop_row(ui, "Tab positions", |ui| {
        if ui.add_enabled(t.at.len() < 256, egui::Button::new("Add tab position")).clicked() {
            t.at.push([0.0, 0.0]);
        }
    });
    t.count = u32::try_from(t.at.len()).unwrap_or(u32::MAX);
    hint(ui, "each tab is an uncut gap of this width on the contour nearest its position; turn tabs off for none");
}

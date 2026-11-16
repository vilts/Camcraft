//! The operation page: the remaining turning kinds' fields: threading, chamfer, taper, multi-pass, form, boring - one module per kind family, split out
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
        // Threading (M5.2 D): the wave the model already carries, cut
        // one turn per pitch. The insert's V is the tool's own two
        // numbers (width and included angle) and never appears here -
        // the page names the thread, the tool names the cutter.
        OpParams::Threading {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            pitch,
            depth,
            infeed,
            feed,
            plunge_feed,
            infeed_plan,
            hand,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "threading.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "threading.top");
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "threading.bottom",
                );
                hint(ui, "the helix runs between these heights, one turn per pitch - the whole thread walks past the bar's floor in the model");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the helix settles at, and the play the verdict measures between insert and modeled flank");
                prop_row(ui, "Pitch", |ui| num(ui, u, pitch));
                hint(ui, "axis travel per turn - the thread's own number, and the lattice the passes walk along");
                prop_row(ui, "Depth", |ui| num(ui, u, depth));
                hint(ui, "radial depth of the groove, crest to root - the deepest rung stands this far under the turned skin");
                prop_row(ui, "Infeed", |ui| num(ui, u, infeed));
                hint(ui, "radial growth per pass of the growing wave; the last pass reaches full depth and the turns between them are the spring pass");
                super::thread_infeed_page::rows(ui, u, infeed_plan);
                // #629: the hand, by name. The draft takes the pick;
                // Apply rides EditOperationParams, so it undoes.
                // The row lays right to left: Left first reads "Right
                // hand  Left hand" on screen.
                prop_row(ui, "Hand", |ui| {
                    ui.radio_value(hand, camcraft_app::cl::ThreadHand::Left, "Left hand");
                    ui.radio_value(hand, camcraft_app::cl::ThreadHand::Right, "Right hand");
                });
                hint(ui, "the way the thread winds - every pass runs toward the chuck; right hand posts under M03, left hand under M04 with the insert mounted for reverse rotation");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "the synchronized helix cuts at this feed - the spindle turns once as the tool travels one pitch toward the chuck");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the lead-in arc swings into the first turn at this feed; every pass after the first rides the ladder in rapids");
            }
        }
        // Chamfering (M5.2 E): the ride is the profile finish's, so
        // the page names the window, the promise and the feeds - and
        // the angle is a PROMISE, spent at the door: the model must
        // already carry this cone within the tolerance, or the
        // generation says so by name.
        OpParams::Chamfering {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            angle_deg,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "chamfer.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "chamfer.top");
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "chamfer.bottom",
                );
                hint(ui, "the two stations the chamfered corner spans - the ride walks the face between them and nowhere outside");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "how far the model may stand off the promised cone before the op refuses - and the chord budget the ride settles at");
                prop_row(ui, "Angle from cylinder", |ui| num_unit(ui, angle_deg, "°"));
                hint(ui, "the slope the drawing states, measured off the cylinder - the MODEL owns which way the face falls, and its own line is what the tool rides");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "the nose tracks the modeled face at this feed");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the radial lead-in and slide-out at the window ends ride this feed");
            }
        }
        // Taper turning (M5.2 E): the diameters state the cone the
        // same way the angle does - a promise the model keeps or
        // the op names. Equal diameters are the straight turn.
        OpParams::TaperTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            bottom_dia,
            top_dia,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "taper.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "taper.top");
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "taper.bottom",
                );
                hint(ui, "the stations the taper runs between - the diameters stand at these planes");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "how far the model may stand off the authored cone before the op refuses - and the chord budget the ride settles at");
                prop_row(ui, "Bottom Diameter", |ui| num(ui, u, bottom_dia));
                hint(ui, "the cone's size at the window's low station");
                prop_row(ui, "Top Diameter", |ui| num(ui, u, top_dia));
                hint(ui, "and at its high one - equal diameters state the straight turn of a cylinder");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "the nose tracks the modeled cone at this feed");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the radial lead-in and slide-out at the window ends ride this feed");
            }
        }
        // Multi-pass turning (M5.2 E): the rough page and the finish
        // page's numbers in one program, plus the ride count - each
        // spring pass takes about half of what the last one
        // sprang, and the last of them lands ON the profile.
        OpParams::MultiPassTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            passes,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multipass.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "multipass.top");
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multipass.bottom",
                );
                hint(ui, "the length of bar the ladder eats and the rides finish, station window and all (M5.2 B's law, one program long)");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget every ride settles at, and the play the verdict measures");
                prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
                hint(ui, "radial shell each rung eats - never more than twice the nose, the ladder's own door");
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                hint(ui, "what the last rung stands off the skin for the finishing rides to share out");
                let mut n = f64::from(*passes);
                prop_row(ui, "Finishing passes", |ui| num_plain(ui, &mut n));
                *passes = n.clamp(1.0, 64.0).round().max(1.0) as u32;
                hint(ui, "spring passes after the rough - each removes about half of what the last sprang back; the last one is the profile finish itself");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "rung and ride both track the offset at this feed");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the radial plunge into each block's start, and the slide out of its end");
            }
        }
        // Form turning (M5.2 E2): the page names the window and the
        // budget and nothing else - there is no depth to author,
        // because the model states it, and no crown to author,
        // because the gauge is TOOL steel the tool page edits.
        OpParams::FormTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "form.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "form.top");
                height_editor(ui, "Bottom of Part Z", world, op_ctx.as_deref(), bottom_z, "form.bottom");
                hint(ui, "the stations the formed surfaces stand between - a valley outside the window is another op's promise");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "how far a floor may stand off the model's valley before the verdict speaks, and the lattice the door reads at");
                hint(ui, "the crown's arc and the band's width ride the mounted form gauge - the tool page states them");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "the spindle's pace, stated for the sheet - a gauge reads no distance off the skin");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the sink to each crown's depth is this kind's whole cut, and it rides this feed");
            }
        }
        // Turning boring (M5.2 F): the page names the window and the
        // budget and nothing else - the wall is in the model and the
        // size is TOOL steel, the ground nose the tool page edits.
        OpParams::BoringTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "bore.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "bore.top");
                height_editor(ui, "Bottom of Part Z", world, op_ctx.as_deref(), bottom_z, "bore.bottom");
                hint(ui, "the length of wall the bar rides - the bar enters along the axis, so the window must stand inside one open hole");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "how far the nose's ride may stand off the wall before the verdict speaks, and the chord budget the wall is sampled at");
                hint(ui, "the ground nose that holds the hole to size rides the mounted boring bar - the tool page states it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                hint(ui, "the wall is walked tangentially, station by station, at this feed");
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the bar's lead-out: the radial move from the axis onto the wall, and the slide back off it");
            }
        }
        _ => {}
    }
}

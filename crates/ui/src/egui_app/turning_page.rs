//! The operation page: the turning kinds' fields: facing, profiles, grooves, part-off - one module per kind family, split out
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
        // Turning face (M5.2 A): the ladder runs at axial stations, so
        // the heights trio is clearance / stock top / the plane to face
        // - the words match the generator's refusals, one truth. The
        // passes tab carries only the radial stepdown (the ridge law:
        // the kernel refuses past twice the insert nose) and the
        // tolerance; there is no stepover to show - the passes spiral
        // through the axis by construction - and no climb: a lathe cut
        // runs one way.
        OpParams::Facing {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "facing.clearance",
                );
                height_editor(ui, "Top of Part Z", world, op_ctx.as_deref(), top_z, "facing.top");
                height_editor(ui, "Facing Plane Z", world, op_ctx.as_deref(), bottom_z, "facing.face");
                hint(ui, "the bar faces the spindle: stock is read between the facing plane and the part top; the ladder steps down from the top, one stepdown per rung");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
                hint(ui, "axial infeed per rung - the kernel refuses past twice the insert nose radius, the ridge it would leave");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the verify budget: the swept envelope is judged against the model within it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the descent between rungs travels in air at the plunge feed");
            }
        }
        // Profile turning (M5.2 B): the window trio names the bar
        // the rungs ride, not a plane they face toward; the passes
        // tab carries the RADIAL stepdown (the same ridge law read
        // as a shell width - twice the insert nose at the most) and
        // the leave the finish takes off. No stepover, no climb:
        // the ride IS the pattern and the work spins one way.
        OpParams::ProfileRough {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            feed,
            plunge_feed, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "profile_rough.clearance",
                );
                height_editor(
                    ui,
                    "Top of Part Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "profile_rough.top",
                );
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "profile_rough.bottom",
                );
                hint(ui, "the rungs ride the bar between these heights; the window's ends are where the tool plunges in and slides out");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
                hint(ui, "radial shell one rung eats - the kernel refuses past twice the insert nose radius, the sheet the next rung in would miss");
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                hint(ui, "the wall the last rung stands off the skin - the finish pass takes it off");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the silhouette and each rung's ride settle at, and the envelope the verdict measures - one number, both sides");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the radial lead onto each rung feeds at the plunge number; along the rung, at the feed");
            }
        }
        OpParams::ProfileFinish {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "profile_finish.clearance",
                );
                height_editor(
                    ui,
                    "Top of Part Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "profile_finish.top",
                );
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "profile_finish.bottom",
                );
                hint(ui, "one ride of the nose along the bar's offset, bottom of the window up to top - the whole finish is one pass");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the ride settles at, and the envelope the verdict measures - the surface left stands within it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the radial lead in and slide out feed at the plunge number; along the profile, at the feed");
            }
        }
        // Grooving rough (M5.2 C): the blade eats a BAND, so the
        // heights are the window's axial ends - between them the
        // kerfs tile one blade width apart, floor depth read off
        // the model's own silhouette. No stepover: the tile step IS
        // the width. No climb: the work spins one way.
        OpParams::GrooveRough {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            feed: _,
            plunge_feed, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "groove_rough.clearance",
                );
                height_editor(
                    ui,
                    "Top of Part Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "groove_rough.top",
                );
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "groove_rough.bottom",
                );
                hint(ui, "the band's axial ends: the kerfs tile between these heights, one blade width apart, and the window must be at least a blade wide");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
                hint(ui, "radial infeed per rung - each center's ladder steps from the window's shell down to the leave line under it");
                prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
                hint(ui, "the skin the floor stands off - the finish groove takes it off");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the band settles at, and the meridian the verdict measures the swept blade against");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the blade's only feed is radial, at this number; the step from center to center and every retract travel rapid at the bar's widest radius - the Feed the parameter carries is inert here, so the page says nothing with it (the tool page's law, M5.2 C)");
            }
        }
        // Grooving finish (M5.2 C): one full-depth plunge per center,
        // leave-free - the band walked once, at depth.
        OpParams::GrooveFinish {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed: _,
            plunge_feed, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "groove_finish.clearance",
                );
                height_editor(
                    ui,
                    "Top of Part Z",
                    world,
                    op_ctx.as_deref(),
                    top_z,
                    "groove_finish.top",
                );
                height_editor(
                    ui,
                    "Bottom of Part Z",
                    world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "groove_finish.bottom",
                );
                hint(ui, "one plunge per center between these heights, to the model's own floor - the whole finish is one walk of the band");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the band settles at, and the meridian the verdict measures - the groove left stands within it");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "one full-depth plunge at this feed per center, to the model's floor - everything else on this path is a rapid");
            }
        }
        // Parting (M5.2 C): one center, the plane the part is cut
        // off at. The kerf's column comes from the model - the
        // ladder falls through the bar's full height and the last
        // rung stands ON the axis - so the page names just the
        // plane and the clearance; the Top of Part the parameter
        // carries shapes nothing here and stays unshown.
        OpParams::Part {
            tolerance,
            clearance_z,
            stepdown,
            feed: _,
            plunge_feed,
            bottom_z,
            top_z: _,
            bar_load,
            bar_advance,
            bar_unload,
            bar_transfer,
            inspect_tol,
            kerf, picks: _,
        } => {
            if tab == SEC_HEIGHTS {
                height_editor(
                    ui,
                    "Clearance Z",
                    world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "part.clearance",
                );
                height_editor(ui, "Parting Plane Z", world, op_ctx.as_deref(), bottom_z, "part.plane");
                hint(ui, "where a part gets cut off from the bar it was turned on: the blade stands one center deep at this plane and its kerf falls through the model's whole column");
                group(ui, LEVELS, None, |ui| prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown)));
                hint(ui, "radial infeed per rung of the ladder down through the bar - the last rung severs, tip on the axis");
            }
            if tab == SEC_PASSES {
                prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
                hint(ui, "the chord budget the kerf band settles at; through-axis cuts are the cut, not a gouge - the verdict reads the separation");
                // #3469: the piece kept and the back stock, by name. The
                // row lays right to left: Chuck first reads "Cut-off
                // piece  Chuck piece" on screen.
                prop_row(ui, "The part is", |ui| {
                    radio_pair(
                        ui,
                        &mut kerf.kept,
                        (camcraft_app::turning::PartKept::Chuck, "Chuck piece"),
                        (camcraft_app::turning::PartKept::CutOff, "Cut-off piece"),
                    );
                });
                hint(ui, "the parting plane is the part's end face, and the whole kerf goes on the other side: Cut-off piece keeps what the blade cuts off the free end; Chuck piece keeps what stays in the chuck and cuts a stub off the free end (a part held end for end)");
                prop_row(ui, "Back stock", |ui| num(ui, u, &mut kerf.back_stock));
                hint(ui, "how far the whole kerf stands off the parting plane, so the part keeps this much on its end face for a later facing pass; 0 puts the blade's flank on the plane");
            }
            if tab == SEC_FEEDS {
                prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
                hint(ui, "the ladder down the column plunges at this feed; every retract off the kerf is a rapid back to the clear radius at the same station");
            }
            // The bar feeder's signals (M5.2 G.2): they shape no
            // toolpath and enter no sim - they are what the POSTED
            // program says around the cut. One program cuts one
            // part, so the honest frame is load, then cut, then
            // unload; the advance is the bar's next bite, measured
            // from wherever the stub left the axis.
            if tab == SEC_MACHINE {
                check_row(ui, "Load bar before the cut", bar_load);
                if *bar_load {
                    auto_value(ui, "Bar advance", "Feeder stop", bar_advance, "no measured advance", 10.0, |ui, v| {
                        num(ui, u, v)
                    });
                    hint(ui, "how far the feeder pushes the bar past the previous part's stub - Feeder stop posts the act alone and lets the feeder index by its own stops");
                }
                check_row(ui, "Unload part after the cut", bar_unload);
                hint(ui, "the signals post as stream events around this section - a script voices them for the machine, M-code or comment, and silence posts silence");
                check_row(ui, "Transfer to sub-spindle", bar_transfer);
                hint(ui, "the handoff step (M5.3 C): after the last cut, the part moves to the machine's second spindle - check the program gate, it names a machine that states no second spindle");
                auto_value(ui, "Probe budget", "Off", inspect_tol, "no probe", 0.02, |ui, v| num(ui, u, v));
                hint(ui, "the measurement ask (M5.3 D): Off asks no probe; with a budget the program posts its PROBE intent and the post measures the part as machined - every blade's cut composed into one - against this bar; what stands above the model past it holds the post and names the pass to re-run (0.1 um is the least any probe measures)");
            }
        }
        _ => {}
    }
}

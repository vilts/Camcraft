//! The turning arms of `OpParams::stamp_with` (#1849 split): the lathe
//! kinds (facing, profile, groove, part, threading and the contour
//! rides). Provenance: moved verbatim from `params.rs`; our own digest
//! law.

use super::*;
use super::digest::{self, w};

impl OpParams {
    /// The digest bytes of the lathe kinds (facing, profile, groove,
    /// part, threading and the contour rides); `stamp_with` dispatches
    /// here exhaustively, so any other kind never reaches this match.
    pub(super) fn stamp_turning(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        match self {
            // The facing ladder (M5.2 stage A, tag 18): three heights,
            // a radial infeed and two feeds - the whole authored shape.
            // The tolerance rides like the rotary twins' (the verify
            // budget a stamped path was measured under must not
            // survive a budget edit), and the axis the stations ride
            // is model data: it moves the stamp through the source
            // digest, never through a number here.
            OpParams::Facing {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
            // The profile kinds (M5.2 stage B, tags 19, 20): the same
            // law, and here the tolerance is load-bearing TWICE -
            // the kernel chords its own silhouette and ride at it,
            // so a budget edit moves the path, not just the verdict.
            OpParams::ProfileRough {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                stock_to_leave,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *stock_to_leave, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
            OpParams::ProfileFinish {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
            }
            // The blade kinds (M5.2 stage C, tags 21, 22, 23): the
            // profile law again, tolerance load-bearing twice (the
            // silhouette chords at it, the meridian verdict gates at
            // it). The blade's width rides the tool digest, not a
            // number here - a re-ground blade is a different cut,
            // and the stamp says so through the TOOL row.
            OpParams::GrooveRough {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                stock_to_leave,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *stock_to_leave, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
            OpParams::GrooveFinish {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
            }
            OpParams::Part {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                bar_load,
                bar_advance,
                bar_unload,
                bar_transfer,
                inspect_tol,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                // The feeder signals change the POSTED program, so
                // they dirty the stamp like every authored number -
                // even though no geometry reads them (M5.2 G.2). The
                // handoff step rides the same law (M5.3 C): a fold
                // added to the digest reads ops Stale once, the
                // stamp-salt precedent says working as designed.
                w(h, if *bar_load { 1.0 } else { 0.0 });
                w(h, *bar_advance);
                w(h, if *bar_unload { 1.0 } else { 0.0 });
                w(h, if *bar_transfer { 1.0 } else { 0.0 });
                // The measurement ask rides the same fold (M5.3 D):
                // it changes the posted stream, no geometry reads it.
                w(h, *inspect_tol);
            }
            // The threading ladder (M5.2 stage D, tag 24): the wave's
            // own three numbers ride beside the twins' - pitch, depth
            // and infeed all MOVE the path, the first by naming the
            // lattice, the second and third by what stands between
            // crest and root. The insert's V digests through the
            // TOOL row, the blade's law again.
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
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*pitch, *depth, *infeed, *feed, *plunge_feed] {
                    w(h, v);
                }
                // The infeed plan (#416) moves every pass - bytes only
                // when it differs from the default, so every earlier
                // thread keeps its digest (the approach precedent).
                if *infeed_plan != crate::turning::ThreadInfeed::default() {
                    h.write_u8(0xD4);
                    h.write_u8(infeed_plan.method as u8);
                    h.write_u8(infeed_plan.schedule as u8);
                    h.write_u32(infeed_plan.spring_passes);
                    // #418: a byte only when set, so a #416 plan's
                    // digest is unchanged.
                    if infeed_plan.repeat_each_depth {
                        h.write_u8(0xD5);
                    }
                    // #419: the dips' bytes only when they lay any, so
                    // every earlier plan's digest is unchanged.
                    if infeed_plan.dips.is_on() {
                        h.write_u8(0xD6);
                        for v in [infeed_plan.dips.interval, infeed_plan.dips.height, infeed_plan.dips.length] {
                            w(h, v);
                        }
                    }
                    for v in [infeed_plan.flank_angle, infeed_plan.min_depth, infeed_plan.finish_allowance] {
                        w(h, v);
                    }
                }
                // #629: the hand sets the helix's sense, and the passes
                // now run toward the chuck - written for BOTH hands, so
                // every thread cached before the fix goes stale once and
                // regenerates the new way, saying so.
                h.write_u8(match hand {
                    crate::cl::ThreadHand::Right => 0xA1,
                    crate::cl::ThreadHand::Left => 0xA2,
                });
            }
            // The contour rides (M5.2 stage E, tags 25-27): each
            // promise-number MOVES the verdict's window or the ride
            // the door judges - the chamfer's angle re-states which
            // line the skin is measured against, the taper's
            // diameters both the line and its stations' reach, the
            // ladder's numbers and ride count the very blocks the
            // program walks. The nose spends itself through the
            // TOOL row, the profile kinds' law again.
            OpParams::Chamfering {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                angle_deg,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*angle_deg, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
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
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*bottom_dia, *top_dia, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
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
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                w(h, *stepdown);
                w(h, *stock_to_leave);
                w(h, (*passes) as f64);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
            }
            // The crown gauge (M5.2 stage E2, tag 28): no number
            // here shapes the cut's DEPTH - the model states it -
            // so what moves is the window the valleys stand in and
            // the verdict's budget. The crown and the band digest
            // through the TOOL row, the blade's law again: a
            // re-ground gauge makes this op Stale on its own.
            OpParams::FormTurning {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
            }
            // The boring bar (M5.2 stage F, tag 29): the same
            // shape - the wall is in the model and the nose is
            // TOOL steel, so what moves here is the window the
            // ride walks and the verdict's budget; a re-ground
            // bar makes this op Stale through the global nose row
            // on its own.
            OpParams::BoringTurning {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
            }
            _ => {}
        }
    }
}

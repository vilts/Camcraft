//! The hole-list arms of `OpParams::stamp_with` (#1849 split): the
//! hole-list kinds (drill, bore, probe, thread). Provenance: moved
//! verbatim from `params.rs`; our own digest law.

use super::*;
use super::digest::{self, w};

impl OpParams {
    /// The digest bytes of the hole-list kinds (drill, bore, probe,
    /// thread); `stamp_with` dispatches here exhaustively, so any other
    /// kind never reaches this match.
    pub(super) fn stamp_holes(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| digest::wh_opt(world, setup, src, h, v);
        match self {
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
                break_through,
                break_through_from, picks: _
            } => {
                // #1557: the break-through, written only when stated,
                // so every older op keeps the digest it was cached under.
                if let Some(b) = break_through {
                    h.write_u8(0xB7);
                    w(h, *b);
                }
                // #2441: legacy `Tip` keeps the pre-v68 digest unchanged;
                // `FullDiameter` writes `0xB8` so switching invalidates cache.
                if *break_through_from == crate::drilling::BreakThroughFrom::FullDiameter {
                    h.write_u8(0xB8);
                }
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh_opt(h, *bottom_z);
                wh_opt(h, *retract_z);
                for v in [*feed, *plunge_feed, *peck, *dwell] {
                    w(h, v);
                }
                h.write_u8(match cycle {
                    CycleChoice::Auto => 0,
                    CycleChoice::Spot => 1,
                    CycleChoice::Drill => 2,
                    CycleChoice::Peck => 3,
                    CycleChoice::Tap => 4,
                    CycleChoice::FloatingTap => 7,
                    CycleChoice::Bore => 5,
                    CycleChoice::BackBore => 6,
                    CycleChoice::Ream => 8,
                    CycleChoice::FineBore { .. } => 9,
                });
                // #1445: the fine-bore shift is params data; only a fine
                // bore writes it, so every other op keeps its digest.
                if let CycleChoice::FineBore { shift_x_um, shift_y_um } = cycle {
                    h.write_i32(*shift_x_um);
                    h.write_i32(*shift_y_um);
                }
                // The hole list is params data, so it digests: recognition
                // output, a typed edit, or a moved model (whose re-
                // detect changes the numbers) all mark the op Stale
                // through here or through the source arm.
                h.write_u32(holes.len() as u32);
                for hole in holes {
                    // #1560: in the form the file stores (lattice).
                    super::hole_digest::hole(h, hole);
                }
                // The radial list digests too (M5.2 F2): same law -
                // detection output or a typed edit is params data, and
                // a turned model that grows a cross hole must mark
                // the op Stale rather than re-detect silently.
                h.write_u32(cross.len() as u32);
                for c in cross {
                    super::hole_digest::cross(h, c);
                }
            }
            // The two hole-list rides (M2.4 C): the drill's digest
            // law verbatim - the holes are params data, so they
            // digest, and the Thread's pitch needs no word here
            // because it is the TOOL's: the tool row carries it.
            OpParams::Bore {
                links: _,
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
                break_through,
                pass_start, picks: _
            } => {
                // #1557: the break-through, written only when stated,
                // so every older op keeps the digest it was cached under.
                if let Some(b) = break_through {
                    h.write_u8(0xB7);
                    w(h, *b);
                }
                // #3360: written only off Auto, the #3316 bytes.
                match pass_start {
                    crate::machining2d::PassStart::Auto => {}
                    crate::machining2d::PassStart::Inside => h.write_u8(0xC1),
                    crate::machining2d::PassStart::Outside => h.write_u8(0xC2),
                }
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh_opt(h, *bottom_z);
                wh_opt(h, *retract_z);
                for v in [*stock_to_leave, *stepdown, *feed, *plunge_feed, *tolerance] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u32(holes.len() as u32);
                for hole in holes {
                    // #1560: in the form the file stores (lattice).
                    super::hole_digest::hole(h, hole);
                }
            }
            // The probe (#108): every number moves the touches or the
            // verdict, so every number digests - the modes and the
            // directions by their stable ordinals.
            OpParams::Probe {
                clearance_z,
                mode,
                dir,
                dir_y,
                target,
                radius,
                approach,
                overtravel,
                backoff,
                fast_feed,
                slow_feed,
                depth,
                inset,
                set_wcs,
                tolerance,
            } => {
                wh(h, *clearance_z);
                h.write_u8(*mode as u8);
                h.write_u8(*dir as u8);
                h.write_u8(*dir_y as u8);
                for v in [
                    target[0], target[1], target[2], *radius, *approach, *overtravel, *backoff,
                    *fast_feed, *slow_feed, *depth, *inset, *tolerance,
                ] {
                    w(h, v);
                }
                h.write_u8(set_wcs.unwrap_or(0xff));
            }
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
                break_through, picks: _
            } => {
                // #1557: the break-through, written only when stated,
                // so every older op keeps the digest it was cached under.
                if let Some(b) = break_through {
                    h.write_u8(0xB7);
                    w(h, *b);
                }
                // #985: an external thread is a different ride on the
                // same list. Written only when set, so every internal
                // thread keeps the digest it was cached under.
                if *external {
                    h.write_u8(0xE7);
                }
                // #988: the pass control, written only when it differs
                // from the #894 ride, so no older thread goes Stale.
                if *passes != crate::machining2d::ThreadPasses::default() {
                    h.write_u8(0xE8);
                    h.write_u8(match passes.schedule {
                        crate::machining2d::PassSchedule::Equal => 0,
                        crate::machining2d::PassSchedule::ConstantArea => 1,
                    });
                    w(h, passes.finish);
                    h.write_u32(passes.spring);
                    w(h, passes.allowance);
                    h.write_u8(u8::from(passes.spindle_reverse));
                    h.write_u8(u8::from(passes.cutter_left));
                }
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh_opt(h, *bottom_z);
                wh_opt(h, *retract_z);
                for v in [*infeed, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                // #894: the thread's own pitch and hand. Written for
                // every Thread op, so each one made before the rework
                // reads Stale once - its path is a different ride now.
                w(h, pitch.unwrap_or(0.0));
                h.write_u8(match hand {
                    crate::cl::ThreadHand::Right => 0xA1,
                    crate::cl::ThreadHand::Left => 0xA2,
                });
                h.write_u32(holes.len() as u32);
                for hole in holes {
                    // #1560: in the form the file stores (lattice).
                    super::hole_digest::hole(h, hole);
                }
            }
            _ => {}
        }
    }
}

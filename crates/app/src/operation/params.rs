//! The digest of an operation's parameters (`OpParams::stamp_with`).
//! The pocket family's arms live here; the other families' arms and the
//! shared writers live in the child modules (#1849 split).
//! Provenance: our own digest law over this repo's operation registry.

use super::*;
use digest::{w, wt};

impl OpParams {
    /// The digest-relevant scalars, written to a hasher in a fixed
    /// order. A height contributes its reference (tag, plus slot for a
    /// work offset) and its offset bits; a non-Fixed reference ALSO
    /// contributes its resolved Z (or an unresolved marker). The source
    /// digests in `current_stamp` only cover the operation's own source
    /// entity - a stock-sourced Face with a `TopOfModel` cut height must
    /// go Stale when a model moves, and nothing else feeds that world in.
    /// Resolved Zs are not double-counted for Fixed heights: their
    /// offset bits are the whole truth. The Selection references (M11.3
    /// A) take the appended tags 6/7 and resolve against `src`, the op's
    /// own source: a selection change moves the resolved Z, so the op
    /// goes Stale; a params set that never reads the selection writes
    /// exactly its old bytes.
    pub(super) fn stamp_with(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| digest::wh_opt(world, setup, src, h, v);
        // The kind tag opens the digest (M5.0.2): what was a magic
        // literal at the head of each arm is now the registry row's
        // data - the bytes written are unchanged.
        h.write_u8(self.kind().info().tag);
        match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                blend_turns,
                holes,
                tilt,
            } => {
                w(h, *stepover);
                wh(h, *clearance_z);
                wh(h, *cut_z);
                w(h, *feed);
                w(h, *plunge_feed);
                h.write_u8(u8::from(*climb));
                // Appended at v5-era staging: presence first, then the
                // value. Even a legacy (None, None) face digest differs
                // from a pre-stamp face op - the mechanism saying so.
                wh_opt(h, *top_z);
                match *stepdown {
                    Some(v) => {
                        h.write_u8(1);
                        w(h, v);
                    }
                    None => h.write_u8(0),
                }
                wt(h, *tilt);
                // M11.1 A: the blends change the moves, so they change
                // the digest - written only when ON, so every legacy
                // (sharp) face keeps the digest it was cached under.
                if *blend_turns {
                    h.write_u8(0xB1);
                }
                // M13.11: the hole law changes the moves only away from
                // its default, and is written only then - every legacy
                // (lift) face keeps the digest it was cached under.
                match holes {
                    crate::machining2d::FaceHoles::Lift => {}
                    crate::machining2d::FaceHoles::CutAir => h.write_u8(0xB2),
                    crate::machining2d::FaceHoles::Through => h.write_u8(0xB3),
                }
            }
            OpParams::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                lead,
                side,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                climb,
                entry,
                helix_radius,
                blend_turns,
                links: _,
                tilt,
            } => {
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *retract_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *tolerance, *tab_width, *tab_height] {
                    w(h, v);
                }
                h.write_u32(*tab_count);
                h.write_u8(u8::from(*climb));
                match lead {
                    Lead::None => h.write_u8(0),
                    Lead::Linear { length } => {
                        h.write_u8(1);
                        w(h, *length);
                    }
                    Lead::Arc { radius } => {
                        h.write_u8(2);
                        w(h, *radius);
                    }
                    Lead::Tangential { radius } => {
                        h.write_u8(3);
                        w(h, *radius);
                    }
                    // M11.3 C: an appended tag - no earlier op holds it.
                    Lead::Perpendicular { length } => {
                        h.write_u8(4);
                        w(h, *length);
                    }
                }
                // Appended at the v7-era side field: even a pre-Side
                // contour op digests differently - the mechanism saying
                // so, so nothing quietly reinterprets a committed path
                // under the new Auto law.
                h.write_u8(match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                });
                // Appended at M8.2 A's entry field, the pocket doors'
                // word: a pre-entry contour op goes Stale once and
                // regenerates byte-identically on the Outside side (the
                // Plunge default IS that law); on a bore the second
                // generation is the interior dive - the fix, saying so
                // through the same mechanism.
                w(h, *helix_radius);
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                wt(h, *tilt);
                // M11.1 B: written only when ON (A's word), so every
                // legacy sharp contour keeps its cached digest.
                if *blend_turns {
                    h.write_u8(0xB1);
                }
                // The level links ride the shared tail below (M11.2).
            }
            OpParams::Pocket {
                blend_radius,
                links: _,
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
                strategy,
                entry,
                helix_radius,
                optimal_load,
                blend_turns,
                tilt,
            } => {
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                // M5.1 H: the entry family's numbers change the posted
                // ladder exactly like the 3-D pocket's do, so they
                // digest the same way (and pre-H ops go Stale once and
                // regenerate byte-identically - the mechanism saying
                // so, as it did for `rest_rough` and `feed_opt`).
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius, *optimal_load] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    // M11.4 C: the HSM plan is a new generation of the
                    // word (old Adaptive ops go Stale once); the rings
                    // keep the byte they always digested under.
                    PocketStrategy::Adaptive => 4,
                    PocketStrategy::OffsetRings => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                // v5-era hop plane: presence, then the value (same shape
                // as Face's staging fields appended above).
                wh_opt(h, *retract_z);
                wt(h, *tilt);
                // M11.1 C: written only when ON, as for Face/Contour.
                if *blend_turns {
                    h.write_u8(0xB1);
                    // #1733: a set radius only, so every earlier
                    // blended pocket keeps its digest.
                    if let Some(r) = blend_radius {
                        h.write_u8(0xB2);
                        w(h, *r);
                    }
                }
            }
            // The roughing twins (M2.4 A): the pocket's digest minus
            // the strategy byte - the kind tag at the head already
            // pins which ladder runs, and a field that cannot move
            // has nothing to say about staleness.
            OpParams::Adaptive2D {
                blend_turns,
                blend_radius,
                links: _,
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
                optimal_load,
                method,
                troch,
                tilt,
            } => {
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius, *optimal_load] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                wh_opt(h, *retract_z);
                wt(h, *tilt);
                // M11.4 C: the kind now runs the HSM plan, not rings -
                // a generation word so every pre-C op goes Stale once.
                h.write_u8(0xA4);
                // #1509: written only off the default, so every Spiral
                // op keeps the digest it had before the choice existed.
                if *method == crate::machining2d::AdaptiveMethod::Trochoidal {
                    h.write_u8(0xA5);
                    if *troch != crate::machining2d::AdaptiveTrochParams::default() {
                        h.write_u8(0xA6);
                        w(h, troch.max_engagement);
                        match troch.ret {
                            None => h.write_u8(0),
                            Some(crate::machining2d::TrochoidReturn::Microlift { height }) => {
                                h.write_u8(1);
                                w(h, height);
                            }
                            Some(crate::machining2d::TrochoidReturn::Retract) => h.write_u8(2),
                        }
                        w(h, troch.return_feed.unwrap_or(-1.0));
                        w(h, troch.loop_width.unwrap_or(-1.0));
                    }
                }
                // #1733: written only when ON, as for the pocket.
                if *blend_turns {
                    h.write_u8(0xB1);
                    // #1733: a set radius only, so every earlier
                    // blended pocket keeps its digest.
                    if let Some(r) = blend_radius {
                        h.write_u8(0xB2);
                        w(h, *r);
                    }
                }
            }
            OpParams::Offset2D { .. }
            | OpParams::Slot { .. }
            | OpParams::TrochoidalSlot { .. }
            | OpParams::Trace { .. }
            | OpParams::Engrave { .. }
            | OpParams::Chamfer2D { .. }
            | OpParams::Circular { .. } => self.stamp_mill2d(world, setup, src, h),
            OpParams::Pocket3D { .. }
            | OpParams::Finish3D { .. }
            | OpParams::Deburr { .. } => self.stamp_mill3d(world, setup, src, h),
            OpParams::Contour5D { .. }
            | OpParams::Parallel5D { .. }
            | OpParams::Swarf { .. }
            | OpParams::RotaryContour { .. }
            | OpParams::RotaryParallel { .. }
            | OpParams::Flow5D { .. }
            | OpParams::MultiAxisContour { .. }
            | OpParams::RotaryOffset { .. }
            | OpParams::MultiAxisRough { .. }
            | OpParams::AdvancedSwarf { .. } => self.stamp_multiaxis(world, setup, src, h),
            OpParams::Facing { .. }
            | OpParams::ProfileRough { .. }
            | OpParams::ProfileFinish { .. }
            | OpParams::GrooveRough { .. }
            | OpParams::GrooveFinish { .. }
            | OpParams::Part { .. }
            | OpParams::Threading { .. }
            | OpParams::Chamfering { .. }
            | OpParams::TaperTurning { .. }
            | OpParams::MultiPassTurning { .. }
            | OpParams::FormTurning { .. }
            | OpParams::BoringTurning { .. } => self.stamp_turning(world, setup, src, h),
            OpParams::Drill { .. }
            | OpParams::Bore { .. }
            | OpParams::Probe { .. }
            | OpParams::Thread { .. } => self.stamp_holes(world, setup, src, h),
            OpParams::WireContour { .. }
            | OpParams::SinkerBurn { .. }
            | OpParams::SinkerSideBurn { .. }
            | OpParams::PlasmaProfile { .. } => self.stamp_cutting(world, setup, src, h),
        }
        self.stamp_links(h);
    }
}

mod digest;
mod mill2d;
mod mill3d;
mod multiaxis;
mod turning;
mod holes;
mod cutting;
#[cfg(test)]
mod size_tests;

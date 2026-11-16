//! The multi-axis arms of `OpParams::stamp_with` (#1849 split): the
//! 5-axis, rotary and swarf kinds. Provenance: moved verbatim from
//! `params.rs`; our own digest law.

use super::*;
use super::digest::{self, w};

impl OpParams {
    /// The digest bytes of the 5-axis, rotary and swarf kinds;
    /// `stamp_with` dispatches here exhaustively, so any other kind never
    /// reaches this match.
    pub(super) fn stamp_multiaxis(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| digest::wh_opt(world, setup, src, h, v);
        match self {
            // The 5-axis twins and plain swarf (tags 8/9/10): the
            // finish's scalar law with the pattern byte dropped - the
            // tag already names the kinematic - and the axis trio
            // appended, because leaning the spindle rewrites every
            // pose on the path. The raster kinds' optional stepover
            // rides the same Some/None split as the finish's: a
            // derived spacing must never digest as the number 0.
            OpParams::Contour5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::Parallel5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::Swarf {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The rotary twins (M5.1 stage E, tags 12/13): the same
            // door's bytes - the axis the fit reads is model data,
            // not a parameter, so it enters through the facets' own
            // stamp. Only the stepover split keeps them apart.
            | OpParams::RotaryContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
            }
            | OpParams::RotaryParallel {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // Flow (M5.1 I, tag 16) is the parallel's numbers whole,
            // the field read off the model and not a parameter: the
            // byte shape is the raster twins', one tag apart.
            | OpParams::Flow5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The drive-curve contour (M5.1 J, tag 17) is flow's byte
            // shape with the stepover absent - it binds no stepover
            // field, so the read-back below simply skips it; the tag
            // alone separates the two digests. What is NOT a parameter
            // here is the path itself: the picked chains enter through
            // op.src's own digest, the selection's law.
            | OpParams::MultiAxisContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                // The or-pattern cannot bind the optional stepover
                // (E0642: the contour has none) - read it back below.
                w(h, *tolerance);
                if let OpParams::Parallel5D { stepover, .. }
                | OpParams::Swarf { stepover, .. }
                | OpParams::RotaryParallel { stepover, .. }
                | OpParams::Flow5D { stepover, .. } = self
                {
                    match stepover {
                        Some(s) => {
                            h.write_u8(1);
                            w(h, *s);
                        }
                        None => h.write_u8(0),
                    }
                }
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                for v in [*lead_deg, *lag_deg, *max_tilt_deg] {
                    w(h, v);
                }
                // The or-pattern cannot bind stage F's flag (the
                // swarf and rotary twins answer shank contact with
                // surface truth, not a free axis - it is not their
                // field to omit) - read it back the stepover's way.
                if let OpParams::Contour5D { shank_avoid, .. }
                | OpParams::Parallel5D { shank_avoid, .. }
                | OpParams::Flow5D { shank_avoid, .. }
                | OpParams::MultiAxisContour { shank_avoid, .. } = self
                {
                    h.write_u8(u8::from(*shank_avoid));
                }
            }
            // The wrap ladder (M5.1 stage E2, tag 14): the Pocket3D
            // door's bytes without the fields the kernel overrides or
            // never runs (retract, rest, feed-opt), the tolerance and
            // the three pose angles appended - the ladder's numbers,
            // read the wrap's way, digest as the ladder it is.
            OpParams::RotaryOffset {
                links: _,
                tolerance,
                stepover,
                stock_to_leave,
                clearance_z,
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
                lead_deg,
                lag_deg,
                max_tilt_deg,
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
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive | PocketStrategy::OffsetRings => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                for v in [*tolerance, *lead_deg, *lag_deg, *max_tilt_deg] {
                    w(h, v);
                }
            }
            // The stamped ladder (M5.1 stage E3, tag 15): the Pocket3D
            // arm's bytes verbatim - the hop plane included, the rest
            // and feed-opt switches absent (the kernel runs neither,
            // variant docs) - then the tolerance and the three pose
            // angles: leaning the spindle rewrites every pose on the
            // path, so it moves the stamp even though no position
            // moves.
            OpParams::MultiAxisRough {
                links: _,
                tolerance,
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
                lead_deg,
                lag_deg,
                max_tilt_deg,
                shank_avoid,
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
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive | PocketStrategy::OffsetRings => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                wh_opt(h, *retract_z);
                for v in [*tolerance, *lead_deg, *lag_deg, *max_tilt_deg] {
                    w(h, v);
                }
                // The dodge rides the digest: the flag changes the
                // poses the stamp states, and a stale path must not
                // survive its flip (stage F).
                h.write_u8(u8::from(*shank_avoid));
            }
            // The ladder is the swarf door's bytes plus the rung count
            // (an integer on the digest rides `write_u32` like the tab
            // law's count above): two and three passes are different
            // programs, and the stamp says so.
            OpParams::AdvancedSwarf {
                tolerance,
                stepover,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                passes,
            } => {
                w(h, *tolerance);
                match stepover {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                for v in [*lead_deg, *lag_deg, *max_tilt_deg] {
                    w(h, v);
                }
                h.write_u32(*passes);
            }
            _ => {}
        }
    }
}

//! The 2D milling arms of `OpParams::stamp_with` (#1849 split): the 2D
//! milling rides past the pocket family (offset, slots, trace, engrave,
//! chamfer, circular). Provenance: moved verbatim from `params.rs`; our
//! own digest law.

use super::*;
use super::digest::{self, w, wp2, wt};

impl OpParams {
    /// The digest bytes of the 2D milling rides past the pocket family
    /// (offset, slots, trace, engrave, chamfer, circular); `stamp_with`
    /// dispatches here exhaustively, so any other kind never reaches this
    /// match.
    pub(super) fn stamp_mill2d(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| digest::wh_opt(world, setup, src, h, v);
        match self {
            OpParams::Offset2D {
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
                tilt,
            } => {
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius] {
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
                // #1733: written only when ON, as for the pocket.
                if *blend_turns {
                    h.write_u8(0xB1);
                    if let Some(r) = blend_radius {
                        h.write_u8(0xB2);
                        w(h, *r);
                    }
                }
            }
            OpParams::Slot {
                links: _,
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                stepover,
                feed,
                plunge_feed,
                climb,
                entry,
                helix_radius,
                tilt,
                extend_start,
                extend_end,
            } => {
                wp2(h, *a);
                wp2(h, *b);
                w(h, *width);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *stepover, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                // Appended at M8.2 C's entry fields: a pre-entry slot
                // op goes Stale once and regenerates byte-identically
                // (the Plunge default IS the old law) - the same
                // handshake the contour's M8.2 A append made.
                w(h, *helix_radius);
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                wt(h, *tilt);
                // #1731: appended only when set, so a slot with no
                // extension keeps its hash and stays Fresh.
                if *extend_start != 0.0 || *extend_end != 0.0 {
                    w(h, *extend_start);
                    w(h, *extend_end);
                }
            }
            OpParams::TrochoidalSlot {
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
            } => {
                wp2(h, *a);
                wp2(h, *b);
                for v in [*width, loop_width.unwrap_or(-1.0), *step, *max_engagement, *smoothing, *stock_to_leave] {
                    w(h, v);
                }
                match ret {
                    crate::machining2d::TrochoidReturn::Microlift { height } => {
                        h.write_u8(0);
                        w(h, *height);
                    }
                    crate::machining2d::TrochoidReturn::Retract => h.write_u8(1),
                }
                wh(h, *clearance_z);
                wh_opt(h, *retract_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *return_feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                tilt,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                wt(h, *tilt);
            }
            // The engrave twin (M2.4 B): the trace's bytes, the tag
            // at the head saying which name authored them.
            OpParams::Engrave {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                tilt,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                wt(h, *tilt);
            }
            // The chamfer ride digests its whole promise: the land,
            // the angle, the side the cone rides, the chord budget,
            // and the ONE height it stands on - the depth is derived
            // from width and angle, so nothing else to write (M2.4 B).
            OpParams::Chamfer2D {
                width,
                angle_deg,
                clearance_z,
                retract_z,
                top_z,
                feed,
                plunge_feed,
                climb,
                side,
                tolerance,
                tilt,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                for v in [*width, *angle_deg, *tolerance, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                });
                wh_opt(h, *retract_z);
                wt(h, *tilt);
            }
            // The circular ladder digests the pocket family's bytes
            // plus the authored disk: center, radius, and the dome
            // law's one bit (M2.4 B).
            OpParams::Circular {
                links: _,
                center,
                radius,
                vault,
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
                tilt,
            } => {
                wp2(h, *center);
                w(h, *radius);
                h.write_u8(u8::from(*vault));
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius] {
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
            }
            _ => {}
        }
    }
}

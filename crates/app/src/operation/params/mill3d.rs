//! The 3D milling arms of `OpParams::stamp_with` (#1849 split): the 3D
//! pocket, the 3D finish family and the deburr ride. Provenance: moved
//! verbatim from `params.rs`; our own digest law.

use super::*;
use super::digest::{self, w};

impl OpParams {
    /// The digest bytes of the 3D pocket, the 3D finish family and the
    /// deburr ride; `stamp_with` dispatches here exhaustively, so any
    /// other kind never reaches this match.
    pub(super) fn stamp_mill3d(
        &self,
        world: &World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| digest::wh(world, setup, src, h, v);
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| digest::wh_opt(world, setup, src, h, v);
        match self {
            OpParams::Pocket3D {
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
                rest_rough,
                optimal_load,
                feed_opt,
                shell,
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
                    // M11.4 (the Pocket3D port): the HSM plan per level
                    // is a new generation of the word (old Adaptive ops
                    // go Stale once); the rings keep the byte they
                    // always digested under.
                    PocketStrategy::Adaptive => 4,
                    PocketStrategy::OffsetRings => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                    Entry3D::Arc => 3,
                });
                wh_opt(h, *retract_z);
                // v12-era rest pass: the flag is the generator's
                // switch, so it belongs in the digest (a model edit
                // must mark a rest path Stale like any other).
                h.write_u8(u8::from(*rest_rough));
                // Same law for the v14 feed modulation: flipping it
                // changes every F word, so it changes the stamp.
                h.write_u8(u8::from(*feed_opt));
                // Appended last, per the digest's append-only law:
                // a no-shell op's stamp bytes are exactly the
                // pre-shell stream, so nothing goes Stale that didn't
                // change
                // (the shell flag is the generator's switch, like
                // `rest_rough` before it).
                h.write_u8(u8::from(*shell));
            }
            OpParams::Finish3D {
                pattern,
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                ramp,
                min_step,
                ramp_angle,
                entry_ramp,
                steep_stepover,
                pencil_reference,
                fill_openings,
            } => {
                // v16: the pattern chooses a different kinematic (and
                // the raster fields only answer to Parallel), so they
                // belong in the digest; the Some/None split keeps a
                // derived stepover from digesting as the number 0.
                h.write_u8(*pattern as u8);
                match stepover {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                // v17 (M2.2e): ramp changes the chain between lines,
                // so it belongs in the digest; the one-time staleness
                // it hands old ops is the step-up law, same as ever.
                h.write_u8(u8::from(*ramp));
                // v18 (M2.2e): the surface-finish pair. min_step
                // floors the march's refinement (same Some/None
                // split as stepover above - None must not digest as
                // the number 0), ramp_angle caps the chain's slope
                // - both change moves, both belong in the digest.
                match min_step {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *ramp_angle);
                // v0.5 (M8.2 B): the ramped first contact rewrites the
                // entry legs of every ring, so it stamps the digest
                // like every other move-changing word.
                h.write_u8(u8::from(*entry_ramp));
                // v46 (M2.5 A): the ring ladder's own spacing authors
                // a different level count under SteepShallow, so it
                // digests - same Some/None split as `stepover` above,
                // so a scallop-derived ladder never digests as the
                // number 0. The two new patterns themselves ride the
                // pattern byte untouched: append-only ordinals.
                match steep_stepover {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                // v47 (M2.5 B): the crease pair. pencil_reference
                // authors which corners Pencil keeps (and digests
                // nothing new for Corner, where it is inert - inert
                // numbers still ride the digest, the same honesty
                // `stepover` keeps under Waterline); the two new
                // patterns ride the pattern byte, append-only.
                match pencil_reference {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                // #1196: the land width changes the surface the path
                // is planned on. Only a stated width digests (a tag
                // byte, then the number), so every op without one
                // keeps the stamp it had - no one-time staleness.
                if let Some(s) = fill_openings {
                    h.write_u8(0x4C);
                    w(h, *s);
                }
            }
            // The deburr ride digests the one authored number, the
            // window's two planes, the clearance plane it hops to,
            // and the two feeds - the cone itself is the TOOL's
            // word, already riding the tool revision, and the depth
            // is derived, chamfer's grammar on a real dihedral
            // (M2.5 E B).
            OpParams::Deburr {
                width,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*width, *feed, *plunge_feed] {
                    w(h, v);
                }
            }
            _ => {}
        }
    }
}

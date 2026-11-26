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
                method,
                pass_start, picks: _
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
                // #1591: written only where the method is read and off
                // its default, so every earlier op keeps its digest.
                if *strategy == PocketStrategy::Adaptive && *method == crate::machining2d::AdaptiveMethod::Trochoidal {
                    h.write_u8(0xA5);
                }
                // #3361: written only off Auto, so every earlier op keeps
                // its digest (a forced value on rows changes the notes).
                digest::write_pass_start(*pass_start, h);
            }
            OpParams::Finish3D {
                pattern,
                direction_angle,
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
                gap_link,
                waterlines,
                pass_start, picks: _
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
                // The gap link authors the chain between runs. Only
                // a non-Jump choice digests (one tag byte plus the
                // variant), so every op that hops as before keeps
                // the stamp it had.
                if *gap_link != crate::finishing3d::GapLink::Jump {
                    h.write_u8(0x4D);
                    h.write_u8(match *gap_link {
                        crate::finishing3d::GapLink::Straight => 2,
                        crate::finishing3d::GapLink::Jump => 1,
                    });
                }
                // #2811: skipping the waterline ring pass changes the cut
                // path on steep flanks. Only `false` digests (one tag
                // byte), so every op with waterlines on keeps its stamp.
                if !*waterlines {
                    h.write_u8(0x4E);
                }
                // Keep the old stamp for the default direction and for
                // patterns whose paths do not depend on this angle.
                if *pattern == crate::finishing3d::FinishPattern::Parallel && *direction_angle != 0.0 {
                    h.write_u8(0x50);
                    w(h, *direction_angle);
                }
                // #3362: written only off Auto and only for the patterns
                // that read it, so every other op keeps its stamp.
                if crate::finishing3d::reads_pass_start(*pattern) {
                    digest::write_pass_start(*pass_start, h);
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
                exclude_edges,
                min_edge_angle_deg,
                exclude_holes,
                hole_min_diameter,
                hole_max_diameter,
                unreachable,
                sizing, picks: _
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*width, *feed, *plunge_feed] {
                    w(h, v);
                }
                // #1888: the edge filters, each digested only off its
                // rest value - an op from before them keeps its stamp.
                if *min_edge_angle_deg != 0.0 {
                    h.write_u8(0x4D);
                    w(h, *min_edge_angle_deg);
                }
                if *exclude_holes {
                    h.write_u8(0x4E);
                    w(h, *hole_min_diameter);
                    w(h, *hole_max_diameter);
                }
                for g in exclude_edges {
                    h.write_u8(0x4F);
                    h.write(g.entity.as_bytes());
                    h.write_u64(g.feature.id());
                }
                // #1889: only `Refuse` writes a byte, so the stamp of
                // each earlier Deburr (skip with a note) does not move.
                if *unreachable == crate::finishing3d::UnreachableEdges::Refuse {
                    h.write_u8(0x52);
                }
                // #1890: the break sizing writes only off one Constant
                // Width chamfer pass, the ride of each earlier Deburr.
                if *sizing != crate::finishing3d::DeburrBreak::default() {
                    h.write_u8(0x53);
                    h.write_u8(u8::from(sizing.mode == crate::finishing3d::BreakMode::ConstantDepth));
                    w(h, sizing.depth);
                    h.write_u8(u8::from(sizing.profile == crate::finishing3d::BreakProfile::Round));
                    h.write_u32(sizing.passes);
                }
            }
            // #1894: a new kind, so every field is digested from the
            // start - no earlier stamp to keep.
            OpParams::MultiAxisDeburr {
                width,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                exclude_edges,
                min_edge_angle_deg,
                exclude_holes,
                hole_min_diameter,
                hole_max_diameter,
                unreachable,
                sizing,
                axis_mode,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                shank_avoid, picks: _
            } => {
                h.write_u8(0x5A);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*width, *feed, *plunge_feed, *min_edge_angle_deg, *hole_min_diameter, *hole_max_diameter] {
                    w(h, v);
                }
                h.write_u8(u8::from(*exclude_holes));
                for g in exclude_edges {
                    h.write_u8(0x4F);
                    h.write(g.entity.as_bytes());
                    h.write_u64(g.feature.id());
                }
                h.write_u8(u8::from(*unreachable == crate::finishing3d::UnreachableEdges::Refuse));
                h.write_u8(u8::from(sizing.mode == crate::finishing3d::BreakMode::ConstantDepth));
                w(h, sizing.depth);
                h.write_u8(u8::from(sizing.profile == crate::finishing3d::BreakProfile::Round));
                h.write_u32(sizing.passes);
                h.write_u8(u8::from(*axis_mode == crate::multiaxis::DeburrAxisMode::Axis4));
                for v in [*lead_deg, *lag_deg, *max_tilt_deg] {
                    w(h, v);
                }
                h.write_u8(u8::from(*shank_avoid));
            }
            _ => {}
        }
    }
}

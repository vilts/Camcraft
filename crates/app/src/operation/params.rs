use super::*;

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
        let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| {
            let (tag, slot) = match v.reference {
                HeightReference::Fixed => (0u8, 0u8),
                HeightReference::TopOfStock => (1, 0),
                HeightReference::BottomOfStock => (2, 0),
                HeightReference::TopOfModel => (3, 0),
                HeightReference::BottomOfModel => (4, 0),
                // #329: the slot is the setup's; digesting the resolved
                // one keeps every migrated stamp exactly as it was.
                HeightReference::WorkOffset => (5, u8::try_from(crate::setup::wcs_slot(world, setup)).unwrap_or(u8::MAX)),
                HeightReference::TopOfSelection => (6, 0),
                HeightReference::BottomOfSelection => (7, 0),
            };
            h.write_u8(tag);
            h.write_u8(slot);
            w(h, v.offset);
            if tag != 0 {
                match resolve_height(world, setup, src, v) {
                    Some(z) => {
                        h.write_u8(1);
                        w(h, z);
                    }
                    None => h.write_u8(0),
                }
            }
        };
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| match v {
            Some(v) => {
                h.write_u8(1);
                wh(h, v);
            }
            None => h.write_u8(0),
        };
        let wp2 = |h: &mut std::hash::DefaultHasher, p: Point2| {
            w(h, p.x);
            w(h, p.y);
        };
        // The 3+2 index (M5.1 stage B): a tilt changes every move the
        // generator makes, so it changes the digest. Presence first,
        // then the two letters and the anchor - the same shape the
        // optional scalars above use.
        let wt = |h: &mut std::hash::DefaultHasher, t: Option<Tilt>| match t {
            Some(t) => {
                h.write_u8(1);
                w(h, t.a_deg);
                w(h, t.b_deg);
                for v in t.origin {
                    w(h, v);
                }
            }
            None => h.write_u8(0),
        };
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
            } => {
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
                });
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
            } => {
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
            // The wire contour (#532): every stored word. The wire
            // and the gap are the machine condition's - `current_stamp`
            // digests them (`wire_op::stamp_condition`).
            OpParams::WireContour(p) => {
                wh(h, p.top_z);
                wh(h, p.bottom_z);
                h.write(p.condition.as_bytes());
                h.write_u8(0xff);
                // #1370: the side as its own discriminant - die and punch
                // keep the bytes they always wrote (0 / 1), so no closed
                // cut's stamp moves; an open run's left / right (#1109)
                // and its run-out clearance, read only by an open run,
                // stand apart.
                match p.side {
                    super::WireSide::Die => h.write_u8(0),
                    super::WireSide::Punch => h.write_u8(1),
                    super::WireSide::Left => {
                        h.write_u8(2);
                        w(h, p.clearance);
                    }
                    super::WireSide::Right => {
                        h.write_u8(3);
                        w(h, p.clearance);
                    }
                }
                // #540: written only when set, so no contour's stamp moves.
                if let Some(c) = p.burnout {
                    h.write(b"burnout");
                    w(h, c);
                }
                match p.start {
                    Some([x, y]) => {
                        h.write_u8(1);
                        w(h, x);
                        w(h, y);
                    }
                    None => h.write_u8(0),
                }
                // #539: the tabs, only when stated, so no stamp of an
                // untabbed cut moves. `capture` shapes no move and is
                // read live at the post.
                if let Some(t) = &p.tabs {
                    h.write(b"tabs");
                    h.write_u32(t.count.unwrap_or(u32::MAX));
                    w(h, t.width.unwrap_or(f64::NAN));
                    h.write_usize(t.at.len());
                    for [x, y] in &t.at {
                        w(h, *x);
                        w(h, *y);
                    }
                    h.write(t.cutoff_condition.as_bytes());
                    h.write_u8(0xff);
                }
                // #536: only a stated taper is digested, so every
                // vertical cut's stamp stays what it was.
                if p.taper_deg != 0.0 {
                    h.write(b"taper");
                    w(h, p.taper_deg);
                }
                // #537: likewise only a stated 4-axis cut.
                if let Some(f) = &p.four_axis {
                    super::wire_ruled::digest(f, h);
                }
            }
            // The sinker burn (#541): every stored number, the datum
            // and the condition id.
            // #545: the side burn writes the burn's bytes under its own tag.
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(super::SinkerSideBurnOp { burn: p, .. }) => {
                wh(h, p.clearance_z);
                wh(h, p.top_z);
                wh(h, p.floor_z);
                h.write_u8(match p.datum {
                    super::SinkerDatum::WorkTop => 0,
                    super::SinkerDatum::CavityFloor => 1,
                    super::SinkerDatum::ElectrodeTip => 2,
                });
                for v in [p.depth, p.x, p.y, p.feed] {
                    w(h, v);
                }
                h.write(p.condition.as_bytes());
                h.write_u8(0xff);
                // #543: the pass and the finished cavity it is judged
                // against - a byte and the sizes only for a rough or
                // finish, so a single burn's digest never moves.
                if p.pass != super::SinkerPass::Single {
                    h.write_u8(match p.pass {
                        super::SinkerPass::Single => 0,
                        super::SinkerPass::Rough => 1,
                        super::SinkerPass::Finish => 2,
                    });
                    w(h, p.cavity_x);
                    w(h, p.cavity_y);
                }
                // #544: the orbit - bytes only when one is stated, so a
                // plain plunge's digest does not move.
                if let Some(o) = &p.orbit {
                    h.write(o.kind.word().as_bytes());
                    w(h, o.radius);
                    h.write_u32(o.vectors);
                    h.write_u32(o.turns);
                }
                // #546: the layer step and a measured wear, a tagged
                // byte only when stated - an older burn's digest holds.
                for (tag, v) in [(1u8, p.layer), (2, p.wear_lost), (3, p.wear_over)] {
                    if let Some(v) = v {
                        h.write_u8(tag);
                        w(h, v);
                    }
                }
                // #545: then the side's direction, amount and undersize.
                if let OpParams::SinkerSideBurn(s) = self {
                    h.write(s.direction.word().as_bytes());
                    w(h, s.amount);
                    w(h, s.undersize);
                }
            }
            // The plasma profile (#670): every stored number, the
            // optional ones with a presence byte.
            OpParams::PlasmaProfile(p) => {
                wh(h, p.clearance_z);
                wh(h, p.top_z);
                wh(h, p.bottom_z);
                h.write_u8(u8::from(p.comp == super::PlasmaComp::Control));
                h.write_u8(u8::from(p.reversed));
                for v in [p.lead_in, p.lead_out, p.small_hole_dia, p.small_hole_feed, p.overburn] {
                    w(h, v);
                }
                for v in [p.kerf, p.feed, p.pierce_height, p.cut_height, p.pierce_delay] {
                    match v {
                        Some(x) => {
                            h.write_u8(1);
                            w(h, x);
                        }
                        None => h.write_u8(0),
                    }
                }
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
            } => {
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
        }
        // M11.2 C/D: the level transitions and links change the moves,
        // so they change the digest - appended only when they differ
        // from the kind's pre-M11.2 shape, so every op that never
        // touched them keeps the digest it was cached under. (A contour
        // saved with the old stay-down ramp goes Stale once and
        // regenerates the same path - the mechanism saying so.)
        // (The runtime centre-cutting flag is the tool's, never the op's.)
        if let Some(l) = self.links().map(|l| crate::links::Links {
            approach: crate::links::Approach { center_cutting: true, ..l.approach },
            ..*l
        })
            && l != self.default_links()
        {
            h.write_u8(0xC2);
            let link = |k: crate::links::Link| match k {
                crate::links::Link::Clearance => 0u8,
                crate::links::Link::Retract => 1,
                crate::links::Link::KeepDown => 2,
            };
            h.write_u8(match l.transition {
                None => 0,
                Some(Entry3D::Plunge) => 1,
                Some(Entry3D::Ramp) => 2,
                Some(Entry3D::Helix) => 3,
                Some(Entry3D::Arc) => 4,
            });
            h.write_u8(link(l.level));
            h.write_u8(link(l.pass));
            for v in [l.ramp_angle, l.helix_pitch, l.max_stay_down] {
                w(h, v);
            }
            // M11.3 B/C/D: the entry, lead and feed parameters - bytes
            // only when they differ from the defaults, so an op with
            // links but default approach keeps its digest.
            let a = l.approach;
            if !a.is_default() {
                h.write_u8(0xC3);
                for v in [
                    a.ramp_angle, a.ramp_max_len, a.helix_min_radius, a.helix_pitch, a.helix_angle,
                    a.arc_radius, a.arc_sweep, a.plunge_height, a.ramp_feed, a.plunge_feed,
                    a.lead_feed_pct, a.max_engagement, a.ext_in, a.ext_out, a.lead_height,
                    a.overlap, a.axis_tol,
                ] {
                    w(h, v);
                }
                for f in [a.ramp_to_depth, a.helix_cw, a.axis_start] {
                    h.write_u8(u8::from(f));
                }
                // M11.3: the horizontal arc entry - a byte only when on,
                // so every earlier approach keeps its digest.
                if a.arc_xy {
                    h.write_u8(0xC4);
                }
                // #1449: pre-drilled entry - a byte only when on.
                if a.predrilled {
                    h.write_u8(0xC7);
                }
                // M13.11: control compensation moves what the post
                // writes - a byte only when on, so every earlier
                // approach keeps its digest.
                if a.compensation == crate::links::Compensation::Control {
                    h.write_u8(0xC5);
                }
                // #38: the wear variant posts the centre path under
                // G41/G42 - its own byte, again only when on.
                if a.compensation == crate::links::Compensation::Wear {
                    h.write_u8(0xC6);
                }
                // #1448: the user's start point - bytes only when set.
                if let Some(q) = a.start_point {
                    h.write_u8(0xC7);
                    w(h, q.x);
                    w(h, q.y);
                }
                // #1473: the exit point - bytes only when set.
                if let Some(q) = a.exit_point {
                    h.write_u8(0xC8);
                    w(h, q.x);
                    w(h, q.y);
                }
                match a.lead_out {
                    None => h.write_u8(0xFF),
                    Some(Lead::None) => h.write_u8(0),
                    Some(Lead::Linear { length }) => {
                        h.write_u8(1);
                        w(h, length);
                    }
                    Some(Lead::Arc { radius }) => {
                        h.write_u8(2);
                        w(h, radius);
                    }
                    Some(Lead::Tangential { radius }) => {
                        h.write_u8(3);
                        w(h, radius);
                    }
                    Some(Lead::Perpendicular { length }) => {
                        h.write_u8(4);
                        w(h, length);
                    }
                }
            }
        }
    }
}

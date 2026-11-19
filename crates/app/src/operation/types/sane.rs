//! Split from `types.rs` by M5.0.1; structure only.

use crate::machining2d::PocketStrategy;

use crate::operation::types::params::OpParams;
use crate::operation::types::sel::lead_sane;

/// The refusal of pre-drilled entry on a kind that does not read it (#1449).
pub const PREDRILLED_POCKET_ONLY: &str =
    "pre-drilled entry is read by the 2D Pocket only so far - turn it off for this operation kind";

/// The refusal of the Adaptive strategy on 2D Pocket (#1517): Adaptive
/// clearing lives in the 2D Adaptive operation only.
pub const POCKET_ADAPTIVE_RETIRED: &str =
    "Adaptive clearing lives in the 2D Adaptive operation now - use 2D Adaptive instead of Pocket with Adaptive";

/// The refusal of a corner-arc radius no control can cut (#1733); the
/// bound by the tool radius is the generator's (it owns the tool).
pub const ARC_RADIUS_DOOR: &str = "corner arc radius must be a number of at least 0.01 mm";

/// The refusal of arc turns under 2D Adaptive's Trochoidal method (#1733):
/// its loops are arcs already, and the option is never silently dropped.
pub const ARC_TURNS_TROCHOIDAL: &str =
    "arc turns at corners are not available with the Trochoidal method - turn the option off or use Spiral";

impl OpParams {
    /// The corner-arc option's door (#1733), for the kinds that carry it.
    fn arc_turn_refusal(&self) -> Option<&'static str> {
        let (on, radius, trochoidal) = match self {
            OpParams::Pocket { blend_turns, blend_radius, .. }
            | OpParams::Offset2D { blend_turns, blend_radius, .. } => (*blend_turns, *blend_radius, false),
            OpParams::Adaptive2D { blend_turns, blend_radius, method, .. } => {
                (*blend_turns, *blend_radius, *method == crate::machining2d::AdaptiveMethod::Trochoidal)
            }
            _ => return None,
        };
        if radius.is_some_and(|r| !(r.is_finite() && r >= crate::machining2d::MIN_BLEND_RADIUS)) {
            return Some(ARC_RADIUS_DOOR);
        }
        (on && trochoidal).then_some(ARC_TURNS_TROCHOIDAL)
    }

    /// Pre-drilled entry asked on a kind other than the 2-D Pocket.
    fn predrilled_elsewhere(&self) -> bool {
        !matches!(self, OpParams::Pocket { .. }) && self.links().is_some_and(|l| l.approach.predrilled)
    }

    /// The by-name refusal behind a failed [`Self::is_sane`], where a
    /// kind can say it (#546: the sinker burn's wear rate and layer
    /// step; the side burn's structural refusal, which names a wear rate
    /// or layer step it does not support yet); `None` keeps the
    /// commands' generic "not usable".
    #[must_use]
    pub fn named_refusal(&self) -> Option<String> {
        if matches!(self, OpParams::Pocket { strategy: PocketStrategy::Adaptive, .. }) {
            return Some(POCKET_ADAPTIVE_RETIRED.into());
        }
        if let Some(why) = self.tilt().and_then(|t| t.refusal()) {
            return Some(why);
        }
        if self.predrilled_elsewhere() {
            return Some(PREDRILLED_POCKET_ONLY.into());
        }
        if let Some(why) = self.arc_turn_refusal() {
            return Some(why.into());
        }
        // #1489: the rest group, in the rest generator's own words.
        if let Some(why) = crate::operation::rest_op::rest_refusal(self) {
            return Some(why);
        }
        // #1448: only the untilted 2D contour takes a picked start so far;
        // every other kind refuses it here rather than ignore it.
        // #1473: the exit point likewise.
        for (what, set) in [
            ("start", self.links().is_some_and(|l| l.approach.start_point.is_some())),
            ("exit", self.links().is_some_and(|l| l.approach.exit_point.is_some())),
        ] {
            if !set {
                continue;
            }
            match self {
                OpParams::Contour { tilt: None, .. } => {}
                OpParams::Contour { .. } => {
                    return Some(format!("a {what} point is not taken on a tilted (3+2) contour yet - clear it or the tilt"));
                }
                _ => return Some(format!("a {what} point is taken only by the 2D contour so far - clear it on this operation")),
            }
        }
        match self {
            OpParams::SinkerBurn(p) => p.compensation_refusal(),
            OpParams::SinkerSideBurn(p) => p.refusal(),
            OpParams::Adaptive2D { troch, .. } => troch.refusal(),
            _ => None,
        }
    }

    /// Shallow sanity for command construction, no world: finite numbers,
    /// positive increments, in-range slots. Ordering needs resolved
    /// heights and runs in [`crate::operation::depth_sane`] wherever a command holds the
    /// world; the generators own the deep rules (stepover vs diameter,
    /// lead sizes), which still fire inside `GenerateOperation`'s
    /// constructor. A From: Selection height (M11.3 A) is structurally
    /// sane like any reference - a finite offset; whether the op's source
    /// answers it is a world question, asked by the commands' heights
    /// door (`selection_heights_sane`) and refused there by name.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        // The 3+2 index answers at the door (M5.1 stage B): a tilt is
        // only ever storable on the planar kinds, so one check covers
        // all eight arms - `tilt()` is None for the rest by the
        // accessor's exhaustive law. The mounted machine's REACH is a
        // different question and belongs to generation, which names
        // the machine when the angle is past it.
        let tilt_ok = self.tilt().is_none_or(|t| t.is_sane());
        // The level transitions and links (M11.2) answer at the door for
        // every ladder kind at once: the ramp angle where it is read, a
        // pitch and a stay-down limit that are zero or positive.
        let links_ok = self.links().is_none_or(|l| l.validate().is_ok());
        // #1449: pre-drilled entry is read by the 2-D Pocket only - set
        // on another kind it would be a silently dropped choice.
        let links_ok = links_ok && !self.predrilled_elsewhere();
        // #1733: the corner-arc radius and the method that refuses it.
        let links_ok = links_ok && self.arc_turn_refusal().is_none();
        // #1489: a rest group the generator would refuse.
        let links_ok = links_ok && crate::operation::rest_op::rest_refusal(self).is_none();
        tilt_ok
            && links_ok
            && match self {
            // The plasma door (#670): the parameter set's own sanity.
            OpParams::PlasmaProfile(p) => p.is_sane(),
            // The wire door (#532): the parameter set's own sanity; the
            // condition is the command door's world question.
            OpParams::WireContour(p) => p.is_sane(),
            // The sinker door (#541): the parameter set's own sanity.
            OpParams::SinkerBurn(p) => p.is_sane(),
            // #545: the side burn's own door.
            OpParams::SinkerSideBurn(p) => p.is_sane(),
            // The probe's door (#108): every distance and feed a real
            // number, the measure no faster than the seek, the back-off
            // inside the approach, a slot G54..G59 when one is named.
            OpParams::Probe {
                clearance_z,
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
                ..
            } => {
                clearance_z.is_sane()
                    && target.iter().all(|v| v.is_finite())
                    && nneg(*radius)
                    && pos(*approach)
                    && pos(*overtravel)
                    && pos(*backoff)
                    && *backoff <= *approach
                    && pos(*fast_feed)
                    && pos(*slow_feed)
                    && *slow_feed <= *fast_feed
                    && nneg(*depth)
                    && nneg(*inset)
                    && pos(*tolerance)
                    && set_wcs.is_none_or(|s| s <= 5)
            }
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && clearance_z.is_sane()
                    && cut_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && top_z.is_none_or(|t| t.is_sane())
                    && stepdown.is_none_or(|s| s.is_finite() && s > 0.0)
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
                tolerance,
                tab_width,
                tab_height,
                helix_radius,
                ..
            } => {
                // Structural only, the pocket doors' word (M8.2 A): a
                // Plunge entry never consults the radius, and where a
                // Helix does, the generator certifies it against the
                // bore's wall per level - dive room is the refusal of
                // record there, said in the path's notes.
                nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*tolerance)
                    && nneg(*tab_width)
                    && nneg(*tab_height)
                    && pos(*helix_radius)
                    && lead_sane(lead)
            }
            OpParams::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                optimal_load,
                ..
            } => {
                // The entry family joined the planar pocket at M5.1 H,
                // so its door mirrors Pocket3D's word for word: the
                // generator owns the radius-vs-tool check (it owns the
                // tool), and the optimal load is finite always,
                // bounded (0, 50%] of D only when Adaptive reads it
                // (the old (0, 90] degree door, M11.4 B).
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && !matches!(strategy, PocketStrategy::Adaptive)
                    && (optimal_load.is_finite()
                        && (!matches!(strategy, PocketStrategy::OffsetRings)
                            || crate::load::load_in_door(*optimal_load)))
            }
            // The twins' doors are the pocket's, read with the
            // strategy already decided (M2.4 A): Adaptive's engagement
            // angle is bounded always because its kind IS the read,
            // and Offset has no angle to check - the ring-vs-tool
            // tiling check stays the generator's refusal of record.
            OpParams::Adaptive2D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                optimal_load,
                troch,
                ..
            } => {
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && crate::load::load_in_door(*optimal_load)
                    && troch.is_sane()
            }
            OpParams::Offset2D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                ..
            } => {
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
            }
            // The engrave door (M2.4 B) is the trace's verbatim -
            // the twin law says the numbers are asked the same
            // questions, only the name differs.
            OpParams::Engrave {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The 2-D chamfer's door (M2.4 B): the authored promise -
            // a land wider than nothing, cut at an angle that is a
            // chamfer and not a flat (0) or a wall (90): the ride
            // would descend nothing or everything. Whether the TOOL's
            // cone can lie on that promise is the generator's
            // question, refused there by name.
            OpParams::Chamfer2D {
                width,
                angle_deg,
                clearance_z,
                retract_z,
                top_z,
                feed,
                plunge_feed,
                tolerance,
                ..
            } => {
                pos(*width)
                    && (0.0..90.0).contains(angle_deg)
                    && pos(*tolerance)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The circular door (M2.4 B): the pocket's numbers beside
            // the authored disk - a center that is a place and a
            // radius wider than the ring spacing it seeds. Ring-vs-
            // tool and the vault's dome law are the generator's,
            // refused there by name.
            OpParams::Circular {
                center,
                radius,
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                ..
            } => {
                center.x.is_finite()
                    && center.y.is_finite()
                    && pos(*radius)
                    && pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
            }
            OpParams::Pocket3D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                optimal_load,
                ..
            } => {
                // Structural only, like the pocket's: the helix radius
                // is checked against the tool radius by the generator,
                // which is the only reader that owns a tool. A Plunge
                // entry never consults it, so any positive finite value
                // - the default 1.0 among them - is sane here. Same
                // law for the engagement angle: it must be finite
                // always, and inside its own (0, 90] only when the
                // Adaptive strategy actually reads it - the generator
                // repeats the bound as the refusal of record.
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (optimal_load.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive | PocketStrategy::OffsetRings)
                            || crate::load::load_in_door(*optimal_load)))
            }
            OpParams::Finish3D {
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                min_step,
                ramp_angle,
                steep_stepover,
                pencil_reference,
                fill_openings,
                ..
            } => {
                // No stepdown to check: the scallop law derives it from
                // tolerance and the tool radius, and the generator's
                // level cap refuses a tolerance that would need more
                // rings than the machine of record may hold.
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    // M2.5 A: the flank's own number is a spacing
                    // like the raster's - finite and positive, or absent.
                    && steep_stepover.is_none_or(pos)
                    // M2.5 B: the reference ball is a diameter like
                    // the tool's - finite and positive, or absent
                    // (which bigger is whose law is the kernel's to
                    // refuse by name; sanity only asks for a number).
                    && pencil_reference.is_none_or(pos)
                    // #1196: the land width is a diameter - finite
                    // and positive, or absent (no land model).
                    && fill_openings.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && min_step.is_none_or(pos)
                    // (0, 90): a flat ramp never descends - that is
                    // a traverse with a name it doesn't earn; a
                    // vertical one is a plunge, and the run's first
                    // contact already owns those. Both refusals
                    // happen at the gate, never at the emitter.
                    && (0.0..90.0).contains(ramp_angle)
            }
            // The deburr ride (M2.5 E B): `width` is the one authored
            // number and it must stand - a zero break removes nothing,
            // and the tip's drop derived from it must be finite and
            // positive, which is the kernel's own door restated at
            // command creation (the tool's cone completes the product
            // at generate; sanity asks only for the number).
            OpParams::Deburr {
                width,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*width)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Contour5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
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
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The swarf door (M5.1 stage D) is the raster's door with
            // the ruling added at generation (a dome refuses there,
            // by name - nothing in these numbers says "ruled"): the
            // leans still pose the unruled floors and rims the blade
            // family shares its paths with.
            | OpParams::Swarf {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The rotary door (M5.1 stage E) is the same door: the
            // leans now pose off the RADIAL instead of the normal,
            // but a quadrant is just as far as a tool may ever look
            // along the wall it rides. The axis the fit reads is
            // model data - it refuses at generation, by name, and
            // needs no number here.
            | OpParams::RotaryContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::RotaryParallel {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // Flow's door (M5.1 I) is the raster twins' - the field
            // that bends the lines is model data, refused or honored
            // at generation, and needs no number here. The
            // drive-curve contour stands in the same doorway (M5.1 J):
            // no stepover to question - the chain is the path - so it
            // falls to the inner match's `_` arm, and its top/bottom
            // are the window the picked vertices must live in.
            | OpParams::Flow5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::MultiAxisContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                // The finish door's shape (no stepdown - the scallop
                // law authors it), plus the pose door (M5.1 stage C):
                // the two leans are angles OFF the normal, so each is
                // confined to a quadrant - past 90 degrees the tool
                // would point along the surface it is cutting, and no
                // combination of lead and lag states is meant to reach
                // for that. The clamp is a cone half-angle: above 180
                // nothing is excluded, and at or below 0 the cone
                // holds no direction at all - both are typos about the
                // machine's reach, refused here rather than silently
                // ignored at the stamp. The raster kinds' optional
                // stepover rides the same Some-or-derived law as the
                // finish's.
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
                    && (match self {
                        OpParams::Parallel5D { stepover, .. }
                        | OpParams::Swarf { stepover, .. }
                        | OpParams::RotaryParallel { stepover, .. }
                        | OpParams::Flow5D { stepover, .. } => stepover.is_none_or(pos),
                        _ => true,
                    })
            }
            // The wrap ladder's door (M5.1 stage E2): the Pocket3D
            // door's numbers - plain stepover, radial stepdown, the
            // helix and engagement read by the entry law - crossed
            // with the rotary door's pose clause. The tolerance is
            // the verify budget here, not a scallop input, but a
            // sphere sweep still needs a positive one to be measured
            // against. There is no retract to check: the field does
            // not exist - the kernel names its own rapids plane.
            OpParams::RotaryOffset {
                tolerance,
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                optimal_load,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (optimal_load.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive | PocketStrategy::OffsetRings)
                            || crate::load::load_in_door(*optimal_load)))
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
            }
            // The stamped ladder's door (M5.1 stage E3): the Pocket3D
            // door verbatim - the hop plane included, which the plain
            // pocket stores and the wrap discards - plus the pose
            // clause and the verify budget. Positions are the
            // pocket's, so what is sane for one is sane for the
            // other; only the angles are new questions.
            OpParams::MultiAxisRough {
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
                helix_radius,
                strategy,
                optimal_load,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (optimal_load.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive | PocketStrategy::OffsetRings)
                            || crate::load::load_in_door(*optimal_load)))
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
            }
            OpParams::AdvancedSwarf {
                tolerance,
                stepover,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                passes,
                ..
            } => {
                // The swarf door, plus the ladder count: 1..4 is the
                // generator's named range (motion refuses 0 and 5+ by
                // the same numbers) - a rough above that is a file
                // typo, not a machinist's ladder, and the door says so
                // at command creation, not deep in a pass loop.
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
                    && (1..=4).contains(passes)
            }
            OpParams::Slot {
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
                extend_start,
                extend_end,
                ..
            } => {
                a.x.is_finite()
                    && a.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && pos(*width)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*stepover)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && nneg(*extend_start)
                    && nneg(*extend_end)
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
                ..
            } => {
                a.x.is_finite()
                    && a.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && pos(*width)
                    && loop_width.is_none_or(pos)
                    && pos(*step)
                    && pos(*max_engagement)
                    && *max_engagement <= 180.0
                    && match ret {
                        crate::machining2d::TrochoidReturn::Microlift { height } => pos(*height),
                        crate::machining2d::TrochoidReturn::Retract => true,
                    }
                    && nneg(*smoothing)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*return_feed)
                    && pos(*plunge_feed)
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
                cross,
                break_through,
                ..
            } => {
                // An empty hole list is sane here: a new Drill op exists
                // before its holes are detected, and creation must be
                // allowed to get the panel on screen. Generation is the
                // gate that refuses it, by name.
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && peck.is_finite()
                    && *peck >= 0.0
                    && dwell.is_finite()
                    && *dwell >= 0.0
                    && holes.iter().all(|h| h.is_usable())
                    && cross.iter().all(|h| h.is_usable())
                    // #1557: a stated break-through is a length.
                    && break_through.is_none_or(nneg)
            }
            // The facing ladder's door (M5.2 stage A): the wrap
            // ladder's numbers without the pattern's - a positive
            // verify budget, heights, feeds, and an axial infeed
            // that must be a positive distance. Which radii the
            // stations meet is the axis fit's and the profile's
            // question: refused at generation, by name.
            OpParams::Facing {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The profile ladders' doors (M5.2 stage B): the facing
            // numbers plus a leave that may be zero (the ride owns
            // the skin then) but never negative - the kernel refuses
            // stock to REMOVE past the door it stands behind, and so
            // does this one, by name; the stepdown ceiling against
            // the nose is the TOOL's question and stays the
            // generator's to ask.
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
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && stock_to_leave.is_finite()
                    && *stock_to_leave >= 0.0
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::ProfileFinish {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The blade kinds' doors (M5.2 stage C): the profile
            // numbers again - the blade's width is the TOOL's
            // question (the catalog and the tool editor answer it),
            // and the window-vs-blade fit is the generator's, refused
            // there by name. Parting's third height is a plane, not a
            // window end - same sanity law, different name.
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
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && stock_to_leave.is_finite()
                    && *stock_to_leave >= 0.0
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::GrooveFinish {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Part {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                bar_advance,
                inspect_tol,
                ..
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    // The feeder's advance is a length even when the
                    // signal is off: 0 stands (the act alone), a
                    // negative or unmeasurable one is not authored.
                    && bar_advance.is_finite()
                    && *bar_advance >= 0.0
                    // The probe budget (M5.3 D): 0 is silence, and a
                    // nonzero ask must be a measurement a probe can
                    // take - 0.1 um floors the world's best machines.
                    // The budget also chords the gate's composed
                    // scan (a quarter of it per sample edge), so a
                    // sub-floor ask is not merely unmeasurable, it
                    // would chase samples no machine holds - the
                    // generator's level-cap precedent, at the door.
                    && inspect_tol.is_finite()
                    && (*inspect_tol == 0.0 || *inspect_tol >= 1e-4)
            }
            // The threading door (M5.2 stage D): three positive helix
            // numbers - pitch, depth, infeed - beside the twins'
            // heights and feeds. Whether the V FITS the pitch it is
            // asked to cut is the TOOL's question and stays the
            // generator's, refused there by name (the kernel's
            // width-vs-pitch door).
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
                hand: _,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*pitch)
                    && pos(*depth)
                    && pos(*infeed)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    // The plan's tool-free numbers (#416); whether the
                    // flank angle fits the insert is the tool's
                    // question, asked by name at the threading gate.
                    && infeed_plan.is_sane()
                    && infeed_plan.refusal(*depth, *infeed, 180.0).is_none()
            }
            // The contour doors (M5.2 stage E): the twins' heights and
            // feeds, plus each kind's own promise-number, finite.
            // Whether the face the number states IS in the model -
            // and whether 90 degrees of it is really a face - is the
            // kernel's question and stays the kernel's, refused
            // there by name (the promise-at-the-door law).
            OpParams::Chamfering {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                angle_deg,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && angle_deg.is_finite()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // A diameter names a size: the taper door admits zero
            // only at the axis end of the cone (a point is a legal
            // small end), and both-zero is refused by the kernel.
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
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && bottom_dia.is_finite()
                    && *bottom_dia >= 0.0
                    && top_dia.is_finite()
                    && *top_dia > 0.0
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The multi-pass door is profile rough's law plus the
            // ride count - one ride is the classic pair, so the
            // count must at least be one; the cap is the kernel's.
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
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && stock_to_leave.is_finite()
                    && *stock_to_leave >= 0.0
                    && *passes >= 1
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The form door (M5.2 E2): a cut with no authored depth
            // has no authored numbers to doubt — the crown and the
            // band are TOOL steel, judged by the tool's own sanity,
            // and the model owns the plunge. What stands here is
            // the twins' heights and feeds verbatim.
            OpParams::FormTurning {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The boring door (M5.2 F): the form's set again - the
            // wall the nose rides is MODEL steel, and the bar's
            // ground nose is TOOL steel; what stands authored here
            // is the window, the tolerance and the two feeds, and
            // the kernel's doors do the rest.
            OpParams::BoringTurning {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The two hole-list rides (M2.4 C): the drill's door -
            // an empty list is SANE, so creation gets the panel on
            // screen, and generation refuses it by name. The wall
            // fit (does the cutter have a circle inside each hole?)
            // is the pair's question and stays the generator's to
            // ask, because only there do holes and tool meet.
            OpParams::Bore {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stock_to_leave,
                stepdown,
                feed,
                plunge_feed,
                tolerance,
                holes,
                break_through,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && nneg(*stock_to_leave)
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*tolerance)
                    && holes.iter().all(|h| h.is_usable())
                    && break_through.is_none_or(nneg)
            }
            OpParams::Thread {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                infeed,
                feed,
                plunge_feed,
                holes,
                pitch,
                passes,
                break_through,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && pitch.is_none_or(pos)
                    // #988: signs are judged by name at the door.
                    && passes.finish.is_finite()
                    && passes.allowance.is_finite()
                    && pos(*infeed)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && holes.iter().all(|h| h.is_usable())
                    && break_through.is_none_or(nneg)
            }
        }
    }

    /// The trochoidal slot's microlift bound (#774), on resolved heights:
    /// the lifted return is a feed move, so it may rise at most the
    /// clearance plane's height above the cut top - never feed above the
    /// clearance plane. `None` for every other kind and for Retract.
    /// Asked by the commands' heights door with `clearance` and `top`
    /// resolved; the motion layer holds the same law as a backstop.
    #[must_use]
    pub fn microlift_refusal(&self, clearance: f64, top: f64) -> Option<String> {
        let (who, height) = match self {
            OpParams::TrochoidalSlot { ret: crate::machining2d::TrochoidReturn::Microlift { height }, .. } => {
                ("the trochoidal slot's", *height)
            }
            OpParams::Adaptive2D {
                method: crate::machining2d::AdaptiveMethod::Trochoidal,
                troch: crate::machining2d::AdaptiveTrochParams { ret: Some(crate::machining2d::TrochoidReturn::Microlift { height }), .. },
                ..
            } => ("the 2D Adaptive (Trochoidal)", *height),
            _ => return None,
        };
        let room = clearance - top;
        (height > room + 1e-9).then(|| {
            format!(
                "a {height:.3} mm microlift would feed {who} return above the clearance plane - at most {room:.3} mm (the clearance above the top) fits; use the Retract return to hop higher"
            )
        })
    }
}

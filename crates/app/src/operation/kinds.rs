//! The operation-kind registry (M5.0.2): every per-kind table collapsed to
//! one row per kind, so a new kind registers here instead of being hunted
//! through the fan-out (docs/M5-PHASE5-PLAN.md, the coupling fact).
//!
//! A kind is: an `OpParams` variant (its fields), a row in [`KindInfo::ALL`]
//! (label, stamp tag, verify mode, retype seed), and exhaustive per-kind
//! glue that stays where it belongs - region builder, height walk, digest
//! body, generator call, DTO encode/decode. Those matches are deliberate:
//! an exhaustive match on a fresh variant fails to compile, which is the
//! machinery that FORCES registration. The registry owns only what a match
//! cannot express as data - and what used to be duplicated as a magic
//! literal (the stamp tag) or a silent `_ =>` arm (the ribbon's retype).
//!
//! Provenance: our own domain model; the kind families and the shared-field
//! carry-over rule are specced from public documentation of commercial CAM
//! workflows. No vendor source was consulted.

use crate::drilling::CycleChoice;
use crate::finishing3d::FinishPattern;
use crate::geo2d::Point2;
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;
use crate::tools::ToolKind;
use crate::verify::Mode;

use super::tool_law as tk;
use super::types::{Height, HeightReference, OpParams, OpType, Tilt};

/// The numbers that ride a retype: every kind stores them under the same
/// name, so the draft rule is one carry, then the row's seed (M1.5.2's
/// law, here since M5.0.2 - `crates/ui` used to hold this table).
#[derive(Clone, Copy, Debug)]
pub struct Carried {
    pub feed: f64,
    pub plunge: f64,
    pub climb: bool,
    pub clearance: Height,
    pub stepover: f64,
    pub stepdown: f64,
    pub top: Height,
    pub bottom: Height,
    pub retract: Height,
    /// The 3+2 index (M5.1 stage B): the planar kinds carry it across
    /// a retype, the non-planar seeds never read it - so a tilted
    /// pocket that becomes a 3D pocket loses the tilt WITH the kind
    /// that owned it, and a tilted pocket that becomes a tilted
    /// contour keeps it.
    pub tilt: Option<Tilt>,
}

impl Carried {
    /// Fold the shared numbers out of any kind. Where a kind has no field
    /// (Face has no bottom plane, Trace no stepover), the fold's default
    /// seeds the draft - never a guess about the old kind.
    #[must_use]
    pub fn of(p: &OpParams) -> Self {
        Self {
            feed: feed_of(p),
            plunge: plunge_of(p),
            climb: climb_of(p),
            clearance: p.clearance(),
            stepover: stepover_of(p).unwrap_or(3.0),
            stepdown: stepdown_of(p).unwrap_or(5.0),
            top: top_of(p).unwrap_or_else(|| Height::of(HeightReference::TopOfModel, 0.0)),
            // #214: the part's own floor, not the G54 plane - a shop that
            // zeroes G54 Z on the part top would otherwise get a zero-depth
            // operation that CreateOperation refuses before it can be edited.
            bottom: bottom_of(p).unwrap_or_else(|| Height::of(HeightReference::BottomOfModel, 0.0)),
            retract: retract_of(p).unwrap_or_else(|| Height::of(HeightReference::TopOfStock, 10.0)),
            tilt: p.tilt(),
        }
    }
}

fn feed_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { feed, .. }
        | OpParams::Contour { feed, .. }
        | OpParams::Pocket { feed, .. }
        | OpParams::Adaptive2D { feed, .. }
        | OpParams::Offset2D { feed, .. }
        // The engrave ride and the chamfer ride feed along the curve
        // like the contour's; the circular ladder feeds along its
        // rings (M2.4 B).
        | OpParams::Engrave { feed, .. }
        | OpParams::Chamfer2D { feed, .. }
        | OpParams::Circular { feed, .. }
        | OpParams::Pocket3D { feed, .. }
        | OpParams::Slot { feed, .. }
        | OpParams::TrochoidalSlot { feed, .. }
        | OpParams::Trace { feed, .. }
        | OpParams::Drill { feed, .. }
        | OpParams::Finish3D { feed, .. }
        // The deburr ride feeds along the edge at the cone's flank -
        // the same distance-per-minute the chamfer rides state (M2.5 E B).
        | OpParams::Deburr { feed, .. }
        | OpParams::Contour5D { feed, .. }
        | OpParams::Parallel5D { feed, .. }
        | OpParams::Swarf { feed, .. }
        | OpParams::AdvancedSwarf { feed, .. }
        | OpParams::RotaryContour { feed, .. }
        | OpParams::RotaryParallel { feed, .. }
        | OpParams::RotaryOffset { feed, .. }
        | OpParams::MultiAxisRough { feed, .. }
        | OpParams::Flow5D { feed, .. }
        | OpParams::MultiAxisContour { feed, .. }
        // The facing pass feeds along X at the station's Z - the
        // retype's feed number means the same distance-per-minute
        // here as on a mill (M5.2 A). The profile rides feed along
        // the offset curve: the nose's own travel, same units.
        | OpParams::Facing { feed, .. }
        | OpParams::ProfileRough { feed, .. }
        | OpParams::ProfileFinish { feed, .. }
        // A blade path never feeds axially at all - every feed move
        // is a radial plunge at the plunge number, every traverse a
        // rapid - so this number rides the trio and the stamp only,
        // inert but named (M5.2 C).
        | OpParams::GrooveRough { feed, .. }
        | OpParams::GrooveFinish { feed, .. }
        | OpParams::Part { feed, .. }
        // Threading's feed is the helix's own travel per minute -
        // the synchronized spindle carries the distance-per-minute
        // meaning unchanged (M5.2 D), and so does a contour ride:
        // the skin's own tangency pace, station by station (E).
        | OpParams::Threading { feed, .. }
        | OpParams::Chamfering { feed, .. }
        | OpParams::TaperTurning { feed, .. }
        // A gauge owns no tangential ride: its feed is the plunge's
        // number, stated but inert, the blade trio's precedent
        // (M5.2 E2).
        | OpParams::MultiPassTurning { feed, .. }
        // The boring bar rides the wall tangentially station by
        // station - the contour twins' pace, read inside out
        // (M5.2 F).
        | OpParams::BoringTurning { feed, .. }
        // The wall ride and the helix feed along their circles like
        // every 2D ride (M2.4 C).
        | OpParams::Bore { feed, .. }
        | OpParams::Thread { feed, .. }
        | OpParams::FormTurning { feed, .. } => *feed,
        // The seek feed is the probe's travel pace (#108).
        OpParams::Probe { fast_feed, .. } => *fast_feed,
        // A blank plasma feed is the controller's (#670): no number.
        OpParams::PlasmaProfile(p) => p.feed.unwrap_or(0.0),
    }
}

fn plunge_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { plunge_feed, .. }
        | OpParams::Contour { plunge_feed, .. }
        | OpParams::Pocket { plunge_feed, .. }
        | OpParams::Adaptive2D { plunge_feed, .. }
        | OpParams::Offset2D { plunge_feed, .. }
        // The engrave twin plunges into the line like the trace; the
        // chamfer ride enters over the shoulder at the plunge number;
        // the circular ladder plunges through open disk like the
        // pocket (M2.4 B).
        | OpParams::Engrave { plunge_feed, .. }
        | OpParams::Chamfer2D { plunge_feed, .. }
        | OpParams::Circular { plunge_feed, .. }
        | OpParams::Pocket3D { plunge_feed, .. }
        | OpParams::Slot { plunge_feed, .. }
        | OpParams::TrochoidalSlot { plunge_feed, .. }
        | OpParams::Trace { plunge_feed, .. }
        | OpParams::Drill { plunge_feed, .. }
        | OpParams::Finish3D { plunge_feed, .. }
        // Deburr's plunge stands the tip on the edge from the
        // clearance plane - the chamfer's number, one dihedral over (M2.5 E B).
        | OpParams::Deburr { plunge_feed, .. }
        | OpParams::Contour5D { plunge_feed, .. }
        | OpParams::Parallel5D { plunge_feed, .. }
        | OpParams::Swarf { plunge_feed, .. }
        | OpParams::AdvancedSwarf { plunge_feed, .. }
        | OpParams::RotaryContour { plunge_feed, .. }
        | OpParams::RotaryParallel { plunge_feed, .. }
        | OpParams::RotaryOffset { plunge_feed, .. }
        | OpParams::MultiAxisRough { plunge_feed, .. }
        | OpParams::Flow5D { plunge_feed, .. }
        | OpParams::MultiAxisContour { plunge_feed, .. }
        // Facing's plunge is the radial approach off the rapids plane
        // onto the station - the same number the ladder's first move
        // posts at (M5.2 A). The profile kinds plunge the same way:
        // radially in to the rung's ride, radially out at the end
        // (M5.2 B).
        | OpParams::Facing { plunge_feed, .. }
        | OpParams::ProfileRough { plunge_feed, .. }
        | OpParams::ProfileFinish { plunge_feed, .. }
        // The blade's plunge IS its cut - the radial sink every rung
        // eats at (M5.2 C).
        | OpParams::GrooveRough { plunge_feed, .. }
        | OpParams::GrooveFinish { plunge_feed, .. }
        // Threading's plunge is the rung's radial sink - the blade's
        // law, one ladder deeper (M5.2 D). The contour rides lead
        // in and out radially by the profile ride's own law (E).
        | OpParams::Part { plunge_feed, .. }
        | OpParams::Threading { plunge_feed, .. }
        | OpParams::Chamfering { plunge_feed, .. }
        | OpParams::TaperTurning { plunge_feed, .. }
        // The crown's sink is this kind's whole program: the plunge
        // feed is the only move the gauge makes under power
        // (M5.2 E2).
        | OpParams::MultiPassTurning { plunge_feed, .. }
        // The bar's plunge is its radial lead-out off the axis -
        // the same in-to-out approach read inward (M5.2 F).
        | OpParams::BoringTurning { plunge_feed, .. }
        // The dive down the hole's air and the feed-out to the wall
        // stand on this number (M2.4 C).
        | OpParams::Bore { plunge_feed, .. }
        | OpParams::Thread { plunge_feed, .. }
        | OpParams::FormTurning { plunge_feed, .. } => *plunge_feed,
        // The measuring re-probe is the slow number (#108).
        OpParams::Probe { slow_feed, .. } => *slow_feed,
        // The torch never plunges into solid: it pierces at the
        // controller's height and delay (#670).
        OpParams::PlasmaProfile(p) => p.feed.unwrap_or(0.0),
    }
}

fn climb_of(p: &OpParams) -> bool {
    match p {
        OpParams::Face { climb, .. }
        | OpParams::Contour { climb, .. }
        | OpParams::Pocket { climb, .. }
        | OpParams::Adaptive2D { climb, .. }
        | OpParams::Offset2D { climb, .. }
        // The engrave center runs on the line, which flank leads is
        // still the author's climb number; the chamfer cone rides the
        // wall like a contour; the circular rings are the pocket's
        // (M2.4 B).
        | OpParams::Engrave { climb, .. }
        | OpParams::Chamfer2D { climb, .. }
        | OpParams::Circular { climb, .. }
        | OpParams::Pocket3D { climb, .. }
        | OpParams::Slot { climb, .. }
        | OpParams::TrochoidalSlot { climb, .. }
        | OpParams::Trace { climb, .. }
        | OpParams::Finish3D { climb, .. }
        | OpParams::Contour5D { climb, .. }
        | OpParams::Parallel5D { climb, .. }
        | OpParams::Swarf { climb, .. }
        | OpParams::AdvancedSwarf { climb, .. }
        | OpParams::RotaryContour { climb, .. }
        | OpParams::RotaryParallel { climb, .. }
        | OpParams::RotaryOffset { climb, .. }
        | OpParams::MultiAxisRough { climb, .. }
        | OpParams::Flow5D { climb, .. }
        | OpParams::MultiAxisContour { climb, .. } => *climb,
        // A drill has no climb notion - re-typing one to a milling kind
        // starts from the fresh default instead of inventing a carry.
        // Facing's neither: the work spins and the insert traverses;
        // which flank meets the cut is the spindle's direction, not a
        // number this ladder authors (M5.2 A) - and neither do the
        // profile rides, which are the same kinematic (M5.2 B).
        OpParams::Drill { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // Nor the blades: the band's two edges meet the walls
        // alike - which flank shears is again the spindle's
        // direction (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Nor the wave: the V's two flanks alternate with the spin,
        // and which one leads is the spindle's direction (M5.2 D).
        | OpParams::Part { .. }
        // Nor the contour rides: the nose touches a face of
        // revolution tangentially - which side of the skin it
        // stands on is the spindle's whole story (M5.2 E).
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // Nor the crown gauge: it stands at one station while the
        // work turns - which face of the skin the band rests on is
        // the spindle's whole story (M5.2 E2).
        | OpParams::MultiPassTurning { .. }
        // Nor the boring bar: the nose stands on the wall while the
        // work turns - which flank of the ball carries the chip is
        // the spindle's whole story, inward as outward (M5.2 F).
        | OpParams::BoringTurning { .. }
        | OpParams::FormTurning { .. }
        // Nor the deburr ride: the cone stands OVER the edge and the
        // chain's welded direction says which way the run goes -
        // which flank breaks the burr is the edge's own geometry,
        // not a number an author could flip (M2.5 E B).
        | OpParams::Deburr { .. } => true,
        // The wall ride and the helix choose their flank: the climb
        // number says which way the circle runs (M2.4 C).
        OpParams::Bore { climb, .. } | OpParams::Thread { climb, .. } => *climb,
        OpParams::Probe { .. } | OpParams::PlasmaProfile(_) => false,
    }
}

fn stepover_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Face { stepover, .. }
        | OpParams::Pocket { stepover, .. }
        | OpParams::Adaptive2D { stepover, .. }
        | OpParams::Offset2D { stepover, .. }
        // The circular ladder's ring spacing is the pocket's plain
        // number (M2.4 B).
        | OpParams::Circular { stepover, .. }
        | OpParams::Pocket3D { stepover, .. }
        // The wrap ladder's spacing is a plain number like the
        // pocket's, not the twins' derived Option (variant docs).
        | OpParams::RotaryOffset { stepover, .. }
        // The stamped ladder's spacing is the pocket's plain number.
        | OpParams::MultiAxisRough { stepover, .. }
        | OpParams::Slot { stepover, .. } => Some(*stepover),
        // The trochoid's forward step is no stepover: a retype seeds.
        OpParams::TrochoidalSlot { .. } => None,
        // Parallel5D's spacing - and the swarf kinds', which run the
        // same derivation - is an Option the same way Finish3D's
        // raster's is: a derivation by default, so the fold reads the
        // field as "not authored by a number to carry" - the retype
        // starts a fresh derived line set, never a borrowed override.
        // The engrave ride follows one chain - the trace's answer,
        // no spacing to carry; the chamfer is one ride of the cone,
        // the authored wall is the pattern (M2.4 B).
        OpParams::Contour { .. }
        | OpParams::Engrave { .. }
        | OpParams::Chamfer2D { .. }
        | OpParams::Trace { .. }
        | OpParams::Drill { .. }
        | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        // Flow authors its spacing the twins' way: derived unless a
        // number is typed over it (M5.1 I). The drive-curve contour
        // has NO spacing at all - the picked chain is the path, one
        // pass over it (M5.1 J).
        | OpParams::Flow5D { .. }
        // The drive-curve contour has NO spacing at all, and facing
        // none either: one radial pass per station runs the profile's
        // whole width - the stepdown IS the spacing (M5.2 A). The
        // profile kinds say the same: the rungs' spacing is the
        // stepdown, the finish's pattern is the skin itself.
        | OpParams::MultiAxisContour { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // A blade has no spacing either: its width IS the pattern's
        // step, tiled by the kernel's tangency law (M5.2 C) - and a
        // threading pass has no spacing a number could name: the
        // helix's PITCH is the lattice, authored here as the thread's
        // own lead, not a stepover (M5.2 D). A contour ride has none
        // either: the chord law the tolerance settles spaces the
        // walk, and the model is the whole pattern (M5.2 E).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // A gauge tiles nothing: one crown stands on one modeled
        // valley, the model's own spacing (M5.2 E2). The boring bar
        // tiles nothing either - the wall it rides is one continuous
        // hole, and the ride is the pattern (M5.2 F).
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        // One ring per hole is the boring answer, one helix per hole
        // the thread's: the wall is the pattern, nothing tiles (M2.4 C).
        | OpParams::Bore { .. }
        | OpParams::Thread { .. }
        // One ride per exposed edge is the deburr answer: the model's
        // own edges are the pattern, nothing tiles (M2.5 E B).
        | OpParams::Deburr { .. } => None,
        OpParams::Probe { .. } | OpParams::PlasmaProfile(_) => None,
    }
}

fn stepdown_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Contour { stepdown, .. }
        | OpParams::Pocket { stepdown, .. }
        | OpParams::Adaptive2D { stepdown, .. }
        | OpParams::Offset2D { stepdown, .. }
        // The engrave twin steps down through the sheet like the
        // trace; the circular ladder is the pocket's rung law
        // (M2.4 B). The chamfer has no stepdown arm: its depth is
        // the wall's own, derived (below, in the None list).
        | OpParams::Engrave { stepdown, .. }
        | OpParams::Circular { stepdown, .. }
        | OpParams::Pocket3D { stepdown, .. }
        // The ladder's infeed is radial here, but it is still the
        // number the retype carries.
        | OpParams::RotaryOffset { stepdown, .. }
        // Z infeed here, the pocket's law (M5.1 stage E3).
        | OpParams::MultiAxisRough { stepdown, .. }
        // Radial infeed here, the wrap ladder's law, read down the
        // axis instead of out of the skin (M5.2 A).
        | OpParams::Facing { stepdown, .. }
        // The rung ladder's spacing is radial too - the same number
        // read as a shell width off the skin (M5.2 B).
        | OpParams::ProfileRough { stepdown, .. }
        // The blade's infeed is radial too - the rung's sink, and
        // the parting ladder's (M5.2 C).
        | OpParams::GrooveRough { stepdown, .. }
        | OpParams::Part { stepdown, .. }
        // The multi-pass ladder is the profile rough's rung law
        // restated - the same shell width, the same nose-reach
        // ceiling (M5.2 E).
        | OpParams::MultiPassTurning { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::TrochoidalSlot { stepdown, .. }
        // The boring ladder's axial bite is the pocket's rung law
        // spoken on one circle (M2.4 C).
        | OpParams::Bore { stepdown, .. }
        | OpParams::Trace { stepdown, .. } => Some(*stepdown),
        OpParams::Face { .. } | OpParams::Drill { .. } | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. } | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. } | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. } | OpParams::RotaryParallel { .. }
        | OpParams::Flow5D { .. }
        // One ride is the whole finish - there is no infeed to carry,
        // and one plunge per center is the groove finish's answer too.
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's ladder spacing is INFEED, its own field - the
        // retype carries no stepover number for it (M5.2 D). A
        // chamfer and a taper are one ride each: no infeed exists
        // between the skin and the promise to step (M5.2 E).
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // A gauge plunge is one sink: the crown's shape, not a
        // stepped infeed, is the whole descent (M5.2 E2). One ride
        // of the wall is the boring answer too - no rough ladder
        // exists yet between the hole's present size and its own
        // skin (M5.2 F).
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        | OpParams::MultiAxisContour { .. }
        // The 2-D chamfer is one ride: between the shoulder and the
        // derived depth there is no infeed to step (M2.4 B). The
        // thread's descent is its PITCH - infeed is its own field,
        // the threading law restated on a mill (M2.4 C).
        | OpParams::Chamfer2D { .. }
        | OpParams::Thread { .. } => None,
        // One ride of the edge is the deburr answer too: the tip's
        // depth is derived from width and the tool's cone, and
        // between the clearance plane and the edge there is no
        // infeed ladder to step (M2.5 E B).
        OpParams::Deburr { .. } => None,
        OpParams::Probe { .. } | OpParams::PlasmaProfile(_) => None,
    }
}

/// The section plane, present or not (Face has no `top_z` field - its
/// region rides `region_height`). `crates/ui`'s height wording reads it.
#[must_use]
pub fn top_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Adaptive2D { top_z, .. }
        | OpParams::Offset2D { top_z, .. }
        // The engrave and circular tops are the pocket family's; the
        // chamfer's top plane is where the cone's apex stands - the
        // ride's only height (M2.4 B).
        | OpParams::Engrave { top_z, .. }
        | OpParams::Chamfer2D { top_z, .. }
        | OpParams::Circular { top_z, .. }
        | OpParams::Pocket3D { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::TrochoidalSlot { top_z, .. }
        | OpParams::Trace { top_z, .. }
        | OpParams::Drill { top_z, .. }
        | OpParams::Finish3D { top_z, .. }
        | OpParams::Contour5D { top_z, .. }
        | OpParams::Parallel5D { top_z, .. }
        | OpParams::Swarf { top_z, .. }
        | OpParams::AdvancedSwarf { top_z, .. }
        | OpParams::RotaryContour { top_z, .. }
        | OpParams::RotaryParallel { top_z, .. }
        | OpParams::RotaryOffset { top_z, .. }
        | OpParams::MultiAxisRough { top_z, .. }
        | OpParams::Flow5D { top_z, .. }
        | OpParams::MultiAxisContour { top_z, .. }
        // Facing's top is the first station above the shoulder the
        // ladder steps down from (M5.2 A); the profile kinds' is the
        // high end of the window their rungs ride (M5.2 B).
        | OpParams::Facing { top_z, .. }
        | OpParams::ProfileRough { top_z, .. }
        | OpParams::ProfileFinish { top_z, .. }
        // The blade windows' high end; parting's is the bar's own
        // top the ladder rises from (M5.2 C).
        | OpParams::GrooveRough { top_z, .. }
        | OpParams::GrooveFinish { top_z, .. }
        | OpParams::Part { top_z, .. }
        // The threading window's high end: the first turn's station
        // the lattice begins from (M5.2 D). The contour window's
        // high end is the face's own upper corner (M5.2 E).
        | OpParams::Threading { top_z, .. }
        | OpParams::Chamfering { top_z, .. }
        | OpParams::TaperTurning { top_z, .. }
        // The form window's top: the highest crown the gauge sinks
        // stands at or below this station (M5.2 E2). The bore
        // window's top: where the bar's nose stops riding the wall.
        | OpParams::MultiPassTurning { top_z, .. }
        | OpParams::FormTurning { top_z, .. }
        | OpParams::BoringTurning { top_z, .. }
        // The holes' entry plane: where the feed move opens (M2.4 C).
        | OpParams::Bore { top_z, .. }
        | OpParams::Thread { top_z, .. }
        // The window's tip ceiling: rides whose TIP stands above it
        // are clipped there (M2.5 E B).
        | OpParams::Deburr { top_z, .. } => Some(*top_z),
        OpParams::Face { .. } => None,
        OpParams::Probe { .. } => None,
        // The plasma cut rides the plate's top (#670).
        OpParams::PlasmaProfile(p) => Some(p.top_z),
    }
}

/// The finished plane, present or not (Face none, Drill only an explicit
/// Depth override). Read by `Carried::of` and by the height wording.
#[must_use]
pub fn bottom_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { bottom_z, .. }
        | OpParams::Pocket { bottom_z, .. }
        | OpParams::Adaptive2D { bottom_z, .. }
        | OpParams::Offset2D { bottom_z, .. }
        // The engrave twin and the circular ladder both finish on an
        // authored plane; the chamfer's floor is the wall's own,
        // derived from width and angle - no plane to carry (M2.4 B).
        | OpParams::Engrave { bottom_z, .. }
        | OpParams::Circular { bottom_z, .. }
        | OpParams::Pocket3D { bottom_z, .. }
        | OpParams::Slot { bottom_z, .. }
        | OpParams::TrochoidalSlot { bottom_z, .. }
        | OpParams::Trace { bottom_z, .. }
        | OpParams::Finish3D { bottom_z, .. }
        | OpParams::Contour5D { bottom_z, .. }
        | OpParams::Parallel5D { bottom_z, .. }
        | OpParams::Swarf { bottom_z, .. }
        | OpParams::AdvancedSwarf { bottom_z, .. }
        | OpParams::RotaryContour { bottom_z, .. }
        | OpParams::RotaryParallel { bottom_z, .. }
        | OpParams::RotaryOffset { bottom_z, .. }
        | OpParams::MultiAxisRough { bottom_z, .. }
        | OpParams::Flow5D { bottom_z, .. }
        | OpParams::MultiAxisContour { bottom_z, .. }
        // Facing's finished plane: the facing Z every station
        // steps toward (M5.2 A); the profile kinds' is the low end
        // of the station window, the plunge-in end of the ride
        // (M5.2 B).
        | OpParams::Facing { bottom_z, .. }
        | OpParams::ProfileRough { bottom_z, .. }
        | OpParams::ProfileFinish { bottom_z, .. }
        // The blade windows' low end - and parting's plane itself,
        // the station every rung sinks toward (M5.2 C).
        | OpParams::GrooveRough { bottom_z, .. }
        | OpParams::GrooveFinish { bottom_z, .. }
        | OpParams::Part { bottom_z, .. }
        // The threading window's low end - the last center's station
        // and the whole turns whose lattice it completes (M5.2 D).
        // The contour window opens at the face's low corner, where
        // the ride plunges in (M5.2 E).
        | OpParams::Threading { bottom_z, .. }
        | OpParams::Chamfering { bottom_z, .. }
        | OpParams::TaperTurning { bottom_z, .. }
        // The form window's bottom: the lowest valley run's station
        // stands at or above it (M5.2 E2). The bore window's bottom:
        // the station the bar plunges in at, floor-reach refused
        // below it (M5.2 F).
        | OpParams::MultiPassTurning { bottom_z, .. }
        | OpParams::FormTurning { bottom_z, .. }
        | OpParams::BoringTurning { bottom_z, .. }
        // The window's tip floor: an edge running below it is clipped
        // by the edge's own line, and the ride stops there (M2.5 E B).
        | OpParams::Deburr { bottom_z, .. } => Some(*bottom_z),
        // A drill's floor is normally the hole's own, from the model;
        // only an explicit Depth override is a plane to carry over.
        // Bore and Thread keep the drill's law: the floors come from
        // the recognized holes; only an explicit override is a plane
        // the retype carries (M2.4 C).
        OpParams::Drill { bottom_z, .. }
        | OpParams::Bore { bottom_z, .. }
        | OpParams::Thread { bottom_z, .. } => *bottom_z,
        // The chamfer's floor is derived - width · tan(angle) below
        // the top plane the cone's apex stands at (M2.4 B).
        OpParams::Chamfer2D { .. } => None,
        OpParams::Face { .. } => None,
        OpParams::Probe { .. } => None,
        // The plate's bottom: the thickness the small-hole rule reads.
        OpParams::PlasmaProfile(p) => Some(p.bottom_z),
    }
}

/// The between-pass hop plane, present or not: Contour always stores one,
/// the other hopped kinds keep it in an option (None hops to clearance).
fn retract_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { retract_z, .. } => Some(*retract_z),
        OpParams::Pocket { retract_z, .. }
        | OpParams::Adaptive2D { retract_z, .. }
        | OpParams::Offset2D { retract_z, .. }
        // The three new kinds hop between loops like their parents:
        // the trace, the contour and the pocket (M2.4 B).
        | OpParams::Engrave { retract_z, .. }
        | OpParams::Chamfer2D { retract_z, .. }
        | OpParams::Circular { retract_z, .. }
        | OpParams::Pocket3D { retract_z, .. }
        | OpParams::Slot { retract_z, .. }
        | OpParams::TrochoidalSlot { retract_z, .. }
        | OpParams::Trace { retract_z, .. }
        | OpParams::Drill { retract_z, .. }
        // Both new rides hop between holes at the authored plane,
        // the drill's own answer (M2.4 C).
        | OpParams::Bore { retract_z, .. }
        | OpParams::Thread { retract_z, .. }
        | OpParams::Finish3D { retract_z, .. }
        | OpParams::Contour5D { retract_z, .. }
        | OpParams::Parallel5D { retract_z, .. }
        | OpParams::Swarf { retract_z, .. }
        | OpParams::AdvancedSwarf { retract_z, .. }
        | OpParams::RotaryContour { retract_z, .. }
        | OpParams::RotaryParallel { retract_z, .. }
        // The stamped ladder hops like the pocket it stamps (E3).
        | OpParams::MultiAxisRough { retract_z, .. }
        | OpParams::Flow5D { retract_z, .. }
        // The drive-curve contour hops between chains like the
        // raster hops between lines (M5.1 J).
        | OpParams::MultiAxisContour { retract_z, .. } => *retract_z,
        // No hop plane to carry: the kernel names its own rapids
        // plane (one infeed and a tool radius off the skin - the
        // facing ladder states it one infeed clear of the profile,
        // the axis way; the profile rungs state theirs the same way,
        // a chord-tolerance clear of the ride's reach), and Face
        // never stored one either.
        OpParams::Face { .. }
        | OpParams::RotaryOffset { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // The blades name their rapids plane too: the whole bar's
        // radius plus tolerance, clear of every shoulder the band
        // traverses over (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's rapids plane is the clearance height itself -
        // the whole bar's radius clears every turn's crest the pass
        // traverses over (M5.2 D). The contour rides name theirs off
        // the ride's own reach, a tolerance clear of the highest CL
        // point, and author no hop (M5.2 E).
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // The gauge rapids plane is the whole bar's radius plus
        // tolerance, clear of every crest the crowns pass over
        // under rapids (M5.2 E2). The boring bar names its own:
        // it comes OFF the wall to the axis between the lead-out
        // and the lead-back-in, and travels home at the clearance
        // height - no hop plane exists between the ride's ends
        // (M5.2 F).
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        // The deburr ride hops every run to the clearance plane -
        // the kernel's own word, no between-run plane authored (M2.5 E B).
        | OpParams::Deburr { .. } => None,
        OpParams::Probe { .. } | OpParams::PlasmaProfile(_) => None,
    }
}

// The seeds, one fn per row: what quick-add starts, and what a retype
// builds from the carried numbers. Bodies are M1.5.2's draft rule, moved
// verbatim out of `crates/ui/src/operations.rs::retype` (M5.0.2) - the
// comments there explain every default.

fn seed_face(c: &Carried) -> OpParams {
    OpParams::Face {
        stepover: c.stepover,
        clearance_z: c.clearance,
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        // A fresh face cuts one level; staging is opt-in in the
        // panel, never a surprise default.
        top_z: None,
        stepdown: None,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        // Turning-point blends are opt-in (M11.1 A): a fresh face is
        // the sharp serpentine every golden and every saved file knows.
        blend_turns: false,
        // M13.11: a fresh face lifts over its holes - a bore or a boss
        // is bridged in the air; cutting across is the user's choice.
        holes: Default::default(),
        tilt: c.tilt,
    }
}

fn seed_contour(c: &Carried) -> OpParams {
    OpParams::Contour {
        side: Side::Auto,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: c.retract,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        // M8.2 A: the wall ladder seeds at the graze the kind always
        // posted; the numbers ride the pockets' defaults so a switched
        // entry starts from a legal one.
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        // Turning-point blends are opt-in (M11.1 B), as on Face.
        blend_turns: false,
        // Retract and re-enter: the transition every earlier contour
        // posted; staying down is the user's pick on Lead in / Lead out.
        links: crate::links::Links::PLANAR,
        tilt: c.tilt,
    }
}

fn seed_pocket(c: &Carried) -> OpParams {
    OpParams::Pocket {
        links: crate::links::Links::PLANAR,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        // New pockets let the shape come from the outline; explicit
        // choices on live operations are never retyped over. The entry
        // family seeds at the dive - a new pocket posts what pockets
        // always posted until M5.1 H made the choice exist; the numbers
        // ride the 3-D pocket's defaults so a switched strategy or
        // entry starts from a legal one.
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Helix, // M11.3: new pocket-family ops helix in (ramp, then plunge, stand in)
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        // Turning-point blends are opt-in (M11.1 C), as on Face.
        blend_turns: false,
        tilt: c.tilt,
    }
}

/// 2D Adaptive (M2.4 A): the pocket seed with the strategy pinned out
/// of the params - the kind IS the engagement ladder, so the numbers
/// ride the pocket's defaults and only the door the strategy hides
/// (optimal_load) stays visible.
fn seed_adaptive2d(c: &Carried) -> OpParams {
    OpParams::Adaptive2D {
        links: crate::links::Links::PLANAR,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        entry: Entry3D::Helix, // M11.3: new pocket-family ops helix in (ramp, then plunge, stand in)
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        tilt: c.tilt,
    }
}

/// 2D Offset (M2.4 A): same seed minus the engagement door - style-A
/// rings are spaced by the stepover alone, so nothing hides a field
/// the pane must not show.
fn seed_offset2d(c: &Carried) -> OpParams {
    OpParams::Offset2D {
        links: crate::links::Links::PLANAR,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        entry: Entry3D::Helix, // M11.3: new pocket-family ops helix in (ramp, then plunge, stand in)
        helix_radius: 1.0,
        tilt: c.tilt,
    }
}

/// Engrave (M2.4 B): the trace's seed verbatim - the twin law again,
/// the name carries the shop task, the numbers ride the same law.
fn seed_engrave(c: &Carried) -> OpParams {
    OpParams::Engrave {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tilt: c.tilt,
    }
}

/// 2D Chamfer (M2.4 B): the shop's chamfer seed - a 0.8 mm land at
/// 45°, the drawing's commonest edge, and the contour's chord budget
/// so the offset center line arcs as the boundary does.
fn seed_chamfer2d(c: &Carried) -> OpParams {
    OpParams::Chamfer2D {
        width: 0.8,
        angle_deg: 45.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        side: Side::Auto,
        tolerance: 0.01,
        tilt: c.tilt,
    }
}

/// Circular (M2.4 B): a disk the pane's author can see - 25 mm
/// radius flat floors at the pocket family's numbers.
fn seed_circular(c: &Carried) -> OpParams {
    OpParams::Circular {
        links: crate::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius: 25.0,
        vault: false,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: c.tilt,
    }
}

fn seed_slot(c: &Carried) -> OpParams {
    OpParams::Slot {
        links: crate::links::Links::PLANAR,
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        width: 4.0,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        stepover: c.stepover,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: c.tilt,
    }
}

/// The trochoidal slot's draft (#220): the Slot's authored line at a
/// width that leaves a 10 mm cutter room to loop, a 10%-of-D step and
/// a 90 degree limit; the planner cuts the step if the stock needs it.
fn seed_trochoidal_slot(c: &Carried) -> OpParams {
    OpParams::TrochoidalSlot {
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        width: 16.0,
        loop_width: None,
        step: 1.0,
        max_engagement: crate::machining2d::DEFAULT_MAX_ENGAGEMENT,
        ret: crate::machining2d::TrochoidReturn::Microlift { height: crate::machining2d::DEFAULT_MICROLIFT },
        smoothing: 1.0,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        return_feed: c.feed * 2.0,
        plunge_feed: c.plunge,
        climb: c.climb,
    }
}

fn seed_trace(c: &Carried) -> OpParams {
    OpParams::Trace {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tilt: c.tilt,
    }
}

fn seed_drill(c: &Carried) -> OpParams {
    OpParams::Drill {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        // No shared bottom: each hole's floor rides the hole itself
        // (or the explicit Depth field if one is set); the G54
        // default below would be a plane no drilled hole implies.
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: c.feed,
        plunge_feed: c.plunge,
        // Creation is legal with an empty list; the generator
        // refuses to run one, and Detect holes fills it.
        holes: Vec::new(),
        cross: Vec::new(),
    }
}

/// The standalone bore's seed (M2.4 C): the drill's law verbatim -
/// the entry plane carried, no shared bottom (each hole's floor
/// rides the hole), the hole list filled by recognition. The
/// tolerance is the wall circle's arc-fit budget, the house
/// finishing value; the entry family stays the plain dive until
/// the boring pane grows the choice (the kernel takes it).
fn seed_standalone_bore(c: &Carried) -> OpParams {
    OpParams::Bore {
        links: crate::links::Links::PLANAR,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tolerance: 0.01,
        holes: Vec::new(),
    }
}

/// The thread mill's seed (M2.4 C): the drill's four heights again.
/// The PITCH is the tool's and rides no params - the tap cycle's
/// law. The infeed starts small: the validator caps it at the
/// standard's depth for whatever pitch the mounted tool states,
/// and the pane lets the author step more per turn if they wish.
/// Climb opens CCW: the standard read for a right-hand thread.
fn seed_thread_mill(c: &Carried) -> OpParams {
    OpParams::Thread {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: None,
        infeed: 0.1,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: true,
        holes: Vec::new(),
    }
}

/// The probe's draft (#108): a -Z face touch at the model origin,
/// setting G54 - the picked geometry re-seats `target`, `mode` and the
/// directions; the numbers are general touch-probe starting values.
fn seed_probe(c: &Carried) -> OpParams {
    use crate::probing::{ProbeDir, ProbeMode};
    OpParams::Probe {
        clearance_z: c.clearance,
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0, 0.0, 0.0],
        radius: 10.0,
        approach: 5.0,
        // #570: a search limit, not a destination - twice the 0.5 mm
        // tolerance below, so a face inside tolerance always trips well
        // before it, and a missing one alarms 1 mm past, not 5.
        overtravel: crate::operation::probe_op::default_overtravel(0.5),
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    }
}

fn seed_pocket3d(c: &Carried) -> OpParams {
    OpParams::Pocket3D {
        links: crate::links::Links::ROUGH3D,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        strategy: PocketStrategy::Auto,
        // The dive the 2-D pocket already makes; ramp and helix are
        // opt-in once the walls are known to allow them.
        entry: Entry3D::Helix, // M11.3: new pocket-family ops helix in (ramp, then plunge, stand in)
        helix_radius: 1.0,
        // The staircase is honest noise until the user asks for
        // the closing pass: rest roughing costs a second ladder.
        rest_rough: false,
        // The law's default (r·(1−cos 45°) ≈ 15% of the diameter);
        // reads only under Adaptive, which the fresh op does not pick.
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        // Feed modulation costs a sim run; the fresh op posts the
        // programmed feed everywhere until the user asks otherwise.
        feed_opt: false,
        // Off is the ladder exactly as it always generated; the shell
        // is the user's ask, not the fresh op's guess (M2.5 F).
        shell: false,
    }
}

fn seed_finish3d(c: &Carried) -> OpParams {
    OpParams::Finish3D {
        pattern: FinishPattern::Parallel,
        // None = the stepover cos-law derives spacing from the
        // tolerance; the panel can override it per line set.
        stepover: None,
        // A general 3-D finish budget: half the common 0.1 mm shop
        // tolerance - coarse enough that the scallop stepdown on a
        // typical R3 ball stays well under the level cap, fine enough
        // that the rings read as a finish.
        tolerance: 0.05,
        clearance_z: c.clearance,
        // None hops rings to clearance - the pocket family's law.
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        // Fresh finishes hop: the ramp is opt-in, like the pocket's
        // between-ring entry - the certified diagonal is a claim
        // about the ground, and a new op has none on record yet.
        ramp: false,
        // The march refines to its own floor until a machinist says
        // otherwise; the ramp's slope cap waits for a ramp to govern,
        // at the law's own 45 degrees.
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        // The ladder rides the scallop law until a machinist authors
        // the flank's own spacing (M2.5 A) - and only the Steep-and-
        // shallow pattern would ever read it.
        steep_stepover: None,
        pencil_reference: None,
    }
}

fn seed_deburr(c: &Carried) -> OpParams {
    OpParams::Deburr {
        // A shop hand-break knows this one: half a millimetre of
        // shine off a sharp edge is the deburr, not a chamfer. The
        // tip's depth follows from the tool's cone - derived, never
        // a second number to disagree (variant docs, M2.5 E B).
        width: 0.5,
        clearance_z: c.clearance,
        // The window is the model's own height: every exposed edge
        // stands inside it until a machinist narrows the ride.
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The 5-axis stage C seeds: the finish seed's numbers with the pose
/// at rest - lead and lag 0 (straight up the normal, the identity
/// pose) and a 90-degree cone (the reach the machine letters refuse
/// at posting anyway; the cone only folds when a lean is asked for).
/// A retype FROM a tilted planar op loses the index with the kind
/// that owned it, like every non-planar seed (`Carried`'s tilt note).
fn seed_contour5d(c: &Carried) -> OpParams {
    OpParams::Contour5D {
        tolerance: 0.05,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

fn seed_parallel5d(c: &Carried) -> OpParams {
    OpParams::Parallel5D {
        tolerance: 0.05,
        // None = the stepover cos-law derives spacing from the
        // tolerance, the finish's own law.
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

/// The flowline seed (M5.1 I): the parallel's numbers whole - a flow
/// pass IS the raster's law on lines that turn, so what a fresh one
/// starts with is what a fresh parallel starts with. The direction
/// field is not a parameter: the generator reads the surface's own
/// least-curving direction off the model, and plain ground degrades
/// to the straight passes silently (the kernel's fallback law).
fn seed_flow5d(c: &Carried) -> OpParams {
    OpParams::Flow5D {
        tolerance: 0.05,
        // None = the stepover cos-law derives spacing from the
        // tolerance, the finish's own law.
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

/// The drive-curve seed (M5.1 J): the flow's numbers minus the
/// stepover - the spacing question does not exist when the chain IS
/// the path. What it DOES need that no seed can know is the
/// selection: a fresh one cuts nothing until edge chains are picked
/// in its Geometry section, and generation refuses by name until
/// then. The tolerance is the verify budget only (variant docs).
fn seed_multi_axis_contour(c: &Carried) -> OpParams {
    OpParams::MultiAxisContour {
        tolerance: 0.05,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

/// The M5.2 stage A seed: the facing ladder's numbers, read the
/// wrap's way - `stepdown` is the AXIAL infeed one station takes off
/// the end. The carried window is the station range: whatever way
/// the machine stands the bar, the face to cut lies between top and
/// bottom heights.
fn seed_facing(c: &Carried) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        // The one number NOT folded: an infeed is bounded by twice
        // the nose's reach (the kernel's law), and the mill default
        // the fold carries would refuse against the house insert's
        // 0.8 mm nose the moment a user created the op. 0.5 is the
        // handbook rough-facing infeed, and it stands under 2r of
        // every TN row in the catalog.
        stepdown: 0.5,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage B rough seed: the facing ladder's shape spoken
/// radially - the same stepdown 0.5 (under twice the house insert's
/// nose, the same law read as a shell width), plus the leave the
/// finish takes off. 0.25 is the handbook finish allowance: a whole
/// nose's worth of chord sag at tolerance 0.05 leaves far less, so
/// the ride's own budget, not this number, shapes the finish.
fn seed_profile_rough(c: &Carried) -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage B finish seed: the window and the feeds - there is
/// no infeed to answer for. The carried stepdown is deliberately
/// dropped: one ride IS the pattern.
fn seed_profile_finish(c: &Carried) -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage C rough seed: the profile rough's shape on a
/// blade. The stepdown is the same 0.5 - under twice the blade's
/// tip reaches nothing here (the band has no nose to chord), but it
/// is the handbook grooving infeed, and it stays far under any
/// real bar's column. The leave is the same 0.25: a full skin the
/// finish plunge takes off.
fn seed_groove_rough(c: &Carried) -> OpParams {
    OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The stage C finish seed: the window and the feeds - one plunge
/// per center is the whole pattern, so there is no infeed to
/// answer for (and no leave: the floor IS the model).
fn seed_groove_finish(c: &Carried) -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The parting seed: the bar's top and the parting plane carried
/// as the window pair, the rung ladder's 0.5 infeed, the feeds.
/// The plane defaults to the document's bottom - where a part gets
/// cut off from the bar it was turned on.
fn seed_part(c: &Carried) -> OpParams {
    OpParams::Part {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        feed: c.feed,
        plunge_feed: c.plunge,
        // The feeder signals are off by default: a seeded parting
        // operation says nothing the controller wasn't told to
        // expect (M5.2 G.2).
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
    }
}

/// The threading seed: the window and feeds carried, and the
/// commonest thread in the shop as the modeled numbers - a 2 mm
/// pitch, its ISO 0.541 x P depth, and a quarter-pitch-of-depth
/// infeed ladder (three or four rungs to the root). The V itself
/// comes from the tool, never authored here (M5.2 D).
fn seed_threading(c: &Carried) -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        pitch: 2.0,
        depth: 1.08,
        infeed: 0.4,
        feed: c.feed,
        plunge_feed: c.plunge,
        hand: crate::cl::ThreadHand::Right,
    }
}

/// The chamfer seed: the window and feeds carried, and the common
/// angle - 45 degrees is what a chamfer is unless the drawing says
/// otherwise (M5.2 E). The face it promises must BE in the model
/// within the tolerance; the door, not this number, owns that
/// question, so a wrong model refuses by name rather than cutting
/// a cone the drawing never had.
fn seed_chamfer(c: &Carried) -> OpParams {
    OpParams::Chamfering {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        angle_deg: 45.0,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The taper seed: the shop's mild cone - a metric-stationary
/// 20-to-16 across whatever window is carried, which is the
/// handbooks' commonest taper and the drawing's answer to a
/// collar. Diameters are the authored promise; the model keeps it
/// or the ride is refused (M5.2 E).
fn seed_taper(c: &Carried) -> OpParams {
    OpParams::TaperTurning {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        bottom_dia: 20.0,
        top_dia: 16.0,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The multi-pass seed: the rough rung numbers carried whole from
/// stage B (the same 0.5 under twice the house insert's nose, the
/// same 0.25 handbook leave) and the handbook's answer for a
/// finishing sequence - three spring rides, each taking half of
/// what the last one sprang (M5.2 E).
fn seed_multi_pass(c: &Carried) -> OpParams {
    OpParams::MultiPassTurning {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        passes: 3,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The form seed: the window carried whole and the feeds the shop
/// runs a sink on - there is no depth to author, because the model
/// states it, and no crown to name here, because the gauge is
/// TOOL steel (M5.2 E2).
fn seed_form(c: &Carried) -> OpParams {
    OpParams::FormTurning {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The boring seed: the window and feeds carried, the house
/// finishing tolerance, and nothing else - the wall the nose rides
/// is the hole the model already has, and the size it holds is
/// TOOL steel, the ground nose the bar carries (M5.2 F).
fn seed_bore(c: &Carried) -> OpParams {
    OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The stage D seeds: the raster's numbers whole - a swarf path IS
/// the raster run along the surface's own ruling, so what a fresh
/// one starts with is what a fresh parallel starts with, pose at
/// rest. The ruling is not a parameter: the generator reads it off
/// the model and refuses by name when the model has none.
fn seed_swarf(c: &Carried) -> OpParams {
    OpParams::Swarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// The ladder's default rungs: 2 - rough then finish. One would
/// make the kind a spelling of plain swarf, and the panel's whole
/// claim is the coarse-first pass; the generator's named range runs
/// to 4 (motion refuses past it).
fn seed_advanced_swarf(c: &Carried) -> OpParams {
    OpParams::AdvancedSwarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        passes: 2,
    }
}

/// The stage E seeds: the finish's numbers whole, the pose at rest -
/// lead and lag 0 stand the tool exactly on the RADIAL (the rotary
/// identity) and the 90-degree cone states the reach. The axis is not
/// a parameter: the generator fits it to the model and refuses by
/// name when the model says nothing of turning, like the ruling
/// before it.
fn seed_rotary_contour(c: &Carried) -> OpParams {
    OpParams::RotaryContour {
        tolerance: 0.05,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

fn seed_rotary_parallel(c: &Carried) -> OpParams {
    OpParams::RotaryParallel {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// The stage E3 seed: the pocket ladder's numbers verbatim - the
/// stamp moves no position, so the draft rule reads the SAME carried
/// stepover/stepdown/strategy the plain pocket does - crossed with
/// the twins' pose trio, defaulted inert (lead and lag at zero, cone
/// wide open: a fresh op poses by the tangency law alone until the
/// machinist asks for more).
fn seed_multiaxis_rough(c: &Carried) -> OpParams {
    OpParams::MultiAxisRough {
        links: crate::links::Links::ROUGH3D,
        tolerance: 0.05,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

/// The stage E2 seed: the pocket ladder's numbers, read the wrap's
/// way - `stepdown` is the radial infeed one level peels off the
/// skin, `stepover` the spacing around it in developed arc length.
/// The dive it starts with is the one the 2-D pocket already makes
/// and the twins' law allows: a plunge down the radial, no rest
/// fields the kernel cannot honor (variant docs in types::params).
fn seed_rotary_offset(c: &Carried) -> OpParams {
    OpParams::RotaryOffset {
        links: crate::links::Links::ROUGH3D,
        tolerance: 0.05,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// One kind, as data: its name, the digest byte `stamp_with` writes (the
/// tag was a magic literal in eight arms until M5.0.2 - it is order, and
/// order belongs in the table), the verify law the generator picks, and
/// the retype seed. The row order is the product order: combo index,
/// ribbon index, `OpType::ordinal` - all one sequence (the law the UI
/// section tests quote).
pub struct KindInfo {
    pub ty: OpType,
    pub label: &'static str,
    /// The ribbon cell's short caption - the product's own UI dialect.
    /// It was imgui's wording kept verbatim until M6.2 restructured the
    /// ribbon; the promise is retired in RECOMMENDED.md, the strings
    /// stay (a caption is a name, and names do not need churn).
    pub caption: &'static str,
    pub tag: u8,
    pub mode: Mode,
    /// Which catalog section the kind belongs to (M6.2): the ribbon's
    /// family sections, the type combo's grouping, and the machine
    /// pairing all read this one field - `family` below is derived from
    /// it, so no second taxonomy can drift from the first.
    pub section: Section,
    pub seed: fn(&Carried) -> OpParams,
    /// The tool kinds that can machine this kind (M10.1 A) - one of the
    /// named sets in [`super::tool_law`]; never empty, and every
    /// `ToolKind` is claimed by some row (the law tests hold both).
    pub tool_kinds: &'static [ToolKind],
}

/// The catalog's taxonomy, as registry data (M6.2): the five sections
/// the ROADMAP's Operation Catalog heads divide into, so the ribbon's
/// family sections and the page's type combo quote the same table the
/// tree and the sheet already read - no second truth. `Family` (the
/// two machine kinds, M5.2 stage A) is derived, not stored: Turning is
/// the lathe's work, the four mill sections are the mill's, and a
/// mill-turn admits both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Mill2D,
    Mill3D,
    Drilling,
    MultiAxis,
    Turning,
    /// The plasma table's work (#670): the torch's 2D through-cuts.
    Plasma,
}

impl Section {
    /// The catalog's order, which is the ribbon's: the sections appear
    /// left to right in exactly this sequence, each one's kinds in the
    /// registry's own order (the row order is the product order).
    pub const ORDER: [Section; 6] = [
        Section::Mill2D,
        Section::Mill3D,
        Section::Drilling,
        Section::MultiAxis,
        Section::Turning,
        Section::Plasma,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Section::Mill2D => "Milling 2D",
            Section::Mill3D => "Milling 3D",
            Section::Drilling => "Drilling",
            Section::MultiAxis => "Multi-axis",
            Section::Turning => "Turning",
            Section::Plasma => "Plasma",
        }
    }

    /// Whose machine this section's kinds drive - the derivation that
    /// replaces the stored `family` field (M6.2): one taxonomy column,
    /// two read-outs, the command door and the ribbon cannot disagree.
    #[must_use]
    pub fn family(self) -> Family {
        match self {
            Section::Turning => Family::Turn,
            Section::Plasma => Family::Plasma,
            _ => Family::Mill,
        }
    }

    /// The kinds in this section, as registry indices (product order
    /// within the section). A surface that lists a section walks this,
    /// never a hand-typed slice of `KindInfo::ALL`.
    #[must_use]
    pub fn kinds(self) -> Vec<usize> {
        KindInfo::ALL
            .iter()
            .enumerate()
            .filter(|(_, k)| k.section == self)
            .map(|(i, _)| i)
            .collect()
    }

    /// The kinds of this section a document mounted on `machine` may
    /// create (M13.5 B): `kinds` filtered through [`KindInfo::admits`],
    /// the command door's own word - so the ribbon's section lists
    /// and the door cannot disagree about the lathe's Drill cell.
    #[must_use]
    pub fn kinds_for(self, machine: crate::machine::Kind) -> Vec<usize> {
        self.kinds()
            .into_iter()
            .filter(|i| KindInfo::ALL.get(*i).is_some_and(|k| k.admits(machine)))
            .collect()
    }
}

/// The sections a machine kind can run, in catalog order: a section
/// shows when any of its kinds is admitted by [`KindInfo::admits`] -
/// the mill document's ribbon never shows a Turning section, a
/// lathe's shows Drilling (the axial Drill, M13.5 A) and Turning, and
/// a mill-turn shows all five. This is the hiding law's data (M6.2) -
/// the ribbon's darkening (M3.2 E) is retired because cells for work
/// the steel cannot spin should not be there to darken. The command
/// door's `family_gate` reads the same per-kind word.
#[must_use]
pub fn shown_for(machine: crate::machine::Kind) -> Vec<Section> {
    Section::ORDER
        .into_iter()
        .filter(|s| !s.kinds_for(machine).is_empty())
        .collect()
}

/// The two machine families a document's `.mch` kind can belong to
/// (M5.2 stage A): a mill and its live tooling speak Mill, a lathe
/// speaks Turn, and a mill-turn's machine kind admits both. The
/// ribbon's darkening is a courtesy; this pairing, enforced at the
/// command door, is the law.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Mill,
    Turn,
    /// The plasma table's torch work (#670): admitted on a plasma
    /// table only - a mill-turn spins steel, it strikes no arc.
    Plasma,
}

impl Family {
    /// The pairing in one place (M6.2): the command door, the ribbon's
    /// hiding and the combo's filtering all read this - the table that
    /// used to be hand-typed twice (panes' `admits`, the door's
    /// `matches!`) is one function now.
    #[must_use]
    pub fn admits(self, machine: crate::machine::Kind) -> bool {
        use crate::machine::Kind;
        matches!(
            (self, machine),
            (Family::Mill | Family::Turn, Kind::MillTurn)
                | (Family::Mill, Kind::Mill)
                | (Family::Turn, Kind::Lathe)
                | (Family::Plasma, Kind::Plasma)
        )
    }

    /// The family's work, in the words a refusal names ("milling work").
    #[must_use]
    pub fn work(self) -> &'static str {
        match self {
            Family::Mill => "milling",
            Family::Turn => "turning",
            Family::Plasma => "plasma cutting",
        }
    }

    /// The machine the family's work needs, as a refusal names it.
    #[must_use]
    pub fn machine(self) -> &'static str {
        match self {
            Family::Mill => "mill",
            Family::Turn => "lathe",
            Family::Plasma => "plasma table",
        }
    }
}

impl KindInfo {
    pub const ALL: &'static [KindInfo] = &[
        KindInfo {
            ty: OpType::Face,
            label: "Face",
            caption: "Face",
            tag: 0,
            // Face trims the footprint silhouette: boundary laws rule.
            mode: Mode::Boundary,
            section: Section::Mill2D,
            seed: seed_face,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Contour,
            label: "2D Contour",
            caption: "Contour",
            tag: 1,
            mode: Mode::Boundary,
            section: Section::Mill2D,
            seed: seed_contour,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Pocket,
            label: "2D Pocket",
            caption: "Pocket",
            tag: 2,
            // The pocket hollows the region's inside: area laws rule.
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_pocket,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Slot,
            label: "Slot",
            caption: "Slot",
            tag: 3,
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_slot,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Trace,
            label: "Trace",
            caption: "Trace",
            tag: 4,
            // The center runs ON the line: tangency laws stand down.
            mode: Mode::Engrave,
            section: Section::Mill2D,
            seed: seed_trace,
            tool_kinds: tk::CENTERLINE,
        },
        KindInfo {
            ty: OpType::Adaptive2D,
            label: "2D Adaptive",
            caption: "Adaptive",
            tag: 5,
            // The pocket's inside-out law with the engagement door
            // open: rings spaced by constant side load, area rules
            // (M2.4 A - the catalog name over the pocket's ladder).
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_adaptive2d,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Offset2D,
            label: "2D Offset",
            caption: "Offset",
            tag: 6,
            // Concentric rings from the boundary inward at the
            // roughing stepover - style-A offsetting, area rules,
            // same machinery under a second catalog name (M2.4 A).
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_offset2d,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Engrave,
            label: "Engrave",
            caption: "Engrave",
            tag: 7,
            // The trace's twin (M2.4 B): the center runs ON the line,
            // the same tangency stand-down - the name speaks the shop
            // task, the kernel is Trace's verbatim.
            mode: Mode::Engrave,
            section: Section::Mill2D,
            seed: seed_engrave,
            tool_kinds: tk::CENTERLINE,
        },
        KindInfo {
            ty: OpType::Chamfer2D,
            label: "2D Chamfer",
            caption: "Chamfer",
            tag: 8,
            // One ride with the cone lying ON the wall: the axis runs
            // on the wall loop and the tool's congruence doors keep
            // the promise (M2.4 B). Mode::Cone (M2.5 E B): law (b)
            // once read that ride as five gouges per square - a
            // tangency law cannot convict a generatrix that lies ON
            // the face by construction - while the shank band still
            // answers for the wall beside the edge.
            mode: Mode::Cone,
            section: Section::Mill2D,
            seed: seed_chamfer2d,
            tool_kinds: tk::CONE,
        },
        KindInfo {
            ty: OpType::Circular,
            label: "Circular",
            caption: "Circular",
            tag: 9,
            // Rings hollow an authored disk: area laws rule, the
            // pattern's inside-out like the pocket's, the circles
            // authored like the slot's geometry (M2.4 B).
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_circular,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Thread,
            label: "Thread",
            caption: "Thread",
            tag: 10,
            // Boundary for the same reason as the bore ride: the
            // helix's disc must stay inside the minor cylinder, and
            // the rim's tangency at the major circle is the wall -
            // 3-axis interpolation, the milling family's own answer
            // (M2.4 C, closing the Mill2D run).
            mode: Mode::Boundary,
            section: Section::Mill2D,
            seed: seed_thread_mill,
            tool_kinds: tk::THREAD_MILL,
        },
        KindInfo {
            ty: OpType::Drill,
            label: "Drill",
            caption: "Drill",
            tag: 11,
            // Drill verifies as Boundary on purpose: its region's rim
            // loops put the gouge law's tangency test at the hole wall,
            // where a matched drill passes and an oversized tool is
            // flagged. What the move-by-move laws cannot see for a
            // straight deep plunge (it clears its own kerf, so the band
            // sampling never fires) the cycle reach law in `verify`
            // checks over `path.cycles`.
            mode: Mode::Boundary,
            section: Section::Drilling,
            seed: seed_drill,
            tool_kinds: tk::HOLE_CYCLES,
        },
        KindInfo {
            ty: OpType::Bore,
            label: "Bore",
            caption: "Bore",
            tag: 12,
            // The drill's rim-loop answer (M2.4 C): the wall the ring
            // rides IS the region, so the gouge law's tangency test
            // stands at the hole - a cutter with a circle left inside
            // passes, one too wide to turn is refused by name at the
            // door the same rows' data feeds.
            mode: Mode::Boundary,
            section: Section::Drilling,
            seed: seed_standalone_bore,
            tool_kinds: tk::FLAT_MILLS,
        },
        KindInfo {
            ty: OpType::Pocket3D,
            label: "3D Pocket",
            caption: "3D Pocket",
            tag: 13,
            // The 3-D pocket verifies as Area with the mesh in hand
            // (the `model` field `VerifyContext` carries): the region
            // is its silhouette, the per-level islands come from the
            // facets.
            mode: Mode::Area,
            section: Section::Mill3D,
            seed: seed_pocket3d,
            tool_kinds: tk::ROUGH_MILLS,
        },
        KindInfo {
            ty: OpType::Finish3D,
            label: "3D Finish",
            caption: "3D Finish",
            tag: 14,
            // The waterline's verdict is measured, not deduced from
            // tangency: law (f) of `verify` scans the swept ball against
            // the mesh (the `finish_tol` field carries the budget).
            mode: Mode::Envelope,
            section: Section::Mill3D,
            seed: seed_finish3d,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::Deburr,
            label: "Deburr",
            caption: "Deburr",
            tag: 15,
            // The cone lies ON the break the edge makes: the axis
            // runs on the edge's own XY and the virtual tip stands
            // inside the corner by construction - the 2-D chamfer's
            // doctrine read on a real dihedral (M2.5 E B). Law (b)
            // stands down; the shank band keeps its voice.
            mode: Mode::Cone,
            section: Section::Mill3D,
            seed: seed_deburr,
            tool_kinds: tk::CONE,
        },
        KindInfo {
            ty: OpType::Contour5D,
            label: "5-Axis Contour",
            caption: "5-Axis Contour",
            tag: 16,
            // Same measured envelope law (M5.1 stage C): the ball is
            // the cutter and a sphere's envelope is rotation-invariant,
            // so the pose changes what the SHANK sees, not what the
            // ball leaves - and the shank's question is the stage-A
            // leaning band law's, run on the stamped moves.
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_contour5d,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::Parallel5D,
            label: "5-Axis Parallel",
            caption: "5-Axis Parallel",
            tag: 17,
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_parallel5d,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::Swarf,
            label: "Swarf",
            caption: "Swarf",
            tag: 18,
            // The same measured envelope, read along the ruling (M5.1
            // stage D): the ball's path is the raster's, so the verdict
            // law is the finish's - the pose question (the blade lies
            // under a leaning shank) is the stage-A band law again.
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_swarf,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::AdvancedSwarf,
            label: "Advanced Swarf",
            caption: "Adv. Swarf",
            tag: 19,
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_advanced_swarf,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::RotaryContour,
            label: "Rotary Contour",
            caption: "Rotary Contour",
            tag: 20,
            // The rings are the finish's, posed radially (M5.1 stage
            // E): the ball's path decides the verdict, the fitted
            // axis and the lean answer only where the spindle points.
            // Rotary (M13): the envelope measures the posed ball; the
            // bands and rapids answer to the radial field, not to a
            // vertical column a lying tool never stands in.
            mode: Mode::Rotary,
            section: Section::MultiAxis,
            seed: seed_rotary_contour,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::RotaryParallel,
            label: "Rotary Parallel",
            caption: "Rotary Parallel",
            tag: 21,
            mode: Mode::Rotary,
            section: Section::MultiAxis,
            seed: seed_rotary_parallel,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::RotaryOffset,
            label: "Rotary Offset",
            caption: "Rotary Offset",
            tag: 22,
            // The rough ladder's verdict is the measured envelope too
            // (M5.1 stage E2): law (f) sweeps a sphere, so the pose
            // cannot lie to it on a bar lying or standing, and the
            // kind's tolerance is exactly that law's budget. Rotary
            // (M13): the wrap hops lap the bar radially - the radial
            // field judges them, the vertical column never could.
            mode: Mode::Rotary,
            section: Section::MultiAxis,
            seed: seed_rotary_offset,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::MultiAxisRough,
            label: "Multi-Axis Rough",
            caption: "Multi-Axis Rough",
            tag: 23,
            // The positions are the pocket ladder's, bit for bit, and
            // a sphere's envelope is rotation-invariant about its own
            // center: law (f) measures what the vertical rough would
            // have cut, so the pose cannot lie to the verdict (M5.1
            // stage E3). The tolerance is that law's budget, its only
            // reading - the ladder itself chords by the pocket's own
            // lattice laws.
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_multiaxis_rough,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::Flow5D,
            label: "5-Axis Flow",
            caption: "5-Axis Flow",
            tag: 24,
            // The parallel's verdict law on lines that turn (M5.1 I):
            // a sphere's envelope is rotation-invariant, so the field
            // decides where the lines GO and the lean law where the
            // spindle points, and the measured scan judges both.
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_flow5d,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::MultiAxisContour,
            label: "Multi-Axis Contour",
            caption: "Multi-Axis Contour",
            tag: 25,
            // The ball rides the picked chains wherever they stand
            // (M5.1 J): there is no authored pattern for a law to
            // distrust, and the verdict is the same sphere-envelope
            // scan - rest-height errors at a vertex fan and a gouged
            // chain are exactly what the sweep measures.
            mode: Mode::Envelope,
            section: Section::MultiAxis,
            seed: seed_multi_axis_contour,
            tool_kinds: tk::BALL,
        },
        KindInfo {
            ty: OpType::Facing,
            label: "Facing",
            caption: "Facing",
            tag: 26,
            // The facing verdict is measured like the finish's (law
            // f): the ball is the insert nose circle swept true - and
            // a meridian circle revolved IS a sphere about the axis,
            // so the scan reads the lathe's own cut wherever the fit
            // stands it. The nose kinds keep this law after stage C:
            // the revolve verdict is the BLADE's, where no sphere
            // stands - the band is the tool.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_facing,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::ProfileRough,
            label: "Profile Rough",
            caption: "Profile Rough",
            tag: 27,
            // The rungs' verdict is the same sphere-envelope law
            // (M5.2 B): a nose circle revolved IS a sphere about the
            // axis, so the scan measures each rung's shell exactly -
            // and the standing leave the verdict reports is what the
            // finish pass owns.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_profile_rough,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::ProfileFinish,
            label: "Profile Finish",
            caption: "Profile Finish",
            tag: 28,
            // The ride is the pattern: no authored spacing for a law
            // to distrust, and the tolerance is exactly the
            // envelope's budget - the chord law the kernel settled
            // the ride with is the same promise the scan checks.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_profile_finish,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::GrooveRough,
            label: "Groove Rough",
            caption: "Groove Rough",
            tag: 29,
            // No sphere can judge a band: a grooved part is undercut
            // BY DESIGN, and material standing beside the groove is
            // out of every ball's reach along Z. The verdict is the
            // meridian scan of `turning::revolve` - the boxcar min
            // of the cuts against the silhouette, exact on E's own
            // lattice (M5.2 C).
            mode: Mode::Revolve,
            section: Section::Turning,
            seed: seed_groove_rough,
            tool_kinds: tk::BLADE,
        },
        KindInfo {
            ty: OpType::GrooveFinish,
            label: "Groove Finish",
            caption: "Groove Finish",
            tag: 30,
            // The same meridian law, leave-free: one plunge per
            // center to the band's floor, and the floor no band
            // reached rests by the window shell - silence is what a
            // missed pass must never buy (M5.2 C).
            mode: Mode::Revolve,
            section: Section::Turning,
            seed: seed_groove_finish,
            tool_kinds: tk::BLADE,
        },
        KindInfo {
            ty: OpType::Part,
            label: "Parting",
            caption: "Part",
            tag: 31,
            // Separation as a band cut: the through-axis kerf is
            // judged silent inside its own kerf and right of it
            // (the workpiece left with the scrap), and the verdict
            // that reads it is the same meridian scan (M5.2 C).
            mode: Mode::Revolve,
            section: Section::Turning,
            seed: seed_part,
            tool_kinds: tk::BLADE,
        },
        KindInfo {
            ty: OpType::Threading,
            label: "Threading",
            caption: "Threading",
            tag: 32,
            // The blade verdict with the wave clause (M5.2 D): the
            // tool's V, not a boxcar, is the footprint - at every
            // center's station the floor stands deeper by the flank
            // slope's cotangent times the distance along the axis.
            // The lattice is the helix's own, so the scan judges
            // every `pitch` of station on the model's phase, and
            // the shell owns the crest above the thread.
            mode: Mode::Revolve,
            section: Section::Turning,
            seed: seed_threading,
            tool_kinds: tk::THREAD_INSERT,
        },
        KindInfo {
            ty: OpType::Chamfering,
            label: "Chamfering",
            caption: "Chamfer",
            tag: 33,
            // The promised-face rides (M5.2 E) judge like the
            // profile finish, and for the profile finish's reason:
            // the nose-ball sweeps the MODEL's own offset, so the
            // envelope scan measures the ride against the very
            // skin the door judged the authored line against.
            // Untouched columns outside the face's window are
            // another op's jurisdiction - the scan's law, not a
            // silence.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_chamfer,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::TaperTurning,
            label: "Taper Turning",
            caption: "Taper",
            tag: 34,
            // The same envelope law across the cone: the authored
            // diameters spent themselves at the door, and what the
            // ball rolls over is the modeled skin within the
            // chord budget - one truth, ride and verdict alike.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_taper,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::MultiPassTurning,
            label: "Multi-Pass Turning",
            caption: "Multi-Pass",
            tag: 35,
            // Rough and finish in one program: the rungs' shells
            // and the spring rides' midpoints all stand at a
            // nose's distance-or-more from the skin, so the same
            // envelope law convicts any rung too thin and forgives
            // the leave it is told to stand - the last ride lands
            // ON the profile either way.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_multi_pass,
            tool_kinds: tk::INSERT,
        },
        KindInfo {
            ty: OpType::FormTurning,
            label: "Form Turning",
            caption: "Form",
            tag: 36,
            // The crown floor of the meridian scan: the gauge's
            // arc, not the nose circle, says what stands between
            // the band's edges, and the crest law keeps the shell
            // for what the crown never reaches (M5.2 E2).
            mode: Mode::Revolve,
            section: Section::Turning,
            seed: seed_form,
            tool_kinds: tk::FORM,
        },
        KindInfo {
            ty: OpType::BoringTurning,
            label: "Boring",
            caption: "Bore",
            tag: 37,
            // The profile finish's inside twin (M5.2 F): the nose
            // stands ON the wall and the bar rides it, so the
            // envelope scan - rolled about the centers with the
            // tool's own ground nose - measures the ride against
            // the very skin the kernel offset. The hole-existence
            // doors are what make this safe: a plain bar's
            // min-envelope is its own skin, and only the axis
            // parity knows whether the nose stands in air.
            mode: Mode::Envelope,
            section: Section::Turning,
            seed: seed_bore,
            tool_kinds: tk::BAR,
        },
        KindInfo {
            ty: OpType::Probe,
            label: "Probe",
            caption: "Probe",
            tag: 38,
            // A probe cuts nothing: its own protected-move law judges
            // the stylus path (`operation::probe_op`), the cutting
            // laws stand down the way they do for the centre-line ride.
            mode: Mode::Engrave,
            section: Section::Drilling,
            seed: seed_probe,
            tool_kinds: tk::PROBE,
        },
        KindInfo {
            ty: OpType::PlasmaProfile,
            label: "Plasma Profile",
            caption: "Plasma Profile",
            tag: 39,
            // The torch cuts a kerf through the plate: the milling laws
            // (tangency, reach, depth) have no torch to judge, so they
            // stand down as for the centre-line ride; the plasma
            // generator certifies its own kerf law (no lit move's kerf
            // reaches the part) and #672 owns the plasma verdicts.
            mode: Mode::Engrave,
            section: Section::Plasma,
            seed: super::plasma_op::seed_plasma_profile,
            tool_kinds: tk::TORCH,
        },
        KindInfo {
            ty: OpType::TrochoidalSlot,
            label: "Trochoidal slot",
            caption: "Trochoid",
            tag: 40,
            // The slot's authored stadium, hollowed: area laws rule.
            mode: Mode::Area,
            section: Section::Mill2D,
            seed: seed_trochoidal_slot,
            tool_kinds: tk::FLAT_MILLS,
        },
    ];

    /// Whose machine this kind drives - read off the row's section
    /// (M6.2). The field it replaces was the pairing's stored half;
    /// with `section` the truth is one column, and this method is the
    /// door's and the sim's word for it.
    #[must_use]
    pub fn family(&self) -> Family {
        self.section.family()
    }

    /// Whether this kind may enter a document mounted on `machine`:
    /// the family pairing, plus the one per-kind exception the lathe
    /// owns (M13.5 A) - the Drill kind is admitted on a plain lathe,
    /// because a hole down the spindle axis is the turret's simplest
    /// hole and the mill's cycle vocabulary is its vocabulary. The
    /// generator's axial doors then refuse by name every hole that
    /// is not on the axis. The command door, the ribbon's hiding
    /// (`shown_for`, `Section::kinds_for`) and the retype menu all
    /// read this one word (M13.5 B) - never the family table alone.
    #[must_use]
    pub fn admits(&self, machine: crate::machine::Kind) -> bool {
        self.family().admits(machine)
            || (self.ty == OpType::Drill && machine == crate::machine::Kind::Lathe)
    }
}

/// The kinds, in product order (combo, ribbon, digest tags).
pub const KIND_COUNT: usize = KindInfo::ALL.len();

/// The panel labels, table order (was `crates/ui` panes::TYPES - now the
/// same list as [`OpType::Display`] reads).
pub const ALL_LABELS: [&str; KIND_COUNT] = [
    KindInfo::ALL[0].label,
    KindInfo::ALL[1].label,
    KindInfo::ALL[2].label,
    KindInfo::ALL[3].label,
    KindInfo::ALL[4].label,
    KindInfo::ALL[5].label,
    KindInfo::ALL[6].label,
    KindInfo::ALL[7].label,
    KindInfo::ALL[8].label,
    KindInfo::ALL[9].label,
    KindInfo::ALL[10].label,
    KindInfo::ALL[11].label,
    KindInfo::ALL[12].label,
    KindInfo::ALL[13].label,
    KindInfo::ALL[14].label,
    KindInfo::ALL[15].label,
    KindInfo::ALL[16].label,
    KindInfo::ALL[17].label,
    KindInfo::ALL[18].label,
    KindInfo::ALL[19].label,
    KindInfo::ALL[20].label,
    KindInfo::ALL[21].label,
    KindInfo::ALL[22].label,
    KindInfo::ALL[23].label,
    KindInfo::ALL[24].label,
    KindInfo::ALL[25].label,
    KindInfo::ALL[26].label,
    KindInfo::ALL[27].label,
    KindInfo::ALL[28].label,
    KindInfo::ALL[29].label,
    KindInfo::ALL[30].label,
    KindInfo::ALL[31].label,
    KindInfo::ALL[32].label,
    KindInfo::ALL[33].label,
    KindInfo::ALL[34].label,
    KindInfo::ALL[35].label,
    KindInfo::ALL[36].label,
    KindInfo::ALL[37].label,
    KindInfo::ALL[38].label,
    KindInfo::ALL[39].label,
    KindInfo::ALL[40].label,
];

impl OpType {
    /// The table index. Exhaustive: a new variant must join the registry
    /// (and a row in [`KindInfo::ALL`] in the same order) to compile.
    #[must_use]
    pub fn ordinal(self) -> usize {
        match self {
            OpType::Face => 0,
            OpType::Contour => 1,
            OpType::Pocket => 2,
            OpType::Slot => 3,
            OpType::Trace => 4,
            OpType::Adaptive2D => 5,
            OpType::Offset2D => 6,
            OpType::Engrave => 7,
            OpType::Chamfer2D => 8,
            OpType::Circular => 9,
            // M2.4 C: the thread mill closes the Mill2D run and the
            // boring ride joins Drilling behind the drill - the
            // partition's contiguity law and the family law (turning
            // stays the tail run) both say mid-run, not tail-append.
            OpType::Thread => 10,
            OpType::Drill => 11,
            OpType::Bore => 12,
            OpType::Pocket3D => 13,
            OpType::Finish3D => 14,
            // M2.5 E B: the deburr joins the Mill3D run where the
            // partition's contiguity law puts it - behind the finish,
            // before the multi-axis run opens.
            OpType::Deburr => 15,
            OpType::Contour5D => 16,
            OpType::Parallel5D => 17,
            OpType::Swarf => 18,
            OpType::AdvancedSwarf => 19,
            OpType::RotaryContour => 20,
            OpType::RotaryParallel => 21,
            OpType::RotaryOffset => 22,
            OpType::MultiAxisRough => 23,
            OpType::Flow5D => 24,
            OpType::MultiAxisContour => 25,
            OpType::Facing => 26,
            OpType::ProfileRough => 27,
            OpType::ProfileFinish => 28,
            OpType::GrooveRough => 29,
            OpType::GrooveFinish => 30,
            OpType::Part => 31,
            OpType::Threading => 32,
            OpType::Chamfering => 33,
            OpType::TaperTurning => 34,
            OpType::MultiPassTurning => 35,
            OpType::FormTurning => 36,
            OpType::BoringTurning => 37,
            // #108: the probe APPENDS - stored kind ids never move (pins,
            // digest tags and files stay valid). The menu still lists it
            // in the Drilling section: a section's kinds are read by
            // `Section::kinds` in table order, not as a contiguous run.
            OpType::Probe => 38,
            // #670: the plasma profile appends - the plasma table's
            // section, its own run at the tail.
            OpType::PlasmaProfile => 39,
            // #220: appended after the plasma profile (39) (stored ids never move).
            OpType::TrochoidalSlot => 40,
        }
    }

    /// The kind's registry row.
    #[must_use]
    pub fn info(self) -> &'static KindInfo {
        let row = &KindInfo::ALL[self.ordinal()];
        debug_assert_eq!(
            row.ty, self,
            "KindInfo::ALL is out of step with OpType::ordinal"
        );
        row
    }
}

/// M11.3: a NEW pocket-family op's helix radius from its tool - 0.4 x
/// the tool radius (a helix bore wide enough for the adaptive spiral and
/// for chips to clear, inside the roughing ladders' radius <= tool radius
/// law), snapped to 0.01 mm. Called where an op is created from a seed
/// with a known tool (the ribbon's quick-add); explicit params, saved
/// files and retypes keep their own number.
pub fn seed_helix_for_tool(params: &mut OpParams, tool_diameter: f64) {
    let r = tool_diameter * 0.5;
    if !(r.is_finite() && r > 0.0) {
        return;
    }
    let want = ((0.4 * r) * 100.0).round() / 100.0;
    let want = want.clamp(0.01, r);
    match params {
        OpParams::Pocket { helix_radius, .. }
        | OpParams::Adaptive2D { helix_radius, .. }
        | OpParams::Offset2D { helix_radius, .. }
        | OpParams::Pocket3D { helix_radius, .. } => *helix_radius = want,
        _ => {}
    }
}

/// The type-change draft rule (M1.5.2), one law for both callers: the
/// page's Type combo and the ribbon's Create cells (through
/// `actions::add_operation`). The carried numbers meet the kind's row,
/// and the row answers with a fresh parameter set. An out-of-range index
/// is a caller bug - the ribbon and combo only ever pass table
/// positions - so debug builds stop, and a release build seeds a Face
/// and says so rather than panicking on a UI path (Prime Directive).
#[must_use]
pub fn retype_params(cur: &OpParams, kind: usize) -> OpParams {
    let c = Carried::of(cur);
    let row = match KindInfo::ALL.get(kind) {
        Some(r) => r,
        None => {
            debug_assert!(false, "op kind index {kind} is past the table");
            tracing::error!("kinds: unknown op kind index {kind}, seeding a Face");
            &KindInfo::ALL[0]
        }
    };
    (row.seed)(&c)
}

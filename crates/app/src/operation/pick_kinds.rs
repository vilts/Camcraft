//! Which kinds take a user start / exit pick (#1448, #3324): one capability
//! per registry row, the one accessor that reads the picks of any kind, and
//! the one refusal of a pick that a kind does not take - in the kind's own
//! name. The design is `docs/decisions/1448-start-exit-shared.md`; each
//! family slice of #1448 changes its own arm here when its planner reads a
//! pick.
//!
//! Provenance: our own parameter model over the general machining practice
//! of a chosen entry and exit position; no vendor material.

use crate::path_picks::{PathPicks, PickRole};

use super::kinds::KindInfo;
use super::types::{OpParams, OpType};

/// What a pick means on a kind, and so how the viewport snaps it. Every
/// capability except [`Self::NotTaken`] takes a start pick and an exit pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickCapability {
    /// A point on a closed ring: the ring is re-sequenced to start there
    /// (the 2D contour). Snaps to a wall. Takes a start and an exit.
    /// The Sinker Burn's ring is its orbit (#3333): the pick is an angle
    /// round the burn position.
    Ring,
    /// A point inside the region: the descent place of the first pass
    /// and the end of the last pass. Snaps to a point in the region.
    Region,
    /// The first hole of the order and the last one. Snaps to a hole.
    HoleOrder,
    /// One end of an open chain or a slot, and the other end. Snaps to
    /// an end.
    Ends,
    /// The corner of the first raster row and the end of the last row.
    /// Snaps to a corner.
    Raster,
    /// The wire's threading point (`WireContourOp.picks`) and its exit
    /// (#3332): the end of an open run, or where a closed cut leaves.
    Threading,
    /// A point in the lathe (Z, X) plane (#3331): the first pass starts
    /// there and the last pass leaves there. Each pick states its Z.
    Meridian,
    /// The kind does not take a pick; the reason, said by name in the
    /// refusal and on the operation page.
    NotTaken(&'static str),
}

/// The reason of a cutting kind whose family slice of #1448 has not shipped.
pub const PICK_OWED: &str =
    "the planner chooses the entry and the exit of this kind until its slice of #1448 ships";

/// The reason of a drill's cross holes and of a lathe's axial hole (#3328).
pub const PICK_TURRET: &str = "a pick orders the Z holes of a mill, and the turret's holes keep the list order";

/// The reason of lathe Threading (#3331): its own law places the start.
pub const PICK_THREAD_LAW: &str = "the thread start law sets the start and the exit of a thread";

/// The reason of the probe: it has no path to start.
pub const PICK_NO_PATH: &str = "a probe touches the part and cuts no path";

impl PickCapability {
    /// Does this capability take a pick of `role`?
    #[must_use]
    pub fn takes(self, role: PickRole) -> bool {
        match self {
            Self::NotTaken(_) => false,
            Self::Threading | Self::Ring | Self::Region | Self::HoleOrder | Self::Ends | Self::Raster | Self::Meridian => {
                matches!(role, PickRole::Start | PickRole::Exit)
            }
        }
    }
}

impl KindInfo {
    /// The kind's pick capability. Exhaustive: a new kind fails to compile
    /// until it states one (the registry's own law).
    #[must_use]
    pub fn pick(&self) -> PickCapability {
        match self.ty {
            // #3330: the 5-axis contour's waterline rings are seated at
            // the start pick and run on to the exit pick.
            OpType::Contour | OpType::Contour5D => PickCapability::Ring,
            OpType::WireContour => PickCapability::Threading,
            // #3326: the corner of the first row, the side of the last.
            OpType::Face => PickCapability::Raster,
            // #3333: the orbit ring round the burn position, and the two
            // ends of the side move.
            OpType::SinkerBurn => PickCapability::Ring,
            OpType::SinkerSideBurn => PickCapability::Ends,
            OpType::Probe => PickCapability::NotTaken(PICK_NO_PATH),
            // #3328: the first hole and the last hole of the list.
            OpType::Thread | OpType::Drill | OpType::Bore => PickCapability::HoleOrder,
            // #3331 slice 1: the pick is a point in the lathe (Z, X) plane.
            OpType::Facing | OpType::ProfileRough | OpType::ProfileFinish => PickCapability::Meridian,
            OpType::Threading => PickCapability::NotTaken(PICK_THREAD_LAW),
            // #3325: the end where the pass starts, and where it stops.
            OpType::Slot => PickCapability::Ends,
            OpType::Pocket
            | OpType::Trace
            | OpType::Adaptive2D
            | OpType::Offset2D
            | OpType::Engrave
            | OpType::Chamfer2D
            | OpType::Circular
            | OpType::Pocket3D
            | OpType::Finish3D
            | OpType::Deburr
            | OpType::MultiAxisDeburr
            | OpType::Parallel5D
            | OpType::Swarf
            | OpType::AdvancedSwarf
            | OpType::RotaryContour
            | OpType::RotaryParallel
            | OpType::RotaryOffset
            | OpType::MultiAxisRough
            | OpType::Flow5D
            | OpType::MultiAxisContour
            | OpType::GrooveRough
            | OpType::GrooveFinish
            | OpType::Part
            | OpType::Chamfering
            | OpType::TaperTurning
            | OpType::MultiPassTurning
            | OpType::FormTurning
            | OpType::BoringTurning
            | OpType::PlasmaProfile
            // #1883: the waterjet's lead picks are owed, as the plasma's.
            | OpType::WaterjetProfile
            | OpType::TrochoidalSlot
            | OpType::SlotUndercut
            // #2826: the flow band's own start and exit ride its
            // picked chains' own ends, the planner's choice.
            | OpType::RotaryFlow
            | OpType::VCarve => PickCapability::NotTaken(PICK_OWED),
        }
    }
}

impl OpParams {
    fn picks_ref(&self) -> Option<&PathPicks> {
        match self {
            OpParams::Face { picks, .. }
            | OpParams::Contour { picks, .. }
            | OpParams::Pocket { picks, .. }
            | OpParams::Adaptive2D { picks, .. }
            | OpParams::Offset2D { picks, .. }
            | OpParams::Engrave { picks, .. }
            | OpParams::VCarve { picks, .. }
            | OpParams::Chamfer2D { picks, .. }
            | OpParams::Circular { picks, .. }
            | OpParams::Slot { picks, .. }
            | OpParams::TrochoidalSlot { picks, .. }
            | OpParams::Trace { picks, .. }
            | OpParams::Drill { picks, .. }
            | OpParams::Pocket3D { picks, .. }
            | OpParams::Finish3D { picks, .. }
            | OpParams::Deburr { picks, .. }
            | OpParams::MultiAxisDeburr { picks, .. }
            | OpParams::Contour5D { picks, .. }
            | OpParams::Parallel5D { picks, .. }
            | OpParams::Swarf { picks, .. }
            | OpParams::AdvancedSwarf { picks, .. }
            | OpParams::RotaryContour { picks, .. }
            | OpParams::RotaryParallel { picks, .. }
            | OpParams::RotaryOffset { picks, .. }
            | OpParams::MultiAxisRough { picks, .. }
            | OpParams::Flow5D { picks, .. }
            | OpParams::MultiAxisContour { picks, .. }
            | OpParams::Facing { picks, .. }
            | OpParams::ProfileRough { picks, .. }
            | OpParams::ProfileFinish { picks, .. }
            | OpParams::GrooveRough { picks, .. }
            | OpParams::GrooveFinish { picks, .. }
            | OpParams::Part { picks, .. }
            | OpParams::Threading { picks, .. }
            | OpParams::Chamfering { picks, .. }
            | OpParams::TaperTurning { picks, .. }
            | OpParams::MultiPassTurning { picks, .. }
            | OpParams::FormTurning { picks, .. }
            | OpParams::BoringTurning { picks, .. }
            | OpParams::Bore { picks, .. }
            | OpParams::Thread { picks, .. } => Some(picks),
            OpParams::PlasmaProfile(p) => Some(&p.picks),
            OpParams::WaterjetProfile(p) => Some(&p.picks),
            OpParams::WireContour(p) => Some(&p.picks),
            OpParams::SinkerBurn(p) => Some(&p.picks),
            OpParams::SinkerSideBurn(p) => Some(&p.burn.picks),
            OpParams::SlotUndercut(p) => Some(&p.picks),
            // #2826: the flow band's picks ride the op's own chain
            // selection, the planner reads them there.
            OpParams::RotaryFlow { .. } => None,
            OpParams::Probe { .. } => None,
        }
    }

    /// Mutable access to the operation-level picks for any path kind;
    /// `None` only for `Probe`.
    pub fn path_picks_mut(&mut self) -> Option<&mut PathPicks> {
        match self {
            OpParams::Face { picks, .. }
            | OpParams::Contour { picks, .. }
            | OpParams::Pocket { picks, .. }
            | OpParams::Adaptive2D { picks, .. }
            | OpParams::Offset2D { picks, .. }
            | OpParams::Engrave { picks, .. }
            | OpParams::VCarve { picks, .. }
            | OpParams::Chamfer2D { picks, .. }
            | OpParams::Circular { picks, .. }
            | OpParams::Slot { picks, .. }
            | OpParams::TrochoidalSlot { picks, .. }
            | OpParams::Trace { picks, .. }
            | OpParams::Drill { picks, .. }
            | OpParams::Pocket3D { picks, .. }
            | OpParams::Finish3D { picks, .. }
            | OpParams::Deburr { picks, .. }
            | OpParams::MultiAxisDeburr { picks, .. }
            | OpParams::Contour5D { picks, .. }
            | OpParams::Parallel5D { picks, .. }
            | OpParams::Swarf { picks, .. }
            | OpParams::AdvancedSwarf { picks, .. }
            | OpParams::RotaryContour { picks, .. }
            | OpParams::RotaryParallel { picks, .. }
            | OpParams::RotaryOffset { picks, .. }
            | OpParams::MultiAxisRough { picks, .. }
            | OpParams::Flow5D { picks, .. }
            | OpParams::MultiAxisContour { picks, .. }
            | OpParams::Facing { picks, .. }
            | OpParams::ProfileRough { picks, .. }
            | OpParams::ProfileFinish { picks, .. }
            | OpParams::GrooveRough { picks, .. }
            | OpParams::GrooveFinish { picks, .. }
            | OpParams::Part { picks, .. }
            | OpParams::Threading { picks, .. }
            | OpParams::Chamfering { picks, .. }
            | OpParams::TaperTurning { picks, .. }
            | OpParams::MultiPassTurning { picks, .. }
            | OpParams::FormTurning { picks, .. }
            | OpParams::BoringTurning { picks, .. }
            | OpParams::Bore { picks, .. }
            | OpParams::Thread { picks, .. } => Some(picks),
            OpParams::PlasmaProfile(p) => Some(&mut p.picks),
            OpParams::WaterjetProfile(p) => Some(&mut p.picks),
            OpParams::WireContour(p) => Some(&mut p.picks),
            OpParams::SinkerBurn(p) => Some(&mut p.picks),
            OpParams::SinkerSideBurn(p) => Some(&mut p.burn.picks),
            OpParams::SlotUndercut(p) => Some(&mut p.picks),
            // #2826: the flow band's picks ride the op's own chain
            // selection, the planner reads them there.
            OpParams::RotaryFlow { .. } => None,
            OpParams::Probe { .. } => None,
        }
    }

    /// The start and exit picks of this operation, for every kind, read from
    /// the operation-level `picks` field (#3339). `Probe` answers empty lists.
    #[must_use]
    pub fn path_picks(&self) -> PathPicks {
        self.picks_ref().cloned().unwrap_or_default()
    }

    /// The by-name refusal of a pick that this kind does not take (#1448):
    /// it names the kind, the pick and the reason. A tilted (3+2) contour
    /// keeps its own refusal. `None` when every pick is taken.
    pub(super) fn pick_refusal(&self) -> Option<String> {
        let picks = self.path_picks();
        if let Some(why) = picks.refusal() {
            return Some(why);
        }
        if picks.is_empty() {
            return None;
        }
        let info = self.kind().info();
        let cap = info.pick();
        for role in [PickRole::Start, PickRole::Exit] {
            if picks.of(role).is_empty() {
                continue;
            }
            let what = role.word();
            if matches!(self, OpParams::Contour { tilt: Some(_), .. }) {
                return Some(format!("a {what} point is not taken on a tilted (3+2) contour yet - clear it or the tilt"));
            }
            // #3326: the Face reads a pick in the setup's XY only.
            if matches!(self, OpParams::Face { tilt: Some(_), .. }) {
                return Some(format!("a {what} point is not taken on a tilted (3+2) face yet - clear it or the tilt"));
            }
            // #3325: a pick is a model-frame point; the tilted slot's ends are not.
            if matches!(self, OpParams::Slot { tilt: Some(_), .. }) {
                return Some(format!("a {what} point is not taken on a tilted (3+2) slot yet - clear it or the tilt"));
            }
            if !cap.takes(role) {
                let why = match cap {
                    PickCapability::NotTaken(why) => why,
                    _ => "this kind takes a start point only",
                };
                return Some(format!("{}: a {what} point is not taken - {why}; clear it on this operation", info.label));
            }
            // #3328: the turret's radial holes have no XY order to pick.
            if matches!(self, OpParams::Drill { cross, .. } if !cross.is_empty()) {
                return Some(format!("{}: a {what} point is not taken on cross holes - {PICK_TURRET}; clear it on this operation", info.label));
            }
            // #3331: a turning pick is a point of the lathe (Z, X) plane.
            if cap == PickCapability::Meridian && picks.of(role).iter().any(|p| p.z.is_none()) {
                return Some(format!(
                    "{}: the {what} point needs a Z - a turning point is a point in the lathe (Z, X) plane; state its Z or clear it",
                    info.label
                ));
            }
        }
        match self {
            OpParams::WireContour(w) => wire_exit_refusal(w),
            // #3333: a pick that the sinker's orbit or side move cannot hold.
            OpParams::SinkerBurn(p) => p.orbit_ends().err(),
            OpParams::SinkerSideBurn(p) => p.side_ends().err(),
            _ => self.hole_order().err(),
        }
    }

    /// The cut order of this operation's hole list under its picks
    /// (#3328), as indices into the list: `None` for a kind that has no
    /// hole order, or with no pick (the list order). The refusal names
    /// the kind: a pick that is on no hole, or both picks on one hole.
    fn hole_order(&self) -> Result<Option<Vec<usize>>, String> {
        let (OpParams::Drill { holes, picks, .. } | OpParams::Bore { holes, picks, .. } | OpParams::Thread { holes, picks, .. }) = self else {
            return Ok(None);
        };
        if picks.is_empty() {
            return Ok(None);
        }
        let circles: Vec<_> = holes.iter().map(|h| (h.xy, h.radius)).collect();
        crate::drilling::hole_order(&circles, picks).map(Some).map_err(|why| format!("{}: {why}", self.kind().info().label))
    }

    /// `items` (one for each hole of the list, in the list order) in the
    /// cut order that the picks give (#3328). No pick returns the list
    /// order unchanged. This is the generator's second check of the
    /// door's refusals, for a project file or a caller that skipped it.
    pub(super) fn holes_in_pick_order<T: Clone>(&self, items: &[T]) -> Result<Vec<T>, String> {
        Ok(match self.hole_order()? {
            Some(order) => crate::drilling::in_order(items, &order),
            None => items.to_vec(),
        })
    }
}

/// The wire's exit pick beside what it is not cut with (#3332): the exit
/// is read on a vertical, single-pass, untabbed cut of one profile or
/// one open run. Each other cut starts or ends its passes at the
/// threading point, so the pair is refused by name at the command door
/// and at generation, never cut as something else. `None` when the op
/// states no exit point or none of those.
#[must_use]
pub(super) fn wire_exit_refusal(w: &super::types::WireContourOp) -> Option<String> {
    w.picks.first(PickRole::Exit)?;
    let with = if w.four_axis.is_some() {
        "a 4-axis cut - its two profiles are paired from the start hole, and the wire returns there".to_owned()
    } else if w.burnout.is_some() {
        "a no-core burnout - its rings are cut from the start hole, and the wire returns there".to_owned()
    } else if w.tabs.is_some() {
        "tabs - each stretch between tabs and the tab cut-off start from the threading point with the wire threaded".to_owned()
    } else if !w.skims.is_empty() {
        format!("{} skim pass(es) - every pass starts and ends at the threading point", w.skims.len())
    } else if w.taper_deg != 0.0 {
        format!("a {:.3} degree taper - a leaning cut returns to the threading point", w.taper_deg)
    } else {
        return None;
    };
    Some(format!("Wire Contour: an exit point is stated with {with}; clear the exit point or that setting"))
}

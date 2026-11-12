//! The axes-in-the-plane pick draft (#557): while a Setups page row
//! collects picks for +Y or +X, a viewport Edge or Vertex click lands
//! here instead of in the open operation's geometry. Each axis takes one
//! straight edge (Edge mode) or two points (Vertex mode); a face sets no
//! direction and is refused by name in the status line. Ephemeral view
//! state like the lay-on-a-plane draft - nothing here writes the world;
//! Apply queues the undoable `AlignPartToSetup` through
//! `CommandKind::TurnPartInPlane`.
//!
//! Provenance: our own UI over plain fixture practice (indicate an edge
//! along the table's travel); no vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{self, AxisPick, InPlaneAxes, InPlaneAxis};
use camcraft_app::units::{self, Unit};
use flecs_ecs::prelude::World;

use crate::state::{GeoPick, State};

/// What one axis is picked by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AxisBy {
    /// One straight edge, start to end.
    #[default]
    Edge,
    /// Two points (feature corners), first to second.
    Points,
}

impl AxisBy {
    /// What a click takes in this mode, in plain words.
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            AxisBy::Edge => "a straight edge",
            AxisBy::Points => "two points",
        }
    }
}

/// One setup's in-plane axis picks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisDraft {
    /// The setup the picks will turn.
    pub setup: String,
    /// The axis viewport clicks land in; `None` is not collecting.
    pub picking: Option<InPlaneAxis>,
    pub y_by: AxisBy,
    pub x_by: AxisBy,
    /// The picks and flips, per axis (an empty pick list is "not given").
    pub axes: InPlaneAxes,
    /// These picks were just applied: they stay, so a Flip re-applies
    /// (the turn composes onto what the setup now holds).
    pub applied: bool,
}

impl AxisDraft {
    fn new(setup: &str) -> Self {
        AxisDraft {
            setup: setup.to_owned(),
            axes: InPlaneAxes { y: Some(AxisPick::default()), x: Some(AxisPick::default()), primary: InPlaneAxis::Y },
            ..AxisDraft::default()
        }
    }

    #[must_use]
    pub fn by(&self, axis: InPlaneAxis) -> AxisBy {
        match axis {
            InPlaneAxis::Y => self.y_by,
            InPlaneAxis::X => self.x_by,
        }
    }

    /// One axis's picks and flip (always present in a live draft).
    pub fn slot(&mut self, axis: InPlaneAxis) -> &mut AxisPick {
        let s = match axis {
            InPlaneAxis::Y => &mut self.axes.y,
            InPlaneAxis::X => &mut self.axes.x,
        };
        s.get_or_insert_with(AxisPick::default)
    }

    /// What Apply sends: an axis with no picks is not given.
    #[must_use]
    pub fn wanted(&self) -> InPlaneAxes {
        let given = |p: &Option<AxisPick>| p.clone().filter(|p| !p.picks.is_empty());
        InPlaneAxes { y: given(&self.axes.y), x: given(&self.axes.x), primary: self.axes.primary }
    }
}

/// The draft of `setup`, created when there is none (or it is another
/// setup's).
pub fn draft_for<'a>(state: &'a mut State, setup: &str) -> &'a mut AxisDraft {
    if state.axis_pick.as_ref().is_none_or(|d| d.setup != setup) {
        state.axis_pick = Some(AxisDraft::new(setup));
    }
    state.axis_pick.get_or_insert_with(|| AxisDraft::new(setup))
}

fn mode_of(by: AxisBy) -> GeoPick {
    match by {
        AxisBy::Edge => GeoPick::Edge,
        AxisBy::Points => GeoPick::Vertex,
    }
}

/// Start collecting picks for `axis` of `setup`: Edge mode for an edge,
/// Vertex mode for two points. The lay-on-a-plane draft stops.
pub fn arm(state: &mut State, setup: &str, axis: InPlaneAxis) {
    state.plane_pick = None;
    state.points_pick = None; // #555: one draft collects at a time
    crate::wcs_pick::disarm(state); // #701: one draft collects at a time
    let d = draft_for(state, setup);
    d.picking = Some(axis);
    let by = d.by(axis);
    state.geo_pick = mode_of(by);
    state.status = format!("{setup}: pick {} for {}", by.noun(), axis.label());
}

/// Stop collecting; the picks stay for Apply.
pub fn disarm(state: &mut State) {
    if let Some(d) = state.axis_pick.as_mut() {
        d.picking = None;
    }
}

/// Disarm every Setups page pick whose rows were not drawn this frame
/// (#664): a page change, an operation opened, the section closed - an
/// armed axis or plane pick must not feed a hidden draft. The shell
/// calls it once a frame after the dock, whatever the dock shows (#667),
/// and it consumes the frame's row marks; the picks themselves stay, so
/// the page re-opens disarmed with them shown. Ephemeral view state, no
/// command.
pub fn disarm_hidden(state: &mut State) {
    let shown = std::mem::take(&mut state.setup_pick_rows);
    // #557's stray-click hint holds while the rows are on screen; the
    // plane row and the axes rows draw together, so either mark counts.
    state.axis_rows_shown = !shown.is_empty();
    if let Some(d) = state.axis_pick.as_mut().filter(|d| !shown.contains(&d.setup)) {
        d.picking = None;
    }
    if state.plane_pick.as_ref().is_some_and(|d| !shown.contains(&d.setup)) {
        crate::plane_pick::pause(state);
    }
    // #555's three-points draft keeps the same law (merge with #664).
    if state.points_pick.as_ref().is_some_and(|d| !shown.contains(&d.setup)) {
        crate::points_pick::disarm(state);
    }
    if state.wcs_pick.as_ref().is_some_and(|d| !shown.contains(&d.setup)) {
        crate::wcs_pick::pause(state); // #701: same law for the role draft
    }
}

/// Disarm every Setups page pick at once (#667): the shell calls it
/// before the dock draws when the properties pane is closed or hidden
/// behind another tab, so no viewport click that frame - the viewport
/// draws inside the dock - can feed a draft whose rows are off screen.
pub fn disarm_all(state: &mut State) {
    state.setup_pick_rows.clear();
    disarm_hidden(state);
}

/// Is a Setups page row collecting axis picks?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.axis_pick.as_ref().is_some_and(|d| d.picking.is_some())
}

/// Choose what `axis` is picked by, and arm its pick at once: choosing
/// "by Edge" / "by Two points" means "I am about to pick". A changed
/// mode drops the axis's picks (an edge and two points do not mix);
/// re-choosing the current one keeps them.
pub fn set_by(state: &mut State, setup: &str, axis: InPlaneAxis, by: AxisBy) {
    let d = draft_for(state, setup);
    if d.by(axis) != by {
        match axis {
            InPlaneAxis::Y => d.y_by = by,
            InPlaneAxis::X => d.x_by = by,
        }
        d.slot(axis).picks.clear();
        d.applied = false;
    }
    arm(state, setup, axis);
}

/// A feature click while the axes rows are on screen but no axis is
/// armed (and no lay-on-a-plane pick collects) would fall through to
/// the operation pick - with no operation open on a Setups page. Say
/// what to do instead and swallow the click. `true` when handled.
pub fn hint_stray(state: &mut State) -> bool {
    if !state.axis_rows_shown || collecting(state) || crate::plane_pick::collecting(state) {
        return false;
    }
    let (y, x) = state.axis_pick.as_ref().map_or((AxisBy::Edge, AxisBy::Edge), |d| (d.y_by, d.x_by));
    let what = |b: AxisBy| match b {
        AxisBy::Edge => "edge",
        AxisBy::Points => "points",
    };
    state.status = format!(
        "no axis is being picked - click Pick +Y {} or Pick +X {} first",
        what(y),
        what(x)
    );
    true
}

/// One viewport pick into the axis being collected. An edge replaces
/// the axis's edge; a point joins, the last two kept. A pick of the
/// wrong kind is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(d) = state.axis_pick.as_mut() else { return };
    let Some(axis) = d.picking else { return };
    d.applied = false;
    let by = d.by(axis);
    let setup = d.setup.clone();
    let words = format!("{} of '{}'", pick.feature, pick.entity);
    let refuse = |what: &str| format!("{setup}: {words} is {what} - {} takes {}", axis.label(), by.noun());
    let status = match (by, pick.feature) {
        (AxisBy::Edge, FeatureSel::Chain(_)) => {
            d.slot(axis).picks = vec![pick];
            format!("{setup}: edge picked for {}", axis.label())
        }
        (AxisBy::Points, FeatureSel::Vertex(_)) => {
            let picks = &mut d.slot(axis).picks;
            picks.push(pick);
            let extra = picks.len().saturating_sub(2);
            picks.drain(..extra);
            format!("{setup}: {} point{} for {}", picks.len(), if picks.len() == 1 { "" } else { "s" }, axis.label())
        }
        (_, FeatureSel::Face(_) | FeatureSel::Patch(_)) => refuse("a face (a face sets no direction)"),
        (AxisBy::Edge, FeatureSel::Vertex(_)) => refuse("a point"),
        (AxisBy::Points, FeatureSel::Chain(_)) => refuse("an edge"),
    };
    state.status = status;
}

/// The picked row's words (#557 hand check): what was picked and where,
/// in the document's units - never an internal feature id. Points read
/// "1 of 2 points on 'block' (x, y, z) mm" / "2 points on 'block' (..)
/// -> (..) mm"; an edge reads "edge on 'block', 50.000 mm long, along
/// (dx, dy, dz)". Coordinates are the document frame (as modelled).
#[must_use]
pub fn picks_text(world: &World, picks: &[GeoSel], unit: Unit) -> String {
    let Some(first) = picks.first() else { return "nothing picked".to_owned() };
    let mut names: Vec<&str> = Vec::new();
    for g in picks {
        if !names.contains(&g.entity.as_str()) {
            names.push(&g.entity);
        }
    }
    let on = names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
    let at = |p: &[f64; 3]| {
        let c: Vec<String> = p.iter().map(|v| units::len(*v, unit, 3)).collect();
        format!("({})", c.join(", "))
    };
    let coords: Vec<Vec<[f64; 3]>> = picks.iter().filter_map(|g| pick::pick_coords(world, g)).collect();
    let suffix = unit.suffix();
    match first.feature {
        FeatureSel::Chain(_) => match coords.first().map(Vec::as_slice) {
            Some([a, b]) => {
                let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let dir = if l > 0.0 && l.is_finite() {
                    format!(", along ({:.3}, {:.3}, {:.3})", d[0] / l, d[1] / l, d[2] / l)
                } else {
                    String::new()
                };
                format!("edge on {on}, {} {suffix} long{dir}", units::len(l, unit, 3))
            }
            _ => format!("edge on {on} (no longer straight or in the model - re-pick it)"),
        },
        _ => {
            let pts: Vec<String> = coords.iter().filter_map(|c| c.first()).map(at).collect();
            let head = match picks.len() {
                1 => format!("1 of 2 points on {on}"),
                n => format!("{n} points on {on}"),
            };
            if pts.is_empty() { head } else { format!("{head} {} {suffix}", pts.join(" -> ")) }
        }
    }
}

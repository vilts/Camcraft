//! The Boundary group's draft law (M11.5 UX, the user's hand-test): ONE
//! list of boundary entries over `AreaControls`' `extra` + `open`, each
//! with a role (Include / Exclude / Open - side A / Open - side B), what
//! its picks spell, and the roles its shape cannot take, disabled with
//! the reason. Viewport picks land here while the group is collecting:
//! a plain click takes ONE edge (a run - one straight stretch or one
//! arc, never the whole tangent loop), Ctrl toggles a run in or out
//! (Ctrl on an edge of a picked loop breaks it open), Shift closes the
//! picks around their face loop, Alt narrows any of them to the single
//! mesh segment. Everything here edits the page draft (`OpUi::area`):
//! Apply commits it as one undoable `EditOperationParams::with_area`, so
//! the history stays the one source of truth. Egui-free on purpose - the
//! page draws it, the tests drive it.
//!
//! Provenance: our own UI law; no vendor material consulted.

use std::hash::{DefaultHasher, Hash, Hasher};

use camcraft_app::geo2d::Region;
use camcraft_app::model;
use camcraft_app::operation::{
    self, BoundaryMode, BoundaryShape, ChainSide, ExtraBoundary, GeoSel, OpenBoundary, PickShape,
};

use crate::state::{BoundaryPick, State};

/// A boundary entry's role, as the page's dropdown names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Include,
    Exclude,
    /// Open boundary, machining side A: the side holding more of the
    /// operation's own area (`ChainSide::Inside`).
    OpenA,
    /// Open boundary, machining side B: the other side (`Outside`).
    OpenB,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Include, Role::Exclude, Role::OpenA, Role::OpenB];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Role::Include => "Include",
            Role::Exclude => "Exclude",
            Role::OpenA => "Open - machine side A",
            Role::OpenB => "Open - machine side B",
        }
    }

    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, Role::OpenA | Role::OpenB)
    }
}

/// Where an entry lives in the draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryRef {
    Extra(usize),
    Open,
}

/// One row of the list, read off the draft.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub at: EntryRef,
    pub role: Role,
    /// The picks; empty for the stock outline / part silhouette.
    pub picks: Vec<GeoSel>,
    /// What the picks spell; `None` for stock / silhouette (closed).
    pub shape: Option<PickShape>,
    /// "3 edges, open chain", "stock outline", ...
    pub summary: String,
    /// What is wrong with the entry as it stands, if anything.
    pub issue: Option<String>,
}

/// Ephemeral view state of the group (never a command, never saved):
/// whether it is open this frame, and the caches its reads ride.
#[derive(Clone, Debug, Default)]
pub struct BoundaryView {
    /// The Boundary group is open on the page this frame.
    pub shown: bool,
    /// The machined-area preview of the draft (key, cut plane, region,
    /// refusal), rebuilt only when its key moves.
    pub preview: Option<Preview>,
    shapes: Vec<(u64, PickShape)>,
}

#[derive(Clone, Debug)]
pub struct Preview {
    key: u64,
    pub z: f64,
    pub region: Option<Region>,
    /// What the tool really sweeps (`operation::machined_footprint`):
    /// the region for most kinds, Face's laid bounding band for Face -
    /// the viewport fills this, so it never promises a shape the kind's
    /// law does not follow.
    pub footprint: Option<Region>,
    pub refusal: Option<String>,
    /// #2246: the triangles of the fill (`footprint`, else `region`),
    /// made with the preview, so a still frame never triangulates.
    pub fill: Option<crate::boundary_viz::Fill>,
}

fn key_of(doc: u64, what: &impl std::fmt::Debug) -> u64 {
    let mut h = DefaultHasher::new();
    doc.hash(&mut h);
    format!("{what:?}").hash(&mut h);
    h.finish()
}

fn setup_of(state: &State) -> Option<String> {
    model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
        .and_then(|o| o.setup)
}

/// What `picks` spell, from the cache when this document version has
/// read them already.
#[must_use]
pub fn shape_of(state: &State, picks: &[GeoSel]) -> PickShape {
    let key = key_of(state.doc_version, &picks);
    if let Some((_, s)) = state.boundary_view.shapes.iter().find(|(k, _)| *k == key) {
        return s.clone();
    }
    operation::boundary_pick_shape(&state.world, setup_of(state).as_deref(), picks, &state.op_ui.params)
}

/// Fill the shape cache for every entry of the draft (the page calls it
/// once a frame, so the viewport reads never re-walk the picks).
pub fn warm(state: &mut State) {
    let mut lists: Vec<Vec<GeoSel>> = state
        .op_ui
        .area
        .extra
        .iter()
        .filter_map(|b| match &b.shape {
            BoundaryShape::Picks(p) => Some(p.clone()),
            _ => None,
        })
        .collect();
    lists.extend(state.op_ui.area.open.iter().map(|o| o.picks.clone()));
    let mut fresh = Vec::with_capacity(lists.len());
    for p in lists {
        let key = key_of(state.doc_version, &p);
        let s = shape_of(state, &p);
        fresh.push((key, s));
    }
    state.boundary_view.shapes = fresh;
}

fn role_of_extra(b: &ExtraBoundary) -> Role {
    if b.mode == BoundaryMode::Include { Role::Include } else { Role::Exclude }
}

/// The list, extras in order, the open boundary last.
#[must_use]
pub fn entries(state: &State) -> Vec<Entry> {
    let a = &state.op_ui.area;
    let mut out = Vec::with_capacity(a.extra.len() + 1);
    for (i, b) in a.extra.iter().enumerate() {
        let role = role_of_extra(b);
        let (picks, shape, summary) = match &b.shape {
            BoundaryShape::Picks(p) => {
                let s = shape_of(state, p);
                let summary = s.summary();
                (p.clone(), Some(s), summary)
            }
            BoundaryShape::Stock => (Vec::new(), None, "stock outline, closed".to_owned()),
            BoundaryShape::Silhouette => (Vec::new(), None, "part silhouette, closed".to_owned()),
        };
        let issue = match &shape {
            Some(PickShape::Open { .. } | PickShape::Chains { .. }) => Some(format!(
                "an {} must enclose an area and these picks are an open chain - Shift-click an edge or 'Extend to loop' to close it, or 'Use as open boundary'",
                role.label()
            )),
            Some(PickShape::Invalid(why)) => Some(why.clone()),
            _ => None,
        };
        out.push(Entry { at: EntryRef::Extra(i), role, picks, shape, summary, issue });
    }
    if let Some(o) = &a.open {
        let s = shape_of(state, &o.picks);
        let issue = match &s {
            PickShape::Invalid(why) => Some(why.clone()),
            _ => None,
        };
        let role = if o.side == ChainSide::Inside { Role::OpenA } else { Role::OpenB };
        out.push(Entry { at: EntryRef::Open, role, picks: o.picks.clone(), summary: s.summary(), shape: Some(s), issue });
    }
    out
}

/// Why `entry` cannot take `role` (the dropdown greys it with this as
/// the tooltip), or `None` when it can.
#[must_use]
pub fn role_block(state: &State, entry: &Entry, role: Role) -> Option<String> {
    if role == entry.role {
        return None;
    }
    if role.is_open() {
        if entry.picks.is_empty() {
            return Some("the open boundary is drawn from picked edges - the stock outline and the silhouette are not picks".into());
        }
        if entry.at != EntryRef::Open && state.op_ui.area.open.is_some() {
            return Some("an operation has one open boundary - change or remove the current one first".into());
        }
        return None;
    }
    if entry.shape.as_ref().is_some_and(PickShape::is_open) {
        return Some(format!(
            "these picks are an open chain ({}) - an {} must enclose an area; close it first (Shift-click / Extend to loop)",
            entry.summary,
            role.label()
        ));
    }
    None
}

/// Give `at` a new role: Include / Exclude flip the mode, the open roles
/// move the picks into the open boundary (and back). Refused with the
/// same reason the dropdown shows.
pub fn set_role(state: &mut State, at: EntryRef, role: Role) -> Result<(), String> {
    let Some(entry) = entries(state).into_iter().find(|e| e.at == at) else {
        return Err("that boundary is gone".into());
    };
    if let Some(why) = role_block(state, &entry, role) {
        return Err(why);
    }
    let a = &mut state.op_ui.area;
    let side = if role == Role::OpenB { ChainSide::Outside } else { ChainSide::Inside };
    let mode = if role == Role::Exclude { BoundaryMode::Exclude } else { BoundaryMode::Include };
    match (at, role.is_open()) {
        (EntryRef::Extra(i), false) => {
            if let Some(b) = a.extra.get_mut(i) {
                b.mode = mode;
            }
        }
        (EntryRef::Extra(i), true) => {
            if i < a.extra.len() {
                a.extra.remove(i);
            }
            a.open = Some(OpenBoundary { picks: entry.picks, side });
            retarget_after_removal(state, i, Some(BoundaryPick::Open));
        }
        (EntryRef::Open, true) => {
            if let Some(o) = &mut a.open {
                o.side = side;
            }
        }
        (EntryRef::Open, false) => {
            a.open = None;
            a.extra.push(ExtraBoundary { mode, shape: BoundaryShape::Picks(entry.picks) });
            let i = a.extra.len() - 1;
            if state.boundary_pick == Some(BoundaryPick::Open) {
                state.boundary_pick = Some(BoundaryPick::Extra(i));
            }
        }
    }
    Ok(())
}

/// After extra `i` left the list: a pick target on it moves to `to`,
/// targets past it shift down one.
fn retarget_after_removal(state: &mut State, i: usize, to: Option<BoundaryPick>) {
    state.boundary_pick = match state.boundary_pick {
        Some(BoundaryPick::Extra(j)) if j == i => to,
        Some(BoundaryPick::Extra(j)) if j > i => Some(BoundaryPick::Extra(j - 1)),
        other => other,
    };
}

/// Drop an entry from the draft.
pub fn remove(state: &mut State, at: EntryRef) {
    match at {
        EntryRef::Extra(i) => {
            if i < state.op_ui.area.extra.len() {
                state.op_ui.area.extra.remove(i);
                retarget_after_removal(state, i, None);
            }
        }
        EntryRef::Open => {
            state.op_ui.area.open = None;
            if state.boundary_pick == Some(BoundaryPick::Open) {
                state.boundary_pick = None;
            }
        }
    }
}

/// The pick target that fills `at`.
#[must_use]
pub fn target_of(at: EntryRef) -> BoundaryPick {
    match at {
        EntryRef::Extra(i) => BoundaryPick::Extra(i),
        EntryRef::Open => BoundaryPick::Open,
    }
}

fn picks_mut(state: &mut State, at: EntryRef) -> Option<&mut Vec<GeoSel>> {
    let a = &mut state.op_ui.area;
    match at {
        EntryRef::Extra(i) => match a.extra.get_mut(i).map(|b| &mut b.shape) {
            Some(BoundaryShape::Picks(p)) => Some(p),
            _ => None,
        },
        EntryRef::Open => a.open.as_mut().map(|o| &mut o.picks),
    }
}

/// Close an entry's edge picks around their face loop.
pub fn extend(state: &mut State, at: EntryRef) -> Result<(), String> {
    let picks = picks_mut(state, at).map(|p| p.clone()).ok_or("that boundary has no picks")?;
    let closed = operation::extend_to_loop_in(&state.world, setup_of(state).as_deref(), &picks)?;
    if let Some(p) = picks_mut(state, at) {
        *p = closed;
    }
    Ok(())
}

/// How a viewport click reads while the group collects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickMods {
    /// Command (Cmd on macOS, Ctrl elsewhere): toggle the pick in or out of the entry.
    pub toggle: bool,
    /// Shift: add it and close the entry around its face loop.
    pub close: bool,
}

/// A Face/Edge pick while the group is collecting, into the target's
/// entry: plain replaces the entry's picks with this one, Ctrl toggles
/// it, Shift adds and closes. A new-entry target (Include / Exclude)
/// creates the entry and then fills it. A pure draft write - Apply
/// commits it; the status line says what the entry now spells.
pub fn take_pick(state: &mut State, target: BoundaryPick, pick: GeoSel, mods: PickMods) {
    let at = match target {
        BoundaryPick::Include | BoundaryPick::Exclude => {
            let mode = if target == BoundaryPick::Include { BoundaryMode::Include } else { BoundaryMode::Exclude };
            let a = &mut state.op_ui.area;
            a.extra.push(ExtraBoundary { mode, shape: BoundaryShape::Picks(Vec::new()) });
            let i = a.extra.len() - 1;
            state.boundary_pick = Some(BoundaryPick::Extra(i));
            EntryRef::Extra(i)
        }
        BoundaryPick::Extra(i) => EntryRef::Extra(i),
        BoundaryPick::Open => {
            if state.op_ui.area.open.is_none() {
                state.op_ui.area.open = Some(OpenBoundary { picks: Vec::new(), side: ChainSide::Inside });
            }
            EntryRef::Open
        }
    };
    let Some(current) = picks_mut(state, at).map(|p| p.clone()) else {
        state.status = "that boundary is the stock outline or the silhouette - it takes no picks".into();
        return;
    };
    let (next, verb) = if mods.close {
        let with: Vec<GeoSel> = if current.contains(&pick) { current } else { [current, vec![pick.clone()]].concat() };
        match operation::extend_to_loop(&state.world, &with) {
            Ok(c) => (c, "closed around its loop"),
            Err(_) => (with, "added (no face loop runs through these edges, so it stays open)"),
        }
    } else if mods.toggle {
        let (n, removed) = operation::toggle_run(&state.world, &current, &pick);
        (n, if removed { "removed" } else { "added" })
    } else {
        (vec![pick.clone()], "picked")
    };
    if next.is_empty() {
        remove(state, at);
        state.status = format!("{} of '{}' removed - the boundary had no picks left, so it is gone", pick.feature, pick.entity);
        return;
    }
    if let Some(p) = picks_mut(state, at) {
        *p = next;
    }
    let n = match at {
        EntryRef::Extra(i) => format!("boundary {}", i + 1),
        EntryRef::Open => "the open boundary".to_owned(),
    };
    let entry = entries(state).into_iter().find(|e| e.at == at);
    let what = entry.as_ref().map_or(String::new(), |e| e.summary.clone());
    let tail = match entry.and_then(|e| e.issue) {
        Some(issue) => format!(" - {issue}"),
        None => " - Apply commits it".to_owned(),
    };
    state.status = format!("{} of '{}' {verb}: {n} is {what}{tail}", pick.feature, pick.entity);
}

/// Rebuild the machined-area preview when the draft or the document
/// moved: the operation as it would be after Apply, through the same
/// region law generation runs (`operation::region_of`).
pub fn refresh_preview(state: &mut State) {
    let live = model::find_by_name(&state.world, &state.op_ui.name).and_then(|e| model::operation_of(&state.world, &e));
    let Some(mut op) = live else {
        state.boundary_view.preview = None;
        return;
    };
    op.params = state.op_ui.params.clone();
    op.area = state.op_ui.area.clone();
    let key = key_of(state.doc_version, &(&op.params, &op.area, &op.src));
    if state.boundary_view.preview.as_ref().is_some_and(|p| p.key == key) {
        return;
    }
    let z = operation::region_z(&state.world, op.setup.as_deref(), Some(&op.src), &op.params);
    let refusal = operation::area_pick_issue(&state.world, op.setup.as_deref(), &op.area, &op.params)
        .or_else(|| operation::area_refusal(&state.world, &op));
    let region = if refusal.is_some() { None } else { operation::region_of(&state.world, &op) };
    let footprint = if region.is_some() { operation::machined_footprint(&state.world, &op) } else { None };
    let refusal = refusal.or_else(|| {
        (region.is_none() && !op.area.is_default())
            .then(|| "the Boundary settings leave nothing to preview - Generate names the step".to_owned())
    });
    let fill = z.and(footprint.as_ref().or(region.as_ref())).and_then(crate::boundary_viz::fill_of);
    state.boundary_view.preview = z.map(|z| Preview { key, z, region, footprint, refusal, fill });
}

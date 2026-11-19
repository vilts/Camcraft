//! The locate-in-fixture pick draft (#561): while a Fixtures page row or
//! the Model page collects mates, a viewport Face or Vertex click lands
//! here instead of in the open operation's geometry - first the part's
//! face or point, then the fixture's face or point it sits on, one mate
//! per pair, up to three (floor, jaw, stop). Ephemeral view state like
//! the lay-on-a-plane draft (#330) - nothing here writes the world;
//! Apply queues the undoable `Locate` through `CommandKind::Locate`.
//!
//! Provenance: our own UI over 3-2-1 fixturing practice; no vendor
//! material consulted.

use camcraft_app::fixture_locate::{self, LocateBody, LocateOffset, LocateSpec, MAX_MATES, Mate, Mover};
use camcraft_app::geometry::units;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::MateDatum;

use crate::state::{GeoPick, State};

/// The mates being collected for one fixture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocateDraft {
    pub fixture: String,
    /// The part; empty until the first part pick names it (or the page
    /// that armed the draft did).
    pub part: String,
    /// The body the first pick of each mate is on (#2173): the stock by
    /// default when the fixture's setup has one - a vise holds the blank.
    pub body: LocateBody,
    pub mover: Mover,
    pub mates: Vec<Mate>,
    /// The part half of the mate being picked, waiting for its fixture half.
    pub half: Option<GeoSel>,
    pub offset: LocateOffset,
}

impl LocateDraft {
    /// The command's input as drafted.
    #[must_use]
    pub fn spec(&self) -> LocateSpec {
        LocateSpec {
            part: self.part.clone(),
            fixture: self.fixture.clone(),
            body: self.body,
            mates: self.mates.clone(),
            offset: self.offset,
        }
    }
}

/// Would the drafted locate be accepted? The solve the command runs,
/// plus "nothing would move" - the why greys Apply out.
pub fn check(state: &State, d: &LocateDraft) -> Result<(), String> {
    let plan = fixture_locate::plan(&state.world, &d.spec())?;
    if fixture_locate::is_still(plan.delta) { Err(STILL.to_owned()) } else { Ok(()) }
}

/// The part already sits where the mates and the offset put it.
pub const STILL: &str = "nothing would move";

/// The held body's name for this draft: the part, or the stock of the
/// fixture's setup (#2173). Empty when not known yet.
#[must_use]
pub fn held(state: &State, d: &LocateDraft) -> String {
    match d.body {
        LocateBody::Part => d.part.clone(),
        LocateBody::Stock => fixture_locate::stock_of(&state.world, &d.fixture).unwrap_or_default(),
    }
}

/// The held body in plain words: "part" or "stock".
#[must_use]
pub fn noun(d: &LocateDraft) -> &'static str {
    match d.body {
        LocateBody::Part => "part",
        LocateBody::Stock => "stock",
    }
}

/// Hold the other body: the mates picked so far are on the old one, so
/// they are dropped (#2173).
pub fn set_body(state: &mut State, body: LocateBody) {
    let Some(d) = state.locate_pick.as_mut() else { return };
    if d.body != body {
        d.body = body;
        d.mates.clear();
        d.half = None;
    }
}

/// Is this draft's refusal only "it is already there" (a located draft
/// waiting for an Offset or Turn edit)?
#[must_use]
pub fn is_located(d: &LocateDraft, check: &Result<(), String>) -> bool {
    d.half.is_none() && !d.mates.is_empty() && matches!(check, Err(w) if w == STILL)
}

/// A unit normal's name in the setup frame: `+Z (top)`, `-Y (front)`;
/// off the axes, its components.
fn facing(n: [f64; 3]) -> String {
    const AXES: [(usize, f64, &str); 6] =
        [(2, 1.0, "+Z (top)"), (2, -1.0, "-Z (bottom)"), (0, 1.0, "+X (right)"), (0, -1.0, "-X (left)"), (1, 1.0, "+Y (back)"), (1, -1.0, "-Y (front)")];
    AXES.iter()
        .find(|(k, sign, _)| (n[*k] - sign).abs() < 1e-3)
        .map_or_else(|| format!("({:.2}, {:.2}, {:.2})", n[0], n[1], n[2]), |(_, _, name)| (*name).to_owned())
}

/// One pick in plain words, read in the fixture's setup frame: a face by
/// its outward normal and its number on the body (`part face -Z (bottom),
/// #3`), a point by its coordinates. A pick that does not read as a mate
/// keeps its raw words.
#[must_use]
pub fn describe(state: &State, fixture: &str, g: &GeoSel) -> String {
    let name = camcraft_app::pick::geo_label(&state.world, g);
    match (fixture_locate::read_pick(&state.world, fixture, g), &g.feature) {
        (Ok(MateDatum::Plane { normal, .. }), FeatureSel::Face(_)) => {
            format!("{name} ({} face)", facing(normal))
        }
        (Ok(MateDatum::Point(p)), _) => {
            let u = state.units();
            let c = p.map(|v| units::len(v, u, 2));
            format!("{name} ({}, {}, {}) {}", c[0], c[1], c[2], u.suffix())
        }
        _ => name,
    }
}

/// The mate whose fixture normal the typed turn goes about: the first
/// face-to-face mate (1-based), `None` when the turn is about setup +Z.
#[must_use]
pub fn turn_mate(d: &LocateDraft) -> Option<usize> {
    d.mates
        .iter()
        .position(|m| matches!((&m.part.feature, &m.fixture.feature), (FeatureSel::Face(_), FeatureSel::Face(_))))
        .map(|i| i + 1)
}

/// The document's parts (models that are not fixtures), name order.
#[must_use]
pub fn parts(state: &State) -> Vec<String> {
    model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| camcraft_app::setup_align::is_part(&state.world, e))
        .map(|(n, _)| n)
        .collect()
}

/// Start collecting mates for `fixture` (and `part`, when the page knows
/// it - else the only part, else the first part pick). Face mode: a click
/// takes a feature point near the cursor, else the face (#692's one rule);
/// Vertex mode takes points only.
pub fn arm(state: &mut State, fixture: &str, part: Option<&str>) {
    let part = part.map(str::to_owned).or_else(|| match parts(state).as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    });
    // #2173: a vise holds the stock, so the stock is the default body
    // when the fixture's setup has one.
    let body = if fixture_locate::stock_of(&state.world, fixture).is_some() { LocateBody::Stock } else { LocateBody::Part };
    let d = LocateDraft { fixture: fixture.to_owned(), part: part.unwrap_or_default(), body, ..LocateDraft::default() };
    state.status = format!("{fixture}: pick a {} face or point, then the {fixture} face or point it sits on", noun(&d));
    state.locate_pick = Some(d);
    if !matches!(state.geo_pick, GeoPick::Face | GeoPick::Vertex) {
        state.geo_pick = GeoPick::Face;
    }
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.locate_pick = None;
}

/// After the dock: a draft whose form was not drawn this frame (its page
/// left, the active setup changed, the pane closed) is dropped, so a
/// located draft kept for Offset / Turn edits never captures picks with
/// no form on screen (review of #566). The shell resets
/// `locate_form_drawn` before the dock.
pub fn sweep(state: &mut State) {
    if state.locate_pick.is_some() && !state.locate_form_drawn {
        disarm(state);
    }
}

/// Drop the waiting half pick, else the last mate.
pub fn remove_last(state: &mut State) {
    let Some(d) = state.locate_pick.as_mut() else { return };
    if d.half.take().is_none() {
        d.mates.pop();
    }
}

/// One viewport pick into the draft: a part pick opens a mate, the next
/// fixture pick closes it. Every pick it cannot take is said, by name.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(d) = state.locate_pick.as_ref() else { return };
    let words = describe(state, &d.fixture, &pick);
    if matches!(pick.feature, FeatureSel::Chain(_) | FeatureSel::Patch(_)) {
        state.status = format!("{words} is not a mate - pick a flat face or a vertex");
        return;
    }
    if d.mates.len() >= MAX_MATES && d.half.is_none() {
        state.status = "three mates fully locate the part - Remove last to change one".into();
        return;
    }
    let is_part = model::find_by_name(&state.world, &pick.entity)
        .is_some_and(|e| camcraft_app::setup_align::is_part(&state.world, &e));
    let fixture = d.fixture.clone();
    let (body, noun) = (d.body, noun(d));
    let stock = held(state, d);
    if pick.entity == fixture {
        let Some(d) = state.locate_pick.as_mut() else { return };
        state.status = match d.half.take() {
            Some(part) => {
                d.mates.push(Mate { part, fixture: pick });
                let d = d.clone();
                format!("mate {} picked - {}", d.mates.len(), next_hint(&d, &check(state, &d)))
            }
            // A ray that meets the steel first cannot reach a part face
            // inside it: say how to get there.
            None => format!(
                "pick the {noun}'s face or point first, then {words} - hide the fixture (eye) to pick {noun} faces hidden in it"
            ),
        };
        return;
    }
    let Some(d) = state.locate_pick.as_mut() else { return };
    if body == LocateBody::Stock {
        state.status = if stock.is_empty() {
            "the fixture's setup has no stock - set the stock first, or choose to locate the part".to_owned()
        } else if pick.entity != stock {
            format!("'{}' is neither the stock '{stock}' nor the fixture '{fixture}' - pick a stock face or point", pick.entity)
        } else {
            d.half = Some(pick);
            format!("{words} - now pick the {fixture} face or point it sits on")
        };
        return;
    }
    state.status = if !is_part {
        format!("'{}' is neither the part nor the fixture '{fixture}'", pick.entity)
    } else if !d.part.is_empty() && d.part != pick.entity && !d.mates.is_empty() {
        format!("the mates so far are on '{}' - finish or clear them before picking '{}'", d.part, pick.entity)
    } else {
        d.part.clone_from(&pick.entity);
        d.half = Some(pick);
        format!("{words} - now pick the {fixture} face or point it sits on")
    };
}

/// What the draft wants next, in plain words. Apply is offered only when
/// `check` (the solve) accepts the mates; a refused draft says why.
#[must_use]
pub fn next_hint(d: &LocateDraft, check: &Result<(), String>) -> String {
    match (d.half.as_ref(), d.mates.len(), check) {
        (Some(_), _, _) => format!("pick the {} face or point it sits on", d.fixture),
        (None, 0, _) => format!("pick a {} face or point in the viewport (the floor mate first)", noun(d)),
        (None, _, Err(w)) if w == STILL => {
            "located - edit Offset or Turn to adjust (one more undo step), pick another mate, or Stop locating".into()
        }
        (None, _, Err(why)) => format!("not yet - {why}; Remove last and re-pick"),
        (None, n, Ok(())) if n >= MAX_MATES => format!("the {} is fully located - Apply", noun(d)),
        (None, 1, Ok(())) => {
            "Apply (one face mate sets height and tilt only), or pick the next mate (the fixed jaw) to place X / Y".into()
        }
        (None, _, Ok(())) => "Apply, or pick the last mate (the stop)".into(),
    }
}
